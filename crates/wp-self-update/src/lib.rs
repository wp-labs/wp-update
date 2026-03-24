mod fetch;
mod install;
mod lock;
mod manifest;
mod platform;
mod types;
mod versioning;

pub use manifest::updates_manifest_url;
use orion_error::{ToStructError, UvsFrom};
pub use types::{
    CheckReport, CheckRequest, GithubReleaseAssetInfo, GithubReleaseInfo, GithubRepo,
    ResolvedRelease, SourceConfig, SourceKind, UpdateChannel, UpdateProduct, UpdateReport,
    UpdateRequest, UpdateTarget, VersionRelation,
};
pub use versioning::{compare_versions_str, relation_message};

use fetch::load_release;
use install::{
    confirm_update, create_temp_update_dir, discover_extracted_bins, extract_artifact_archive,
    fetch_asset_bytes, find_extracted_bins, install_bins, is_gzip_artifact,
    is_probably_package_managed, resolve_install_dir, rollback_bins, run_health_check,
    stage_raw_binary, validate_download_url, verify_asset_sha256,
};
use lock::UpdateLock;
use std::path::PathBuf;
use wp_error::run_error::RunResult;

pub async fn check(request: CheckRequest) -> RunResult<CheckReport> {
    let channel = request.source.channel;
    let channel_name = source_channel_name(&request.source).to_string();
    let manifest_format = source_format_name(&request.source).to_string();
    let (release, source) = load_release(&request.source, channel).await?;
    versioning::validate_artifact_version_consistency(&release.version, &release.artifact)?;

    let relation = compare_versions_str(&request.current_version, &release.version)?;
    Ok(CheckReport {
        product: request.product,
        channel: channel_name,
        branch: request.branch,
        source,
        manifest_format,
        current_version: request.current_version,
        latest_version: release.version.clone(),
        update_available: relation == VersionRelation::UpdateAvailable,
        platform_key: release.target,
        artifact: release.artifact,
        sha256: release.sha256,
    })
}

pub async fn update(request: UpdateRequest) -> RunResult<UpdateReport> {
    let channel = request.source.channel;
    let channel_name = source_channel_name(&request.source).to_string();
    let (release, source) = load_release(&request.source, channel).await?;
    versioning::validate_artifact_version_consistency(&release.version, &release.artifact)?;
    validate_download_url(&release.artifact, &request.source)?;

    let relation = compare_versions_str(&request.current_version, &release.version)?;
    let install_dir = resolve_install_dir(request.install_dir.as_deref())?;
    let install_dir_display = install_dir.display().to_string();

    if relation != VersionRelation::UpdateAvailable && !request.force {
        return Ok(UpdateReport {
            product: request.product.clone(),
            channel: channel_name.clone(),
            source,
            current_version: request.current_version,
            latest_version: release.version,
            install_dir: install_dir_display,
            artifact: release.artifact,
            dry_run: request.dry_run,
            updated: false,
            status: relation_message(relation).to_string(),
        });
    }

    if is_probably_package_managed(&install_dir) && !request.force {
        return Err(wp_error::run_error::RunReason::from_conf()
            .to_err()
            .with_detail(format!(
                "refusing to replace binaries under {}; looks like a package-managed install, rerun with --force if this is intentional",
                install_dir.display()
            )));
    }

    if request.dry_run {
        return Ok(UpdateReport {
            product: request.product.clone(),
            channel: channel_name.clone(),
            source,
            current_version: request.current_version,
            latest_version: release.version,
            install_dir: install_dir_display,
            artifact: release.artifact,
            dry_run: true,
            updated: false,
            status: "dry-run".to_string(),
        });
    }

    if !request.yes
        && !confirm_update(
            &request.current_version,
            &release.version,
            &install_dir,
            &release.artifact,
        )?
    {
        return Ok(UpdateReport {
            product: request.product.clone(),
            channel: channel_name.clone(),
            source,
            current_version: request.current_version,
            latest_version: release.version,
            install_dir: install_dir_display,
            artifact: release.artifact,
            dry_run: false,
            updated: false,
            status: "aborted".to_string(),
        });
    }

    let _lock = UpdateLock::acquire(&install_dir)?;
    let asset_bytes = fetch_asset_bytes(&release.artifact).await?;
    verify_asset_sha256(&asset_bytes, &release.sha256)?;

    let extract_root = create_temp_update_dir()?;
    let install_result = async {
        let (extracted, selected_bins) =
            prepare_install_payload(&asset_bytes, &extract_root, &request.target)?;
        let backup_dir = install_bins(&install_dir, &extracted, &selected_bins)?;
        if let Err(err) = run_health_check(&install_dir, &release.version, &selected_bins) {
            rollback_bins(&install_dir, &backup_dir, &selected_bins)?;
            return Err(err);
        }
        Ok::<PathBuf, wp_error::RunError>(backup_dir)
    }
    .await;

    let _ = std::fs::remove_dir_all(&extract_root);
    let backup_dir = install_result?;

    Ok(UpdateReport {
        product: request.product,
        channel: channel_name,
        source,
        current_version: request.current_version,
        latest_version: release.version,
        install_dir: install_dir_display,
        artifact: release.artifact,
        dry_run: false,
        updated: true,
        status: format!("installed (backup: {})", backup_dir.display()),
    })
}

fn source_channel_name(source: &SourceConfig) -> String {
    match source.kind {
        SourceKind::Manifest { .. } => source.channel.as_str().to_string(),
        SourceKind::GithubLatest { .. } => "main".to_string(),
        SourceKind::GithubTag { ref tag, .. } => tag.clone(),
    }
}

fn source_format_name(source: &SourceConfig) -> &'static str {
    match source.kind {
        SourceKind::Manifest { .. } => "v2",
        SourceKind::GithubLatest { .. } | SourceKind::GithubTag { .. } => "github-release",
    }
}

fn prepare_install_payload(
    asset_bytes: &[u8],
    extract_root: &std::path::Path,
    target: &UpdateTarget,
) -> RunResult<(std::collections::HashMap<String, PathBuf>, Vec<String>)> {
    if is_gzip_artifact(asset_bytes) {
        extract_artifact_archive(asset_bytes, extract_root)?;
        return resolve_target_bins(extract_root, target);
    }

    let bins = resolve_raw_binary_bins(target)?;
    let extracted = stage_raw_binary(asset_bytes, extract_root, &bins[0])?;
    Ok((extracted, bins))
}

fn resolve_raw_binary_bins(target: &UpdateTarget) -> RunResult<Vec<String>> {
    match target {
        UpdateTarget::Product(product) => {
            let bins = product.owned_bins();
            if bins.len() == 1 {
                return Ok(bins);
            }
            Err(wp_error::run_error::RunReason::from_conf()
                .to_err()
                .with_detail("raw binary artifacts require exactly one target binary".to_string()))
        }
        UpdateTarget::Bins(bins) => {
            if bins.len() == 1 {
                return Ok(bins.clone());
            }
            Err(wp_error::run_error::RunReason::from_conf()
                .to_err()
                .with_detail("raw binary artifacts require exactly one target binary".to_string()))
        }
        UpdateTarget::Auto => {
            let current_exe = std::env::current_exe().map_err(|e| {
                wp_error::run_error::RunReason::from_conf()
                    .to_err()
                    .with_detail(format!("failed to resolve current executable path: {}", e))
            })?;
            let Some(name) = current_exe.file_name().and_then(|value| value.to_str()) else {
                return Err(wp_error::run_error::RunReason::from_conf()
                    .to_err()
                    .with_detail(format!(
                        "failed to resolve executable name from {}",
                        current_exe.display()
                    )));
            };
            Ok(vec![name.to_string()])
        }
    }
}

fn resolve_target_bins(
    extract_root: &std::path::Path,
    target: &UpdateTarget,
) -> RunResult<(std::collections::HashMap<String, PathBuf>, Vec<String>)> {
    match target {
        UpdateTarget::Product(product) => {
            let bins = product.owned_bins();
            let extracted = find_extracted_bins(extract_root, &bins)?;
            Ok((extracted, bins))
        }
        UpdateTarget::Bins(bins) => {
            let extracted = find_extracted_bins(extract_root, bins)?;
            Ok((extracted, bins.clone()))
        }
        UpdateTarget::Auto => {
            let extracted = discover_extracted_bins(extract_root)?;
            let mut bins: Vec<String> = extracted.keys().cloned().collect();
            bins.sort();
            Ok((extracted, bins))
        }
    }
}

#[doc(hidden)]
pub use fetch::load_github_release_info;
#[doc(hidden)]
pub use install::{
    extract_artifact_archive as extract_tar_gz_archive, fetch_asset_bytes as download_asset_bytes,
};
#[doc(hidden)]
pub use manifest::{parse_v2_release, updates_manifest_path};
#[doc(hidden)]
pub use versioning::validate_artifact_version_consistency;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::{
        create_temp_update_dir, extract_artifact_archive, find_extracted_bins, install_bins,
        rollback_bins, run_health_check,
    };
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;
    use tar::Builder;
    use tempfile::tempdir;

    fn build_artifact_tar_gz(version: &str, healthy: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let encoder = GzEncoder::new(&mut out, Compression::default());
        let mut builder = Builder::new(encoder);
        for bin in UpdateProduct::Suite.bins() {
            let body = if healthy || *bin != "wproj" {
                format!("#!/bin/sh\necho \"{} {}\"\n", bin, version)
            } else {
                "#!/bin/sh\nexit 1\n".to_string()
            };
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("artifacts/{}", bin), body.as_bytes())
                .expect("append tar entry");
        }
        let encoder = builder.into_inner().expect("finish tar builder");
        encoder.finish().expect("finish gzip");
        out
    }

    fn write_existing_bins(dir: &Path, version: &str) {
        for bin in UpdateProduct::Suite.bins() {
            let path = dir.join(bin);
            fs::write(&path, format!("#!/bin/sh\necho \"{} {}\"\n", bin, version))
                .expect("write existing bin");
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&path)
                    .expect("stat existing bin")
                    .permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&path, perms).expect("chmod existing bin");
            }
        }
    }

    fn apply_artifact(install_dir: &Path, artifact: &[u8], version: &str) -> RunResult<PathBuf> {
        let extract_root = create_temp_update_dir()?;
        let install_result = (|| {
            extract_artifact_archive(artifact, &extract_root)?;
            let bins = UpdateProduct::Suite.owned_bins();
            let extracted = find_extracted_bins(&extract_root, &bins)?;
            let backup_dir = install_bins(install_dir, &extracted, &bins)?;
            if let Err(err) = run_health_check(install_dir, version, &bins) {
                rollback_bins(install_dir, &backup_dir, &bins)?;
                return Err(err);
            }
            Ok(backup_dir)
        })();
        let _ = std::fs::remove_dir_all(&extract_root);
        install_result
    }

    fn build_raw_binary(version: &str, name: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho \"{} {}\"\n", name, version).into_bytes()
    }

    fn build_help_only_binary(name: &str) -> Vec<u8> {
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--help\" ] || [ \"$1\" = \"help\" ]; then\n  echo \"{} help\"\n  exit 0\nfi\nif [ \"$1\" = \"--version\" ] || [ \"$1\" = \"-V\" ] || [ \"$1\" = \"version\" ]; then\n  echo \"unknown command: $1\" 1>&2\n  exit 1\nfi\necho \"{} help\"\n",
            name, name
        )
        .into_bytes()
    }

    #[test]
    fn installs_release_artifact() {
        let artifact = build_artifact_tar_gz("0.30.0", true);
        let install_dir = tempdir().expect("install tempdir");
        write_existing_bins(install_dir.path(), "0.21.0");

        let backup_dir =
            apply_artifact(install_dir.path(), &artifact, "0.30.0").expect("install artifact");
        assert!(backup_dir.exists());

        let out = Command::new(install_dir.path().join("wproj"))
            .arg("--version")
            .output()
            .expect("run installed wproj");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("0.30.0"));
    }

    #[test]
    fn rolls_back_on_health_check_failure() {
        let artifact = build_artifact_tar_gz("0.30.0", false);
        let install_dir = tempdir().expect("install tempdir");
        write_existing_bins(install_dir.path(), "0.21.0");

        let err =
            apply_artifact(install_dir.path(), &artifact, "0.30.0").expect_err("expected failure");
        assert!(format!("{}", err).contains("health check failed"));

        let out = Command::new(install_dir.path().join("wproj"))
            .arg("--version")
            .output()
            .expect("run rolled back wproj");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("0.21.0"));
    }

    #[test]
    fn prepares_raw_binary_for_single_bin_targets() {
        let extract_root = tempdir().expect("extract tempdir");
        let artifact = build_raw_binary("0.30.0", "wproj");

        let (extracted, bins) = prepare_install_payload(
            &artifact,
            extract_root.path(),
            &UpdateTarget::Bins(vec!["wproj".to_string()]),
        )
        .expect("prepare raw binary");

        assert_eq!(bins, vec!["wproj".to_string()]);
        assert!(extracted.contains_key("wproj"));
    }

    #[test]
    fn rejects_raw_binary_for_multi_bin_targets() {
        let extract_root = tempdir().expect("extract tempdir");
        let artifact = build_raw_binary("0.30.0", "suite");

        let err = prepare_install_payload(
            &artifact,
            extract_root.path(),
            &UpdateTarget::Product(UpdateProduct::Suite),
        )
        .expect_err("expected rejection");

        assert!(format!("{}", err).contains("exactly one target binary"));
    }

    #[test]
    fn installs_raw_binary_that_only_supports_help_probe() {
        let artifact = build_help_only_binary("wpl-check");
        let install_dir = tempdir().expect("install tempdir");

        let extract_root = create_temp_update_dir().expect("extract root");
        let install_result = (|| {
            let (extracted, bins) = prepare_install_payload(
                &artifact,
                &extract_root,
                &UpdateTarget::Bins(vec!["wpl-check".to_string()]),
            )?;
            let backup_dir = install_bins(install_dir.path(), &extracted, &bins)?;
            if let Err(err) = run_health_check(install_dir.path(), "0.30.0", &bins) {
                rollback_bins(install_dir.path(), &backup_dir, &bins)?;
                return Err(err);
            }
            Ok::<PathBuf, wp_error::RunError>(backup_dir)
        })();
        let _ = std::fs::remove_dir_all(&extract_root);

        assert!(install_result.is_ok());
        assert!(install_dir.path().join("wpl-check").exists());
    }
}

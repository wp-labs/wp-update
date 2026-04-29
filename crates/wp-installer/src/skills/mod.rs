mod source;
mod target;

pub(crate) use source::SkillInstallArgs;

use crate::error::{skill_install_failed, InstallerReason, InstallerResult};
use orion_error::{ErrorWrapAs, IntoAs};
use serde::Serialize;
use source::{parse_skill_source, SkillSource};
use std::fs;
use std::path::{Path, PathBuf};
use target::{install_skill_into_target, resolve_default_target_dirs};
use tempfile::TempDir;
use wp_self_update::{download_asset_bytes, extract_tar_gz_archive, load_github_release_info};

#[derive(Debug, Serialize)]
pub(crate) struct SkillCheckReport {
    pub(crate) skill: String,
    pub(crate) repo: String,
    pub(crate) path: String,
    pub(crate) tag: String,
    pub(crate) archive: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillInstallLocationReport {
    pub(crate) platform: String,
    pub(crate) location: String,
    pub(crate) files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillInstallReport {
    pub(crate) skill: String,
    pub(crate) repo: String,
    pub(crate) path: String,
    pub(crate) tag: String,
    pub(crate) archive: String,
    pub(crate) locations: Vec<SkillInstallLocationReport>,
    pub(crate) status: String,
}

pub(crate) async fn check_skill(args: SkillInstallArgs) -> InstallerResult<SkillCheckReport> {
    let (source, selector) = parse_skill_source(&args)?;
    // Check validates that the requested path exists in the archive.
    let (release, archive, _archive_dir, archive_root) =
        download_repo_archive(&source, &selector).await?;
    let skill_src = archive_root.join(&source.subdir);
    if !skill_src.is_dir() {
        return Err(skill_install_failed(format!(
            "skill not found: {} (expected {})",
            source.skill_name,
            skill_src.display()
        )));
    }

    Ok(SkillCheckReport {
        skill: source.skill_name,
        repo: source.repo.url,
        path: source.subdir.display().to_string(),
        tag: release.tag_name,
        archive,
        status: "available".to_string(),
    })
}

pub(crate) async fn install_skill(args: SkillInstallArgs) -> InstallerResult<SkillInstallReport> {
    let (source, selector) = parse_skill_source(&args)?;
    let target_dirs = resolve_default_target_dirs()?;
    let (release, archive, _archive_dir, archive_root) =
        download_repo_archive(&source, &selector).await?;
    let skill_src = archive_root.join(&source.subdir);
    if !skill_src.is_dir() {
        return Err(skill_install_failed(format!(
            "skill not found: {} (expected {})",
            source.skill_name,
            skill_src.display()
        )));
    }

    let mut installs = Vec::new();
    for target_base in target_dirs {
        let installed = install_skill_into_target(&source.skill_name, &skill_src, &target_base)?;
        installs.push(SkillInstallLocationReport {
            platform: installed.platform,
            location: installed.location.display().to_string(),
            files: installed
                .files
                .into_iter()
                .map(|file| file.display().to_string())
                .collect(),
        });
    }

    Ok(SkillInstallReport {
        skill: source.skill_name,
        repo: source.repo.url,
        path: source.subdir.display().to_string(),
        tag: release.tag_name,
        archive,
        locations: installs,
        status: "installed".to_string(),
    })
}

async fn download_repo_archive(
    source: &SkillSource,
    selector: &source::SkillReleaseSelector,
) -> InstallerResult<(wp_self_update::GithubReleaseInfo, String, TempDir, PathBuf)> {
    let release = load_github_release_info(
        &source.repo,
        match selector {
            source::SkillReleaseSelector::Latest => None,
            source::SkillReleaseSelector::Tag(tag) => Some(tag.as_str()),
        },
    )
    .await
    .wrap_as(
        InstallerReason::SelfUpdateFailed,
        "failed to load GitHub release metadata for skill archive",
    )?;
    let expected_asset_name = format!("{}-{}.tar.gz", source.repo.name, release.tag_name);
    let asset_url = release
        .assets
        .iter()
        .find(|asset| asset.name == expected_asset_name)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| {
            skill_install_failed(format_missing_asset_error(
                source,
                &expected_asset_name,
                &release,
            ))
        })?;

    let bytes = download_asset_bytes(&asset_url).await.wrap_as(
        InstallerReason::SelfUpdateFailed,
        "failed to download skill archive",
    )?;
    let temp_dir = TempDir::new().into_as(
        InstallerReason::SkillInstallFailed,
        "failed to create temp skill dir",
    )?;
    extract_tar_gz_archive(&bytes, temp_dir.path()).wrap_as(
        InstallerReason::SelfUpdateFailed,
        "failed to extract downloaded skill archive",
    )?;
    let archive_root = locate_archive_root(temp_dir.path())?;
    Ok((release, asset_url, temp_dir, archive_root))
}

fn format_missing_asset_error(
    source: &SkillSource,
    expected_name: &str,
    release: &wp_self_update::GithubReleaseInfo,
) -> String {
    let mut names = release
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    format!(
        "GitHub release missing skill archive '{}' for {} (available: {})",
        expected_name,
        source.repo.url,
        names.join(", ")
    )
}

fn locate_archive_root(dest: &Path) -> InstallerResult<PathBuf> {
    let entries = fs::read_dir(dest)
        .into_as(
            InstallerReason::SkillInstallFailed,
            format!("failed to read extracted archive root {}", dest.display()),
        )?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<PathBuf>>();

    if entries.is_empty() {
        return Err(skill_install_failed("downloaded skill archive was empty"));
    }
    if entries.len() == 1 && entries[0].is_dir() {
        return Ok(entries[0].clone());
    }
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wp_self_update::{GithubReleaseAssetInfo, GithubReleaseInfo, GithubRepo};

    #[test]
    fn missing_asset_error_lists_available_names() {
        let source = SkillSource {
            skill_name: "warpparse-log-engineering".to_string(),
            repo: GithubRepo::parse("wp-labs/wp-skills").unwrap(),
            subdir: PathBuf::from("skills/warpparse-log-engineering"),
        };
        let err = format_missing_asset_error(
            &source,
            "wp-skills-v0.1.2.tar.gz",
            &GithubReleaseInfo {
                tag_name: "v0.1.2".to_string(),
                assets: vec![GithubReleaseAssetInfo {
                    name: "wp-skills-v0.1.2.zip".to_string(),
                    browser_download_url: "https://example.com/a.zip".to_string(),
                }],
            },
        );

        assert!(err.contains("wp-skills-v0.1.2.tar.gz"));
        assert!(err.contains("wp-skills-v0.1.2.zip"));
    }

    #[test]
    fn locate_archive_root_uses_extract_root_for_flat_repo_archives() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".claude")).unwrap();
        fs::create_dir_all(temp.path().join("skills")).unwrap();

        let root = locate_archive_root(temp.path()).unwrap();
        assert_eq!(root, temp.path());
    }
}

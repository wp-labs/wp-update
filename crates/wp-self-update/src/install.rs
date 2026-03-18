use crate::fetch::is_retryable_status;
use crate::{SourceConfig, SourceKind};
use flate2::read::GzDecoder;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use orion_error::{ToStructError, UvsFrom};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Cursor, IsTerminal, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tar::Archive;
use uuid::Uuid;
use wp_error::run_error::{RunReason, RunResult};

const FETCH_ASSET_CONNECT_TIMEOUT_SECS: u64 = 5;
const FETCH_ASSET_REQUEST_TIMEOUT_SECS: u64 = 120;
const FETCH_ASSET_MAX_ATTEMPTS: usize = 3;

pub(crate) fn resolve_install_dir(raw: Option<&Path>) -> RunResult<PathBuf> {
    let base = if let Some(raw) = raw {
        raw.to_path_buf()
    } else {
        let exe = std::env::current_exe().map_err(|e| {
            RunReason::from_conf()
                .to_err()
                .with_detail(format!("failed to resolve current executable path: {}", e))
        })?;
        exe.parent().map(Path::to_path_buf).ok_or_else(|| {
            RunReason::from_conf().to_err().with_detail(format!(
                "failed to resolve install dir from {}",
                exe.display()
            ))
        })?
    };
    let canonical = base.canonicalize().map_err(|e| {
        RunReason::from_conf().to_err().with_detail(format!(
            "failed to access install dir {}: {}",
            base.display(),
            e
        ))
    })?;
    if !canonical.is_dir() {
        return Err(RunReason::from_conf().to_err().with_detail(format!(
            "install dir is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

pub(crate) fn is_probably_package_managed(install_dir: &Path) -> bool {
    let path = install_dir.to_string_lossy();
    path.contains("/Cellar/")
        || path.contains("/Homebrew/")
        || path.starts_with("/usr/bin")
        || path.starts_with("/usr/local/bin")
        || path.starts_with("/opt/homebrew/bin")
}

pub(crate) fn confirm_update(
    current: &str,
    latest: &str,
    install_dir: &Path,
    artifact: &str,
) -> RunResult<bool> {
    println!("Self-update plan");
    println!("  Current  : {}", current);
    println!("  Latest   : {}", latest);
    println!("  Install  : {}", install_dir.display());
    println!("  Artifact : {}", artifact);
    print!("Proceed with installation? [y/N]: ");
    io::stdout()
        .flush()
        .map_err(|e| RunReason::from_conf().to_err().with_detail(e.to_string()))?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| RunReason::from_conf().to_err().with_detail(e.to_string()))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

pub(crate) fn validate_download_url(raw: &str, source: &SourceConfig) -> RunResult<()> {
    let parsed = reqwest::Url::parse(raw).map_err(|e| {
        RunReason::from_conf()
            .to_err()
            .with_detail(format!("invalid artifact url '{}': {}", raw, e))
    })?;
    let host = parsed.host_str().unwrap_or_default();
    match parsed.scheme() {
        "https" => {
            if is_allowed_artifact_host(host, source) {
                return Ok(());
            }
            Err(RunReason::from_conf().to_err().with_detail(format!(
                "artifact host '{}' is not in the allowed release domain set",
                host
            )))
        }
        "http" => {
            if matches!(host, "127.0.0.1" | "localhost") {
                return Ok(());
            }
            Err(RunReason::from_conf().to_err().with_detail(format!(
                "insecure artifact url '{}' is not allowed; use https or loopback http for local testing",
                raw
            )))
        }
        other => Err(RunReason::from_conf().to_err().with_detail(format!(
            "unsupported artifact url scheme '{}': {}",
            other, raw
        ))),
    }
}

fn is_allowed_artifact_host(host: &str, source: &SourceConfig) -> bool {
    if matches!(
        host,
        "github.com"
            | "objects.githubusercontent.com"
            | "release-assets.githubusercontent.com"
            | "github-releases.githubusercontent.com"
            | "raw.githubusercontent.com"
            | "127.0.0.1"
            | "localhost"
    ) {
        return true;
    }

    match &source.kind {
        SourceKind::Manifest {
            updates_base_url, ..
        } => {
            if let Ok(url) = reqwest::Url::parse(updates_base_url) {
                if url.host_str() == Some(host) {
                    return true;
                }
            }
        }
        SourceKind::GithubLatest { repo } => {
            if let Ok(url) = reqwest::Url::parse(&repo.url) {
                if url.host_str() == Some(host) {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) async fn fetch_asset_bytes(url: &str) -> RunResult<Vec<u8>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(FETCH_ASSET_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(FETCH_ASSET_REQUEST_TIMEOUT_SECS))
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .build()
        .map_err(|e| {
            RunReason::from_conf()
                .to_err()
                .with_detail(format!("failed to build HTTP client: {}", e))
        })?;

    let mut last_error: Option<String> = None;
    for attempt in 1..=FETCH_ASSET_MAX_ATTEMPTS {
        match client.get(url).send().await {
            Ok(rsp) => {
                let status = rsp.status();
                if status.is_success() {
                    let bytes = match fetch_asset_bytes_from_response(url, rsp).await {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            return fetch_asset_bytes_via_curl(url).map_err(|curl_err| {
                                RunReason::from_conf().to_err().with_detail(format!(
                                    "failed to read artifact response {}: {}; curl fallback failed: {}",
                                    url, err, curl_err
                                ))
                            });
                        }
                    };
                    return Ok(bytes.to_vec());
                }
                if is_retryable_status(status) && attempt < FETCH_ASSET_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    continue;
                }
                return Err(RunReason::from_conf()
                    .to_err()
                    .with_detail(format!("artifact request failed {}: HTTP {}", url, status)));
            }
            Err(e) => {
                last_error = Some(e.to_string());
                if attempt < FETCH_ASSET_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    continue;
                }
            }
        }
    }
    Err(RunReason::from_conf().to_err().with_detail(format!(
        "failed to fetch artifact {} after {} attempts: {}",
        url,
        FETCH_ASSET_MAX_ATTEMPTS,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
}

async fn fetch_asset_bytes_from_response(
    url: &str,
    mut rsp: reqwest::Response,
) -> Result<Vec<u8>, reqwest::Error> {
    let mut progress = new_download_progress(url, rsp.content_length());
    let mut bytes = Vec::new();
    while let Some(chunk) = rsp.chunk().await? {
        progress.inc(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
    }
    progress.finish();
    Ok(bytes)
}

enum DownloadProgress {
    Tty(ProgressBar),
    Text(TextDownloadProgress),
}

impl DownloadProgress {
    fn inc(&mut self, amount: u64) {
        match self {
            Self::Tty(progress) => progress.inc(amount),
            Self::Text(progress) => progress.inc(amount),
        }
    }

    fn finish(&mut self) {
        match self {
            Self::Tty(progress) => {
                let label = progress.message().to_string();
                progress.finish_with_message(format!("downloaded {}", label));
            }
            Self::Text(progress) => progress.finish(),
        }
    }
}

struct TextDownloadProgress {
    label: String,
    total: Option<u64>,
    downloaded: u64,
    last_percent: u64,
    last_log_at: Instant,
    started_at: Instant,
}

impl TextDownloadProgress {
    fn new(label: String, total: Option<u64>) -> Self {
        match total {
            Some(total) if total > 0 => {
                eprintln!("Downloading {} ({})...", label, HumanBytes(total));
            }
            _ => eprintln!("Downloading {}...", label),
        }

        let now = Instant::now();
        Self {
            label,
            total,
            downloaded: 0,
            last_percent: 0,
            last_log_at: now,
            started_at: now,
        }
    }

    fn inc(&mut self, amount: u64) {
        self.downloaded = self.downloaded.saturating_add(amount);
        let now = Instant::now();

        if let Some(total) = self.total.filter(|total| *total > 0) {
            let percent = ((self.downloaded.saturating_mul(100)) / total).min(100);
            let percent_advanced = percent >= self.last_percent.saturating_add(10);
            let timed_out = now.duration_since(self.last_log_at) >= Duration::from_secs(2);
            if percent_advanced || timed_out {
                self.last_percent = percent;
                self.last_log_at = now;
                eprintln!(
                    "Downloaded {}: {}/{} ({}%)",
                    self.label,
                    HumanBytes(self.downloaded),
                    HumanBytes(total),
                    percent
                );
            }
            return;
        }

        let grew_enough = self.downloaded.saturating_sub(self.last_percent) >= 5 * 1024 * 1024;
        let timed_out = now.duration_since(self.last_log_at) >= Duration::from_secs(2);
        if grew_enough || timed_out {
            self.last_percent = self.downloaded;
            self.last_log_at = now;
            eprintln!("Downloaded {}: {}", self.label, HumanBytes(self.downloaded));
        }
    }

    fn finish(&self) {
        eprintln!(
            "Downloaded {}: {} in {:.1}s",
            self.label,
            HumanBytes(self.downloaded),
            self.started_at.elapsed().as_secs_f32()
        );
    }
}

fn new_download_progress(url: &str, total: Option<u64>) -> DownloadProgress {
    let label = download_label(url);
    if !io::stderr().is_terminal() {
        return DownloadProgress::Text(TextDownloadProgress::new(label, total));
    }

    let progress = match total {
        Some(total) if total > 0 => ProgressBar::new(total),
        _ => ProgressBar::new_spinner(),
    };
    if let Ok(style) = ProgressStyle::with_template(
        "{spinner:.green} downloading {msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    ) {
        progress.set_style(style.progress_chars("#>-"));
    }
    if total.is_none() {
        progress.enable_steady_tick(Duration::from_millis(120));
    }
    progress.set_message(label);
    DownloadProgress::Tty(progress)
}

fn download_label(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "artifact".to_string())
}

fn fetch_asset_bytes_via_curl(url: &str) -> Result<Vec<u8>, String> {
    let temp = std::env::temp_dir().join(format!("wp-inst-download-{}", Uuid::new_v4()));
    let label = download_label(url);
    eprintln!("Retrying {} via curl...", label);

    let mut cmd = Command::new("curl");
    cmd.arg("-LfsS");
    if io::stderr().is_terminal() {
        cmd.arg("--progress-bar");
    } else {
        cmd.arg("--no-progress-meter");
    }
    let status = cmd
        .arg("-o")
        .arg(&temp)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to start curl: {}", e))?;
    if status.success() {
        let bytes = fs::read(&temp).map_err(|e| format!("failed to read curl output: {}", e))?;
        let _ = fs::remove_file(&temp);
        eprintln!("Downloaded {}: {}", label, HumanBytes(bytes.len() as u64));
        return Ok(bytes);
    }

    let _ = fs::remove_file(&temp);
    Err(format!("curl exited with status {}", status))
}

pub(crate) fn verify_asset_sha256(bytes: &[u8], expected_hex: &str) -> RunResult<()> {
    use sha2::{Digest, Sha256};
    let actual_hex = hex::encode(Sha256::digest(bytes));
    if actual_hex == expected_hex {
        return Ok(());
    }
    Err(RunReason::from_conf().to_err().with_detail(format!(
        "artifact sha256 mismatch: expected {}, got {}",
        expected_hex, actual_hex
    )))
}

pub(crate) fn create_temp_update_dir() -> RunResult<PathBuf> {
    let dir = std::env::temp_dir().join(format!("wproj-self-update-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|e| {
        RunReason::from_conf().to_err().with_detail(format!(
            "failed to create temp update dir {}: {}",
            dir.display(),
            e
        ))
    })?;
    Ok(dir)
}

pub(crate) fn extract_artifact_archive(bytes: &[u8], extract_root: &Path) -> RunResult<()> {
    let cursor = Cursor::new(bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);
    archive.unpack(extract_root).map_err(|e| {
        RunReason::from_conf().to_err().with_detail(format!(
            "failed to extract artifact into {}: {}",
            extract_root.display(),
            e
        ))
    })
}

pub(crate) fn is_gzip_artifact(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

pub(crate) fn stage_raw_binary(
    bytes: &[u8],
    extract_root: &Path,
    bin_name: &str,
) -> RunResult<HashMap<String, PathBuf>> {
    let path = extract_root.join(bin_name);
    fs::write(&path, bytes).map_err(|e| {
        RunReason::from_conf().to_err().with_detail(format!(
            "failed to stage raw binary into {}: {}",
            path.display(),
            e
        ))
    })?;
    set_exec_permission(&path)?;

    let mut found = HashMap::new();
    found.insert(bin_name.to_string(), path);
    Ok(found)
}

pub(crate) fn find_extracted_bins(
    extract_root: &Path,
    required_bins: &[String],
) -> RunResult<HashMap<String, PathBuf>> {
    let mut found = HashMap::new();
    for entry in walkdir::WalkDir::new(extract_root) {
        let entry = entry.map_err(|e| {
            RunReason::from_conf()
                .to_err()
                .with_detail(format!("failed to walk extracted artifact: {}", e))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };
        if required_bins.iter().any(|candidate| candidate == name) {
            found.insert(name.to_string(), entry.path().to_path_buf());
        }
    }

    let missing: Vec<&str> = required_bins
        .iter()
        .map(String::as_str)
        .filter(|name| !found.contains_key(*name))
        .collect();
    if !missing.is_empty() {
        return Err(RunReason::from_conf().to_err().with_detail(format!(
            "artifact missing required binaries: {}",
            missing.join(", ")
        )));
    }
    Ok(found)
}

pub(crate) fn discover_extracted_bins(extract_root: &Path) -> RunResult<HashMap<String, PathBuf>> {
    let mut artifact_files = HashMap::new();
    let mut all_files = HashMap::new();

    for entry in walkdir::WalkDir::new(extract_root) {
        let entry = entry.map_err(|e| {
            RunReason::from_conf()
                .to_err()
                .with_detail(format!("failed to walk extracted artifact: {}", e))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };

        all_files
            .entry(name.to_string())
            .or_insert_with(|| entry.path().to_path_buf());

        let is_artifact_file = entry
            .path()
            .strip_prefix(extract_root)
            .ok()
            .map(|rel| {
                rel.components()
                    .any(|component| component.as_os_str() == "artifacts")
            })
            .unwrap_or(false);
        if is_artifact_file {
            artifact_files
                .entry(name.to_string())
                .or_insert_with(|| entry.path().to_path_buf());
        }
    }

    let discovered = if artifact_files.is_empty() {
        all_files
    } else {
        artifact_files
    };
    if discovered.is_empty() {
        return Err(RunReason::from_conf()
            .to_err()
            .with_detail("artifact did not contain any installable files".to_string()));
    }
    Ok(discovered)
}

pub(crate) fn install_bins(
    install_dir: &Path,
    extracted: &HashMap<String, PathBuf>,
    bins: &[String],
) -> RunResult<PathBuf> {
    let update_root = install_dir.join(".warp_parse-update");
    let backup_dir = update_root
        .join("backups")
        .join(format!("{}", Uuid::new_v4()));
    fs::create_dir_all(&backup_dir).map_err(|e| {
        RunReason::from_conf().to_err().with_detail(format!(
            "failed to create backup dir {}: {}",
            backup_dir.display(),
            e
        ))
    })?;

    let mut installed = Vec::new();
    for name in bins {
        let src = extracted.get(name).ok_or_else(|| {
            RunReason::from_conf()
                .to_err()
                .with_detail(format!("missing extracted binary '{}'", name))
        })?;
        let dst = install_dir.join(name);
        let backup = backup_dir.join(name);
        let existed = dst.exists();
        if existed {
            fs::copy(&dst, &backup).map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "failed to back up {} to {}: {}",
                    dst.display(),
                    backup.display(),
                    e
                ))
            })?;
        }

        let staged = update_root.join(format!("{}.{}", name, Uuid::new_v4()));
        if let Some(parent) = staged.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "failed to create staging dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        fs::copy(src, &staged).map_err(|e| {
            RunReason::from_conf().to_err().with_detail(format!(
                "failed to stage {} into {}: {}",
                src.display(),
                staged.display(),
                e
            ))
        })?;
        set_exec_permission(&staged)?;
        if let Err(err) = fs::rename(&staged, &dst) {
            let _ = fs::remove_file(&staged);
            rollback_installed_bins(&installed)?;
            return Err(RunReason::from_conf().to_err().with_detail(format!(
                "failed to install {} into {}: {}",
                src.display(),
                dst.display(),
                err
            )));
        }
        installed.push(InstalledBin {
            dst,
            backup,
            existed,
        });
    }
    Ok(backup_dir)
}

pub(crate) fn rollback_bins(
    install_dir: &Path,
    backup_dir: &Path,
    bins: &[String],
) -> RunResult<()> {
    let installed: Vec<InstalledBin> = bins
        .iter()
        .map(|name| InstalledBin {
            dst: install_dir.join(name),
            backup: backup_dir.join(name),
            existed: backup_dir.join(name).exists(),
        })
        .collect();
    rollback_installed_bins(&installed)
}

fn rollback_installed_bins(installed: &[InstalledBin]) -> RunResult<()> {
    for item in installed.iter().rev() {
        if item.existed {
            fs::copy(&item.backup, &item.dst).map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "failed to restore backup {} to {}: {}",
                    item.backup.display(),
                    item.dst.display(),
                    e
                ))
            })?;
            set_exec_permission(&item.dst)?;
        } else if item.dst.exists() {
            fs::remove_file(&item.dst).map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "failed to remove partially installed {}: {}",
                    item.dst.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

pub(crate) fn run_health_check(
    install_dir: &Path,
    version: &str,
    bins: &[String],
) -> RunResult<()> {
    let expected = version.trim().trim_start_matches('v');
    for name in bins {
        let exe = install_dir.join(name);
        let version_args = [["--version"], ["-V"], ["version"]];
        let help_args = [["--help"], ["help"]];

        let mut saw_success = false;
        let mut saw_version_mismatch = false;
        let mut last_output = String::new();

        for args in version_args {
            let output = Command::new(&exe).args(args).output().map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "health check failed to start {} {}: {}",
                    name,
                    args.join(" "),
                    e
                ))
            })?;
            let merged = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            last_output = merged.trim().to_string();
            if !output.status.success() {
                continue;
            }
            saw_success = true;
            if merged.contains(expected) {
                saw_version_mismatch = false;
                break;
            }
            saw_version_mismatch = true;
        }

        if saw_success && !saw_version_mismatch {
            continue;
        }

        for args in help_args {
            let output = Command::new(&exe).args(args).output().map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "health check failed to start {} {}: {}",
                    name,
                    args.join(" "),
                    e
                ))
            })?;
            let merged = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            last_output = merged.trim().to_string();
            if output.status.success() && !last_output.is_empty() {
                saw_success = true;
                saw_version_mismatch = false;
                break;
            }
        }

        if saw_success && !saw_version_mismatch {
            continue;
        }

        if saw_version_mismatch {
            return Err(RunReason::from_conf().to_err().with_detail(format!(
                "health check version mismatch for {}: expected output to contain '{}', got '{}'",
                name, expected, last_output
            )));
        }

        return Err(RunReason::from_conf().to_err().with_detail(format!(
            "health check failed for {}: no supported version/help probe succeeded",
            name
        )));
    }
    Ok(())
}

fn set_exec_permission(path: &Path) -> RunResult<()> {
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(path)
            .map_err(|e| {
                RunReason::from_conf().to_err().with_detail(format!(
                    "failed to stat {}: {}",
                    path.display(),
                    e
                ))
            })?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| {
            RunReason::from_conf().to_err().with_detail(format!(
                "failed to set executable permission on {}: {}",
                path.display(),
                e
            ))
        })?;
    }
    Ok(())
}

pub(crate) struct InstalledBin {
    pub(crate) dst: PathBuf,
    pub(crate) backup: PathBuf,
    pub(crate) existed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UpdateChannel;
    use crate::UpdateProduct;
    use httpmock::Method::GET;
    use httpmock::MockServer;

    #[test]
    fn package_managed_dir_detects_usr_local_bin() {
        assert!(is_probably_package_managed(Path::new("/usr/local/bin")));
    }

    #[test]
    fn download_url_rejects_untrusted_https_host() {
        let err = validate_download_url(
            "https://evil.example.com/warp-parse-v0.30.0.tar.gz",
            &SourceConfig {
                channel: UpdateChannel::Stable,
                kind: SourceKind::Manifest {
                    updates_base_url: "https://raw.githubusercontent.com/wp-labs/wp-install/main"
                        .to_string(),
                    updates_root: None,
                },
            },
        )
        .unwrap_err();
        assert!(format!("{}", err).contains("allowed release domain"));
    }

    #[test]
    fn find_extracted_bins_accepts_selected_product_bins() {
        let root = std::env::temp_dir().join(format!("wp-update-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("wproj"), "#!/bin/sh\n").expect("write wproj");
        let found =
            find_extracted_bins(&root, &UpdateProduct::Wproj.owned_bins()).expect("find bins");
        assert!(found.contains_key("wproj"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discover_extracted_bins_prefers_artifacts_dir() {
        let root = std::env::temp_dir().join(format!("wp-update-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("artifacts")).expect("create artifacts dir");
        fs::write(root.join("README.txt"), "notes").expect("write readme");
        fs::write(root.join("artifacts").join("warp-parse"), "#!/bin/sh\n")
            .expect("write artifact bin");

        let found = discover_extracted_bins(&root).expect("discover bins");
        assert!(found.contains_key("warp-parse"));
        assert!(!found.contains_key("README.txt"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stage_raw_binary_writes_executable_file() {
        let root = std::env::temp_dir().join(format!("wp-update-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create root");

        let found = stage_raw_binary(b"#!/bin/sh\n", &root, "wp-inst").expect("stage raw binary");
        assert!(found.contains_key("wp-inst"));
        assert!(root.join("wp-inst").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn fetch_asset_bytes_keeps_raw_body_when_server_claims_gzip() {
        let server = MockServer::start();
        let expected = b"not-a-gzip-stream".to_vec();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/artifact.tar.gz");
            then.status(200)
                .header("content-encoding", "gzip")
                .body(expected.clone());
        });

        let bytes = fetch_asset_bytes(&server.url("/artifact.tar.gz"))
            .await
            .expect("fetch asset");
        mock.assert();
        assert_eq!(bytes, expected);
    }
}

use crate::types::{ResolvedRelease, UpdateChannel};
use crate::{
    error::{integrity_check_failed, invalid_request, UpdateResult},
    platform::detect_target_candidates,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct UpdateManifestV2 {
    version: String,
    channel: String,
    assets: HashMap<String, UpdateAssetV2>,
}

#[derive(Debug, Deserialize)]
struct UpdateAssetV2 {
    url: String,
    sha256: String,
}

pub fn updates_manifest_url(base_url: &str, channel: UpdateChannel) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{}/{}/manifest.json", base, channel.as_str())
}

pub fn updates_manifest_path(root: &Path, channel: UpdateChannel) -> PathBuf {
    root.join(channel.as_str()).join("manifest.json")
}

pub fn parse_v2_release(
    raw: &str,
    source: &str,
    expected_channel: UpdateChannel,
) -> UpdateResult<ResolvedRelease> {
    let manifest = serde_json::from_str::<UpdateManifestV2>(raw)
        .map_err(|e| invalid_request(format!("invalid v2 manifest JSON {}: {}", source, e)))?;

    if manifest.channel != expected_channel.as_str() {
        return Err(invalid_request(format!(
            "manifest channel mismatch: expected '{}', got '{}' ({})",
            expected_channel.as_str(),
            manifest.channel,
            source
        )));
    }

    let candidates = detect_target_candidates()?;
    let (target, asset) = select_asset(&manifest.assets, candidates).ok_or_else(|| {
        let mut keys: Vec<&str> = manifest.assets.keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        invalid_request(format!(
            "manifest missing asset for any platform target {:?}: {} (available: {})",
            candidates,
            source,
            keys.join(", ")
        ))
    })?;

    Ok(ResolvedRelease {
        version: manifest.version,
        target: target.to_string(),
        artifact: asset.url.clone(),
        sha256: validate_sha256_hex(&asset.sha256, source, target)?,
    })
}

/// 按候选顺序取 `assets` 中第一个命中的资产，返回 `(命中的 target, 资产)`。
fn select_asset<'a>(
    assets: &'a HashMap<String, UpdateAssetV2>,
    candidates: &'a [&'a str],
) -> Option<(&'a str, &'a UpdateAssetV2)> {
    candidates
        .iter()
        .find_map(|candidate| assets.get(*candidate).map(|asset| (*candidate, asset)))
}

fn validate_sha256_hex(raw: &str, source: &str, target: &str) -> UpdateResult<String> {
    let value = raw.trim().to_ascii_lowercase();
    let is_hex_64 = value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit());
    if is_hex_64 {
        return Ok(value);
    }
    Err(integrity_check_failed(format!(
        "invalid sha256 for target '{}' in {}: expected 64 hex chars, got '{}'",
        target, source, raw
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_manifest_path_mapping_ok() {
        let root = Path::new("./repo");
        assert_eq!(
            updates_manifest_path(root, UpdateChannel::Stable),
            PathBuf::from("./repo/stable/manifest.json")
        );
        assert_eq!(
            updates_manifest_path(root, UpdateChannel::Beta),
            PathBuf::from("./repo/beta/manifest.json")
        );
        assert_eq!(
            updates_manifest_path(root, UpdateChannel::Alpha),
            PathBuf::from("./repo/alpha/manifest.json")
        );
    }

    #[test]
    fn updates_manifest_url_mapping_ok() {
        let base = "https://raw.githubusercontent.com/wp-labs/wp-install/main";
        assert_eq!(
            updates_manifest_url(base, UpdateChannel::Stable),
            "https://raw.githubusercontent.com/wp-labs/wp-install/main/stable/manifest.json"
        );
        assert_eq!(
            updates_manifest_url(base, UpdateChannel::Beta),
            "https://raw.githubusercontent.com/wp-labs/wp-install/main/beta/manifest.json"
        );
        assert_eq!(
            updates_manifest_url(base, UpdateChannel::Alpha),
            "https://raw.githubusercontent.com/wp-labs/wp-install/main/alpha/manifest.json"
        );
    }

    #[test]
    fn parse_v2_release_ok() {
        let raw = r#"{
  "version": "0.12.2-alpha",
  "channel": "alpha",
  "assets": {
    "aarch64-apple-darwin": { "url": "https://example.com/app-v0.12.2-alpha-aarch64-apple-darwin.tar.gz", "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" },
    "aarch64-unknown-linux-gnu": { "url": "https://example.com/app-v0.12.2-alpha-aarch64-unknown-linux-gnu.tar.gz", "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" },
    "x86_64-unknown-linux-gnu": { "url": "https://example.com/app-v0.12.2-alpha-x86_64-unknown-linux-gnu.tar.gz", "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }
  }
}"#;
        let release = parse_v2_release(raw, "test", UpdateChannel::Alpha).unwrap();
        assert_eq!(release.version, "0.12.2-alpha");
    }

    #[test]
    fn parse_v2_release_channel_mismatch_err() {
        let raw = r#"{
  "version": "0.12.2-alpha",
  "channel": "beta",
  "assets": {"aarch64-apple-darwin": { "url": "https://example.com/a.tar.gz", "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }}
}"#;
        let err = parse_v2_release(raw, "test", UpdateChannel::Alpha).unwrap_err();
        assert!(format!("{}", err).contains("channel mismatch"));
    }

    #[test]
    fn parse_v2_release_invalid_sha256_err() {
        let raw = r#"{
  "version": "0.12.2-alpha",
  "channel": "alpha",
  "assets": {
    "aarch64-apple-darwin": { "url": "https://example.com/a.tar.gz", "sha256": "" },
    "aarch64-unknown-linux-gnu": { "url": "https://example.com/b.tar.gz", "sha256": "" },
    "x86_64-unknown-linux-gnu": { "url": "https://example.com/c.tar.gz", "sha256": "" }
  }
}"#;
        let err = parse_v2_release(raw, "test", UpdateChannel::Alpha).unwrap_err();
        assert!(format!("{}", err).contains("invalid sha256"));
    }

    fn asset(url: &str) -> UpdateAssetV2 {
        UpdateAssetV2 {
            url: url.to_string(),
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        }
    }

    #[test]
    fn select_asset_prefers_first_candidate() {
        // musl 与 gnu 同时存在时，musl 优先。
        let mut assets = HashMap::new();
        assets.insert("x86_64-unknown-linux-musl".to_string(), asset("musl"));
        assets.insert("x86_64-unknown-linux-gnu".to_string(), asset("gnu"));
        let candidates = ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"];

        let (target, selected) = select_asset(&assets, &candidates).unwrap();
        assert_eq!(target, "x86_64-unknown-linux-musl");
        assert_eq!(selected.url, "musl");
    }

    #[test]
    fn select_asset_falls_back_to_gnu() {
        // 仅有 gnu 的旧发布：回退命中 gnu。
        let mut assets = HashMap::new();
        assets.insert("x86_64-unknown-linux-gnu".to_string(), asset("gnu"));
        let candidates = ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"];

        let (target, selected) = select_asset(&assets, &candidates).unwrap();
        assert_eq!(target, "x86_64-unknown-linux-gnu");
        assert_eq!(selected.url, "gnu");
    }

    #[test]
    fn select_asset_none_when_no_candidate_present() {
        let mut assets = HashMap::new();
        assets.insert("aarch64-apple-darwin".to_string(), asset("darwin"));
        let candidates = ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"];

        assert!(select_asset(&assets, &candidates).is_none());
    }
}

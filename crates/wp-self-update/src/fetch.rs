use crate::error::{remote_fetch_failed, UpdateResult};
use crate::{
    parse_v2_release, updates_manifest_path, updates_manifest_url, GithubReleaseAssetInfo,
    GithubReleaseInfo, GithubRepo, ResolvedRelease, SourceConfig, SourceKind, UpdateChannel,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::time::Duration;

const FETCH_CONNECT_TIMEOUT_SECS: u64 = 5;
const FETCH_REQUEST_TIMEOUT_SECS: u64 = 10;
const FETCH_RETRY_MAX_ATTEMPTS: usize = 3;

pub(crate) async fn load_release(
    source: &SourceConfig,
    channel: UpdateChannel,
) -> UpdateResult<(ResolvedRelease, String)> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(FETCH_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(FETCH_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| remote_fetch_failed(format!("failed to build HTTP client: {}", e)))?;

    match &source.kind {
        SourceKind::Manifest {
            updates_base_url,
            updates_root,
        } => {
            if let Some(root) = updates_root.as_deref() {
                let path = updates_manifest_path(root, channel);
                let raw = std::fs::read_to_string(&path).map_err(|e| {
                    remote_fetch_failed(format!(
                        "failed to read manifest {}: {}",
                        path.display(),
                        e
                    ))
                })?;
                let release = parse_v2_release(&raw, &path.display().to_string(), channel)?;
                return Ok((release, path.display().to_string()));
            }

            let url = updates_manifest_url(updates_base_url, channel);
            let raw = fetch_text(&client, &url, true).await?;
            let release = parse_v2_release(&raw, &url, channel)?;
            Ok((release, url))
        }
        SourceKind::GithubLatest { repo } => {
            let url = repo.latest_release_api_url();
            let raw = fetch_github_release_text(&client, &url).await?;
            let release = parse_github_release(&raw, repo, &url)?;
            Ok((release, url))
        }
        SourceKind::GithubTag { repo, tag } => {
            let url = repo.tag_release_api_url(tag);
            let raw = fetch_github_release_text(&client, &url).await?;
            let release = parse_github_release(&raw, repo, &url)?;
            Ok((release, url))
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

async fn fetch_github_release_text(client: &reqwest::Client, url: &str) -> UpdateResult<String> {
    let request = client
        .get(url)
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", "wp-inst");
    fetch_text_from_request(request, url, false).await
}

pub async fn load_github_release_info(
    repo: &GithubRepo,
    tag: Option<&str>,
) -> UpdateResult<GithubReleaseInfo> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(FETCH_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(FETCH_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| remote_fetch_failed(format!("failed to build HTTP client: {}", e)))?;

    let url = match tag {
        Some(tag) => repo.tag_release_api_url(tag),
        None => repo.latest_release_api_url(),
    };
    let raw = fetch_github_release_text(&client, &url).await?;
    parse_github_release_info(&raw, &url)
}

async fn fetch_text(
    client: &reqwest::Client,
    url: &str,
    not_found_is_terminal: bool,
) -> UpdateResult<String> {
    fetch_text_from_request(client.get(url), url, not_found_is_terminal).await
}

async fn fetch_text_from_request(
    request: reqwest::RequestBuilder,
    url: &str,
    not_found_is_terminal: bool,
) -> UpdateResult<String> {
    let mut last_error: Option<String> = None;
    for attempt in 1..=FETCH_RETRY_MAX_ATTEMPTS {
        let Some(request) = request.try_clone() else {
            return Err(remote_fetch_failed(format!(
                "failed to clone HTTP request for {}",
                url
            )));
        };
        match request.send().await {
            Ok(rsp) => {
                let status = rsp.status();
                if status.is_success() {
                    return rsp.text().await.map_err(|e| {
                        remote_fetch_failed(format!("failed to read response {}: {}", url, e))
                    });
                }
                if not_found_is_terminal && status == StatusCode::NOT_FOUND {
                    return Err(remote_fetch_failed(format!("manifest not found: {}", url)));
                }
                if is_retryable_status(status) && attempt < FETCH_RETRY_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    continue;
                }
                return Err(remote_fetch_failed(format!(
                    "request failed {}: HTTP {}",
                    url, status
                )));
            }
            Err(e) => {
                last_error = Some(e.to_string());
                if attempt < FETCH_RETRY_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    continue;
                }
            }
        }
    }
    Err(remote_fetch_failed(format!(
        "failed to fetch manifest {} after {} attempts: {}",
        url,
        FETCH_RETRY_MAX_ATTEMPTS,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
}

fn parse_github_release(
    raw: &str,
    repo: &GithubRepo,
    source: &str,
) -> UpdateResult<ResolvedRelease> {
    let release = serde_json::from_str::<GithubLatestRelease>(raw).map_err(|e| {
        remote_fetch_failed(format!("invalid GitHub release JSON {}: {}", source, e))
    })?;

    let target = crate::platform::detect_target_triple_v2()?;
    let asset = select_github_release_asset(&release.assets, target).ok_or_else(|| {
        let mut names: Vec<&str> = release
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .collect();
        names.sort_unstable();
        remote_fetch_failed(format!(
            "GitHub release missing asset for target '{}': {} (available: {})",
            target,
            repo.url,
            names.join(", ")
        ))
    })?;

    let digest = asset.digest.as_deref().ok_or_else(|| {
        remote_fetch_failed(format!(
            "GitHub release asset '{}' is missing sha256 digest metadata: {}",
            asset.name, repo.url
        ))
    })?;
    let sha256 = parse_github_asset_digest(digest, &asset.name, source)?;

    Ok(ResolvedRelease {
        version: release.tag_name,
        target: target.to_string(),
        artifact: asset.browser_download_url.clone(),
        sha256,
    })
}

fn parse_github_release_info(raw: &str, source: &str) -> UpdateResult<GithubReleaseInfo> {
    let release = serde_json::from_str::<GithubLatestRelease>(raw).map_err(|e| {
        remote_fetch_failed(format!("invalid GitHub release JSON {}: {}", source, e))
    })?;

    Ok(GithubReleaseInfo {
        tag_name: release.tag_name,
        assets: release
            .assets
            .into_iter()
            .map(|asset| GithubReleaseAssetInfo {
                name: asset.name,
                browser_download_url: asset.browser_download_url,
            })
            .collect(),
    })
}

fn select_github_release_asset<'a>(
    assets: &'a [GithubReleaseAsset],
    target: &str,
) -> Option<&'a GithubReleaseAsset> {
    let mut exact_raw = None;
    let mut raw = None;
    let mut archive = None;

    for asset in assets {
        if !asset.name.contains(target) {
            continue;
        }
        if asset.name.ends_with(target) {
            exact_raw = Some(asset);
            continue;
        }
        if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz") {
            archive = Some(asset);
            continue;
        }
        raw = Some(asset);
    }

    exact_raw.or(raw).or(archive)
}

fn parse_github_asset_digest(raw: &str, asset_name: &str, source: &str) -> UpdateResult<String> {
    let Some(value) = raw.strip_prefix("sha256:") else {
        return Err(remote_fetch_failed(format!(
            "unsupported GitHub asset digest '{}' for {} ({})",
            raw, asset_name, source
        )));
    };

    let normalized = value.trim().to_ascii_lowercase();
    let is_hex_64 = normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit());
    if is_hex_64 {
        return Ok(normalized);
    }
    Err(remote_fetch_failed(format!(
        "invalid GitHub asset sha256 '{}' for {} ({})",
        raw, asset_name, source
    )))
}

pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_status_rules_ok() {
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn parse_github_asset_digest_accepts_sha256_prefix() {
        let value = parse_github_asset_digest(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "wpl-check",
            "test",
        )
        .unwrap();
        assert_eq!(
            value,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn parse_github_release_info_collects_assets() {
        let info = parse_github_release_info(
            r#"{
              "tag_name": "v0.1.2",
              "assets": [
                {"name": "wp-skills-v0.1.2.tar.gz", "browser_download_url": "https://example.com/a.tar.gz", "digest": null}
              ]
            }"#,
            "test",
        )
        .unwrap();

        assert_eq!(info.tag_name, "v0.1.2");
        assert_eq!(info.assets.len(), 1);
        assert_eq!(info.assets[0].name, "wp-skills-v0.1.2.tar.gz");
        assert_eq!(
            info.assets[0].browser_download_url,
            "https://example.com/a.tar.gz"
        );
    }

    #[test]
    fn select_github_release_asset_prefers_raw_binary() {
        let assets = vec![
            GithubReleaseAsset {
                name: "wpl-check-v0.1.7-aarch64-apple-darwin.tar.gz".to_string(),
                browser_download_url: "https://example.com/archive".to_string(),
                digest: Some(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                ),
            },
            GithubReleaseAsset {
                name: "wpl-check-v0.1.7-aarch64-apple-darwin".to_string(),
                browser_download_url: "https://example.com/raw".to_string(),
                digest: Some(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                ),
            },
        ];

        let selected = select_github_release_asset(&assets, "aarch64-apple-darwin").unwrap();
        assert_eq!(selected.browser_download_url, "https://example.com/raw");
    }
}

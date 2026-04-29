use crate::cli::{Channel, CheckArgs, CommonArgs, InstallArgs};
use crate::error::{invalid_request, InstallerResult};
use std::env;
use std::path::PathBuf;
use wp_self_update::{GithubRepo, SourceConfig, SourceKind, UpdateChannel, UpdateTarget};

const DEFAULT_MANIFEST_BASE_URL_ENV: &str = "WP_INSTALLER_DEFAULT_BASE_URL";
const DEFAULT_MANIFEST_ROOT_ENV: &str = "WP_INSTALLER_DEFAULT_ROOT";
pub(crate) const CUSTOM_PRODUCT_LABEL: &str = "custom";

#[derive(Debug, Clone)]
struct SourceDefaults {
    updates_base_url: Option<String>,
    updates_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum ManifestSourceRef {
    BaseUrl(String),
    LocalRoot(PathBuf),
}

pub(crate) fn resolve_source_config(args: &CommonArgs) -> InstallerResult<SourceConfig> {
    if args.github.is_some() {
        if args.source.is_some() || args.updates_base_url.is_some() || args.updates_root.is_some() {
            return Err(invalid_request(
                "--github cannot be combined with --source, --base-url, or --local-root",
            ));
        }
        if args.effective_channel() != Channel::Stable {
            return Err(invalid_request(
                "--github release selection does not support --channel; omit it",
            ));
        }

        let repo = GithubRepo::parse(
            args.github
                .as_deref()
                .ok_or_else(|| invalid_request("missing GitHub repository"))?,
        )
        .map_err(|e| invalid_request(format!("invalid GitHub repository: {}", e)))?;

        return Ok(SourceConfig {
            channel: UpdateChannel::Stable,
            kind: match args.tag.clone() {
                Some(tag) => SourceKind::GithubTag { repo, tag },
                None => SourceKind::GithubLatest { repo },
            },
        });
    }

    if args.tag.is_some() || args.latest {
        return Err(invalid_request("--tag requires --github <repo>"));
    }

    let defaults = default_source_overrides();
    let source_ref = args
        .source
        .as_deref()
        .map(parse_manifest_source_ref)
        .transpose()?;

    let updates_root = args
        .updates_root
        .clone()
        .or_else(|| match &source_ref {
            Some(ManifestSourceRef::LocalRoot(path)) => Some(path.clone()),
            _ => None,
        })
        .or(defaults.updates_root);
    let updates_base_url = args
        .updates_base_url
        .clone()
        .or_else(|| match &source_ref {
            Some(ManifestSourceRef::BaseUrl(url)) => Some(url.clone()),
            _ => None,
        })
        .or(defaults.updates_base_url);

    if updates_root.is_none() && updates_base_url.is_none() {
        return Err(invalid_request(format!(
            "manifest source is required: provide --source, --base-url, --local-root, or set {} / {}",
            DEFAULT_MANIFEST_BASE_URL_ENV, DEFAULT_MANIFEST_ROOT_ENV
        )));
    }

    Ok(SourceConfig {
        channel: match args.effective_channel() {
            Channel::Stable => UpdateChannel::Stable,
            Channel::Beta => UpdateChannel::Beta,
            Channel::Alpha => UpdateChannel::Alpha,
        },
        kind: SourceKind::Manifest {
            updates_base_url: updates_base_url.unwrap_or_default(),
            updates_root,
        },
    })
}

pub(crate) fn current_check_version_or_default(args: &CheckArgs, default: &str) -> String {
    args.current_version
        .clone()
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn current_install_version_or_default(args: &InstallArgs, default: &str) -> String {
    args.current_version
        .clone()
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn default_update_target(source: &SourceConfig) -> UpdateTarget {
    match &source.kind {
        SourceKind::Manifest { .. } => UpdateTarget::Auto,
        SourceKind::GithubLatest { repo } | SourceKind::GithubTag { repo, .. } => {
            UpdateTarget::Bins(vec![repo.name.clone()])
        }
    }
}

pub(crate) fn product_label_for_source(source: &SourceConfig) -> String {
    match &source.kind {
        SourceKind::Manifest { .. } => CUSTOM_PRODUCT_LABEL.to_string(),
        SourceKind::GithubLatest { repo } | SourceKind::GithubTag { repo, .. } => repo.name.clone(),
    }
}

pub(crate) fn source_branch_name(args: &CommonArgs) -> String {
    if args.github.is_some() && args.tag.is_none() {
        return "main".to_string();
    }
    if let Some(tag) = &args.tag {
        return tag.clone();
    }
    "installer".to_string()
}

fn parse_manifest_source_ref(raw: &str) -> InstallerResult<ManifestSourceRef> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(invalid_request("--source cannot be empty"));
    }
    if value.starts_with("https://") || value.starts_with("http://") {
        return Ok(ManifestSourceRef::BaseUrl(value.to_string()));
    }
    Ok(ManifestSourceRef::LocalRoot(PathBuf::from(value)))
}

fn default_source_overrides() -> SourceDefaults {
    SourceDefaults {
        updates_base_url: env::var(DEFAULT_MANIFEST_BASE_URL_ENV).ok(),
        updates_root: env::var_os(DEFAULT_MANIFEST_ROOT_ENV).map(PathBuf::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ArtifactKind, CommonArgs, KindArgs};

    #[test]
    fn resolve_source_config_builds_channel_relative_manifest_root() {
        let source = resolve_source_config(&CommonArgs {
            source: Some("./updates".to_string()),
            channel: Some(Channel::Beta),
            ..CommonArgs::default()
        })
        .unwrap();

        assert_eq!(source.channel, UpdateChannel::Beta);
        match source.kind {
            SourceKind::Manifest {
                updates_base_url,
                updates_root,
            } => {
                assert_eq!(updates_base_url, "");
                assert_eq!(updates_root, Some(PathBuf::from("./updates")));
            }
            _ => panic!("expected manifest source"),
        }
    }

    #[test]
    fn resolve_source_config_rejects_missing_manifest_source() {
        let err = resolve_source_config(&CommonArgs::default()).unwrap_err();
        assert!(err.to_string().contains("manifest source is required"));
    }

    #[test]
    fn resolve_source_config_builds_github_tag_source() {
        let source = resolve_source_config(&CommonArgs {
            github: Some("https://github.com/wp-labs/wpl-check".to_string()),
            tag: Some("v0.1.7".to_string()),
            ..CommonArgs::default()
        })
        .unwrap();

        match source.kind {
            SourceKind::GithubTag { repo, tag } => {
                assert_eq!(repo.name, "wpl-check");
                assert_eq!(tag, "v0.1.7");
            }
            _ => panic!("expected github tag source"),
        }
    }

    #[test]
    fn resolve_source_config_defaults_github_to_latest() {
        let source = resolve_source_config(&CommonArgs {
            github: Some("https://github.com/wp-labs/wpl-check".to_string()),
            ..CommonArgs::default()
        })
        .unwrap();

        match source.kind {
            SourceKind::GithubLatest { repo } => {
                assert_eq!(repo.name, "wpl-check");
            }
            _ => panic!("expected github latest source"),
        }
    }

    #[test]
    fn common_args_default_to_binary_mode() {
        let args = CommonArgs::default();
        assert_eq!(args.artifact_kind(), ArtifactKind::Bin);
        assert_eq!(args.kind, KindArgs::default());
    }
}

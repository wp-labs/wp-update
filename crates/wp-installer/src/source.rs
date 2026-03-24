use crate::cli::{Channel, RequestArgs, SourceArgs};
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

pub(crate) fn resolve_source_config(
    source: &SourceArgs,
) -> Result<SourceConfig, Box<dyn std::error::Error>> {
    if source.github.is_some() {
        if source.updates_base_url.is_some() || source.updates_root.is_some() {
            return Err("--github cannot be combined with --base-url or --local-root".into());
        }
        if source.latest && source.tag.is_some() {
            return Err("--github cannot combine --latest with --tag <tag>".into());
        }
        if source.channel != Channel::Stable {
            return Err("--github release selection does not support --channel; omit it".into());
        }

        let repo = GithubRepo::parse(
            source
                .github
                .as_deref()
                .ok_or_else(|| "missing GitHub repository".to_string())?,
        )
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        return Ok(SourceConfig {
            channel: UpdateChannel::Stable,
            kind: match source.tag.clone() {
                Some(tag) => SourceKind::GithubTag { repo, tag },
                None => SourceKind::GithubLatest { repo },
            },
        });
    }

    if source.latest {
        return Err("--latest requires --github <repo>".into());
    }
    if source.tag.is_some() {
        return Err("--tag requires --github <repo>".into());
    }

    let defaults = default_source_overrides();
    let updates_root = source.updates_root.clone().or(defaults.updates_root);
    let updates_base_url = source
        .updates_base_url
        .clone()
        .or(defaults.updates_base_url);

    if updates_root.is_none() && updates_base_url.is_none() {
        return Err(format!(
            "manifest source is required: provide --base-url, --local-root, or set {} / {}",
            DEFAULT_MANIFEST_BASE_URL_ENV, DEFAULT_MANIFEST_ROOT_ENV
        )
        .into());
    }

    Ok(SourceConfig {
        channel: match source.channel {
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

pub(crate) fn current_version_or_default(args: &RequestArgs, default: &str) -> String {
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

pub(crate) fn source_branch_name(source: &SourceArgs) -> String {
    if source.github.is_some() && source.latest {
        return "main".to_string();
    }
    if let Some(tag) = &source.tag {
        return tag.clone();
    }
    "installer".to_string()
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

    #[test]
    fn resolve_source_config_builds_channel_relative_manifest_root() {
        let source = resolve_source_config(&SourceArgs {
            channel: Channel::Beta,
            updates_base_url: Some("https://example.com/releases/warp-parse".to_string()),
            updates_root: None,
            github: None,
            latest: false,
            tag: None,
            json: false,
        })
        .unwrap();

        assert_eq!(source.channel, UpdateChannel::Beta);
        match source.kind {
            SourceKind::Manifest {
                updates_base_url,
                updates_root,
            } => {
                assert_eq!(updates_base_url, "https://example.com/releases/warp-parse");
                assert_eq!(updates_root, None);
            }
            _ => panic!("expected manifest source"),
        }
    }

    #[test]
    fn resolve_source_config_rejects_missing_manifest_source() {
        let err = resolve_source_config(&SourceArgs {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
            github: None,
            latest: false,
            tag: None,
            json: false,
        })
        .unwrap_err();

        assert!(err.to_string().contains("manifest source is required"));
    }

    #[test]
    fn resolve_source_config_builds_github_tag_source() {
        let source = resolve_source_config(&SourceArgs {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
            github: Some("https://github.com/wp-labs/wpl-check".to_string()),
            latest: false,
            tag: Some("v0.1.7".to_string()),
            json: false,
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
    fn resolve_source_config_rejects_conflicting_github_selectors() {
        let err = resolve_source_config(&SourceArgs {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
            github: Some("https://github.com/wp-labs/wpl-check".to_string()),
            latest: true,
            tag: Some("v0.1.7".to_string()),
            json: false,
        })
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("cannot combine --latest with --tag"));
    }

    #[test]
    fn resolve_source_config_defaults_github_to_latest() {
        let source = resolve_source_config(&SourceArgs {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
            github: Some("https://github.com/wp-labs/wpl-check".to_string()),
            latest: false,
            tag: None,
            json: false,
        })
        .unwrap();

        match source.kind {
            SourceKind::GithubLatest { repo } => {
                assert_eq!(repo.name, "wpl-check");
            }
            _ => panic!("expected github latest source"),
        }
    }
}

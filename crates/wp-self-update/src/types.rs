use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
    Alpha,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub channel: UpdateChannel,
    pub kind: SourceKind,
}

#[derive(Debug, Clone)]
pub enum SourceKind {
    Manifest {
        updates_base_url: String,
        updates_root: Option<PathBuf>,
    },
    GithubLatest {
        repo: GithubRepo,
    },
}

#[derive(Debug, Clone)]
pub struct CheckRequest {
    pub product: String,
    pub source: SourceConfig,
    pub current_version: String,
    pub branch: String,
}

#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub product: String,
    pub target: UpdateTarget,
    pub source: SourceConfig,
    pub current_version: String,
    pub install_dir: Option<PathBuf>,
    pub yes: bool,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub product: String,
    pub channel: String,
    pub branch: String,
    pub source: String,
    pub manifest_format: String,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub platform_key: String,
    pub artifact: String,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub product: String,
    pub channel: String,
    pub source: String,
    pub current_version: String,
    pub latest_version: String,
    pub install_dir: String,
    pub artifact: String,
    pub dry_run: bool,
    pub updated: bool,
    pub status: String,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VersionRelation {
    UpdateAvailable,
    UpToDate,
    AheadOfChannel,
}

#[derive(Debug)]
pub struct ResolvedRelease {
    pub version: String,
    pub target: String,
    pub artifact: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GithubRepo {
    pub owner: String,
    pub name: String,
    pub url: String,
}

impl GithubRepo {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let value = raw.trim().trim_end_matches('/');
        if value.is_empty() {
            return Err("GitHub repository cannot be empty".to_string());
        }

        let (owner, name) = if let Some(rest) = value.strip_prefix("https://github.com/") {
            parse_repo_segments(rest)?
        } else if let Some(rest) = value.strip_prefix("http://github.com/") {
            parse_repo_segments(rest)?
        } else if value.contains('/') && !value.contains("://") {
            parse_repo_segments(value)?
        } else {
            return Err(format!(
                "unsupported GitHub repository reference '{}': use https://github.com/<owner>/<repo> or <owner>/<repo>",
                raw
            ));
        };

        Ok(Self {
            url: format!("https://github.com/{owner}/{name}"),
            owner,
            name,
        })
    }

    pub fn latest_release_api_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.name
        )
    }
}

fn parse_repo_segments(raw: &str) -> Result<(String, String), String> {
    let mut parts = raw
        .split('/')
        .filter(|segment| !segment.is_empty())
        .take(2)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(format!(
            "invalid GitHub repository reference '{}': expected <owner>/<repo>",
            raw
        ));
    }
    if let Some(name) = parts.get_mut(1) {
        if let Some(trimmed) = name.strip_suffix(".git") {
            *name = trimmed.to_string();
        }
    }
    Ok((parts.remove(0), parts.remove(0)))
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UpdateTarget {
    Product(UpdateProduct),
    Auto,
    Bins(Vec<String>),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateProduct {
    Suite,
    Wparse,
    Wpgen,
    Wprescue,
    Wproj,
}

impl UpdateProduct {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Suite => "suite",
            Self::Wparse => "wparse",
            Self::Wpgen => "wpgen",
            Self::Wprescue => "wprescue",
            Self::Wproj => "wproj",
        }
    }

    pub fn bins(self) -> &'static [&'static str] {
        match self {
            Self::Suite => &["wparse", "wpgen", "wprescue", "wproj"],
            Self::Wparse => &["wparse"],
            Self::Wpgen => &["wpgen"],
            Self::Wprescue => &["wprescue"],
            Self::Wproj => &["wproj"],
        }
    }

    pub fn owned_bins(self) -> Vec<String> {
        self.bins().iter().map(|bin| (*bin).to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_repo_from_full_url() {
        let repo = GithubRepo::parse("https://github.com/wp-labs/wpl-check").unwrap();
        assert_eq!(repo.owner, "wp-labs");
        assert_eq!(repo.name, "wpl-check");
        assert_eq!(repo.url, "https://github.com/wp-labs/wpl-check");
    }

    #[test]
    fn parse_github_repo_from_short_form() {
        let repo = GithubRepo::parse("wp-labs/wpl-check").unwrap();
        assert_eq!(repo.owner, "wp-labs");
        assert_eq!(repo.name, "wpl-check");
    }
}

use std::path::{Component, Path, PathBuf};
use wp_self_update::GithubRepo;

#[derive(Debug, Clone)]
pub(crate) struct SkillInstallArgs {
    pub(crate) github: String,
    pub(crate) latest: bool,
    pub(crate) tag: Option<String>,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct SkillSource {
    pub(super) skill_name: String,
    pub(super) repo: GithubRepo,
    pub(super) subdir: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum SkillReleaseSelector {
    Latest,
    Tag(String),
}

pub(super) fn parse_skill_source(
    args: &SkillInstallArgs,
) -> Result<(SkillSource, SkillReleaseSelector), Box<dyn std::error::Error>> {
    let repo = GithubRepo::parse(&args.github)
        .map_err(|err| format!("invalid GitHub repository '{}': {}", args.github, err))?;
    let subdir = normalize_skill_path(&args.path)?;
    let skill_name = subdir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid skill path '{}'", args.path))?
        .to_string();
    let selector = if args.latest {
        SkillReleaseSelector::Latest
    } else {
        match args.tag.clone() {
            Some(tag) => SkillReleaseSelector::Tag(tag),
            None => SkillReleaseSelector::Latest,
        }
    };

    Ok((
        SkillSource {
            skill_name,
            repo,
            subdir,
        },
        selector,
    ))
}

fn normalize_skill_path(raw: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let value = raw.trim().trim_matches('/');
    if value.is_empty() {
        return Err("skill path cannot be empty".into());
    }

    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("skill path must be relative: {}", raw).into());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("skill path cannot contain '..': {}", raw).into());
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_source_from_repo_and_path() {
        let (source, selector) = parse_skill_source(&SkillInstallArgs {
            github: "wp-labs/wp-skills".to_string(),
            latest: false,
            tag: Some("v0.1.2".to_string()),
            path: "skills/warpparse-log-engineering".to_string(),
        })
        .unwrap();

        assert_eq!(source.skill_name, "warpparse-log-engineering");
        assert_eq!(source.repo.name, "wp-skills");
        assert_eq!(
            source.subdir,
            PathBuf::from("skills/warpparse-log-engineering")
        );
        assert_eq!(selector, SkillReleaseSelector::Tag("v0.1.2".to_string()));
    }

    #[test]
    fn defaults_skill_source_selector_to_latest() {
        let (_, selector) = parse_skill_source(&SkillInstallArgs {
            github: "wp-labs/wp-skills".to_string(),
            latest: false,
            tag: None,
            path: "skills/warpparse-log-engineering".to_string(),
        })
        .unwrap();

        assert_eq!(selector, SkillReleaseSelector::Latest);
    }

    #[test]
    fn rejects_parent_directory_in_skill_path() {
        let err = parse_skill_source(&SkillInstallArgs {
            github: "wp-labs/wp-skills".to_string(),
            latest: true,
            tag: None,
            path: "../skills/warpparse-log-engineering".to_string(),
        })
        .unwrap_err();

        assert!(err.to_string().contains("cannot contain '..'"));
    }
}

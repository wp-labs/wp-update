use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SKILLS_PLATFORM_ENV: &str = "WP_SKILLS_PLATFORM";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct InstalledSkill {
    pub(super) platform: String,
    pub(super) location: PathBuf,
    pub(super) files: Vec<PathBuf>,
}

pub(super) fn resolve_default_target_dirs() -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut target_dirs = Vec::new();

    if env::var(SKILLS_PLATFORM_ENV).is_err() {
        let codex = home_subdir(".codex/skills")?;
        let claude = home_subdir(".claude/skills")?;
        if codex.is_dir() {
            target_dirs.push(codex);
        }
        if claude.is_dir() {
            target_dirs.push(claude.clone());
        }
        if target_dirs.is_empty() {
            target_dirs.push(claude);
        }
    } else {
        match env::var(SKILLS_PLATFORM_ENV)?.as_str() {
            "codex" => target_dirs.push(home_subdir(".codex/skills")?),
            "claude-code" => target_dirs.push(home_subdir(".claude/skills")?),
            "auto" => {
                let claude = home_subdir(".claude/skills")?;
                let codex = home_subdir(".codex/skills")?;
                if claude.is_dir() {
                    target_dirs.push(claude);
                } else if codex.is_dir() {
                    target_dirs.push(codex);
                } else {
                    target_dirs.push(claude);
                }
            }
            _ => target_dirs.push(home_subdir(".claude/skills")?),
        }
    }

    target_dirs.sort();
    target_dirs.dedup();
    Ok(target_dirs)
}

pub(super) fn install_skill_into_target(
    skill_name: &str,
    src_dir: &Path,
    target_base: &Path,
) -> Result<InstalledSkill, Box<dyn std::error::Error>> {
    fs::create_dir_all(target_base)?;
    let dst_dir = target_base.join(skill_name);
    if dst_dir.exists() {
        fs::remove_dir_all(&dst_dir)?;
    }
    copy_dir_recursive(src_dir, &dst_dir)?;

    Ok(InstalledSkill {
        platform: platform_name(target_base),
        location: dst_dir.clone(),
        files: list_relative_files(&dst_dir)?,
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
            continue;
        }

        fs::copy(&src_path, &dst_path)?;
        let permissions = fs::metadata(&src_path)?.permissions();
        fs::set_permissions(&dst_path, permissions)?;
    }
    Ok(())
}

fn list_relative_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_relative_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_relative_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_relative_files(root, &path, files)?;
            continue;
        }
        files.push(path.strip_prefix(root)?.to_path_buf());
    }
    Ok(())
}

fn platform_name(target_base: &Path) -> String {
    let path = target_base.to_string_lossy();
    if path.ends_with("/.codex/skills") {
        "codex".to_string()
    } else if path.ends_with("/.claude/skills") {
        "claude-code".to_string()
    } else {
        "custom".to_string()
    }
}

fn home_subdir(suffix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = env::var("HOME").map_err(|_| "HOME is not set")?;
    Ok(PathBuf::from(home).join(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolves_existing_platform_dirs_when_platform_env_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        let home = tempdir().unwrap();
        fs::create_dir_all(home.path().join(".codex/skills")).unwrap();
        fs::create_dir_all(home.path().join(".claude/skills")).unwrap();

        env::set_var("HOME", home.path());
        env::remove_var(SKILLS_PLATFORM_ENV);

        let dirs = resolve_default_target_dirs().unwrap();

        assert_eq!(
            dirs,
            vec![
                home.path().join(".claude/skills"),
                home.path().join(".codex/skills")
            ]
        );
    }

    #[test]
    fn installs_skill_tree_into_target_dir() {
        let src = tempdir().unwrap();
        let target = tempdir().unwrap();
        fs::create_dir_all(src.path().join("nested")).unwrap();
        fs::write(src.path().join("SKILL.md"), "content").unwrap();
        fs::write(src.path().join("nested/info.txt"), "nested").unwrap();

        let installed =
            install_skill_into_target("warpparse-log-engineering", src.path(), target.path())
                .unwrap();

        assert_eq!(
            installed.location,
            target.path().join("warpparse-log-engineering")
        );
        assert!(installed.location.join("SKILL.md").exists());
        assert!(installed.location.join("nested/info.txt").exists());
        assert_eq!(
            installed.files,
            vec![PathBuf::from("SKILL.md"), PathBuf::from("nested/info.txt")]
        );
    }
}

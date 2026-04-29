use crate::error::{skill_install_failed, InstallerReason, InstallerResult};
use orion_error::IntoAs;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SKILLS_PLATFORM_ENV: &str = "WP_SKILLS_PLATFORM";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct InstalledSkill {
    pub(super) platform: String,
    pub(super) location: PathBuf,
    pub(super) files: Vec<PathBuf>,
}

pub(super) fn resolve_default_target_dirs() -> InstallerResult<Vec<PathBuf>> {
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
        match env::var(SKILLS_PLATFORM_ENV)
            .map_err(|e| {
                orion_error::StructError::builder(InstallerReason::SkillInstallFailed)
                    .detail("failed to read WP_SKILLS_PLATFORM")
                    .source_std(e)
                    .finish()
            })?
            .as_str()
        {
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
) -> InstallerResult<InstalledSkill> {
    fs::create_dir_all(target_base).into_as(
        InstallerReason::SkillInstallFailed,
        format!(
            "failed to create skill target dir {}",
            target_base.display()
        ),
    )?;
    let dst_dir = target_base.join(skill_name);
    let staged_dir = unique_work_path(target_base, &format!(".{skill_name}.staging"))?;
    copy_dir_recursive(src_dir, &staged_dir).map_err(|err| {
        let _ = fs::remove_dir_all(&staged_dir);
        err
    })?;

    let backup_dir = if dst_dir.exists() {
        let backup_dir = unique_work_path(target_base, &format!(".{skill_name}.backup"))?;
        fs::rename(&dst_dir, &backup_dir).map_err(|e| {
            let _ = fs::remove_dir_all(&staged_dir);
            skill_install_failed(format!(
                "failed to move existing skill {} to backup {}: {}",
                dst_dir.display(),
                backup_dir.display(),
                e
            ))
        })?;
        Some(backup_dir)
    } else {
        None
    };

    if let Err(err) = fs::rename(&staged_dir, &dst_dir) {
        let _ = fs::remove_dir_all(&staged_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        if let Some(backup_dir) = &backup_dir {
            if let Err(restore_err) = fs::rename(backup_dir, &dst_dir) {
                return Err(skill_install_failed(format!(
                    "failed to activate skill {} at {}: {}; additionally failed to restore backup {}: {}",
                    skill_name,
                    dst_dir.display(),
                    err,
                    backup_dir.display(),
                    restore_err
                )));
            }
        }
        return Err(skill_install_failed(format!(
            "failed to activate skill {} at {}: {}",
            skill_name,
            dst_dir.display(),
            err
        )));
    }

    if let Some(backup_dir) = backup_dir {
        let _ = fs::remove_dir_all(&backup_dir);
    }

    Ok(InstalledSkill {
        platform: platform_name(target_base),
        location: dst_dir.clone(),
        files: list_relative_files(&dst_dir)?,
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> InstallerResult<()> {
    fs::create_dir_all(dst).into_as(
        InstallerReason::SkillInstallFailed,
        format!("failed to create staged skill dir {}", dst.display()),
    )?;
    for entry in fs::read_dir(src).into_as(
        InstallerReason::SkillInstallFailed,
        format!("failed to read skill source dir {}", src.display()),
    )? {
        let entry = entry.into_as(
            InstallerReason::SkillInstallFailed,
            format!("failed to read entry under {}", src.display()),
        )?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().into_as(
            InstallerReason::SkillInstallFailed,
            format!("failed to inspect {}", src_path.display()),
        )?;

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
            continue;
        }

        fs::copy(&src_path, &dst_path).into_as(
            InstallerReason::SkillInstallFailed,
            format!(
                "failed to copy skill file {} to {}",
                src_path.display(),
                dst_path.display()
            ),
        )?;
        let permissions = fs::metadata(&src_path)
            .into_as(
                InstallerReason::SkillInstallFailed,
                format!("failed to read file metadata {}", src_path.display()),
            )?
            .permissions();
        fs::set_permissions(&dst_path, permissions).into_as(
            InstallerReason::SkillInstallFailed,
            format!("failed to set permissions on {}", dst_path.display()),
        )?;
    }
    Ok(())
}

fn list_relative_files(root: &Path) -> InstallerResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_relative_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_relative_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> InstallerResult<()> {
    for entry in fs::read_dir(current).into_as(
        InstallerReason::SkillInstallFailed,
        format!("failed to read installed skill dir {}", current.display()),
    )? {
        let entry = entry.into_as(
            InstallerReason::SkillInstallFailed,
            format!("failed to read entry under {}", current.display()),
        )?;
        let path = entry.path();
        if entry
            .file_type()
            .into_as(
                InstallerReason::SkillInstallFailed,
                format!("failed to inspect installed path {}", path.display()),
            )?
            .is_dir()
        {
            collect_relative_files(root, &path, files)?;
            continue;
        }
        files.push(
            path.strip_prefix(root)
                .map_err(|e| {
                    orion_error::StructError::builder(InstallerReason::SkillInstallFailed)
                        .detail(format!(
                            "failed to compute relative installed path {} from {}",
                            path.display(),
                            root.display()
                        ))
                        .source_std(e)
                        .finish()
                })?
                .to_path_buf(),
        );
    }
    Ok(())
}

fn unique_work_path(target_base: &Path, prefix: &str) -> InstallerResult<PathBuf> {
    let base_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| {
            orion_error::StructError::builder(InstallerReason::SkillInstallFailed)
                .detail("system clock error while creating work path")
                .source_std(e)
                .finish()
        })?
        .as_nanos();
    for attempt in 0..100u32 {
        let candidate = target_base.join(format!(
            "{}-{}-{}-{}",
            prefix,
            std::process::id(),
            base_nanos,
            attempt
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(skill_install_failed(format!(
        "failed to allocate a unique work path under {}",
        target_base.display()
    )))
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

fn home_subdir(suffix: &str) -> InstallerResult<PathBuf> {
    let home = env::var("HOME").map_err(|e| {
        orion_error::StructError::builder(InstallerReason::SkillInstallFailed)
            .detail("HOME is not set")
            .source_std(e)
            .finish()
    })?;
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

    #[test]
    fn replacing_existing_skill_swaps_directory_contents() {
        let src = tempdir().unwrap();
        let target = tempdir().unwrap();

        fs::create_dir_all(src.path().join("nested")).unwrap();
        fs::write(src.path().join("SKILL.md"), "new").unwrap();
        fs::write(src.path().join("nested/info.txt"), "nested").unwrap();

        let existing = target.path().join("warpparse-log-engineering");
        fs::create_dir_all(existing.join("old")).unwrap();
        fs::write(existing.join("SKILL.md"), "old").unwrap();
        fs::write(existing.join("old/legacy.txt"), "legacy").unwrap();

        let installed =
            install_skill_into_target("warpparse-log-engineering", src.path(), target.path())
                .unwrap();

        assert_eq!(
            fs::read_to_string(installed.location.join("SKILL.md")).unwrap(),
            "new"
        );
        assert!(!installed.location.join("old/legacy.txt").exists());
        assert!(installed.location.join("nested/info.txt").exists());
    }
}

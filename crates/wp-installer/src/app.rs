use clap::Parser;
use wp_self_update::{check, update, CheckRequest, UpdateRequest};

use crate::cli::{ArtifactKind, CheckArgs, Cli, Command, CommonArgs, InstallArgs};
use crate::report::{
    print_check_report, print_skill_check_report, print_skill_install_report, print_update_report,
};
use crate::skills::{check_skill, install_skill, SkillInstallArgs};
use crate::source::{
    current_check_version_or_default, current_install_version_or_default, default_update_target,
    product_label_for_source, resolve_source_config, source_branch_name,
};

pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check(args)) => run_check(args).await?,
        Some(Command::Install(args)) => run_install(args).await?,
        None => run_install(cli.install).await?,
    }
    Ok(())
}

async fn run_check(args: CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.common.artifact_kind() {
        ArtifactKind::Bin => {
            validate_bin_common_args(&args.common)?;
            let source = resolve_source_config(&args.common)?;
            let report = check(CheckRequest {
                product: product_label_for_source(&source),
                source,
                current_version: current_check_version_or_default(&args, "0.0.0"),
                branch: source_branch_name(&args.common),
            })
            .await?;
            print_check_report(args.common.json, &report)?;
        }
        ArtifactKind::Skill => {
            validate_skill_check_args(&args)?;
            let report = check_skill(skill_args_from_common(&args.common)?).await?;
            print_skill_check_report(args.common.json, &report)?;
        }
    }
    Ok(())
}

async fn run_install(args: InstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.common.artifact_kind() {
        ArtifactKind::Bin => {
            validate_bin_common_args(&args.common)?;
            let source = resolve_source_config(&args.common)?;
            let report = update(UpdateRequest {
                product: product_label_for_source(&source),
                target: default_update_target(&source),
                source,
                current_version: current_install_version_or_default(&args, "0.0.0"),
                install_dir: args.install_dir,
                yes: args.yes,
                dry_run: args.dry_run,
                force: args.force,
            })
            .await?;
            print_update_report("install", args.common.json, &report)?;
        }
        ArtifactKind::Skill => {
            validate_skill_install_args(&args)?;
            let report = install_skill(skill_args_from_common(&args.common)?).await?;
            print_skill_install_report(args.common.json, &report)?;
        }
    }
    Ok(())
}

fn validate_bin_common_args(common: &CommonArgs) -> Result<(), Box<dyn std::error::Error>> {
    if common.skill_path.is_some() {
        return Err("--path requires --skill".into());
    }
    Ok(())
}

fn skill_args_from_common(
    common: &CommonArgs,
) -> Result<SkillInstallArgs, Box<dyn std::error::Error>> {
    Ok(SkillInstallArgs {
        github: common
            .github
            .clone()
            .ok_or_else(|| "missing GitHub skill source".to_string())?,
        latest: common.latest,
        tag: common.tag.clone(),
        path: common
            .skill_path
            .clone()
            .ok_or_else(|| "missing skill path".to_string())?,
    })
}

fn validate_skill_common_args(common: &CommonArgs) -> Result<(), Box<dyn std::error::Error>> {
    if common.github.is_none() {
        return Err("--skill requires --github <repo>".into());
    }
    if common.skill_path.is_none() {
        return Err("--skill requires --path <repo-subdir>".into());
    }
    if common.source.is_some() || common.updates_base_url.is_some() || common.updates_root.is_some()
    {
        return Err("--skill cannot be combined with --source, --base-url, or --local-root".into());
    }
    if common.channel.is_some() {
        return Err("--skill does not support --channel".into());
    }
    Ok(())
}

fn validate_skill_check_args(args: &CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
    validate_skill_common_args(&args.common)?;
    if args.current_version.is_some() {
        return Err("--skill cannot be combined with --current-version".into());
    }
    Ok(())
}

fn validate_skill_install_args(args: &InstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    validate_skill_common_args(&args.common)?;
    if args.current_version.is_some() {
        return Err("--skill cannot be combined with --current-version".into());
    }
    if args.yes {
        return Err("--skill cannot be combined with --yes".into());
    }
    if args.install_dir.is_some() {
        return Err("--skill cannot be combined with --dir".into());
    }
    if args.dry_run {
        return Err("--skill cannot be combined with --dry-run".into());
    }
    if args.force {
        return Err("--skill cannot be combined with --force".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_skill_requires_path() {
        let err = validate_skill_install_args(&InstallArgs {
            common: CommonArgs {
                kind: crate::cli::KindArgs {
                    bin: false,
                    skill: true,
                },
                github: Some("wp-labs/wp-skills".to_string()),
                ..CommonArgs::default()
            },
            ..InstallArgs::default()
        })
        .unwrap_err();

        assert!(err.to_string().contains("--path"));
    }

    #[test]
    fn binary_mode_rejects_path_without_skill() {
        let err = validate_bin_common_args(&CommonArgs {
            github: Some("wp-labs/wpl-check".to_string()),
            skill_path: Some("skills/warpparse-log-engineering".to_string()),
            ..CommonArgs::default()
        })
        .unwrap_err();

        assert!(err.to_string().contains("--path requires --skill"));
    }

    #[test]
    fn skill_mode_rejects_binary_install_dir() {
        let err = validate_skill_install_args(&InstallArgs {
            common: CommonArgs {
                kind: crate::cli::KindArgs {
                    bin: false,
                    skill: true,
                },
                github: Some("wp-labs/wp-skills".to_string()),
                skill_path: Some("skills/warpparse-log-engineering".to_string()),
                ..CommonArgs::default()
            },
            install_dir: Some("/tmp/install".into()),
            ..InstallArgs::default()
        })
        .unwrap_err();

        assert!(err.to_string().contains("--dir"));
    }

    #[test]
    fn skill_mode_rejects_yes() {
        let err = validate_skill_install_args(&InstallArgs {
            common: CommonArgs {
                kind: crate::cli::KindArgs {
                    bin: false,
                    skill: true,
                },
                github: Some("wp-labs/wp-skills".to_string()),
                skill_path: Some("skills/warpparse-log-engineering".to_string()),
                ..CommonArgs::default()
            },
            yes: true,
            ..InstallArgs::default()
        })
        .unwrap_err();

        assert!(err.to_string().contains("--yes"));
    }

    #[test]
    fn skill_check_accepts_default_latest() {
        validate_skill_check_args(&CheckArgs {
            common: CommonArgs {
                kind: crate::cli::KindArgs {
                    bin: false,
                    skill: true,
                },
                github: Some("wp-labs/wp-skills".to_string()),
                skill_path: Some("skills/warpparse-log-engineering".to_string()),
                ..CommonArgs::default()
            },
            ..CheckArgs::default()
        })
        .expect("skill mode should default to latest");
    }

    #[test]
    fn skill_mode_rejects_explicit_channel_even_when_stable() {
        let err = validate_skill_check_args(&CheckArgs {
            common: CommonArgs {
                kind: crate::cli::KindArgs {
                    bin: false,
                    skill: true,
                },
                github: Some("wp-labs/wp-skills".to_string()),
                skill_path: Some("skills/warpparse-log-engineering".to_string()),
                channel: Some(crate::cli::Channel::Stable),
                ..CommonArgs::default()
            },
            ..CheckArgs::default()
        })
        .unwrap_err();

        assert!(err.to_string().contains("--channel"));
    }
}

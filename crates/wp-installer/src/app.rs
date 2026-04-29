use orion_error::ErrorWrapAs;
use wp_self_update::{check, update, CheckRequest, UpdateRequest};

use crate::cli::{ArtifactKind, CheckArgs, Cli, Command, CommonArgs, InstallArgs};
use crate::error::{invalid_request, InstallerReason, InstallerResult};
use crate::report::{
    print_check_report, print_skill_check_report, print_skill_install_report, print_update_report,
};
use crate::skills::{check_skill, install_skill, SkillInstallArgs};
use crate::source::{
    current_check_version_or_default, current_install_version_or_default, default_update_target,
    product_label_for_source, resolve_source_config, source_branch_name,
};

pub(crate) async fn run_with_cli(cli: Cli) -> InstallerResult<()> {
    match cli.command {
        Some(Command::Check(args)) => run_check(args).await?,
        Some(Command::Install(args)) => run_install(args).await?,
        None => run_install(cli.install).await?,
    }
    Ok(())
}

async fn run_check(args: CheckArgs) -> InstallerResult<()> {
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
            .await
            .wrap_as(
                InstallerReason::SelfUpdateFailed,
                "failed to check binary update",
            )?;
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

async fn run_install(args: InstallArgs) -> InstallerResult<()> {
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
            .await
            .wrap_as(
                InstallerReason::SelfUpdateFailed,
                "failed to install binary update",
            )?;
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

fn validate_bin_common_args(common: &CommonArgs) -> InstallerResult<()> {
    if common.skill_path.is_some() {
        return Err(invalid_request("--path requires --skill"));
    }
    Ok(())
}

fn skill_args_from_common(common: &CommonArgs) -> InstallerResult<SkillInstallArgs> {
    Ok(SkillInstallArgs {
        github: common
            .github
            .clone()
            .ok_or_else(|| invalid_request("missing GitHub skill source"))?,
        latest: common.latest,
        tag: common.tag.clone(),
        path: common
            .skill_path
            .clone()
            .ok_or_else(|| invalid_request("missing skill path"))?,
    })
}

fn validate_skill_common_args(common: &CommonArgs) -> InstallerResult<()> {
    if common.github.is_none() {
        return Err(invalid_request("--skill requires --github <repo>"));
    }
    if common.skill_path.is_none() {
        return Err(invalid_request("--skill requires --path <repo-subdir>"));
    }
    if common.source.is_some() || common.updates_base_url.is_some() || common.updates_root.is_some()
    {
        return Err(invalid_request(
            "--skill cannot be combined with --source, --base-url, or --local-root",
        ));
    }
    if common.channel.is_some() {
        return Err(invalid_request("--skill does not support --channel"));
    }
    Ok(())
}

fn validate_skill_check_args(args: &CheckArgs) -> InstallerResult<()> {
    validate_skill_common_args(&args.common)?;
    if args.current_version.is_some() {
        return Err(invalid_request(
            "--skill cannot be combined with --current-version",
        ));
    }
    Ok(())
}

fn validate_skill_install_args(args: &InstallArgs) -> InstallerResult<()> {
    validate_skill_common_args(&args.common)?;
    if args.current_version.is_some() {
        return Err(invalid_request(
            "--skill cannot be combined with --current-version",
        ));
    }
    if args.yes {
        return Err(invalid_request("--skill cannot be combined with --yes"));
    }
    if args.install_dir.is_some() {
        return Err(invalid_request("--skill cannot be combined with --dir"));
    }
    if args.dry_run {
        return Err(invalid_request("--skill cannot be combined with --dry-run"));
    }
    if args.force {
        return Err(invalid_request("--skill cannot be combined with --force"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::InstallerReason;
    use orion_error::ErrorIdentityProvider;

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

    #[test]
    fn invalid_request_exposes_stable_identity() {
        let err = validate_bin_common_args(&CommonArgs {
            skill_path: Some("skills/warpparse-log-engineering".to_string()),
            ..CommonArgs::default()
        })
        .unwrap_err();

        assert_eq!(err.reason(), &InstallerReason::InvalidRequest);
        assert_eq!(err.reason().stable_code(), "conf.installer_invalid_request");
    }

    #[tokio::test]
    async fn binary_check_wraps_self_update_error_with_source_chain() {
        let err = run_check(CheckArgs {
            common: CommonArgs {
                github: Some("wp-labs/wpl-check".to_string()),
                ..CommonArgs::default()
            },
            current_version: Some("not-semver".to_string()),
        })
        .await
        .unwrap_err();

        assert_eq!(err.reason(), &InstallerReason::SelfUpdateFailed);
        assert_eq!(
            err.reason().stable_code(),
            "sys.installer_self_update_failed"
        );
        assert!(!err.source_frames().is_empty());
        assert!(err
            .root_cause_frame()
            .map(|frame| {
                frame.is_root_cause
                    && frame
                        .type_name
                        .as_deref()
                        .unwrap_or_default()
                        .contains("wp_self_update::error::UpdateReason")
            })
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn binary_install_wraps_self_update_error_with_source_chain() {
        let err = run_install(InstallArgs {
            common: CommonArgs {
                github: Some("wp-labs/wpl-check".to_string()),
                ..CommonArgs::default()
            },
            current_version: Some("not-semver".to_string()),
            ..InstallArgs::default()
        })
        .await
        .unwrap_err();

        assert_eq!(err.reason(), &InstallerReason::SelfUpdateFailed);
        assert!(!err.source_frames().is_empty());
        assert!(err
            .root_cause_frame()
            .map(|frame| {
                frame.is_root_cause
                    && frame
                        .type_name
                        .as_deref()
                        .unwrap_or_default()
                        .contains("wp_self_update::error::UpdateReason")
            })
            .unwrap_or(false));
    }
}

use clap::Parser;
use wp_self_update::{check, update, CheckRequest, UpdateRequest};

use crate::cli::{ApplyArgs, CheckArgs, Cli, Command, DirectArgs};
use crate::report::{print_check_report, print_update_report};
use crate::skills::{install_skill, SkillInstallArgs};
use crate::source::{
    current_version_or_default, default_update_target, product_label_for_source,
    resolve_source_config, source_branch_name,
};

pub(crate) async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check(args)) => run_check(args).await?,
        Some(Command::Update(args)) => run_apply("update", args).await?,
        Some(Command::Install(args)) => run_apply("install", args).await?,
        None => run_direct(cli.direct).await?,
    }
    Ok(())
}

async fn run_check(args: CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
    let source = resolve_source_config(&args.request.source)?;
    let report = check(CheckRequest {
        product: product_label_for_source(&source),
        source,
        current_version: current_version_or_default(&args.request, "0.0.0"),
        branch: source_branch_name(&args.request.source),
    })
    .await?;
    print_check_report(&args.request.source, &report)?;
    Ok(())
}

async fn run_apply(action: &str, args: ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let source = resolve_source_config(&args.request.source)?;
    let report = update(UpdateRequest {
        product: product_label_for_source(&source),
        target: default_update_target(&source),
        source,
        current_version: current_version_or_default(&args.request, "0.0.0"),
        install_dir: args.install_dir,
        yes: args.yes,
        dry_run: args.dry_run,
        force: args.force,
    })
    .await?;
    print_update_report(action, &args.request.source, &report)?;
    Ok(())
}

async fn run_direct(args: DirectArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.skill {
        validate_direct_skill_args(&args)?;
        return install_skill(SkillInstallArgs {
            github: args
                .source
                .github
                .clone()
                .ok_or_else(|| "missing GitHub skill source".to_string())?,
            latest: args.source.latest,
            tag: args.source.tag.clone(),
            path: args
                .skill_path
                .clone()
                .ok_or_else(|| "missing skill path".to_string())?,
        })
        .await;
    }

    if args.skill_path.is_some() {
        return Err("--path requires --skill".into());
    }

    if args.source.github.is_none() {
        return Err("either provide a subcommand or use --github <repo> for direct install".into());
    }

    let source = resolve_source_config(&args.source)?;
    let report = update(UpdateRequest {
        product: product_label_for_source(&source),
        target: default_update_target(&source),
        source,
        current_version: args
            .current_version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string()),
        install_dir: args.install_dir,
        yes: args.yes,
        dry_run: args.dry_run,
        force: args.force,
    })
    .await?;
    print_update_report("install", &args.source, &report)?;
    Ok(())
}

fn validate_direct_skill_args(args: &DirectArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.source.github.is_none() {
        return Err("--skill requires --github <repo>".into());
    }
    if args.skill_path.is_none() {
        return Err("--skill requires --path <repo-subdir>".into());
    }
    if args.source.updates_base_url.is_some() || args.source.updates_root.is_some() {
        return Err("--skill cannot be combined with --base-url or --local-root".into());
    }
    if args.source.latest && args.source.tag.is_some() {
        return Err("--skill cannot combine --latest with --tag <tag>".into());
    }
    if args.current_version.is_some() {
        return Err("--skill cannot be combined with --current-version".into());
    }
    if args.yes {
        return Err("--skill cannot be combined with --yes".into());
    }
    if args.dry_run {
        return Err("--skill cannot be combined with --dry-run".into());
    }
    if args.force {
        return Err("--skill cannot be combined with --force".into());
    }
    if args.install_dir.is_some() {
        return Err(
            "--skill cannot be combined with --install-dir; skill installs use default skills directories"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SourceArgs;
    use std::path::PathBuf;

    #[test]
    fn direct_skill_rejects_binary_install_flags() {
        let err = validate_direct_skill_args(&DirectArgs {
            source: SourceArgs {
                github: Some("wp-labs/wp-skills".to_string()),
                tag: Some("v0.1.2".to_string()),
                ..SourceArgs::default()
            },
            skill: true,
            skill_path: Some("skills/warpparse-log-engineering".to_string()),
            current_version: None,
            yes: false,
            dry_run: false,
            force: false,
            install_dir: Some(PathBuf::from("/tmp/install")),
        })
        .unwrap_err();

        assert!(err.to_string().contains("--install-dir"));
    }

    #[test]
    fn direct_skill_requires_path() {
        let err = validate_direct_skill_args(&DirectArgs {
            source: SourceArgs {
                github: Some("wp-labs/wp-skills".to_string()),
                tag: Some("v0.1.2".to_string()),
                ..SourceArgs::default()
            },
            skill: true,
            skill_path: None,
            current_version: None,
            yes: false,
            dry_run: false,
            force: false,
            install_dir: None,
        })
        .unwrap_err();

        assert!(err.to_string().contains("--path"));
    }

    #[test]
    fn direct_skill_defaults_to_latest_when_selector_missing() {
        validate_direct_skill_args(&DirectArgs {
            source: SourceArgs {
                github: Some("wp-labs/wp-skills".to_string()),
                ..SourceArgs::default()
            },
            skill: true,
            skill_path: Some("skills/warpparse-log-engineering".to_string()),
            current_version: None,
            yes: false,
            dry_run: false,
            force: false,
            install_dir: None,
        })
        .expect("skill mode should default to latest");
    }

    #[test]
    fn direct_skill_rejects_conflicting_release_selectors() {
        let err = validate_direct_skill_args(&DirectArgs {
            source: SourceArgs {
                github: Some("wp-labs/wp-skills".to_string()),
                latest: true,
                tag: Some("v0.1.2".to_string()),
                ..SourceArgs::default()
            },
            skill: true,
            skill_path: Some("skills/warpparse-log-engineering".to_string()),
            current_version: None,
            yes: false,
            dry_run: false,
            force: false,
            install_dir: None,
        })
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("cannot combine --latest with --tag"));
    }
}

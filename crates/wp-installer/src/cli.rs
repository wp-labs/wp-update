use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "wp-inst", about = "Bootstrap installer for wp-* binaries")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    #[command(flatten)]
    pub(crate) direct: DirectArgs,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Check(CheckArgs),
    Update(ApplyArgs),
    Install(ApplyArgs),
}

#[derive(Args, Debug, Clone)]
pub(crate) struct SourceArgs {
    #[arg(long, value_enum, default_value_t = Channel::Stable)]
    pub(crate) channel: Channel,
    #[arg(
        long = "base-url",
        help = "Override manifest base URL; final path is {channel}/manifest.json"
    )]
    pub(crate) updates_base_url: Option<String>,
    #[arg(
        long = "local-root",
        help = "Override local manifest root; final path is {channel}/manifest.json"
    )]
    pub(crate) updates_root: Option<PathBuf>,
    #[arg(long, help = "GitHub repository URL or <owner>/<repo>")]
    pub(crate) github: Option<String>,
    #[arg(
        long,
        conflicts_with = "tag",
        default_value_t = false,
        help = "Resolve the latest GitHub release"
    )]
    pub(crate) latest: bool,
    #[arg(
        long,
        conflicts_with = "latest",
        help = "Resolve a specific GitHub release tag"
    )]
    pub(crate) tag: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl Default for SourceArgs {
    fn default() -> Self {
        Self {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
            github: None,
            latest: false,
            tag: None,
            json: false,
        }
    }
}

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct DirectArgs {
    #[command(flatten)]
    pub(crate) source: SourceArgs,
    #[arg(
        long = "skill",
        default_value_t = false,
        help = "Install a skill from a GitHub release archive into default Codex/Claude skills directories"
    )]
    pub(crate) skill: bool,
    #[arg(
        long = "path",
        help = "Repository subdirectory for the skill, e.g. skills/warpparse-log-engineering"
    )]
    pub(crate) skill_path: Option<String>,
    #[arg(long = "current-version")]
    pub(crate) current_version: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) yes: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    pub(crate) dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
    #[arg(long = "install-dir")]
    pub(crate) install_dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct RequestArgs {
    #[command(flatten)]
    pub(crate) source: SourceArgs,
    #[arg(long = "current-version")]
    pub(crate) current_version: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct CheckArgs {
    #[command(flatten)]
    pub(crate) request: RequestArgs,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct ApplyArgs {
    #[command(flatten)]
    pub(crate) request: RequestArgs,
    #[arg(long, default_value_t = false)]
    pub(crate) yes: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    pub(crate) dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
    #[arg(long = "install-dir")]
    pub(crate) install_dir: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum Channel {
    #[default]
    Stable,
    Beta,
    Alpha,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_updates_base_url_without_product() {
        let cli = Cli::try_parse_from([
            "wp-inst",
            "check",
            "--base-url",
            "https://example.com/releases/warp-parse",
        ])
        .expect("parse cli");

        match cli.command {
            Some(Command::Check(args)) => {
                assert_eq!(
                    args.request.source.updates_base_url.as_deref(),
                    Some("https://example.com/releases/warp-parse")
                );
            }
            _ => panic!("expected check command"),
        }
    }

    #[test]
    fn cli_accepts_direct_github_latest_install() {
        let cli = Cli::try_parse_from([
            "wp-inst",
            "--github",
            "https://github.com/wp-labs/wpl-check",
        ])
        .expect("parse cli");

        assert!(cli.command.is_none());
        assert_eq!(
            cli.direct.source.github.as_deref(),
            Some("https://github.com/wp-labs/wpl-check")
        );
        assert!(!cli.direct.source.latest);
        assert_eq!(cli.direct.source.tag, None);
    }

    #[test]
    fn cli_accepts_direct_github_tag_install() {
        let cli = Cli::try_parse_from([
            "wp-inst",
            "--github",
            "https://github.com/wp-labs/wpl-check",
            "--tag",
            "v0.1.7",
        ])
        .expect("parse cli");

        assert!(cli.command.is_none());
        assert_eq!(
            cli.direct.source.github.as_deref(),
            Some("https://github.com/wp-labs/wpl-check")
        );
        assert!(!cli.direct.source.latest);
        assert_eq!(cli.direct.source.tag.as_deref(), Some("v0.1.7"));
    }

    #[test]
    fn cli_accepts_direct_skill_install() {
        let cli = Cli::try_parse_from([
            "wp-inst",
            "--github",
            "wp-labs/wp-skills",
            "--path",
            "skills/warpparse-log-engineering",
            "--skill",
        ])
        .expect("parse cli");

        assert!(cli.command.is_none());
        assert_eq!(
            cli.direct.source.github.as_deref(),
            Some("wp-labs/wp-skills")
        );
        assert!(!cli.direct.source.latest);
        assert_eq!(cli.direct.source.tag, None);
        assert_eq!(
            cli.direct.skill_path.as_deref(),
            Some("skills/warpparse-log-engineering")
        );
        assert!(cli.direct.skill);
    }

    #[test]
    fn cli_rejects_removed_skill_subcommand() {
        let err = Cli::try_parse_from(["wp-inst", "skill"]).unwrap_err();
        assert!(err.to_string().contains("unrecognized subcommand"));
    }
}

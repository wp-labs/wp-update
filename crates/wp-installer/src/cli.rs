use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "wp-inst",
    about = "Bootstrap installer for wp-* binaries",
    version
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    #[command(flatten)]
    pub(crate) install: InstallArgs,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Check(CheckArgs),
    #[command(alias = "update")]
    Install(InstallArgs),
}

#[derive(Args, Debug, Clone, Default, Eq, PartialEq)]
pub(crate) struct KindArgs {
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "skill",
        help = "Operate on binary artifacts (default)"
    )]
    pub(crate) bin: bool,
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "bin",
        help = "Operate on skills from a GitHub release archive"
    )]
    pub(crate) skill: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum ArtifactKind {
    Bin,
    Skill,
}

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct CommonArgs {
    #[command(flatten)]
    pub(crate) kind: KindArgs,
    #[arg(long, help = "GitHub repository URL or <owner>/<repo>")]
    pub(crate) github: Option<String>,
    #[arg(long, help = "Manifest source base URL or local root directory")]
    pub(crate) source: Option<String>,
    #[arg(long, help = "Resolve a specific GitHub release tag")]
    pub(crate) tag: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) channel: Option<Channel>,
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    #[arg(
        long = "path",
        help = "Repository subdirectory for the skill, e.g. skills/warpparse-log-engineering"
    )]
    pub(crate) skill_path: Option<String>,
    #[arg(long, hide = true, default_value_t = false, conflicts_with = "tag")]
    pub(crate) latest: bool,
    #[arg(long = "base-url", hide = true)]
    pub(crate) updates_base_url: Option<String>,
    #[arg(long = "local-root", hide = true)]
    pub(crate) updates_root: Option<PathBuf>,
}

impl CommonArgs {
    pub(crate) fn artifact_kind(&self) -> ArtifactKind {
        if self.kind.skill {
            ArtifactKind::Skill
        } else {
            ArtifactKind::Bin
        }
    }

    pub(crate) fn effective_channel(&self) -> Channel {
        self.channel.unwrap_or(Channel::Stable)
    }
}

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct CheckArgs {
    #[command(flatten)]
    pub(crate) common: CommonArgs,
    #[arg(long = "current-version", hide = true)]
    pub(crate) current_version: Option<String>,
}

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct InstallArgs {
    #[command(flatten)]
    pub(crate) common: CommonArgs,
    #[arg(long = "dir", alias = "install-dir")]
    pub(crate) install_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub(crate) yes: bool,
    #[arg(long = "current-version", hide = true)]
    pub(crate) current_version: Option<String>,
    #[arg(long = "dry-run", hide = true, default_value_t = false)]
    pub(crate) dry_run: bool,
    #[arg(long, hide = true, default_value_t = false)]
    pub(crate) force: bool,
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
    fn cli_defaults_to_install_mode_without_command() {
        let cli = Cli::try_parse_from(["wp-inst", "--github", "wp-labs/wpl-check"]).unwrap();

        assert!(cli.command.is_none());
        assert_eq!(
            cli.install.common.github.as_deref(),
            Some("wp-labs/wpl-check")
        );
    }

    #[test]
    fn cli_accepts_check_with_source() {
        let cli =
            Cli::try_parse_from(["wp-inst", "check", "--source", "./updates"]).expect("parse cli");

        match cli.command {
            Some(Command::Check(args)) => {
                assert_eq!(args.common.source.as_deref(), Some("./updates"));
            }
            _ => panic!("expected check command"),
        }
    }

    #[test]
    fn cli_accepts_install_alias_update() {
        let cli = Cli::try_parse_from(["wp-inst", "update", "--github", "wp-labs/wpl-check"])
            .expect("parse cli");

        match cli.command {
            Some(Command::Install(args)) => {
                assert_eq!(args.common.github.as_deref(), Some("wp-labs/wpl-check"));
            }
            _ => panic!("expected install command"),
        }
    }

    #[test]
    fn cli_accepts_skill_mode() {
        let cli = Cli::try_parse_from([
            "wp-inst",
            "check",
            "--skill",
            "--github",
            "wp-labs/wp-skills",
            "--path",
            "skills/warpparse-log-engineering",
        ])
        .expect("parse cli");

        match cli.command {
            Some(Command::Check(args)) => {
                assert!(args.common.kind.skill);
                assert_eq!(
                    args.common.skill_path.as_deref(),
                    Some("skills/warpparse-log-engineering")
                );
            }
            _ => panic!("expected check command"),
        }
    }

    #[test]
    fn cli_accepts_legacy_base_url_flag() {
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
                    args.common.updates_base_url.as_deref(),
                    Some("https://example.com/releases/warp-parse")
                );
            }
            _ => panic!("expected check command"),
        }
    }
}

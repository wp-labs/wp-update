use clap::{Args, Parser, Subcommand, ValueEnum};
use std::env;
use std::path::PathBuf;
use wp_self_update::{
    check, update, CheckReport, CheckRequest, GithubRepo, SourceConfig, SourceKind, UpdateChannel,
    UpdateReport, UpdateRequest, UpdateTarget,
};

const DEFAULT_MANIFEST_BASE_URL_ENV: &str = "WP_INSTALLER_DEFAULT_BASE_URL";
const DEFAULT_MANIFEST_ROOT_ENV: &str = "WP_INSTALLER_DEFAULT_ROOT";
const CUSTOM_PRODUCT_LABEL: &str = "custom";

#[derive(Parser, Debug)]
#[command(name = "wp-inst", about = "Bootstrap installer for wp-* binaries")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    direct: DirectArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    Check(CheckArgs),
    Update(ApplyArgs),
    Install(ApplyArgs),
}

#[derive(Args, Debug, Clone)]
struct SourceArgs {
    #[arg(long, value_enum, default_value_t = Channel::Stable)]
    channel: Channel,
    #[arg(
        long = "base-url",
        help = "Override manifest base URL; final path is {channel}/manifest.json"
    )]
    updates_base_url: Option<String>,
    #[arg(
        long = "local-root",
        help = "Override local manifest root; final path is {channel}/manifest.json"
    )]
    updates_root: Option<PathBuf>,
    #[arg(
        long,
        help = "Use the latest GitHub release from a repository URL or <owner>/<repo>"
    )]
    github: Option<String>,
    #[arg(
        long,
        conflicts_with = "tag",
        default_value_t = false,
        help = "Resolve the latest GitHub release"
    )]
    latest: bool,
    #[arg(
        long,
        conflicts_with = "latest",
        help = "Resolve a specific GitHub release tag"
    )]
    tag: Option<String>,
    #[arg(long, default_value_t = false)]
    json: bool,
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
struct DirectArgs {
    #[command(flatten)]
    source: SourceArgs,
    #[arg(long = "current-version")]
    current_version: Option<String>,
    #[arg(long, default_value_t = false)]
    yes: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,
    #[arg(long, default_value_t = false)]
    force: bool,
    #[arg(long = "install-dir")]
    install_dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct RequestArgs {
    #[command(flatten)]
    source: SourceArgs,
    #[arg(long = "current-version")]
    current_version: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct CheckArgs {
    #[command(flatten)]
    request: RequestArgs,
}

#[derive(Args, Debug, Clone)]
struct ApplyArgs {
    #[command(flatten)]
    request: RequestArgs,
    #[arg(long, default_value_t = false)]
    yes: bool,
    #[arg(long = "dry-run", default_value_t = false)]
    dry_run: bool,
    #[arg(long, default_value_t = false)]
    force: bool,
    #[arg(long = "install-dir")]
    install_dir: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, ValueEnum)]
enum Channel {
    #[default]
    Stable,
    Beta,
    Alpha,
}

#[derive(Debug, Clone)]
struct SourceDefaults {
    updates_base_url: Option<String>,
    updates_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let exit_code = match run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("wp-inst error: {}", err);
            1
        }
    };
    std::process::exit(exit_code);
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
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
    if args.source.github.is_none() && !args.source.latest && args.source.tag.is_none() {
        return Err(
            "either provide a subcommand or use --github <repo> with --latest or --tag <tag> for direct install".into(),
        );
    }
    if args.source.github.is_none() {
        return Err("GitHub release selection requires --github <repo>".into());
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

fn resolve_source_config(source: &SourceArgs) -> Result<SourceConfig, Box<dyn std::error::Error>> {
    if source.github.is_some() {
        if source.updates_base_url.is_some() || source.updates_root.is_some() {
            return Err("--github cannot be combined with --base-url or --local-root".into());
        }
        if source.latest == source.tag.is_some() {
            return Err("--github requires exactly one of --latest or --tag <tag>".into());
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
            kind: if source.latest {
                SourceKind::GithubLatest { repo }
            } else {
                SourceKind::GithubTag {
                    repo,
                    tag: source
                        .tag
                        .clone()
                        .ok_or_else(|| "missing GitHub release tag".to_string())?,
                }
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

fn default_source_overrides() -> SourceDefaults {
    SourceDefaults {
        updates_base_url: env::var(DEFAULT_MANIFEST_BASE_URL_ENV).ok(),
        updates_root: env::var_os(DEFAULT_MANIFEST_ROOT_ENV).map(PathBuf::from),
    }
}

fn current_version_or_default(args: &RequestArgs, default: &str) -> String {
    args.current_version
        .clone()
        .unwrap_or_else(|| default.to_string())
}

fn default_update_target(source: &SourceConfig) -> UpdateTarget {
    match &source.kind {
        SourceKind::Manifest { .. } => UpdateTarget::Auto,
        SourceKind::GithubLatest { repo } | SourceKind::GithubTag { repo, .. } => {
            UpdateTarget::Bins(vec![repo.name.clone()])
        }
    }
}

fn product_label_for_source(source: &SourceConfig) -> String {
    match &source.kind {
        SourceKind::Manifest { .. } => CUSTOM_PRODUCT_LABEL.to_string(),
        SourceKind::GithubLatest { repo } | SourceKind::GithubTag { repo, .. } => repo.name.clone(),
    }
}

fn source_branch_name(source: &SourceArgs) -> String {
    if source.github.is_some() && source.latest {
        return "main".to_string();
    }
    if let Some(tag) = &source.tag {
        return tag.clone();
    }
    "installer".to_string()
}

fn print_check_report(
    source: &SourceArgs,
    report: &CheckReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("{} check", display_product_label(&report.product));
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Target   : {}", report.platform_key);
    println!("  Artifact : {}", report.artifact);
    println!(
        "  Status   : {}",
        if report.update_available {
            "update available"
        } else {
            "up-to-date"
        }
    );
    Ok(())
}

fn print_update_report(
    action: &str,
    source: &SourceArgs,
    report: &UpdateReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("{} {}", display_product_label(&report.product), action);
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Install  : {}", report.install_dir);
    println!("  Artifact : {}", report.artifact);
    println!("  Status   : {}", report.status);
    Ok(())
}

fn display_product_label(product: &str) -> &str {
    if product == CUSTOM_PRODUCT_LABEL {
        "wp-inst"
    } else {
        product
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
            "--latest",
        ])
        .expect("parse cli");

        assert!(cli.command.is_none());
        assert_eq!(
            cli.direct.source.github.as_deref(),
            Some("https://github.com/wp-labs/wpl-check")
        );
        assert!(cli.direct.source.latest);
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
            .contains("requires exactly one of --latest or --tag"));
    }

    #[test]
    fn display_product_label_uses_github_repo_name() {
        assert_eq!(display_product_label("wpl-check"), "wpl-check");
    }

    #[test]
    fn display_product_label_falls_back_to_wp_inst_for_manifest_mode() {
        assert_eq!(display_product_label(CUSTOM_PRODUCT_LABEL), "wp-inst");
    }
}

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::env;
use std::path::PathBuf;
use wp_self_update::{
    check, update, CheckReport, CheckRequest, SourceConfig, UpdateChannel, UpdateReport,
    UpdateRequest, UpdateTarget,
};

const DEFAULT_MANIFEST_BASE_URL_ENV: &str = "WP_INSTALLER_DEFAULT_BASE_URL";
const DEFAULT_MANIFEST_ROOT_ENV: &str = "WP_INSTALLER_DEFAULT_ROOT";
const CUSTOM_PRODUCT_LABEL: &str = "custom";

#[derive(Parser, Debug)]
#[command(name = "wp-inst", about = "Bootstrap installer for wp-* binaries")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    #[arg(long, default_value_t = false)]
    json: bool,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Channel {
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
        Command::Check(args) => run_check(args).await?,
        Command::Update(args) => run_apply("update", args).await?,
        Command::Install(args) => run_apply("install", args).await?,
    }
    Ok(())
}

async fn run_check(args: CheckArgs) -> Result<(), Box<dyn std::error::Error>> {
    let report = check(CheckRequest {
        product: CUSTOM_PRODUCT_LABEL.to_string(),
        source: resolve_source_config(&args.request.source)?,
        current_version: current_version_or_default(&args.request, "0.0.0"),
        branch: "installer".to_string(),
    })
    .await?;
    print_check_report(&args.request.source, &report)?;
    Ok(())
}

async fn run_apply(action: &str, args: ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let report = update(UpdateRequest {
        product: CUSTOM_PRODUCT_LABEL.to_string(),
        target: UpdateTarget::Auto,
        source: resolve_source_config(&args.request.source)?,
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

fn resolve_source_config(source: &SourceArgs) -> Result<SourceConfig, Box<dyn std::error::Error>> {
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
        updates_base_url: updates_base_url.unwrap_or_default(),
        updates_root,
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

fn print_check_report(
    source: &SourceArgs,
    report: &CheckReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("wp-inst check");
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
    println!("wp-inst {}", action);
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Install  : {}", report.install_dir);
    println!("  Artifact : {}", report.artifact);
    println!("  Status   : {}", report.status);
    Ok(())
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
            json: false,
        })
        .unwrap();

        assert_eq!(source.channel, UpdateChannel::Beta);
        assert_eq!(
            source.updates_base_url,
            "https://example.com/releases/warp-parse"
        );
        assert_eq!(source.updates_root, None);
    }

    #[test]
    fn resolve_source_config_rejects_missing_manifest_source() {
        let err = resolve_source_config(&SourceArgs {
            channel: Channel::Stable,
            updates_base_url: None,
            updates_root: None,
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
            Command::Check(args) => {
                assert_eq!(
                    args.request.source.updates_base_url.as_deref(),
                    Some("https://example.com/releases/warp-parse")
                );
            }
            _ => panic!("expected check command"),
        }
    }
}

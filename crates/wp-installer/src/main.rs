use clap::{Args, Parser, Subcommand, ValueEnum};
use std::env;
use std::path::PathBuf;
use wp_self_update::{
    CheckReport, CheckRequest, SourceConfig, UpdateChannel, UpdateProduct, UpdateReport,
    UpdateRequest, check, update,
};

const DEFAULT_MANIFEST_BASE_URL_ENV: &str = "WP_INSTALLER_DEFAULT_BASE_URL";
const DEFAULT_MANIFEST_ROOT_ENV: &str = "WP_INSTALLER_DEFAULT_ROOT";

#[derive(Parser, Debug)]
#[command(name = "wp-installer", about = "Bootstrap installer for wp-* binaries")]
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
        long = "updates-base-url",
        help = "Override manifest base URL; final path is {channel}/manifest.json"
    )]
    updates_base_url: Option<String>,
    #[arg(
        long = "updates-root",
        help = "Override local manifest root; final path is {channel}/manifest.json"
    )]
    updates_root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct ProductArgs {
    #[arg(long, value_enum)]
    product: Product,
    #[command(flatten)]
    source: SourceArgs,
    #[arg(long = "current-version")]
    current_version: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct CheckArgs {
    #[command(flatten)]
    product: ProductArgs,
}

#[derive(Args, Debug, Clone)]
struct ApplyArgs {
    #[command(flatten)]
    product: ProductArgs,
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

#[tokio::main]
async fn main() {
    let exit_code = match run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("wp-installer error: {}", err);
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
    let product = args.product.product;
    let report = check(CheckRequest {
        product: product.into(),
        source: resolve_source_config(product, &args.product.source)?,
        current_version: current_version_or_default(&args.product, "0.0.0"),
        branch: "installer".to_string(),
    })
    .await?;
    print_check_report(&product, &args.product.source, &report)?;
    Ok(())
}

async fn run_apply(action: &str, args: ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let product = args.product.product;
    let report = update(UpdateRequest {
        product: product.into(),
        source: resolve_source_config(product, &args.product.source)?,
        current_version: current_version_or_default(&args.product, "0.0.0"),
        install_dir: args.install_dir,
        yes: args.yes,
        dry_run: args.dry_run,
        force: args.force,
    })
    .await?;
    print_update_report(action, &product, &args.product.source, &report)?;
    Ok(())
}

fn resolve_source_config(
    product: Product,
    source: &SourceArgs,
) -> Result<SourceConfig, Box<dyn std::error::Error>> {
    let defaults = default_source_overrides(product);
    let updates_root = source.updates_root.clone().or(defaults.updates_root);
    let updates_base_url = source
        .updates_base_url
        .clone()
        .or(defaults.updates_base_url);

    if updates_root.is_none() && updates_base_url.is_none() {
        return Err(format!(
            "manifest source is required for product '{}': provide --updates-base-url, --updates-root, or set {} / {}",
            product_name(product),
            product_base_url_env(product),
            product_root_env(product)
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

#[derive(Debug, Clone)]
struct SourceDefaults {
    updates_base_url: Option<String>,
    updates_root: Option<PathBuf>,
}

fn default_source_overrides(product: Product) -> SourceDefaults {
    SourceDefaults {
        updates_base_url: env::var(product_base_url_env(product))
            .or_else(|_| env::var(DEFAULT_MANIFEST_BASE_URL_ENV))
            .ok(),
        updates_root: env::var_os(product_root_env(product))
            .map(PathBuf::from)
            .or_else(|| env::var_os(DEFAULT_MANIFEST_ROOT_ENV).map(PathBuf::from)),
    }
}

fn product_base_url_env(product: Product) -> &'static str {
    match product {
        Product::Suite => "WP_INSTALLER_SUITE_BASE_URL",
        Product::Wparse => "WP_INSTALLER_WPARSE_BASE_URL",
        Product::Wpgen => "WP_INSTALLER_WPGEN_BASE_URL",
        Product::Wprescue => "WP_INSTALLER_WPRESCUE_BASE_URL",
        Product::Wproj => "WP_INSTALLER_WPROJ_BASE_URL",
    }
}

fn product_root_env(product: Product) -> &'static str {
    match product {
        Product::Suite => "WP_INSTALLER_SUITE_ROOT",
        Product::Wparse => "WP_INSTALLER_WPARSE_ROOT",
        Product::Wpgen => "WP_INSTALLER_WPGEN_ROOT",
        Product::Wprescue => "WP_INSTALLER_WPRESCUE_ROOT",
        Product::Wproj => "WP_INSTALLER_WPROJ_ROOT",
    }
}

fn current_version_or_default(args: &ProductArgs, default: &str) -> String {
    args.current_version
        .clone()
        .unwrap_or_else(|| default.to_string())
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum Product {
    Suite,
    Wparse,
    Wpgen,
    Wprescue,
    Wproj,
}

impl From<Product> for UpdateProduct {
    fn from(value: Product) -> Self {
        match value {
            Product::Suite => UpdateProduct::Suite,
            Product::Wparse => UpdateProduct::Wparse,
            Product::Wpgen => UpdateProduct::Wpgen,
            Product::Wprescue => UpdateProduct::Wprescue,
            Product::Wproj => UpdateProduct::Wproj,
        }
    }
}

fn print_check_report(
    product: &Product,
    source: &SourceArgs,
    report: &CheckReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("wp-installer check");
    println!("  Product  : {}", product_name(*product));
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Target   : {}", report.platform_key);
    println!("  Artifact : {}", report.artifact);
    println!("  Status   : {}", if report.update_available { "update available" } else { "up-to-date" });
    Ok(())
}

fn print_update_report(
    action: &str,
    product: &Product,
    source: &SourceArgs,
    report: &UpdateReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if source.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("wp-installer {}", action);
    println!("  Product  : {}", product_name(*product));
    println!("  Channel  : {}", report.channel);
    println!("  Current  : {}", report.current_version);
    println!("  Latest   : {}", report.latest_version);
    println!("  Install  : {}", report.install_dir);
    println!("  Artifact : {}", report.artifact);
    println!("  Status   : {}", report.status);
    Ok(())
}

fn product_name(product: Product) -> &'static str {
    match product {
        Product::Suite => "suite",
        Product::Wparse => "wparse",
        Product::Wpgen => "wpgen",
        Product::Wprescue => "wprescue",
        Product::Wproj => "wproj",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_source_config_builds_channel_relative_manifest_root() {
        let source = resolve_source_config(
            Product::Wparse,
            &SourceArgs {
                channel: Channel::Beta,
                updates_base_url: Some("https://example.com/releases/wparse".to_string()),
                updates_root: None,
                json: false,
            },
        )
        .unwrap();

        assert_eq!(source.channel, UpdateChannel::Beta);
        assert_eq!(source.updates_base_url, "https://example.com/releases/wparse");
        assert_eq!(source.updates_root, None);
    }

    #[test]
    fn resolve_source_config_rejects_missing_manifest_source() {
        let err = resolve_source_config(
            Product::Suite,
            &SourceArgs {
                channel: Channel::Stable,
                updates_base_url: None,
                updates_root: None,
                json: false,
            },
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("manifest source is required for product 'suite'"));
    }

    #[test]
    fn product_specific_env_keys_are_stable() {
        assert_eq!(product_base_url_env(Product::Wparse), "WP_INSTALLER_WPARSE_BASE_URL");
        assert_eq!(product_root_env(Product::Wparse), "WP_INSTALLER_WPARSE_ROOT");
        assert_eq!(product_base_url_env(Product::Suite), "WP_INSTALLER_SUITE_BASE_URL");
        assert_eq!(product_root_env(Product::Suite), "WP_INSTALLER_SUITE_ROOT");
    }
}

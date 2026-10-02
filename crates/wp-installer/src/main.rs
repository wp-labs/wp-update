mod app;
mod cli;
mod error;
mod report;
mod skills;
mod source;

use clap::Parser;
use cli::{Cli, Command};
use error::InstallerError;
use orion_error::protocol::DefaultExposurePolicy;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let json_errors = cli_wants_json(&cli);

    let exit_code = match app::run_with_cli(cli).await {
        Ok(()) => 0,
        Err(err) => {
            if json_errors {
                match err.exposure(&DefaultExposurePolicy).to_cli_error_json() {
                    Ok(value) => eprintln!("{}", value),
                    Err(_) => eprintln!("wp-inst error\n{}", render_with_causes(&err)),
                }
            } else {
                eprintln!("wp-inst error\n{}", render_with_causes(&err));
            }
            1
        }
    };
    std::process::exit(exit_code);
}

/// 渲染错误，并附上底层原因链（`render()` 只输出 reason/detail，不含 source）。
fn render_with_causes(err: &InstallerError) -> String {
    let mut out = err.render();
    let frames = err.source_frames();
    if frames.is_empty() {
        return out;
    }
    out.push_str("\ncaused by:");
    for frame in frames {
        let reason = frame
            .reason
            .as_ref()
            .map(|s| s.to_string())
            .or_else(|| frame.display.as_ref().map(|s| s.to_string()))
            .unwrap_or_else(|| frame.message.to_string());
        out.push_str("\n  - ");
        out.push_str(reason.trim());
        if let Some(detail) = frame.detail.as_ref().map(|d| d.to_string()) {
            if !detail.trim().is_empty() {
                out.push_str(": ");
                out.push_str(detail.trim());
            }
        }
    }
    out
}

fn cli_wants_json(cli: &Cli) -> bool {
    match &cli.command {
        Some(Command::Check(args)) => args.common.json,
        Some(Command::Install(args)) => args.common.json,
        None => cli.install.common.json,
    }
}

mod app;
mod cli;
mod error;
mod report;
mod skills;
mod source;

use clap::Parser;
use cli::{Cli, Command};
use orion_error::DefaultExposurePolicy;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let json_errors = cli_wants_json(&cli);

    let exit_code = match app::run_with_cli(cli).await {
        Ok(()) => 0,
        Err(err) => {
            if json_errors {
                match err
                    .exposure_snapshot(&DefaultExposurePolicy)
                    .to_cli_error_json()
                {
                    Ok(value) => eprintln!("{}", value),
                    Err(_) => eprintln!("wp-inst error\n{}", err.render()),
                }
            } else {
                eprintln!("wp-inst error\n{}", err.render());
            }
            1
        }
    };
    std::process::exit(exit_code);
}

fn cli_wants_json(cli: &Cli) -> bool {
    match &cli.command {
        Some(Command::Check(args)) => args.common.json,
        Some(Command::Install(args)) => args.common.json,
        None => cli.install.common.json,
    }
}

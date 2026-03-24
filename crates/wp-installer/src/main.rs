mod app;
mod cli;
mod report;
mod skills;
mod source;

#[tokio::main]
async fn main() {
    let exit_code = match app::run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("wp-inst error: {}", err);
            1
        }
    };
    std::process::exit(exit_code);
}

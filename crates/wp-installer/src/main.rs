mod app;
mod cli;
mod error;
mod report;
mod skills;
mod source;

#[tokio::main]
async fn main() {
    let exit_code = match app::run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("wp-inst error\n{}", err.render());
            1
        }
    };
    std::process::exit(exit_code);
}

use clap::Parser;
use eska::cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.run().await {
        Ok(result) => print!("{}", result),
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }
}

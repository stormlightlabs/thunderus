use clap::Parser;
use thndrs::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(err) = thndrs::run(&cli) {
        eprintln!("thndrs: {err}");
        std::process::exit(1);
    }
}

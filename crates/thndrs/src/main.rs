use thndrs_lib::cli::Cli;

fn main() {
    let cli = match Cli::parse_configured() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("thndrs: {err}");
            std::process::exit(2);
        }
    };
    if let Err(err) = thndrs_lib::run(&cli) {
        eprintln!("thndrs: {err}");
        std::process::exit(thndrs_lib::exit_code(&err));
    }
}

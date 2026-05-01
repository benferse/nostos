use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Install miette's fancy report handler for config diagnostics
    miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().build())
    }))
    .ok();

    let cli = nostos::cli::Cli::parse();
    match nostos::cli::run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

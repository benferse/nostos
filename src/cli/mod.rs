mod apply;
mod plan;
mod status;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

/// nostos — Welcome home, hero
#[derive(Parser)]
#[command(name = "nostos", about = "Welcome home, hero", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// A top-level subcommand.
#[derive(Subcommand)]
pub enum Command {
    /// Show platform info and config validity
    Status,
    /// Dry-run: show what apply would do
    Plan,
    /// Apply dotfiles from the repo to this machine
    Apply,
}

/// Run the CLI. Returns an ExitCode.
pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Status => status::run(),
        Command::Plan => plan::run(),
        Command::Apply => apply::run(),
    }
}

mod apply;
mod plan;
mod status;
mod track;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

/// nostos — Welcome home, hero
#[derive(Parser)]
#[command(name = "nostos", about = "Welcome home, hero", version)]
pub struct Cli {
    /// Path to the nostos repo (overrides stored path and current directory)
    #[arg(long, global = true)]
    pub repo: Option<std::path::PathBuf>,

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
    /// Track a target file back into the repo
    Track {
        /// Path to the target file to track back into the repo
        path: std::path::PathBuf,
    },
}

/// Run the CLI. Returns an ExitCode.
pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let repo = cli.repo.as_deref();
    match cli.command {
        Command::Status => status::run(repo),
        Command::Plan => plan::run(repo),
        Command::Apply => apply::run(repo),
        Command::Track { path } => track::run(repo, path),
    }
}

mod apply;
mod init;
mod plan;
mod status;

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
    /// Clone a dotfiles repo and set up this machine
    Init {
        /// Git URL of the dotfiles repository to clone
        url: String,
        /// Machine identity (defaults to hostname)
        #[arg(long)]
        machine: Option<String>,
        /// Apply dotfiles after cloning
        #[arg(long)]
        apply: bool,
    },
    /// Show platform info and config validity
    Status,
    /// Dry-run: show what apply would do
    Plan,
    /// Apply dotfiles from the repo to this machine
    Apply,
}

/// Run the CLI. Returns an ExitCode.
pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let repo = cli.repo.as_deref();
    match cli.command {
        Command::Init { url, machine, apply } => init::run(url, machine, apply),
        Command::Status => status::run(repo),
        Command::Plan => plan::run(repo),
        Command::Apply => apply::run(repo),
    }
}

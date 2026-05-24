mod apply;
mod init;
mod plan;
mod report;
mod status;
mod sync;
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
        /// Destination path for the cloned dotfiles repo
        #[arg(long)]
        dest: Option<std::path::PathBuf>,
    },
    /// Show platform info and config validity
    Status,
    /// Dry-run: show what apply would do
    Plan,
    /// Apply dotfiles from the repo to this machine
    Apply,
    /// Track a target file or directory back into the repo
    Track {
        /// Path to the target file or directory to track back into the repo
        path: std::path::PathBuf,
        /// Remove orphaned source files (only meaningful for directory track)
        #[arg(long)]
        prune: bool,
    },
    /// Sync the local dotfiles repo with the remote
    Sync {
        /// Apply dotfiles after pulling
        #[arg(long)]
        apply: bool,
    },
}

/// Run the CLI. Returns an ExitCode.
pub fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let repo = cli.repo.as_deref();
    match cli.command {
        Command::Init {
            url,
            machine,
            apply,
            dest,
        } => init::run(url, machine, apply, dest),
        Command::Status => status::run(repo),
        Command::Plan => plan::run(repo),
        Command::Apply => apply::run(repo),
        Command::Track { path, prune } => track::run(repo, path, prune),
        Command::Sync { apply } => sync::run(repo, apply),
    }
}

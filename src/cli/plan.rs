use crate::config;
use crate::reconcile::dotfiles::{self, DotfileAction};
use crate::state::State;
use std::process::ExitCode;

pub fn run() -> anyhow::Result<ExitCode> {
    let (_path, cfg) = match config::find_config() {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(3));
        }
    };

    let dotfiles_config = match cfg.dotfiles {
        Some(dc) => dc,
        None => {
            eprintln!("Error: missing required [dotfiles] section in nostos.toml");
            return Ok(ExitCode::from(3));
        }
    };

    let state = State::load().unwrap_or_default();
    let repo_root = std::env::current_dir()?;

    let report = match dotfiles::plan(&dotfiles_config, &state, &repo_root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::from(3));
        }
    };

    if report.actions.is_empty() && report.errors.is_empty() {
        println!("Dotfiles: nothing to do (source directory is empty)");
        return Ok(ExitCode::SUCCESS);
    }

    println!("Dotfiles:");
    for action in &report.actions {
        match action {
            DotfileAction::NewFile { target, .. } => {
                println!("  {} — new file", display_target(target));
            }
            DotfileAction::UpToDate { target } => {
                println!("  {} — up to date", display_target(target));
            }
            DotfileAction::CleanUpdate { target, .. } => {
                println!("  {} — clean update (repo changed)", display_target(target));
            }
            DotfileAction::LocalModification { target } => {
                println!(
                    "  {} — local modification (not in repo)",
                    display_target(target)
                );
            }
            DotfileAction::Conflict { target, .. } => {
                println!(
                    "  {} — conflict (both sides changed, will back up)",
                    display_target(target)
                );
            }
        }
    }

    for err in &report.errors {
        eprintln!("  ⚠ {err}");
    }

    Ok(ExitCode::SUCCESS)
}

fn display_target(path: &std::path::Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

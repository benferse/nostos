use crate::config;
use crate::reconcile::dotfiles::{self, DotfileAction};
use crate::state::State;
use std::process::ExitCode;

pub fn run() -> anyhow::Result<ExitCode> {
    let (_path, cfg) = config::find_config().map_err(|e| {
        eprintln!("{e}");
        std::process::exit(3);
    })?;

    let dotfiles_config = match cfg.dotfiles {
        Some(dc) => dc,
        None => {
            eprintln!("Error: missing required [dotfiles] section in nostos.toml");
            return Ok(ExitCode::from(3));
        }
    };

    let mut state = State::load().unwrap_or_default();
    let repo_root = std::env::current_dir()?;

    let report = dotfiles::apply(&dotfiles_config, &mut state, &repo_root).map_err(|e| {
        eprintln!("Error: {e}");
        std::process::exit(3);
    })?;

    for action in &report.actions {
        match action {
            DotfileAction::NewFile { target, .. } => {
                println!("  ✓ Copied → {}", target.display());
            }
            DotfileAction::UpToDate { target } => {
                println!("  ✓ {} — up to date", display_target(target));
            }
            DotfileAction::CleanUpdate { target, .. } => {
                println!("  ✓ Updated → {}", target.display());
            }
            DotfileAction::LocalModification { target } => {
                println!(
                    "  ⚠ {} — local modification, skipped",
                    display_target(target)
                );
            }
            DotfileAction::Conflict { target, backup, .. } => {
                println!(
                    "  ⚠ Backed up {} → {}",
                    display_target(target),
                    backup.display()
                );
                println!("  ✓ Updated → {}", target.display());
            }
        }
    }

    for err in &report.errors {
        eprintln!("  ✗ {err}");
    }

    // Save updated state
    if let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    // Exit code 5 if there were warnings (local mods, conflicts)
    if report.has_warnings() || !report.errors.is_empty() {
        Ok(ExitCode::from(5))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn display_target(path: &std::path::Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

use crate::config;
use crate::reconcile::{self, dotfiles::DotfileAction};
use crate::state::State;
use std::path::Path;
use std::process::ExitCode;

pub fn run(repo: Option<&Path>) -> anyhow::Result<ExitCode> {
    let (config_path, cfg) = match config::find_config_with_repo(repo) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(3));
        }
    };

    let mut state = State::load().unwrap_or_default();
    state.ensure_machine_identity();
    let repo_root = config::repo_root_from_config(&config_path);

    let report = match reconcile::apply(&cfg, &mut state, &repo_root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::from(3));
        }
    };

    // Print dotfiles results
    if let Some(ref dotfiles_report) = report.dotfiles {
        print_apply_actions(&dotfiles_report.actions);
        for err in &dotfiles_report.errors {
            eprintln!("  ✗ {err}");
        }
    }

    // Print files results
    if let Some(ref files_report) = report.files {
        print_apply_actions(&files_report.actions);
        for err in &files_report.errors {
            eprintln!("  ✗ {err}");
        }
    }

    // Print pending phase warnings
    for phase in &report.pending_phases {
        if let Some(ref reason) = phase.skipped_reason {
            println!("⚠ {reason}");
        }
    }

    // Store repo path in state if not already set
    if state.repo_path().is_none() {
        state.set_repo_path(repo_root.to_string_lossy().to_string());
    }

    // Save updated state
    if let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    // Exit code 5 if there were warnings (local mods, conflicts)
    if report.has_warnings() || report.has_errors() {
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

fn print_apply_actions(actions: &[DotfileAction]) {
    for action in actions {
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
}


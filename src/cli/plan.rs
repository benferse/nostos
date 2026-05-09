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

    let state = State::load().unwrap_or_default();
    let repo_root = config::repo_root_from_config(&config_path);

    let report = match reconcile::plan(&cfg, &state, &repo_root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::from(3));
        }
    };

    // Print dotfiles results
    if let Some(ref dotfiles_report) = report.dotfiles {
        if dotfiles_report.actions.is_empty() && dotfiles_report.errors.is_empty() {
            println!("Dotfiles: nothing to do (source directory is empty)");
        } else {
            println!("Dotfiles:");
            print_actions(&dotfiles_report.actions);
            for err in &dotfiles_report.errors {
                eprintln!("  ⚠ {err}");
            }
        }
    }

    // Print files results
    if let Some(ref files_report) = report.files {
        if files_report.actions.is_empty() && files_report.errors.is_empty() {
            println!("Files: nothing to do (source directory is empty)");
        } else {
            println!("Files:");
            print_actions(&files_report.actions);
            for err in &files_report.errors {
                eprintln!("  ⚠ {err}");
            }
        }
    }

    // Print pending phase warnings
    for phase in &report.pending_phases {
        if let Some(ref reason) = phase.skipped_reason {
            println!("⚠ {reason}");
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn display_target(path: &std::path::Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn print_actions(actions: &[DotfileAction]) {
    for action in actions {
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
}


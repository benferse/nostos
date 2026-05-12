use crate::reconcile::dotfiles::DotfileAction;
use crate::reconcile::ApplyReport;

/// Format a target path for display, showing just the filename.
pub fn display_target(path: &std::path::Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Print plan-mode actions for a single section (dotfiles or files).
pub fn print_plan_actions(actions: &[DotfileAction]) {
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

/// Print apply-mode actions for a single section (dotfiles or files).
pub fn print_apply_actions(actions: &[DotfileAction]) {
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

/// Print a section report (dotfiles or files) in plan mode.
pub fn print_plan_section(label: &str, report: &crate::reconcile::dotfiles::Report) {
    if report.actions.is_empty() && report.errors.is_empty() {
        println!("{label}: nothing to do (source directory is empty)");
    } else {
        println!("{label}:");
        print_plan_actions(&report.actions);
        for err in &report.errors {
            eprintln!("  ⚠ {err}");
        }
    }
}

/// Print a section report (dotfiles or files) in apply mode.
pub fn print_apply_section(label: &str, report: &crate::reconcile::dotfiles::Report) {
    println!("{label}:");
    print_apply_actions(&report.actions);
    for err in &report.errors {
        eprintln!("  ✗ {err}");
    }
}

/// Print pending phase warnings from an apply report.
pub fn print_pending_phases(report: &ApplyReport) {
    for phase in &report.pending_phases {
        if let Some(ref reason) = phase.skipped_reason {
            println!("⚠ {reason}");
        }
    }
}

use crate::reconcile::dotfiles::DotfileAction;
use crate::reconcile::ApplyReport;
use std::path::{Path, PathBuf};

/// Format a path for display.
///
/// If the path is under the user's home directory, the home prefix is
/// replaced with `~` and the remainder is rendered with forward slashes
/// (e.g. `/home/ben/.config/git/config` → `~/.config/git/config`). If the
/// path is exactly the home directory, returns `~`. Otherwise returns the
/// absolute path verbatim.
pub fn display_path(path: &Path) -> String {
    display_path_with_home(path, home_dir().as_deref())
}

fn display_path_with_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && !home.as_os_str().is_empty()
    {
        if path == home {
            return "~".to_string();
        }
        if let Ok(rel) = path.strip_prefix(home) {
            let rel_str = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if rel_str.is_empty() {
                return "~".to_string();
            }
            return format!("~/{rel_str}");
        }
    }
    path.display().to_string()
}

/// Resolve the user's home directory the same way the rest of the codebase
/// does: `HOME` env first (so tests can override it cross-platform), then
/// `dirs::home_dir()` as a fallback.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(dirs::home_dir)
}

/// Print plan-mode actions for a single section (dotfiles or files).
pub fn print_plan_actions(actions: &[DotfileAction]) {
    for action in actions {
        match action {
            DotfileAction::NewFile { target, .. } => {
                println!("  {} — new file", display_path(target));
            }
            DotfileAction::UpToDate { target } => {
                println!("  {} — up to date", display_path(target));
            }
            DotfileAction::CleanUpdate { target, .. } => {
                println!("  {} — clean update (repo changed)", display_path(target));
            }
            DotfileAction::LocalModification { target } => {
                println!(
                    "  {} — local modification (not in repo)",
                    display_path(target)
                );
            }
            DotfileAction::Conflict { target, .. } => {
                println!(
                    "  {} — conflict (both sides changed, will back up)",
                    display_path(target)
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
                println!("  ✓ Copied → {}", display_path(target));
            }
            DotfileAction::UpToDate { target } => {
                println!("  ✓ {} — up to date", display_path(target));
            }
            DotfileAction::CleanUpdate { target, .. } => {
                println!("  ✓ Updated → {}", display_path(target));
            }
            DotfileAction::LocalModification { target } => {
                println!(
                    "  ⚠ {} — local modification, skipped",
                    display_path(target)
                );
            }
            DotfileAction::Conflict { target, backup, .. } => {
                println!(
                    "  ⚠ Backed up {} → {}",
                    display_path(target),
                    display_path(backup)
                );
                println!("  ✓ Updated → {}", display_path(target));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_home_substitutes_tilde() {
        let home = PathBuf::from("/home/ben");
        let path = PathBuf::from("/home/ben/.config/git/config");
        assert_eq!(
            display_path_with_home(&path, Some(&home)),
            "~/.config/git/config"
        );
    }

    #[test]
    fn target_equal_to_home_renders_as_tilde() {
        let home = PathBuf::from("/home/ben");
        assert_eq!(display_path_with_home(&home, Some(&home)), "~");
    }

    #[test]
    fn outside_home_returns_absolute_path() {
        let home = PathBuf::from("/home/ben");
        let path = PathBuf::from("/etc/hosts");
        assert_eq!(display_path_with_home(&path, Some(&home)), "/etc/hosts");
    }

    #[test]
    fn home_unset_returns_absolute_path() {
        let path = PathBuf::from("/home/ben/.bashrc");
        assert_eq!(
            display_path_with_home(&path, None),
            "/home/ben/.bashrc"
        );
    }

    #[test]
    fn empty_home_treated_as_unset() {
        let home = PathBuf::from("");
        let path = PathBuf::from("/home/ben/.bashrc");
        assert_eq!(
            display_path_with_home(&path, Some(&home)),
            "/home/ben/.bashrc"
        );
    }

    /// Path-component prefix safety: `/home/benny/...` must NOT match
    /// `HOME=/home/ben`. Using `Path::strip_prefix` (component-based) instead
    /// of string prefix matching is what makes this safe.
    #[test]
    fn sibling_directory_with_shared_string_prefix_not_substituted() {
        let home = PathBuf::from("/home/ben");
        let path = PathBuf::from("/home/benny/.bashrc");
        assert_eq!(
            display_path_with_home(&path, Some(&home)),
            "/home/benny/.bashrc"
        );
    }

    #[test]
    fn nested_path_renders_with_forward_slashes() {
        let home = PathBuf::from("/Users/ben");
        let path = PathBuf::from("/Users/ben/.config/nvim/init.lua");
        assert_eq!(
            display_path_with_home(&path, Some(&home)),
            "~/.config/nvim/init.lua"
        );
    }

    /// On Windows, paths use backslash components but we render with
    /// forward slashes for cross-platform consistency (matches the
    /// forward-slash logical key invariant in CONTEXT.md).
    #[cfg(windows)]
    #[test]
    fn windows_path_under_userprofile_uses_forward_slashes() {
        let home = PathBuf::from(r"C:\Users\ben");
        let path = PathBuf::from(r"C:\Users\ben\.config\git\config");
        assert_eq!(
            display_path_with_home(&path, Some(&home)),
            "~/.config/git/config"
        );
    }
}

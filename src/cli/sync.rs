use crate::config;
use crate::git;
use crate::reconcile;
use crate::state::State;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn run(repo: Option<&Path>, apply: bool) -> anyhow::Result<ExitCode> {
    let repo_path = resolve_repo_path(repo)?;

    // Bail early if a rebase/merge/cherry-pick is in progress
    if let Err(e) = git::check_in_progress(&repo_path) {
        eprintln!("Error: {e}");
        eprintln!("Re-run `nostos sync` once the operation is complete.");
        return Ok(ExitCode::FAILURE);
    }

    // Step 1 — Auto-commit local changes
    let mut state = State::load().unwrap_or_default();
    state.ensure_machine_identity();
    let machine_id = state.machine_id().unwrap_or("unknown");
    let changed_paths = match git::changed_paths(&repo_path) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };
    let commit_message = build_auto_sync_commit_message(machine_id, &changed_paths);

    match git::commit(&repo_path, &commit_message) {
        Ok(git::CommitOutcome::Committed) => {
            println!("Committed local changes");
        }
        Ok(git::CommitOutcome::NothingToCommit) => {}
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    }

    // Step 2 — Pull
    if let Err(e) = git::pull(&repo_path) {
        eprintln!("Error: {e}");
        return Ok(ExitCode::FAILURE);
    }
    println!("Pulled from remote");

    // Step 3 — Push
    if let Err(e) = git::push(&repo_path) {
        eprintln!("Error: {e}");
        return Ok(ExitCode::FAILURE);
    }
    println!("Pushed to remote");

    // Step 4 — Optional apply
    if apply {
        run_apply(&repo_path, &mut state)?;
    } else {
        println!("Run `nostos plan` to see what changed, then `nostos apply`");
    }

    Ok(ExitCode::SUCCESS)
}

fn resolve_repo_path(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }

    let state = State::load().unwrap_or_default();
    if let Some(p) = state.repo_path() {
        return Ok(PathBuf::from(p));
    }

    anyhow::bail!("No repo path configured — run `nostos init` first");
}

fn build_auto_sync_commit_message(machine_id: &str, changed_paths: &[String]) -> String {
    let prefix = format!("nostos: auto-sync from {machine_id}");
    if changed_paths.is_empty() {
        return prefix;
    }

    let filenames: Vec<String> = changed_paths
        .iter()
        .map(|path| leaf_filename(path))
        .collect();
    let total = filenames.len();
    let shown = total.min(3);
    let remaining = total.saturating_sub(shown);
    let summary = format_filename_summary(&filenames[..shown], remaining);

    format!("{prefix} — {total} file(s): {summary}")
}

fn leaf_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string())
}

fn format_filename_summary(filenames: &[String], remaining: usize) -> String {
    match filenames {
        [] => String::new(),
        [only] if remaining == 0 => only.clone(),
        [first, second] if remaining == 0 => format!("{first} and {second}"),
        [first, second, third] if remaining == 0 => format!("{first}, {second}, and {third}"),
        _ => {
            let listed = filenames.join(", ");
            format!("{listed}, and {remaining} more")
        }
    }
}

fn run_apply(repo_path: &Path, state: &mut State) -> anyhow::Result<()> {
    let config_path = repo_path.join("nostos.toml");
    let cfg = match config::load(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    let platform = crate::platform::detect().map_err(|e| anyhow::anyhow!("{e}"))?;
    let machine_id = state.machine_id().map(|s| s.to_string());

    let report = match reconcile::apply(&cfg, state, repo_path, &platform, machine_id.as_deref())
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    if let Some(ref dotfiles_report) = report.dotfiles {
        super::report::print_apply_section("Dotfiles", dotfiles_report);
    }
    if let Some(ref files_report) = report.files {
        super::report::print_apply_section("Files", files_report);
    }

    if state.repo_path().is_none() {
        state.set_repo_path(repo_path.to_string_lossy().to_string());
    }
    if let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_auto_sync_commit_message;

    #[test]
    fn auto_sync_commit_message_uses_leaf_filenames() {
        let changed = vec![
            "dotfiles/bashrc".to_string(),
            "config/git/gitconfig".to_string(),
        ];

        let message = build_auto_sync_commit_message("work-macbook", &changed);

        assert_eq!(
            message,
            "nostos: auto-sync from work-macbook — 2 file(s): bashrc and gitconfig"
        );
    }

    #[test]
    fn auto_sync_commit_message_lists_three_filenames() {
        let changed = vec![
            "dotfiles/bashrc".to_string(),
            "dotfiles/gitconfig".to_string(),
            "dotfiles/zshrc".to_string(),
        ];

        let message = build_auto_sync_commit_message("work-macbook", &changed);

        assert_eq!(
            message,
            "nostos: auto-sync from work-macbook — 3 file(s): bashrc, gitconfig, and zshrc"
        );
    }

    #[test]
    fn auto_sync_commit_message_appends_remaining_count() {
        let changed = vec![
            "dotfiles/bashrc".to_string(),
            "dotfiles/gitconfig".to_string(),
            "dotfiles/zshrc".to_string(),
            "dotfiles/tmux.conf".to_string(),
        ];

        let message = build_auto_sync_commit_message("work-macbook", &changed);

        assert_eq!(
            message,
            "nostos: auto-sync from work-macbook — 4 file(s): bashrc, gitconfig, zshrc, and 1 more"
        );
    }

    #[test]
    fn auto_sync_commit_message_falls_back_to_prefix_when_clean() {
        let message = build_auto_sync_commit_message("work-macbook", &[]);

        assert_eq!(message, "nostos: auto-sync from work-macbook");
    }
}

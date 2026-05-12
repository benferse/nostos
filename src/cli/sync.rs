use crate::config;
use crate::git;
use crate::reconcile;
use crate::state::State;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn run(repo: Option<&Path>, apply: bool) -> anyhow::Result<ExitCode> {
    let repo_path = resolve_repo_path(repo)?;

    // Step 1 — Auto-commit local changes
    let mut state = State::load().unwrap_or_default();
    state.ensure_machine_identity();
    let machine_id = state.machine_id().unwrap_or("unknown");

    match git::commit(&repo_path, &format!("nostos: auto-sync from {machine_id}")) {
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

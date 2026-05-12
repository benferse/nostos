use crate::config;
use crate::reconcile;
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

    let platform = crate::platform::detect().map_err(|e| anyhow::anyhow!("{e}"))?;
    let machine_id = state.machine_id().map(|s| s.to_string());

    let report = match reconcile::apply(&cfg, &mut state, &repo_root, &platform, machine_id.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::from(3));
        }
    };

    if let Some(ref dotfiles_report) = report.dotfiles {
        super::report::print_apply_section("Dotfiles", dotfiles_report);
    }
    if let Some(ref files_report) = report.files {
        super::report::print_apply_section("Files", files_report);
    }
    super::report::print_pending_phases(&report);

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


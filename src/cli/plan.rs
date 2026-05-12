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

    let state = State::load().unwrap_or_default();
    let repo_root = config::repo_root_from_config(&config_path);

    let platform = crate::platform::detect().map_err(|e| anyhow::anyhow!("{e}"))?;
    let machine_id = state.machine_id().map(|s| s.to_string());

    let report = match reconcile::plan(&cfg, &state, &repo_root, &platform, machine_id.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(ExitCode::from(3));
        }
    };

    if let Some(ref dotfiles_report) = report.dotfiles {
        super::report::print_plan_section("Dotfiles", dotfiles_report);
    }
    if let Some(ref files_report) = report.files {
        super::report::print_plan_section("Files", files_report);
    }
    super::report::print_pending_phases(&report);

    Ok(ExitCode::SUCCESS)
}


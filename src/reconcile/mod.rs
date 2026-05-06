pub mod dotfiles;

use crate::config::Config;
use crate::state::State;
use std::path::Path;

/// Summary of a single reconciliation phase.
#[derive(Debug, Default)]
pub struct PhaseReport {
    /// Human-readable name of the phase.
    pub phase: String,
    /// Whether this phase actually executed.
    pub executed: bool,
    /// Warning message if the phase was skipped (not yet implemented).
    pub skipped_reason: Option<String>,
}

/// Top-level report aggregating all reconciliation phases.
#[derive(Debug, Default)]
pub struct ApplyReport {
    /// Results from the dotfiles phase.
    pub dotfiles: Option<dotfiles::Report>,
    /// Phase summaries for phases that are not yet implemented.
    pub pending_phases: Vec<PhaseReport>,
}

impl ApplyReport {
    /// Whether any phase produced warnings requiring user attention.
    pub fn has_warnings(&self) -> bool {
        self.dotfiles.as_ref().is_some_and(|r| r.has_warnings())
    }

    /// Whether any phase had errors.
    pub fn has_errors(&self) -> bool {
        self.dotfiles.as_ref().is_some_and(|r| !r.errors.is_empty())
    }
}

/// Build a plan (dry-run) across all phases.
pub fn plan(config: &Config, state: &State, repo_root: &Path) -> Result<ApplyReport, PlanError> {
    let mut report = ApplyReport::default();

    // Phase 1: Pre-apply hooks (not yet implemented)
    if !config.hooks.is_empty() {
        let pre_hooks: Vec<_> = config
            .hooks
            .iter()
            .filter(|h| h.when == crate::config::hooks::HookWhen::PreApply)
            .collect();
        if !pre_hooks.is_empty() {
            report.pending_phases.push(PhaseReport {
                phase: "pre-apply hooks".to_string(),
                executed: false,
                skipped_reason: Some(format!(
                    "{} pre-apply hook(s) declared but hook execution is not yet implemented",
                    pre_hooks.len()
                )),
            });
        }
    }

    // Phase 2: Dotfiles (active)
    if let Some(ref dotfiles_config) = config.dotfiles {
        let dotfiles_report =
            dotfiles::plan(dotfiles_config, state, repo_root).map_err(PlanError::Dotfiles)?;
        report.dotfiles = Some(dotfiles_report);
    }

    // Phase 3: Tools (not yet implemented)
    if !config.tools.is_empty() {
        report.pending_phases.push(PhaseReport {
            phase: "tools".to_string(),
            executed: false,
            skipped_reason: Some(format!(
                "{} tool(s) declared but tool installation is not yet implemented",
                config.tools.len()
            )),
        });
    }

    // Phase 4: Post-apply hooks (not yet implemented)
    if !config.hooks.is_empty() {
        let post_hooks: Vec<_> = config
            .hooks
            .iter()
            .filter(|h| h.when == crate::config::hooks::HookWhen::PostApply)
            .collect();
        if !post_hooks.is_empty() {
            report.pending_phases.push(PhaseReport {
                phase: "post-apply hooks".to_string(),
                executed: false,
                skipped_reason: Some(format!(
                    "{} post-apply hook(s) declared but hook execution is not yet implemented",
                    post_hooks.len()
                )),
            });
        }
    }

    // Files section (not yet implemented)
    if config.files.is_some() {
        report.pending_phases.push(PhaseReport {
            phase: "files".to_string(),
            executed: false,
            skipped_reason: Some(
                "[files] section declared but verbatim file copy is not yet implemented"
                    .to_string(),
            ),
        });
    }

    Ok(report)
}

/// Apply all phases: execute changes.
pub fn apply(
    config: &Config,
    state: &mut State,
    repo_root: &Path,
) -> Result<ApplyReport, ApplyError> {
    let mut report = ApplyReport::default();

    // Phase 1: Pre-apply hooks (not yet implemented)
    if !config.hooks.is_empty() {
        let pre_hooks: Vec<_> = config
            .hooks
            .iter()
            .filter(|h| h.when == crate::config::hooks::HookWhen::PreApply)
            .collect();
        if !pre_hooks.is_empty() {
            report.pending_phases.push(PhaseReport {
                phase: "pre-apply hooks".to_string(),
                executed: false,
                skipped_reason: Some(format!(
                    "{} pre-apply hook(s) declared but hook execution is not yet implemented",
                    pre_hooks.len()
                )),
            });
        }
    }

    // Phase 2: Dotfiles (active)
    if let Some(ref dotfiles_config) = config.dotfiles {
        let dotfiles_report = dotfiles::apply(dotfiles_config, state, repo_root)
            .map_err(ApplyError::Dotfiles)?;
        report.dotfiles = Some(dotfiles_report);
    }

    // Phase 3: Tools (not yet implemented)
    if !config.tools.is_empty() {
        report.pending_phases.push(PhaseReport {
            phase: "tools".to_string(),
            executed: false,
            skipped_reason: Some(format!(
                "{} tool(s) declared but tool installation is not yet implemented",
                config.tools.len()
            )),
        });
    }

    // Phase 4: Post-apply hooks (not yet implemented)
    if !config.hooks.is_empty() {
        let post_hooks: Vec<_> = config
            .hooks
            .iter()
            .filter(|h| h.when == crate::config::hooks::HookWhen::PostApply)
            .collect();
        if !post_hooks.is_empty() {
            report.pending_phases.push(PhaseReport {
                phase: "post-apply hooks".to_string(),
                executed: false,
                skipped_reason: Some(format!(
                    "{} post-apply hook(s) declared but hook execution is not yet implemented",
                    post_hooks.len()
                )),
            });
        }
    }

    // Files section (not yet implemented)
    if config.files.is_some() {
        report.pending_phases.push(PhaseReport {
            phase: "files".to_string(),
            executed: false,
            skipped_reason: Some(
                "[files] section declared but verbatim file copy is not yet implemented"
                    .to_string(),
            ),
        });
    }

    Ok(report)
}

/// Errors from plan operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    #[error("dotfiles: {0}")]
    Dotfiles(#[from] dotfiles::Error),
}

/// Errors from apply operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError {
    #[error("dotfiles: {0}")]
    Dotfiles(#[from] dotfiles::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::State;
    use std::path::Path;

    fn empty_config() -> Config {
        toml::from_str("").unwrap()
    }

    #[test]
    fn plan_empty_config_succeeds() {
        let config = empty_config();
        let state = State::default();
        let report = plan(&config, &state, Path::new("/nonexistent")).unwrap();
        assert!(report.dotfiles.is_none());
        assert!(report.pending_phases.is_empty());
    }

    #[test]
    fn plan_full_config_reports_pending_phases() {
        let config: Config = toml::from_str(
            r#"
            [[tool]]
            name = "ripgrep"

            [[hook]]
            name = "setup"
            run = "hooks/setup.sh"
            when = "pre-apply"

            [[hook]]
            name = "cleanup"
            run = "hooks/cleanup.sh"
            when = "post-apply"

            [files]
            source = "files/"
            target = "~"
        "#,
        )
        .unwrap();
        let state = State::default();
        let report = plan(&config, &state, Path::new("/nonexistent")).unwrap();
        assert!(report.dotfiles.is_none());
        // Should have: pre-apply hooks, tools, post-apply hooks, files = 4 pending phases
        assert_eq!(report.pending_phases.len(), 4);
        assert!(report.pending_phases.iter().any(|p| p.phase == "tools"));
        assert!(report
            .pending_phases
            .iter()
            .any(|p| p.phase == "pre-apply hooks"));
        assert!(report
            .pending_phases
            .iter()
            .any(|p| p.phase == "post-apply hooks"));
        assert!(report.pending_phases.iter().any(|p| p.phase == "files"));
    }

    #[test]
    fn apply_report_has_warnings_delegation() {
        let report = ApplyReport::default();
        assert!(!report.has_warnings());
        assert!(!report.has_errors());
    }
}

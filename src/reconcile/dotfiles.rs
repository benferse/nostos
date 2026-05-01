use crate::config::dotfiles::DotfilesConfig;
use crate::state::{AppliedEntry, State};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// What needs to happen for a single dotfile.
#[derive(Debug, Clone, PartialEq)]
pub enum DotfileAction {
    /// Source and target match — nothing to do.
    UpToDate {
        target: PathBuf,
    },
    /// Repo changed, target unchanged — safe to copy.
    CleanUpdate {
        source: PathBuf,
        target: PathBuf,
        source_hash: String,
    },
    /// Target changed, repo unchanged — warn and skip.
    LocalModification {
        target: PathBuf,
    },
    /// Both changed — back up target, copy from repo.
    Conflict {
        source: PathBuf,
        target: PathBuf,
        backup: PathBuf,
        source_hash: String,
    },
    /// Target doesn't exist or is unmanaged — new file to copy.
    NewFile {
        source: PathBuf,
        target: PathBuf,
        source_hash: String,
    },
}

/// Outcome of reconciliation.
#[derive(Debug, Default)]
pub struct Report {
    pub actions: Vec<DotfileAction>,
    /// Errors that occurred for individual files (non-fatal).
    pub errors: Vec<String>,
}

impl Report {
    /// Whether any actions require user attention (conflicts, local mods).
    pub fn has_warnings(&self) -> bool {
        self.actions.iter().any(|a| {
            matches!(
                a,
                DotfileAction::LocalModification { .. } | DotfileAction::Conflict { .. }
            )
        })
    }
}

/// Errors that can occur during reconciliation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("dotfile source directory not found: {0}")]
    SourceNotFound(PathBuf),

    #[error("cannot determine home directory for tilde expansion")]
    NoHomeDir,

    #[error("failed to read directory {0}: {1}")]
    ReadDir(PathBuf, #[source] std::io::Error),

    #[error("failed to copy file to {0}: {1}")]
    FileCopy(PathBuf, #[source] std::io::Error),

    #[error("failed to create directory {0}: {1}")]
    CreateDir(PathBuf, #[source] std::io::Error),
}

/// Build a plan without executing it.
pub fn plan(
    config: &DotfilesConfig,
    state: &State,
    repo_root: &Path,
) -> Result<Report, Error> {
    let source_dir = repo_root.join(&config.source);
    if !source_dir.is_dir() {
        return Err(Error::SourceNotFound(source_dir));
    }

    let target_base = expand_tilde(&config.target)?;
    let mut report = Report::default();

    let files = walk_source_dir(&source_dir)?;
    for rel_path in files {
        let source = source_dir.join(&rel_path);
        let dot_path = dot_prepend(&rel_path);
        let target = target_base.join(&dot_path);

        match classify(&source, &target, &dot_path, state) {
            Ok(action) => report.actions.push(action),
            Err(msg) => report.errors.push(msg),
        }
    }

    Ok(report)
}

/// Execute: build plan, apply it (copy files, create backups), update state.
pub fn apply(
    config: &DotfilesConfig,
    state: &mut State,
    repo_root: &Path,
) -> Result<Report, Error> {
    let source_dir = repo_root.join(&config.source);
    if !source_dir.is_dir() {
        return Err(Error::SourceNotFound(source_dir));
    }

    let target_base = expand_tilde(&config.target)?;
    let mut report = Report::default();

    let files = walk_source_dir(&source_dir)?;
    for rel_path in files {
        let source = source_dir.join(&rel_path);
        let dot_path = dot_prepend(&rel_path);
        let target = target_base.join(&dot_path);

        let action = match classify(&source, &target, &dot_path, state) {
            Ok(action) => action,
            Err(msg) => {
                report.errors.push(msg);
                continue;
            }
        };

        match &action {
            DotfileAction::UpToDate { .. } => {}
            DotfileAction::LocalModification { .. } => {}
            DotfileAction::NewFile { source, target, source_hash }
            | DotfileAction::CleanUpdate { source, target, source_hash } => {
                if let Err(e) = copy_file(source, target) {
                    report.errors.push(format!("failed to copy to {}: {e}", target.display()));
                    continue;
                }
                state.applied.insert(
                    dot_path,
                    AppliedEntry {
                        hash: source_hash.clone(),
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
            DotfileAction::Conflict {
                source,
                target,
                backup,
                source_hash,
            } => {
                // Back up the existing file first
                if let Err(e) = std::fs::copy(target, backup) {
                    report.errors.push(format!(
                        "failed to back up {}: {e}",
                        target.display()
                    ));
                    continue;
                }
                if let Err(e) = copy_file(source, target) {
                    report.errors.push(format!("failed to copy to {}: {e}", target.display()));
                    continue;
                }
                state.applied.insert(
                    dot_path,
                    AppliedEntry {
                        hash: source_hash.clone(),
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
        }

        report.actions.push(action);
    }

    Ok(report)
}

/// Classify a single dotfile into one of the five action states.
fn classify(
    source: &Path,
    target: &Path,
    state_key: &str,
    state: &State,
) -> Result<DotfileAction, String> {
    // Check if target path is a directory (where we expect a file)
    if target.is_dir() {
        return Err(format!(
            "target {} is a directory, expected a file",
            target.display()
        ));
    }

    let source_hash = hash_file(source).map_err(|e| {
        format!("cannot read source {}: {e}", source.display())
    })?;

    let target_exists = target.exists();
    let state_entry = state.applied.get(state_key);

    if !target_exists {
        // NewFile: target missing (regardless of whether state entry exists)
        return Ok(DotfileAction::NewFile {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            source_hash,
        });
    }

    let target_hash = hash_file(target).map_err(|e| {
        format!("cannot read target {}: {e}", target.display())
    })?;

    match state_entry {
        None => {
            // No state entry — file is unmanaged
            if source_hash == target_hash {
                // Already matches, treat as up to date
                Ok(DotfileAction::UpToDate {
                    target: target.to_path_buf(),
                })
            } else {
                // Pre-existing unmanaged file — treat as conflict (back up and copy)
                Ok(DotfileAction::Conflict {
                    source: source.to_path_buf(),
                    target: target.to_path_buf(),
                    backup: backup_path(target),
                    source_hash,
                })
            }
        }
        Some(entry) => {
            let state_hash = &entry.hash;

            if source_hash == target_hash {
                // Both match — up to date
                Ok(DotfileAction::UpToDate {
                    target: target.to_path_buf(),
                })
            } else if target_hash == *state_hash && source_hash != *state_hash {
                // Target unchanged, source updated — clean update
                Ok(DotfileAction::CleanUpdate {
                    source: source.to_path_buf(),
                    target: target.to_path_buf(),
                    source_hash,
                })
            } else if source_hash == *state_hash && target_hash != *state_hash {
                // Source unchanged, target edited locally — local modification
                Ok(DotfileAction::LocalModification {
                    target: target.to_path_buf(),
                })
            } else {
                // Both changed — conflict
                Ok(DotfileAction::Conflict {
                    source: source.to_path_buf(),
                    target: target.to_path_buf(),
                    backup: backup_path(target),
                    source_hash,
                })
            }
        }
    }
}

/// Recursively walk a directory and return all file paths relative to it.
fn walk_source_dir(dir: &Path) -> Result<Vec<String>, Error> {
    let mut files = Vec::new();
    walk_recursive(dir, dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_recursive(base: &Path, current: &Path, files: &mut Vec<String>) -> Result<(), Error> {
    let entries =
        std::fs::read_dir(current).map_err(|e| Error::ReadDir(current.to_path_buf(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::ReadDir(current.to_path_buf(), e))?;
        let path = entry.path();

        if path.is_dir() {
            walk_recursive(base, &path, files)?;
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(base)
        {
            files.push(rel.to_string_lossy().to_string());
        }
    }

    Ok(())
}

/// Apply the dot-prepend convention: prepend `.` to the first path component.
///
/// `bashrc` → `.bashrc`
/// `config/starship.toml` → `.config/starship.toml`
fn dot_prepend(rel_path: &str) -> String {
    format!(".{rel_path}")
}

/// Expand `~` to the user's home directory.
fn expand_tilde(path: &str) -> Result<PathBuf, Error> {
    if path == "~" || path.starts_with("~/") {
        let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
        if path == "~" {
            Ok(home)
        } else {
            Ok(home.join(&path[2..]))
        }
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Hash a file's contents using SHA-256, returning `sha256:<hex>`.
fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let content = std::fs::read(path)?;
    let digest = Sha256::digest(&content);
    Ok(format!("sha256:{:x}", digest))
}

/// Generate a backup path for a conflicting file.
fn backup_path(target: &Path) -> PathBuf {
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let file_name = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    target.with_file_name(format!("{file_name}.nostos-backup-{timestamp}"))
}

/// Copy a file, creating parent directories as needed.
fn copy_file(source: &Path, target: &Path) -> Result<(), Error> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::CreateDir(parent.to_path_buf(), e))?;
    }
    std::fs::copy(source, target)
        .map_err(|e| Error::FileCopy(target.to_path_buf(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to set up a test repo with a dotfiles source directory.
    struct TestRepo {
        _dir: TempDir,
        repo_root: PathBuf,
        target_dir: PathBuf,
        state_path: PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let repo_root = dir.path().join("repo");
            let target_dir = dir.path().join("home");
            let state_path = dir.path().join("state.toml");
            fs::create_dir_all(repo_root.join("dotfiles")).unwrap();
            fs::create_dir_all(&target_dir).unwrap();
            TestRepo {
                _dir: dir,
                repo_root,
                target_dir,
                state_path,
            }
        }

        fn config(&self) -> DotfilesConfig {
            DotfilesConfig {
                source: "dotfiles/".to_string(),
                target: self.target_dir.to_string_lossy().to_string(),
            }
        }

        fn add_source(&self, rel_path: &str, content: &str) {
            let path = self.repo_root.join("dotfiles").join(rel_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }

        fn add_target(&self, dot_path: &str, content: &str) {
            let path = self.target_dir.join(dot_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }

        fn read_target(&self, dot_path: &str) -> String {
            fs::read_to_string(self.target_dir.join(dot_path)).unwrap()
        }

        fn target_exists(&self, dot_path: &str) -> bool {
            self.target_dir.join(dot_path).exists()
        }

        fn load_state(&self) -> State {
            State::load_from(&self.state_path).unwrap()
        }

        fn make_state_entry(&self, dot_path: &str, content: &str) -> (String, AppliedEntry) {
            let digest = Sha256::digest(content.as_bytes());
            (
                dot_path.to_string(),
                AppliedEntry {
                    hash: format!("sha256:{:x}", digest),
                    timestamp: chrono::Utc::now(),
                },
            )
        }
    }

    // ── Plan tests ──────────────────────────────────────────

    #[test]
    fn plan_new_file() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# bash config");
        let state = repo.load_state();
        let config = repo.config();

        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(&report.actions[0], DotfileAction::NewFile { .. }));
    }

    #[test]
    fn plan_up_to_date() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# bash config");
        repo.add_target(".bashrc", "# bash config");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# bash config");
        state.applied.insert(key, entry);

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::UpToDate { .. }
        ));
    }

    #[test]
    fn plan_clean_update() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# updated config");
        repo.add_target(".bashrc", "# original config");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# original config");
        state.applied.insert(key, entry);

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::CleanUpdate { .. }
        ));
    }

    #[test]
    fn plan_local_modification() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# original config");
        repo.add_target(".bashrc", "# user edited this");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# original config");
        state.applied.insert(key, entry);

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::LocalModification { .. }
        ));
    }

    #[test]
    fn plan_conflict() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# repo version 2");
        repo.add_target(".bashrc", "# user version 2");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# original config");
        state.applied.insert(key, entry);

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::Conflict { .. }
        ));
    }

    #[test]
    fn plan_preexisting_unmanaged_file() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# repo version");
        repo.add_target(".bashrc", "# existing different file");
        let state = State::default(); // no state entry

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::Conflict { .. }
        ));
    }

    #[test]
    fn plan_first_run_all_new() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "bash");
        repo.add_source("gitconfig", "git");
        let state = State::default();
        let config = repo.config();

        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 2);
        assert!(report
            .actions
            .iter()
            .all(|a| matches!(a, DotfileAction::NewFile { .. })));
    }

    #[test]
    fn plan_nested_directories() {
        let repo = TestRepo::new();
        repo.add_source("config/starship.toml", "starship config");
        let state = State::default();
        let config = repo.config();

        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        match &report.actions[0] {
            DotfileAction::NewFile { target, .. } => {
                assert!(target.ends_with(".config/starship.toml"));
            }
            other => panic!("expected NewFile, got {other:?}"),
        }
    }

    #[test]
    fn plan_empty_source_dir() {
        let repo = TestRepo::new();
        let state = State::default();
        let config = repo.config();

        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert!(report.actions.is_empty());
    }

    #[test]
    fn plan_source_not_found() {
        let repo = TestRepo::new();
        let config = DotfilesConfig {
            source: "nonexistent/".to_string(),
            target: repo.target_dir.to_string_lossy().to_string(),
        };
        let state = State::default();
        assert!(matches!(
            plan(&config, &state, &repo.repo_root),
            Err(Error::SourceNotFound(_))
        ));
    }

    #[test]
    fn plan_target_deleted_since_apply() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# config");
        // Target doesn't exist, but state has an entry
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# config");
        state.applied.insert(key, entry);

        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::NewFile { .. }
        ));
    }

    #[test]
    fn plan_target_is_directory() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# config");
        // Make target a directory instead of a file
        fs::create_dir_all(repo.target_dir.join(".bashrc")).unwrap();
        let state = State::default();
        let config = repo.config();

        let report = plan(&config, &state, &repo.repo_root).unwrap();
        assert!(!report.errors.is_empty());
        assert!(report.actions.is_empty());
    }

    #[test]
    fn plan_does_not_modify_filesystem() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# bash config");
        let state = State::default();
        let config = repo.config();

        plan(&config, &state, &repo.repo_root).unwrap();

        // Verify target was NOT created
        assert!(!repo.target_exists(".bashrc"));
    }

    // ── Apply tests ─────────────────────────────────────────

    #[test]
    fn apply_copies_new_files() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# bash config");
        let mut state = State::default();
        let config = repo.config();

        let report = apply(&config, &mut state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::NewFile { .. }
        ));
        assert_eq!(repo.read_target(".bashrc"), "# bash config");
        assert!(state.applied.contains_key(".bashrc"));
    }

    #[test]
    fn apply_creates_parent_directories() {
        let repo = TestRepo::new();
        repo.add_source("config/alacritty/alacritty.toml", "config content");
        let mut state = State::default();
        let config = repo.config();

        apply(&config, &mut state, &repo.repo_root).unwrap();
        assert_eq!(
            repo.read_target(".config/alacritty/alacritty.toml"),
            "config content"
        );
    }

    #[test]
    fn apply_creates_backup_on_conflict() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# repo version 2");
        repo.add_target(".bashrc", "# user version 2");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# original");
        state.applied.insert(key, entry);
        let config = repo.config();

        let report = apply(&config, &mut state, &repo.repo_root).unwrap();

        // Target should have repo content
        assert_eq!(repo.read_target(".bashrc"), "# repo version 2");

        // A backup file should exist with user's content
        match &report.actions[0] {
            DotfileAction::Conflict { backup, .. } => {
                let backup_content = fs::read_to_string(backup).unwrap();
                assert_eq!(backup_content, "# user version 2");
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn apply_idempotent() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# bash config");
        let mut state = State::default();
        let config = repo.config();

        // First apply
        apply(&config, &mut state, &repo.repo_root).unwrap();

        // Second apply — should be all up to date
        let report = apply(&config, &mut state, &repo.repo_root).unwrap();
        assert_eq!(report.actions.len(), 1);
        assert!(matches!(
            &report.actions[0],
            DotfileAction::UpToDate { .. }
        ));
    }

    #[test]
    fn apply_skips_local_modification() {
        let repo = TestRepo::new();
        repo.add_source("bashrc", "# original");
        repo.add_target(".bashrc", "# user edited");
        let mut state = State::default();
        let (key, entry) = repo.make_state_entry(".bashrc", "# original");
        state.applied.insert(key, entry);
        let config = repo.config();

        let report = apply(&config, &mut state, &repo.repo_root).unwrap();

        // Should skip — user's file preserved
        assert_eq!(repo.read_target(".bashrc"), "# user edited");
        assert!(matches!(
            &report.actions[0],
            DotfileAction::LocalModification { .. }
        ));
    }

    #[test]
    fn apply_multiple_files_mixed_states() {
        let repo = TestRepo::new();

        // File 1: new file
        repo.add_source("newfile", "new content");

        // File 2: up to date
        repo.add_source("uptodate", "same content");
        repo.add_target(".uptodate", "same content");

        // File 3: local mod
        repo.add_source("localmod", "original");
        repo.add_target(".localmod", "user edited");

        let mut state = State::default();
        let (k2, e2) = repo.make_state_entry(".uptodate", "same content");
        state.applied.insert(k2, e2);
        let (k3, e3) = repo.make_state_entry(".localmod", "original");
        state.applied.insert(k3, e3);

        let config = repo.config();
        let report = apply(&config, &mut state, &repo.repo_root).unwrap();

        assert_eq!(report.actions.len(), 3);
        assert!(report.has_warnings()); // local mod
    }

    #[test]
    fn apply_tilde_expansion() {
        // This test verifies tilde expansion works by using an absolute path
        // (we can't actually test "~" without touching the real home dir)
        let repo = TestRepo::new();
        repo.add_source("testfile", "content");
        let mut state = State::default();
        let config = repo.config(); // uses absolute target_dir path

        apply(&config, &mut state, &repo.repo_root).unwrap();
        assert!(repo.target_exists(".testfile"));
    }

    #[test]
    fn dot_prepend_convention() {
        assert_eq!(dot_prepend("bashrc"), ".bashrc");
        assert_eq!(dot_prepend("config/starship.toml"), ".config/starship.toml");
        assert_eq!(dot_prepend("ssh/config"), ".ssh/config");
    }

    #[test]
    fn expand_tilde_with_absolute_path() {
        let result = expand_tilde("/home/user").unwrap();
        assert_eq!(result, PathBuf::from("/home/user"));
    }

    #[test]
    fn expand_tilde_with_home() {
        let result = expand_tilde("~").unwrap();
        let home = dirs::home_dir().expect("home dir should exist in test");
        assert_eq!(result, home);
    }

    #[test]
    fn expand_tilde_with_subpath() {
        let result = expand_tilde("~/Documents").unwrap();
        let home = dirs::home_dir().expect("home dir should exist in test");
        assert_eq!(result, home.join("Documents"));
    }
}

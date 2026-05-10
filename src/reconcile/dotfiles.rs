use crate::state::{AppliedEntry, State};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// What needs to happen for a single file (dotfile or verbatim).
///
/// Despite the name, this enum is used for both `[dotfiles]` (with
/// dot-prepend) and `[files]` (verbatim copy) reconciliation.
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
    /// Per-file actions determined (or executed) during reconciliation.
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
    /// The configured source directory does not exist.
    #[error("source directory not found: {0}")]
    SourceNotFound(PathBuf),

    /// The source path is a symlink; nostos refuses to follow it.
    #[error("source path is a symlink (refusing to follow): {0}")]
    SourceIsSymlink(PathBuf),

    /// The home directory could not be determined for tilde expansion.
    #[error("cannot determine home directory for tilde expansion")]
    NoHomeDir,

    /// A directory entry in the source tree could not be read.
    #[error("failed to read directory {0}: {1}")]
    ReadDir(PathBuf, #[source] std::io::Error),

    /// A file could not be copied to its target location.
    #[error("failed to copy file to {0}: {1}")]
    FileCopy(PathBuf, #[source] std::io::Error),

    /// A required parent directory could not be created.
    #[error("failed to create directory {0}: {1}")]
    CreateDir(PathBuf, #[source] std::io::Error),
}

/// Build a plan for dotfiles (with dot-prepend convention).
pub fn plan(
    config: &crate::config::dotfiles::DotfilesConfig,
    state: &State,
    repo_root: &Path,
) -> Result<Report, Error> {
    plan_inner(&config.source, &config.target, true, state, repo_root)
}

/// Execute dotfiles (with dot-prepend convention).
pub fn apply(
    config: &crate::config::dotfiles::DotfilesConfig,
    state: &mut State,
    repo_root: &Path,
) -> Result<Report, Error> {
    apply_inner(&config.source, &config.target, true, state, repo_root)
}

/// Build a plan for verbatim files (no dot-prepend).
pub fn plan_files(
    config: &crate::config::files::FilesConfig,
    state: &State,
    repo_root: &Path,
) -> Result<Report, Error> {
    plan_inner(&config.source, &config.target, false, state, repo_root)
}

/// Execute verbatim files (no dot-prepend).
pub fn apply_files(
    config: &crate::config::files::FilesConfig,
    state: &mut State,
    repo_root: &Path,
) -> Result<Report, Error> {
    apply_inner(&config.source, &config.target, false, state, repo_root)
}

/// Shared plan logic: walk source tree, classify each file.
fn plan_inner(
    source: &str,
    target: &str,
    prepend_dot: bool,
    state: &State,
    repo_root: &Path,
) -> Result<Report, Error> {
    let source_dir = repo_root.join(source);
    check_source_dir(&source_dir)?;

    let target_base = expand_tilde(target)?;
    let mut report = Report::default();

    let (files, skips) = walk_source_dir(&source_dir)?;
    report.errors.extend(skips);
    for rel_path in files {
        let src = source_dir.join(&rel_path);
        let target_rel = if prepend_dot {
            dot_prepend(&rel_path)
        } else {
            rel_path
        };
        let tgt = target_base.join(&target_rel);

        match classify(&src, &tgt, &target_rel, state) {
            Ok(action) => report.actions.push(action),
            Err(msg) => report.errors.push(msg),
        }
    }

    Ok(report)
}

/// Shared apply logic: walk source tree, classify and execute each file.
fn apply_inner(
    source: &str,
    target: &str,
    prepend_dot: bool,
    state: &mut State,
    repo_root: &Path,
) -> Result<Report, Error> {
    let source_dir = repo_root.join(source);
    check_source_dir(&source_dir)?;

    let target_base = expand_tilde(target)?;
    let mut report = Report::default();

    let (files, skips) = walk_source_dir(&source_dir)?;
    report.errors.extend(skips);
    for rel_path in files {
        let src = source_dir.join(&rel_path);
        let target_rel = if prepend_dot {
            dot_prepend(&rel_path)
        } else {
            rel_path
        };
        let tgt = target_base.join(&target_rel);

        let action = match classify(&src, &tgt, &target_rel, state) {
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
                    target_rel,
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
                    target_rel,
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

/// Validate the configured source directory: it must exist, be a real
/// directory, and not be a symlink itself. Refusing symlinks at the root
/// matters because `Path::is_dir` follows symlinks, which would otherwise
/// silently let `dotfiles/` point to an arbitrary location on disk.
fn check_source_dir(source_dir: &Path) -> Result<(), Error> {
    // Normalize away any trailing path separator. On Linux, `lstat` (used by
    // `symlink_metadata`) follows symlinks when the path ends with `/`, so a
    // configured source like `"dotfiles/"` would defeat the symlink check.
    // `Path::components` strips trailing separators.
    let normalized: PathBuf = source_dir.components().collect();
    let meta = match std::fs::symlink_metadata(&normalized) {
        Ok(m) => m,
        Err(_) => return Err(Error::SourceNotFound(normalized)),
    };
    if meta.file_type().is_symlink() {
        return Err(Error::SourceIsSymlink(normalized));
    }
    if !meta.is_dir() {
        return Err(Error::SourceNotFound(normalized));
    }
    Ok(())
}

/// Recursively walk a directory and return all file paths relative to it,
/// along with a list of skip messages for any symlinks (or unreadable
/// entries) encountered. Symlinks are intentionally not followed: a symlink
/// in the dotfiles tree pointing at, e.g., `/etc/passwd` would otherwise be
/// dereferenced by `std::fs::copy` and written into the user's home as a
/// regular file.
fn walk_source_dir(dir: &Path) -> Result<(Vec<String>, Vec<String>), Error> {
    let mut files = Vec::new();
    let mut skips = Vec::new();
    walk_recursive(dir, dir, &mut files, &mut skips)?;
    files.sort();
    skips.sort();
    Ok((files, skips))
}

fn walk_recursive(
    base: &Path,
    current: &Path,
    files: &mut Vec<String>,
    skips: &mut Vec<String>,
) -> Result<(), Error> {
    let entries =
        std::fs::read_dir(current).map_err(|e| Error::ReadDir(current.to_path_buf(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::ReadDir(current.to_path_buf(), e))?;
        let path = entry.path();
        let display_rel = path
            .strip_prefix(base)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                skips.push(format!(
                    "skipping {display_rel}: cannot read file type: {e}"
                ));
                continue;
            }
        };

        if file_type.is_symlink() {
            skips.push(format!("skipping symlink in source: {display_rel}"));
            continue;
        }

        if file_type.is_dir() {
            walk_recursive(base, &path, files, skips)?;
        } else if file_type.is_file()
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
///
/// Streams the file through the hasher with a fixed-size buffer so that large
/// files do not cause RSS to grow proportionally to file size.
fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
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
    use crate::config::dotfiles::DotfilesConfig;
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
    fn apply_preserves_existing_directory_contents() {
        let repo = TestRepo::new();

        // Pre-populate the target with existing content in .config/
        repo.add_target(".config/existing-app/settings.json", r#"{"theme":"dark"}"#);
        repo.add_target(".config/other.conf", "existing config");

        // Source repo wants to place config/foo/bar.toml → .config/foo/bar.toml
        repo.add_source("config/foo/bar.toml", "new dotfile content");
        let mut state = State::default();
        let config = repo.config();

        apply(&config, &mut state, &repo.repo_root).unwrap();

        // New file was placed correctly
        assert_eq!(repo.read_target(".config/foo/bar.toml"), "new dotfile content");

        // Pre-existing content is untouched
        assert_eq!(
            repo.read_target(".config/existing-app/settings.json"),
            r#"{"theme":"dark"}"#,
        );
        assert_eq!(repo.read_target(".config/other.conf"), "existing config");
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

    // ── Symlink tests (Unix only) ──────────────────────────

    #[cfg(unix)]
    #[test]
    fn plan_skips_file_symlink_in_source() {
        use std::os::unix::fs::symlink;

        let repo = TestRepo::new();
        // Create a file outside the dotfiles tree, then symlink it from inside.
        let outside = repo._dir.path().join("secret.txt");
        fs::write(&outside, "SECRET").unwrap();
        symlink(&outside, repo.repo_root.join("dotfiles/sneaky")).unwrap();

        let state = State::default();
        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();

        assert!(report.actions.is_empty(), "no actions for symlinked source");
        assert!(
            report.errors.iter().any(|e| e.contains("symlink") && e.contains("sneaky")),
            "expected a symlink-skip error, got {:?}",
            report.errors
        );
    }

    #[cfg(unix)]
    #[test]
    fn apply_skips_file_symlink_and_does_not_copy_target() {
        use std::os::unix::fs::symlink;

        let repo = TestRepo::new();
        let outside = repo._dir.path().join("secret.txt");
        fs::write(&outside, "SECRET").unwrap();
        symlink(&outside, repo.repo_root.join("dotfiles/sneaky")).unwrap();

        let mut state = State::default();
        let config = repo.config();
        let report = apply(&config, &mut state, &repo.repo_root).unwrap();

        assert!(report.actions.is_empty());
        assert!(report.errors.iter().any(|e| e.contains("symlink")));
        assert!(
            !repo.target_exists(".sneaky"),
            "symlink contents must not be copied into the target tree"
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_does_not_descend_into_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let repo = TestRepo::new();
        // Build a real directory outside the source, then symlink it in.
        let outside_dir = repo._dir.path().join("outside_dir");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("hidden.conf"), "do not copy").unwrap();
        symlink(&outside_dir, repo.repo_root.join("dotfiles/linked")).unwrap();
        // Also a real file alongside, to confirm normal traversal still works.
        repo.add_source("normal", "normal content");

        let state = State::default();
        let config = repo.config();
        let report = plan(&config, &state, &repo.repo_root).unwrap();

        // Only the real file should produce an action.
        assert_eq!(report.actions.len(), 1);
        match &report.actions[0] {
            DotfileAction::NewFile { target, .. } => assert!(target.ends_with(".normal")),
            other => panic!("unexpected action: {other:?}"),
        }
        assert!(report.errors.iter().any(|e| e.contains("symlink") && e.contains("linked")));
        // The file inside the linked dir must not surface as an action.
        assert!(
            !report.actions.iter().any(|a| matches!(a, DotfileAction::NewFile { source, .. } if source.ends_with("hidden.conf"))),
            "must not descend into a directory symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn plan_rejects_source_dir_that_is_a_symlink() {
        use std::os::unix::fs::symlink;

        let repo = TestRepo::new();
        // Replace the existing dotfiles dir with a symlink to a real dir.
        let real = repo._dir.path().join("real_dotfiles");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("bashrc"), "# bash").unwrap();
        fs::remove_dir_all(repo.repo_root.join("dotfiles")).unwrap();
        symlink(&real, repo.repo_root.join("dotfiles")).unwrap();

        let state = State::default();
        let config = repo.config();
        let result = plan(&config, &state, &repo.repo_root);
        assert!(
            matches!(result, Err(Error::SourceIsSymlink(_))),
            "expected SourceIsSymlink, got {result:?}"
        );
    }

    // ── Hash streaming tests ───────────────────────────────

    #[test]
    fn hash_file_streaming_matches_one_shot_on_large_input() {
        // 64 KiB + 123 bytes deliberately straddles the 8 KiB read buffer with
        // a partial final read, exercising the EOF path in hash_file.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("big.bin");
        let mut content = Vec::with_capacity(64 * 1024 + 123);
        for i in 0..(64 * 1024 + 123) {
            content.push((i % 256) as u8);
        }
        fs::write(&path, &content).unwrap();

        let streamed = hash_file(&path).unwrap();
        let expected = format!("sha256:{:x}", Sha256::digest(&content));
        assert_eq!(streamed, expected);
    }

    #[test]
    fn hash_file_handles_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.bin");
        fs::write(&path, b"").unwrap();

        let hashed = hash_file(&path).unwrap();
        let expected = format!("sha256:{:x}", Sha256::digest([]));
        assert_eq!(hashed, expected);
    }
}

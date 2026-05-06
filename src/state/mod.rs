use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Machine identity information.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MachineInfo {
    /// Machine identifier (hostname by default, or user-assigned).
    pub id: String,
}

/// Repository location information.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoInfo {
    /// Absolute path to the repository root on this machine.
    pub path: String,
}

/// Local state tracking what nostos has applied on this machine.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Machine identity.
    #[serde(default)]
    pub machine: Option<MachineInfo>,

    /// Per-file tracking of applied hashes and timestamps.
    /// Keys are target-relative paths with dot prepended (e.g., ".bashrc").
    #[serde(default)]
    pub applied: BTreeMap<String, AppliedEntry>,

    /// Location of the dotfiles repository on this machine.
    #[serde(default)]
    pub repo: Option<RepoInfo>,
}

/// Record of a single applied dotfile.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppliedEntry {
    /// Content hash at last apply (e.g., "sha256:abc123...").
    pub hash: String,
    /// Timestamp of last apply.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Errors that can occur during state operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to read state file: {0}")]
    Read(#[source] std::io::Error),

    #[error("failed to parse state file")]
    Parse(#[source] toml::de::Error),

    #[error("failed to write state file: {0}")]
    Write(#[source] std::io::Error),

    #[error("cannot determine config directory for state file")]
    NoConfigDir,
}

impl State {
    /// Ensure machine identity is set. If not already present, detect from hostname.
    pub fn ensure_machine_identity(&mut self) {
        if self.machine.is_none() {
            let id = detect_hostname().unwrap_or_else(|| "unknown".to_string());
            self.machine = Some(MachineInfo { id });
        }
    }

    /// Get the machine ID, if set.
    pub fn machine_id(&self) -> Option<&str> {
        self.machine.as_ref().map(|m| m.id.as_str())
    }

    /// Get the stored repo path, if any.
    pub fn repo_path(&self) -> Option<&str> {
        self.repo.as_ref().map(|r| r.path.as_str())
    }

    /// Set the repo path. Used on first apply to remember where the repo is.
    pub fn set_repo_path(&mut self, path: String) {
        self.repo = Some(RepoInfo { path });
    }

    /// Load state from the platform-appropriate location.
    pub fn load() -> Result<Self, Error> {
        let path = default_state_path()?;
        Self::load_from(&path)
    }

    /// Load state from a specific path. Returns empty state if the file
    /// does not exist.
    pub fn load_from(path: &Path) -> Result<Self, Error> {
        match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).map_err(Error::Parse),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(Error::Read(e)),
        }
    }

    /// Save state to the platform-appropriate location.
    pub fn save(&self) -> Result<(), Error> {
        let path = default_state_path()?;
        self.save_to(&path)
    }

    /// Save state to a specific path using atomic write.
    ///
    /// Writes to a uniquely-named temporary file in the same directory and
    /// then renames it over `path`. Using a unique temp file (rather than a
    /// fixed `state.toml.tmp`) prevents two concurrent writers — e.g.,
    /// nostos invocations running against different repos in parallel shells
    /// — from clobbering each other's temp data.
    pub fn save_to(&self, path: &Path) -> Result<(), Error> {
        use std::io::Write;

        let parent = parent_or_dot(path);
        std::fs::create_dir_all(parent).map_err(Error::Write)?;

        let content = toml::to_string_pretty(self).expect("state should always serialize");

        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(Error::Write)?;
        tmp.write_all(content.as_bytes()).map_err(Error::Write)?;
        #[cfg(windows)]
        {
            let mut attempts = 0;
            loop {
                match tmp.persist(path) {
                    Ok(_) => break,
                    Err(e)
                        if attempts < 5
                            && e.error.kind() == std::io::ErrorKind::PermissionDenied =>
                    {
                        attempts += 1;
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        tmp = e.file;
                    }
                    Err(e) => return Err(Error::Write(e.error)),
                }
            }
        }
        #[cfg(not(windows))]
        {
            tmp.persist(path).map_err(|e| Error::Write(e.error))?;
        }

        Ok(())
    }
}

/// Resolve the directory we should place the temp file in.
///
/// `Path::parent()` returns `Some("")` for a bare filename like
/// `state.toml` and `None` for the root, so naive use of the result would
/// hand `tempfile::NamedTempFile::new_in` an empty path. Fall back to "."
/// in either case.
fn parent_or_dot(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Get the default state file path for this platform.
///
/// Checks `XDG_CONFIG_HOME` first (standard on Linux, also useful for test
/// isolation on macOS where `dirs::config_dir()` ignores it), then falls
/// back to the platform config directory.
fn default_state_path() -> Result<PathBuf, Error> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| dirs::config_dir().unwrap_or_default());

    if config_dir.as_os_str().is_empty() {
        return Err(Error::NoConfigDir);
    }

    Ok(config_dir.join("nostos").join("state.toml"))
}

/// Detect the system hostname.
fn detect_hostname() -> Option<String> {
    // Try HOSTNAME env var first (common on Linux)
    if let Ok(h) = std::env::var("HOSTNAME")
        && !h.is_empty()
    {
        return Some(h);
    }

    // Try COMPUTERNAME on Windows
    if let Ok(h) = std::env::var("COMPUTERNAME")
        && !h.is_empty()
    {
        return Some(h);
    }

    hostname_from_command()
}

fn hostname_from_command() -> Option<String> {
    use std::process::Command;
    let output = Command::new("hostname").output().ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn round_trip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");

        let mut state = State::default();
        state.applied.insert(
            ".bashrc".to_string(),
            AppliedEntry {
                hash: "sha256:abc123".to_string(),
                timestamp: Utc::now(),
            },
        );

        state.save_to(&path).expect("save should succeed");
        let loaded = State::load_from(&path).expect("load should succeed");
        assert_eq!(state.applied.len(), loaded.applied.len());
        assert_eq!(
            state.applied[".bashrc"].hash,
            loaded.applied[".bashrc"].hash
        );
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let state = State::load_from(&path).expect("should return empty state");
        assert!(state.applied.is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "this is not valid toml [[[[").unwrap();
        assert!(State::load_from(&path).is_err());
    }

    #[test]
    fn creates_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("state.toml");
        let state = State::default();
        state.save_to(&path).expect("should create dirs and save");
        assert!(path.exists());
    }

    #[test]
    fn applied_entries_serialize_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");

        let mut state = State::default();
        state.applied.insert(
            ".config/starship.toml".to_string(),
            AppliedEntry {
                hash: "sha256:def456".to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-04-26T20:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
        );

        state.save_to(&path).expect("save");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("sha256:def456"));
        assert!(content.contains(".config/starship.toml"));
    }

    #[test]
    fn atomic_write_no_temp_file_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let state = State::default();
        state.save_to(&path).expect("save");

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "parent dir should contain only state.toml, got {entries:?}"
        );
        assert_eq!(entries[0], "state.toml");
    }

    #[test]
    fn update_existing_entry_and_resave() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");

        let mut state = State::default();
        state.applied.insert(
            ".bashrc".to_string(),
            AppliedEntry {
                hash: "sha256:old".to_string(),
                timestamp: Utc::now(),
            },
        );
        state.save_to(&path).expect("save");

        // Update the entry
        state.applied.get_mut(".bashrc").unwrap().hash = "sha256:new".to_string();
        state.save_to(&path).expect("save again");

        let loaded = State::load_from(&path).expect("load");
        assert_eq!(loaded.applied[".bashrc"].hash, "sha256:new");
    }

    #[test]
    fn many_entries_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");

        let mut state = State::default();
        for i in 0..50 {
            state.applied.insert(
                format!(".file{i}"),
                AppliedEntry {
                    hash: format!("sha256:hash{i}"),
                    timestamp: Utc::now(),
                },
            );
        }
        state.save_to(&path).expect("save");
        let loaded = State::load_from(&path).expect("load");
        assert_eq!(loaded.applied.len(), 50);
    }

    #[test]
    fn save_to_bare_filename_uses_current_dir() {
        // The parent helper must resolve a bare filename to "."; otherwise
        // NamedTempFile::new_in would fail on an empty path. We verify the
        // helper directly to avoid mutating process-global cwd.
        assert_eq!(parent_or_dot(Path::new("state.toml")), Path::new("."));
        assert_eq!(parent_or_dot(Path::new("./state.toml")), Path::new("."));
        assert_eq!(parent_or_dot(Path::new("a/state.toml")), Path::new("a"));
        assert_eq!(parent_or_dot(Path::new("/")), Path::new("."));
    }

    #[test]
    fn concurrent_writes_dont_corrupt_state() {
        // Many threads writing distinct, valid State payloads to the same
        // path should never leave a half-written or interleaved file. With
        // unique tempfiles, the file always contains exactly one writer's
        // payload (last writer wins).
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        const N: usize = 8;

        let barrier = Arc::new(Barrier::new(N));
        let path = Arc::new(path);

        // Pre-build the expected serialized content for each thread so we can
        // match the final on-disk file against one of them exactly.
        let expected_contents: Vec<String> = (0..N)
            .map(|i| {
                let mut state = State::default();
                state.applied.insert(
                    format!(".file{i}"),
                    AppliedEntry {
                        hash: format!("sha256:thread{i}"),
                        timestamp: chrono::DateTime::parse_from_rfc3339(
                            "2026-04-26T20:00:00Z",
                        )
                        .unwrap()
                        .with_timezone(&Utc),
                    },
                );
                toml::to_string_pretty(&state).unwrap()
            })
            .collect();

        let handles: Vec<_> = (0..N)
            .map(|i| {
                let barrier = Arc::clone(&barrier);
                let path = Arc::clone(&path);
                let expected = expected_contents[i].clone();
                std::thread::spawn(move || {
                    // Reconstruct the same State the expected content was built from.
                    let state: State = toml::from_str(&expected).unwrap();
                    barrier.wait();
                    state.save_to(&path).expect("concurrent save should succeed");
                })
            })
            .collect();

        for h in handles {
            h.join().expect("worker thread should not panic");
        }

        // Final file must parse and exactly equal one of the writers' payloads.
        let final_content = std::fs::read_to_string(path.as_path()).unwrap();
        let _: State = toml::from_str(&final_content).expect("file should parse");
        assert!(
            expected_contents.iter().any(|e| e == &final_content),
            "final file must match one writer's content exactly; got {final_content:?}"
        );

        // Parent dir holds only the state file — no leaked tempfiles.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1, "leaked entries: {entries:?}");
        assert_eq!(entries[0], "state.toml");
    }

    #[test]
    fn state_with_machine_identity_roundtrips() {
        let state = State {
            machine: Some(MachineInfo {
                id: "work-macbook".to_string(),
            }),
            applied: BTreeMap::new(),
            repo: None,
        };
        let serialized = toml::to_string_pretty(&state).unwrap();
        let deserialized: State = toml::from_str(&serialized).unwrap();
        assert_eq!(state, deserialized);
    }

    #[test]
    fn state_without_machine_loads_as_none() {
        let state: State = toml::from_str("[applied]\n").unwrap();
        assert!(state.machine.is_none());
    }

    #[test]
    fn ensure_machine_identity_sets_id() {
        let mut state = State::default();
        assert!(state.machine.is_none());
        state.ensure_machine_identity();
        assert!(state.machine.is_some());
        assert!(!state.machine.as_ref().unwrap().id.is_empty());
    }

    #[test]
    fn ensure_machine_identity_preserves_existing() {
        let mut state = State {
            machine: Some(MachineInfo {
                id: "my-machine".to_string(),
            }),
            applied: BTreeMap::new(),
            repo: None,
        };
        state.ensure_machine_identity();
        assert_eq!(state.machine_id(), Some("my-machine"));
    }

    #[test]
    fn legacy_state_without_machine_section() {
        // Old state files from before machine identity was added should still load
        let toml_str = r#"
[applied]
".bashrc" = { hash = "sha256:abc123", timestamp = "2026-04-26T20:00:00Z" }
"#;
        let state: State = toml::from_str(toml_str).unwrap();
        assert!(state.machine.is_none());
        assert_eq!(state.applied.len(), 1);
    }

    #[test]
    fn state_with_repo_path_roundtrips() {
        let state = State {
            repo: Some(RepoInfo {
                path: "/home/user/dotfiles".to_string(),
            }),
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&state).unwrap();
        let deserialized: State = toml::from_str(&serialized).unwrap();
        assert_eq!(state.repo, deserialized.repo);
    }

    #[test]
    fn state_without_repo_loads_as_none() {
        let state: State = toml::from_str("[applied]\n").unwrap();
        assert!(state.repo.is_none());
    }

    #[test]
    fn set_repo_path_stores_path() {
        let mut state = State::default();
        assert!(state.repo_path().is_none());
        state.set_repo_path("/home/user/dotfiles".to_string());
        assert_eq!(state.repo_path(), Some("/home/user/dotfiles"));
    }

    #[test]
    fn legacy_state_without_repo_section() {
        let toml_str = r#"
[applied]
".bashrc" = { hash = "sha256:abc123", timestamp = "2026-04-26T20:00:00Z" }
"#;
        let state: State = toml::from_str(toml_str).unwrap();
        assert!(state.repo.is_none());
        assert_eq!(state.applied.len(), 1);
    }
}

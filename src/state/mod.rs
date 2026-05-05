use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Local state tracking what nostos has applied on this machine.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Per-file tracking of applied hashes and timestamps.
    /// Keys are target-relative paths with dot prepended (e.g., ".bashrc").
    #[serde(default)]
    pub applied: BTreeMap<String, AppliedEntry>,
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
    /// The state file could not be read from disk.
    #[error("failed to read state file: {0}")]
    Read(#[source] std::io::Error),

    /// The state file contains invalid TOML.
    #[error("failed to parse state file")]
    Parse(#[source] toml::de::Error),

    /// The state file could not be written to disk.
    #[error("failed to write state file: {0}")]
    Write(#[source] std::io::Error),

    /// The platform config directory could not be determined.
    #[error("cannot determine config directory for state file")]
    NoConfigDir,
}

impl State {
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
        tmp.persist(path).map_err(|e| Error::Write(e.error))?;

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
fn default_state_path() -> Result<PathBuf, Error> {
    let config_dir = dirs::config_dir().ok_or(Error::NoConfigDir)?;
    Ok(config_dir.join("nostos").join("state.toml"))
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
}

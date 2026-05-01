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

    /// Save state to a specific path using atomic write (write to temp
    /// file, then rename).
    pub fn save_to(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(Error::Write)?;
        }

        let content = toml::to_string_pretty(self).expect("state should always serialize");

        // Atomic write: write to a temp file in the same directory, then rename
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &content).map_err(Error::Write)?;
        std::fs::rename(&tmp_path, path).map_err(Error::Write)?;

        Ok(())
    }
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

        let tmp = path.with_extension("toml.tmp");
        assert!(!tmp.exists(), "temp file should be cleaned up");
        assert!(path.exists(), "state file should exist");
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
}

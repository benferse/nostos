use std::collections::HashMap;

/// Configuration for the `[files]` section (verbatim copy, no dot-prepend).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    /// Source directory relative to repo root.
    pub source: String,
    /// Target directory.
    pub target: String,
    /// Platform-specific overrides: `[files.platforms.<os>]` maps
    /// target-relative paths to alternate source paths in the repo.
    #[serde(default)]
    pub platforms: HashMap<String, HashMap<String, String>>,
    /// Machine-specific overrides: `[files.machines.<id>]` maps
    /// target-relative paths to alternate source paths in the repo.
    #[serde(default)]
    pub machines: HashMap<String, HashMap<String, String>>,
}

use std::collections::HashMap;

/// Configuration for the `[dotfiles]` section.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DotfilesConfig {
    /// Source directory relative to the repo root (e.g., "dotfiles/").
    pub source: String,
    /// Target directory where dotfiles are placed (e.g., "~").
    pub target: String,
    /// Platform-specific overrides: `[dotfiles.platforms.<os>]` maps
    /// target-relative paths to alternate source paths in the repo.
    #[serde(default)]
    pub platforms: HashMap<String, HashMap<String, String>>,
    /// Machine-specific overrides: `[dotfiles.machines.<id>]` maps
    /// target-relative paths to alternate source paths in the repo.
    #[serde(default)]
    pub machines: HashMap<String, HashMap<String, String>>,
}

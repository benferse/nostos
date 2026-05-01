/// Configuration for the `[dotfiles]` section.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DotfilesConfig {
    /// Source directory relative to the repo root (e.g., "dotfiles/").
    pub source: String,
    /// Target directory where dotfiles are placed (e.g., "~").
    pub target: String,
}

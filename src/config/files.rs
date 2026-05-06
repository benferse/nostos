/// Configuration for the `[files]` section (verbatim copy, no dot-prepend).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesConfig {
    /// Source directory relative to repo root.
    pub source: String,
    /// Target directory.
    pub target: String,
}

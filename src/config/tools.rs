/// Configuration for a single `[[tool]]` entry.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    /// Tool name (also the default package name for all managers).
    pub name: String,
    /// Binary name to check for presence on PATH. Defaults to `name`.
    pub bin: Option<String>,
    /// Per-manager install overrides. Keys are manager names, values are package names.
    /// Special key: "script" for script-based installation.
    #[serde(default)]
    pub install: std::collections::BTreeMap<String, String>,
    /// Platforms this tool applies to. Empty/absent means all platforms.
    #[serde(default)]
    pub platforms: Vec<String>,
}

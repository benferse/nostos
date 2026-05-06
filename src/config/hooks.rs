/// Valid values for the hook `when` field.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookWhen {
    PreApply,
    PostApply,
}

/// Configuration for a single `[[hook]]` entry.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookConfig {
    /// Unique name for this hook.
    pub name: String,
    /// Path to the script, relative to repo root.
    pub run: String,
    /// When this hook runs in the apply lifecycle.
    pub when: HookWhen,
    /// Platforms this hook applies to. Empty means all.
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Machines this hook applies to. Empty means all.
    #[serde(default)]
    pub machines: Vec<String>,
}

/// Per-platform installer preferences.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PreferencesConfig {
    /// macOS preferences.
    pub macos: Option<PlatformPreferences>,
    /// Windows preferences.
    pub windows: Option<PlatformPreferences>,
    /// Linux preferences — either generic or per-distro.
    pub linux: Option<LinuxPreferences>,
}

/// Preferences for a single platform.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformPreferences {
    #[serde(rename = "installer-priority")]
    pub installer_priority: Vec<String>,
}

/// Linux preferences with optional per-distro overrides.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LinuxPreferences {
    /// Fallback priority for unrecognized distros.
    #[serde(rename = "installer-priority", default)]
    pub installer_priority: Vec<String>,
    /// Per-distro overrides. Flatten captures distro-named sub-tables.
    #[serde(flatten)]
    pub distros: std::collections::BTreeMap<String, PlatformPreferences>,
}

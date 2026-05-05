pub mod dotfiles;
pub mod files;
pub mod hooks;
pub mod preferences;
pub mod tools;

use std::path::{Path, PathBuf};

/// Errors that can occur when loading or parsing config.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum Error {
    #[error("config file not found: {path}")]
    NotFound { path: String },

    #[error("failed to read config file: {path}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse nostos.toml")]
    #[diagnostic(help("check TOML syntax at the indicated location"))]
    ParseError {
        #[source_code]
        src: miette::NamedSource<String>,
        #[label("parse error near here")]
        span: miette::SourceOffset,
        msg: String,
    },
}

/// Top-level nostos configuration.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub dotfiles: Option<dotfiles::DotfilesConfig>,
    #[serde(default, rename = "tool")]
    pub tools: Vec<tools::ToolConfig>,
    #[serde(default, rename = "hook")]
    pub hooks: Vec<hooks::HookConfig>,
    pub files: Option<files::FilesConfig>,
    #[serde(default)]
    pub preferences: Option<preferences::PreferencesConfig>,
}

/// Find and load `nostos.toml` in the current directory.
pub fn find_config() -> Result<(PathBuf, Config), Error> {
    let path = PathBuf::from("nostos.toml");
    if !path.exists() {
        return Err(Error::NotFound {
            path: path.display().to_string(),
        });
    }
    let config = load(&path)?;
    Ok((path, config))
}

/// Find config with resolution order: explicit path > state-stored path > current directory.
pub fn find_config_with_repo(explicit_repo: Option<&Path>) -> Result<(PathBuf, Config), Error> {
    // 1. Explicit --repo flag
    if let Some(repo_path) = explicit_repo {
        let config_path = repo_path.join("nostos.toml");
        if !config_path.exists() {
            return Err(Error::NotFound {
                path: config_path.display().to_string(),
            });
        }
        return load(&config_path).map(|c| (config_path, c));
    }

    // 2. State-stored repo path
    if let Ok(state) = crate::state::State::load()
        && let Some(repo_path_str) = state.repo_path()
    {
        let repo_path = PathBuf::from(repo_path_str);
        let config_path = repo_path.join("nostos.toml");
        if config_path.exists() {
            return load(&config_path).map(|c| (config_path, c));
        }
    }

    // 3. Current directory (existing behavior)
    find_config()
}

/// Load and parse a nostos config from the given path.
pub fn load(path: &Path) -> Result<Config, Error> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::ReadError {
        path: path.display().to_string(),
        source: e,
    })?;

    parse(&content, path)
}

/// Parse nostos config from a TOML string.
pub fn parse(content: &str, path: &Path) -> Result<Config, Error> {
    let config: Config = toml::from_str(content).map_err(|e| {
        let offset = e.span().map_or(0, |s| s.start);
        Error::ParseError {
            src: miette::NamedSource::new(path.display().to_string(), content.to_string()),
            span: miette::SourceOffset::from(offset),
            msg: e.message().to_string(),
        }
    })?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse_str(s: &str) -> Result<Config, Error> {
        parse(s, Path::new("test.toml"))
    }

    #[test]
    fn valid_minimal_config() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"
            "#,
        )
        .expect("should parse");
        let df = config.dotfiles.expect("dotfiles should be present");
        assert_eq!(df.source, "dotfiles/");
        assert_eq!(df.target, "~");
    }

    #[test]
    fn missing_dotfiles_section() {
        // Empty config is valid TOML but has no dotfiles
        let config = parse_str("").expect("should parse empty");
        assert!(config.dotfiles.is_none());
    }

    #[test]
    fn unknown_top_level_key() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [hooks]
            something = true
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn unknown_key_inside_dotfiles() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"
            extra = "bad"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn missing_source_field() {
        let result = parse_str(
            r#"
            [dotfiles]
            target = "~"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn missing_target_field() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn empty_file_is_valid() {
        // Empty TOML is valid, just no dotfiles section
        let config = parse_str("").expect("should parse");
        assert!(config.dotfiles.is_none());
    }

    #[test]
    fn invalid_toml_syntax() {
        let result = parse_str("[dotfiles\nsource = broken");
        assert!(result.is_err());
    }

    #[test]
    fn config_with_comments_and_whitespace() {
        let config = parse_str(
            r#"
            # This is a comment
            [dotfiles]
            source = "dotfiles/"   # inline comment
            target = "~"
            "#,
        )
        .expect("should parse");
        assert!(config.dotfiles.is_some());
    }

    #[test]
    fn empty_strings_in_source_or_target() {
        // Technically valid TOML — reconciler will catch empty paths
        let config = parse_str(
            r#"
            [dotfiles]
            source = ""
            target = ""
            "#,
        )
        .expect("should parse");
        let df = config.dotfiles.expect("dotfiles present");
        assert_eq!(df.source, "");
        assert_eq!(df.target, "");
    }

    #[test]
    fn config_with_tools() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [[tool]]
            name = "ripgrep"
            bin = "rg"

            [[tool]]
            name = "fd"
            install.apt = "fd-find"
            install.cargo = "fd-find"
            platforms = ["linux"]
            "#,
        )
        .expect("should parse");
        assert_eq!(config.tools.len(), 2);
        assert_eq!(config.tools[0].name, "ripgrep");
        assert_eq!(config.tools[0].bin.as_deref(), Some("rg"));
        assert_eq!(config.tools[1].install.get("apt").unwrap(), "fd-find");
        assert_eq!(config.tools[1].platforms, vec!["linux"]);
    }

    #[test]
    fn config_with_hooks() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [[hook]]
            name = "install-homebrew"
            run = "hooks/install-homebrew.sh"
            when = "pre-apply"
            platforms = ["macos"]
            "#,
        )
        .expect("should parse");
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].name, "install-homebrew");
        assert_eq!(config.hooks[0].when, hooks::HookWhen::PreApply);
    }

    #[test]
    fn config_with_files_section() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [files]
            source = "files/"
            target = "~"
            "#,
        )
        .expect("should parse");
        let files = config.files.expect("files should be present");
        assert_eq!(files.source, "files/");
    }

    #[test]
    fn config_with_preferences() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [preferences.macos]
            installer-priority = ["brew", "cargo"]

            [preferences.linux.ubuntu]
            installer-priority = ["apt", "cargo"]
            "#,
        )
        .expect("should parse");
        let prefs = config.preferences.expect("preferences present");
        let macos = prefs.macos.expect("macos prefs");
        assert_eq!(macos.installer_priority, vec!["brew", "cargo"]);
    }

    #[test]
    fn tool_missing_name() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [[tool]]
            bin = "rg"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn hook_invalid_when() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [[hook]]
            name = "test"
            run = "hooks/test.sh"
            when = "during-apply"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn hook_missing_required_fields() {
        let result = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [[hook]]
            name = "test"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn full_realistic_config() {
        let config = parse_str(
            r#"
            [dotfiles]
            source = "dotfiles/"
            target = "~"

            [preferences.macos]
            installer-priority = ["brew", "cargo"]

            [preferences.windows]
            installer-priority = ["winget", "scoop", "cargo"]

            [preferences.linux.ubuntu]
            installer-priority = ["apt", "cargo"]

            [[tool]]
            name = "ripgrep"
            bin = "rg"

            [[tool]]
            name = "fd"
            install.apt = "fd-find"
            install.cargo = "fd-find"

            [[tool]]
            name = "cargo-edit"
            bin = "cargo-add"
            install.cargo = "cargo-edit"

            [[hook]]
            name = "install-homebrew"
            run = "hooks/install-homebrew.sh"
            when = "pre-apply"
            platforms = ["macos"]

            [[hook]]
            name = "setup-ssh"
            run = "hooks/setup-ssh.sh"
            when = "post-apply"
            platforms = ["linux", "macos"]
            "#,
        )
        .expect("should parse full config");
        assert_eq!(config.tools.len(), 3);
        assert_eq!(config.hooks.len(), 2);
    }
}

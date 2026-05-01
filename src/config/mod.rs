pub mod dotfiles;

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

    #[error("missing required [dotfiles] section")]
    #[diagnostic(help("add a [dotfiles] section with `source` and `target` fields"))]
    MissingDotfiles,
}

/// Top-level nostos configuration.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub dotfiles: Option<dotfiles::DotfilesConfig>,
}

/// Find and load `nostos.toml` in the current directory.
///
/// This is isolated behind a helper so adding `--repo` later is trivial.
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
}

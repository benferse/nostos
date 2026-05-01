use super::Error;

/// Operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

/// CPU architecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    Other(String),
}

/// Linux distribution information parsed from `/etc/os-release`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    /// Machine-readable ID (e.g., "ubuntu", "fedora").
    pub id: String,
    /// Human-readable name (e.g., "Ubuntu 24.04 LTS").
    pub name: String,
}

impl std::fmt::Display for Os {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Os::Linux => write!(f, "linux"),
            Os::MacOs => write!(f, "macos"),
            Os::Windows => write!(f, "windows"),
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::Aarch64 => write!(f, "aarch64"),
            Arch::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::fmt::Display for Distro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub fn detect_os() -> Result<Os, Error> {
    if cfg!(target_os = "linux") {
        Ok(Os::Linux)
    } else if cfg!(target_os = "macos") {
        Ok(Os::MacOs)
    } else if cfg!(target_os = "windows") {
        Ok(Os::Windows)
    } else {
        Err(Error::UnsupportedOs)
    }
}

pub fn detect_arch() -> Arch {
    if cfg!(target_arch = "x86_64") {
        Arch::X86_64
    } else if cfg!(target_arch = "aarch64") {
        Arch::Aarch64
    } else {
        Arch::Other(std::env::consts::ARCH.to_string())
    }
}

/// Attempt to detect the Linux distro by reading `/etc/os-release`.
pub fn detect_distro() -> Option<Distro> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;
    parse_os_release(&content)
}

/// Parse `/etc/os-release` content into a `Distro`.
///
/// Expects `ID=...` and `PRETTY_NAME=...` (or `NAME=...`) lines.
/// Values may be quoted or unquoted.
pub fn parse_os_release(content: &str) -> Option<Distro> {
    let mut id = None;
    let mut pretty_name = None;
    let mut name = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("ID=") {
            id = Some(unquote(val));
        } else if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
            pretty_name = Some(unquote(val));
        } else if let Some(val) = line.strip_prefix("NAME=") {
            name = Some(unquote(val));
        }
    }

    let id = id?;
    let display_name = pretty_name.or(name).unwrap_or_else(|| id.clone());

    Some(Distro {
        id,
        name: display_name,
    })
}

/// Remove surrounding quotes from a value if present.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_os_release_ubuntu() {
        let content = r#"
NAME="Ubuntu"
VERSION="24.04 LTS (Noble Numbat)"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Ubuntu 24.04 LTS"
VERSION_ID="24.04"
"#;
        let distro = parse_os_release(content).expect("should parse");
        assert_eq!(distro.id, "ubuntu");
        assert_eq!(distro.name, "Ubuntu 24.04 LTS");
    }

    #[test]
    fn parse_os_release_missing_id() {
        let content = "PRETTY_NAME=\"Some Linux\"\n";
        assert!(parse_os_release(content).is_none());
    }

    #[test]
    fn parse_os_release_unquoted_values() {
        let content = "ID=arch\nNAME=Arch Linux\n";
        let distro = parse_os_release(content).expect("should parse");
        assert_eq!(distro.id, "arch");
        assert_eq!(distro.name, "Arch Linux");
    }

    #[test]
    fn parse_os_release_single_quotes() {
        let content = "ID='fedora'\nPRETTY_NAME='Fedora Linux 40'\n";
        let distro = parse_os_release(content).expect("should parse");
        assert_eq!(distro.id, "fedora");
        assert_eq!(distro.name, "Fedora Linux 40");
    }

    #[test]
    fn unquote_single_char_does_not_panic() {
        // A lone quote character should not panic
        assert_eq!(unquote("\""), "\"");
        assert_eq!(unquote("'"), "'");
        assert_eq!(unquote(""), "");
    }
}

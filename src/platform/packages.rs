use std::path::PathBuf;

/// A package manager detected on the current system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageManager {
    /// Short name (e.g., "brew", "apt", "cargo", "winget").
    pub name: String,
    /// Path to the binary.
    pub path: PathBuf,
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Known package managers to check for.
const KNOWN_MANAGERS: &[&str] = &[
    "apt",
    "brew",
    "cargo",
    "choco",
    "dnf",
    "pacman",
    "pip",
    "scoop",
    "snap",
    "winget",
    "yum",
];

/// Discover which package managers are available on the current system.
/// Checks PATH for known manager binaries.
pub fn discover() -> Vec<PackageManager> {
    let mut found = Vec::new();
    for &name in KNOWN_MANAGERS {
        if let Some(path) = which(name) {
            found.push(PackageManager {
                name: name.to_string(),
                path,
            });
        }
    }
    found.sort();
    found
}

/// Simple `which`-like lookup: find a binary on PATH.
fn which(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.to_string_lossy().split(sep) {
        let candidate = PathBuf::from(dir).join(binary);
        // On Windows, also check with .exe extension
        if cfg!(windows) {
            let with_exe = candidate.with_extension("exe");
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_finds_at_least_one_manager() {
        // On any development machine, at least cargo should be available
        // (since we're building a Rust project)
        let managers = discover();
        assert!(
            !managers.is_empty(),
            "expected at least one package manager (cargo) to be detected"
        );
    }

    #[test]
    fn discover_finds_cargo() {
        // Since we're running in a Rust build environment, cargo must be present
        let managers = discover();
        assert!(
            managers.iter().any(|m| m.name == "cargo"),
            "cargo should be detected in a Rust build environment"
        );
    }

    #[test]
    fn which_finds_existing_binary() {
        // cargo must exist since we're running cargo test
        assert!(which("cargo").is_some());
    }

    #[test]
    fn which_returns_none_for_nonexistent() {
        assert!(which("nonexistent_binary_xyz_12345").is_none());
    }

    #[test]
    fn package_manager_display() {
        let pm = PackageManager {
            name: "brew".to_string(),
            path: PathBuf::from("/usr/local/bin/brew"),
        };
        assert_eq!(format!("{pm}"), "brew");
    }
}

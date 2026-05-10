//! File map builder: resolves the base → platform → machine cascade
//! into a flat target-relative → source-absolute path map.
//!
//! This is the core algorithmic module for layering. It takes a config
//! section (dotfiles or files), the current platform, and an optional
//! machine identity, then produces the map that drives reconciliation.

use crate::platform::{Os, Platform};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A diagnostic produced during file map construction.
#[derive(Debug, Clone, PartialEq)]
pub enum Diagnostic {
    /// An override references a source file that does not exist in the repo.
    MissingSource {
        layer: String,
        target: String,
        source: PathBuf,
    },
    /// A platform name in config does not match any known platform.
    UnknownPlatform {
        name: String,
    },
    /// A file or directory was skipped during the base directory walk
    /// (e.g., symlinks, unreadable entries).
    WalkSkip(String),
    /// An override source path resolves to a location outside the repo root.
    SourceOutsideRepo {
        layer: String,
        target: String,
        source: PathBuf,
    },
}

/// Inputs common to both dotfiles and files sections.
pub struct FileMapInput<'a> {
    /// Source directory relative to repo root (e.g., "dotfiles/").
    pub source: &'a str,
    /// Platform-specific overrides.
    pub platforms: &'a HashMap<String, HashMap<String, String>>,
    /// Machine-specific overrides.
    pub machines: &'a HashMap<String, HashMap<String, String>>,
}

impl<'a> From<&'a crate::config::dotfiles::DotfilesConfig> for FileMapInput<'a> {
    fn from(c: &'a crate::config::dotfiles::DotfilesConfig) -> Self {
        FileMapInput {
            source: &c.source,
            platforms: &c.platforms,
            machines: &c.machines,
        }
    }
}

impl<'a> From<&'a crate::config::files::FilesConfig> for FileMapInput<'a> {
    fn from(c: &'a crate::config::files::FilesConfig) -> Self {
        FileMapInput {
            source: &c.source,
            platforms: &c.platforms,
            machines: &c.machines,
        }
    }
}

/// Build the file map by walking the base source directory and applying
/// platform and machine overrides.
///
/// Returns `(file_map, diagnostics)` where `file_map` maps target-relative
/// paths to absolute source paths, and `diagnostics` contains warnings
/// and errors encountered during construction.
pub fn build(
    input: &FileMapInput,
    repo_root: &Path,
    platform: &Platform,
    machine_id: Option<&str>,
) -> Result<(HashMap<String, PathBuf>, Vec<Diagnostic>), super::dotfiles::Error> {
    let source_dir = repo_root.join(input.source);
    let mut diagnostics = Vec::new();

    // Walk base directory, excluding reserved override directories
    let (base_files, skips) = super::dotfiles::walk_source_dir(&source_dir)?;

    // Convert walk skips to diagnostics
    for skip in skips {
        diagnostics.push(Diagnostic::WalkSkip(skip));
    }

    let mut file_map: HashMap<String, PathBuf> = HashMap::new();
    for rel_path in &base_files {
        // Skip files under platforms/ and machines/ — those are only
        // reachable via explicit override mappings.
        if rel_path.starts_with("platforms/") || rel_path.starts_with("machines/") {
            continue;
        }
        file_map.insert(rel_path.clone(), source_dir.join(rel_path));
    }

    // Warn on unknown platform names
    let known_platforms = ["linux", "macos", "windows"];
    for name in input.platforms.keys() {
        if !known_platforms.contains(&name.as_str()) {
            diagnostics.push(Diagnostic::UnknownPlatform { name: name.clone() });
        }
    }

    // Apply platform overrides
    let platform_key = match platform.os {
        Os::Linux => "linux",
        Os::MacOs => "macos",
        Os::Windows => "windows",
    };

    if let Some(overrides) = input.platforms.get(platform_key) {
        apply_overrides(&mut file_map, overrides, platform_key, repo_root, &mut diagnostics);
    }

    // Apply machine overrides
    if let Some(machine) = machine_id
        && let Some(overrides) = input.machines.get(machine)
    {
        apply_overrides(&mut file_map, overrides, machine, repo_root, &mut diagnostics);
    }

    Ok((file_map, diagnostics))
}

/// Apply a set of overrides (from a platform or machine layer) to the file map.
///
/// Validates that each override source exists, is not a symlink, and
/// resolves to a path inside the repository root.
fn apply_overrides(
    file_map: &mut HashMap<String, PathBuf>,
    overrides: &HashMap<String, String>,
    layer_name: &str,
    repo_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let canonical_repo = match repo_root.canonicalize() {
        Ok(p) => p,
        Err(_) => return,
    };

    for (target, source) in overrides {
        let source_path = repo_root.join(source);

        // Use symlink_metadata to detect symlinks without following them.
        let metadata = match std::fs::symlink_metadata(&source_path) {
            Ok(m) => m,
            Err(_) => {
                diagnostics.push(Diagnostic::MissingSource {
                    layer: layer_name.to_string(),
                    target: target.clone(),
                    source: source_path,
                });
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            diagnostics.push(Diagnostic::SourceOutsideRepo {
                layer: layer_name.to_string(),
                target: target.clone(),
                source: source_path,
            });
            continue;
        }

        // Verify the canonical path is inside the repo root.
        let canonical_source = match source_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                diagnostics.push(Diagnostic::MissingSource {
                    layer: layer_name.to_string(),
                    target: target.clone(),
                    source: source_path,
                });
                continue;
            }
        };

        if !canonical_source.starts_with(&canonical_repo) {
            diagnostics.push(Diagnostic::SourceOutsideRepo {
                layer: layer_name.to_string(),
                target: target.clone(),
                source: source_path,
            });
            continue;
        }

        file_map.insert(target.clone(), source_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to set up a test repo for file map tests.
    struct MapTestRepo {
        _dir: TempDir,
        repo_root: PathBuf,
    }

    impl MapTestRepo {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let repo_root = dir.path().join("repo");
            fs::create_dir_all(repo_root.join("dotfiles")).unwrap();
            MapTestRepo { _dir: dir, repo_root }
        }

        fn add_file(&self, rel_path: &str, content: &str) {
            let path = self.repo_root.join(rel_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }

        fn linux_platform(&self) -> Platform {
            Platform {
                os: Os::Linux,
                arch: crate::platform::Arch::X86_64,
                distro: None,
                managers: vec![],
            }
        }

        fn macos_platform(&self) -> Platform {
            Platform {
                os: Os::MacOs,
                arch: crate::platform::Arch::Aarch64,
                distro: None,
                managers: vec![],
            }
        }

        fn base_input(&self) -> FileMapInput<'static> {
            // Leaks are fine in tests — avoids lifetime gymnastics
            let platforms: &'static HashMap<String, HashMap<String, String>> =
                Box::leak(Box::default());
            let machines: &'static HashMap<String, HashMap<String, String>> =
                Box::leak(Box::default());
            FileMapInput {
                source: "dotfiles/",
                platforms,
                machines,
            }
        }
    }

    #[test]
    fn base_only_file_map() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# bash config");
        repo.add_file("dotfiles/config/starship.toml", "# starship");

        let input = repo.base_input();
        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        assert!(diagnostics.is_empty());
        assert_eq!(map.len(), 2);
        assert_eq!(map["bashrc"], repo.repo_root.join("dotfiles/bashrc"));
        assert_eq!(
            map["config/starship.toml"],
            repo.repo_root.join("dotfiles/config/starship.toml")
        );
    }

    #[test]
    fn platform_override_replaces_base_file() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/config/alacritty.toml", "# generic");
        repo.add_file("dotfiles/platforms/linux/alacritty.toml", "# linux version");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "config/alacritty.toml".to_string(),
                "dotfiles/platforms/linux/alacritty.toml".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        assert!(diagnostics.is_empty());
        // The override should replace the base source path
        assert_eq!(
            map["config/alacritty.toml"],
            repo.repo_root.join("dotfiles/platforms/linux/alacritty.toml")
        );
    }

    #[test]
    fn platform_override_not_applied_on_different_platform() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/config/alacritty.toml", "# generic");
        repo.add_file("dotfiles/platforms/linux/alacritty.toml", "# linux version");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "config/alacritty.toml".to_string(),
                "dotfiles/platforms/linux/alacritty.toml".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        // On macOS, the linux override should NOT apply
        let (map, _) = build(&input, &repo.repo_root, &repo.macos_platform(), None)
            .expect("should build");

        assert_eq!(
            map["config/alacritty.toml"],
            repo.repo_root.join("dotfiles/config/alacritty.toml")
        );
    }

    #[test]
    fn platform_override_adds_new_file() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# bash");
        repo.add_file("dotfiles/platforms/linux/xresources", "! X11 resources");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "xresources".to_string(),
                "dotfiles/platforms/linux/xresources".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        assert!(diagnostics.is_empty());
        assert_eq!(map.len(), 2); // bashrc + xresources
        assert_eq!(
            map["xresources"],
            repo.repo_root.join("dotfiles/platforms/linux/xresources")
        );
    }

    #[test]
    fn machine_override_replaces_platform_override() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/gitconfig", "# base");
        repo.add_file("dotfiles/platforms/linux/gitconfig", "# linux");
        repo.add_file("dotfiles/machines/work-macbook/gitconfig", "# work");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "gitconfig".to_string(),
                "dotfiles/platforms/linux/gitconfig".to_string(),
            )]),
        )]);
        let machines = HashMap::from([(
            "work-macbook".to_string(),
            HashMap::from([(
                "gitconfig".to_string(),
                "dotfiles/machines/work-macbook/gitconfig".to_string(),
            )]),
        )]);
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(
            &input,
            &repo.repo_root,
            &repo.linux_platform(),
            Some("work-macbook"),
        )
        .expect("should build");

        assert!(diagnostics.is_empty());
        // Machine override wins over platform override
        assert_eq!(
            map["gitconfig"],
            repo.repo_root.join("dotfiles/machines/work-macbook/gitconfig")
        );
    }

    #[test]
    fn missing_override_source_produces_error() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# bash");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "bashrc".to_string(),
                "dotfiles/platforms/linux/bashrc".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        // The override should NOT be applied (source doesn't exist)
        assert_eq!(map["bashrc"], repo.repo_root.join("dotfiles/bashrc"));
        // Should have a MissingSource diagnostic
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(&diagnostics[0], Diagnostic::MissingSource { layer, target, .. }
            if layer == "linux" && target == "bashrc"
        ));
    }

    #[test]
    fn unknown_platform_name_produces_warning() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# bash");
        repo.add_file("dotfiles/platforms/haiku/bashrc", "# haiku");

        let platforms = HashMap::from([(
            "haiku".to_string(),
            HashMap::from([(
                "bashrc".to_string(),
                "dotfiles/platforms/haiku/bashrc".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (_map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        assert!(diagnostics.iter().any(|d| matches!(d,
            Diagnostic::UnknownPlatform { name } if name == "haiku"
        )));
    }

    #[test]
    fn layering_works_for_files_section() {
        let repo = MapTestRepo::new();
        fs::create_dir_all(repo.repo_root.join("files")).unwrap();
        repo.add_file("files/bin/myscript", "#!/bin/sh\n# generic");
        repo.add_file("files/platforms/linux/bin/open", "#!/bin/sh\nxdg-open");

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "bin/open".to_string(),
                "files/platforms/linux/bin/open".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "files/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        assert!(diagnostics.is_empty());
        assert_eq!(map.len(), 2); // myscript + bin/open
        assert_eq!(
            map["bin/open"],
            repo.repo_root.join("files/platforms/linux/bin/open")
        );
    }

    #[test]
    fn override_rejects_path_traversal_outside_repo() {
        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# base");
        // Create a real file outside the repo root that the traversal path resolves to.
        let outside = repo._dir.path().join("secret.txt");
        fs::write(&outside, "SECRET").unwrap();

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "bashrc".to_string(),
                "../secret.txt".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        // The traversal override must be rejected.
        assert_eq!(map["bashrc"], repo.repo_root.join("dotfiles/bashrc"),
            "path traversal should not replace the base file");
        assert!(
            diagnostics.iter().any(|d| matches!(d, Diagnostic::SourceOutsideRepo { .. })),
            "expected SourceOutsideRepo diagnostic, got {diagnostics:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn override_rejects_symlink_source() {
        use std::os::unix::fs::symlink;

        let repo = MapTestRepo::new();
        repo.add_file("dotfiles/bashrc", "# base");
        // Create a symlink inside the repo pointing outside it.
        let outside = repo._dir.path().join("secret.txt");
        fs::write(&outside, "SECRET").unwrap();
        fs::create_dir_all(repo.repo_root.join("dotfiles/platforms/linux")).unwrap();
        symlink(&outside, repo.repo_root.join("dotfiles/platforms/linux/bashrc")).unwrap();

        let platforms = HashMap::from([(
            "linux".to_string(),
            HashMap::from([(
                "bashrc".to_string(),
                "dotfiles/platforms/linux/bashrc".to_string(),
            )]),
        )]);
        let machines = HashMap::new();
        let input = FileMapInput {
            source: "dotfiles/",
            platforms: &platforms,
            machines: &machines,
        };

        let (map, diagnostics) = build(&input, &repo.repo_root, &repo.linux_platform(), None)
            .expect("should build");

        // Symlink override must be rejected; base file preserved.
        assert_eq!(map["bashrc"], repo.repo_root.join("dotfiles/bashrc"),
            "symlink source should not replace the base file");
        assert!(
            diagnostics.iter().any(|d| matches!(d, Diagnostic::SourceOutsideRepo { .. })),
            "expected SourceOutsideRepo diagnostic, got {diagnostics:?}"
        );
    }
}

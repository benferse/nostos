use crate::config;
use crate::reconcile::dotfiles::{expand_tilde, hash_file, walk_dir};
use crate::state::{AppliedEntry, State};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn run(repo: Option<&Path>, path: PathBuf) -> anyhow::Result<ExitCode> {
    let state = State::load().unwrap_or_default();

    // Resolve repo root: explicit flag > state > cwd (if nostos.toml exists)
    let repo_root = if let Some(r) = repo {
        r.to_path_buf()
    } else if let Some(p) = state.repo_path() {
        PathBuf::from(p)
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
        if cwd.join("nostos.toml").exists() {
            cwd
        } else {
            eprintln!("No repo path configured — run `nostos init` first");
            return Ok(ExitCode::from(3));
        }
    };

    let (_, cfg) = match config::find_config_with_repo(Some(&repo_root)) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(3));
        }
    };

    // Canonicalize the target path
    let target_path = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?
            .join(&path)
    };
    let target_path = target_path.canonicalize().map_err(|e| {
        anyhow::anyhow!("cannot resolve path {}: {e}", path.display())
    })?;

    // Directory detection: if path is a directory, enter recursive mode
    if target_path.is_dir() {
        return track_directory(&target_path, &cfg, &repo_root, &state);
    }

    // Try to find the file in state (managed file case)
    if let Some(result) = try_track_managed(&target_path, &cfg, &state, &repo_root)? {
        return Ok(result);
    }

    // New file case — use dot heuristic
    track_new_file(&target_path, &cfg, &repo_root)
}

/// Recursively track all files in a target directory back into the repo.
fn track_directory(
    target_dir: &Path,
    cfg: &config::Config,
    repo_root: &Path,
    state: &State,
) -> anyhow::Result<ExitCode> {
    // Walk the target directory
    let (files, _skips) = walk_dir(target_dir)
        .map_err(|e| anyhow::anyhow!("cannot walk directory {}: {e}", target_dir.display()))?;

    // Resolve configured target directories (expanded and canonicalized)
    let sections = resolve_sections(cfg)?;

    let mut updated = 0u32;
    let mut new_count = 0u32;
    let mut errors: Vec<String> = Vec::new();
    let mut state = state.clone();

    for rel_file in &files {
        let abs_file = target_dir.join(rel_file.replace('/', std::path::MAIN_SEPARATOR_STR));

        // Try to route this file to a config section
        let Some(routing) = route_file(&abs_file, &sections)? else {
            continue;
        };

        // Check if this file is already managed (in state)
        let state_key = &routing.state_key;
        if state.applied.contains_key(state_key) {
            // Managed file: copy target → source, update state
            let source_rel = match state.applied[state_key].source.as_deref() {
                Some(s) => s.to_string(),
                None => routing.source_rel.clone(),
            };
            let source_path = repo_root.join(&source_rel);

            if let Err(e) = copy_file(&abs_file, &source_path) {
                errors.push(format!("{}: {e}", abs_file.display()));
                continue;
            }

            let new_hash = match hash_file(&abs_file) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("{}: cannot hash: {e}", abs_file.display()));
                    continue;
                }
            };
            if let Some(applied) = state.applied.get_mut(state_key) {
                applied.hash = new_hash;
                applied.timestamp = chrono::Utc::now();
            }
            updated += 1;
        } else {
            // New file: copy target → source, create state entry
            let source_path = repo_root.join(&routing.source_rel);

            if let Err(e) = copy_file(&abs_file, &source_path) {
                errors.push(format!("{}: {e}", abs_file.display()));
                continue;
            }

            let new_hash = match hash_file(&abs_file) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("{}: cannot hash: {e}", abs_file.display()));
                    continue;
                }
            };
            state.applied.insert(
                state_key.clone(),
                AppliedEntry {
                    hash: new_hash,
                    timestamp: chrono::Utc::now(),
                    source: Some(routing.source_rel.clone()),
                },
            );
            new_count += 1;
        }
    }

    let total = updated + new_count;
    if total == 0 && errors.is_empty() {
        println!("No files to track in {}", target_dir.display());
        return Ok(ExitCode::SUCCESS);
    }

    if total > 0 && let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    if total > 0 {
        println!("Tracked {total} files: {updated} updated, {new_count} new");
    }

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("error: {err}");
        }
        return Ok(ExitCode::from(1));
    }

    Ok(ExitCode::SUCCESS)
}

/// Routing information for a single file discovered during directory walk.
struct FileRouting {
    /// The key used in state.applied (target-relative with dot prepended).
    state_key: String,
    /// Repo-relative source path (e.g., "dotfiles/config/starship.toml").
    source_rel: String,
}

/// A resolved config section with its expanded target directory.
struct ResolvedSection {
    /// Expanded + canonicalized target directory.
    target_dir: PathBuf,
    /// Source directory relative to repo root (e.g., "dotfiles/").
    source_dir: String,
    /// Whether this is a dotfiles section (dot-prepend/strip applies).
    is_dotfiles: bool,
}

/// Resolve all configured sections into expanded target paths.
fn resolve_sections(cfg: &config::Config) -> anyhow::Result<Vec<ResolvedSection>> {
    let mut sections = Vec::new();

    if let Some(ref df) = cfg.dotfiles {
        let expanded = expand_tilde(&df.target).map_err(|e| anyhow::anyhow!("{e}"))?;
        let canonical = expanded.canonicalize().unwrap_or(expanded);
        sections.push(ResolvedSection {
            target_dir: canonical,
            source_dir: df.source.clone(),
            is_dotfiles: true,
        });
    }

    if let Some(ref f) = cfg.files {
        let expanded = expand_tilde(&f.target).map_err(|e| anyhow::anyhow!("{e}"))?;
        let canonical = expanded.canonicalize().unwrap_or(expanded);
        sections.push(ResolvedSection {
            target_dir: canonical,
            source_dir: f.source.clone(),
            is_dotfiles: false,
        });
    }

    Ok(sections)
}

/// Route a target file to a config section using longest-prefix matching.
fn route_file(
    abs_path: &Path,
    sections: &[ResolvedSection],
) -> anyhow::Result<Option<FileRouting>> {
    // Find the section with the longest matching target prefix
    let mut best: Option<(usize, &ResolvedSection)> = None;

    for section in sections {
        if let Ok(rel) = abs_path.strip_prefix(&section.target_dir) {
            let prefix_len = section.target_dir.as_os_str().len();
            if best.is_none() || prefix_len > best.unwrap().0 {
                best = Some((prefix_len, section));
            }
            let _ = rel; // used only for strip_prefix check
        }
    }

    let Some((_, section)) = best else {
        return Ok(None);
    };

    let rel = abs_path.strip_prefix(&section.target_dir).unwrap();
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    if section.is_dotfiles {
        // rel_str is already target-relative with dot (e.g., ".bashrc", ".config/starship.toml")
        // State key = target-relative path as-is (matches what apply stores)
        let state_key = rel_str.clone();

        // Source = "dotfiles/" + strip_leading_dot(rel_str) = "dotfiles/bashrc"
        let stripped = rel_str.strip_prefix('.').unwrap_or(&rel_str);
        let source_rel = format!("{}{stripped}", section.source_dir);

        Ok(Some(FileRouting {
            state_key,
            source_rel,
        }))
    } else {
        // For [files]: state key = target-relative path, source = "files/" + rel
        let state_key = rel_str.clone();
        let source_rel = format!("{}{rel_str}", section.source_dir);

        Ok(Some(FileRouting {
            state_key,
            source_rel,
        }))
    }
}

/// Copy a file, creating parent directories as needed.
fn copy_file(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create directory {}: {e}", parent.display()))?;
    }
    std::fs::copy(src, dest)
        .map_err(|e| anyhow::anyhow!("cannot copy {} → {}: {e}", src.display(), dest.display()))?;
    Ok(())
}

/// Try to resolve a target path against a config section's target directory,
/// returning the state key and entry if the file is managed.
fn resolve_managed_key<'a>(
    target_path: &Path,
    section_target: &str,
    state: &'a State,
) -> anyhow::Result<Option<(String, &'a crate::state::AppliedEntry)>> {
    let target_base = expand_tilde(section_target)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let target_base = target_base.canonicalize().unwrap_or(target_base);
    if let Ok(rel) = target_path.strip_prefix(&target_base) {
        let key = rel.to_string_lossy().replace('\\', "/");
        if let Some(entry) = state.applied.get(&key) {
            return Ok(Some((key, entry)));
        }
    }
    Ok(None)
}

/// Attempt to track a managed file (one that exists in state.applied).
/// Returns `Some(ExitCode)` if we handled it, `None` if the file is not managed.
fn try_track_managed(
    target_path: &Path,
    cfg: &config::Config,
    state: &State,
    repo_root: &Path,
) -> anyhow::Result<Option<ExitCode>> {
    // Try each config section that has a target directory
    let sections: Vec<&str> = [
        cfg.dotfiles.as_ref().map(|df| df.target.as_str()),
        cfg.files.as_ref().map(|f| f.target.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect();

    for section_target in sections {
        if let Some((key, entry)) = resolve_managed_key(target_path, section_target, state)? {
            return do_track_managed(target_path, entry, &key, repo_root, state)
                .map(Some);
        }
    }

    Ok(None)
}

/// Execute the tracking of a managed file: copy target → source, update state.
fn do_track_managed(
    target_path: &Path,
    entry: &crate::state::AppliedEntry,
    key: &str,
    repo_root: &Path,
    state: &State,
) -> anyhow::Result<ExitCode> {
    let source_rel = match entry.source.as_deref() {
        Some(source_rel) => source_rel,
        None => {
            eprintln!(
                "This file was applied by an older version of nostos before source paths were recorded."
            );
            eprintln!("Run `nostos apply` to refresh state, then re-run `nostos track`.");
            return Ok(ExitCode::from(3));
        }
    };
    let source_path = repo_root.join(source_rel);

    if let Some(parent) = source_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create directory {}: {e}", parent.display()))?;
    }
    std::fs::copy(target_path, &source_path)
        .map_err(|e| anyhow::anyhow!("cannot copy {} → {}: {e}", target_path.display(), source_path.display()))?;

    let new_hash = hash_file(target_path)
        .map_err(|e| anyhow::anyhow!("cannot hash {}: {e}", target_path.display()))?;

    let mut state = state.clone();
    if let Some(applied) = state.applied.get_mut(key) {
        applied.hash = new_hash;
        applied.timestamp = chrono::Utc::now();
    }

    if let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    println!(
        "Tracked {} → {}",
        target_path.display(),
        source_path.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// Copy a file into the repo and print guidance for the user.
fn copy_and_print_guidance(
    target_path: &Path,
    dest: &Path,
    repo_root: &Path,
    section_name: &str,
    repo_filename: &str,
) -> anyhow::Result<ExitCode> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create directory {}: {e}", parent.display()))?;
    }
    std::fs::copy(target_path, dest)
        .map_err(|e| anyhow::anyhow!("cannot copy {}: {e}", target_path.display()))?;

    let rel_dest = dest
        .strip_prefix(repo_root)
        .unwrap_or(dest)
        .display();
    println!("Copied {} → {}", target_path.display(), dest.display());
    println!();
    println!("Add to nostos.toml (file is not yet managed until you do):");
    println!("  # The base source directory already covers this file.");
    println!("  # If you need a platform/machine override, add:");
    println!("  # [{section_name}.platforms.<os>]");
    println!("  # \"{repo_filename}\" = \"{rel_dest}\"");

    Ok(ExitCode::SUCCESS)
}

/// Track a new (unmanaged) file into the repo using the first-segment dot heuristic.
///
/// Routing logic:
/// 1. Compute the path relative to each section's target directory.
/// 2. If the first segment of the relative path starts with `.`, route to `[dotfiles]`
///    and strip the leading dot from that first segment only, preserving nested dirs.
/// 3. Otherwise route to `[files]` verbatim.
fn track_new_file(
    target_path: &Path,
    cfg: &config::Config,
    repo_root: &Path,
) -> anyhow::Result<ExitCode> {
    // Collect configured targets and try to compute a relative path
    let targets: Vec<&str> = [
        cfg.dotfiles.as_ref().map(|df| df.target.as_str()),
        cfg.files.as_ref().map(|f| f.target.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect();

    // Find the relative path from any configured target
    let mut relative: Option<PathBuf> = None;
    for &section_target in &targets {
        let expanded = expand_tilde(section_target).map_err(|e| anyhow::anyhow!("{e}"))?;
        let expanded = expanded.canonicalize().unwrap_or(expanded);
        if let Ok(rel) = target_path.strip_prefix(&expanded) {
            relative = Some(rel.to_path_buf());
            break;
        }
    }

    let rel = match relative {
        Some(r) => r,
        None => {
            eprintln!(
                "Cannot track {}: not under any configured target directory",
                target_path.display()
            );
            return Ok(ExitCode::from(3));
        }
    };

    // Determine routing by checking first segment for leading dot
    let first_segment = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default();

    let needs_dotfiles = first_segment.starts_with('.');

    if needs_dotfiles {
        let df = match cfg.dotfiles {
            Some(ref df) => df,
            None => {
                eprintln!(
                    "Cannot track {}: no [dotfiles] section configured",
                    target_path.display()
                );
                return Ok(ExitCode::from(3));
            }
        };

        // Strip leading dot from first segment, preserve rest of path
        let stripped_first = first_segment.strip_prefix('.').unwrap_or(&first_segment);
        let source_rel: PathBuf = std::iter::once(stripped_first)
            .map(|s| Path::new(s).to_path_buf())
            .chain(rel.components().skip(1).map(|c| PathBuf::from(c.as_os_str())))
            .fold(PathBuf::new(), |acc, p| acc.join(p));
        let source_rel_str = source_rel.to_string_lossy().replace('\\', "/");
        let dest = repo_root.join(&df.source).join(&source_rel);
        copy_and_print_guidance(target_path, &dest, repo_root, "dotfiles", &source_rel_str)
    } else {
        let files = match cfg.files {
            Some(ref f) => f,
            None => {
                eprintln!(
                    "Cannot track {}: no [files] section configured",
                    target_path.display()
                );
                return Ok(ExitCode::from(3));
            }
        };

        let source_rel_str = rel.to_string_lossy().replace('\\', "/");
        let dest = repo_root.join(&files.source).join(&rel);
        copy_and_print_guidance(target_path, &dest, repo_root, "files", &source_rel_str)
    }
}

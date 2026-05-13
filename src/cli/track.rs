use crate::config;
use crate::reconcile::dotfiles::{expand_tilde, hash_file};
use crate::state::State;
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

    // Try to find the file in state (managed file case)
    if let Some(result) = try_track_managed(&target_path, &cfg, &state, &repo_root)? {
        return Ok(result);
    }

    // New file case — use dot heuristic
    track_new_file(&target_path, &cfg, &repo_root)
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
    let sections: Vec<(&str, &str)> = [
        cfg.dotfiles.as_ref().map(|df| (df.target.as_str(), df.source.as_str())),
        cfg.files.as_ref().map(|f| (f.target.as_str(), f.source.as_str())),
    ]
    .into_iter()
    .flatten()
    .collect();

    for (section_target, section_source) in sections {
        if let Some((key, entry)) = resolve_managed_key(target_path, section_target, state)? {
            return do_track_managed(target_path, entry, &key, section_source, repo_root, state)
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
    default_source_dir: &str,
    repo_root: &Path,
    state: &State,
) -> anyhow::Result<ExitCode> {
    // Determine source path from state or derive from convention
    let source_path = if let Some(ref source_rel) = entry.source {
        repo_root.join(source_rel)
    } else {
        let filename = key.strip_prefix('.').unwrap_or(key);
        repo_root.join(default_source_dir).join(filename)
    };

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

/// Track a new (unmanaged) file into the repo.
fn track_new_file(
    target_path: &Path,
    cfg: &config::Config,
    repo_root: &Path,
) -> anyhow::Result<ExitCode> {
    let filename = target_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_dotfile = filename.starts_with('.');

    if is_dotfile {
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

        let stripped = filename.strip_prefix('.').unwrap_or(&filename);
        let dest = repo_root.join(&df.source).join(stripped);
        copy_and_print_guidance(target_path, &dest, repo_root, "dotfiles", stripped)
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

        let dest = repo_root.join(&files.source).join(&filename);
        copy_and_print_guidance(target_path, &dest, repo_root, "files", &filename)
    }
}

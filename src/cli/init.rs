use crate::config;
use crate::state::{MachineInfo, State};
use std::path::PathBuf;
use std::process::ExitCode;

pub fn run(
    url: String,
    machine: Option<String>,
    apply: bool,
    dest: Option<PathBuf>,
) -> anyhow::Result<ExitCode> {
    let dest = dest.unwrap_or(default_dotfiles_path()?);
    let mut state = State::load().unwrap_or_default();

    if let Some(repo_path) = state.repo_path().map(PathBuf::from)
        && repo_path.exists()
    {
        eprintln!(
            "nostos is already initialized at {} — use `nostos sync` to update an existing repo",
            repo_path.display()
        );
        return Ok(ExitCode::from(3));
    }

    // Clone the repository
    println!("Cloning {url} → {}", dest.display());
    if let Err(e) = crate::git::clone(&url, &dest) {
        eprintln!("Error: {e}");
        return Ok(ExitCode::from(3));
    }

    // Set machine identity
    if let Some(ref id) = machine {
        state.machine = Some(MachineInfo { id: id.clone() });
    } else {
        state.ensure_machine_identity();
    }

    // Store the repo path
    state.set_repo_path(dest.to_string_lossy().to_string());

    // Save state
    if let Err(e) = state.save() {
        eprintln!("Warning: failed to save state: {e}");
    }

    // Detect platform for summary
    let platform = crate::platform::detect().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Count files that would be applied
    let file_count = count_plannable_files(&dest);

    // Print summary
    println!();
    println!("Repository: {}", dest.display());
    println!("Platform:   {platform}");
    println!("Machine:    {}", state.machine_id().unwrap_or("unknown"));
    println!("Files:      {file_count} would be applied");

    if apply {
        println!();
        println!("Applying dotfiles…");

        let (config_path, cfg) = match config::find_config_with_repo(Some(&dest)) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error: {e}");
                return Ok(ExitCode::from(3));
            }
        };

        let repo_root = config::repo_root_from_config(&config_path);
        let machine_id = state.machine_id().map(|s| s.to_string());

        let report = match crate::reconcile::apply(
            &cfg,
            &mut state,
            &repo_root,
            &platform,
            machine_id.as_deref(),
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error: {e}");
                return Ok(ExitCode::from(3));
            }
        };

        if let Some(ref dotfiles_report) = report.dotfiles {
            for action in &dotfiles_report.actions {
                match action {
                    crate::reconcile::dotfiles::DotfileAction::NewFile { target, .. } => {
                        println!("  ✓ Copied → {}", target.display());
                    }
                    crate::reconcile::dotfiles::DotfileAction::UpToDate { target } => {
                        println!(
                            "  ✓ {} — up to date",
                            target
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| target.display().to_string())
                        );
                    }
                    _ => {}
                }
            }
            for err in &dotfiles_report.errors {
                eprintln!("  ✗ {err}");
            }
        }

        // Save state again after apply
        if let Err(e) = state.save() {
            eprintln!("Warning: failed to save state: {e}");
        }

        if report.has_warnings() || report.has_errors() {
            return Ok(ExitCode::from(5));
        }
    } else {
        println!();
        println!("Run `nostos plan` to preview changes, then `nostos apply` to apply them.");
    }

    Ok(ExitCode::SUCCESS)
}

/// Default dotfiles directory: `~/.dotfiles`.
///
/// Checks `HOME` first (cross-platform), then falls back to `dirs::home_dir()`.
/// This ensures tests can override the home directory on all platforms,
/// including Windows where `dirs::home_dir()` ignores `HOME`.
fn default_dotfiles_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    Ok(home.join(".dotfiles"))
}

/// Count files in the dotfiles source directory that would be applied.
fn count_plannable_files(repo_root: &std::path::Path) -> usize {
    let config_path = repo_root.join("nostos.toml");
    let cfg = match config::load(&config_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let mut count = 0;

    if let Some(ref df) = cfg.dotfiles {
        let source_dir = repo_root.join(&df.source);
        if source_dir.is_dir() {
            count += count_files_recursive(&source_dir);
        }
    }

    if let Some(ref files) = cfg.files {
        let source_dir = repo_root.join(&files.source);
        if source_dir.is_dir() {
            count += count_files_recursive(&source_dir);
        }
    }

    count
}

fn count_files_recursive(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files_recursive(&path);
            }
        }
    }
    count
}

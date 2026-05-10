//! Git CLI wrapper — shells out to the `git` binary for all git operations.
//!
//! See ADR 0001 for the rationale: using the CLI inherits the user's existing
//! authentication config (SSH agent, credential managers) transparently.

use std::path::Path;
use std::process::Command;

/// Outcome of a [`commit`] call when the operation succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    /// Changes were staged and committed.
    Committed,
    /// The working tree was already clean — nothing to commit.
    NothingToCommit,
}

/// Errors arising from git operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The target path is not inside a git repository.
    #[error("not a git repository: {path}")]
    NotARepo {
        /// The path that was expected to be a git repo.
        path: String,
    },

    /// A merge or rebase conflict occurred during pull.
    #[error("merge conflict in {path} — resolve conflicts with git tools, then re-run nostos")]
    MergeConflict {
        /// The repository path where the conflict occurred.
        path: String,
    },

    /// Git reported an authentication failure.
    #[error("authentication failed for remote in {path} — check your SSH keys or credential manager")]
    AuthFailed {
        /// The repository path.
        path: String,
    },

    /// A network error prevented the git operation from completing.
    #[error("network error in {path} — check your connection and try again")]
    NetworkError {
        /// The repository path.
        path: String,
    },

    /// The git binary was not found on the system.
    #[error("git is not installed or not in PATH")]
    GitNotFound,

    /// A git command failed with an unrecognized error.
    #[error("git {operation} failed in {path}: {message}")]
    Command {
        /// Which git subcommand was run.
        operation: String,
        /// The repository path.
        path: String,
        /// The stderr output from git.
        message: String,
    },
}

/// Check whether `git` is available on the system PATH.
pub fn is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Returns `true` if the working tree at `repo` has no uncommitted changes.
pub fn is_clean(repo: &Path) -> Result<bool, Error> {
    ensure_repo(repo)?;

    let output = run_git(repo, &["status", "--porcelain"])?;
    Ok(output.trim().is_empty())
}

/// Stage all changes and commit with the given message.
///
/// Returns [`CommitOutcome::NothingToCommit`] if the working tree is already
/// clean, rather than treating it as an error.
pub fn commit(repo: &Path, message: &str) -> Result<CommitOutcome, Error> {
    ensure_repo(repo)?;

    run_git(repo, &["add", "--all"])?;

    let output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if output.status.success() {
        return Ok(CommitOutcome::Committed);
    }

    // git commit exits non-zero when there's nothing to commit — detect that
    // from stdout/stderr rather than treating it as a hard error.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let lower = combined.to_lowercase();

    if lower.contains("nothing to commit") || lower.contains("nothing added to commit") {
        return Ok(CommitOutcome::NothingToCommit);
    }

    let path = repo.display().to_string();
    Err(classify_error("commit", &path, combined.trim()))
}

/// Clone a repository from `url` to `dest`.
pub fn clone(url: &str, dest: &Path) -> Result<(), Error> {
    let output = Command::new("git")
        .args(["clone", url])
        .arg(dest)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let dest_str = dest.display().to_string();
        return Err(classify_error("clone", &dest_str, &stderr));
    }

    Ok(())
}

/// Pull with `--rebase` from the tracking remote.
pub fn pull(repo: &Path) -> Result<(), Error> {
    ensure_repo(repo)?;

    let output = Command::new("git")
        .args(["pull", "--rebase"])
        .current_dir(repo)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let path = repo.display().to_string();
        return Err(classify_error("pull", &path, &stderr));
    }

    Ok(())
}

/// Push the current branch to the tracking remote.
pub fn push(repo: &Path) -> Result<(), Error> {
    ensure_repo(repo)?;

    let output = Command::new("git")
        .args(["push"])
        .current_dir(repo)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let path = repo.display().to_string();
        return Err(classify_error("push", &path, &stderr));
    }

    Ok(())
}

// --- internal helpers ---

/// Verify that `path` is inside a git repository.
fn ensure_repo(path: &Path) -> Result<(), Error> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if !output.status.success() {
        return Err(Error::NotARepo {
            path: path.display().to_string(),
        });
    }

    Ok(())
}

/// Run a git command in `repo` and return its stdout on success.
fn run_git(repo: &Path, args: &[&str]) -> Result<String, Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|_| Error::GitNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let operation = args.first().copied().unwrap_or("unknown").to_string();
        let path = repo.display().to_string();
        return Err(classify_error(&operation, &path, &stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Classify a git stderr message into a typed error variant.
fn classify_error(operation: &str, path: &str, stderr: &str) -> Error {
    let lower = stderr.to_lowercase();

    if lower.contains("conflict") || lower.contains("could not apply") {
        return Error::MergeConflict {
            path: path.to_string(),
        };
    }

    if lower.contains("authentication")
        || lower.contains("permission denied")
        || lower.contains("could not read from remote")
    {
        return Error::AuthFailed {
            path: path.to_string(),
        };
    }

    if lower.contains("could not resolve host")
        || lower.contains("unable to access")
        || lower.contains("connection refused")
        || lower.contains("network is unreachable")
    {
        return Error::NetworkError {
            path: path.to_string(),
        };
    }

    Error::Command {
        operation: operation.to_string(),
        path: path.to_string(),
        message: stderr.trim().to_string(),
    }
}

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

    /// A git operation (rebase, merge, cherry-pick) is already in progress.
    #[error("{operation} is in progress in {path} — {guidance}")]
    OperationInProgress {
        /// The type of operation detected (e.g. "A rebase", "A merge").
        operation: String,
        /// The repository path.
        path: String,
        /// Actionable guidance for the user.
        guidance: String,
    },

    /// A merge or rebase conflict occurred during pull.
    #[error("merge conflict in {path} — resolve conflicts with git tools, then re-run nostos")]
    MergeConflict {
        /// The repository path where the conflict occurred.
        path: String,
    },

    /// Git reported an authentication failure.
    #[error(
        "authentication failed for remote in {path} — check your SSH keys or credential manager"
    )]
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

    /// The repository is on a detached HEAD and not currently on a branch.
    #[error("detached HEAD in {path} — check out a branch, then re-run nostos")]
    NotOnBranch {
        /// The repository path.
        path: String,
    },

    /// The current branch has no upstream configured.
    #[error("current branch has no upstream in {path} — run `git push -u origin <branch>` first")]
    NoUpstream {
        /// The repository path.
        path: String,
    },

    /// The repository has no remote configured.
    #[error(
        "no remote configured for repository in {path} — add one with `git remote add origin <url>`"
    )]
    NoRemote {
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

/// Check for in-progress git operations (rebase, merge, cherry-pick).
///
/// Returns `Ok(())` if no operation is in progress, or an
/// [`Error::OperationInProgress`] with actionable guidance if one is detected.
pub fn check_in_progress(repo: &Path) -> Result<(), Error> {
    let git_dir = resolve_git_dir(repo)?;

    if git_dir.join("rebase-merge").is_dir() {
        return Err(Error::OperationInProgress {
            operation: "A rebase".to_string(),
            path: repo.display().to_string(),
            guidance: "resolve conflicts, then run `git rebase --continue` \
                       (or `git rebase --abort`)"
                .to_string(),
        });
    }

    if git_dir.join("rebase-apply").is_dir() {
        return Err(Error::OperationInProgress {
            operation: "A rebase".to_string(),
            path: repo.display().to_string(),
            guidance: "resolve conflicts, then run `git rebase --continue` \
                       (or `git rebase --abort`)"
                .to_string(),
        });
    }

    if git_dir.join("MERGE_HEAD").exists() {
        return Err(Error::OperationInProgress {
            operation: "A merge".to_string(),
            path: repo.display().to_string(),
            guidance: "resolve conflicts, then run `git merge --continue` \
                       (or `git merge --abort`)"
                .to_string(),
        });
    }

    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Err(Error::OperationInProgress {
            operation: "A cherry-pick".to_string(),
            path: repo.display().to_string(),
            guidance: "resolve conflicts, then run `git cherry-pick --continue` \
                       (or `git cherry-pick --abort`)"
                .to_string(),
        });
    }

    Ok(())
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

/// Resolve the `.git` directory for `path`, respecting worktrees.
fn resolve_git_dir(path: &Path) -> Result<std::path::PathBuf, Error> {
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

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let git_dir = std::path::Path::new(&raw);

    // git rev-parse --git-dir may return a relative path
    if git_dir.is_absolute() {
        Ok(git_dir.to_path_buf())
    } else {
        Ok(path.join(git_dir))
    }
}

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
        || lower.contains("permission denied (publickey")
        || lower.contains("could not read from remote")
    {
        return Error::AuthFailed {
            path: path.to_string(),
        };
    }

    if lower.contains("not currently on a branch") {
        return Error::NotOnBranch {
            path: path.to_string(),
        };
    }

    if lower.contains("has no upstream branch") || lower.contains("no tracking information") {
        return Error::NoUpstream {
            path: path.to_string(),
        };
    }

    if lower.contains("no remote repository specified")
        || lower.contains("no configured push destination")
    {
        return Error::NoRemote {
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

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "/repo";

    #[test]
    fn classify_error_detects_detached_head() {
        let err = classify_error("pull", PATH, "You are not currently on a branch.");

        assert!(matches!(err, Error::NotOnBranch { .. }));
        assert_eq!(
            err.to_string(),
            "detached HEAD in /repo — check out a branch, then re-run nostos"
        );
    }

    #[test]
    fn classify_error_detects_missing_upstream() {
        let err = classify_error(
            "push",
            PATH,
            "fatal: The current branch main has no upstream branch.",
        );

        assert!(matches!(err, Error::NoUpstream { .. }));
        assert_eq!(
            err.to_string(),
            "current branch has no upstream in /repo — run `git push -u origin <branch>` first"
        );
    }

    #[test]
    fn classify_error_detects_missing_tracking_information() {
        let err = classify_error(
            "pull",
            PATH,
            "There is no tracking information for the current branch.",
        );

        assert!(matches!(err, Error::NoUpstream { .. }));
    }

    #[test]
    fn classify_error_detects_missing_remote() {
        let err = classify_error("push", PATH, "fatal: No configured push destination.");

        assert!(matches!(err, Error::NoRemote { .. }));
        assert_eq!(
            err.to_string(),
            "no remote configured for repository in /repo — add one with `git remote add origin <url>`"
        );
    }
}

//! Integration tests for the `git` module.
//!
//! These tests shell out to real `git` — they will be skipped if `git` is not
//! available on the system.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use nostos::git;

/// Skip the test if git is not installed.
macro_rules! require_git {
    () => {
        if !git::is_available() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
    };
}

/// Create a fresh git repo in a temp dir with user config set.
fn init_repo(dir: &Path) {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir)
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .expect("git config user.name failed");
    Command::new("git")
        .args(["config", "user.email", "test@test.local"])
        .current_dir(dir)
        .output()
        .expect("git config user.email failed");
}

/// Create a bare remote repo and clone it into a working directory, returning
/// both temp dirs and the working path.
fn setup_remote_and_clone() -> (TempDir, TempDir) {
    let remote_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .current_dir(remote_dir.path())
        .output()
        .expect("git init --bare failed");

    let work_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["clone", remote_dir.path().to_str().unwrap(), "."])
        .current_dir(work_dir.path())
        .output()
        .expect("git clone failed");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.local"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();

    (remote_dir, work_dir)
}

// --- Tracer bullet: is_clean on a clean repo ---

#[test]
fn is_clean_returns_true_on_clean_repo() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    // Make an initial commit so HEAD exists
    fs::write(dir.path().join("README.md"), "hello").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(git::is_clean(dir.path()).unwrap());
}

#[test]
fn is_clean_returns_false_when_files_modified() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    fs::write(dir.path().join("README.md"), "hello").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Now modify a file
    fs::write(dir.path().join("README.md"), "changed").unwrap();
    assert!(!git::is_clean(dir.path()).unwrap());
}

#[test]
fn commit_stages_and_commits_changes() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    fs::write(dir.path().join("file.txt"), "content").unwrap();
    let result = git::commit(dir.path(), "add file").unwrap();
    assert_eq!(result, git::CommitOutcome::Committed);
    assert!(git::is_clean(dir.path()).unwrap());
}

#[test]
fn commit_returns_nothing_to_commit_on_clean_tree() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    fs::write(dir.path().join("file.txt"), "content").unwrap();
    git::commit(dir.path(), "initial").unwrap();

    let result = git::commit(dir.path(), "should be noop").unwrap();
    assert_eq!(result, git::CommitOutcome::NothingToCommit);
}

#[test]
fn changed_paths_lists_repo_relative_files() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    fs::create_dir_all(dir.path().join("dotfiles")).unwrap();
    fs::write(dir.path().join("dotfiles/bashrc"), "content").unwrap();
    fs::write(dir.path().join("gitconfig"), "content").unwrap();

    let paths = git::changed_paths(dir.path()).unwrap();

    assert_eq!(paths, vec!["dotfiles/bashrc", "gitconfig"]);
}

#[test]
fn changed_paths_reports_rename_destination() {
    require_git!();
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());

    fs::write(dir.path().join("old name.txt"), "content").unwrap();
    git::commit(dir.path(), "initial").unwrap();
    fs::rename(
        dir.path().join("old name.txt"),
        dir.path().join("new name.txt"),
    )
    .unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(dir.path())
        .output()
        .expect("git add failed");

    let paths = git::changed_paths(dir.path()).unwrap();

    assert_eq!(paths, vec!["new name.txt"]);
}

#[test]
fn clone_creates_working_copy() {
    require_git!();
    let remote_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .current_dir(remote_dir.path())
        .output()
        .expect("git init --bare failed");

    let dest_dir = TempDir::new().unwrap();
    let clone_path = dest_dir.path().join("repo");

    git::clone(remote_dir.path().to_str().unwrap(), &clone_path).unwrap();
    assert!(clone_path.join(".git").exists());
}

#[test]
fn pull_fast_forward_succeeds() {
    require_git!();
    let (remote_dir, work_dir) = setup_remote_and_clone();

    // Push an initial commit from a second clone so there's something to pull
    let pusher_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["clone", remote_dir.path().to_str().unwrap(), "."])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Pusher"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "pusher@test.local"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    fs::write(pusher_dir.path().join("file.txt"), "from pusher").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "upstream commit"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["push"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();

    // Now pull from the original working copy
    git::pull(work_dir.path()).unwrap();
    assert!(work_dir.path().join("file.txt").exists());
}

#[test]
fn pull_with_conflict_returns_merge_conflict_error() {
    require_git!();
    let (remote_dir, work_dir) = setup_remote_and_clone();

    // Push an initial commit from a second clone
    let pusher_dir = TempDir::new().unwrap();
    Command::new("git")
        .args(["clone", remote_dir.path().to_str().unwrap(), "."])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Pusher"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "pusher@test.local"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    fs::write(pusher_dir.path().join("file.txt"), "version A").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "A"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["push"])
        .current_dir(pusher_dir.path())
        .output()
        .unwrap();

    // Create a conflicting commit locally
    fs::write(work_dir.path().join("file.txt"), "version B").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "B"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();

    let err = git::pull(work_dir.path()).unwrap_err();
    assert!(
        matches!(err, git::Error::MergeConflict { .. }),
        "expected MergeConflict, got: {err:?}"
    );

    // Clean up the failed rebase so temp dir can be deleted
    Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(work_dir.path())
        .output()
        .ok();
}

#[test]
fn push_sends_commits_to_remote() {
    require_git!();
    let (_remote_dir, work_dir) = setup_remote_and_clone();

    fs::write(work_dir.path().join("pushed.txt"), "data").unwrap();
    Command::new("git")
        .args(["add", "--all"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "to push"])
        .current_dir(work_dir.path())
        .output()
        .unwrap();

    git::push(work_dir.path()).unwrap();
}

#[test]
fn not_a_repo_returns_typed_error() {
    require_git!();
    let dir = TempDir::new().unwrap();
    // dir is NOT a git repo

    let err = git::is_clean(dir.path()).unwrap_err();
    assert!(
        matches!(err, git::Error::NotARepo { .. }),
        "expected NotARepo, got: {err:?}"
    );
}

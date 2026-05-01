//! CLI integration tests — single-command tests for status, plan, apply.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper to set up a temp repo directory with nostos.toml and dotfiles.
struct TestFixture {
    dir: TempDir,
}

impl TestFixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("dotfiles")).unwrap();
        Self { dir }
    }

    fn with_config(self, config: &str) -> Self {
        fs::write(self.dir.path().join("nostos.toml"), config).unwrap();
        self
    }

    fn with_default_config(self) -> Self {
        let target = self.target_dir().to_string_lossy().to_string();
        self.with_config(&format!(
            "[dotfiles]\nsource = \"dotfiles/\"\ntarget = \"{target}\"\n"
        ))
    }

    fn add_source(&self, rel_path: &str, content: &str) {
        let path = self.dir.path().join("dotfiles").join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn target_dir(&self) -> std::path::PathBuf {
        let t = self.dir.path().join("home");
        fs::create_dir_all(&t).unwrap();
        t
    }

    fn add_target(&self, dot_path: &str, content: &str) {
        let path = self.target_dir().join(dot_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn read_target(&self, dot_path: &str) -> String {
        fs::read_to_string(self.target_dir().join(dot_path)).unwrap()
    }

    fn nostos(&self) -> Command {
        let mut cmd = Command::cargo_bin("nostos").unwrap();
        cmd.current_dir(self.dir.path());
        // Override state dir to avoid polluting real config
        cmd.env(
            "XDG_CONFIG_HOME",
            self.dir.path().join("xdg-config").to_str().unwrap(),
        );
        cmd
    }
}

// ── Status tests ─────────────────────────────────────────

#[test]
fn status_shows_platform_info() {
    let fix = TestFixture::new().with_default_config();
    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Platform:"));
}

#[test]
fn status_with_no_config() {
    let fix = TestFixture::new();
    // No nostos.toml
    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("not found"));
}

#[test]
fn status_with_invalid_config() {
    let fix = TestFixture::new().with_config("invalid toml [[[");
    fix.nostos()
        .arg("status")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("parse"));
}

// ── Plan tests ───────────────────────────────────────────

#[test]
fn plan_shows_new_files() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# bash config");

    fix.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(predicate::str::contains("new file"));
}

#[test]
fn plan_with_no_config_exits_3() {
    let fix = TestFixture::new();
    fix.nostos().arg("plan").assert().code(3);
}

#[test]
fn plan_with_mixed_states() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("newfile", "new content");
    fix.add_source("existing", "same content");
    fix.add_target(".existing", "same content");

    fix.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("new file").and(predicate::str::contains("up to date").or(
                // Unmanaged existing file with matching content
                predicate::str::contains("conflict"),
            )),
        );
}

// ── Apply tests ──────────────────────────────────────────

#[test]
fn apply_copies_files() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# bash config");

    fix.nostos()
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    assert_eq!(fix.read_target(".bashrc"), "# bash config");
}

#[test]
fn apply_idempotent() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# bash config");

    // First apply
    fix.nostos().arg("apply").assert().success();

    // Second apply — should show up to date
    fix.nostos()
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"));
}

#[test]
fn apply_local_modification_skips_and_exits_5() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# original");

    // First apply
    fix.nostos().arg("apply").assert().success();

    // User edits the file
    fix.add_target(".bashrc", "# user edited");

    // Second apply — should skip and exit 5
    fix.nostos()
        .arg("apply")
        .assert()
        .code(5)
        .stdout(predicate::str::contains("local modification"));
}

#[test]
fn apply_conflict_creates_backup() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# version 1");

    // First apply
    fix.nostos().arg("apply").assert().success();

    // User edits target AND source changes
    fix.add_target(".bashrc", "# user version");
    fix.add_source("bashrc", "# repo version 2");

    // Apply again — conflict
    fix.nostos()
        .arg("apply")
        .assert()
        .code(5)
        .stdout(predicate::str::contains("Backed up"));

    // Target should have new repo content
    assert_eq!(fix.read_target(".bashrc"), "# repo version 2");

    // A backup file should exist
    let home = fix.target_dir();
    let backups: Vec<_> = fs::read_dir(&home)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("nostos-backup")
        })
        .collect();
    assert!(!backups.is_empty(), "backup file should exist");
}

#[test]
fn no_subcommand_shows_help() {
    Command::cargo_bin("nostos")
        .unwrap()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage"));
}

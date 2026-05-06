//! End-to-end workflow tests — multi-step sequences simulating real user
//! workflows.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper for workflow tests. Creates a realistic repo layout.
struct Workflow {
    dir: TempDir,
}

impl Workflow {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("dotfiles")).unwrap();
        fs::create_dir_all(dir.path().join("home")).unwrap();
        Self { dir }
    }

    fn write_config(&self) {
        let target = self.dir.path().join("home").to_string_lossy().to_string();
        let config = format!(
            "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n",
        );
        fs::write(self.dir.path().join("nostos.toml"), config).unwrap();
    }

    fn add_source(&self, rel_path: &str, content: &str) {
        let path = self.dir.path().join("dotfiles").join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn edit_target(&self, dot_path: &str, content: &str) {
        let path = self.dir.path().join("home").join(dot_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn read_target(&self, dot_path: &str) -> String {
        fs::read_to_string(self.dir.path().join("home").join(dot_path)).unwrap()
    }

    fn target_exists(&self, dot_path: &str) -> bool {
        self.dir.path().join("home").join(dot_path).exists()
    }

    fn nostos(&self) -> Command {
        let mut cmd = Command::cargo_bin("nostos").unwrap();
        cmd.current_dir(self.dir.path());
        cmd.env(
            "XDG_CONFIG_HOME",
            self.dir.path().join("xdg-config").to_str().unwrap(),
        );
        cmd
    }
}

// ── Workflow 1: Fresh machine setup ─────────────────────

#[test]
fn workflow_fresh_machine_setup() {
    let w = Workflow::new();
    w.write_config();
    w.add_source("bashrc", "# bash config");
    w.add_source("gitconfig", "[user]\n  name = Test");
    w.add_source("config/starship.toml", "format = \"$all\"");

    // Plan shows 3 new files
    w.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("new file")
                .count(3),
        );

    // Apply copies all 3
    w.nostos()
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied").count(3));

    // Verify files exist with correct content
    assert_eq!(w.read_target(".bashrc"), "# bash config");
    assert_eq!(w.read_target(".gitconfig"), "[user]\n  name = Test");
    assert_eq!(
        w.read_target(".config/starship.toml"),
        "format = \"$all\""
    );

    // Plan again — all up to date
    w.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date").count(3));
}

// ── Workflow 2: Edit-and-reapply cycle ──────────────────

#[test]
fn workflow_edit_and_reapply() {
    let w = Workflow::new();
    w.write_config();
    w.add_source("bashrc", "# original bash config");

    // Apply
    w.nostos().arg("apply").assert().success();

    // User edits the file
    w.edit_target(".bashrc", "# user customized bash config");

    // Plan shows local modification
    w.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(predicate::str::contains("local modification"));

    // Apply skips, exits 5
    w.nostos()
        .arg("apply")
        .assert()
        .code(5)
        .stdout(predicate::str::contains("local modification"));

    // User's file is preserved
    assert_eq!(w.read_target(".bashrc"), "# user customized bash config");
}

// ── Workflow 3: Repo update cycle ───────────────────────

#[test]
fn workflow_repo_update_cycle() {
    let w = Workflow::new();
    w.write_config();
    w.add_source("bashrc", "# version 1");

    // Apply v1
    w.nostos().arg("apply").assert().success();
    assert_eq!(w.read_target(".bashrc"), "# version 1");

    // Simulate git pull — source changes
    w.add_source("bashrc", "# version 2 with new aliases");

    // Plan shows clean update
    w.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(predicate::str::contains("clean update"));

    // Apply updates
    w.nostos()
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated"));

    assert_eq!(w.read_target(".bashrc"), "# version 2 with new aliases");

    // Plan again — up to date
    w.nostos()
        .arg("plan")
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"));
}

// ── Workflow 4: Conflict with backup ────────────────────

#[test]
fn workflow_conflict_with_backup() {
    let w = Workflow::new();
    w.write_config();
    w.add_source("bashrc", "# original");

    // Apply
    w.nostos().arg("apply").assert().success();

    // Both sides change
    w.edit_target(".bashrc", "# user custom version");
    w.add_source("bashrc", "# repo updated version");

    // Apply — conflict, backup created
    w.nostos()
        .arg("apply")
        .assert()
        .code(5)
        .stdout(predicate::str::contains("Backed up"));

    // Target has repo version
    assert_eq!(w.read_target(".bashrc"), "# repo updated version");

    // Backup exists with user's version
    let home = w.dir.path().join("home");
    let backups: Vec<_> = fs::read_dir(&home)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("nostos-backup")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let backup_content = fs::read_to_string(backups[0].path()).unwrap();
    assert_eq!(backup_content, "# user custom version");
}

// ── Workflow 5: Mixed-state batch ───────────────────────

#[test]
fn workflow_mixed_state_batch() {
    let w = Workflow::new();
    w.write_config();

    // Start with 3 files
    w.add_source("bashrc", "bash v1");
    w.add_source("gitconfig", "git v1");
    w.add_source("vimrc", "vim v1");

    // Apply all 3
    w.nostos().arg("apply").assert().success();

    // Now add 2 new source files
    w.add_source("tmux.conf", "tmux config");
    w.add_source("config/starship.toml", "starship config");

    // Edit one target (local mod)
    w.edit_target(".vimrc", "vim user edit");

    // Update one source (clean update)
    w.add_source("gitconfig", "git v2");

    // Plan should show mix of states
    let output = w
        .nostos()
        .arg("plan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    assert!(stdout.contains("new file"), "should have new files");
    assert!(stdout.contains("up to date"), "bashrc should be up to date");
    assert!(
        stdout.contains("local modification"),
        "vimrc should show local mod"
    );
    assert!(
        stdout.contains("clean update"),
        "gitconfig should show clean update"
    );
}

// ── Workflow 6: Nested directory creation ────────────────

#[test]
fn workflow_nested_directory_creation() {
    let w = Workflow::new();
    w.write_config();
    w.add_source(
        "config/alacritty/alacritty.toml",
        "[window]\npadding = { x = 5, y = 5 }",
    );

    // Apply
    w.nostos().arg("apply").assert().success();

    // Verify deeply nested target was created
    assert!(w.target_exists(".config/alacritty/alacritty.toml"));
    assert_eq!(
        w.read_target(".config/alacritty/alacritty.toml"),
        "[window]\npadding = { x = 5, y = 5 }"
    );
}

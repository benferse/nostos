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
            "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n"
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

/// Helper to create a bare git repo to use as a fake "remote" for init tests.
fn create_bare_repo(dir: &std::path::Path) -> std::path::PathBuf {
    let bare = dir.join("remote.git");
    std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .expect("git init --bare");

    // Create a non-bare clone, add a nostos.toml + dotfiles, push
    let work = dir.join("work");
    std::process::Command::new("git")
        .args(["clone"])
        .arg(&bare)
        .arg(&work)
        .output()
        .expect("git clone");

    fs::create_dir_all(work.join("dotfiles")).unwrap();
    fs::write(work.join("dotfiles/bashrc"), "# bash config\n").unwrap();

    // nostos.toml — use a placeholder target; init tests that apply will
    // override HOME so the target resolves under the temp tree.
    fs::write(
        work.join("nostos.toml"),
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = \"~/\"\n",
    )
    .unwrap();

    // git add + commit + push
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&work)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args([
            "-c", "user.name=test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "initial",
        ])
        .current_dir(&work)
        .output()
        .expect("git commit");
    std::process::Command::new("git")
        .args(["push"])
        .current_dir(&work)
        .output()
        .expect("git push");

    bare
}

// ── Init tests ───────────────────────────────────────────

#[test]
fn init_clones_repo_and_sets_state() {
    let dir = TempDir::new().unwrap();
    let bare = create_bare_repo(dir.path());
    let fake_home = dir.path().join("fakehome");
    fs::create_dir_all(&fake_home).unwrap();

    Command::cargo_bin("nostos")
        .unwrap()
        .env("HOME", &fake_home)
        .env(
            "XDG_CONFIG_HOME",
            fake_home.join("xdg-config").to_str().unwrap(),
        )
        .arg("init")
        .arg(bare.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository:"))
        .stdout(predicate::str::contains("Platform:"))
        .stdout(predicate::str::contains("Machine:"));

    // .dotfiles should exist under fake home
    assert!(fake_home.join(".dotfiles").exists());

    // State file should contain repo path and machine identity
    let state_path = fake_home.join("xdg-config/nostos/state.toml");
    let state_content = fs::read_to_string(&state_path).unwrap();
    assert!(state_content.contains(".dotfiles"));
    assert!(state_content.contains("[machine]"));
}

#[test]
fn init_with_machine_flag() {
    let dir = TempDir::new().unwrap();
    let bare = create_bare_repo(dir.path());
    let fake_home = dir.path().join("fakehome");
    fs::create_dir_all(&fake_home).unwrap();

    Command::cargo_bin("nostos")
        .unwrap()
        .env("HOME", &fake_home)
        .env(
            "XDG_CONFIG_HOME",
            fake_home.join("xdg-config").to_str().unwrap(),
        )
        .arg("init")
        .arg(bare.to_str().unwrap())
        .arg("--machine")
        .arg("work-laptop")
        .assert()
        .success()
        .stdout(predicate::str::contains("Machine:    work-laptop"));

    let state_path = fake_home.join("xdg-config/nostos/state.toml");
    let state_content = fs::read_to_string(&state_path).unwrap();
    assert!(state_content.contains("work-laptop"));
}

#[test]
fn init_target_exists_errors() {
    let dir = TempDir::new().unwrap();
    let bare = create_bare_repo(dir.path());
    let fake_home = dir.path().join("fakehome");
    fs::create_dir_all(fake_home.join(".dotfiles")).unwrap();

    Command::cargo_bin("nostos")
        .unwrap()
        .env("HOME", &fake_home)
        .env(
            "XDG_CONFIG_HOME",
            fake_home.join("xdg-config").to_str().unwrap(),
        )
        .arg("init")
        .arg(bare.to_str().unwrap())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Target directory already exists"));
}

#[test]
fn init_with_apply_copies_files() {
    let dir = TempDir::new().unwrap();
    let fake_home = dir.path().join("fakehome");
    fs::create_dir_all(&fake_home).unwrap();

    // Create a bare repo with a nostos.toml that targets the fake home
    let bare = dir.path().join("remote.git");
    std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .expect("git init --bare");

    let work = dir.path().join("work");
    std::process::Command::new("git")
        .args(["clone"])
        .arg(&bare)
        .arg(&work)
        .output()
        .expect("git clone");

    fs::create_dir_all(work.join("dotfiles")).unwrap();
    fs::write(work.join("dotfiles/bashrc"), "# from init --apply\n").unwrap();

    // Use absolute target path so apply works predictably
    let target = fake_home.to_string_lossy().to_string();
    fs::write(
        work.join("nostos.toml"),
        format!("[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n"),
    )
    .unwrap();

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&work)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args([
            "-c", "user.name=test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "initial",
        ])
        .current_dir(&work)
        .output()
        .expect("git commit");
    std::process::Command::new("git")
        .args(["push"])
        .current_dir(&work)
        .output()
        .expect("git push");

    Command::cargo_bin("nostos")
        .unwrap()
        .env("HOME", &fake_home)
        .env(
            "XDG_CONFIG_HOME",
            fake_home.join("xdg-config").to_str().unwrap(),
        )
        .arg("init")
        .arg(bare.to_str().unwrap())
        .arg("--apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("Applying"))
        .stdout(predicate::str::contains("Copied"));

    let content = fs::read_to_string(fake_home.join(".bashrc")).unwrap();
    // Normalise line endings — git on Windows may convert \n → \r\n
    let content = content.replace("\r\n", "\n");
    assert_eq!(content, "# from init --apply\n");
}

#[test]
fn init_suggests_plan_without_apply() {
    let dir = TempDir::new().unwrap();
    let bare = create_bare_repo(dir.path());
    let fake_home = dir.path().join("fakehome");
    fs::create_dir_all(&fake_home).unwrap();

    Command::cargo_bin("nostos")
        .unwrap()
        .env("HOME", &fake_home)
        .env(
            "XDG_CONFIG_HOME",
            fake_home.join("xdg-config").to_str().unwrap(),
        )
        .arg("init")
        .arg(bare.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("nostos plan"));
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

// ── --repo flag tests ────────────────────────────────────

#[test]
fn apply_with_repo_flag() {
    let fixture = TestFixture::new().with_default_config();
    fixture.add_source("bashrc", "# bash config");

    // Run from a different directory using --repo
    let other_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("--repo")
        .arg(fixture.dir.path())
        .arg("apply")
        .current_dir(other_dir.path())
        .assert()
        .success();

    // Verify file was placed
    assert!(fixture.target_dir().join(".bashrc").exists());
}

#[test]
fn plan_with_repo_flag() {
    let fixture = TestFixture::new().with_default_config();
    fixture.add_source("bashrc", "# bash config");

    let other_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("--repo")
        .arg(fixture.dir.path())
        .arg("plan")
        .current_dir(other_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("new file"));
}

#[test]
fn status_with_repo_flag() {
    let fixture = TestFixture::new().with_default_config();

    let other_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("--repo")
        .arg(fixture.dir.path())
        .arg("status")
        .current_dir(other_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn repo_flag_nonexistent_path() {
    Command::cargo_bin("nostos")
        .unwrap()
        .arg("--repo")
        .arg("/nonexistent/path/xyz")
        .arg("plan")
        .assert()
        .failure();
}

#[test]
fn plan_with_tools_shows_not_yet_implemented() {
    let fixture = TestFixture::new();
    let target = fixture.target_dir().to_string_lossy().to_string();
    let config = format!(
        r#"[dotfiles]
source = "dotfiles/"
target = '{target}'

[[tool]]
name = "ripgrep"
bin = "rg"

[[tool]]
name = "fd"
install.apt = "fd-find"
"#
    );
    fs::write(fixture.dir.path().join("nostos.toml"), config).unwrap();
    fixture.add_source("bashrc", "# config");

    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("plan")
        .current_dir(fixture.dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("2 tool(s) declared but tool installation is not yet implemented"));
}

#[test]
fn plan_with_hooks_shows_not_yet_implemented() {
    let fixture = TestFixture::new();
    let target = fixture.target_dir().to_string_lossy().to_string();
    let config = format!(
        r#"[dotfiles]
source = "dotfiles/"
target = '{target}'

[[hook]]
name = "setup"
run = "hooks/setup.sh"
when = "pre-apply"
"#
    );
    fs::write(fixture.dir.path().join("nostos.toml"), config).unwrap();
    fixture.add_source("bashrc", "# config");

    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("plan")
        .current_dir(fixture.dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("hook execution is not yet implemented"));
}

#[test]
fn status_shows_platform_and_managers() {
    let fixture = TestFixture::new().with_default_config();

    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("status")
        .current_dir(fixture.dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Platform:"))
        .stdout(predicate::str::contains("Managers:"));
}

#[test]
fn config_only_tools_no_dotfiles_is_valid() {
    let fixture = TestFixture::new();
    let config = r#"
[[tool]]
name = "ripgrep"
bin = "rg"
"#;
    fs::write(fixture.dir.path().join("nostos.toml"), config).unwrap();

    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        fixture.dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("plan")
        .current_dir(fixture.dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("tool(s) declared"));
}

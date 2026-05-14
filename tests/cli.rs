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
fn status_shows_machine_not_set() {
    let fix = TestFixture::new().with_default_config();
    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Machine:   (not set)"));
}

#[test]
fn status_shows_repo_not_set() {
    let fix = TestFixture::new().with_default_config();
    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Repo:      (not set"));
}

#[test]
fn status_shows_base_layer() {
    let fix = TestFixture::new().with_default_config();
    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Layers:    base"));
}

#[test]
fn status_shows_active_platform_layer() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();

    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    };

    let config = format!(
        r#"[dotfiles]
source = "dotfiles/"
target = '{target}'

[dotfiles.platforms.{platform}]
"bashrc" = "dotfiles/platforms/{platform}/bashrc"
"#
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("dotfiles")).unwrap();

    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Layers:    base + {platform}")));
}

#[test]
fn status_empty_platform_section_not_counted_as_layer() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();

    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    };

    // Platform section exists but has no entries
    let config = format!(
        r#"[dotfiles]
source = "dotfiles/"
target = '{target}'

[dotfiles.platforms.{platform}]
"#
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("dotfiles")).unwrap();

    fix.nostos()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Layers:    base"))
        .stdout(predicate::str::contains(format!("base + {platform}")).not());
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

// ── Track tests ──────────────────────────────────────────

#[test]
fn track_managed_file_updates_source_and_state() {
    let fix = TestFixture::new().with_default_config();
    fix.add_source("bashrc", "# original");

    // Apply first to create state
    fix.nostos().arg("apply").assert().success();
    assert_eq!(fix.read_target(".bashrc"), "# original");

    // Edit the target file
    fix.add_target(".bashrc", "# user edited version");

    // Track it back
    let target_path = fix.target_dir().join(".bashrc");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Tracked"));

    // Verify the source was updated
    let source_content =
        fs::read_to_string(fix.dir.path().join("dotfiles").join("bashrc")).unwrap();
    assert_eq!(source_content, "# user edited version");

    // Verify a subsequent apply sees no changes
    fix.nostos()
        .arg("apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"));
}

#[test]
fn track_new_dotfile_copies_without_dot() {
    let fix = TestFixture::new().with_default_config();

    // Create a dotfile in the target directory (not yet managed)
    fix.add_target(".newrc", "# brand new dotfile");

    let target_path = fix.target_dir().join(".newrc");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Verify file went to dotfiles source dir without the dot prefix
    let source_path = fix.dir.path().join("dotfiles").join("newrc");
    assert!(source_path.exists(), "source file should exist without dot");
    assert_eq!(fs::read_to_string(&source_path).unwrap(), "# brand new dotfile");
}

#[test]
fn track_new_plain_file_copies_to_files_dir() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!(
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n\n[files]\nsource = \"files/\"\ntarget = '{target}'\n"
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("files")).unwrap();

    // Create a non-dot file in the target directory
    fix.add_target("myscript", "#!/bin/sh\necho hello");

    let target_path = fix.target_dir().join("myscript");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Verify file went to files source dir with the same name
    let source_path = fix.dir.path().join("files").join("myscript");
    assert!(source_path.exists(), "source file should exist in files dir");
    assert_eq!(
        fs::read_to_string(&source_path).unwrap(),
        "#!/bin/sh\necho hello"
    );
}

#[test]
fn track_new_nested_dotfile_preserves_directory_structure() {
    let fix = TestFixture::new().with_default_config();

    // Create ~/.config/nvim/init.vim in target (first segment .config starts with dot)
    fix.add_target(".config/nvim/init.vim", "\" nvim config");

    let target_path = fix.target_dir().join(".config/nvim/init.vim");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Should land at dotfiles/config/nvim/init.vim (dot stripped from first segment only)
    let source_path = fix.dir.path().join("dotfiles/config/nvim/init.vim");
    assert!(
        source_path.exists(),
        "nested dotfile should preserve directory structure"
    );
    assert_eq!(fs::read_to_string(&source_path).unwrap(), "\" nvim config");
}

#[test]
fn track_new_nested_dotfile_local_bin() {
    let fix = TestFixture::new().with_default_config();

    // Create ~/.local/bin/foo in target
    fix.add_target(".local/bin/foo", "#!/bin/sh\necho foo");

    let target_path = fix.target_dir().join(".local/bin/foo");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Should land at dotfiles/local/bin/foo
    let source_path = fix.dir.path().join("dotfiles/local/bin/foo");
    assert!(
        source_path.exists(),
        "nested dotfile should preserve full directory nesting"
    );
    assert_eq!(
        fs::read_to_string(&source_path).unwrap(),
        "#!/bin/sh\necho foo"
    );
}

#[test]
fn track_new_mid_path_dot_routes_to_files() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!(
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n\n[files]\nsource = \"files/\"\ntarget = '{target}'\n"
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("files")).unwrap();

    // Create ~/projects/.envrc — first segment "projects" has no dot
    fix.add_target("projects/.envrc", "# direnv");

    let target_path = fix.target_dir().join("projects/.envrc");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Should go to files/projects/.envrc (mid-path dot is NOT first-segment routing)
    let source_path = fix.dir.path().join("files/projects/.envrc");
    assert!(
        source_path.exists(),
        "mid-path dotfile should route to files section"
    );
    assert_eq!(fs::read_to_string(&source_path).unwrap(), "# direnv");
}

#[test]
fn track_new_nested_plain_file_preserves_directory_structure() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!(
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n\n[files]\nsource = \"files/\"\ntarget = '{target}'\n"
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("files")).unwrap();

    // Create ~/projects/scripts/deploy.sh — no dot anywhere in the path
    fix.add_target("projects/scripts/deploy.sh", "#!/bin/sh\ndeploy");

    let target_path = fix.target_dir().join("projects/scripts/deploy.sh");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Should go to files/projects/scripts/deploy.sh with full nesting preserved
    let source_path = fix.dir.path().join("files/projects/scripts/deploy.sh");
    assert!(
        source_path.exists(),
        "nested plain file should preserve directory structure in files section"
    );
    assert_eq!(
        fs::read_to_string(&source_path).unwrap(),
        "#!/bin/sh\ndeploy"
    );
}

// ── Shared-target collision tests ────────────────────────

#[test]
fn apply_shared_target_no_collision_between_dotfiles_and_files() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!(
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n\n\
         [files]\nsource = \"files/\"\ntarget = '{target}'\n"
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("files/config")).unwrap();

    // Both sections have "config/starship.toml" in their source dirs
    fix.add_source("config/starship.toml", "# dotfiles version");
    let files_path = fix.dir.path().join("files/config/starship.toml");
    fs::write(&files_path, "# files version").unwrap();

    fix.nostos().arg("apply").assert().success();

    // Dotfiles source "config/starship.toml" → target ".config/starship.toml" (dot-prepend)
    assert_eq!(fix.read_target(".config/starship.toml"), "# dotfiles version");
    // Files source "config/starship.toml" → target "config/starship.toml" (verbatim)
    assert_eq!(fix.read_target("config/starship.toml"), "# files version");
}

#[test]
fn track_shared_target_routes_dotfile_and_plain_to_correct_sections() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!(
        "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n\n\
         [files]\nsource = \"files/\"\ntarget = '{target}'\n"
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("dotfiles")).unwrap();
    fs::create_dir_all(fix.dir.path().join("files")).unwrap();

    // Place both files in target: .config/starship.toml and config/starship.toml
    fix.add_target(".config/starship.toml", "# from dotfiles");
    fix.add_target("config/starship.toml", "# from files");

    // Track the dotfile version — first segment ".config" has dot → dotfiles section
    let dotfile_target = fix.target_dir().join(".config/starship.toml");
    fix.nostos()
        .arg("track")
        .arg(&dotfile_target)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    let dotfile_source = fix.dir.path().join("dotfiles/config/starship.toml");
    assert!(
        dotfile_source.exists(),
        "dotfile should route to dotfiles/config/starship.toml"
    );
    assert_eq!(
        fs::read_to_string(&dotfile_source).unwrap(),
        "# from dotfiles"
    );

    // Track the plain version — first segment "config" has no dot → files section
    let plain_target = fix.target_dir().join("config/starship.toml");
    fix.nostos()
        .arg("track")
        .arg(&plain_target)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    let plain_source = fix.dir.path().join("files/config/starship.toml");
    assert!(
        plain_source.exists(),
        "plain file should route to files/config/starship.toml"
    );
    assert_eq!(
        fs::read_to_string(&plain_source).unwrap(),
        "# from files"
    );

    // Cross-contamination check: dotfiles didn't get the plain file, files didn't get the dotfile
    assert!(
        !fix.dir.path().join("files/.config/starship.toml").exists(),
        "files section should NOT contain .config/starship.toml"
    );
    assert!(
        !fix.dir.path().join("dotfiles/.config/starship.toml").exists(),
        "dotfiles section should NOT contain .config/starship.toml with leading dot"
    );
}

// ── Sync tests ───────────────────────────────────────────

/// A pair of git repos for testing sync: a bare "remote" and a local clone.
struct SyncFixture {
    root: TempDir,
}

impl SyncFixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let remote = root.path().join("remote.git");
        let local = root.path().join("local");
        let home = root.path().join("home");
        fs::create_dir_all(&home).unwrap();

        // Initialise a bare remote
        run_git(root.path(), &["init", "--bare", remote.to_str().unwrap()]);

        // Clone it to "local"
        run_git(
            root.path(),
            &["clone", remote.to_str().unwrap(), local.to_str().unwrap()],
        );

        // Configure user identity for the local repo
        run_git(&local, &["config", "user.email", "test@example.com"]);
        run_git(&local, &["config", "user.name", "Test"]);

        // Seed with nostos.toml and a dotfile
        let target = home.to_string_lossy().to_string();
        let config = format!(
            "[dotfiles]\nsource = \"dotfiles/\"\ntarget = '{target}'\n"
        );
        fs::write(local.join("nostos.toml"), config).unwrap();
        fs::create_dir_all(local.join("dotfiles")).unwrap();
        fs::write(local.join("dotfiles").join("bashrc"), "# initial").unwrap();

        run_git(&local, &["add", "--all"]);
        run_git(&local, &["commit", "-m", "initial seed"]);
        run_git(&local, &["push"]);

        SyncFixture { root }
    }

    fn local_path(&self) -> std::path::PathBuf {
        self.root.path().join("local")
    }

    fn remote_path(&self) -> std::path::PathBuf {
        self.root.path().join("remote.git")
    }

    fn home_path(&self) -> std::path::PathBuf {
        self.root.path().join("home")
    }

    fn nostos_sync(&self) -> Command {
        let mut cmd = Command::cargo_bin("nostos").unwrap();
        cmd.arg("--repo").arg(self.local_path());
        cmd.arg("sync");
        cmd.env(
            "XDG_CONFIG_HOME",
            self.root.path().join("xdg-config").to_str().unwrap(),
        );
        cmd
    }
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should be available");
    assert!(
        out.status.success(),
        "git {:?} in {} failed: {}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn track_no_dotfiles_section_errors() {
    let fix = TestFixture::new();
    // Config with only [files], no [dotfiles]
    let target = fix.target_dir().to_string_lossy().to_string();
    let config = format!("[files]\nsource = \"files/\"\ntarget = '{target}'\n");
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();
    fs::create_dir_all(fix.dir.path().join("files")).unwrap();

    // Try to track a dotfile — should fail
    fix.add_target(".myrc", "# content");
    let target_path = fix.target_dir().join(".myrc");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no [dotfiles] section configured"));
}

#[test]
fn track_no_files_section_errors() {
    let fix = TestFixture::new().with_default_config();

    // Try to track a non-dot file with no [files] section — should fail
    fix.add_target("plainfile", "content");
    let target_path = fix.target_dir().join("plainfile");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no [files] section configured"));
}

#[test]
fn track_managed_file_with_layered_source() {
    let fix = TestFixture::new();
    let target = fix.target_dir().to_string_lossy().to_string();

    // Use the current platform so the override applies on any OS
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    };

    let config = format!(
        r#"[dotfiles]
source = "dotfiles/"
target = '{target}'

[dotfiles.platforms.{platform}]
"bashrc" = "dotfiles/platforms/{platform}/bashrc"
"#
    );
    fs::write(fix.dir.path().join("nostos.toml"), &config).unwrap();

    // Create both base and platform-override source files
    fix.add_source("bashrc", "# base bashrc");
    let platform_dir = fix.dir.path().join(format!("dotfiles/platforms/{platform}"));
    fs::create_dir_all(&platform_dir).unwrap();
    fs::write(platform_dir.join("bashrc"), "# platform bashrc").unwrap();

    // Apply — should use the platform override
    fix.nostos().arg("apply").assert().success();
    assert_eq!(fix.read_target(".bashrc"), "# platform bashrc");

    // Edit target
    fix.add_target(".bashrc", "# platform edited");

    // Track it back — should go to the platform source, not base
    let target_path = fix.target_dir().join(".bashrc");
    fix.nostos()
        .arg("track")
        .arg(&target_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Tracked"));

    // Verify the platform source was updated (not the base)
    let platform_content = fs::read_to_string(platform_dir.join("bashrc")).unwrap();
    assert_eq!(platform_content, "# platform edited");

    // Base should be untouched
    let base_content =
        fs::read_to_string(fix.dir.path().join("dotfiles").join("bashrc")).unwrap();
    assert_eq!(base_content, "# base bashrc");
}

#[test]
fn sync_dirty_working_tree_auto_commits() {
    let fix = SyncFixture::new();

    // Make a local change
    fs::write(
        fix.local_path().join("dotfiles").join("vimrc"),
        "# new file",
    )
    .unwrap();

    fix.nostos_sync()
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed local changes"))
        .stdout(predicate::str::contains("Pushed to remote"));
}

#[test]
fn sync_clean_working_tree_skips_commit() {
    let fix = SyncFixture::new();

    fix.nostos_sync()
        .assert()
        .success()
        .stdout(predicate::str::contains("Committed local changes").not())
        .stdout(predicate::str::contains("Pulled from remote"))
        .stdout(predicate::str::contains("Pushed to remote"));
}

#[test]
fn sync_with_apply_flag() {
    let fix = SyncFixture::new();

    fix.nostos_sync()
        .arg("--apply")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dotfiles:"));

    assert!(fix.home_path().join(".bashrc").exists());
}

#[test]
fn sync_no_repo_path_errors() {
    let dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("nostos").unwrap();
    cmd.env(
        "XDG_CONFIG_HOME",
        dir.path().join("xdg-config").to_str().unwrap(),
    );
    cmd.arg("sync")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No repo path configured"));
}

#[test]
fn sync_pulls_remote_commits() {
    let fix = SyncFixture::new();

    // Simulate a remote change via a second clone
    let second = fix.root.path().join("second");
    run_git(
        fix.root.path(),
        &[
            "clone",
            fix.remote_path().to_str().unwrap(),
            second.to_str().unwrap(),
        ],
    );
    run_git(&second, &["config", "user.email", "test@example.com"]);
    run_git(&second, &["config", "user.name", "Test"]);
    fs::write(
        second.join("dotfiles").join("gitconfig"),
        "# from remote",
    )
    .unwrap();
    run_git(&second, &["add", "--all"]);
    run_git(&second, &["commit", "-m", "remote commit"]);
    run_git(&second, &["push"]);

    // Sync from the local clone — should pull the remote commit
    fix.nostos_sync()
        .assert()
        .success()
        .stdout(predicate::str::contains("Pulled from remote"));

    assert!(fix.local_path().join("dotfiles").join("gitconfig").exists());
}

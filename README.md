# nostos

> [!WARNING]
> **Pre-release software:** nostos is still in active development.
> Use at your own risk for now.

A cross-platform dev environment sync tool. Manage your dotfiles with
conflict detection, safe backups, and a simple TOML config.

## Installation

```shell
cargo install --path .
```

Requires Rust 1.95+ (edition 2024).

## Quick start

1. Create a repo with a `dotfiles/` directory containing your config files
   (without leading dots):

   ```
   my-env-repo/
   ├── nostos.toml
   └── dotfiles/
       ├── bashrc
       ├── gitconfig
       └── config/
           └── starship.toml
   ```

2. Add a `nostos.toml` at the repo root:

   ```toml
   [dotfiles]
   source = "dotfiles/"
   target = "~"

   # Declare tools you need — not yet installed by nostos, but the config
   # is validated now and will "just work" when tool installation ships.
   [[tool]]
   name = "ripgrep"
   bin = "rg"
   install.brew = "ripgrep"
   install.apt = "ripgrep"

   [[hook]]
   name = "post-setup"
   run = "scripts/post-apply.sh"
   when = "post-apply"
   ```

3. Run nostos:

   ```shell
   # Check platform info, available package managers, and config validity
   nostos status

   # Preview what would happen (dry run)
   nostos plan

   # Apply dotfiles to your home directory
   nostos apply
   ```

   After the first `apply`, nostos remembers your repo location. You can
   run commands from anywhere, or use `--repo <path>` to override:

   ```shell
   nostos --repo ~/dotfiles plan
   ```

## How it works

- **Dot-prepend convention** — files in the source directory get a `.`
  prepended when placed in the target: `bashrc` → `~/.bashrc`,
  `config/starship.toml` → `~/.config/starship.toml`.

- **Conflict detection** — nostos tracks SHA-256 hashes of placed files.
  If a target file was modified locally or already exists from another
  source, nostos warns you and creates a timestamped backup instead of
  overwriting.

- **Safe by default** — `nostos plan` always shows what would happen
  before `nostos apply` changes anything. Conflicts produce backups
  (e.g., `~/.bashrc.nostos-backup-20260501-121500`), never silent
  overwrites.

- **Platform/machine layering** — override specific files per OS or per
  machine. The cascade (base → platform → machine) means machine-specific
  configs always win.

## Platform and machine overrides

nostos supports platform-specific and machine-specific dotfile overrides.
Base files come from the source directory walk. Platform and machine
overrides replace or add files using explicit mappings in `nostos.toml`.

```toml
[dotfiles]
source = "dotfiles/"
target = "~"

# Override gitconfig on Linux (e.g., different credential helper)
[dotfiles.platforms.linux]
"gitconfig" = "dotfiles/platforms/linux/gitconfig"

# Machine-specific git identity and SSH config
[dotfiles.machines.work-macbook]
"gitconfig" = "dotfiles/machines/work-macbook/gitconfig"
"ssh/config" = "dotfiles/machines/work-macbook/ssh-config"
```

Corresponding repo layout:

```
my-env-repo/
├── nostos.toml
└── dotfiles/
    ├── bashrc
    ├── gitconfig
    ├── platforms/
    │   └── linux/
    │       └── gitconfig
    └── machines/
        └── work-macbook/
            ├── gitconfig
            └── ssh-config
```

**Cascade order:** base → platform → machine. If the same target path
appears at multiple layers, machine wins over platform wins over base.
Overrides replace or add entries but never remove base files.

**Machine identity** is auto-detected from hostname and stored in
`state.toml`.

## Commands

| Command  | Description                                        | Exit codes           |
|----------|----------------------------------------------------|----------------------|
| `status` | Platform, managers, machine identity, config check | 0, 3, 4 (platform)  |
| `plan`   | Dry-run showing all planned actions                | 0, 3 (config error)  |
| `apply`  | Apply dotfiles, save state                         | 0, 3, 5 (warnings)   |

Exit code 5 means some files were skipped (local modifications detected)
but other files were applied successfully.

### Global flags

| Flag            | Description                                      |
|-----------------|--------------------------------------------------|
| `--repo <path>` | Override the repo/config location                |

### Repo resolution order

1. `--repo` flag (explicit)
2. Path stored in state from a previous `apply`
3. Current working directory

## Why "nostos"?

The name comes from the Ancient Greek *νόστος* (nostos), meaning
"homecoming" — the safe return home after a long journey. It's the root
of the word *nostalgia* and a central theme in the Odyssey. A fitting
name for a tool that brings your configuration safely home, no matter
where you are.

## Building from source

Requires Rust 1.95+ (edition 2024).

```shell
cargo build
cargo test
cargo clippy --all-targets
```

## Status

**v0.1.0** — Reliable dotfile management with forward-compatible config.

What works today:
- Dotfile sync with conflict detection and safe backups
- Platform/machine dotfile layering with cascade overrides
- Full config schema validation (tools, hooks, files, preferences)
- Cross-platform (Linux, macOS, Windows)
- Package manager discovery
- Machine identity and repo path tracking

Coming next:
- Tool installation (the config schema is already validated)
- Hook execution
- `nostos init` / `nostos sync` (git operations)

## License

See [LICENSE](LICENSE) for details.

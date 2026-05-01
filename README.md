# nostos

A cross-platform dev environment sync tool. Manage your dotfiles with
conflict detection, safe backups, and a simple TOML config.

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
   ```

3. Run nostos from within the repo:

   ```shell
   # Check platform info and config validity
   nostos status

   # Preview what would happen (dry run)
   nostos plan

   # Apply dotfiles to your home directory
   nostos apply
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

## Commands

| Command  | Description                              | Exit codes           |
|----------|------------------------------------------|----------------------|
| `status` | Show platform info and config validity   | 0, 3, 4 (platform)  |
| `plan`   | Dry-run showing all planned actions      | 0, 3 (config error)  |
| `apply`  | Apply dotfiles, save state               | 0, 3, 5 (warnings)   |

Exit code 5 means some files were skipped (local modifications detected)
but other files were applied successfully.

## Building from source

Requires Rust 1.95+ (edition 2024).

```shell
cargo build
cargo test
cargo clippy --all-targets
```

## License

See [LICENSE](LICENSE) for details.

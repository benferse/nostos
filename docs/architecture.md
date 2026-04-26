# nostos — Architecture

This document bridges the high-level concept (see `concept.md`) with
implementation. It covers module structure, dependency choices, config
schema, and a phased build plan.

## MVP milestone

The smallest useful first build. Everything else is deferred until this
works end-to-end.

**In scope for MVP:**
- Config parser (TOML, single file, no layering yet)
- Platform detection (OS, architecture, distro)
- Dotfiles module (copy-based placement, dot-prepend convention)
- Conflict detection (hash-based state tracking)
- `nostos apply` command (apply dotfiles)
- `nostos plan` command (dry-run of apply)
- `nostos status` command (show machine identity and platform info)

**Deferred to post-MVP:**
- Tool resolver and installer registry
- Hook executor
- Platform/machine layering and overrides
- `nostos init` (git clone via libgit2)
- `nostos sync` (commit/push/pull)
- `nostos track` (reverse sync)
- `[files]` section (verbatim copy)
- Per-distro installer preferences
- Machine-specific dotfile overrides

## Module structure

```
src/
├── main.rs              ← entry point, delegates to cli
├── lib.rs               ← public API surface
├── cli/                 ← command-line interface
│   ├── mod.rs           ← clap app definition
│   ├── apply.rs         ← nostos apply
│   ├── plan.rs          ← nostos plan
│   └── status.rs        ← nostos status
├── config/              ← configuration parsing and merging
│   ├── mod.rs           ← top-level Config struct
│   ├── dotfiles.rs      ← dotfile config types
│   ├── tools.rs         ← tool config types
│   └── hooks.rs         ← hook config types
├── reconcile/           ← applying desired state to the machine
│   ├── mod.rs           ← reconciler orchestration
│   ├── dotfiles.rs      ← copy files, detect conflicts
│   ├── tools.rs         ← resolve and install tools
│   └── hooks.rs         ← run hook scripts
├── platform/            ← OS and environment detection
│   ├── mod.rs           ← Platform struct, detection logic
│   ├── os.rs            ← OS, arch, distro detection
│   └── packages.rs      ← available package manager discovery
├── state/               ← local machine state (state.toml)
│   └── mod.rs           ← read/write applied hashes, machine identity
└── git/                 ← git operations
    └── mod.rs           ← clone, commit, push, pull via libgit2
```

## Config schema

Canonical definition of `nostos.toml`. All fields, types, and
optionality in one place.

```toml
# ── Dotfiles ─────────────────────────────────────────────

[dotfiles]
source = "dotfiles/"                    # required, path relative to repo root
target = "~"                            # required, target directory

# Platform-specific overrides. Keys are repo-relative paths (without
# leading dot). Values are alternate source paths.
[dotfiles.platforms.<platform>]         # optional, platform = macos|linux|windows
"<target-path>" = "<source-path>"

# Machine-specific overrides. Same key/value format as platform overrides.
[dotfiles.machines.<machine-id>]        # optional
"<target-path>" = "<source-path>"

# ── Files (verbatim copy, no dot-prepend) ────────────────

[files]
source = "files/"                       # required
target = "~"                            # required

# ── Hooks ────────────────────────────────────────────────

[[hook]]
name = "install-homebrew"               # required, unique identifier
run = "hooks/install-homebrew.sh"       # required, path relative to repo root
when = "pre-apply"                      # required, "pre-apply" or "post-apply"
platforms = ["macos"]                   # optional, default: all platforms
machines = ["work-macbook"]             # optional, default: all machines

# ── Installer preferences ───────────────────────────────

[preferences.<platform>]               # platform = macos|windows
installer-priority = ["brew", "cargo"] # required, ordered list

[preferences.linux.<distro>]           # distro = ubuntu|fedora|arch|...
installer-priority = ["apt", "cargo"]  # required, ordered list

[preferences.linux]                    # fallback for unrecognized distros
installer-priority = ["cargo"]         # required, ordered list

# ── Tools ────────────────────────────────────────────────

[[tool]]
name = "ripgrep"                       # required, also the default package name
install.<manager> = "<package-name>"   # optional, overrides default for a manager
platforms = ["linux"]                  # optional, default: all platforms
```

### Field types

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `dotfiles.source` | string (path) | yes | — |
| `dotfiles.target` | string (path) | yes | — |
| `files.source` | string (path) | no | — |
| `files.target` | string (path) | no | — |
| `hook.name` | string | yes | — |
| `hook.run` | string (path) | yes | — |
| `hook.when` | enum: `pre-apply`, `post-apply` | yes | — |
| `hook.platforms` | array of strings | no | all platforms |
| `hook.machines` | array of strings | no | all machines |
| `preferences.*.installer-priority` | array of strings | yes | — |
| `tool.name` | string | yes | — |
| `tool.install.*` | string | no | tool name |
| `tool.platforms` | array of strings | no | all platforms |

## Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `clap` | CLI argument parsing | derive API |
| `serde` | Serialization/deserialization | with `derive` feature |
| `toml` | Config file parsing | |
| `git2` | Git operations via libgit2 | deferred to post-MVP |
| `sha2` | Content hashing for conflict detection | SHA-256 |
| `dirs` | Platform-appropriate config/data directories | state.toml location |

## State file schema

`state.toml` is stored at the platform-appropriate location (see
concept.md) and is never synced to git.

```toml
[machine]
id = "work-macbook"                    # set at init time

[applied]
# Key: target path (with dot), Value: hash + timestamp of last apply
".bashrc" = { hash = "sha256:...", timestamp = "2026-04-26T20:00:00Z" }
".config/starship.toml" = { hash = "sha256:...", timestamp = "2026-04-25T10:30:00Z" }
```

## Error handling

nostos uses application-level error propagation at command boundaries and
structured module-level error types within internal components. The
general approach:

- **Config parsing errors** — report file, line, and field with a clear
  message. Fail fast — don't partially apply a broken config.
- **Dotfile conflicts** — not errors. Reported as warnings, backed up,
  and continued.
- **Tool install failures** — reported per-tool, does not abort remaining
  tools. Exit code reflects whether any failures occurred.
- **Hook failures** — reported per-hook, does not abort remaining hooks
  or other phases.
- **Git errors** — reported with actionable guidance (e.g., "merge
  conflict in nostos.toml — resolve with git and re-run `nostos apply`").

## Testing strategy

- **Config parsing** — unit tests with inline TOML strings covering valid
  configs, missing fields, invalid values, and edge cases.
- **Platform detection** — unit tests with mocked `/etc/os-release` and
  environment variables.
- **Dotfiles reconciler** — integration tests using a temp directory as
  both the repo source and the target. Covers all five conflict states.
- **State file** — unit tests for read/write/update of `state.toml`.
- **CLI commands** — integration tests that run the binary and assert
  stdout/stderr output and exit codes.

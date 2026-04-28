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
- Structured error handling (`thiserror` + `anyhow` + `miette` for
  config diagnostics) — wired up from the start so typed module errors
  are surfaced with context at command boundaries

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
- Multi-repo support (`repo add`/`remove`/`list`, unified merge across
  repos, `[[repo]]` state tracking — see concept.md)

## Module structure

```
src/
├── main.rs              ← entry point, delegates to cli
├── lib.rs               ← public API surface
├── cli/                 ← command-line interface
│   ├── mod.rs           ← clap app definition
│   ├── apply.rs         ← nostos apply
│   ├── init.rs          ← nostos init (post-MVP)
│   ├── plan.rs          ← nostos plan
│   ├── repo.rs          ← nostos repo add/remove/list (post-MVP)
│   ├── status.rs        ← nostos status
│   ├── sync.rs          ← nostos sync (post-MVP)
│   └── track.rs         ← nostos track (post-MVP)
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
│   └── mod.rs           ← read/write applied hashes, machine identity,
│                           repo registry (post-MVP: [[repo]] array)
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
| `dotfiles.source` | string (path) | yes (if present) | — |
| `dotfiles.target` | string (path) | yes (if present) | — |
| `files.source` | string (path) | yes (if present) | — |
| `files.target` | string (path) | yes (if present) | — |
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
| `toml` | Config file parsing | `toml::Spanned<T>` used for byte-offset reporting in config errors |
| `thiserror` | Typed error enums per module | derive API |
| `anyhow` | Application-level error propagation | CLI layer only — never in public library APIs |
| `miette` | Rich diagnostics for config parse errors | `fancy` feature in the binary; config module only |
| `git2` | Git operations via libgit2 | deferred to post-MVP |
| `sha2` | Content hashing for conflict detection | SHA-256 |
| `dirs` | Platform-appropriate config/data directories | state.toml location |

## State file schema

`state.toml` is stored at the platform-appropriate location (see
concept.md) and is never synced to git.

```toml
[machine]
id = "work-macbook"                    # set at init time

# --- Post-MVP: multi-repo support ---
# When multi-repo is enabled, the repo registry tracks all configured
# repos and their ordering. For single-repo (MVP), this section is
# absent and the single clone path is inferred.
#
# [[repo]]
# name = "dotfiles"
# url = "https://github.com/user/dotfiles.git"
# path = "~/.config/nostos/repos/dotfiles"
# order = 0
#
# [[repo]]
# name = "work-dotfiles"
# url = "https://dev.azure.com/org/project/_git/work-dotfiles"
# path = "~/.config/nostos/repos/work-dotfiles"
# order = 1

[applied]
# Key: target path (with dot), Value: hash + timestamp of last apply
# Post-MVP: entries gain a `source` field for repo attribution
".bashrc" = { hash = "sha256:...", timestamp = "2026-04-26T20:00:00Z", source = "dotfiles" }
".config/starship.toml" = { hash = "sha256:...", timestamp = "2026-04-25T10:30:00Z", source = "work-dotfiles" }
```

## Error handling

### Crate roles

| Crate | Where used | Role |
|-------|-----------|------|
| `thiserror` | Per-module `Error` enums (`config::Error`, `reconcile::Error`, `state::Error`, `git::Error`, `platform::Error`) | Typed, matchable error variants with `#[derive(Error)]` |
| `anyhow` | CLI command handlers in `src/cli/` | Aggregates errors from multiple modules via `?` with `.context()` breadcrumbs |
| `miette` | `config::Error` variants that carry source spans | Config errors implement `Diagnostic` so the binary renders file/line/column with labels and `help:` hints |

The binary entry point installs `miette`'s fancy report handler so
config errors render with source context:

```text
Error: invalid value for `hook.when`
  × expected "pre-apply" or "post-apply", got "before"
   ╭─[nostos.toml:14:10]
 13 │ name = "install-homebrew"
 14 │ when = "before"
    ·          ──┬──
    ·            ╰── unknown variant
 15 │ run  = "hooks/install-homebrew.sh"
   ╰────
  help: valid values are "pre-apply", "post-apply"
```

### Conventions

- Library code (`src/lib.rs` and submodules) returns
  `Result<T, module::Error>`. Public APIs never expose `anyhow::Error`.
- CLI command handlers convert module errors via `?` into
  `anyhow::Result`, attaching `.context("while applying dotfiles")`-style
  breadcrumbs at command boundaries.
- Error enums are `#[non_exhaustive]` so adding variants is
  non-breaking.
- Config errors capture `toml::Spanned<T>` values and implement
  `miette::Diagnostic` to surface file, line, and column with a `help:`
  hint.
- Recoverable conditions (dotfile conflicts, per-tool install failures,
  per-hook failures) are **not** `Err` returns. They are collected into a
  structured `Report` by the reconciler, which determines the final exit
  code.

### Failure semantics per phase

- **Config parsing** — any parse or validation error produces `Err` with
  a `miette` diagnostic. Fail fast — don't partially apply a broken
  config.
- **Platform detection** — unsupported OS or missing required package
  manager produces `Err`. Cannot proceed without a valid platform.
- **Dotfile conflicts** — not errors. The reconciler logs them as
  warnings, backs up the conflicting target, and continues. Conflicts
  are entries in the `Report`.
- **Tool install failures** — reported per-tool in the `Report`. Does
  not abort remaining tools. The final exit code reflects whether any
  failures occurred.
- **Hook failures** — reported per-hook in the `Report`. Does not abort
  remaining hooks or other phases.
- **Git errors** — produce `Err` with actionable guidance (e.g., "merge
  conflict in nostos.toml — resolve with git and re-run
  `nostos apply`").

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success — no failures, no conflicts requiring attention |
| 1 | Generic / unexpected error |
| 2 | CLI / usage error (invalid arguments; matches `clap` default behavior unless explicitly overridden) |
| 3 | Config error (parse, validation, missing file) |
| 4 | Platform / environment error (unsupported OS, missing required package manager) |
| 5 | Partial failure (`apply` completed but one or more tools/hooks failed) |
| 6 | Git error (clone/pull/push/merge conflict) |
| 130 | Interrupted (SIGINT) — standard Unix convention |

`nostos plan` never exits non-zero for conflicts — it is a dry-run.
Only structural errors (bad config, unsupported platform) produce
non-zero exits from `plan`.

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

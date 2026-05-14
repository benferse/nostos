# nostos

A cross-platform dev environment sync tool that manages dotfiles, tool
installation, and configuration across machines using git-backed repos.

## Language

**Dotfile**:
A configuration file placed with a leading dot prepended to its name
(e.g., repo `bashrc` → target `~/.bashrc`). Managed via the `[dotfiles]`
config section.
_Avoid_: config file (too broad)

**File**:
A file placed verbatim with no path transformation. Managed via the
`[files]` config section.
_Avoid_: raw file, static file

**Base**:
The set of files discovered by walking a section's `source` directory.
Applies to all platforms and machines before overrides.
_Avoid_: default, fallback

**Override**:
A platform or machine layer entry that replaces or adds to the base file
map. An override substitutes the source path for a given target path.
Overrides cannot remove base files.
_Avoid_: overlay (implies merging content, not replacing files)

**Layering**:
The cascade of base → platform override → machine override that produces
the final file map for reconciliation.

**Reconciler**:
The component that computes the diff between desired state (config) and
current state (disk + state file), then applies changes. Operates per
phase: dotfiles, files, tools, hooks.

**Apply**:
Execute the reconciler to bring the machine into the desired state.

**Plan**:
Dry-run the reconciler to show what `apply` would do without making
changes.

**Track**:
Reverse-sync a target file back into the repo source, updating the state
hash. Does not commit. For files already managed (present in state), the
recorded `source` field determines the destination path. For new
(unmanaged) files, the **first-segment dot heuristic** decides the
section: if the first segment of the file's path relative to the section
target starts with `.`, route to `[dotfiles]` and strip the leading dot
from that first segment only (preserving any nested directories);
otherwise route to `[files]` verbatim. New files always land in the base
source directory, never in a platform/machine layer. See ADR 0003.

**Sync**:
The git workflow: auto-commit local changes, pull --rebase from remote,
push to remote. Does not auto-apply.

**Init**:
Clone a config repo and set machine identity. Does not auto-apply unless
`--apply` is passed.

**Machine identity**:
A string identifying the current machine (e.g., `work-macbook`), used to
match `[*.machines.<id>]` overrides. Auto-detected from the OS hostname
verbatim (e.g., `ben-mbp.local`) when `--machine` is omitted; dots are
preserved and the "Quote machine identities in TOML keys" invariant
applies.

**State**:
The local record of what nostos has applied (`state.toml`). Tracks
per-file hashes, timestamps, and source paths. Never synced to git.

## Relationships

- A **Dotfile** or **File** has one **Base** source path, optionally
  replaced by an **Override** through **Layering**
- **Apply** invokes the **Reconciler**, which reads **State** and config
  to determine actions
- **Plan** invokes the same **Reconciler** in read-only mode
- **Track** writes a target file back to the repo and updates **State**
- **Sync** commits, pulls, and pushes the repo but does not invoke
  the **Reconciler**
- **Init** clones the repo and sets **Machine identity**

## Example dialogue

> **Dev:** "I ran **apply** but my Linux-specific alacritty config wasn't
> placed — why?"
>
> **Domain expert:** "Check your **layering**. The **base** has a generic
> `alacritty.toml`, and the **override** in `[dotfiles.platforms.linux]`
> should point to the Linux-specific source. If the override source path
> doesn't exist, `**plan**` will report an error."

## Invariants

**Forward-slash path keys**:
All logical path keys (file map keys, config override keys, state entry
keys) use forward slashes regardless of platform. OS-specific path
separators from `std::fs` and `Path::display()` must be normalised to
`/` before use as map keys or comparison targets. This applies to any
code that walks directories, builds file maps, or matches path prefixes.
Rationale: nostos is cross-platform (Linux, macOS, Windows) and CI runs
on all three; backslash keys silently break prefix filters and map
lookups on Windows.

**Quote machine identities in TOML keys**:
Machine identities (hostnames) may contain dots, which TOML interprets
as nested table separators. Always quote machine IDs when interpolating
into TOML table headers: `[dotfiles.machines."my-host.local"]`. This
applies to both production config generation and test fixtures.

## Flagged ambiguities

- "overlay" was considered for what we call **override**. Rejected because
  overlay implies content merging; overrides replace the entire source file.

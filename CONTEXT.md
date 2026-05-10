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
hash. Does not commit.

**Sync**:
The git workflow: auto-commit local changes, pull --rebase from remote,
push to remote. Does not auto-apply.

**Init**:
Clone a config repo and set machine identity. Does not auto-apply unless
`--apply` is passed.

**Machine identity**:
A string identifying the current machine (e.g., `work-macbook`), used to
match `[*.machines.<id>]` overrides. Auto-detected from hostname if not
explicitly set.

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

## Flagged ambiguities

- "overlay" was considered for what we call **override**. Rejected because
  overlay implies content merging; overrides replace the entire source file.

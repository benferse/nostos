# Use first-segment dot heuristic for `nostos track` of new files

When `nostos track <path>` is run on a file that is not already recorded
in `state.toml`, nostos must infer which config section (`[dotfiles]` or
`[files]`) the new entry belongs to and where to place the source file
inside the repo. The placement rule has to handle both the simple case
(`~/.bashrc`) and nested XDG-style paths (`~/.config/nvim/init.vim`).

## Decision

Compute the path relative to each candidate section's `target` directory.
If the **first segment** of that relative path starts with `.`, route to
`[dotfiles]` and strip the leading dot from the first segment only,
preserving any nested directories. Otherwise route to `[files]` verbatim.

Examples (assuming `dotfiles.target = "~"` and `files.target = "~"`):

| Tracked path                  | Section      | Source placed at              |
| ----------------------------- | ------------ | ----------------------------- |
| `~/.bashrc`                   | `[dotfiles]` | `<dotfiles.source>/bashrc`    |
| `~/.config/nvim/init.vim`     | `[dotfiles]` | `<dotfiles.source>/config/nvim/init.vim` |
| `~/.local/bin/foo`            | `[dotfiles]` | `<dotfiles.source>/local/bin/foo` |
| `~/projects/.envrc`           | `[files]`    | `<files.source>/projects/.envrc` |
| `~/myscript`                  | `[files]`    | `<files.source>/myscript`     |

## Rationale

- The dot in `~/.config/...` is the user-facing signal that "this is
  config that lives under a hidden directory I keep with my dotfiles."
  Routing such files to `[dotfiles]` matches user intuition and keeps
  XDG-style trees in one section.
- Stripping the leading dot from only the first segment mirrors the
  forward direction of the reconciler — `[dotfiles]` semantics dot-prepend
  the leading segment when going from source → target, so the inverse
  must dot-strip exactly that segment.
- Preserving nested directories prevents source-name collisions
  (multiple `init.vim` files under different XDG subtrees) and keeps
  the repo layout legible.
- The "first segment only" rule means a literal dot mid-path
  (`projects/.envrc`) is not treated as a dotfile signal — it is just a
  hidden file inside an ordinary user directory.

## Considered Options

- **Leaf-only heuristic** — inspect only the file's leaf name. Simpler
  to implement and explain ("does the filename start with a dot?"), but
  forces every XDG-style file (`~/.config/*`) into `[files]`, which
  contradicts how users mentally categorise their dotfiles. Also leaves
  the source-path question open: a leaf-only inference still has to
  decide whether to drop directory structure (which collides) or
  preserve it (in which case the heuristic is leaf-only for routing but
  whole-path for placement, which is inconsistent).
- **First-segment heuristic** (chosen) — routes XDG-style files to
  `[dotfiles]` while keeping the routing rule and the placement rule
  symmetrical (both look at the first segment).
- **Ask the user every time** — most ergonomic answer but defeats the
  point of `track` as a one-liner.

## Consequences

- The shipped implementation in `src/cli/track.rs` (`track_new_file`)
  uses leaf-only routing and discards directory structure. This decision
  obsoletes that behaviour; a follow-up bug fix is required, plus tests
  covering nested-path tracking.
- Files placed mid-path with a dot prefix (`~/projects/.envrc`) stay in
  `[files]`. If a user wants such a file in `[dotfiles]` they must edit
  `nostos.toml` by hand — acceptable since the case is rare and the
  alternative (any-segment heuristic) would mis-route many ordinary
  hidden cache directories.

# Keep separate DotfilesConfig and FilesConfig structs

`DotfilesConfig` and `FilesConfig` are structurally identical today —
both carry `source`, `target`, `platforms`, and `machines` fields with
the same serde attributes. The two `From` impls that convert them into
`FileMapInput` are also identical.

We are deliberately keeping them as separate types for now rather than
unifying into a shared generic struct.

## Rationale

The two sections have different *semantics* (dot-prepend vs verbatim
copy) even though their *shapes* happen to match. As nostos evolves
the sections may diverge — for example, `[files]` could gain
permission-mode settings that don't apply to dotfiles, or `[dotfiles]`
could gain an exclusion list. Premature unification would force
awkward `Option` fields or trait gymnastics to re-separate them.

## Decision

Keep `DotfilesConfig` and `FilesConfig` as independent structs. Accept
the small amount of structural duplication in exchange for the freedom
to evolve each section independently. Revisit if a third section with
the same shape appears or if the two structs remain identical after
several milestones of feature work.

## Considered Options

- **Shared generic `SectionConfig`** — removes the duplicate struct
  and collapses the two `From` impls. Risk: premature coupling makes
  future divergence painful.
- **Trait-based abstraction** — a `SyncSection` trait implemented by
  both types. Adds indirection without clear benefit while the shapes
  are trivially identical.
- **Keep separate structs** (chosen) — a few lines of duplication,
  but each type is free to evolve on its own schedule.

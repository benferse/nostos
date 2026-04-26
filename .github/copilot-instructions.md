# Copilot Instructions for nostos

## Build and test

```shell
cargo build
cargo test
cargo test <test_name>    # run a single test
```

Rust edition 2024, minimum toolchain 1.95. See `Cargo.toml` for details.

## Git conventions

- All commits must be GPG-signed (`git commit -S`). Unsigned commits
  will be rejected.
- Include the Co-authored-by trailer for Copilot on all commits.

## Architecture

nostos is a cross-platform dev environment sync tool. See
`docs/concept.md` for the design brief and `docs/architecture.md` for
implementation details including module structure, config schema, and
MVP scope.

Two pillars: **dotfiles** (copy-based, dot-stripping convention) and
**tool installation** (tool-centric model with per-manager install
mappings). Config format is TOML.

## Working style

- **Propose before expanding.** When presenting design options, keep
  proposals concise — a few sentences with tradeoffs, not walls of text.
  Only elaborate with full analysis when asked. Prefer iterative
  deepening over comprehensive first drafts.

- **Keep examples consistent.** When a design decision affects multiple
  examples in a document, update all of them in the same commit. Don't
  leave stale examples for a follow-up pass.

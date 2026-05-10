# Copilot Instructions for nostos

## Build and test

```shell
cargo build
cargo test
cargo test <test_name>    # run a single test
cargo clippy --all-targets
```

Rust edition 2024, minimum toolchain 1.95. See `Cargo.toml` for details.

## Quality gates

After every code change, you **must** actually run:

1. `cargo clippy --all-targets` — fix all warnings before committing.
2. `cargo test` — confirm all tests pass. Do not assume they pass;
   run them and check the output.

Do not claim tests pass or clippy is clean without having executed the
commands in the current state of the working tree.

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

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues on this repo. See `docs/agents/issue-tracker.md`.

### Triage labels

Default label vocabulary (needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout — one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

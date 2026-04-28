---
name: design-review
description: 'Review nostos design documents (concept.md and architecture.md) for internal consistency, stale references, and completeness. Use when design docs have been updated and need a consistency pass, or when a new design section has been added.'
---

# Design Review Skill

Review `docs/concept.md` and `docs/architecture.md` for internal
consistency after changes. Catches stale references, contradictions,
and missing updates that arise when a new design section is added but
existing sections still reflect the old model.

USE FOR: "review the design docs", "check concept.md for consistency",
"are the docs consistent?", "design review", "doc consistency check"

DO NOT USE FOR: code review, PR review, implementation review

## Checks

Read both documents fully, then check for:

### 1. Cross-document consistency

- State file schemas in concept.md and architecture.md must agree
  (field names, types, example values)
- Module structure in architecture.md must cover all commands described
  in concept.md's "Proposed UX" section
- Anything listed as "deferred" in architecture.md should not appear as
  MVP-required in concept.md

### 2. Internal consistency within concept.md

- **Singular vs plural repo language** — the multi-repo section
  describes multiple repos, but earlier sections may still say "the
  repo" in ways that contradict multi-repo support
- **CLI command references** — every `nostos <command>` shown in
  examples must be described in the "Proposed UX" section or in a
  design section. No invented commands.
- **state.toml examples** — all `state.toml` snippets must include
  `[machine]` and should reference `[[repo]]` (or forward-reference to
  the multi-repo section) where appropriate
- **Resolved decisions** — each resolved decision should be consistent
  with the detailed design sections it summarizes. If a design section
  has evolved, the resolved decision entry may be stale.
- **Examples match described behavior** — command output shown in usage
  examples must match the semantics described in the design (e.g., if
  merge is last-repo-wins, examples shouldn't show first-repo-wins)

### 3. Internal consistency within architecture.md

- Module structure should list all CLI subcommands that concept.md
  describes (even if marked post-MVP)
- Config schema fields must match what concept.md's examples use
- Dependency table should cover libraries needed by described features

### 4. Completeness

- Every major design concept in concept.md should have a corresponding
  entry in architecture.md (module, schema field, or deferred note)
- Forward and backward references between sections should resolve
  (anchor links, section names)
- New design sections should be mentioned in the resolved decisions if
  they settle a previously open question

## Output format

Report issues as a numbered list with:

- **Severity**: High (contradicts), Medium (misleads), Low (imprecise)
- **Location**: file and line number(s)
- **Issue**: what's wrong
- **Fix**: concrete suggestion

Group by severity (High first). Skip style and formatting issues —
only report substantive problems.

## Workflow

1. Read `docs/concept.md` fully
2. Read `docs/architecture.md` fully
3. Run all checks above
4. Present findings to the user, grouped by severity
5. Ask whether to fix all, fix high/medium only, or just report

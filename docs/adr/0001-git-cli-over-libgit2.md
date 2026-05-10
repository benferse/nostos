# Use git CLI instead of libgit2 for git operations

The concept doc specifies `git2` (libgit2) for git operations to enable a
zero-external-dependency bootstrap. We're starting with the `git` CLI
instead because it sidesteps significant authentication complexity (SSH
agent forwarding, per-host keys, credential managers like GCM) and reduces
binary size. The `git` command is available on virtually every developer
machine. We can revisit `git2` later if the zero-dependency story becomes
a priority.

## Considered Options

- **git2 crate (libgit2)** — embedded, no external dependency. But
  authentication handling differs from the git CLI, which creates friction
  for users with complex SSH or credential manager setups.
- **git CLI** — shells out to `git`. Inherits the user's existing auth
  config transparently. Simpler to implement and debug.

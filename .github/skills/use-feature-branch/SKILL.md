---
name: use-feature-branch
description: 'Ensure new work is always started on a feature branch. When beginning a task, create a feature branch from main, commit any unstaged changes on the current branch first, and then proceed with the task on the new branch.'
---

# Use Feature Branch Skill

Ensure that new work always begins on a dedicated feature branch. This
skill handles branch creation and stashing of in-progress work so the
agent never commits task work directly to main.

USE FOR: any task that will produce code changes, new features, bug
fixes, documentation updates, or refactors — essentially any work that
should go through a pull request.

DO NOT USE FOR: read-only tasks (code review, exploration, questions
about the codebase), branch cleanup, or tasks where the user has
already specified a branch to work on.

## Workflow

### 1. Check current branch state

Run `git status` to determine:

- The current branch name
- Whether there are unstaged or staged but uncommitted changes
- Whether there are untracked files

### 2. Handle uncommitted changes

If there are uncommitted changes (staged, unstaged, or untracked files)
on the current branch:

- Commit them on the current branch with the message:
  `chore: save work-in-progress before switching branches`
- This preserves the user's in-progress work and avoids carrying
  uncommitted changes onto the new feature branch.

### 3. Create and switch to a feature branch

- If already on a feature branch (any branch other than `main`), ask
  the user whether to continue on the current branch or create a new
  one.
- If on `main`, create a new branch from `main`.
- Branch naming convention: `<category>/<short-description>`
  - Categories: `feat`, `fix`, `docs`, `refactor`, `chore`, `test`
  - Example: `feat/add-bin-field`, `fix/symlink-resolution`,
    `docs/update-architecture`
- Derive the branch name from the task description. If the task
  references a GitHub issue number, include it:
  `fix/issue-42-broken-symlinks`

### 4. Confirm and proceed

- Report the new branch name to the user.
- Proceed with the task on the new branch.

## Rules

- **Never commit task work directly to `main`.** Always use a feature
  branch.
- **Never lose uncommitted work.** Always commit or stash before
  switching branches.
- **Keep branch names concise.** Aim for 3–5 words max in the
  description slug.
- **Use the git tool** for all git operations (status, commit, branch,
  checkout).
- **Sign all commits** with `git commit -S` per repo conventions.
- **Include the Co-authored-by trailer** on any commits made by this
  skill.

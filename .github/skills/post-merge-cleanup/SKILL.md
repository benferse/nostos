# Post-Merge Cleanup Skill

Update the local main branch and delete feature branches that have been
merged (or squash-merged) via pull request.

USE FOR: after a PR is merged and the user wants to sync up, when the
user says "cleanup branches", "update main", or "post-merge cleanup",
or when you have just confirmed a PR merge.

DO NOT USE FOR: deleting branches that have open PRs, or when the user
is actively working on a feature branch they want to keep.

## Workflow

### 1. Switch to main

If not already on `main`, switch to it:

```
git checkout main
```

If there are uncommitted changes on the current branch, commit them
first with:

```
git commit -S -m "chore: save work-in-progress before switching branches"
```

### 2. Pull latest main

```
git pull
```

With `fetch.prune=true` configured, this also removes remote-tracking
refs for branches deleted on the remote.

### 3. Delete gone branches

Delete all local branches whose upstream tracking branch no longer
exists:

```
git gone
```

This runs the global alias:
`git branch -vv | grep ': gone]' | awk '{print $1}' | xargs -r git branch -D`

### 4. Report results

Tell the user:
- Which branches were deleted
- If any branches remain that are not `main` (they may have open PRs
  or no upstream set)

## Rules

- **Never delete `main`.**
- **Never delete a branch with an open PR** unless the user explicitly
  asks.
- **Always report what was deleted** so there are no surprises.
- **Sign any commits** with `git commit -S` per repo conventions.
- If `git gone` finds no branches to delete, just report that
  everything is clean.

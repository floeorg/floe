---
name: land
description: >
  Clean up after a PR is merged - close the issue, remove the worktree, and sync main.
  TRIGGER when: the user says "merged", "landed", "done", or confirms a PR has been merged.
  DO NOT TRIGGER when: the PR is still open or under review.
argument-hint: "[issue number (optional, inferred from branch if omitted)]"
---

# Land

Clean up after a merged PR: close the issue, remove the worktree, sync main.

**Only run this after the user confirms the PR has been merged.**

## Inputs

- `$ARGUMENTS` — issue number. If omitted, infer from the current branch name (e.g. `feature/#123.foo` -> `123`).

## Step 1: Determine context

1. Get the current branch: `git branch --show-current`
2. Infer the issue number from the branch if not provided
3. Check if this is a sub-issue (branch matches `feature/#<epic>/#<sub>.*`)

## Step 2: Close the issue

```bash
glb close <num>
```

## Step 3: Remove the worktree

Navigate back to the main repo and remove the worktree:

```bash
cd ~/Code/Projects/floe
git worktree remove ../floe-worktrees/<num>
```

## Step 4: Sync main

```bash
git checkout main
git pull
```

## Step 5: Epic check

If this was a sub-issue, check whether all sub-issues of the parent epic are now closed:

```bash
glb sub list <epic-num>
```

If all sub-issues are done, tell the user the epic is ready to be finalized — the epic PR can be marked ready for review.

## Step 6: Report

Tell the user:
- Issue #<num> closed
- Worktree removed
- Main synced
- If epic: whether the epic is ready to finalize

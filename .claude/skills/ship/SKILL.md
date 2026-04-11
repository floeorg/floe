---
name: ship
description: >
  Run quality gates, /simplify, /rulify, create or update a PR, poll CI, then mark ready.
  TRIGGER when: (1) the user says "ship it", "ship", or asks to create a PR, OR (2) you have finished implementing a task and are ready to submit it -- invoke this automatically as part of the workflow.
  DO NOT TRIGGER when: the user just wants to run tests or quality gates without creating a PR.
argument-hint: "[issue number (optional, inferred from branch if omitted)]"
---

# Ship

Full pipeline: quality gates, code review, PR, CI loop, merge wait, and land.

On **re-runs** (PR already exists), skip PR creation — just run quality gates, code review, push, and resume the CI + merge loop.

## Inputs

- `$ARGUMENTS` — issue number. If omitted, infer from the current branch name (e.g. `feature/#123.foo` -> `123`).

## Step 1: Determine scope

1. Get the current branch name: `git branch --show-current`
2. Infer the issue number from the branch if not provided via `$ARGUMENTS`
3. Determine which packages/apps were changed:
   - `git diff --name-only $(git merge-base HEAD origin/main)...HEAD` (or the epic branch if this is a sub-issue)
   - Map changed paths to packages and frontend apps
4. Check if this is a sub-issue (branch matches `feature/#<epic>/#<sub>.*`) — if so, the PR base is the epic branch, not main
5. Check if a PR already exists for this branch: `gh pr view --json number,state 2>/dev/null`

## Step 2: Verify against issue

**MANDATORY — produce the checklist below as visible output.** Run `glb show <num>` and list every requirement, acceptance criterion, and sub-task from the issue body. Mark each:

- ✓ `<requirement>` — implemented in `<file:line>`
- ✗ `<requirement>` — MISSING, implementing now
- ⊘ `<requirement>` — skipped because `<reason>` (only use when the user explicitly agreed or it's clearly out of scope)

If anything is ✗, finish it before proceeding.

## Step 3: Code review

Run self-checks in order:

1. **`/simplify`** — review changed code for reuse, quality, and efficiency
2. **`/rulify`** — cross-check changes against `.claude/rules/`

Commit any fixes from this step.

## Step 4: Quality gates

Run quality gates **scoped to changed packages only** — after code review, since /simplify may have rewritten code that needs re-formatting and re-linting.

Check `.claude/rules/` for the specific quality gate commands for each technology in this project (e.g., Rust formatting/linting/testing, frontend linting/typechecking/testing).

Common patterns:
- **Rust**: `cargo fmt -p <pkg>`, `cargo clippy -p <pkg> -- -D warnings`, `cargo nextest run -p <pkg>`
- **Frontend**: `cd <app-dir> && pnpm lint:fix`, `cd <app-dir> && pnpm check`

Commit any fixes from this step.

## Step 5: PR

Push the branch:
```bash
git push -u origin $(git branch --show-current)
```

**If a PR already exists**, skip to step 6.

**If no PR exists**, create a draft:

**Standalone issue** (PR targets main):
```bash
gh pr create --draft --title "[#<num>] <issue title>" --body "$(cat <<'EOF'
closes #<num>

<summary of changes>

## Test plan

<checklist>
EOF
)"
```

**Sub-issue** (PR targets epic branch):
```bash
gh pr create --draft --base feature/#<epic-num>.<summary> \
  --title "[#<epic-num>/#<num>] <issue title>" --body "$(cat <<'EOF'
closes #<num>

<summary of changes>

## Test plan

<checklist>
EOF
)"
```

Get the issue title from `glb show <num>`. The PR body must start with `closes #<num>` and include a test plan.

## Step 6: Local testing instructions (before CI)

**Immediately after pushing/creating the PR**, tell the user how to test locally so they can verify while CI runs:

1. The exact mise command with worktree number: `mise run dev <worktree-num>`
2. URL(s) to open
3. What to do to trigger the feature
4. What to look for to confirm it works

Do NOT wait for CI to finish before giving these instructions.

## Step 7: CI loop

Each poll iteration, check **both**:
1. CI status: `gh pr checks <pr-number>`
2. Merge conflicts: `gh pr view <pr-number> --json mergeable --jq '.mergeable'`

Keep output minimal — just report pass/fail status, not full logs.

Track consecutive failures. **Cap at 3 — after 3 consecutive failures, stop and ask the user.**

### On CI failure or merge conflict:

1. **Merge conflicts** (`mergeable` is `CONFLICTING`): merge the base branch in and resolve conflicts
2. **CI failures**: read failure logs and fix the issue
3. Re-run quality gates (step 4) on affected packages
4. If the fix involved new logic or structural changes (not just mechanical fixes like missing imports or type annotations), re-run `/simplify` and `/rulify`
5. Commit, push, poll again

### On CI pass AND no conflicts:

Proceed to step 8.

## Step 8: Mark ready + report

```bash
gh pr ready <pr-number>
```

Tell the user:

1. **PR URL** — always link the PR.
2. Remind them to say "merged" when the PR is merged so `/land` can clean up.

**Never run `gh pr merge`.**

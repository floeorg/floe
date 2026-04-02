---
name: ship
description: >
  Run quality gates, /simplify, /rulify, create a draft PR, poll CI until it passes, then mark the PR ready.
  TRIGGER when: (1) the user says "ship it", "ship", or asks to create a PR, OR (2) you have finished implementing a task and are ready to submit it -- invoke this automatically as part of the workflow (see .claude/rules/workflow.md steps 4-6).
  DO NOT TRIGGER when: the user just wants to run tests or quality gates without creating a PR.
argument-hint: "[issue number (optional, inferred from branch if omitted)]"
disable-model-invocation: false
---

# Ship

Run the full pre-PR pipeline: quality gates, code review, draft PR, CI loop, and mark ready.

## Inputs

- `$ARGUMENTS` — issue number. If omitted, infer from the current branch name (e.g. `feature/#123.foo` -> `123`).

## Step 1: Determine scope

1. Get the current branch name: `git branch --show-current`
2. Infer the issue number from the branch if not provided via `$ARGUMENTS`
3. Determine what was changed:
   - `git diff --name-only $(git merge-base HEAD origin/main)...HEAD` (or the epic branch if this is a sub-issue)
   - Note whether Rust source (`src/**/*.rs`), Floe examples (`examples/**/*.fl`), or LSP code was changed
4. Check if this is a sub-issue (branch matches `feature/#<epic>/#<sub>.*`) — if so, the PR base is the epic branch, not main

## Step 2: Quality gates

Run quality gates scoped to what changed. Fix issues at each step before proceeding.

**Rust quality gate** (if `src/**/*.rs` changed):
```bash
cargo fmt
cargo clippy -- -D warnings
RUSTFLAGS="-D warnings" cargo test
```

**Floe example quality gate** (if `src/**/*.rs` or `examples/**/*.fl` changed):
```bash
pnpm install --frozen-lockfile   # only if node_modules/ is missing
floe fmt examples/todo-app/src/ examples/store/src/
floe check examples/todo-app/src/ examples/store/src/
floe build examples/todo-app/src/ examples/store/src/
```

**LSP integration tests** (if LSP, checker, or language syntax changed):
```bash
python3 -m pytest tests/lsp/ --floe-bin=./target/debug/floe
```

Commit any fixes from this step.

## Step 3: Code review

Run self-checks in order:

1. **`/simplify`** — review changed code for reuse, quality, and efficiency
2. **`/rulify`** — cross-check changes against `.claude/rules/`

If either made changes:
- Re-run quality gates (step 2) on affected areas
- Commit fixes

## Step 4: Create draft PR

Push the branch and create a draft PR.

```bash
git push -u origin $(git branch --show-current)
```

**PR titles use conventional commit prefixes** (`feat:`, `fix:`, `chore:`, `test:`). Append `!` for breaking changes.

**Standalone issue** (PR targets main):
```bash
gh pr create --draft --title "feat: [#<num>] <issue title>" --body "$(cat <<'EOF'
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
  --title "feat: [#<epic-num>/#<num>] <issue title>" --body "$(cat <<'EOF'
closes #<num>

<summary of changes>

## Test plan

<checklist>
EOF
)"
```

Get the issue title from `glb show <num>`. The PR body must start with `closes #<num>`.

## Step 5: CI loop

Poll CI status using `gh pr checks <pr-number> --watch` or periodic `gh pr checks <pr-number>`.

Track consecutive failures. **Cap at 3 — after 3 consecutive CI failures, stop and ask the user.**

### On CI failure:

1. Read failure logs: `gh pr checks <pr-number>` to identify which check failed, then fetch logs
2. Fix the issue
3. Re-run quality gates (step 2) on affected areas
4. If the fix involved new logic or structural changes (not just mechanical fixes like missing imports or type annotations), re-run `/simplify` and `/rulify`
5. Commit, push, poll again

### On CI pass:

Proceed to step 6.

## Step 6: Mark ready

```bash
gh pr ready <pr-number>
```

## Step 7: Report

Tell the user:

1. **PR URL** — link to the PR
2. Ask the user to review and merge. **Never run `gh pr merge`.**

## Step 8: Wait for merge and land

After reporting, poll the PR merge status periodically:

```bash
gh pr view <pr-number> --json state --jq '.state'
```

Check every 2 minutes. When the state is `MERGED`, automatically run `/land` to clean up (close issue, remove worktree, sync main).

If the user closes the session before the PR is merged, that's fine — `/land` can be run manually in a future session.

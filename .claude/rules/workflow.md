<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Agent Task Workflow

**MANDATORY for every task. Do NOT skip any step.**

## glb (ghlobes) — Issue Tracking

Use `glb` for ALL task tracking via GitHub Issues + Projects. Do NOT use TodoWrite, TaskCreate, or markdown TODOs.

### Finding Work

```bash
glb ready                    # Show unblocked issues
glb list                     # All open issues
glb show <num>               # Detailed view with dependencies
glb path                     # Critical path + high-leverage issues (--by-count, --top N)
glb next                     # Recommend next batch for parallel agents (--agents N, default 3)
```

### Creating Issues

```bash
glb create --title="Summary" --body="Why and what" --priority P2 --status Todo --points 3
```

**Issue body must include a `## Tests` section** listing the tests that need to be written to verify the work. Exception: bug-report issues that already reference a failing test, chores with no behavior change (deps, CI, docs).

**No em dashes in titles.** Issue titles and PR titles must not contain em dashes. Use a regular hyphen (-) or rewrite the sentence instead.

Priorities: P0 (critical), P1 (high), P2 (medium/default), P3 (low), P4 (backlog)

### Points

Use **Fibonacci numbers** for the `--points` field: `1, 2, 3, 5, 8, 13`.
- `1` — trivial (< 1 hour)
- `2` — small (a few hours)
- `3` — medium (half a day)
- `5` — large (full day)
- `8` — very large (2-3 days)
- `13` — epic (break it down into sub-issues instead if possible)

### Epics (sub-issues)

```bash
glb sub add <parent> <child>    # Add a sub-issue to a parent (epic)
glb sub remove <parent> <child> # Remove a sub-issue from a parent
glb sub list <parent>           # List sub-issues with progress
```

**Default: sub-issues branch off `main` and PR into `main`, just like any other issue.** The parent/child relationship is organizational only.

**Only use the epic-branch workflow when the user explicitly asks for it** (e.g. "use an epic branch", "ship this as an epic"). Do NOT create an epic branch on your own just because an issue has sub-issues.

When the user *does* ask for the epic-branch workflow, it looks like this:

```
main
 └── feature/#409.llm-infrastructure              <- epic branch
      ├── feature/#409/#505.model-registry        <- PRs into epic branch
      ├── feature/#409/#498.complexity-routing    <- PRs into epic branch
      └── feature/#409/#494.cost-tracking         <- PRs into epic branch

# When all sub-issues are done:
feature/#409.llm-infrastructure -> main            <- one final PR
```

**Epic workflow (only when explicitly requested):**
1. Create the epic branch: `git worktree add ../floe-worktrees/<epic-num> -b feature/#<num>.<summary> main`
2. **Immediately create the epic PR** (even if empty) so progress is visible
3. Sub-issue worktrees branch off the **epic branch**, not main
4. Sub-issue PRs target the **epic branch** (`gh pr create --base feature/#<epic-num>.<summary>`)
5. Sub-issue PR body uses `closes #<sub-num>` as usual
6. When all sub-issues are merged, mark the epic PR as ready for review

### Rules

- Check `glb ready` before asking "what should I work on?"
- Use `glb search "query"` to find existing issues
- Do NOT create markdown TODO lists or use external trackers

## Multi-Agent Environment

Multiple agents run in parallel on separate branches. This means:

- **Only touch files relevant to your task.** Do not modify, stash, reset, or discard files you didn't create or change yourself.
- **Never run `git stash`, `git reset --hard`, `git checkout -- <file>`, or `git clean`** unless you are certain those changes belong to you. When in doubt, leave it alone.
- If you see unexpected files or changes, investigate before acting — they likely belong to another agent working in parallel.

## Session Start — MANDATORY

Sync before doing anything:

```bash
git checkout main && git pull
```

## Read the Docs — MANDATORY

**Before touching any code**, check whether the feature or area you are working on has a doc.

1. Open `docs/README.md` — it is the index for all product and architecture docs.
2. Find the relevant doc (feature, plugin, architecture topic) and read it before starting.

Do not assume you know how something works — read the doc first.

## Task Workflow

### 1. Create a Worktree

Each task gets its own isolated worktree. See worktrees rule for the full workflow.

```bash
mise run worktree:setup <num> feature/#<num>.<summary>
cd ../floe-worktrees/<num>
```

Do all work — editing, building, testing, committing — from inside this directory.

### 2. Verify Worktree

Before touching any file or running any command, confirm you are in the right place:

```bash
pwd                       # must be .../floe-worktrees/<num>
git branch --show-current # must be your issue branch
```

If either is wrong, stop and fix it before proceeding.

### 3. Claim & Work

```bash
glb update <num> --claim
```

**Before writing new code, find a similar existing implementation in each layer you're about to touch and follow its patterns.**

Commit semi-frequently — don't save everything for one giant commit.

### 4. Ship

**When implementation is done, run `/ship`.** It handles everything: quality gates, code review, draft PR, CI loop (including merge conflicts), and mark ready. After the user merges, say "merged" to trigger `/land`.

## Session Completion

- **NEVER stop before pushing** — that leaves work stranded locally. YOU must push; never say "ready to push when you are."
- **File issues** for any remaining work — `glb create`
- If push fails, resolve and retry until it succeeds

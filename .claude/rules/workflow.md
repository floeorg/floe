# Agent Task Workflow

**MANDATORY for every task. Do NOT skip any step.**

## glb (ghlobes) - Issue Tracking

Use `glb` for ALL task tracking via GitHub Issues + Projects. Do NOT use TodoWrite, TaskCreate, or markdown TODOs.

### Finding Work

```bash
glb ready                    # Show unblocked issues
glb list                     # All open issues
glb show <num>               # Detailed view with dependencies
```

### Creating Issues

```bash
glb create --title="Summary" --body="Why and what" --priority P2 --status Todo --points 3
```

**No em dashes in titles.** Issue titles and PR titles must not contain em dashes (`---`). Use a regular hyphen (`-`) or rewrite the sentence instead.

Priorities: P0 (critical), P1 (high), P2 (medium/default), P3 (low), P4 (backlog)

### Points

Use **Fibonacci numbers** for the `--points` field: `1, 2, 3, 5, 8, 13`.
This represents effort/complexity. When estimating, pick the closest Fibonacci value.
- `1` - trivial (< 1 hour)
- `2` - small (a few hours)
- `3` - medium (half a day)
- `5` - large (full day)
- `8` - very large (2-3 days)
- `13` - epic (break it down into sub-issues instead if possible)

### Epics (sub-issues)

```bash
glb sub add <parent> <child>    # Add a sub-issue to a parent (epic)
glb sub remove <parent> <child> # Remove a sub-issue from a parent
glb sub list <parent>           # List sub-issues with progress
```

Epics use a **shared branch** that sub-issues merge into before going to main:

```
main
 └── feature/#1.lexer                          ← epic branch (created once)
      ├── feature/#1/#14.token-types           ← PRs into epic branch
      ├── feature/#1/#15.standard-scanning     ← PRs into epic branch
      └── feature/#1/#18.keywords              ← PRs into epic branch

# When all sub-issues are done:
feature/#1.lexer → main                        ← one final PR
```

**Epic workflow:**
1. Create the epic branch: `git worktree add ../floe-worktrees/<epic-num> -b feature/#<num>.<summary> main`
2. **Immediately create the epic PR** (even if empty) so progress is visible:
   ```bash
   gh pr create --title "[Epic] [#<epic-num>] <epic title>" \
     --body "closes #<epic-num>" --draft
   ```
3. Sub-issue worktrees branch off the **epic branch**, not main
4. Sub-issue PRs target the **epic branch** (`gh pr create --base feature/#<epic-num>.<summary>`)
5. Sub-issue PR body uses `closes #<sub-num>` as usual
6. When all sub-issues are merged, mark the epic PR as ready for review

**Rules for epics:**
- **Create the epic PR immediately** when starting the epic - don't wait for sub-issues to finish
- **Sub-issues do the actual work** - each gets its own worktree branched off the epic branch
- **Sub-issue branches nest under the epic number:** `feature/#<epic-num>/#<sub-num>.<summary>`
- **Keep the epic branch up to date with main** - periodically merge main into it to avoid drift
- **Standalone issues** (not part of an epic) still branch off main as before

### Rules

- Check `glb ready` before asking "what should I work on?"
- Use `glb search "query"` to find existing issues
- Do NOT create markdown TODO lists or use external trackers

## Multi-Agent Environment

Multiple agents run in parallel on separate branches. This means:

- **Only touch files relevant to your task.** Do not modify, stash, reset, or discard files you didn't create or change yourself.
- **Never run `git stash`, `git reset --hard`, `git checkout -- <file>`, or `git clean`** unless you are certain those changes belong to you. When in doubt, leave it alone.
- If you see unexpected files or changes, investigate before acting - they likely belong to another agent working in parallel.

## Session Start - MANDATORY

Sync before doing anything:

```bash
git checkout main && git pull
```

## Task Workflow

### 1. Create a Worktree

Each task gets its own isolated worktree. See `.claude/rules/worktrees.md` for the full workflow.

```bash
git worktree add ../floe-worktrees/<num> -b feature/#<num>.<summary> main
cd ../floe-worktrees/<num>
```

Do all work - editing, building, testing, committing - from inside this directory.

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

Commit semi-frequently - don't save everything for one giant commit. Use **conventional commits** (`feat:`, `fix:`, `chore:`, `docs:`, `ci:`, `refactor:`, `test:`). Append `!` for breaking changes (e.g. `feat!:`). These prefixes drive automatic version bumps and changelog generation via release-please.

**Before every commit**, run `cargo fmt` (and `floe fmt` if you touched `.fl` files). Never commit unformatted code.

**Use conventional commits.** All commit messages must be prefixed:
- `feat:` — new feature or language syntax
- `fix:` — bug fix
- `chore:` — maintenance, deps, CI, docs, refactoring
- `test:` — adding or updating tests only

Examples:
```
feat: add use keyword for callback flattening
fix: checker resolves pipe expressions to unknown type
chore: remove Brand type in favor of newtypes
test: add codegen snapshot tests for tuples
```

PR titles follow the same convention: `feat: [#260] Add use keyword`

### 4. Quality Gate

Run before closing any task, scoped to what you changed.

**Rust quality gate** (if you changed `src/**/*.rs`):

```bash
cargo fmt
cargo clippy -- -D warnings
RUSTFLAGS="-D warnings" cargo test
```

**All warnings are errors.** Clippy uses `-D warnings`; tests use `RUSTFLAGS="-D warnings"`. Fix warnings before proceeding.

Order: fmt -> clippy -> test.

**Always run the Floe example quality gate too** when changing compiler code — compiler changes can affect formatting, checking, and codegen output.

**Floe example quality gate** (if you changed `src/**/*.rs` or `examples/**/*.fl`):

```bash
floe fmt examples/todo-app/src/ examples/store/src/
floe check examples/todo-app/src/ examples/store/src/
floe build examples/todo-app/src/ examples/store/src/
```

Order: fmt -> check -> build. All must pass with zero errors.

**LSP integration tests** (if you changed LSP, checker, or language syntax):

```bash
python3 scripts/test-lsp.py ./target/debug/floe
```

All tests must pass (0 failures).

### 5. PR (do NOT merge)

Create the PR and **stop**. Do NOT merge - ask the user to review and merge.

**Standalone issue** (not part of an epic) - PR targets main:

```bash
gh pr create --title "[#<num>] <full issue title>" --body "closes #<num>

..."
```

**Sub-issue of an epic** - PR targets the epic branch:

```bash
gh pr create --base feature/#<epic-num>.<summary> \
  --title "[#<epic-num>/#<num>] <full issue title>" \
  --body "closes #<num>

..."
```

**Epic PR** - already created as a draft at the start of the epic (see Epic workflow above). When all sub-issues are merged, mark it as ready for review:

```bash
gh pr ready <epic-pr-number>
```

The PR body **must start with `closes #<num>`** on the first line - this links the PR to the issue and auto-closes it on merge.

After creating the PR, tell the user the PR URL and ask them to review and merge it. **Never run `gh pr merge` yourself.**

### 6. Close

```bash
glb close <num>

# Back in the main repo directory:
git worktree remove ../floe-worktrees/<num>
git pull
```

## Session Completion - MANDATORY

Work is **not done** until `git push` succeeds.

1. **File issues** for remaining work - `glb create`
2. **Run quality gates** (if code changed) - fmt, clippy, test
3. **Update issue status** - close finished work, update in-progress items
4. **Push code to remote**:

   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```

5. **Verify** - all code changes committed AND pushed

**Rules:**

- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

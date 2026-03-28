# Git Worktree Workflow

Each agent works in its own isolated worktree. This is the standard way to handle parallel work - it structurally prevents agents from touching each other's files.

## Location

Worktrees live at `../floe-worktrees/<issue-num>/` relative to the repo root.

```
~/Code/Sandbox/
├── floe/                # main repo (stay on main here)
└── floe-worktrees/
    ├── 14/                   # agent working on issue #14
    ├── 25/                   # agent working on issue #25
    └── ...
```

## Starting a Task

```bash
# From the main repo, ensure main is up to date
git checkout main && git pull

# Create a worktree + branch in one step
git worktree add ../floe-worktrees/<num> -b feature/#<num>.<summary> main

# All work happens inside the worktree
cd ../floe-worktrees/<num>
```

Use the right branch prefix for the type of work:
```
feature/#<num>.<summary>   # new functionality (standalone or epic)
fix/#<num>.<summary>       # bug fixes
chore/#<num>.<summary>     # maintenance, deps, tooling
```

### Epic and sub-issue worktrees

Epics use `feature/` prefix - same as standalone issues. Sub-tasks nest under the epic number to show the relationship.

```bash
# First time: create the epic worktree + branch from main
git worktree add ../floe-worktrees/<epic-num> -b feature/#<epic-num>.<summary> main
cd ../floe-worktrees/<epic-num>

# Sub-issue: branch off the epic branch, nested under epic number
git worktree add ../floe-worktrees/<sub-num> -b feature/#<epic-num>/#<sub-num>.<summary> feature/#<epic-num>.<summary>
cd ../floe-worktrees/<sub-num>
```

Branch naming examples:
```
feature/#14.token-types                   # standalone feature → PRs into main
feature/#1.lexer                          # epic branch → PRs into main when done
feature/#1/#14.token-types                # sub-task of #1 → PRs into feature/#1.lexer
feature/#1/#15.standard-scanning          # sub-task of #1 → PRs into feature/#1.lexer
```

Check `glb show <num>` - if the issue has a parent, it's a sub-issue and should branch off the epic branch. If no parent, branch off main.

## Verify Before Doing Anything

**Before editing any file or running any command**, confirm you are in the correct worktree:

```bash
pwd   # must be .../floe-worktrees/<num>
git branch --show-current   # must be your issue branch
```

If either is wrong, stop and navigate to the correct worktree first. Never edit files or run task commands from the main repo directory or another agent's worktree.

## Building in a Worktree

```bash
cargo check
cargo build
cargo test
```

## Working

Do everything - edit, build, test, commit, push - from inside the worktree directory. Do not return to the main repo directory to do task work.

## Cleanup

After a **standalone or sub-issue PR** is merged:

```bash
cd ~/Code/Sandbox/floe   # back to main repo
git worktree remove ../floe-worktrees/<num>
```

After the **epic final PR** is merged into main:

```bash
cd ~/Code/Sandbox/floe
git worktree remove ../floe-worktrees/<epic-num>
git pull  # pick up the merged changes
```

## Useful Commands

```bash
git worktree list              # see all active worktrees
git worktree prune             # clean up stale worktree refs
```

## Rules

- **Always create a worktree before starting work** - never work directly in the main repo's working tree.
- **Never enter another agent's worktree directory.** If `../floe-worktrees/<num>` already exists, another agent owns that issue - pick something else.
- **One worktree per issue.** Name it `<num>` to match the issue number.
- **Do not stash, reset, or clean** in someone else's worktree. If you see unexpected state, leave it alone.

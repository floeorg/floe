# CLAUDE.md

## Before Starting Any Issue - MANDATORY

**Read the relevant docs before writing code:**
- **`docs/design.md`** — design decisions and rationale for language choices
- **`docs/llms.txt`** — syntax quick reference (what compiles to what)
- **`.claude/rules/architecture.md`** — compiler pipeline and module structure

## Recommended Crates

| Purpose | Crate |
|---|---|
| Error reporting | `ariadne` |
| CLI | `clap` |
| LSP | `tower-lsp` |
| File watching | `notify` |
| WASM | `wasm-bindgen` + `wasm-pack` |
| Typed errors | `thiserror` |
| CLI edge errors | `anyhow` |
| Snapshot testing | `insta` |
| Serialization | `serde` + `serde_json` |

Do **not** use parser generators (pest, nom, lalrpop). The parser is handwritten recursive descent for better error recovery and LSP integration.

## Releases & Versioning

This project uses **conventional commits** + **release-please** for automated versioning and releases.

### How it works

1. **PR titles use conventional commit prefixes:**

   | Prefix | Version bump | Example |
   |---|---|---|
   | `fix:` | patch (0.1.0 -> 0.1.1) | `fix: [#123] crash on nested match` |
   | `feat:` | minor (0.1.0 -> 0.2.0) | `feat: [#456] add pipe lambdas` |
   | `feat!:` | major (0.1.0 -> 1.0.0) | `feat!: [#789] remove arrow functions` |
   | `chore:`, `docs:`, `ci:`, `refactor:`, `test:` | no bump | `docs: update README` |

   The repo uses **squash merges only**. Each PR becomes a single commit on main, and the PR title becomes the commit message. This means only the PR title matters for versioning - individual commits inside PRs can use any message format.

2. **release-please watches main** and auto-opens a "Release PR" that:
   - Bumps the version in `Cargo.toml` based on squash commit messages (PR titles)
   - Updates `CHANGELOG.md` with entries generated from PR titles
   - Title looks like `chore(main): release 0.2.0`

3. **Merging the Release PR** creates a git tag (`v0.2.0`) and a GitHub Release.

4. **The tag triggers the release workflow** which:
   - Cross-compiles binaries for macOS (arm64 + x86), Linux (x86 + arm64), Windows
   - Uploads them as assets on the GitHub Release
   - Publishes `@floeorg/vite-plugin` (and the other `@floeorg/*` packages) to npm
   - Publishes the VS Code extension to Open VSX

Floe is not distributed on crates.io — the compiler CLI is an end-user
tool, not a library for other Rust projects, and `cargo install` compiles
from source which is slow and assumes the Rust toolchain is installed.
Users install via the prebuilt binaries in the GitHub Release or the npm
integrations, following the same pattern as Gleam.

### Package names

| Package | Registry | Name |
|---|---|---|
| Compiler CLI | GitHub Releases | `floe` binary |
| Vite plugin | npm | `@floeorg/vite-plugin` |
| VS Code extension | Open VSX | `floeorg.floe` |

### What you need to do

- Write meaningful PR titles with conventional commit prefixes (the CHANGELOG is generated from them)
- Individual commits inside PRs don't need prefixes - only the PR title matters
- Periodically merge the Release PR that release-please opens
- That's it - everything else is automated

### Config files

| File | Purpose |
|---|---|
| `.github/release-please-config.json` | release-please settings (release type, extra files to bump) |
| `.github/release-please-manifest.json` | current version tracking |
| `.github/workflows/release-please.yml` | workflow that opens Release PRs |
| `.github/workflows/release.yml` | workflow that builds binaries on tag push |
| `CHANGELOG.md` | auto-maintained changelog |

### Pre-1.0 strategy

While pre-1.0, breaking changes bump minor (0.1 -> 0.2) not major. Go to 1.0.0 when the language syntax is stable and people are using it in real projects.
<!-- glb-agent-instructions -->
## Task Tracking with glb

This project uses `glb` (ghlobes) for issue tracking via GitHub Issues + Projects.
All state lives in GitHub — no local database.

### Workflow

1. **Find work:** Run `glb ready` to see unblocked, unclaimed issues.
2. **Claim work:** Run `glb update <number> --claim` to mark it as In Progress.
3. **Do the work:** Implement the issue.
4. **Close:** Run `glb close <number>` when done. Include `--comment` with a brief summary.

### Commands

| Command | What it does |
|---|---|
| `glb ready` | Show issues ready to work (unblocked, not in progress) |
| `glb list` | List all open issues. Filters: `--status`, `--priority`, `--assignee` |
| `glb show <num>` | Show issue details, deps, status, priority, points, sub-issues |
| `glb create --title "..." --priority P1 --status Backlog --points 3` | Create an issue |
| `glb update <num> --claim` | Claim issue (sets status to In Progress) |
| `glb update <num> --status <s> --priority <p> --points <n>` | Update fields |
| `glb close <num>` | Close an issue |
| `glb reopen <num>` | Reopen a closed issue |
| `glb dep add <issue> <blocked_by>` | Add a blocking dependency |
| `glb dep list <issue>` | Show dependencies |
| `glb sub add <parent> <child>` | Add a sub-issue to a parent (epic) |
| `glb sub remove <parent> <child>` | Remove a sub-issue from a parent |
| `glb sub list <parent>` | List sub-issues with progress |
| `glb blocked` | Show all blocked issues |
| `glb path` | Show critical path + high-leverage issues. `--by-count`, `--top N` |
| `glb next` | Recommend next batch for parallel agents. `--agents N` (default 3) |
| `glb search "query"` | Search issues by text |
| `glb stats` | Show open/closed/blocked/ready counts |
| `glb init --update-claude-md` | Refresh these agent instructions |

### Statuses

- **Backlog** — acknowledged, not yet prioritized for active work
- **Todo** — ready to be picked up
- **In Progress** — someone is actively working on it
- **Done** — completed

`glb ready` shows only **Todo** issues that are unblocked and unassigned.

### Points

Use **Fibonacci numbers** for the `--points` field: `1, 2, 3, 5, 8, 13`.
This represents effort/complexity. When estimating, pick the closest Fibonacci value.
- `1` — trivial (< 1 hour)
- `2` — small (a few hours)
- `3` — medium (half a day)
- `5` — large (full day)
- `8` — very large (2–3 days)
- `13` — epic (break it down into sub-issues instead if possible)

### Epics (sub-issues)

Use `glb sub` to organize work into parent/child hierarchies (epics).
GitHub renders these natively with a progress bar on the parent issue.

```
# Create an epic and its tasks
glb create --title "Auth system"          # e.g. becomes #10
glb create --title "Design auth flow"     # e.g. becomes #11
glb create --title "Implement auth"       # e.g. becomes #12

# Link them
glb sub add 10 11
glb sub add 10 12

# Optional: make tasks sequential with a blocking dep
glb dep add 12 11   # #12 blocked by #11
```

### Rules

- **Always run `glb ready` at the start of a session** to find available work.
- **Always `--claim` before starting work** so other agents don't pick the same issue.
- **Never work on issues with status `In Progress`** — another agent is on it.
- **Create issues for new work** instead of just doing it. This keeps the project organized.
- **Add dependencies** when an issue can't be done until another is finished.
- **Close issues when done.** Don't leave them open.

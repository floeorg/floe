<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Agent Instructions (OpenCode compatibility)

Claude Code reads `CLAUDE.md` and `.claude/rules/*.md` natively. This file exists so OpenCode (and other agent tools) can find them too.

## External file loading

When you see a file reference like `@.claude/rules/general.md`, use your Read tool to load it. Lazy — load based on what the current task actually needs, not preemptively. Loaded content is mandatory instruction.

Always-loaded rules are declared in `opencode.json` (`instructions` field). The rules below are topical — load them when relevant.

## Rules library

- @.claude/rules/architecture.md
- @.claude/rules/claude-meta.md
- @.claude/rules/clean-architecture.md
- @.claude/rules/config-and-env.md
- @.claude/rules/docs.md
- @.claude/rules/example-app.md
- @.claude/rules/floe-quality.md
- @.claude/rules/monorepo.md
- @.claude/rules/rust-style.md
- @.claude/rules/syntax-sources.md
- @.claude/rules/testing.md
- @.claude/rules/worktrees.md

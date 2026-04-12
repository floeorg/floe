<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Agent Instructions (OpenCode compatibility)

Claude Code reads `CLAUDE.md` and `.claude/rules/*.md` natively. This file exists so OpenCode (and other agent tools) can find them too.

## External file loading

When you see a file reference like `@.claude/rules/general.md`, use your Read tool to load it. Lazy only — load based on what the current task actually needs, not preemptively. Loaded content is mandatory instruction.

## Always read

@CLAUDE.md
@.claude/rules/general.md
@.claude/rules/workflow.md

## Load when relevant

Additional topical rules live in `.claude/rules/*.md`. Read the ones that apply to the current task (e.g. `rust-style.md` when writing Rust, `testing.md` when writing tests, `worktrees.md` when working in worktrees).

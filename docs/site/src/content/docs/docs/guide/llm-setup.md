---
title: Using Floe with LLMs
---

LLMs don't know Floe yet. You need to give them the language reference so they can write correct Floe code.

## The quick version

Paste this URL into your AI tool's context:

```
https://floe-lang.dev/llms.txt
```

This is a condensed reference covering all Floe syntax, types, compilation rules, and stdlib functions.

## Claude Code

Fetch it directly in a conversation:

```
@url https://floe-lang.dev/llms.txt
```

Or add it to your project's `CLAUDE.md` so every conversation has it:

```markdown
## Floe Reference

When writing .fl files, fetch the language reference first:
https://floe-lang.dev/llms.txt
```

## Cursor

Add it as a doc in your project's `.cursor/rules/` directory, or paste the URL into the context panel when writing Floe.

## GitHub Copilot

Add to `.github/copilot-instructions.md` in your project:

```markdown
When writing Floe (.fl) files, refer to the language reference at:
https://floe-lang.dev/llms.txt

Floe compiles to TypeScript. Key differences from TypeScript:
- Use `fn` instead of `function`
- Use `|>` pipes for data transformation
- Use `match` instead of switch/if-else chains
- Use `Result<T, E>` and `Option<T>` instead of null/exceptions
- No semicolons, no `let`/`var`, no classes
```

## Other tools

Download the reference and include it in your system prompt or context window:

```bash
curl -o llms.txt https://floe-lang.dev/llms.txt
```

## What the reference covers

- Core syntax (functions, types, pipes, pattern matching)
- Type system (records, unions, newtypes, opaque types, Result/Option)
- Compilation rules (what Floe compiles to in TypeScript)
- Standard library functions
- Import system (`throws` for error-wrapping, for-blocks)
- Common pitfalls and rules

---
paths:
  - "**/*.fl"
  - "src/**/*.rs"
---

# Floe Quality Rules

## Never work around compiler bugs

When example app code fails due to a compiler limitation, **fix the compiler** — never simplify or restructure the app code to avoid the bug.

The example apps should showcase ideal Floe code. If valid-looking Floe code doesn't compile, the fix goes in `src/`, not in `examples/`.

## Always write and update tests

Every compiler change must include tests:
- **Parser changes**: add parser unit tests
- **Checker changes**: add checker unit tests
- **Codegen changes**: add codegen snapshot tests
- **Bug fixes**: add a regression test that reproduces the bug
- **Modifications**: update ALL existing tests affected by the change, don't just add new ones

## Update all tooling

Every syntax or language change must also update:
- **Syntax highlighting**: tree-sitter grammar, TextMate grammar, Neovim queries (see `syntax-sources.md`)
- **LSP/IntelliSense**: completions, hover, diagnostics, go-to-definition
- **Type checker**: ensure new or changed syntax is fully checked
- **Formatter**: `floe fmt` handles the new syntax correctly

A feature is not done until highlighting, IntelliSense, and type checking all work.

## Floe File Quality Gate

When creating or modifying `.fl` files, **always run these commands** before considering the work done:

```bash
floe fmt <file-or-directory>
floe check <file-or-directory>
floe build <file-or-directory>
```

Order: fmt -> check -> build. Fix any errors before proceeding.

## End-to-end verification

After compiler changes, verify the full pipeline:
1. `floe check` passes on example apps
2. `floe build` produces valid TypeScript
3. `pnpm dev` / `pnpm build` in example apps succeeds (Vite compiles the generated TS)

A feature is not done until the generated code actually runs.

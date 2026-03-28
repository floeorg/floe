# Language Change Checklist

When adding or modifying language syntax, **every item below must be addressed in the same PR**. A feature is not done until all of these pass.

## 1. Documentation

Update **all three** — never update one without the others:

1. **`docs/design.md`** — the compiler spec. Update the relevant section (AST nodes, codegen table, type checker rules, etc.)
2. **`docs/site/`** — the user-facing docs. Update the relevant pages (guide, reference, examples, etc.)
3. **`docs/llms.txt`** — the LLM quick reference. Update syntax examples, compilation tables, and rules.

These serve different audiences:
- `design.md` is for compiler developers (agents and contributors)
- `site/` is for language users (developers writing Floe)
- `llms.txt` is for LLMs writing Floe code (concise syntax + codegen reference)

## 2. LSP features

Every new or changed language construct must have working:

- **Hover** — shows type info and documentation on hover
- **Go-to-definition** — jumps to the definition site
- **Completions** — appears in autocomplete where relevant
- **Diagnostics** — reports errors correctly

Update `scripts/test-lsp.py` with test cases covering the new/changed behavior:

```bash
python3 scripts/test-lsp.py ./target/debug/floe
```

All tests must pass (0 failures). See `floe-quality.md` for details.

## 3. Example apps

Update the Floe example apps to exercise the new feature naturally. See `example-app.md` for which apps to update and how to verify them.

## 4. Syntax highlighting

Update all editor grammars — see `syntax-sources.md` for the full list.

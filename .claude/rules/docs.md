# Language Change Checklist

When adding or modifying language syntax, **every item below must be addressed in the same PR**. A feature is not done until all of these pass.

## 1. Documentation

Update **both** — never update one without the other:

1. **`docs/site/`** — the user-facing docs. Update the relevant pages (guide, reference, examples, etc.)
2. **`docs/llms.txt`** — the LLM quick reference. Update syntax examples, compilation tables, and rules.

These serve different audiences:
- `site/` is for language users (developers writing Floe)
- `llms.txt` is for LLMs writing Floe code (concise syntax + codegen reference)

Compiler-internal rationale lives in commit messages and CHANGELOG entries — there is no separate design doc to keep in sync.

## 2. LSP features

Every new or changed language construct must have working:

- **Hover** — shows type info and documentation on hover
- **Go-to-definition** — jumps to the definition site
- **Completions** — appears in autocomplete where relevant
- **Diagnostics** — reports errors correctly

Update `tests/lsp/` with test cases covering the new/changed behavior:

```bash
python3 -m pytest tests/lsp/ --floe-bin=./target/debug/floe
```

All tests must pass (0 failures). See `floe-quality.md` for details.

## 3. Example apps

Update the Floe example apps to exercise the new feature naturally. See `example-app.md` for which apps to update and how to verify them.

## 4. Syntax highlighting

Update all editor grammars — see `syntax-sources.md` for the full list.

# Example Apps as Integration Tests

When adding or modifying language features, **update the Floe example apps** to exercise the new feature.

These serve as real-world integration tests — if the examples don't pass `floe check`, the feature isn't done.

## Floe example apps

- `examples/todo-app/` — types, for-blocks, pages, routing
- `examples/store/` — types, error handling, API calls, multi-page app

Only the `.fl` files in these apps are Floe integration tests. The `examples/store-ts/` directory is plain TypeScript and is not part of the Floe quality gate.

## Workflow

1. Implement the feature (lexer, parser, checker, codegen)
2. Update example apps to use it — new syntax should appear naturally, not forced
3. Run the quality gate on all Floe examples (see below)
4. Commit the example app changes in the same PR

## Quality gate for examples

Run on **every** PR that touches the compiler or `.fl` files.

**Important:** Run `pnpm install --frozen-lockfile` first if `node_modules/` is missing — `floe check` needs npm dependencies to resolve TypeScript types. Without them, all external imports resolve to `unknown`.

```bash
pnpm install --frozen-lockfile
floe fmt examples/todo-app/src/ examples/store/src/
floe check examples/todo-app/src/ examples/store/src/
floe build examples/todo-app/src/ examples/store/src/
```

Order: fmt -> check -> build. All must pass with zero errors.

**Note:** `floe fmt` (without `--check`) writes formatted files in place — always run it before committing. CI uses `floe fmt --check` to enforce formatting without modifying files.

If a feature doesn't fit either app, add a new `.fl` file in the appropriate example rather than forcing it.

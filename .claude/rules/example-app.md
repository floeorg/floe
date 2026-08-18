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

**Important:** Run `pnpm install --frozen-lockfile` first if `node_modules/` is missing — `floe check` needs npm dependencies to resolve TypeScript types. Without them, every external import reports **E013**, and `floe check` and `floe build` both fail.

```bash
pnpm install --frozen-lockfile
floe fmt examples/todo-app/src/ examples/store/src/
floe check examples/todo-app/src/ examples/store/src/
(cd examples/todo-app && floe build src/)
(cd examples/store && floe build src/)
```

Order: fmt -> check -> build. All must pass with zero errors.

**Run `floe build` from inside the example.** It names the output after the
source path relative to the working directory. A run from the repository root
writes to `.floe/examples/store/src/`; a run from the example writes to
`examples/store/.floe/src/`, which is the path each example's `rootDirs` reads.

**The examples commit no emitted TypeScript.** Every `.ts` and `.tsx` file
under an example `src/` is a build artifact, and `examples/<app>/.gitignore`
keeps it out of git. The committed copies drifted from the compiler for four
months before issue #1557 removed them. The exceptions are the hand-written
React entry points, `main.tsx`, `router.tsx`, `store-context.tsx` and
`env.d.ts`, which the ignore file re-includes by name.

Each example's `pnpm check` runs `floe check`, then `floe build`, then `tsc
--noEmit` over the fresh output. That checks the hand-written React against
the declarations Floe emits today. The emitted bodies carry `// @ts-nocheck`,
and `scripts/typecheck-emitted.sh` is what checks those, against a ratchet.

**Note:** `floe fmt` (without `--check`) writes formatted files in place — always run it before committing. CI uses `floe fmt --check` to enforce formatting without modifying files.

If a feature doesn't fit either app, add a new `.fl` file in the appropriate example rather than forcing it.

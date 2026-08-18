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
floe fmt examples/todo-app/src/ examples/store/src/ examples/hono-api/src/
floe check examples/todo-app/src/ examples/store/src/ examples/hono-api/src/
(cd examples/todo-app && floe build src/)
(cd examples/store && floe build src/)
(cd examples/hono-api && floe build src/)
```

Order: fmt -> check -> build. All must pass with zero errors. CI runs these
same three commands over these same three examples. Keep the two lists equal.

**Run `floe build` from inside the example.** It names the output after the
source path relative to the working directory. A run from the repository root
writes to `.floe/examples/store/src/`, which no `rootDirs` entry reads. A run
from the example writes to `examples/store/.floe/src/`, which is the path each
example's `rootDirs` reads. A build refuses a source path that sits outside the
working directory, because the output cannot stay inside the output directory.

**The examples commit no emitted TypeScript.** `floe build` writes to `.floe/`,
and the root `.gitignore` covers that directory. The examples carry no ignore
file of their own, on purpose: a rule over `src/` hides a new hand-written
`.tsx` from `git status`, and a person types `git add .`.

The examples tracked 30 emitted files until issue #1557. Fifteen were `.ts` and
`.tsx` bodies, and `// @ts-nocheck` silenced every one of them. The other
fifteen were `.d.ts` files, and tsc never read those at all, because TypeScript
drops `x.d.ts` when `x.ts` stands beside it. The gate therefore read the source
tree and validated nothing, for four months. `examples/store` keeps four
hand-written files, `env.d.ts`, `main.tsx`, `router.tsx` and
`store-context.tsx`. `examples/todo-app` keeps the same list without
`store-context.tsx`.

`pnpm check` in `examples/store` and in `examples/todo-app` runs `floe check`,
then `floe build`, then `tsc --noEmit` over the fresh output. `pnpm check` in
`examples/hono-api` runs `floe check` alone, because no hand-written TypeScript
reads that example's output.

That gate checks the hand-written React against the emitted `.ts` and `.tsx`
bodies, not against the `.d.fl.ts` declarations. An extensionless import such
as `import type { ProductId } from "./types"` resolves through `rootDirs` to
`.floe/src/types.ts`, and the `.d.fl.ts` file never enters the program. So the
gate catches a name or a shape that the hand-written React gets wrong, and it
does not catch a fault inside an emitted body, because every body carries
`// @ts-nocheck`. `scripts/typecheck-emitted.sh` checks the bodies, against a
ratchet. Issue #1586 tracks the stronger option.

**Note:** `floe fmt` (without `--check`) writes formatted files in place — always run it before committing. CI uses `floe fmt --check` to enforce formatting without modifying files.

If a feature doesn't fit either app, add a new `.fl` file in the appropriate example rather than forcing it.

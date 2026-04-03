---
title: Migrating from TypeScript
---

Floe is designed to be familiar to TypeScript developers.

## What Stays the Same

- Import/export syntax
- Template literals
- JSX
- Async (via `Promise.await` stdlib function instead of keywords)
- Type annotations
- Generics

## What Changes

| TypeScript | Floe | Example |
|---|---|---|
| `function` | `fn` | `fn greet(name: string) -> string { ... }` |
| `: ReturnType` | `-> ReturnType` | `fn add(a: number, b: number) -> number` |
| `.filter().map()` | `\|> filter \|> map` | `items \|> filter(.active) \|> map(.name)` |
| `let` / `const` | `const` only | No mutation |
| `===` | `==` | `==` compiles to `===` |
| `switch` | `match` | Exhaustive, no fall-through |
| `try/catch` | Untrusted imports (default) | `import { parseYaml } from "yaml-lib"` (auto-wrapped in Result) |
| `{x && <Comp />}` | `Option.map` | `{x \|> Option.map((v) => <Comp v={v} />)}` |
| `T \| null` | `Option<T>` | `Some(value)` / `None` |
| `throw` | `Result<T, E>` | `Ok(value)` / `Err(error)` |
| `async`/`await` | `Promise.await` | `expr \|> Promise.await` (compiler infers `async`) |

## What's Removed

| Feature | Why | Alternative |
|---------|-----|-------------|
| `let` / `var` | Mutation bugs | `const` only |
| `class` | Complex inheritance hierarchies | Functions + records |
| `this` | Implicit context bugs | Explicit parameters |
| `any` | Type safety escape | `unknown` + narrowing |
| `null` / `undefined` | Nullable reference bugs | `Option<T>` |
| `enum` | Compiles to runtime objects | Union types |
| `interface` | Redundant | `type` |
| `switch` | No exhaustiveness, fall-through | `match` |
| `for` / `while` | Mutation-heavy | Pipes + map/filter/reduce |
| `throw` | Invisible error paths | `Result<T, E>` |
| `return` | Implicit returns | Last expression is the return value |

## Incremental Adoption

Floe compiles to `.ts/.tsx`, so you can adopt it file by file. Write new files as `.fl`, compile them alongside your existing `.ts` files, and your build tool (Vite, Next.js) treats the output as normal TypeScript.

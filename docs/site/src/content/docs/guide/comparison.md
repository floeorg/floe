---
title: Comparison
---

How Floe compares to other languages in its space.

## vs TypeScript

| | TypeScript | Floe |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Null safety** | Optional (`strictNullChecks`) | Enforced (`Option<T>`) |
| **Error handling** | Exceptions | `Result<T, E>` |
| **Mutation** | `let`, `var`, `const` | `const` only |
| **Pattern matching** | No | Yes, exhaustive |
| **Pipes** | No (TC39 Stage 2) | `\|>` built in |
| **Classes** | Yes | No |
| **Type overrides** | `any`, `as`, `!` | None |
| **Runtime** | None | None |

Floe is TypeScript with stricter type safety and functional features added.

## vs Gleam

| | Gleam | Floe |
|---|---|---|
| **Target** | Erlang/JS | TypeScript |
| **Ecosystem** | Hex/npm | npm |
| **JSX** | No | Yes |
| **React** | No | First-class |
| **Syntax** | ML-family | TS-family |
| **Pipes** | Yes | Yes |
| **Pattern matching** | Yes | Yes |
| **Adoption** | New ecosystem | Existing TS ecosystem |

Floe borrows Gleam's ideas (pipes, Result, strict type safety) but targets the TypeScript/React ecosystem.

## vs Elm

| | Elm | Floe |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Architecture** | TEA required | Any React pattern |
| **npm interop** | Ports (painful) | Direct imports |
| **Learning curve** | Steep | Gentle (familiar syntax) |
| **JSX** | No (virtual DOM DSL) | Yes |
| **Community** | Small | TS/React ecosystem |

Floe is less opinionated than Elm. It doesn't enforce an architecture. It makes your code safer without dictating how you structure it.

## vs ReScript

| | ReScript | Floe |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Syntax** | OCaml-inspired | TS-inspired |
| **JSX** | Custom (`@react.component`) | Standard JSX |
| **npm interop** | Bindings required | Direct imports |
| **Output** | JavaScript | TypeScript |

Floe's output is TypeScript, not JavaScript. This means the output itself is type-safe and can be checked by `tsc`.

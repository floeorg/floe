---
title: Comparison
---

How ZenScript compares to other languages in its space.

## vs TypeScript

| | TypeScript | ZenScript |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Null safety** | Optional (`strictNullChecks`) | Enforced (`Option<T>`) |
| **Error handling** | Exceptions | `Result<T, E>` |
| **Mutation** | `let`, `var`, `const` | `const` only |
| **Pattern matching** | No | Yes, exhaustive |
| **Pipes** | No (TC39 Stage 2) | `\|>` built in |
| **Classes** | Yes | No |
| **Escape hatches** | `any`, `as`, `!` | None |
| **Runtime** | None | None |

ZenScript is TypeScript with the escape hatches removed and functional features added.

## vs Gleam

| | Gleam | ZenScript |
|---|---|---|
| **Target** | Erlang/JS | TypeScript |
| **Ecosystem** | Hex/npm | npm |
| **JSX** | No | Yes |
| **React** | No | First-class |
| **Syntax** | ML-family | TS-family |
| **Pipes** | Yes | Yes |
| **Pattern matching** | Yes | Yes |
| **Adoption** | New ecosystem | Existing TS ecosystem |

ZenScript borrows Gleam's ideas (pipes, Result, no escape hatches) but targets the TypeScript/React ecosystem.

## vs Elm

| | Elm | ZenScript |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Architecture** | TEA required | Any React pattern |
| **npm interop** | Ports (painful) | Direct imports |
| **Learning curve** | Steep | Gentle (familiar syntax) |
| **JSX** | No (virtual DOM DSL) | Yes |
| **Community** | Small | TS/React ecosystem |

ZenScript is less opinionated than Elm. It doesn't enforce an architecture — it just makes your code safer.

## vs ReScript

| | ReScript | ZenScript |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Syntax** | OCaml-inspired | TS-inspired |
| **JSX** | Custom (`@react.component`) | Standard JSX |
| **npm interop** | Bindings required | Direct imports |
| **Output** | JavaScript | TypeScript |

ZenScript's output is TypeScript, not JavaScript. This means the output itself is type-safe and can be checked by `tsc`.

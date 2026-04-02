---
title: Introduction
---

Floe is a programming language that compiles to TypeScript. It's designed for TypeScript and React developers who want stronger guarantees without leaving their ecosystem.

## Why Floe?

TypeScript allows `any`, `null`, `undefined`, and type assertions. These lead to runtime errors that the type system was supposed to prevent.

Floe removes these and adds features that make correct code easy to write:

- **Pipes** (`|>`) for readable data transformations
- **Pattern matching** (`match`) with exhaustiveness checking
- **Result/Option** instead of null/undefined/exceptions
- **No `any`** - use `unknown` and narrow
- **No `null`/`undefined`** - use `Option<T>` with `Some`/`None`
- **No classes** - use functions and records

## What does it look like?

```floe
import { useState } from "react"

type Todo {
  id: string,
  text: string,
  done: boolean,
}

export fn App() -> JSX.Element {
  const [todos, setTodos] = useState<Array<Todo>>([])

  const completedCount = todos
    |> filter(.done)
    |> length

  <div>
    <h1>Todos ({completedCount} done)</h1>
  </div>
}
```

This compiles to clean, readable TypeScript:

```typescript
import { useState } from "react";

type Todo = {
  id: string;
  text: string;
  done: boolean;
};

export function App(): JSX.Element {
  const [todos, setTodos] = useState<Todo[]>([]);

  const completedCount = length(filter(todos, (t) => t.done));

  return <div>
    <h1>Todos ({completedCount} done)</h1>
  </div>;
}
```

## Design Philosophy

1. **Familiar syntax** - readable by TypeScript developers, not a new paradigm to learn
2. **Full TypeScript interop** - import TS into Floe and Floe into TS. Types, functions, and React components work both ways.
3. **Strictness is a feature** - no null, no any, no exceptions. Every restriction prevents a category of bugs.

## How Floe Compares

### vs Gleam

| | Gleam | Floe |
|---|---|---|
| **Target** | Erlang/JS | TypeScript |
| **Ecosystem** | Hex/npm | npm |
| **JSX** | No | Yes |
| **React** | No | First-class |
| **Syntax** | ML-family | TS-family |
| **Pipes** | Yes | Yes |
| **Pattern matching** | Yes | Yes |
| **Adoption** | Hex + npm | npm |

Floe borrows Gleam's ideas (pipes, Result, strict type safety) but targets the TypeScript/React ecosystem.

### vs Elm

| | Elm | Floe |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Architecture** | TEA required | Any React pattern |
| **npm interop** | Ports (indirect) | Direct imports |
| **Learning curve** | ML-family syntax | TS-family syntax |
| **JSX** | No (virtual DOM DSL) | Yes |
| **Community** | Small | TS/React ecosystem |

Floe does not enforce an architecture pattern. You choose how to structure your code.

### vs ReScript

| | ReScript | Floe |
|---|---|---|
| **Target** | JavaScript | TypeScript |
| **Syntax** | OCaml-inspired | TS-inspired |
| **JSX** | Custom (`@react.component`) | Standard JSX |
| **npm interop** | Bindings required | Direct imports |
| **Output** | JavaScript | TypeScript |

Floe's output is TypeScript, not JavaScript. The output itself is type-safe and can be checked by `tsc`.

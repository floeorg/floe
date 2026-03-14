---
title: Introduction
---

Floe is a programming language that compiles to TypeScript. It's designed for TypeScript and React developers who want stronger guarantees without leaving their ecosystem.

## Why Floe?

TypeScript is great, but it has escape hatches everywhere: `any`, `null`, `undefined`, type assertions. These lead to runtime errors that the type system was supposed to prevent.

Floe removes the escape hatches and adds features that make correct code easy to write:

- **Pipes** (`|>`) for readable data transformations
- **Pattern matching** (`match`) with exhaustiveness checking
- **Result/Option** instead of null/undefined/exceptions
- **No `any`** — use `unknown` and narrow
- **No `null`/`undefined`** — use `Option<T>` with `Some`/`None`
- **No classes** — use functions and records

## What does it look like?

```floe
import { useState } from "react"

type Todo = {
  id: string,
  text: string,
  done: boolean,
}

export fn App() -> JSX.Element {
  const [todos, setTodos] = useState([])

  const completed = todos
    |> filter(.done)
    |> length

  return <div>
    <h1>Todos ({completed} done)</h1>
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
  const [todos, setTodos] = useState([]);

  const completed = length(filter(todos, (t) => t.done));

  return <div>
    <h1>Todos ({completed} done)</h1>
  </div>;
}
```

## Design Philosophy

1. **Familiar syntax** — A React developer should understand Floe in 30 minutes
2. **No runtime** — The output is vanilla TypeScript with zero dependencies
3. **Eject anytime** — If you stop using Floe, you have normal `.ts` files
4. **Strictness is a feature** — Every restriction exists to prevent a category of bugs

---
title: Pipes
---

The pipe operator `|>` is ZenScript's signature feature. It lets you chain transformations left-to-right, making data flow readable.

## Basic Pipes

```zenscript
// Pipe the left side as the first argument to the right side
const result = "hello" |> toUpperCase
// Compiles to: toUpperCase("hello")
```

## Chaining

```zenscript
const result = users
  |> filter(u => u.active)
  |> map(u => u.name)
  |> sort
  |> join(", ")
```

Compiles to:

```typescript
const result = join(sort(map(filter(users, (u) => u.active), (u) => u.name)), ", ");
```

The piped version reads like a recipe: take users, filter, map, sort, join.

## Placeholder `_`

When the piped value isn't the first argument, use `_`:

```zenscript
const result = 5 |> add(3, _)
// Compiles to: add(3, 5)
```

```zenscript
const result = value
  |> multiply(_, 2)
  |> add(10, _)
  |> toString
```

## Method-Style Pipes

Pipes work with any function, including methods accessed via imports:

```zenscript
import { map, filter, reduce } from "ramda"

const total = orders
  |> filter(o => o.status == "complete")
  |> map(o => o.amount)
  |> reduce((sum, n) => sum + n, 0, _)
```

## When to Use Pipes

Pipes shine when you have a sequence of transformations. They replace:

| Instead of | Use |
|---|---|
| `c(b(a(x)))` | `x \|> a \|> b \|> c` |
| `x.map(...).filter(...).reduce(...)` | `x \|> map(...) \|> filter(...) \|> reduce(...)` |
| Temporary variables | Direct piping |

## Three Operators

ZenScript has exactly three arrow-like operators:

```
=>  arrow functions    (a) => a + 1
->  match arms         Ok(x) -> x
|>  pipe data          data |> transform
```

Each has a distinct purpose. No ambiguity.

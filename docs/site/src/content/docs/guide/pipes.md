---
title: Pipes
---

The pipe operator `|>` is Floe's signature feature. It lets you chain transformations left-to-right, making data flow readable.

## Basic Pipes

```floe
// Pipe the left side as the first argument to the right side
const result = "hello" |> toUpperCase
// Compiles to: toUpperCase("hello")
```

## Chaining

```floe
const result = users
  |> filter(.active)
  |> map(.name)
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

```floe
const result = 5 |> add(3, _)
// Compiles to: add(3, 5)
```

```floe
const result = value
  |> multiply(_, 2)
  |> add(10, _)
  |> toString
```

## Dot Shorthand

For simple field access or comparisons, use `.field`:

```floe
todos |> Array.filter(.done == false)
todos |> Array.map(.text)
users |> Array.sortBy(.name)
```

`.field` creates an implicit lambda. `.done == false` is shorthand for `|t| t.done == false`.

For anything more complex, use `|x|`:

```floe
todos |> Array.map(|t| Todo(..t, done: !t.done))
```

## Method-Style Pipes

Pipes work with any function, including methods accessed via imports:

```floe
import { map, filter, reduce } from "ramda"

const total = orders
  |> filter(.status == "complete")
  |> map(.amount)
  |> reduce(|sum, n| sum + n, 0, _)
```

## Debugging with `tap`

Need to inspect a value mid-pipeline without breaking the chain? Use `tap`:

```floe
const result = users
  |> Array.filter(.active)
  |> tap(Console.log)          // logs filtered users, passes them through
  |> Array.map(.name)
  |> Array.sort
```

`tap` calls the function you give it (for side effects like logging), then returns the original value unchanged. It compiles to an IIFE that calls the function and returns the value.

## When to Use Pipes

Pipes shine when you have a sequence of transformations. They replace:

| Instead of | Use |
|---|---|
| `c(b(a(x)))` | `x \|> a \|> b \|> c` |
| `x.map(...).filter(...).reduce(...)` | `x \|> map(...) \|> filter(...) \|> reduce(...)` |
| Temporary variables | Direct piping |

## Operators

Floe has three arrow-like operators:

```
|x|  anonymous lambdas   |a| a + 1
->   match arms / types  Ok(x) -> x, (string) -> number
|>   pipe data            data |> transform
```

Each has a distinct purpose. No ambiguity.

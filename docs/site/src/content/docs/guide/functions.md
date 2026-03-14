---
title: Functions & Const
---

## Const Declarations

All bindings are immutable. Use `const`:

```floe
const name = "Floe"
const count = 42
const active = true
```

With type annotations:

```floe
const name: string = "Floe"
const count: number = 42
```

### Destructuring

```floe
const [first, second] = getItems()
const { name, age } = getUser()
```

## Functions

```floe
fn add(a: number, b: number) -> number {
  return a + b
}
```

Exported functions **must** have return type annotations:

```floe
export fn greet(name: string) -> string {
  return `Hello, ${name}!`
}
```

### Default Parameters

```floe
fn greet(name: string = "world") -> string {
  return `Hello, ${name}!`
}
```

### Anonymous Functions (Lambdas)

Use `|x|` for inline anonymous functions:

```floe
todos |> Array.map(|t| t.text)
items |> Array.reduce(|acc, x| acc + x.price, 0)
onClick={|| setCount(count + 1)}
```

For simple field access, use dot shorthand:

```floe
todos |> Array.filter(.done == false)
todos |> Array.map(.text)
users |> Array.sortBy(.name)
```

**`const name = |x| ...` is a compile error.** If it has a name, use `fn`:

```floe
// COMPILE ERROR
const double = |x| x * 2

// correct
fn double(x: number) -> number { x * 2 }
```

### Function Types

Use `->` to describe function types:

```floe
type Transform = (string) -> number
type Predicate = (Todo) -> boolean
type Callback = () -> ()
```

### Async Functions

```floe
async fn fetchUser(id: string) -> Promise<User> {
  const response = await fetch(`/api/users/${id}`)
  return await response.json()
}
```

## What's Not Here

- **No `let` or `var`** — all bindings are `const`
- **No `class`** — use functions and records
- **No `this`** — functions are pure by default
- **No `function*` generators** — use arrays and pipes
- **No `=>`** — use `|x|` for lambdas, `->` for types and match arms

These are removed intentionally. See the [comparison](/guide/comparison) for the reasoning.

---
title: Functions & Const
---

## Const Declarations

All bindings are immutable. Use `const`:

```zenscript
const name = "ZenScript"
const count = 42
const active = true
```

With type annotations:

```zenscript
const name: string = "ZenScript"
const count: number = 42
```

### Destructuring

```zenscript
const [first, second] = getItems()
const { name, age } = getUser()
```

## Functions

```zenscript
function add(a: number, b: number): number {
  return a + b
}
```

Exported functions **must** have return type annotations:

```zenscript
export function greet(name: string): string {
  return `Hello, ${name}!`
}
```

### Default Parameters

```zenscript
function greet(name: string = "world"): string {
  return `Hello, ${name}!`
}
```

### Arrow Functions

```zenscript
const double = (x: number) => x * 2
const add = (a: number, b: number) => a + b
```

Single-argument arrows don't need parentheses:

```zenscript
const double = x => x * 2
```

### Async Functions

```zenscript
async function fetchUser(id: string): Promise<User> {
  const response = await fetch(`/api/users/${id}`)
  return await response.json()
}
```

## What's Not Here

- **No `let` or `var`** — all bindings are `const`
- **No `class`** — use functions and records
- **No `this`** — functions are pure by default
- **No `function*` generators** — use arrays and pipes

These are removed intentionally. See the [comparison](/guide/comparison) for the reasoning.

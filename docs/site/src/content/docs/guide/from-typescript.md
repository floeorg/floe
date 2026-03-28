---
title: Migrating from TypeScript
---

Floe is designed to be familiar to TypeScript developers. This guide covers the key differences.

## What Stays the Same

- Import/export syntax
- Template literals
- JSX
- Async/await
- Type annotations
- Generics

## What Changes

### `fn` instead of `function`

```typescript
// TypeScript
function greet(name: string): string {
  return `Hello, ${name}!`
}

// Floe
fn greet(name: string) -> string {
  `Hello, ${name}!`
}
```

### Pipes and dot shorthand instead of method chains

```typescript
// TypeScript
const result = items.filter(x => x.active)
onClick={() => setCount(count + 1)}

// Floe — closures use (x) => just like TS, but prefer pipes + dot shorthand
const result = items |> Array.filter(.active)
onClick={() => setCount(count + 1)}
```

### `->` for return types and function types

```typescript
// TypeScript
function add(a: number, b: number): number { ... }
type Transform = (s: string) => number

// Floe
fn add(a: number, b: number) -> number { ... }
type Transform = fn(string) -> number
```

### `const` only

```typescript
// TypeScript
let count = 0
count += 1

// Floe - no let, no mutation
const count = 0
const newCount = count + 1
```

### `==` is `===`

Floe's `==` compiles to `===`. There is no loose equality.

```floe
// Floe
x == y    // compiles to: x === y
x != y    // compiles to: x !== y
```

### Pipes instead of method chains

```typescript
// TypeScript
const result = users
  .filter(u => u.active)
  .map(u => u.name)
  .join(", ")

// Floe
const result = users
  |> Array.filter(.active)
  |> Array.map(.name)
  |> String.join(", ")
```

### Pattern matching instead of switch

```typescript
// TypeScript
switch (action.type) {
  case "increment": return state + 1
  case "decrement": return state - 1
  default: return state
}

// Floe
match action.type {
  "increment" -> state + 1,
  "decrement" -> state - 1,
  _ -> state,
}
```

### `try` instead of try/catch

```floe
// JSON.parse is in the stdlib - it already returns Result, no try needed
const result = JSON.parse(input)
match result {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

For **external TypeScript imports**, the `try` keyword wraps any expression in a try/catch and returns a `Result<T, Error>`. All TypeScript imports are treated as potentially throwing by default. The compiler requires `try` when calling them. For TS functions you know won't throw, use `trusted`:

```typescript
// TypeScript
try {
  const data = parseYaml(input)
  return data
} catch (e) {
  return null
}

// Floe - wrap throwing TS imports with `try`
import { parseYaml } from "yaml-lib"
const result = try parseYaml(input)
match result {
  Ok(data) -> Some(data),
  Err(_) -> None,
}
```

```floe
import { trusted capitalize, fetchUser } from "some-lib"

capitalize("hello")          // string, no try needed
const user = try fetchUser(id)  // Result<User, Error>

// Or mark the whole import as trusted:
import trusted { capitalize, slugify } from "string-utils"
```

### Option instead of null

```typescript
// TypeScript
function find(id: string): User | null {
  return users.find(u => u.id === id) ?? null
}

// Floe
fn find(id: string) -> Option<User> {
  match users |> find(.id == id) {
    Some(user) -> Some(user),
    None -> None,
  }
}
```

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
| `function` | Verbose | `fn` |
| `return` | Implicit returns | Last expression is the return value |

## Incremental Adoption

Floe compiles to `.ts/.tsx`, so you can adopt it file by file:

1. Add `floe` to your project
2. Write new files as `.fl`
3. Compile them alongside your existing `.ts` files
4. Your build tool (Vite, Next.js) treats the output as normal TypeScript

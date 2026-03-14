---
title: Types
---

## Primitives

```zenscript
const name: string = "Alice"
const age: number = 30
const active: bool = true
```

Note: ZenScript uses `bool`, not `boolean`.

## Record Types

```zenscript
type User = {
  name: string,
  email: string,
  age: number,
}
```

Construct records with the type name:

```zenscript
const user = User(name: "Alice", email: "a@b.com", age: 30)
```

Update with spread:

```zenscript
const updated = User(..user, age: 31)
```

## Union Types

Discriminated unions with variants:

```zenscript
type Color =
  | Red
  | Green
  | Blue
  | Custom(r: number, g: number, b: number)
```

## Result and Option

### Result

For operations that can fail:

```zenscript
type Result<T, E> = Ok(T) | Err(E)

const result = Ok(42)
const error = Err("something went wrong")
```

### Option

For values that may be absent:

```zenscript
type Option<T> = Some(T) | None

const found = Some("hello")
const missing = None
```

### The `?` Operator

Propagate errors concisely:

```zenscript
function getUsername(id: string): Result<string, Error> {
  const user = fetchUser(id)?   // returns Err early if it fails
  return Ok(user.name)
}
```

## Brand Types

Compile-time distinct types that erase at runtime:

```zenscript
type UserId = Brand<string, "UserId">
type PostId = Brand<string, "PostId">

// userId and postId are both strings at runtime,
// but can't be mixed up at compile time
```

## Opaque Types

Types where only the defining module can see the internal structure:

```zenscript
opaque type Email = string

// Only this module can construct/destructure Email values
```

## Type Aliases

```zenscript
type Name = string
type Callback = (event: Event) => void
```

## What's Banned

| Banned | Why | Use Instead |
|--------|-----|-------------|
| `any` | Disables type checking | `unknown` + narrowing |
| `null` | Billion-dollar mistake | `Option<T>` |
| `undefined` | Two nothings is one too many | `Option<T>` |
| `enum` | Quirky JS semantics | Union types |
| `interface` | Redundant with type | `type` |

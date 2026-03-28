---
title: Types
---

## Primitives

```floe
const name: string = "Alice"
const age: number = 30
const active: boolean = true
```

## Record Types

```floe
type User {
  name: string,
  email: string,
  age: number,
}
```

Construct records with the type name:

```floe
const user = User(name: "Alice", email: "a@b.com", age: 30)
```

Update with spread:

```floe
const updated = User(..user, age: 31)
```

### Record Type Composition

Include fields from other record types using spread syntax:

```floe
type BaseProps {
  className: string,
  disabled: boolean,
}

type ButtonProps {
  ...BaseProps,
  onClick: fn() -> (),
  label: string,
}
// ButtonProps has: className, disabled, onClick, label
```

Multiple spreads are allowed:

```floe
type A { x: number }
type B { y: string }
type C { ...A, ...B, z: boolean }
```

Rules:
- Spread must reference a record type (not a union or alias)
- Field name conflicts between spreads or with direct fields are compile errors
- The resulting type is a flat record

## Union Types

Discriminated unions with variants:

```floe
type Color {
  | Red
  | Green
  | Blue
  | Custom { r: number, g: number, b: number }
}
```

### Qualified Variants

Use `Type.Variant` to qualify which union a variant belongs to:

```floe
type Filter { | All | Active | Completed }

const f = Filter.All
const g = Filter.Active
setFilter(Filter.Completed)
```

When two unions share a variant name, the compiler requires qualification:

```floe
type Color { | Red | Green | Blue }
type Light { | Red | Yellow | Green }

const c = Red
// Error: variant `Red` is ambiguous — defined in both `Color` and `Light`
// Help: use `Color.Red` or `Light.Red`

const c = Color.Red   // OK
const l = Light.Red   // OK
```

Unambiguous variants can still be used bare. In match arms, bare variants always work because the type is known from the match subject:

```floe
match filter {
  All -> showAll(),
  Active -> showActive(),
  Completed -> showCompleted(),
}
```

## String Literal Unions

For npm interop with TypeScript libraries that use string literal unions:

```floe
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
type Status = "loading" | "error" | "success"
```

Match on them with exhaustiveness checking:

```floe
fn describe(method: HttpMethod) -> string {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
// Compiler error if you miss a variant
```

String literal unions compile to the same TypeScript type:

```typescript
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";
```

For pure Floe code, prefer regular tagged unions (`| Get | Post`) since they work with constructors, for-blocks, and provide better type safety. String literal unions exist primarily for seamless npm interop.

## Result and Option

### Result

For operations that can fail:

```floe
type Result<T, E> { | Ok { T } | Err { E } }

const result = Ok(42)
const error = Err("something went wrong")
```

### Option

For values that may be absent:

```floe
type Option<T> { | Some { T } | None }

const found = Some("hello")
const missing = None
```

### The `?` Operator

Propagate errors concisely:

```floe
fn getUsername(id: string) -> Result<string, Error> {
  const user = fetchUser(id)?   // returns Err early if it fails
  Ok(user.name)
}
```

## Newtypes

Single-variant wrappers that are distinct at compile time but erase at runtime:

```floe
type UserId { string }
type PostId { string }

// userId and postId are both strings at runtime,
// but can't be mixed up at compile time
```

## Opaque Types

Types where only the defining module can see the internal structure:

```floe
opaque type Email = string

// Only this module can construct/destructure Email values
```

## Tuple Types

Anonymous lightweight product types:

```floe
const point: (number, number) = (10, 20)
const entry: (string, number) = ("key", 42)
```

Destructure with pattern matching:

```floe
const (x, y) = point

fn divmod(a: number, b: number) -> (number, number) {
  (a / b, a % b)
}

match divmod(10, 3) {
  (_, 0) -> "divides evenly",
  (q, r) -> `${q} remainder ${r}`,
}
```

Tuples compile to TypeScript readonly tuples: `(number, string)` becomes `readonly [number, string]`, and `(1, "a")` becomes `[1, "a"] as const`.

## Type Aliases

```floe
type Name = string
type Callback = fn(Event) -> ()
```

## What's Banned

| Banned | Why | Use Instead |
|--------|-----|-------------|
| `any` | Disables type checking | `unknown` + narrowing |
| `null` | Nullable reference bugs | `Option<T>` |
| `undefined` | Redundant with null | `Option<T>` |
| `enum` | Compiles to runtime objects | Union types |
| `interface` | Redundant with type | `type` |

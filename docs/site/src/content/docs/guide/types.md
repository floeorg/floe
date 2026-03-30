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
  onClick: () -> (),
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

Spreads work with generic types and `typeof`, including npm imports:

```floe
import trusted { tv, VariantProps } from "tailwind-variants"

const cardVariants = tv({ base: "rounded-xl", variants: { padding: { sm: "p-4" } } })

type CardProps {
  ...VariantProps<typeof cardVariants>,
  className: string,
}
```

Rules:
- Spread can reference a record type or a generic/foreign type
- Field name conflicts between spreads or with direct fields are compile errors
- The resulting type compiles to a TypeScript intersection

### Default Field Values

Record fields can have default values. Fields with defaults can be omitted when constructing:

```floe
type Config {
  baseUrl: string,
  timeout: number = 5000,
  retries: number = 3,
}

const c1 = Config(baseUrl: "https://api.com")
// timeout is 5000, retries is 3

const c2 = Config(baseUrl: "https://api.com", timeout: 10000)
// timeout is 10000, retries is 3
```

Rules:
- Defaults must be compile-time constants or constructors (no function calls)
- Required fields (no default) must come before defaulted fields

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

## Variant Constructors as Functions

Non-unit variants (variants with fields) can be used as function values by referencing them without arguments:

```floe
type SaveError {
    | Validation { errors: Array<string> }
    | Api { message: string }
}

// Bare variant name becomes an arrow function
const toValidation = Validation
// Equivalent to: fn(errors) Validation(errors: errors)

// Qualified syntax works too
const toApi = SaveError.Api

// Most useful with higher-order functions like mapErr:
result |> Result.mapErr(Validation)
// Instead of: result |> Result.mapErr(fn(e) Validation(e))
```

Unit variants (no fields) are values, not functions — `All` produces `{ tag: "All" }` directly.

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

For operations that can fail. `Result` is a built-in type — no need to define it:

```floe
const result = Ok(42)
const error = Err("something went wrong")
```

### Option

For values that may be absent. `Option` is a built-in type — no need to define it:

```floe
const found = Some("hello")
const missing = None
```

### Settable

For fields that can be set, cleared, or left unchanged. Useful for partial updates (PATCH-style APIs):

```floe
type UpdateUser {
  name: Settable<string> = Unchanged,
  email: Settable<string> = Unchanged,
}

// Set a value
UpdateUser(name: Value("Ryan"), email: Clear)
// → { name: "Ryan", email: null }

// Leave fields unchanged (omitted from output)
UpdateUser(name: Value("Ryan"))
// → { name: "Ryan" }
```

Three variants:
- `Value(x)` — set the field to `x`
- `Clear` — set the field to `null`
- `Unchanged` — omit the field entirely (default)

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
opaque type Email { string }

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
type Callback = (Event) -> ()
```

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + narrowing |
| `null`, `undefined` | `Option<T>` |
| `enum` | Union types |
| `interface` | `type` |

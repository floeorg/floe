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

Two types with identical fields are NOT interchangeable. `User` is not `Product` even if both have `name: string`.

### Default Field Values

Fields with defaults can be omitted when constructing:

```floe
type Config {
  baseUrl: string,
  timeout: number = 5000,
  retries: number = 3,
}

const c = Config(baseUrl: "https://api.com")
// timeout is 5000, retries is 3
```

Rules:
- Defaults must be compile-time constants or constructors (no function calls)
- Required fields (no default) must come before defaulted fields

### Record Composition

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
import { tv, VariantProps } from "tailwind-variants"

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

## Union Types

Discriminated unions with variants. Positional fields use `( )`, named fields use `{ }`:

```floe
type Color {
  | Red
  | Green
  | Blue
  | Custom { r: number, g: number, b: number }
}

type Shape {
  | Circle(number)
  | Rect(number, number)
  | Point
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

### Variant Constructors as Functions

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

Unit variants (no fields) are values, not functions.

## Result and Option

`Result` and `Option` are built-in union types with positional variants:

```floe
// Equivalent to:  type Option<T> { | Some(T) | None }
// Equivalent to:  type Result<T, E> { | Ok(T) | Err(E) }
```

### Result

For operations that can fail:

```floe
const result = Ok(42)
const error = Err("something went wrong")
```

### Option

For values that may be absent:

```floe
const found = Some("hello")
const missing = None
```

### Settable

`Settable<T>` is a three-state type for partial updates. This is the problem it solves: in a PATCH API, you need to distinguish between "set this field to a value", "clear this field to null", and "don't touch this field". TypeScript's `Partial<T>` can't tell the difference between "set to undefined" and "not provided".

```floe
type Settable<T> {
  | Value(T)
  | Clear
  | Unchanged
}
```

Use it with default field values so callers only specify what they're changing:

```floe
type UpdateUser {
  name: Settable<string> = Unchanged,
  email: Settable<string> = Unchanged,
  avatar: Settable<string> = Unchanged,
}

// Set name, clear avatar, leave email alone
const patch = UpdateUser(name: Value("Ryan"), avatar: Clear)
```

#### What it compiles to

`Settable` fields have special codegen. `Unchanged` fields are **omitted entirely** from the output object:

| Floe | TypeScript output |
|------|-------------------|
| `Value("Ryan")` | `"Ryan"` |
| `Clear` | `null` |
| `Unchanged` | *(key omitted)* |

So `UpdateUser(name: Value("Ryan"), avatar: Clear)` compiles to `{ name: "Ryan", avatar: null }` -- no `email` key at all.

#### Real-world example: PATCH endpoint

```floe
fn updateProfile(id: string, patch: UpdateUser) -> Result<User, ApiError> {
    const response = Http.put("/api/users/{id}", patch) |> Promise.await?
    response |> Http.json? |> parse<User>
}

// Only update what changed
updateProfile("123", UpdateUser(
    name: Value("New Name"),
))
// Sends: { name: "New Name" } — email and avatar untouched
```

#### Comparison with TypeScript

| Approach | "set to value" | "clear to null" | "don't change" |
|----------|---------------|-----------------|-----------------|
| TS `Partial<T>` | `{ name: "x" }` | `{ name: null }` | `{ }` or `{ name: undefined }` (ambiguous!) |
| Floe `Settable<T>` | `Value("x")` | `Clear` | `Unchanged` (omitted from output) |

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
type UserId(string)
type PostId(string)

// Both strings at runtime, but can't be mixed up at compile time
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

fn divmod(a: number, b: number) -> (number, number) {
  (a / b, a % b)
}

const (q, r) = divmod(10, 3)
```

Tuples compile to TypeScript readonly tuples: `(number, string)` becomes `readonly [number, string]`.

## TypeScript Bridge Types

When working with npm libraries, use `type Name = ...` to alias existing TypeScript types:

```floe
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
type DivProps = ComponentProps<"div">
type CardProps = VariantProps<typeof cardVariants> & { className: string }
```

String literal unions work with exhaustive matching:

```floe
fn describe(method: HttpMethod) -> string {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
```

For your own data, prefer union types (`type Method { | Get | Post }`) over string literals. Use `=` only when bridging to TypeScript libraries.

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + narrowing |
| `null`, `undefined` | `Option<T>` |
| `enum` | Union types |
| `interface` | `type` |

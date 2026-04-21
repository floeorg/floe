---
title: Types
---

## Primitives

```floe
let name: string = "Alice"
let age: number = 30
let active: boolean = true
```

## Declaring Types

Every type declaration looks like this:

```floe
type Name = RHS
```

The RHS picks what kind of type you get:

| RHS | Kind |
|---|---|
| `{ ... }` | Record |
| `A \| B \| ...` | Tagged sum |
| `Name(T)` | Newtype |
| `(T, ...) => Ret` or `(label: T, ...) => Ret` | Function-type alias (parameter labels optional) |
| `OneOf<"a", "b">` | Structural string-literal union |
| `Intersect<A, B>` | Structural intersection |

## Records

```floe
type User = {
  name: string,
  email: string,
  age: number,
}
```

Construct records with the type name:

```floe
let user = User(name: "Alice", email: "a@b.com", age: 30)
```

Update with spread — explicit fields first, `..base` last, explicit wins:

```floe
let updated = User(age: 31, ..user)
```

Two types with identical fields are NOT interchangeable. `User` is not `Product` even if both have `name: string`.

### Default Field Values

Fields with defaults can be omitted when constructing:

```floe
type Config = {
  baseUrl: string,
  timeout: number = 5000,
  retries: number = 3,
}

let c = Config(baseUrl: "https://api.com")
// timeout is 5000, retries is 3
```

Rules:
- Defaults must be compile-time constants or constructors (no function calls)
- Required fields (no default) must come before defaulted fields

### Record Composition

Include fields from other record types using spread syntax:

```floe
type BaseProps = {
  className: string,
  disabled: boolean,
}

type ButtonProps = {
  ...BaseProps,
  onClick: () -> (),
  label: string,
}
// ButtonProps has: className, disabled, onClick, label
```

Multiple spreads are allowed:

```floe
type A = { x: number }
type B = { y: string }
type C = { ...A, ...B, z: boolean }
```

Spreads work with generic types and `typeof`, including npm imports:

```floe
import { tv, VariantProps } from "tailwind-variants"

let cardVariants = tv({ base: "rounded-xl", variants: { padding: { sm: "p-4" } } })

type CardProps = {
  ...VariantProps<typeof cardVariants>,
  className: string,
}
```

Rules:
- Spread can reference a record type or a generic/foreign type
- Field name conflicts between spreads or with direct fields are compile errors
- The resulting type compiles to a TypeScript intersection

## Tagged Sums

Discriminated unions with nominal variants. The leading `|` is optional. Positional fields use `( )`, named fields use `{ }`:

```floe
type Color =
  | Red
  | Green
  | Blue
  | Custom { r: number, g: number, b: number }

type Shape = Circle(number) | Rect(number, number) | Point
```

`|` at the top level of a `type` declaration always declares fresh constructors. If you want a structural string union instead, use `OneOf<>`.

### Qualified Variants

Use `Type.Variant` to qualify which sum a variant belongs to:

```floe
type Filter = All | Active | Completed

let f = Filter.All
let g = Filter.Active
setFilter(Filter.Completed)
```

When two sums share a variant name, the compiler requires qualification:

```floe
type Color = Red | Green | Blue
type Light = Red | Yellow | Green

let c = Red
// Error: variant `Red` is ambiguous — defined in both `Color` and `Light`
// Help: use `Color.Red` or `Light.Red`

let c = Color.Red   // OK
let l = Light.Red   // OK
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
type SaveError =
    | Validation { errors: Array<string> }
    | Api { message: string }

// Bare variant name becomes an arrow function
let toValidation = Validation
// Equivalent to: fn(errors) Validation(errors: errors)

// Qualified syntax works too
let toApi = SaveError.Api

// Most useful with higher-order functions like mapErr:
result |> Result.mapErr(Validation)
```

Unit variants (no fields) are values, not functions.

## Result and Option

`Result` and `Option` are built-in tagged sums:

```floe
// Equivalent to:  type Option<T> = Some(T) | None
// Equivalent to:  type Result<T, E> = Ok(T) | Err(E)
```

### Result

For operations that can fail:

```floe
let result = Ok(42)
let error = Err("something went wrong")
```

### Option

For values that may be absent:

```floe
let found = Some("hello")
let missing = None
```

### Settable

`Settable<T>` is a three-state type for partial updates. In a PATCH API, you need to distinguish between "set this field to a value", "clear this field to null", and "don't touch this field". TypeScript's `Partial<T>` can't tell the difference between "set to undefined" and "not provided".

```floe,ignore
type Settable<T> = Value(T) | Clear | Unchanged
```

Use it with default field values so callers only specify what they're changing:

```floe
type UpdateUser = {
  name: Settable<string> = Unchanged,
  email: Settable<string> = Unchanged,
  avatar: Settable<string> = Unchanged,
}

// Set name, clear avatar, leave email alone
let patch = UpdateUser(name: Value("Ryan"), avatar: Clear)
```

#### What it compiles to

`Settable` fields have special codegen. `Unchanged` fields are **omitted entirely** from the output object:

| Floe | TypeScript output |
|------|-------------------|
| `Value("Ryan")` | `"Ryan"` |
| `Clear` | `null` |
| `Unchanged` | *(key omitted)* |

So `UpdateUser(name: Value("Ryan"), avatar: Clear)` compiles to `{ name: "Ryan", avatar: null }` -- no `email` key at all.

### The `?` Operator

Propagate errors concisely:

```floe
let getUsername(id: string) -> Result<string, Error> = {
  let user = fetchUser(id)?   // returns Err early if it fails
  Ok(user.name)
}
```

## Newtypes

Single-variant wrappers that are distinct at compile time but erase at runtime:

```floe
type UserId = UserId(string)
type PostId = PostId(string)

// Both strings at runtime, but can't be mixed up at compile time
```

The constructor name typically matches the type name — that is the idiomatic form.

## Opaque Types

Types where only the defining module can see the internal structure:

```floe
opaque type Email = Email(string)

// Only this module can construct/destructure Email values
```

## Function-Type Aliases

Name a function type to use it in records or generics. Parameter labels are optional documentation:

```floe
type Handler = (req: Request) -> Promise<Response>
type Predicate<T> = (T) -> boolean

type Button = {
  label: string,
  onClick: () -> (),
  onSubmit: (form: FormData) -> (),
}
```

Labels never affect structural assignability — `(x: Int) -> Int`, `(y: Int) -> Int`, and `(Int) -> Int` are all the same type. Add labels when the name carries meaning (DDD-style workflow types, multi-param callbacks); skip them when the position is obvious.

## Tuple Types

Anonymous lightweight product types:

```floe
let point: (number, number) = (10, 20)

let divmod(a: number, b: number) -> (number, number) = {
  (a / b, a % b)
}

let (q, r) = divmod(10, 3)
```

Tuples compile to TypeScript readonly tuples: `(number, string)` becomes `readonly [number, string]`.

## Structural TypeScript Types

When bridging to TypeScript libraries, two utility types cover the structural operators TS uses that have no nominal equivalent in Floe:

```floe
type HttpMethod = OneOf<"GET", "POST", "PUT", "DELETE">
type CardProps = Intersect<VariantProps<typeof cardVariants>, { className: string }>
```

- `OneOf<A, B, ...>` compiles to `A | B | ...`
- `Intersect<A, B, ...>` compiles to `A & B & ...`

Alias existing TypeScript types with plain `=`:

```floe
type DivProps = ComponentProps<"div">
type PartialUser = Partial<User>
```

String-literal unions work with exhaustive matching:

```floe
let describe(method: HttpMethod) -> string = {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
```

For your own data, prefer tagged sums (`type Method = Get | Post`). Reach for `OneOf<>` and `Intersect<>` when you need the structural shapes TypeScript libraries hand you.

## Common Errors

| Code | Trigger | Fix |
|---|---|---|
| `E201` | Bare string-literal union (`type M = "a" \| "b"`) | Use `OneOf<"a", "b">` |
| `E202` | Inline record in a function signature | Name the type: `type Arg = { ... }` then `fn f(x: Arg)` |

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + narrowing |
| `null`, `undefined` | `Option<T>` |
| `enum` | Tagged sums |
| `interface` | `type` |
| `"a" \| "b"` | `OneOf<"a", "b">` |
| `A & B` | `Intersect<A, B>` (or record spread) |
| `(x: T) => U` (in types) | `(T) => U` |

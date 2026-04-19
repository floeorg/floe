---
title: Types Reference
---

## Primitive Types

| Type | Description | Example |
|------|-------------|---------|
| `string` | Text | `"hello"` |
| `number` | Integer or float | `42`, `3.14` |
| `boolean` | Boolean | `true`, `false` |

## Built-in Generic Types

| Type | Description |
|------|-------------|
| `Result<T, E>` | Success (`Ok(T)`) or failure (`Err(E)`) |
| `Option<T>` | Present (`Some(T)`) or absent (`None`) |
| `Settable<T>` | Three-state: `Value(T)`, `Clear`, or `Unchanged` (for partial updates) |
| `Array<T>` | Ordered collection |
| `Map<K, V>` | Immutable key-value map |
| `Set<T>` | Immutable unique collection |
| `Promise<T>` | Async value |
| `JSX.Element` | React JSX element |

## Type Declarations

Every type declaration starts with `type Name = RHS`. The shape of the RHS picks the kind:

| RHS | Kind |
|---|---|
| `{ field: T, ... }` | Record |
| `A \| B \| ...` (constructors) | Tagged sum (nominal — declares fresh constructors) |
| `Name(T)` | Newtype (single-value wrapper) |
| `(Args) => Ret` | Function-type alias (structural) |
| `OneOf<"a", "b", ...>` | Structural string-literal union |
| `Intersect<A, B, ...>` | Structural intersection |
| `Partial<T>` / `Pick<T, K>` / `Omit<T, K>` / `ReturnType<...>` / ... | TS utility alias (pass-through) |

`|` at the top level of a `type` declaration is **always nominal**. For a structural union of string literals, use `OneOf<>`.

## Records

Named product types with fields:

```floe
type User = {
  name: string,
  email: string,
  age: number,
}
```

Compiles to:

```typescript
type User = {
  name: string;
  email: string;
  age: number;
};
```

### Default Field Values

Record fields can have default values, used when the field is omitted at construction:

```floe
type Config = {
  baseUrl: string,
  timeout: number = 5000,
  retries: number = 3,
}

let cfg = Config(baseUrl: "https://api.com")
// timeout is 5000, retries is 3
```

### Record Composition

Include fields from other record types using `...` spread:

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
```

Compiles to a TypeScript intersection:

```typescript
type BaseProps = { className: string; disabled: boolean };
type ButtonProps = BaseProps & { onClick: () => void; label: string };
```

Spreads work with generic types, `typeof`, and npm imports:

```floe
import { tv, VariantProps } from "tailwind-variants"

let cardVariants = tv({ base: "rounded-xl", variants: { padding: { sm: "p-4" } } })

type CardProps = {
  ...VariantProps<typeof cardVariants>,
  className: string,
}
```

## Tagged Sums

Nominal discriminated unions. The leading `|` is optional. Positional fields use `( )`, named fields use `{ }`:

```floe
type Shape =
  | Circle(number)
  | Rectangle { width: number, height: number }
  | Point

// Inline form
type Filter = All | Active | Completed
```

Compiles to a TypeScript discriminated union:

```typescript
type Shape =
  | { __tag: "Circle"; value: number }
  | { __tag: "Rectangle"; width: number; height: number }
  | { __tag: "Point" };
```

Positional: single field uses `value`, multiple use `_0`, `_1`. Named fields keep their names.

## Newtypes

Single-variant wrappers that are distinct at compile time but erase to their base type at runtime:

```floe
type UserId = UserId(string)
type PostId = PostId(string)
```

`UserId` and `PostId` are both `string` at runtime, but the compiler prevents mixing them up. The constructor name typically matches the type name.

## Opaque Types

Types where internals are hidden from other modules:

```floe
opaque type Email = Email(string)
```

Only code in the module that defines `Email` can construct or destructure it. Other modules see it as an opaque blob.

## Function-Type Aliases

Structural function types. Use `=>` between the parameter list and return type:

```floe
type Handler = (Request) -> Promise<Response>
type Predicate<T> = (T) -> boolean
type Reducer<S, A> = (S, A) -> S
```

## Structural String-Literal Unions (`OneOf<>`)

For npm interop and config values:

```floe
type HttpMethod = OneOf<"GET", "POST", "PUT", "DELETE">
```

Compiles to `type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"`.

Match arms use string comparisons:

```floe
match method {
    "GET" -> "fetching",
    "POST" -> "creating",
    "PUT" -> "updating",
    "DELETE" -> "removing",
}
```

Exhaustiveness is checked. Writing `type M = "a" | "b"` with a bare `|` is a compile error (**E201**) — use `OneOf<"a", "b">`.

## Structural Intersections (`Intersect<>`)

Combine types structurally:

```floe
type AdminCard = Intersect<ButtonProps, { role: "admin" }>
type CardProps = Intersect<VariantProps<typeof variants>, { className: string }>
```

Compiles to `A & B`. For Floe-native record composition, prefer `...Spread` in a `{ }` record body.

## Type Aliases (TS utilities)

```floe
type DivProps = ComponentProps<"div">
type PartialUser = Partial<User>
type UserKeys = Pick<User, "name" | "email">
```

Recognized utilities pass through unchanged: `OneOf`, `Intersect`, `Partial`, `Required`, `Readonly`, `Pick`, `Omit`, `NonNullable`, `Record`, `Extract`, `Exclude`, `ReturnType`, `Parameters`, `ConstructorParameters`, `Awaited`, `InstanceType`, `Uppercase`, `Lowercase`, `Capitalize`, `Uncapitalize`.

## Type Expressions

```floe
// Named
User
string

// Generic
Array<number>
Result<User, Error>
Option<string>

// Function (structural)
(number, number) -> number

// Tuple
(string, number)

// String literal argument (for npm interop)
ComponentProps<"div">
```

Inline record types (`{ a: T }`) are allowed inside generics and value positions but **not** directly in function signatures — that is a compile error (**E202**). Name the type instead.

## Errors

| Code | Trigger | Fix |
|---|---|---|
| `E201` | Bare string-literal union (`type M = "a" \| "b"`) | Use `OneOf<"a", "b">` |
| `E202` | Inline record in a function signature (`fn f(x: { a: T })`) | Name the type: `type Arg = { a: T }` then `fn f(x: Arg)` |

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + pattern matching |
| `null`, `undefined` | `Option<T>` with `None` |
| `enum` | Tagged sums |
| `interface` | `type` |
| `void` | Unit type `()` |
| `(x: T) => U` (in types) | `(T) => U` |
| `"a" \| "b"` (string literal union) | `OneOf<"a", "b">` |
| `A & B` | `Intersect<A, B>` (or `...Spread` in record composition) |

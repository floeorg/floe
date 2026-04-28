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

Floe has two type-declaration keywords:

- `type Name = RHS` — **nominal** declaration. `Name` is a new, distinct identity.
- `typealias Name = RHS` — **structural** alias. `Name` and `RHS` are interchangeable.

The RHS shape determines which keyword is allowed:

| RHS | `type` | `typealias` |
|---|---|---|
| `{ field: T, ... }` | Nominal record (default) | Structural alias over the shape |
| `{ ...Other, foo: T }` | Nominal record with spread | Structural intersection (`Other & { foo: T }`) |
| `A \| B \| ...` (constructors) | Tagged sum | **Error** — requires nominal identity |
| `Name(T)` | Newtype | **Error** — requires nominal identity |
| `(T, ...) -> Ret` | **Error** — no nominal identity | Structural function-type alias |
| `A & B` | **Error** — no nominal identity | Structural intersection |
| `typeof value` | **Error** — no nominal identity | Structural typeof alias |
| `Partial<T>` / `ReturnType<...>` / generic application | **Error** — no nominal identity | TS utility alias (pass-through) |
| `"a" \| "b" \| ...` | String-literal union | String-literal union (same codegen) |

`opaque type Name = RHS` is nominal on the outside but wraps an arbitrary structural shape inside. Use it when you need to give a structural type nominal identity explicitly (e.g. `opaque type HashedPw = string`).

`|` at the top level of a `type` declaration is **always nominal** — it declares fresh constructors.

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

Include fields from other record types using `...` spread. For structural prop shapes that don't need their own nominal identity, use `typealias` — the spread becomes a TypeScript intersection:

```floe
type BaseProps = {
  className: string,
  disabled: boolean,
}

typealias ButtonProps = {
  ...BaseProps,
  onClick: () -> (),
  label: string,
}
```

Compiles to:

```typescript
type BaseProps = { className: string; disabled: boolean };
type ButtonProps = BaseProps & { onClick: () => void; label: string };
```

If `ButtonProps` had used `type` instead of `typealias`, it would still compile to the same TypeScript but would require explicit construction (`ButtonProps(...)`) at the Floe level — `typealias` is the right choice when you just want a structural shape, which is typical for UI prop types.

Spreads work with generic types, `typeof`, and npm imports:

```floe
import { tv, VariantProps } from "tailwind-variants"

let cardVariants = tv({ base: "rounded-xl", variants: { padding: { sm: "p-4" } } })

typealias CardProps = {
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

Structural function types. Use `->` between the parameter list and return type. Parameter labels are optional documentation:

```floe
typealias Handler = (req: Request) -> Promise<Response>
typealias Predicate<T> = (T) -> boolean
typealias Reducer<S, A> = (state: S, action: A) -> S
```

Labels never affect structural assignability — `(x: Int) -> Int` is interchangeable with `(y: Int) -> Int` and with the unlabelled `(Int) -> Int`. Use them when the name carries meaning (DDD-style workflow types, multi-param callbacks); skip them when the position is obvious. LSP hover surfaces whichever form you wrote.

## Structural String-Literal Unions (`OneOf<>`)

For npm interop and config values:

```floe
typealias HttpMethod = OneOf<"GET", "POST", "PUT", "DELETE">
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
typealias AdminCard = Intersect<ButtonProps, { role: "admin" }>
typealias CardProps = Intersect<VariantProps<typeof variants>, { className: string }>
```

Compiles to `A & B`. For Floe-native record composition, prefer `...Spread` in a `{ }` record body.

## Type Aliases (TS utilities)

```floe
typealias DivProps = ComponentProps<"div">
typealias PartialUser = Partial<User>
typealias UserKeys = Pick<User, OneOf<"name", "email">>
```

Recognized utilities pass through unchanged: `OneOf`, `Intersect`, `Partial`, `Required`, `Readonly`, `Pick`, `Omit`, `NonNullable`, `Record`, `Extract`, `Exclude`, `ReturnType`, `Parameters`, `ConstructorParameters`, `Awaited`, `InstanceType`, `Uppercase`, `Lowercase`, `Capitalize`, `Uncapitalize`.

## Type Expressions

```floe,ignore
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
| `E202` | Inline record in a function signature (`let f(x: { a: T })`) | Name the type: `type Arg = { a: T }` then `let f(x: Arg) -> ...` |

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + pattern matching |
| `null`, `undefined` | `Option<T>` with `None` |
| `enum` | Tagged sums |
| `interface` | `type` |
| `void` | Unit type `()` |
| `(x: T) => U` (in types) | `(T) -> U` |
| `"a" \| "b"` (string literal union) | `OneOf<"a", "b">` |
| `A & B` | `Intersect<A, B>` (or `...Spread` in record composition) |

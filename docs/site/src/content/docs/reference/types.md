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
| `Array<T>` | Ordered collection |
| `Promise<T>` | Async value |

## Type Declarations

Floe has two forms of type declaration:

| Syntax | Meaning | Used for |
|---|---|---|
| `type Name { ... }` | Define a new Floe type | Records, unions, newtypes, opaque types |
| `type Name = ...` | Alias a TypeScript type | TS interop aliases, string literal unions, `&` intersections |

## Floe Types (`{ }` syntax)

### Record Types

Named product types with fields:

```floe
type User {
  name: string,
  email: string,
  age: number,
}
```

Compiles to TypeScript `type`:

```typescript
type User = {
  name: string;
  email: string;
  age: number;
};
```

### Record Type Composition

Include fields from other record types using `...` spread:

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
```

Compiles to TypeScript intersection:

```typescript
type BaseProps = { className: string; disabled: boolean };
type ButtonProps = BaseProps & { onClick: () => void; label: string };
```

Multiple spreads are allowed. Field name conflicts are compile errors.

Spreads work with generic types, `typeof`, and npm imports:

```floe
import trusted { tv, VariantProps } from "tailwind-variants"

const cardVariants = tv({ base: "rounded-xl", variants: { padding: { sm: "p-4" } } })

type CardProps {
  ...VariantProps<typeof cardVariants>,
  className: string,
}
```

Compiles to:

```typescript
type CardProps = VariantProps<typeof cardVariants> & { className: string };
```

### Union Types

Tagged discriminated unions:

```floe
type Shape {
  | Circle { radius: number }
  | Rectangle { width: number, height: number }
  | Point
}
```

Compiles to TypeScript discriminated union:

```typescript
type Shape =
  | { _tag: "Circle"; radius: number }
  | { _tag: "Rectangle"; width: number; height: number }
  | { _tag: "Point" };
```

### Newtypes

Single-variant wrappers that are distinct at compile time but erase to their base type at runtime:

```floe
type UserId { string }
type PostId { string }
```

`UserId` and `PostId` are both `string` at runtime, but the compiler prevents mixing them up.

### Opaque Types

Types where internals are hidden from other modules:

```floe
opaque type Email { string }
```

Only code in the module that defines `Email` can construct or destructure it. Other modules see it as an opaque blob.

## TS Bridge Types (`=` syntax)

### String Literal Unions

String literal unions for npm interop:

```floe
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
```

Compiles to the same TypeScript type (pass-through):

```typescript
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";
```

Match arms use string comparisons instead of tag checks:

```floe
match method {
    "GET" -> "fetching",
    "POST" -> "creating",
    "PUT" -> "updating",
    "DELETE" -> "removing",
}
```

Exhaustiveness is checked -- missing a variant is a compile error.

### Type Aliases

```floe
type DivProps = ComponentProps<"div">
```

### Intersections

Combine TypeScript types with `&` (only valid in `=` declarations):

```floe
type CardProps = VariantProps<typeof variants> & { className: string }
```

For Floe-native record composition, use `...Spread` in `{ }` definitions instead.

## Type Expressions

```floe
// Named
User
string

// Generic
Array<number>
Result<User, Error>
Option<string>

// Function
(number, number) -> number

// Record (inline)
{ name: string, age: number }

// Array
Array<T>

// Tuple
(string, number)

// String literal (for npm interop)
ComponentProps<"div">
```

## Differences from TypeScript

| TypeScript | Floe equivalent |
|------------|----------------|
| `any` | `unknown` + pattern matching |
| `null`, `undefined` | `Option<T>` with `None` |
| `enum` | Union types |
| `interface` | `type` |
| `void` | Unit type `()` |

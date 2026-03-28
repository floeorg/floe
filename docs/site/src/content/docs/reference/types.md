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

## Record Types

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
  onClick: fn() -> (),
  label: string,
}
```

Compiles to TypeScript intersection:

```typescript
type BaseProps = { className: string; disabled: boolean };
type ButtonProps = BaseProps & { onClick: () => void; label: string };
```

Multiple spreads are allowed. Field name conflicts are compile errors.

## Union Types

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

## String Literal Unions

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

## Newtypes

Single-variant wrappers that are distinct at compile time but erase to their base type at runtime:

```floe
type UserId { string }
type PostId { string }
```

`UserId` and `PostId` are both `string` at runtime, but the compiler prevents mixing them up.

## Opaque Types

Types where internals are hidden from other modules:

```floe
opaque type Email = string
```

Only code in the module that defines `Email` can construct or destructure it. Other modules see it as an opaque blob.

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
fn(number, number) -> number

// Record (inline)
{ name: string, age: number }

// Array
Array<T>

// Tuple
(string, number)
```

## Banned Types

| Type | Why Banned | Alternative |
|------|-----------|-------------|
| `any` | Disables all type checking | `unknown` + pattern matching |
| `null` | Nullable reference bugs | `Option<T>` with `None` |
| `undefined` | Same problem as null | `Option<T>` with `None` |
| `enum` | Compiles to runtime objects | Union types |
| `interface` | Redundant with `type` | `type` |
| `void` | Implicit undefined | Explicit return types |

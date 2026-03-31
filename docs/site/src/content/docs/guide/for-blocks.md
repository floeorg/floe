---
title: For Blocks & Traits
---

`for` blocks let you group functions under a type. Think of them as methods without classes. `self` is an explicit parameter, not magic.

## Basic Usage

```floe
type User { name: string, age: number }

for User {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }

  fn isAdult(self) -> boolean {
    self.age >= 18
  }

  fn greet(self, greeting: string) -> string {
    `${greeting}, ${self.name}!`
  }
}
```

The `self` parameter's type is inferred from the `for` block. No annotation needed.

## Pipes

For-block functions are pipe-friendly. `self` is always the first argument:

```floe
user |> display           // display(user)
user |> greet("Hello")    // greet(user, "Hello")
```

This gives you method-call ergonomics without OOP:

```floe
const message = user
  |> greet("Hi")
  |> String.toUpperCase
```

## Generic Types

For blocks work with generic types:

```floe
for Array<User> {
  fn adults(self) -> Array<User> {
    self |> Array.filter(.age >= 18)
  }
}

users |> adults  // only adult users
```

## Importing For-Block Functions

When for-block functions are defined in a different file from the type, use `import { for Type }`:

```floe
// Import specific for-block functions by type
import { for User } from "./user-helpers"
import { for Array, for Map } from "./collections"

// Mix with regular imports
import { Todo, Filter, for Array, for string } from "./todo"
```

`import { for Type }` brings all exported for-block functions for that type from the imported file. For generic types, use the base type only (no type params) -- `import { for Array }` brings all `for Array<T>` extensions.

Importing a type still auto-imports its for-block functions from the same file. The `import { for Type }` syntax is for cross-file for-blocks.

## Real-World Example

From the todo app, validating input strings and filtering todos:

```floe
for string {
  export fn validate(self) -> Validation {
    const trimmed = self |> trim
    const len = trimmed |> String.length
    match len {
      0 -> Empty,
      1 -> TooShort,
      _ -> match len > 100 {
        true -> TooLong,
        false -> Valid(trimmed),
      },
    }
  }
}

for Array<Todo> {
  export fn filterBy(self, f: Filter) -> Array<Todo> {
    match f {
      All -> self,
      Active -> self |> filter(.done == false),
      Completed -> self |> filter(.done == true),
    }
  }

  export fn remaining(self) -> number {
    self
      |> filter(.done == false)
      |> length
  }
}
```

Then import them in another file:

```floe
import { Todo, Filter } from "./types"
import { for string, for Array } from "./todo"

const visible = todos |> filterBy(filter)
const remaining = todos |> remaining
```

## Export

For-block functions can be exported by placing `export` before `fn` inside the block:

```floe
for User {
  export fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

## Rules

1. `self` is always the explicit first parameter. Its type is inferred.
2. No `this`, no implicit context
3. Multiple `for` blocks per type are allowed, even across files
4. Compiles to standalone TypeScript functions (no classes)

## What It Compiles To

```floe
for User {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

Becomes:

```typescript
function display(self: User): string {
  return `${self.name} (${self.age})`;
}
```

No class wrappers, no prototype chains. Plain functions.

## Traits

Traits define behavioral contracts that types can implement. They work with `for` blocks to ensure types provide specific functionality.

### Defining a Trait

A trait declares method signatures that implementing types must provide:

```floe
trait Display {
  fn display(self) -> string
}
```

### Implementing a Trait

Use `for Type: Trait` to implement a trait for a type:

```floe
type User { name: string, age: number }

for User: Display {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

The compiler checks that all required methods are implemented. Missing methods produce a clear error.

### Default Implementations

Traits can provide default method bodies. Implementors inherit them unless they override:

```floe
trait Eq {
  fn eq(self, other: string) -> boolean
  fn neq(self, other: string) -> boolean {
    !(self |> eq(other))
  }
}

for User: Eq {
  fn eq(self, other: string) -> boolean {
    self.name == other
  }
  // neq is inherited from the default implementation
}
```

### Multiple Traits

A type can implement multiple traits:

```floe
for User: Display {
  fn display(self) -> string { self.name }
}

for User: Eq {
  fn eq(self, other: string) -> boolean { self.name == other }
}
```

### Codegen

Traits are **erased at compile time**. `for User: Display` compiles to exactly the same TypeScript as `for User` -- the trait just tells the checker that a contract is satisfied.

```floe
// Floe
for User: Display {
  fn display(self) -> string { self.name }
}

// Compiled TypeScript (identical to plain for-block)
function display(self: User): string { return self.name; }
```

### Deriving Traits

Record types can auto-derive trait implementations with `deriving`. This generates the same code as a handwritten `for` block with no runtime cost:

```floe
type User {
  id: string,
  name: string,
  email: string,
} deriving (Display)
```

This generates `display(self) -> string` with a string representation like `User(id: abc, name: Ryan, email: r@t.com)`.

:::note
`Eq` is not derivable -- structural equality is built-in for all types via `==`.
:::

#### Derivable traits

| Trait | Generated implementation |
|---|---|
| `Display` | `TypeName(field1: val1, field2: val2)` format |

#### Deriving rules

1. `deriving` only works on record types (not unions)
2. A handwritten `for` block overrides a derived implementation
3. Only `Display` is derivable -- `Eq` is built-in via `==`

### Trait Rules

1. All required methods (those without default bodies) must be implemented
2. Default methods are inherited unless overridden
3. Traits are compile-time only -- no runtime representation
4. No orphan rules -- scoping via imports handles conflicts
5. No trait objects or dynamic dispatch -- traits are a static checking tool

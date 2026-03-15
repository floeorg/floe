---
title: For Blocks
---

`for` blocks let you group functions under a type. Think of them as methods without classes. `self` is an explicit parameter, not magic.

## Basic Usage

```floe
type User = { name: string, age: number }

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

## Inline Syntax

When you only need a single function, you can skip the block:

```floe
export for User fn display(self) -> string {
  `${self.name} (${self.age})`
}

export for User fn greet(self, greeting: string) -> string {
  `${greeting}, ${self.name}!`
}
```

This is exactly equivalent to a `for` block with one function. Export is per-function and goes before `for`:

```floe
// Inline: export goes before `for`
export for User fn display(self) -> string { ... }

// Block: export goes before `fn` inside the block
for User {
  export fn display(self) -> string { ... }
}
```

Both forms can be mixed freely in the same file.

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
  |> String.toUpper
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
// Inline for-declaration
export for string fn validate(self) -> Validation {
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

// Block form for grouping related functions
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

For-block functions can be exported like regular functions:

```floe
// Block form: export inside the block
for User {
  export fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}

// Inline form: export before `for`
export for User fn display(self) -> string {
  `${self.name} (${self.age})`
}
```

## Rules

1. `self` is always the explicit first parameter. Its type is inferred.
2. No `this`, no implicit context
3. Multiple `for` blocks per type are allowed, even across files
4. Compiles to standalone TypeScript functions (no classes)
5. Both block and inline syntax are supported and equivalent

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

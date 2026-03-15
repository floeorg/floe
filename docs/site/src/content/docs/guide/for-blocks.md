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

Use them naturally in components:

```floe
const visible = todos |> filterBy(filter)
const remaining = todos |> remaining
```

## Export

For-block functions can be exported just like regular functions:

```floe
for User {
  export fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

Importing a type automatically imports for-block functions defined in the same file. Cross-file for blocks require their own import.

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

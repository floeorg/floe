---
title: Traits
---

Traits define behavioral contracts that types can implement. They work with `for` blocks to ensure types provide specific functionality.

## Defining a Trait

A trait declares method signatures that implementing types must provide:

```floe
trait Display {
  fn display(self) -> string
}
```

## Implementing a Trait

Use `for Type: Trait` to implement a trait for a type:

```floe
type User = { name: string, age: number }

for User: Display {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

The compiler checks that all required methods are implemented. If you forget one, you get a clear error:

```
Error: trait `Display` requires method `display` but it is not implemented for `User`
```

## Default Implementations

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

## Multiple Traits

A type can implement multiple traits:

```floe
for User: Display {
  fn display(self) -> string { self.name }
}

for User: Eq {
  fn eq(self, other: string) -> boolean { self.name == other }
}
```

## Codegen

Traits are **erased at compile time**. `for User: Display` compiles to exactly the same TypeScript as `for User` -- the trait just tells the checker that a contract is satisfied.

```floe
// Floe
for User: Display {
  fn display(self) -> string { self.name }
}

// Compiled TypeScript (identical to plain for-block)
function display(self: User): string { return self.name; }
```

## Rules

1. All required methods (those without default bodies) must be implemented
2. Default methods are inherited unless overridden
3. Traits are compile-time only -- zero runtime cost
4. No orphan rules -- scoping via imports handles conflicts
5. No trait objects or dynamic dispatch -- traits are a static checking tool

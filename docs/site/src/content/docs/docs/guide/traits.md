---
title: Traits
---

Traits define behavioral contracts that types can implement. They work with [`for` blocks](/docs/guide/for-blocks/) to ensure types provide specific functionality.

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
type User { name: string, age: number }

for User: Display {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

The compiler checks that all required methods are implemented. Missing methods produce a clear error.

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

## Deriving Traits

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

### Derivable traits

| Trait | Generated implementation |
|---|---|
| `Display` | `TypeName(field1: val1, field2: val2)` format |

### Deriving rules

1. `deriving` only works on record types (not unions)
2. A handwritten `for` block overrides a derived implementation
3. Only `Display` is derivable -- `Eq` is built-in via `==`

## What It Compiles To

Traits are **erased at compile time**. `for User: Display` compiles to exactly the same TypeScript as `for User` -- the trait just tells the checker that a contract is satisfied.

```floe
// Floe
for User: Display {
  fn display(self) -> string { self.name }
}

// Compiled TypeScript (identical to plain for-block)
function display(self: User): string { return self.name; }
```

No class wrappers, no vtables, no runtime representation. Traits are purely a static checking tool.

## Rules

1. All required methods (those without default bodies) must be implemented
2. Default methods are inherited unless overridden
3. Traits are compile-time only -- no runtime representation
4. No orphan rules -- scoping via imports handles conflicts
5. No trait objects or dynamic dispatch -- traits are a static checking tool

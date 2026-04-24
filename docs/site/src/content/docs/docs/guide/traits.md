---
title: Traits
---

Traits define behavioral contracts that types can implement. They work with [`for` blocks](/docs/guide/for-blocks/) to ensure types provide specific functionality.

## Defining a Trait

A trait declares method signatures that implementing types must provide:

```floe
trait Display {
  let display(self) -> string
}
```

## Implementing a Trait

Use `impl Trait for Type` to implement a trait for a type:

```floe
type User = { name: string, age: number }

impl Display for User {
  let display(self) -> string = {
    `${self.name} (${self.age})`
  }
}
```

The compiler checks that all required methods are implemented. Missing methods produce a clear error.

## Default Implementations

Traits can provide default method bodies. Implementors inherit them unless they override:

```floe
trait Eq {
  let eq(self, other: string) -> boolean
  let neq(self, other: string) -> boolean = {
    !(self |> eq(other))
  }
}

impl Eq for User {
  let eq(self, other: string) -> boolean = {
    self.name == other
  }
  // neq is inherited from the default implementation
}
```

## Exporting and Importing

Traits and their implementations sit on either side of the `export`/`import` rule for *behaviour*:

```floe
// Define and export a trait
export trait Display {
  let display(self) -> string
}

// Export every method in a trait impl at once
export impl Display for User {
  let display(self) -> string = { self.name }
}
```

Import traits with the `for` prefix -- the same syntax used to pull in cross-file for-block methods:

```floe
import { User, for Display } from "./types"

impl Display for User {
  let display(self) -> string = { self.name }
}
```

Writing `import { Display }` for a trait is an error -- the compiler asks you to add the `for` prefix.

## Multiple Traits

A type can implement multiple traits:

```floe
impl Display for User {
  let display(self) -> string = { self.name }
}

impl Eq for User {
  let eq(self, other: string) -> boolean = { self.name == other }
}
```

## Deriving Traits

Floe has no built-in `deriving` syntax. Derives will arrive via the macro system as `@derive(Trait)` attributes on type declarations, expanding at compile time into generated `impl Trait for Type { ... }` blocks. Until macros land, write the impl by hand — three lines for `Display` is a small price for a simpler surface.

:::note
`Eq` is special — it's never derivable and never needs a hand-written impl, because structural equality is built-in for all types via `==`.
:::

## What It Compiles To

Traits are **erased at compile time**. `impl Display for User` compiles to exactly the same TypeScript as `for User` -- the trait just tells the checker that a contract is satisfied.

```floe
// Floe
impl Display for User {
  let display(self) -> string = { self.name }
}
```

```ts
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

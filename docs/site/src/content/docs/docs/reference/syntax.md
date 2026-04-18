---
title: Syntax Reference
---

## Comments

```floe
// Line comment
/* Block comment */
/* Nested /* block */ comments */
```

## Declarations

### Const

```floe
const x = 42
const name: string = "hello"
export const PI = 3.14159

// Destructuring
const (a, b) = pair             // tuple
const { name, age } = user      // record
// (array destructuring not allowed in `const`; use `Array.get` or a match pattern)
```

### Function

```floe
fn name(param: Type) -> ReturnType {
  body
}

// Generic function — type parameters after the name
fn name<T>(param: T) -> T {
  body
}

fn name<A, B>(a: A, b: B) -> (A, B) {
  body
}

export fn name(param: Type) -> ReturnType {
  body
}

fn name() -> Promise<T> {
  expr |> Promise.await
}

// async fn sugar — `async fn f() -> T` means `fn f() -> Promise<T>`
async fn name() -> T {
  expr |> await
}
```

### Type

```floe
// Record
type User {
  name: string,
  email: string,
}

// Union — positional ( ) or named { } fields
type Shape {
  | Circle(number)
  | Rectangle(number, number)
  | Named { width: number, height: number }
}

// Newtype (single-value wrapper)
type OrderId(number)

// String literal union (for npm interop)
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

// Alias
type Name = string

// Newtype
type UserId(string)

// Opaque
opaque type Email(string)

// Deriving traits
type Point {
  x: number,
  y: number,
} deriving (Display)
```

### Use (Callback Flattening)

```floe
// Single binding — rest of block becomes callback body
use x <- doSomething(arg)
doStuff(x)

// Zero binding
use <- delay(1000)
Console.log("done")

// Chaining
use a <- first()
use b <- second(a)
result(b)
```

### For Block

```floe
for Type {
  fn method(self) -> ReturnType {
    body
  }
}

for Array<User> {
  fn adults(self) -> Array<User> {
    self |> Array.filter(.age >= 18)
  }
}
```

### Trait

```floe
trait Display {
  fn display(self) -> string
}

// Trait with default implementation
trait Eq {
  fn eq(self, other: Self) -> boolean
  fn neq(self, other: Self) -> boolean {
    !(self |> eq(other))
  }
}

// Implement a trait
for User: Display {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}
```

### Test Block

```floe
test "addition works" {
  assert add(1, 2) == 3
  assert add(-1, 1) == 0
}
```

## Expressions

### Literals

```floe
42              // number
3.14            // number
1_000_000       // number with separators (underscores for readability)
3.141_592       // float with separators
0xFF_FF         // hex with separators
"hello"         // string
`hello ${name}` // template literal
tag`a ${x} b`   // tagged template literal — `tag` receives the strings and interpolated values
true            // boolean
false           // boolean
[1, 2, 3]      // array
```

Underscores in number literals are purely visual — they are stripped during compilation. They can appear between any two digits but not at the start, end, or adjacent to a decimal point.

Tagged template literals compile to byte-identical TypeScript, so they interoperate cleanly with npm libraries that expose a `` tag`...` `` API (Drizzle's `sql`, styled-components, emotion, graphql-tag). The tag must be a callable value; the runtime values at `${}` are passed as the variadic `...values` arguments.

### Operators

```floe
a + b    a - b    a * b    a / b    a % b   // arithmetic
a == b   a != b   a < b    a > b             // comparison
a <= b   a >= b                               // comparison
a && b   a || b   !a                          // logical
a |> f                                        // pipe
expr?                                         // unwrap
```

### Pipe

```floe
value |> transform
value |> f(other_arg, _)   // placeholder
a |> b |> c                // chaining
value |> match { ... }     // pipe into match
```

### Match

```floe
match expr {
  pattern -> body,
  pattern when guard -> body,
  _ -> default,
}

// Pipe into match
expr |> match {
  pattern -> body,
  _ -> default,
}
```

Patterns: literals (`42`, `"hello"`, `true`), ranges (`1..10`), variants (`Ok(x)`), records (`{ x, y }`), string patterns (`"/users/{id}"`), bindings (`x`), wildcard (`_`), array patterns (`[first, ..rest]`).

### Function Call

```floe
f(a, b)
f(name: value)                 // named argument
f(a, b: 2, c: 3)               // positional first, then named
Constructor(a: 1)              // record constructor
Constructor(..existing, a: 2)  // spread + update
```

Call rules:

- Positional arguments must precede named ones.
- Named arguments may appear in any order — the compiler reorders them to match the declaration.
- Every required parameter must be provided, either positionally or by name.
- A slot cannot be covered twice (positional + name for the same slot, or two named args with the same label).
- Defaulted parameters must be passed by name (not positionally) so a skipped default cannot silently shift a later value into the wrong slot.

### Collect Block

```floe
collect {
    const name = validateName(input.name)?
    const email = validateEmail(input.email)?
    ValidForm(name, email)
}
// Returns Result<T, Array<E>> — accumulates all errors from ?
```

### Constructors

```floe
Ok(value)     // Result success
Err(error)    // Result failure
Some(value)   // Option present
None          // Option absent
```

### Builtins

```floe
todo                              // placeholder, type never, emits warning
unreachable                       // assert unreachable, type never
parse<T>(value)                   // runtime type validation, returns Result<T, Error>
json |> parse<User>?              // pipe form (most common)
data |> parse<Array<Product>>?    // validates arrays
mock<T>                           // generate test data from type, returns T
mock<User>(name: "Alice")         // with field overrides
```

### Qualified Variants

```floe
Filter.All              // zero-arg variant
Filter.Active           // zero-arg variant
Color.Blue(hex: "#00f") // variant with data

// Required when variant name is ambiguous (exists in multiple unions)
// Ok, Err, Some, None are always bare (built-in)
```

### Anonymous Functions (Closures)

```floe
(a: number, b: number) => a + b
(x: number) => x * 2
() => doSomething()
```

Dot shorthand for field access:

```floe
.name           // (x) => x.name
.id != id       // (x) => x.id != id
.done == false  // (x) => x.done == false
```

### Function Types

```floe
() -> ()                    // takes nothing, returns nothing
(string) -> number          // takes string, returns number
(number, number) -> boolean    // takes two numbers, returns boolean
```

### JSX

```floe
<Component prop={value}>children</Component>
<div className="box">text</div>
<Input />
<>fragment</>
```

## Imports

```floe
import { name } from "module"
import { name as alias } from "module"
import { a, b, c } from "module"

// npm imports are untrusted by default — auto-wrapped in Result<T, Error>
import { parseYaml } from "yaml-lib"
const result = parseYaml(input)   // Result<T, Error> — auto-wrapped

// trusted imports — safe to call directly, no wrapping
import trusted { useState } from "react"
const (count, setCount) = useState(0)

// Per-function trusted
import { trusted capitalize, fetchData } from "some-lib"

// Import for-block functions by type
import { for User } from "./helpers"
import { for Array, for Map } from "./collections"

// Mix regular and for-imports
import { Todo, Filter, for Array } from "./todo"
```

## Patterns

```floe
42                    // number literal
"hello"               // string literal
true                  // boolean literal
x                     // binding
_                     // wildcard
Ok(x)                 // variant
Some(inner)           // option
{ field, other }      // record destructure
1..10                 // range (inclusive)
[]                    // empty array
[only]                // single-element array
[first, ..rest]       // array with rest
"/users/{id}"         // string pattern with captures
_ when x > 10        // guard
```

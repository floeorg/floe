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
const [a, b] = pair
const { name, age } = user
```

### Function

```floe
fn name(param: Type) -> ReturnType {
  body
}

export fn name(param: Type) -> ReturnType {
  body
}

async fn name() -> Promise<T> {
  await expr
}
```

### Type

```floe
// Record
type User = {
  name: string,
  email: string,
}

// Union
type Shape =
  | Circle(radius: number)
  | Rectangle(width: number, height: number)

// Alias
type Name = string

// Brand
type UserId = Brand<string, "UserId">

// Opaque
opaque type Email = string
```

### For Block

```floe
// Block form: group multiple functions
for Type {
  fn method(self) -> ReturnType {
    body
  }
}

// Inline form: single function, no block
export for Type fn method(self) -> ReturnType {
  body
}

for Array<User> {
  fn adults(self) -> Array<User> {
    self |> Array.filter(.age >= 18)
  }
}
```

## Expressions

### Literals

```floe
42              // number
3.14            // number
"hello"         // string
`hello ${name}` // template literal
true            // boolean
false           // boolean
[1, 2, 3]      // array
```

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
```

### Match

```floe
match expr {
  pattern -> body,
  pattern when guard -> body,
  _ -> default,
}
```

Patterns: literals (`42`, `"hello"`, `true`), ranges (`1..10`), variants (`Ok(x)`), records (`{ x, y }`), string patterns (`"/users/{id}"`), bindings (`x`), wildcard (`_`).
### Function Call

```floe
f(a, b)
f(name: value)     // named argument
Constructor(a: 1)  // record constructor
Constructor(..existing, a: 2)  // spread + update
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
todo          // placeholder, type never, emits warning
unreachable   // assert unreachable, type never
```

### Qualified Variants

```floe
Filter.All              // zero-arg variant
Filter.Active           // zero-arg variant
Option.Some(value)      // variant with data
Result.Ok(value)        // variant with data
```

### Anonymous Functions (Lambdas)

```floe
|a, b| a + b
|x| x * 2
|| doSomething()
```

Dot shorthand for field access:

```floe
.name           // |x| x.name
.id != id       // |x| x.id != id
.done == false  // |x| x.done == false
```

### Function Types

```floe
() -> ()                  // takes nothing, returns nothing
(string) -> number        // takes string, returns number
(number, number) -> boolean  // takes two numbers, returns boolean
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

// Import for-block functions by type
import { for User } from "./helpers"
import { for Array, for Map } from "./collections"

// Mix regular and for-imports
import { Todo, Filter, for Array } from "./todo"
```

## Patterns

```floe
42                    // literal
"hello"               // string literal
true                  // boolean literal
x                     // binding
_                     // wildcard
Ok(x)                 // variant
Some(inner)           // option
{ field, other }      // record destructure
1..10                 // range
```

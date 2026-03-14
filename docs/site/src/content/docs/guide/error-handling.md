---
title: Error Handling
---

ZenScript replaces exceptions with `Result<T, E>` and replaces null checks with `Option<T>`. Every error path is visible in the type system.

## Result

```zenscript
function divide(a: number, b: number): Result<number, string> {
  match b {
    0 -> Err("division by zero"),
    _ -> Ok(a / b),
  }
}
```

You **must** handle the result:

```zenscript
match divide(10, 3) {
  Ok(value) -> console.log(value),
  Err(msg) -> console.error(msg),
}
```

Ignoring a `Result` is a compile error:

```zenscript
// Error: Result must be handled
divide(10, 3)
```

## The `?` Operator

Propagate errors early instead of nesting matches:

```zenscript
function processOrder(id: string): Result<Receipt, Error> {
  const order = fetchOrder(id)?       // returns Err early if it fails
  const payment = chargeCard(order)?  // same here
  return Ok(Receipt(order, payment))
}
```

The `?` operator:
- On `Ok(value)`: unwraps to `value`
- On `Err(e)`: returns `Err(e)` from the enclosing function

Using `?` outside a function that returns `Result` is a compile error.

## Option

```zenscript
function findUser(id: string): Option<User> {
  match users |> find(u => u.id == id) {
    Some(user) -> Some(user),
    None -> None,
  }
}
```

Handle with match:

```zenscript
match findUser("123") {
  Some(user) -> greet(user.name),
  None -> greet("stranger"),
}
```

## npm Interop

When importing from npm packages, ZenScript automatically wraps nullable types:

```zenscript
import { getElementById } from "some-dom-lib"
// .d.ts says: getElementById(id: string): Element | null
// ZenScript sees: getElementById(id: string): Option<Element>
```

The boundary wrapping also converts:
- `T | undefined` to `Option<T>`
- `any` to `unknown`

This means npm libraries work transparently with ZenScript's type system.

## Comparison with TypeScript

| TypeScript | ZenScript |
|---|---|
| `T \| null` | `Option<T>` |
| `try/catch` | `Result<T, E>` |
| `?.` optional chain | `match` on `Option` |
| `!` non-null assertion | Not available (handle the case) |
| `throw new Error()` | `Err(...)` |

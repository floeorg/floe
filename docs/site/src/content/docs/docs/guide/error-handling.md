---
title: Error Handling
---

Floe replaces exceptions with `Result<T, E>` and replaces null checks with `Option<T>`. Every error path is visible in the type system.

## Result

```floe
let divide(a: number, b: number) -> Result<number, string> = {
  match b {
    0 -> Err("division by zero"),
    _ -> Ok(a / b),
  }
}
```

You **must** handle the result:

```floe
match divide(10, 3) {
  Ok(value) -> Console.log(value),
  Err(msg) -> Console.error(msg),
}
```

Ignoring a `Result` is a compile error:

```floe
// Error: Result must be handled
divide(10, 3)
```

## The `?` Operator

Propagate errors early instead of nesting matches:

```floe
let processOrder(id: string) -> Result<Receipt, Error> = {
  let order = fetchOrder(id)?       // returns Err early if it fails
  let payment = chargeCard(order)?  // same here
  Ok(Receipt(order, payment))
}
```

The `?` operator:
- On `Ok(value)`: unwraps to `value`
- On `Err(e)`: returns `Err(e)` from the enclosing function

Using `?` outside a function that returns `Result` is a compile error.

## The `collect` Block

Normally, `?` short-circuits on the first error. But sometimes you want **all** errors at once -- form validation, batch processing, config parsing. The `collect` block changes `?` from short-circuiting to accumulating:

```floe
let validateForm(input: FormInput) -> Result<ValidForm, Array<ValidationError>> = {
    collect {
        let name = input.name |> validateName?
        let email = input.email |> validateEmail?
        let age = input.age |> validateAge?

        ValidForm(name, email, age)
    }
}
```

If the user submits `name: ""`, `email: "bad"`, `age: -1`, all three validators run and the caller gets `Err([NameEmpty, InvalidEmail, AgeTooLow])` -- not just the first failure.

### How it works

Inside `collect {}`:
- Each `?` that hits `Err` **records** the error and continues (instead of returning early)
- Variables from failed `?` get a zero value so subsequent lines can still run
- If any failed, the block returns `Err(Array<E>)` with all collected errors
- If all succeeded, returns `Ok(last_expression)`

The return type is always `Result<T, Array<E>>`.

### `collect` vs regular `?`

| | Regular `?` | `collect` |
|---|---|---|
| On first error | Returns immediately | Records error, continues |
| Return type | `Result<T, E>` | `Result<T, Array<E>>` |
| Best for | Sequential operations where later steps depend on earlier ones | Independent validations where you want all errors |

Use regular `?` when operations are dependent (step 2 needs step 1's result). Use `collect` when validations are independent and the user benefits from seeing everything wrong at once.

### Real-world example: API config

```floe
type ApiConfig = {
    baseUrl: string,
    apiKey: string,
    timeout: number,
}

let loadConfig(env: Env) -> Result<ApiConfig, Array<ConfigError>> = {
    collect {
        let baseUrl = env |> requireEnv("API_BASE_URL")?
        let apiKey = env |> requireEnv("API_KEY")?
        let timeout = env |> requireEnv("TIMEOUT")? |> Number.parse?

        ApiConfig(baseUrl, apiKey, timeout)
    }
}
// Err([Missing("API_KEY"), ParseError("TIMEOUT: not a number")])
```

## Mapping Error Types

When composing functions with different error types, use `Result.mapErr` to convert errors into a domain type. Variant constructors can be passed directly as functions:

```floe
type AppError = | Validation { errors: Array<string> }
    | Api { message: string }

let saveTodo(text: string, id: string) -> Result<Todo, AppError> = {
    let todoItem = validateTodo(text, id) |> Result.mapErr(Validation)?
    let saved = apiSave(todoItem) |> Result.mapErr(Api)?
    Ok(saved)
}
```

`Validation` here is used as a function — equivalent to `(e) -> Validation(errors: e)`. This works for any non-unit variant.

## Option

```floe
let findUser(id: string) -> Option<User> = {
  match users |> find(.id == id) {
    Some(user) -> Some(user),
    None -> None,
  }
}
```

Handle with match:

```floe
match findUser("123") {
  Some(user) -> greet(user.name),
  None -> greet("stranger"),
}
```

## npm Interop

When importing from npm packages, Floe automatically wraps nullable types:

```floe
import { getElementById } from "some-dom-lib"
// .d.ts says: getElementById(id: string): Element | null
// Floe sees: getElementById(id: string) -> Option<Element>
```

The boundary wrapping also converts:
- `T | undefined` to `Option<T>`
- `any` to `unknown`

npm imports are untrusted by default -- calls are auto-wrapped in `Result<T, Error>`. Use `trusted` to mark safe imports that can be called directly:

```floe
import { parseYaml } from "yaml-lib"                // untrusted (default)
let data = parseYaml(input)?                       // Result<T, Error>, ? unwraps

import trusted { useState } from "react"             // trusted = direct call
let (count, setCount) = useState(0)
```

This means npm libraries work transparently with Floe's type system.

## `todo` and `unreachable`

Floe provides two built-in expressions for common development patterns:

### `todo` - Not Yet Implemented

Use `todo` as a placeholder in unfinished code. It type-checks as `never`, so it satisfies any return type. The compiler emits a warning to remind you to replace it.

```floe
let processPayment(order: Order) -> Result<Receipt, Error> = {
  todo  // warning: placeholder that will panic at runtime
}
```

At runtime, `todo` throws `Error("not implemented")`.

### `unreachable` - Should Never Happen

Use `unreachable` to assert that a code path should never execute. Like `todo`, it has type `never`, but unlike `todo`, it does not emit a warning.

```floe
let direction(key: string) -> string = {
  match key {
    "w" -> "up",
    "s" -> "down",
    "a" -> "left",
    "d" -> "right",
    _ -> unreachable,
  }
}
```

At runtime, `unreachable` throws `Error("unreachable")`.

### When to Use Which

- **`todo`** = "I haven't written this yet" (development aid)
- **`unreachable`** = "This should never happen" (safety assertion)

For runtime type validation with `parse<T>`, see [Type-Driven Features](/docs/guide/type-driven-features/).

## Comparison with TypeScript

| TypeScript | Floe |
|---|---|
| `T \| null` | `Option<T>` |
| `try/catch` | `Result<T, E>` |
| `?.` optional chain | `match` on `Option` |
| `!` non-null assertion | Not available (handle the case) |
| `throw new Error()` | `Err(...)` |

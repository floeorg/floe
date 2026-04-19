---
title: Promise
sidebar:
  order: 14
---

Functions for working with `Promise<T>` values.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Promise.await` | `Promise<T> -> T` | Unwrap a Promise (compiles to `await`) |
| `Promise.all` | `Array<Promise<T>> -> Promise<Array<T>>` | Wait for all, fail on first rejection |
| `Promise.race` | `Array<Promise<T>> -> Promise<T>` | First to settle (resolve or reject) |
| `Promise.any` | `Array<Promise<T>> -> Promise<T>` | First to resolve, fail if all reject |
| `Promise.allSettled` | `Array<Promise<T>> -> Promise<Array<Result<T, Error>>>` | Wait for all, return Results |
| `Promise.resolve` | `T -> Promise<T>` | Wrap a value in a resolved Promise |
| `Promise.reject` | `E -> Promise<T>` | Create a rejected Promise |
| `Promise.delay` | `number -> Promise<()>` | Wait for milliseconds |

## `Promise.await`

`Promise.await` is a stdlib function with signature `Promise<T> -> T`. It compiles to JavaScript's `await` keyword. Using `Promise.await` anywhere in a function body causes the compiler to infer `async` on the emitted function -- no `async` keyword is needed in Floe. `await` is also available as a bare shorthand in pipes: `expr |> await`.

```floe
fn fetchUser(id: string) => Promise<User> {
  let response = fetch(`/api/users/${id}`) |> await
  response.json() |> await
}
// Compiles to: async function fetchUser(id: string): Promise<User> { ... }
```

The return type must explicitly use `Promise<T>`, making async behavior visible to callers. The compiler enforces this -- using `await` in a function with a non-`Promise` return type is a compile error:

```floe
// Error: function `bad` uses `await` but return type is `string`, not `Promise<string>`
fn bad() => string {
  getData() |> await
}

// OK
fn good() => Promise<string> {
  getData() |> await
}
```

This parallels how `?` requires the function to return `Result<T, E>`. Both operators change the function contract, and both require explicit return types.

For functions without a return type annotation, the compiler infers `Promise<T>` automatically:

```floe
fn fetchName(id: string) {
  let user = fetchUser(id) |> await
  user.name
}
// Inferred return type: Promise<string>
```

## `async fn` sugar

`async fn f() => T` is sugar for `fn f() => Promise<T>` — write the inner type and the compiler wraps it automatically. This keeps signatures readable when the return type has several layers (`Result<Option<T>, Error>`, etc.):

```floe
// Verbose — three nested generics
fn findByCode(code: string) => Promise<Result<Option<Snippet>, Error>> {
  // ...
}

// Sugar — the `Promise<>` wrapper is implied by `async`
async fn findByCode(code: string) => Result<Option<Snippet>, Error> {
  // ...
}
```

Behavior:

- The return type annotation is the **inner** type (what the body actually produces). Callers see `Promise<T>`.
- The function body returns `T` directly (no manual wrapping).
- Callers still use `|> await` to unwrap the `Promise<T>`.
- `async fn f() => Promise<T>` is an error — `async` already implies the wrapper.
- Plain `fn f() => Promise<T>` still works for cases where you want to be explicit, or for non-async functions that return promises (e.g. storing them in `Array<Promise<T>>`).

Both forms compile to the same `async function` in TypeScript.

## Untrusted imports

npm imports are untrusted by default. The compiler wraps calls in try/catch and returns `Result<T, Error>`:

- **Sync functions**: wrapped in sync try/catch → `Result<T, Error>`
- **Async functions** (returns `Promise<T>`): wrapped in async try/catch → `Promise<Result<T, Error>>` — use `|> await` to unwrap the Promise

```floe
// Sync npm function — Result<T, Error> directly
import { parseYaml } from "yaml-lib"
let result = parseYaml(text)
// Result<Config, Error> — no await needed

// Async npm function — Promise<Result<T, Error>>, needs |> await
import { transitionIssue } from "jira-api"
let result = transitionIssue(id, tid) |> await
// Result<(), Error>

match result {
    Ok(_) -> Console.log("Moved!"),
    Err(e) -> Console.error("Failed:", e),
}
```

| Tool | For | Does |
|---|---|---|
| `|> await` | Floe async functions | Unwrap `Promise<T>` |
| Untrusted imports (default) | npm sync functions | Wrap in `Result<T, Error>` |
| Untrusted imports (default) | npm async functions | Wrap in `Promise<Result<T, Error>>` — use `|> await` |
| `trusted` imports | npm functions known to be safe | Direct calls, no wrapping |

## Examples

```floe
// Wait for all fetches
let users = Promise.all([fetchUser(1), fetchUser(2), fetchUser(3)]) |> Promise.await

// Race — first response wins
let fastest = Promise.race([fetchFromCDN(url), fetchFromOrigin(url)]) |> Promise.await

// allSettled returns Array<Result<T, Error>> — natural fit for Floe
let results = Promise.allSettled([fetchA(), fetchB(), fetchC()]) |> Promise.await
let successes = results |> Array.filter(Result.isOk)

// Delay
Promise.delay(1000) |> Promise.await  // wait 1 second
```

`Promise.allSettled` returns `Array<Result<T, Error>>` instead of JavaScript's `{status, value, reason}` shape, so you can use all Result helpers on the output.

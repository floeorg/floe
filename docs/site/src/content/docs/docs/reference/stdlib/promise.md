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

`Promise.await` is a stdlib function with signature `Promise<T> -> T`. It compiles to JavaScript's `await` keyword. Using `Promise.await` anywhere in a function body causes the compiler to infer `async` on the emitted function -- no `async` keyword is needed in Floe.

```floe
fn fetchUser(id: string) -> Promise<User> {
  const response = fetch(`/api/users/${id}`) |> Promise.await
  response.json() |> Promise.await
}
// Compiles to: async function fetchUser(id: string): Promise<User> { ... }
```

The return type must explicitly use `Promise<T>`, making async behavior visible to callers.

## Examples

```floe
// Wait for all fetches
const users = Promise.all([fetchUser(1), fetchUser(2), fetchUser(3)]) |> Promise.await

// Race — first response wins
const fastest = Promise.race([fetchFromCDN(url), fetchFromOrigin(url)]) |> Promise.await

// allSettled returns Array<Result<T, Error>> — natural fit for Floe
const results = Promise.allSettled([fetchA(), fetchB(), fetchC()]) |> Promise.await
const successes = results |> Array.filter(Result.isOk)

// Delay
Promise.delay(1000) |> Promise.await  // wait 1 second
```

`Promise.allSettled` returns `Array<Result<T, Error>>` instead of JavaScript's `{status, value, reason}` shape, so you can use all Result helpers on the output.

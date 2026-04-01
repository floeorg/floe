---
title: Promise
sidebar:
  order: 14
---

Functions for working with `Promise<T>` values.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Promise.all` | `Array<Promise<T>> -> Promise<Array<T>>` | Wait for all, fail on first rejection |
| `Promise.race` | `Array<Promise<T>> -> Promise<T>` | First to settle (resolve or reject) |
| `Promise.any` | `Array<Promise<T>> -> Promise<T>` | First to resolve, fail if all reject |
| `Promise.allSettled` | `Array<Promise<T>> -> Promise<Array<Result<T, Error>>>` | Wait for all, return Results |
| `Promise.resolve` | `T -> Promise<T>` | Wrap a value in a resolved Promise |
| `Promise.reject` | `E -> Promise<T>` | Create a rejected Promise |
| `Promise.delay` | `number -> Promise<()>` | Wait for milliseconds |

## Examples

```floe
// Wait for all fetches
const users = await Promise.all([fetchUser(1), fetchUser(2), fetchUser(3)])

// Race — first response wins
const fastest = await Promise.race([fetchFromCDN(url), fetchFromOrigin(url)])

// allSettled returns Array<Result<T, Error>> — natural fit for Floe
const results = await Promise.allSettled([fetchA(), fetchB(), fetchC()])
const successes = results |> Array.filter(Result.isOk)

// Delay
await Promise.delay(1000)  // wait 1 second
```

`Promise.allSettled` returns `Array<Result<T, Error>>` instead of JavaScript's `{status, value, reason}` shape, so you can use all Result helpers on the output.

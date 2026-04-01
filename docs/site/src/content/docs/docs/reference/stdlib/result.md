---
title: Result
sidebar:
  order: 3
---

Functions for working with `Result<T, E>` (`Ok(v)` / `Err(e)`) values.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Result.map` | `Result<T, E>, (T) -> U -> Result<U, E>` | Transform the Ok value |
| `Result.mapErr` | `Result<T, E>, (E) -> F -> Result<T, F>` | Transform the Err value |
| `Result.flatMap` | `Result<T, E>, (T) -> Result<U, E> -> Result<U, E>` | Chain result-returning operations |
| `Result.unwrapOr` | `Result<T, E>, T -> T` | Extract Ok value or use default |
| `Result.isOk` | `Result<T, E> -> boolean` | Check if result is Ok |
| `Result.isErr` | `Result<T, E> -> boolean` | Check if result is Err |
| `Result.toOption` | `Result<T, E> -> Option<T>` | Convert to Option (drops error) |
| `Result.filter` | `Result<T, E>, (T) -> boolean, E -> Result<T, E>` | Keep Ok if predicate passes, else Err |
| `Result.or` | `Result<T, E>, Result<T, E> -> Result<T, E>` | Return first Ok, else second |
| `Result.orElse` | `Result<T, E>, (E) -> Result<T, F> -> Result<T, F>` | Lazy fallback chain |
| `Result.values` | `Array<Result<T, E>> -> Array<T>` | Extract all Ok values, discard Errs |
| `Result.partition` | `Array<Result<T, E>> -> (Array<T>, Array<E>)` | Split into Ok and Err arrays |
| `Result.mapOr` | `Result<T, E>, U, (T) -> U -> U` | Map + default in one step |
| `Result.flatten` | `Result<Result<T, E>, E> -> Result<T, E>` | Unwrap nested Results |
| `Result.zip` | `Result<T, E>, Result<U, E> -> Result<(T, U), E>` | Combine two Results into a tuple |
| `Result.inspect` | `Result<T, E>, (T) -> () -> Result<T, E>` | Side-effect on Ok value |
| `Result.inspectErr` | `Result<T, E>, (E) -> () -> Result<T, E>` | Side-effect on Err value |
| `Result.all` | `Array<Result<T, E>> -> Result<Array<T>, E>` | Collect all Ok values, fail on first Err |
| `Result.any` | `Array<Result<T, E>> -> Result<T, Array<E>>` | First Ok, or all Errs |
| `Result.guard` | `Result<T, E>, (E) -> U, (T) -> U -> U` | Bail with onErr on Err, continue with Ok value (for `use`) |

## Examples

```floe
// Transform success value
const doubled = fetchCount()
  |> Result.map((n) => n * 2)

// Handle errors
const result = fetchUser(id)
  |> Result.mapErr((e) => AppError(e))

// Chain operations
const profile = fetchUser(id)
  |> Result.flatMap((u) => fetchProfile(u.profileId))

// Extract with fallback
const count = fetchCount()
  |> Result.unwrapOr(0)

// Lazy fallback chain
const data = fetchFromPrimary(id)
  |> Result.orElse((e) => fetchFromBackup(id))

// Filter — keep Ok only if predicate passes
const validAge = parseAge(input)
  |> Result.filter((n) => n >= 18, "must be 18+")

// Zip — combine two Results
const pair = Result.zip(fetchUser(id), fetchProfile(id))
// Ok(("Alice", Profile(...))) or first Err

// Collect all Results
const users = [fetchUser(1), fetchUser(2), fetchUser(3)]
  |> Result.all   // Ok([...]) or first Err

// Debug with inspect
const result = fetchUser(id)
  |> Result.inspect((u) => Console.log("got user", u))
  |> Result.mapErr((e) => AppError(e))
```

## Guard Pattern

`Result.guard` combines with [`use`](/docs/guide/use/) for early returns on errors:

```floe
use data <- Result.guard(fetchResult, (e) => <ErrorPage error={e} />)
<Dashboard data={data} />
```

See the [Callback Flattening & Guards](/docs/guide/use/) guide for the full pattern.

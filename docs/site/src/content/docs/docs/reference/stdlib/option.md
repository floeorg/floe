---
title: Option
sidebar:
  order: 2
---

Functions for working with `Option<T>` (`Some(v)` / `None`) values.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Option.map` | `Option<T>, (T) => U -> Option<U>` | Transform the inner value if present |
| `Option.flatMap` | `Option<T>, (T) => Option<U> -> Option<U>` | Chain option-returning operations |
| `Option.unwrapOr` | `Option<T>, T -> T` | Extract value or use default |
| `Option.isSome` | `Option<T> -> boolean` | Check if value is present |
| `Option.isNone` | `Option<T> -> boolean` | Check if value is absent |
| `Option.toResult` | `Option<T>, E -> Result<T, E>` | Convert to Result with error for None |
| `Option.filter` | `Option<T>, (T) => boolean -> Option<T>` | Keep Some if predicate passes, else None |
| `Option.or` | `Option<T>, Option<T> -> Option<T>` | Return first Some, else second |
| `Option.orElse` | `Option<T>, () => Option<T> -> Option<T>` | Lazy fallback chain |
| `Option.values` | `Array<Option<T>> -> Array<T>` | Extract all Some values, discard None |
| `Option.mapOr` | `Option<T>, U, (T) => U -> U` | Map + default in one step |
| `Option.flatten` | `Option<Option<T>> -> Option<T>` | Unwrap nested Options |
| `Option.zip` | `Option<T>, Option<U> -> Option<(T, U)>` | Combine two Options into a tuple |
| `Option.inspect` | `Option<T>, (T) => () => Option<T>` | Side-effect without changing the value |
| `Option.toErr` | `Option<E> -> Result<(), E>` | Convert to Err if present (for `{ data, error }` patterns) |
| `Option.all` | `Array<Option<T>> -> Option<Array<T>>` | Collect all Some values, None if any missing |
| `Option.any` | `Array<Option<T>> -> Option<T>` | Return first Some, or None |
| `Option.guard` | `Option<T>, U, (T) => U -> U` | Bail with fallback on None, continue with unwrapped value (for `use`) |

## Examples

```floe
// Transform without unwrapping
const upper = user.nickname
  |> Option.map((n) => String.toUpperCase(n))
// Some("RYAN") or None

// Chain lookups
const avatar = user.nickname
  |> Option.flatMap((n) => findAvatar(n))

// Extract with fallback
const display = user.nickname
  |> Option.unwrapOr(user.name)

// Lazy fallback chain
const config = localConfig
  |> Option.orElse(() => envConfig)
  |> Option.orElse(() => defaultConfig)

// Convert to Result for error handling
const name = user.nickname
  |> Option.toResult("User has no nickname")

// Filter — keep Some only if predicate passes
const longName = user.nickname
  |> Option.filter((n) => String.length(n) > 3)

// Zip — combine two Options
const pair = Option.zip(firstName, lastName)
// Some(("Alice", "Smith")) or None

// Handle { data, error } pattern (TanStack Query, Supabase, etc.)
const { data, error } = supabase.rpc("get_entries", { query }) |> Promise.await
error |> Option.toErr?              // bail if error exists
const rows = data |> Option.unwrapOr([])

// Collect all Options
const allNames = [Some("Alice"), Some("Bob"), None]
  |> Option.all   // None (one is missing)

// Optional JSX rendering — render if present, nothing if absent
<div>
  {user.nickname |> Option.map((nick) => <Badge>{nick}</Badge>)}
</div>
// None renders nothing (undefined is ignored by React)
```

## Guard Pattern

`Option.guard` combines with [`use`](/docs/guide/use/) for early returns when a value is missing:

```floe
// Bail with fallback if None, continue with unwrapped value
use user <- Option.guard(maybeUser, <LoginPage />)
<ProfilePage user={user} />
```

See the [Callback Flattening & Guards](/docs/guide/use/) guide for the full pattern.

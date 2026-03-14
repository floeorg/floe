---
title: Standard Library
---

ZenScript ships with built-in functions for common operations. These are known to the compiler and inlined as vanilla TypeScript during codegen — no runtime dependency.

All stdlib functions are **pipe-friendly**: the first argument is the data, so they work naturally with `|>`.

```zenscript
[3, 1, 2]
  |> Array.sort
  |> Array.map((n) => n * 10)
  |> Array.reverse
// [30, 20, 10]
```

---

## Array

All array functions return new arrays — they never mutate the original.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Array.sort` | `Array<number> -> Array<number>` | Sort numerically (returns new array) |
| `Array.sortBy` | `Array<T>, (T -> number) -> Array<T>` | Sort by key function |
| `Array.map` | `Array<T>, (T -> U) -> Array<U>` | Transform each element |
| `Array.filter` | `Array<T>, (T -> bool) -> Array<T>` | Keep elements matching predicate |
| `Array.find` | `Array<T>, (T -> bool) -> Option<T>` | First element matching predicate |
| `Array.findIndex` | `Array<T>, (T -> bool) -> Option<number>` | Index of first match |
| `Array.flatMap` | `Array<T>, (T -> Array<U>) -> Array<U>` | Map then flatten one level |
| `Array.at` | `Array<T>, number -> Option<T>` | Safe index access |
| `Array.contains` | `Array<T>, T -> bool` | Check if element exists (structural equality) |
| `Array.head` | `Array<T> -> Option<T>` | First element |
| `Array.last` | `Array<T> -> Option<T>` | Last element |
| `Array.take` | `Array<T>, number -> Array<T>` | First n elements |
| `Array.drop` | `Array<T>, number -> Array<T>` | All except first n elements |
| `Array.reverse` | `Array<T> -> Array<T>` | Reverse order (returns new array) |
| `Array.reduce` | `Array<T>, U, (U, T -> U) -> U` | Fold into a single value |
| `Array.length` | `Array<T> -> number` | Number of elements |
| `Array.zip` | `Array<T>, Array<U> -> Array<[T, U]>` | Pair elements from two arrays |

### Examples

```zenscript
// Sort returns a new array — original unchanged
const nums = [3, 1, 2]
const sorted = Array.sort(nums)     // [1, 2, 3]
// nums is still [3, 1, 2]

// Safe access returns Option
const first = Array.head([1, 2, 3])  // Some(1)
const empty = Array.head([])         // None

// Structural equality for contains
const user1 = User(name: "Ryan")
const found = Array.contains(users, user1)  // true if any user matches by value

// Pipe chains
const result = users
  |> Array.filter((u) => u.active)
  |> Array.sortBy((u) => u.name)
  |> Array.take(10)
  |> Array.map((u) => u.email)
```

---

## Option

Functions for working with `Option<T>` (`Some(v)` / `None`) values.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Option.map` | `Option<T>, (T -> U) -> Option<U>` | Transform the inner value if present |
| `Option.flatMap` | `Option<T>, (T -> Option<U>) -> Option<U>` | Chain option-returning operations |
| `Option.unwrapOr` | `Option<T>, T -> T` | Extract value or use default |
| `Option.isSome` | `Option<T> -> bool` | Check if value is present |
| `Option.isNone` | `Option<T> -> bool` | Check if value is absent |
| `Option.toResult` | `Option<T>, E -> Result<T, E>` | Convert to Result with error for None |

### Examples

```zenscript
// Transform without unwrapping
const upper = user.nickname
  |> Option.map((n) => String.toUpper(n))
// Some("RYAN") or None

// Chain lookups
const avatar = user.nickname
  |> Option.flatMap((n) => findAvatar(n))

// Extract with fallback
const display = user.nickname
  |> Option.unwrapOr(user.name)

// Convert to Result for error handling
const name = user.nickname
  |> Option.toResult("User has no nickname")
```

---

## Result

Functions for working with `Result<T, E>` (`Ok(v)` / `Err(e)`) values.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Result.map` | `Result<T, E>, (T -> U) -> Result<U, E>` | Transform the Ok value |
| `Result.mapErr` | `Result<T, E>, (E -> F) -> Result<T, F>` | Transform the Err value |
| `Result.flatMap` | `Result<T, E>, (T -> Result<U, E>) -> Result<U, E>` | Chain result-returning operations |
| `Result.unwrapOr` | `Result<T, E>, T -> T` | Extract Ok value or use default |
| `Result.isOk` | `Result<T, E> -> bool` | Check if result is Ok |
| `Result.isErr` | `Result<T, E> -> bool` | Check if result is Err |
| `Result.toOption` | `Result<T, E> -> Option<T>` | Convert to Option (drops error) |

### Examples

```zenscript
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
```

---

## String

Pipe-friendly string operations.

| Function | Signature | Description |
|----------|-----------|-------------|
| `String.trim` | `string -> string` | Remove whitespace from both ends |
| `String.trimStart` | `string -> string` | Remove leading whitespace |
| `String.trimEnd` | `string -> string` | Remove trailing whitespace |
| `String.split` | `string, string -> Array<string>` | Split by separator |
| `String.replace` | `string, string, string -> string` | Replace first occurrence |
| `String.startsWith` | `string, string -> bool` | Check prefix |
| `String.endsWith` | `string, string -> bool` | Check suffix |
| `String.contains` | `string, string -> bool` | Check if substring exists |
| `String.toUpper` | `string -> string` | Convert to uppercase |
| `String.toLower` | `string -> string` | Convert to lowercase |
| `String.length` | `string -> number` | Character count |
| `String.slice` | `string, number, number -> string` | Extract substring |
| `String.padStart` | `string, number, string -> string` | Pad from the start |
| `String.padEnd` | `string, number, string -> string` | Pad from the end |
| `String.repeat` | `string, number -> string` | Repeat n times |

### Examples

```zenscript
// Pipe-friendly
const cleaned = "  Hello, World!  "
  |> String.trim
  |> String.toLower
  |> String.replace("world", "zenscript")
// "hello, zenscript!"

// Split and process
const words = "one,two,three"
  |> String.split(",")
  |> Array.map((w) => String.toUpper(w))
// ["ONE", "TWO", "THREE"]
```

---

## Number

Safe numeric operations. Parsing returns `Result` instead of `NaN`.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Number.parse` | `string -> Result<number, ParseError>` | Strict parse (no partial, no NaN) |
| `Number.clamp` | `number, number, number -> number` | Clamp between min and max |
| `Number.isFinite` | `number -> bool` | Check if finite |
| `Number.isInteger` | `number -> bool` | Check if integer |
| `Number.toFixed` | `number, number -> string` | Format with fixed decimals |
| `Number.toString` | `number -> string` | Convert to string |

### Examples

```zenscript
// Safe parsing — no more NaN surprises
const result = "42" |> Number.parse
// Ok(42)

const bad = "not a number" |> Number.parse
// Err(ParseError)

// Must handle the Result
match Number.parse(input) {
  Ok(n)  -> processNumber(n)
  Err(_) -> showError("Invalid number")
}

// Clamp to range
const score = rawScore |> Number.clamp(0, 100)
```

---
title: Standard Library
---

Floe ships with built-in functions for common operations. These are known to the compiler and inlined during codegen.

All stdlib functions are **pipe-friendly**: the first argument is the data, so they work naturally with `|>`.

```floe
[3, 1, 2]
  |> Array.sort
  |> Array.map(fn(n) n * 10)
  |> Array.reverse
// [30, 20, 10]
```

---

## Array

All array functions return new arrays. They never mutate the original.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Array.sort` | `Array<number> -> Array<number>` | Sort numerically (returns new array) |
| `Array.sortBy` | `Array<T>, fn(T) -> number -> Array<T>` | Sort by key function |
| `Array.map` | `Array<T>, fn(T) -> U -> Array<U>` | Transform each element |
| `Array.filter` | `Array<T>, fn(T) -> boolean -> Array<T>` | Keep elements matching predicate |
| `Array.find` | `Array<T>, fn(T) -> boolean -> Option<T>` | First element matching predicate |
| `Array.findIndex` | `Array<T>, fn(T) -> boolean -> Option<number>` | Index of first match |
| `Array.flatMap` | `Array<T>, fn(T) -> Array<U> -> Array<U>` | Map then flatten one level |
| `Array.at` | `Array<T>, number -> Option<T>` | Safe index access |
| `Array.contains` | `Array<T>, T -> boolean` | Check if element exists (structural equality) |
| `Array.head` | `Array<T> -> Option<T>` | First element |
| `Array.last` | `Array<T> -> Option<T>` | Last element |
| `Array.take` | `Array<T>, number -> Array<T>` | First n elements |
| `Array.drop` | `Array<T>, number -> Array<T>` | All except first n elements |
| `Array.reverse` | `Array<T> -> Array<T>` | Reverse order (returns new array) |
| `Array.reduce` | `Array<T>, U, fn(U, T) -> U -> U` | Fold into a single value |
| `Array.length` | `Array<T> -> number` | Number of elements |
| `Array.any` | `Array<T>, fn(T) -> boolean -> boolean` | True if any element matches predicate |
| `Array.all` | `Array<T>, fn(T) -> boolean -> boolean` | True if all elements match predicate |
| `Array.sum` | `Array<number> -> number` | Sum all elements |
| `Array.join` | `Array<string>, string -> string` | Join elements with separator |
| `Array.isEmpty` | `Array<T> -> boolean` | True if array has no elements |
| `Array.chunk` | `Array<T>, number -> Array<Array<T>>` | Split into chunks of given size |
| `Array.unique` | `Array<T> -> Array<T>` | Remove duplicate elements |
| `Array.groupBy` | `Array<T>, fn(T) -> string -> Record` | Group elements by key function |
| `Array.zip` | `Array<T>, Array<U> -> Array<[T, U]>` | Pair elements from two arrays |

### Examples

```floe
// Sort returns a new array, original unchanged
const nums = [3, 1, 2]
const sorted = Array.sort(nums)     // [1, 2, 3]
// nums is still [3, 1, 2]

// Safe access returns Option
const first = Array.head([1, 2, 3])  // Some(1)
const empty = Array.head([])         // None

// Structural equality for contains
const user1 = User(name: "Ryan")
const found = Array.contains(users, user1)  // true if any user matches by value

// Pipe chains with dot shorthand
const result = users
  |> Array.filter(.active)
  |> Array.sortBy(.name)
  |> Array.take(10)
  |> Array.map(.email)

// Check predicates
const hasAdmin = users |> Array.any(.role == "admin")   // true/false
const allActive = users |> Array.all(.active)           // true/false

// Aggregate
const total = [1, 2, 3] |> Array.sum             // 6
const csv = ["a", "b", "c"] |> Array.join(", ")  // "a, b, c"

// Utilities
const empty = Array.isEmpty([])          // true
const chunks = [1, 2, 3, 4, 5] |> Array.chunk(2)   // [[1, 2], [3, 4], [5]]
const deduped = [1, 2, 2, 3] |> Array.unique        // [1, 2, 3]
const grouped = users |> Array.groupBy(.role)        // { admin: [...], user: [...] }
```

---

## Option

Functions for working with `Option<T>` (`Some(v)` / `None`) values.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Option.map` | `Option<T>, fn(T) -> U -> Option<U>` | Transform the inner value if present |
| `Option.flatMap` | `Option<T>, fn(T) -> Option<U> -> Option<U>` | Chain option-returning operations |
| `Option.unwrapOr` | `Option<T>, T -> T` | Extract value or use default |
| `Option.isSome` | `Option<T> -> boolean` | Check if value is present |
| `Option.isNone` | `Option<T> -> boolean` | Check if value is absent |
| `Option.toResult` | `Option<T>, E -> Result<T, E>` | Convert to Result with error for None |

### Examples

```floe
// Transform without unwrapping
const upper = user.nickname
  |> Option.map(fn(n) String.toUpper(n))
// Some("RYAN") or None

// Chain lookups
const avatar = user.nickname
  |> Option.flatMap(fn(n) findAvatar(n))

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
| `Result.map` | `Result<T, E>, fn(T) -> U -> Result<U, E>` | Transform the Ok value |
| `Result.mapErr` | `Result<T, E>, fn(E) -> F -> Result<T, F>` | Transform the Err value |
| `Result.flatMap` | `Result<T, E>, fn(T) -> Result<U, E> -> Result<U, E>` | Chain result-returning operations |
| `Result.unwrapOr` | `Result<T, E>, T -> T` | Extract Ok value or use default |
| `Result.isOk` | `Result<T, E> -> boolean` | Check if result is Ok |
| `Result.isErr` | `Result<T, E> -> boolean` | Check if result is Err |
| `Result.toOption` | `Result<T, E> -> Option<T>` | Convert to Option (drops error) |

### Examples

```floe
// Transform success value
const doubled = fetchCount()
  |> Result.map(fn(n) n * 2)

// Handle errors
const result = fetchUser(id)
  |> Result.mapErr(fn(e) AppError(e))

// Chain operations
const profile = fetchUser(id)
  |> Result.flatMap(fn(u) fetchProfile(u.profileId))

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
| `String.startsWith` | `string, string -> boolean` | Check prefix |
| `String.endsWith` | `string, string -> boolean` | Check suffix |
| `String.contains` | `string, string -> boolean` | Check if substring exists |
| `String.toUpper` | `string -> string` | Convert to uppercase |
| `String.toLower` | `string -> string` | Convert to lowercase |
| `String.length` | `string -> number` | Character count |
| `String.slice` | `string, number, number -> string` | Extract substring |
| `String.padStart` | `string, number, string -> string` | Pad from the start |
| `String.padEnd` | `string, number, string -> string` | Pad from the end |
| `String.repeat` | `string, number -> string` | Repeat n times |

### Examples

```floe
// Pipe-friendly
const cleaned = "  Hello, World!  "
  |> String.trim
  |> String.toLower
  |> String.replace("world", "floe")
// "hello, floe!"

// Split and process
const words = "one,two,three"
  |> String.split(",")
  |> Array.map(fn(w) String.toUpper(w))
// ["ONE", "TWO", "THREE"]
```

---

## Number

Safe numeric operations. Parsing returns `Result` instead of `NaN`.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Number.parse` | `string -> Result<number, ParseError>` | Strict parse (no partial, no NaN) |
| `Number.clamp` | `number, number, number -> number` | Clamp between min and max |
| `Number.isFinite` | `number -> boolean` | Check if finite |
| `Number.isInteger` | `number -> boolean` | Check if integer |
| `Number.toFixed` | `number, number -> string` | Format with fixed decimals |
| `Number.toString` | `number -> string` | Convert to string |

### Examples

```floe
// Safe parsing - no more NaN surprises
const result = "42" |> Number.parse
// Ok(42)

const bad = "not a number" |> Number.parse
// Err(ParseError)

// Must handle the Result
match Number.parse(input) {
  Ok(n)  -> processNumber(n),
  Err(_) -> showError("Invalid number"),
}

// Clamp to range
const score = rawScore |> Number.clamp(0, 100)
```

---

## Console

Output functions for debugging. These compile directly to their JavaScript `console` equivalents.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Console.log` | `T -> ()` | Log a value |
| `Console.warn` | `T -> ()` | Log a warning |
| `Console.error` | `T -> ()` | Log an error |
| `Console.info` | `T -> ()` | Log info |
| `Console.debug` | `T -> ()` | Log debug info |
| `Console.time` | `string -> ()` | Start a named timer |
| `Console.timeEnd` | `string -> ()` | End a named timer and print duration |

### Examples

```floe
Console.log("hello")
Console.warn("careful")

// Timing
Console.time("fetch")
const data = try fetchData()?
Console.timeEnd("fetch")
```

---

## Math

Standard math functions. Compile directly to JavaScript `Math` methods.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Math.floor` | `number -> number` | Round down |
| `Math.ceil` | `number -> number` | Round up |
| `Math.round` | `number -> number` | Round to nearest integer |
| `Math.abs` | `number -> number` | Absolute value |
| `Math.min` | `number, number -> number` | Smaller of two values |
| `Math.max` | `number, number -> number` | Larger of two values |
| `Math.pow` | `number, number -> number` | Exponentiation |
| `Math.sqrt` | `number -> number` | Square root |
| `Math.sign` | `number -> number` | Sign (-1, 0, or 1) |
| `Math.trunc` | `number -> number` | Remove fractional digits |
| `Math.log` | `number -> number` | Natural logarithm |
| `Math.sin` | `number -> number` | Sine |
| `Math.cos` | `number -> number` | Cosine |
| `Math.tan` | `number -> number` | Tangent |
| `Math.random` | `fn() -> number` | Random number between 0 (inclusive) and 1 (exclusive) |

### Examples

```floe
const rounded = 3.7 |> Math.floor    // 3
const clamped = Math.max(0, Math.min(score, 100))
const hyp = Math.sqrt(a * a + b * b)
```

---

## JSON

JSON serialization and parsing. `JSON.parse` returns `Result` instead of throwing.

| Function | Signature | Description |
|----------|-----------|-------------|
| `JSON.stringify` | `T -> string` | Serialize a value to JSON |
| `JSON.parse` | `string -> Result<T, ParseError>` | Parse JSON string safely |

### Examples

```floe
const json = user |> JSON.stringify
// '{"name":"Alice","age":30}'

const parsed = json |> JSON.parse
// Ok({name: "Alice", age: 30})

match JSON.parse(input) {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

---

## Map

Immutable key-value map operations. All functions return new maps -- they never mutate the original.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Map.empty` | `() -> Map<K, V>` | Create an empty map |
| `Map.fromArray` | `Array<[K, V]> -> Map<K, V>` | Create a map from key-value pairs |
| `Map.get` | `Map<K, V>, K -> Option<V>` | Look up a value by key |
| `Map.set` | `Map<K, V>, K, V -> Map<K, V>` | Add or update a key-value pair |
| `Map.remove` | `Map<K, V>, K -> Map<K, V>` | Remove a key-value pair |
| `Map.has` | `Map<K, V>, K -> boolean` | Check if a key exists |
| `Map.keys` | `Map<K, V> -> Array<K>` | Get all keys |
| `Map.values` | `Map<K, V> -> Array<V>` | Get all values |
| `Map.entries` | `Map<K, V> -> Array<[K, V]>` | Get all key-value pairs |
| `Map.size` | `Map<K, V> -> number` | Number of entries |
| `Map.isEmpty` | `Map<K, V> -> boolean` | True if map has no entries |
| `Map.merge` | `Map<K, V>, Map<K, V> -> Map<K, V>` | Merge two maps (second wins on conflict) |

### Examples

```floe
// Create a map from key-value pairs
const config = Map.fromArray([("host", "localhost"), ("port", "8080")])

// All operations are immutable
const updated = config
  |> Map.set("port", "3000")
  |> Map.set("debug", "true")

// Safe lookup returns Option
const port = Map.get(config, "port")   // Some("8080")
const missing = Map.get(config, "foo") // None

// Check membership
const hasHost = config |> Map.has("host")   // true

// Convert to arrays
const keys = config |> Map.keys      // ["host", "port"]
const values = config |> Map.values  // ["localhost", "8080"]

// Merge maps (second map's values win on key conflict)
const defaults = Map.fromArray([("port", "80"), ("host", "0.0.0.0")])
const merged = Map.merge(defaults, config)
// Map { "port" => "8080", "host" => "localhost" }
```

---

## Set

Immutable unique collection operations. All functions return new sets -- they never mutate the original.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Set.empty` | `() -> Set<T>` | Create an empty set |
| `Set.fromArray` | `Array<T> -> Set<T>` | Create a set from an array |
| `Set.toArray` | `Set<T> -> Array<T>` | Convert a set to an array |
| `Set.add` | `Set<T>, T -> Set<T>` | Add an element |
| `Set.remove` | `Set<T>, T -> Set<T>` | Remove an element |
| `Set.has` | `Set<T>, T -> boolean` | Check if an element exists |
| `Set.size` | `Set<T> -> number` | Number of elements |
| `Set.isEmpty` | `Set<T> -> boolean` | True if set has no elements |
| `Set.union` | `Set<T>, Set<T> -> Set<T>` | Union of two sets |
| `Set.intersect` | `Set<T>, Set<T> -> Set<T>` | Intersection of two sets |
| `Set.diff` | `Set<T>, Set<T> -> Set<T>` | Difference (elements in first but not second) |

### Examples

```floe
// Create a set from an array
const tags = Set.fromArray(["urgent", "bug", "frontend"])

// All operations are immutable
const updated = tags
  |> Set.add("backend")
  |> Set.remove("frontend")

// Check membership
const isUrgent = tags |> Set.has("urgent")   // true

// Set operations
const teamA = Set.fromArray(["alice", "bob", "carol"])
const teamB = Set.fromArray(["bob", "carol", "dave"])

const everyone = Set.union(teamA, teamB)       // {"alice", "bob", "carol", "dave"}
const overlap = Set.intersect(teamA, teamB)    // {"bob", "carol"}
const onlyA = Set.diff(teamA, teamB)           // {"alice"}

// Convert back to array
const tagList = tags |> Set.toArray
```

---

## Http

Pipe-friendly HTTP functions that return `Result` natively. No `try` wrapper needed -- errors are captured automatically.

| Function | Signature | Description |
|----------|-----------|-------------|
| `Http.get` | `string -> Result<Response, Error>` | GET request |
| `Http.post` | `string, unknown -> Result<Response, Error>` | POST request with JSON body |
| `Http.put` | `string, unknown -> Result<Response, Error>` | PUT request with JSON body |
| `Http.delete` | `string -> Result<Response, Error>` | DELETE request |
| `Http.json` | `Response -> Result<unknown, Error>` | Parse response body as JSON |
| `Http.text` | `Response -> Result<string, Error>` | Read response body as text |

### Examples

```floe
// Simple GET and parse JSON
const data = await Http.get("https://api.example.com/users")? |> Http.json?

// POST with a body
const result = await Http.post("https://api.example.com/users", { name: "Alice" })?

// Full pipeline
const users = await Http.get(url)?
  |> Http.json?
  |> Result.map(fn(data) Array.filter(data, .active))

// Error handling with match
match await Http.get(url) {
  Ok(response) -> Http.json(response),
  Err(e) -> Console.error(e),
}
```

All Http functions are async and return `Result`. Use `await` and `?` for ergonomic error handling in pipelines.

---

## Pipe

Utility functions for pipeline debugging and control flow.

| Function | Signature | Description |
|----------|-----------|-------------|
| `tap` | `T, fn(T) -> () -> T` | Call a function for side effects, return value unchanged |

### Examples

```floe
// Debug a pipeline without breaking the chain
const result = orders
  |> Array.filter(.active)
  |> tap(Console.log)         // logs filtered orders, passes them through
  |> Array.map(.total)
  |> Array.reduce(fn(sum, n) sum + n, 0)

// Use a closure for custom logging
const processed = data
  |> transform
  |> tap(fn(x) Console.log("after transform:", x))
  |> validate

// Works with any type
const name = "  Alice  "
  |> String.trim
  |> tap(Console.log)         // logs "Alice"
  |> String.toUpper           // "ALICE"
```

`tap` is the pipeline equivalent of a `console.log` that doesn't interrupt the flow. The function you pass receives the value but its return is ignored -- the original value passes through unchanged.

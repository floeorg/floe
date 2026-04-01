---
title: Array
sidebar:
  order: 1
---

All array functions return new arrays. They never mutate the original.

All stdlib functions are **pipe-friendly**: the first argument is the data, so they work naturally with `|>`.

```floe
[3, 1, 2]
  |> Array.sort
  |> Array.map((n) => n * 10)
  |> Array.reverse
// [30, 20, 10]
```

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Array.sort` | `Array<number> -> Array<number>` | Sort numerically (returns new array) |
| `Array.sortBy` | `Array<T>, (T) -> number -> Array<T>` | Sort by numeric key function |
| `Array.map` | `Array<T>, (T) -> U -> Array<U>` | Transform each element |
| `Array.filter` | `Array<T>, (T) -> boolean -> Array<T>` | Keep elements matching predicate |
| `Array.find` | `Array<T>, (T) -> boolean -> Option<T>` | First element matching predicate |
| `Array.findIndex` | `Array<T>, (T) -> boolean -> Option<number>` | Index of first match |
| `Array.findLast` | `Array<T>, (T) -> boolean -> Option<T>` | Last element matching predicate |
| `Array.flatMap` | `Array<T>, (T) -> Array<U> -> Array<U>` | Map then flatten one level |
| `Array.filterMap` | `Array<T>, (T) -> Option<U> -> Array<U>` | Map + filter in one pass (keeps Some values) |
| `Array.at` | `Array<T>, number -> Option<T>` | Safe index access |
| `Array.contains` | `Array<T>, T -> boolean` | Check if element exists (structural equality) |
| `Array.head` | `Array<T> -> Option<T>` | First element |
| `Array.last` | `Array<T> -> Option<T>` | Last element |
| `Array.take` | `Array<T>, number -> Array<T>` | First n elements |
| `Array.takeWhile` | `Array<T>, (T) -> boolean -> Array<T>` | Take elements while predicate holds |
| `Array.drop` | `Array<T>, number -> Array<T>` | All except first n elements |
| `Array.dropWhile` | `Array<T>, (T) -> boolean -> Array<T>` | Drop elements while predicate holds |
| `Array.reverse` | `Array<T> -> Array<T>` | Reverse order (returns new array) |
| `Array.flatten` | `Array<Array<T>> -> Array<T>` | Flatten one level of nesting |
| `Array.reduce` | `Array<T>, (U, T) -> U, U -> U` | Fold with reducer and initial value |
| `Array.length` | `Array<T> -> number` | Number of elements |
| `Array.any` | `Array<T>, (T) -> boolean -> boolean` | True if any element matches predicate |
| `Array.all` | `Array<T>, (T) -> boolean -> boolean` | True if all elements match predicate |
| `Array.sum` | `Array<number> -> number` | Sum all elements |
| `Array.join` | `Array<string>, string -> string` | Join elements with separator |
| `Array.isEmpty` | `Array<T> -> boolean` | True if array has no elements |
| `Array.chunk` | `Array<T>, number -> Array<Array<T>>` | Split into chunks of given size |
| `Array.unique` | `Array<T> -> Array<T>` | Remove duplicate elements |
| `Array.groupBy` | `Array<T>, (T) -> string -> Record` | Group elements by key function |
| `Array.zip` | `Array<T>, Array<U> -> Array<(T, U)>` | Pair elements by index from two arrays |
| `Array.concat` | `Array<T>, Array<T> -> Array<T>` | Concatenate two arrays |
| `Array.append` | `Array<T>, T -> Array<T>` | Append an element to the end |
| `Array.prepend` | `Array<T>, T -> Array<T>` | Prepend an element to the start |
| `Array.from` | `T, (T, number) -> U -> Array<U>` | Create array from iterable with mapping |
| `Array.partition` | `Array<T>, (T) -> boolean -> (Array<T>, Array<T>)` | Split into (matching, non-matching) |
| `Array.intersperse` | `Array<T>, T -> Array<T>` | Insert element between every pair |
| `Array.mapResult` | `Array<T>, (T) -> Result<U, E> -> Result<Array<U>, E>` | Map fallible function, short-circuit on first Err |

## Examples

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

// filterMap — map + filter in one pass
const ages = inputs |> Array.filterMap((s) => Number.parse(s) |> Result.toOption)

// partition — split into two groups
const (adults, minors) = users |> Array.partition(.age >= 18)

// intersperse — great for React
const items = ["Home", "About", "Contact"]
  |> Array.intersperse(" | ")
// ["Home", " | ", "About", " | ", "Contact"]

// Utilities
const empty = Array.isEmpty([])          // true
const chunks = [1, 2, 3, 4, 5] |> Array.chunk(2)   // [[1, 2], [3, 4], [5]]
const deduped = [1, 2, 2, 3] |> Array.unique        // [1, 2, 3]
const grouped = users |> Array.groupBy(.role)        // { admin: [...], user: [...] }
```

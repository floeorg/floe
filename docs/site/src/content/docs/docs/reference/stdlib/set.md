---
title: Set
sidebar:
  order: 8
---

Immutable unique collection operations. All functions return new sets -- they never mutate the original.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Set.empty` | `() => Set<T>` | Create an empty set |
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

## Examples

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

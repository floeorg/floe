---
title: Map
sidebar:
  order: 7
---

Immutable key-value map operations. All functions return new maps -- they never mutate the original.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Map.empty` | `() => Map<K, V>` | Create an empty map |
| `Map.fromArray` | `Array<(K, V)> -> Map<K, V>` | Create a map from key-value pairs |
| `Map.get` | `Map<K, V>, K -> Option<V>` | Look up a value by key |
| `Map.set` | `Map<K, V>, K, V -> Map<K, V>` | Add or update a key-value pair |
| `Map.remove` | `Map<K, V>, K -> Map<K, V>` | Remove a key-value pair |
| `Map.has` | `Map<K, V>, K -> boolean` | Check if a key exists |
| `Map.keys` | `Map<K, V> -> Array<K>` | Get all keys |
| `Map.values` | `Map<K, V> -> Array<V>` | Get all values |
| `Map.entries` | `Map<K, V> -> Array<(K, V)>` | Get all key-value pairs |
| `Map.size` | `Map<K, V> -> number` | Number of entries |
| `Map.isEmpty` | `Map<K, V> -> boolean` | True if map has no entries |
| `Map.merge` | `Map<K, V>, Map<K, V> -> Map<K, V>` | Merge two maps (second wins on conflict) |

## Examples

```floe
// Create a map from key-value pairs
let config = Map.fromArray([("host", "localhost"), ("port", "8080")])

// All operations are immutable
let updated = config
  |> Map.set("port", "3000")
  |> Map.set("debug", "true")

// Safe lookup returns Option
let port = Map.get(config, "port")   // Some("8080")
let missing = Map.get(config, "foo") // None

// Check membership
let hasHost = config |> Map.has("host")   // true

// Convert to arrays
let keys = config |> Map.keys      // ["host", "port"]
let values = config |> Map.values  // ["localhost", "8080"]

// Merge maps (second map's values win on key conflict)
let defaults = Map.fromArray([("port", "80"), ("host", "0.0.0.0")])
let merged = Map.merge(defaults, config)
// Map { "port" -> "8080", "host" -> "localhost" }
```

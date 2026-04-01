---
title: String
sidebar:
  order: 5
---

Pipe-friendly string operations.

## Functions

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
| `String.toUpperCase` | `string -> string` | Convert to uppercase |
| `String.toLowerCase` | `string -> string` | Convert to lowercase |
| `String.length` | `string -> number` | Character count |
| `String.slice` | `string, number, number -> string` | Extract substring |
| `String.padStart` | `string, number, string -> string` | Pad from the start |
| `String.padEnd` | `string, number, string -> string` | Pad from the end |
| `String.repeat` | `string, number -> string` | Repeat n times |
| `String.localeCompare` | `string, string -> number` | Locale-aware string comparison |

## Examples

```floe
// Pipe-friendly
const cleaned = "  Hello, World!  "
  |> String.trim
  |> String.toLowerCase
  |> String.replace("world", "floe")
// "hello, floe!"

// Split and process
const words = "one,two,three"
  |> String.split(",")
  |> Array.map((w) => String.toUpperCase(w))
// ["ONE", "TWO", "THREE"]
```

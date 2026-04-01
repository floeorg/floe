---
title: Number
sidebar:
  order: 6
---

Safe numeric operations. Parsing returns `Result` instead of `NaN`.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Number.parse` | `string -> Result<number, ParseError>` | Strict parse (no partial, no NaN) |
| `Number.clamp` | `number, number, number -> number` | Clamp between min and max |
| `Number.isFinite` | `number -> boolean` | Check if finite |
| `Number.isInteger` | `number -> boolean` | Check if integer |
| `Number.toFixed` | `number, number -> string` | Format with fixed decimals |
| `Number.toString` | `number -> string` | Convert to string |

## Examples

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

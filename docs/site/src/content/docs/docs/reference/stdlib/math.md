---
title: Math
sidebar:
  order: 12
---

Standard math functions. Compile directly to JavaScript `Math` methods.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Math.floor` | `(number) -> number` | Round down |
| `Math.ceil` | `(number) -> number` | Round up |
| `Math.round` | `(number) -> number` | Round to nearest integer |
| `Math.abs` | `(number) -> number` | Absolute value |
| `Math.min` | `(number, number) -> number` | Smaller of two values |
| `Math.max` | `(number, number) -> number` | Larger of two values |
| `Math.pow` | `(number, number) -> number` | Exponentiation |
| `Math.sqrt` | `(number) -> number` | Square root |
| `Math.sign` | `(number) -> number` | Sign (-1, 0, or 1) |
| `Math.trunc` | `(number) -> number` | Remove fractional digits |
| `Math.log` | `(number) -> number` | Natural logarithm |
| `Math.sin` | `(number) -> number` | Sine |
| `Math.cos` | `(number) -> number` | Cosine |
| `Math.tan` | `(number) -> number` | Tangent |
| `Math.random` | `() -> number` | Random number between 0 (inclusive) and 1 (exclusive) |

## Examples

```floe
let rounded = 3.7 |> Math.floor    // 3
let clamped = Math.max(0, Math.min(score, 100))
let hyp = Math.sqrt(a * a + b * b)
```

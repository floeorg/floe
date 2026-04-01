---
title: Pipe Utilities
sidebar:
  order: 15
---

Utility functions for pipeline debugging and control flow.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `tap` | `T, (T) -> () -> T` | Call a function for side effects, return value unchanged |

## Examples

```floe
// Debug a pipeline without breaking the chain
const result = orders
  |> Array.filter(.active)
  |> tap(Console.log)         // logs filtered orders, passes them through
  |> Array.map(.total)
  |> Array.reduce((sum, n) => sum + n, 0)

// Use a closure for custom logging
const processed = data
  |> transform
  |> tap((x) => Console.log("after transform:", x))
  |> validate

// Works with any type
const name = "  Alice  "
  |> String.trim
  |> tap(Console.log)         // logs "Alice"
  |> String.toUpperCase       // "ALICE"
```

`tap` is the pipeline equivalent of a `console.log` that doesn't interrupt the flow. The function you pass receives the value but its return is ignored -- the original value passes through unchanged.

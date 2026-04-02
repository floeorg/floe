---
title: Console
sidebar:
  order: 11
---

Output functions for debugging. These compile directly to their JavaScript `console` equivalents.

:::note
Use `Console.log` (capital C) in Floe, not `console.log`.
:::

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Console.log` | `T -> ()` | Log a value |
| `Console.warn` | `T -> ()` | Log a warning |
| `Console.error` | `T -> ()` | Log an error |
| `Console.info` | `T -> ()` | Log info |
| `Console.debug` | `T -> ()` | Log debug info |
| `Console.time` | `string -> ()` | Start a named timer |
| `Console.timeEnd` | `string -> ()` | End a named timer and print duration |

## Examples

```floe
Console.log("hello")
Console.warn("careful")

// Timing
Console.time("fetch")
const data = fetchData()?
Console.timeEnd("fetch")
```

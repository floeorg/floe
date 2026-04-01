---
title: Date
sidebar:
  order: 9
---

Date construction and accessors. Floe has no `new` keyword, so the `Date` module bridges the gap for creating JS `Date` objects and reading their fields.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Date.now` | `() -> Date` | Current date and time |
| `Date.from` | `string -> Date` | Parse a date string |
| `Date.fromMillis` | `number -> Date` | Create from Unix milliseconds |
| `Date.year` | `Date -> number` | Full year (e.g. 2024) |
| `Date.month` | `Date -> number` | Month (1-12, 1-indexed!) |
| `Date.day` | `Date -> number` | Day of month (1-31) |
| `Date.hour` | `Date -> number` | Hour (0-23) |
| `Date.minute` | `Date -> number` | Minute (0-59) |
| `Date.second` | `Date -> number` | Second (0-59) |
| `Date.millis` | `Date -> number` | Unix timestamp in milliseconds |
| `Date.toIso` | `Date -> string` | ISO 8601 string |

## Examples

```floe
// Create dates
const now = Date.now()
const release = Date.from("2024-06-15")
const epoch = Date.fromMillis(0)

// Read fields with pipes
const year = release |> Date.year         // 2024
const month = release |> Date.month       // 6 (1-indexed, not 0!)
const day = release |> Date.day           // 15

// Convert to string or timestamp
const iso = now |> Date.toIso             // "2024-..."
const timestamp = now |> Date.millis      // 1718...
```

`Date.month` returns 1-12 (January = 1), unlike JavaScript's `getMonth()` which returns 0-11. This fixes a common JS footgun.

For date arithmetic, formatting, and timezone handling, use npm libraries like `date-fns` via interop -- the stdlib only covers construction and basic access.

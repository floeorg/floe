---
title: URLSearchParams
sidebar:
  order: 17
---

Read-only access to query-string parameters. Get one via `URL.searchParams` from a parsed URL, or build one directly with `URLSearchParams.parse` / `URLSearchParams.fromArray`.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `URLSearchParams.parse` | `(string) -> URLSearchParams` | Parse a query string. Malformed input becomes empty entries (never throws). |
| `URLSearchParams.fromArray` | `(Array<(string, string)>) -> URLSearchParams` | Build from a tuple list |
| `URLSearchParams.get` | `(URLSearchParams, string) -> Option<string>` | First value for a key, or `None` |
| `URLSearchParams.getAll` | `(URLSearchParams, string) -> Array<string>` | All values for a key |
| `URLSearchParams.has` | `(URLSearchParams, string) -> bool` | Whether a key is present |
| `URLSearchParams.keys` | `(URLSearchParams) -> Array<string>` | All keys (with duplicates) |
| `URLSearchParams.values` | `(URLSearchParams) -> Array<string>` | All values |
| `URLSearchParams.entries` | `(URLSearchParams) -> Array<(string, string)>` | All key/value pairs |
| `URLSearchParams.size` | `(URLSearchParams) -> number` | Total number of entries |
| `URLSearchParams.toString` | `(URLSearchParams) -> string` | Encoded query string, no leading `?` |

## Examples

```floe
let p = URLSearchParams.parse("a=1&b=2&a=3")

let first = p |> URLSearchParams.get("a")        // Some("1")
let all = p |> URLSearchParams.getAll("a")       // ["1", "3"]
let n = p |> URLSearchParams.size                // 3
let s = p |> URLSearchParams.toString            // "a=1&b=2&a=3"
```

Build from a tuple list:

```floe
let p = URLSearchParams.fromArray([("q", "floe"), ("page", "2")])
p |> URLSearchParams.toString    // "q=floe&page=2"
```

Mutation methods (`set`, `append`, `delete`) are intentionally not exposed today — Floe favours immutable construction. If you need to add or change a key, build a new `URLSearchParams` from the merged entries.

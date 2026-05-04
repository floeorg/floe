---
title: URL
sidebar:
  order: 16
---

URL parsing and field access. The Floe `URL` type is the same nominal `URL` as the runtime — passing a value to a TS function expecting `URL` is zero-cost.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `URL.parse` | `(string) -> Result<URL, ParseError>` | Parse a URL string. Returns `Err` on invalid input. |
| `URL.href` | `(URL) -> string` | Full URL string |
| `URL.origin` | `(URL) -> string` | `protocol://host` |
| `URL.protocol` | `(URL) -> string` | e.g. `"https:"` (note the colon) |
| `URL.host` | `(URL) -> string` | `hostname` plus optional `:port` |
| `URL.hostname` | `(URL) -> string` | Just the host, no port |
| `URL.port` | `(URL) -> string` | Port as string, `""` when default |
| `URL.pathname` | `(URL) -> string` | Path component |
| `URL.search` | `(URL) -> string` | Query string including leading `?` |
| `URL.searchParams` | `(URL) -> URLSearchParams` | Parsed query parameters |
| `URL.hash` | `(URL) -> string` | Fragment including leading `#` |
| `URL.toString` | `(URL) -> string` | Stringifies via `href` |

## Examples

```floe
match URL.parse("https://example.com/posts?id=42") {
    Ok(u) -> {
        let host = u |> URL.host           // "example.com"
        let path = u |> URL.pathname       // "/posts"
        let id = u |> URL.searchParams |> URLSearchParams.get("id")
        Console.log(host, path, id)
    },
    Err(e) -> Console.error(e.message),
}
```

`URL.parse` is the only construction path — there is no `URL("...")` shorthand. The `Result` return type forces callers to handle malformed input rather than letting it throw at runtime.

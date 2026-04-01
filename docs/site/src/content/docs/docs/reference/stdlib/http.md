---
title: Http
sidebar:
  order: 10
---

Pipe-friendly HTTP functions that return `Result` natively. No `try` wrapper needed -- errors are captured automatically.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Http.get` | `string -> Result<Response, Error>` | GET request |
| `Http.post` | `string, unknown -> Result<Response, Error>` | POST request with JSON body |
| `Http.put` | `string, unknown -> Result<Response, Error>` | PUT request with JSON body |
| `Http.delete` | `string -> Result<Response, Error>` | DELETE request |
| `Http.json` | `Response -> Result<unknown, Error>` | Parse response body as JSON |
| `Http.text` | `Response -> Result<string, Error>` | Read response body as text |

## Examples

```floe
// Simple GET and parse JSON
const data = await Http.get("https://api.example.com/users")? |> Http.json?

// POST with a body
const result = await Http.post("https://api.example.com/users", { name: "Alice" })?

// Full pipeline
const users = await Http.get(url)?
  |> Http.json?
  |> Result.map((data) => Array.filter(data, .active))

// Error handling with match
match await Http.get(url) {
  Ok(response) -> Http.json(response),
  Err(e) -> Console.error(e),
}
```

All Http functions are async and return `Result`. Use `await` and `?` for ergonomic error handling in pipelines.

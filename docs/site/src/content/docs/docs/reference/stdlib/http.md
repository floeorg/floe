---
title: Http
sidebar:
  order: 10
---

Pipe-friendly HTTP functions that return `Promise<Result<...>>` natively. As a stdlib module, errors are captured automatically.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Http.get` | `string -> Promise<Result<Response, Error>>` | GET request |
| `Http.post` | `string, unknown -> Promise<Result<Response, Error>>` | POST request with JSON body |
| `Http.put` | `string, unknown -> Promise<Result<Response, Error>>` | PUT request with JSON body |
| `Http.delete` | `string -> Promise<Result<Response, Error>>` | DELETE request |
| `Http.json` | `Response -> Promise<Result<unknown, Error>>` | Parse response body as JSON |
| `Http.text` | `Response -> Promise<Result<string, Error>>` | Read response body as text |

## Examples

```floe
// Simple GET and parse JSON
let data = Http.get("https://api.example.com/users") |> Promise.await? |> Http.json |> Promise.await?

// POST with a body
let result = Http.post("https://api.example.com/users", { name: "Alice" }) |> Promise.await?

// Full pipeline
let users = Http.get(url) |> Promise.await?
  |> Http.json |> Promise.await?
  |> Result.map((data) => Array.filter(data, .active))

// Error handling with match
match Http.get(url) |> Promise.await {
  Ok(response) -> Http.json(response) |> Promise.await,
  Err(e) -> Console.error(e),
}
```

All Http functions return `Promise<Result<...>>`. Use `Promise.await` and `?` for ergonomic error handling in pipelines.

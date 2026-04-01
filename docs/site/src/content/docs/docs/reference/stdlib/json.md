---
title: JSON
sidebar:
  order: 13
---

JSON serialization and parsing. `JSON.parse` returns `Result` instead of throwing.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `JSON.stringify` | `T -> string` | Serialize a value to JSON |
| `JSON.parse` | `string -> Result<T, ParseError>` | Parse JSON string safely |

## Examples

```floe
const json = user |> JSON.stringify
// '{"name":"Alice","age":30}'

const parsed = json |> JSON.parse
// Ok({name: "Alice", age: 30})

match JSON.parse(input) {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

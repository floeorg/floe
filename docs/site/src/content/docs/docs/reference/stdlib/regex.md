---
title: RegExp
sidebar:
  order: 18
---

Regular expression compilation and matching. The Floe `RegExp` type is the runtime `RegExp` — passing it to TS APIs that take `RegExp` is zero-cost.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `RegExp.compile` | `(string, string) -> Result<RegExp, ParseError>` | Compile pattern + flags. Returns `Err` on invalid syntax. |
| `RegExp.test` | `(RegExp, string) -> bool` | Whether the pattern matches the string |
| `RegExp.match` | `(RegExp, string) -> Option<Array<string>>` | Capture groups, or `None` if no match |
| `RegExp.source` | `(RegExp) -> string` | The original pattern string |
| `RegExp.flags` | `(RegExp) -> string` | The flags string (e.g. `"i"`, `"gm"`) |

## Examples

```floe
match RegExp.compile("^[a-z]+", "i") {
    Ok(r) -> {
        let isWord = r |> RegExp.test("Hello")        // true
        let captures = r |> RegExp.match("Hello world")
        // captures: Some(["Hello"])
    },
    Err(e) -> Console.error(e.message),
}
```

The `flags` argument is required — pass `""` when you don't need any. Common flags:

| Flag | Meaning |
|------|---------|
| `"i"` | Case-insensitive |
| `"g"` | Global (find all matches) |
| `"m"` | Multiline (`^` / `$` match line boundaries) |
| `"s"` | Dot matches newline |
| `"u"` | Unicode |

`RegExp.match` returns the first match (or all matches when the `g` flag is set), so the result type is `Option<Array<string>>` — the outer `Option` represents "did the pattern match at all?", and the inner `Array<string>` holds the captured groups.

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
| `RegExp.exec` | `(RegExp, string) -> Option<Array<string>>` | First match's full text + capture groups, or `None` |
| `RegExp.source` | `(RegExp) -> string` | The original pattern string |
| `RegExp.flags` | `(RegExp) -> string` | The flags string (e.g. `"i"`, `"gm"`) |

## Examples

```floe
match RegExp.compile("^[a-z]+", "i") {
    Ok(r) -> {
        let isWord = r |> RegExp.test("Hello")        // true
        let captures = r |> RegExp.exec("Hello world")
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

`RegExp.exec` returns the first match plus its captured groups: `Some(["fullMatch", "cap1", "cap2", ...])`, or `None` if the pattern didn't match. The name mirrors the underlying `RegExp.prototype.exec` JS API. With the `"g"` flag the underlying regex object advances `lastIndex` between calls, so re-using a global regex across calls walks through matches one at a time — bind to a fresh regex each time if that's not what you want.

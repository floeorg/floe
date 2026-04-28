---
title: Operators Reference
---

## Arithmetic

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `a + b` |
| `-` | Subtraction / negation | `a - b`, `-x` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `%` | Modulo | `a % b` |

## Comparison

Equality operators compile to strict equality (`===`, `!==`). Structural equality is used for `==` between same types.

| Operator | Description | Compiles to |
|----------|-------------|-------------|
| `==` | Equal | `===` |
| `!=` | Not equal | `!==` |
| `<` | Less than | `<` |
| `>` | Greater than | `>` |
| `<=` | Less or equal | `<=` |
| `>=` | Greater or equal | `>=` |

## Logical

| Operator | Description | Example |
|----------|-------------|---------|
| `&&` | Logical AND | `a && b` |
| `\|\|` | Logical OR | `a \|\| b` |
| `!` | Logical NOT | `!a` |

## Pipe

| Operator | Description | Example |
|----------|-------------|---------|
| `\|>` | Pipe | `x \|> f` |
| `\|>?` | Pipe-unwrap | `x \|>? f` |

The pipe operator passes the left side as the first argument to the right side. Use `_` as a placeholder for non-first-argument positions.

```floe
let a = x |> f          // f(x)
let b = x |> f(a, _)    // f(a, x)
let c = x |> f |> g     // g(f(x))
let d = x |> match {
    Ok(v) -> v,
    Err(_) -> 0,
}
```

The pipe-unwrap operator `|>?` pipes the value and then unwraps the result — equivalent to `(x |> f)?`.

```floe
x |>? f         // (x |> f)? — unwraps Result/Option, returns early on Err/None
```

## Unwrap

| Operator | Description | Example |
|----------|-------------|---------|
| `?` | Unwrap Result/Option | `expr?` |

The `?` operator unwraps `Ok(value)` or `Some(value)`, and returns early with `Err(e)` or `None` on failure. Only valid inside functions that return `Result` or `Option`.

## Spread and Range

| Operator | Context | Example |
|----------|---------|---------|
| `..` | Record spread in constructors (must be last) | `User(name: "New", ..existing)` |
| `..` | Array rest in match patterns | `[first, ..rest]` |
| `...` | Type spread in record definitions | `type B = { ...A, extra: string }` |
| `1..10` | Range pattern in match | `match n { 1..10 -> "small" }` |

## Arrow and Closure Operators

| Operator | Context | Meaning |
|----------|---------|---------|
| `->` | Closures, match arms, return types, function types | `(x) -> x + 1`, `Ok(x) -> x`, `let add(a, b) -> number`, `(string) -> number` |
| `.field` | Dot shorthand | `.name` (implicit field-access closure) |
| `\|>` | Pipes | `data \|> transform` |

## Precedence (high to low)

1. Unary: `!`, `-`
2. Multiplicative: `*`, `/`, `%`
3. Additive: `+`, `-`
4. Comparison: `<`, `>`, `<=`, `>=`
5. Equality: `==`, `!=`
6. Logical AND: `&&`
7. Logical OR: `||`
8. Pipe: `|>`
9. Unwrap: `?`

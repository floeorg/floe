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

All comparisons compile to strict equality (`===`, `!==`).

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

The pipe operator passes the left side as the first argument to the right side. Use `_` as a placeholder for non-first-argument positions.

```floe
x |> f          // f(x)
x |> f(a, _)    // f(a, x)
x |> f |> g     // g(f(x))
x |> match { ... }  // match x { ... }
```

## Unwrap

| Operator | Description | Example |
|----------|-------------|---------|
| `?` | Unwrap Result/Option | `expr?` |

The `?` operator unwraps `Ok(value)` or `Some(value)`, and returns early with `Err(e)` or `None` on failure. Only valid inside functions that return `Result` or `Option`.

## Arrow and Closure Operators

| Operator | Context | Meaning |
|----------|---------|---------|
| `(x) =>` | Closures | `(x) => x + 1` |
| `.field` | Dot shorthand | `.name` (implicit field-access closure) |
| `->` | Match arms, return types, function types | `Ok(x) -> x`, `fn(string) -> number` |
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

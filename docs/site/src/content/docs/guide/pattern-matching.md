---
title: Pattern Matching
---

The `match` expression lets you branch on the shape of data. The compiler ensures every case is handled.

## Basic Match

```zenscript
match status {
  "active" -> handleActive(),
  "inactive" -> handleInactive(),
  _ -> handleUnknown(),
}
```

## Matching on Result

```zenscript
match fetchUser(id) {
  Ok(user) -> renderProfile(user),
  Err(error) -> renderError(error),
}
```

Both `Ok` and `Err` must be handled. Missing a case is a compile error.

## Matching on Option

```zenscript
match findItem(id) {
  Some(item) -> renderItem(item),
  None -> renderNotFound(),
}
```

## Union Types

```zenscript
type Shape =
  | Circle(radius: number)
  | Rectangle(width: number, height: number)
  | Triangle(base: number, height: number)

function area(shape: Shape): number {
  match shape {
    Circle(r) -> 3.14159 * r * r,
    Rectangle(w, h) -> w * h,
    Triangle(b, h) -> 0.5 * b * h,
  }
}
```

Adding a new variant to `Shape` without updating the `match` is a compile error.

## Range Patterns

```zenscript
match score {
  0..59 -> "F",
  60..69 -> "D",
  70..79 -> "C",
  80..89 -> "B",
  90..100 -> "A",
  _ -> "Invalid",
}
```

## Record Destructuring

```zenscript
match event {
  { type: "click", x, y } -> handleClick(x, y),
  { type: "keydown", key } -> handleKey(key),
  _ -> ignore(),
}
```

## Nested Patterns

```zenscript
match result {
  Ok(Some(value)) -> process(value),
  Ok(None) -> useDefault(),
  Err(e) -> handleError(e),
}
```

## Wildcard

The `_` pattern matches anything. Place it last as a default:

```zenscript
match value {
  1 -> "one",
  2 -> "two",
  _ -> "other",
}
```

## Exhaustiveness

The compiler checks that your match is exhaustive:

```zenscript
// Compile error: non-exhaustive match on bool
match enabled {
  true -> "on",
  // missing: false
}
```

This applies to:
- **Booleans** — must handle `true` and `false`
- **Result** — must handle `Ok` and `Err`
- **Option** — must handle `Some` and `None`
- **Unions** — must handle every variant (or use `_`)

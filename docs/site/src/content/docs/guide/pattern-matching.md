---
title: Pattern Matching
---

The `match` expression lets you branch on the shape of data. The compiler ensures every case is handled.

## Basic Match

```floe
match status {
  "active" -> handleActive(),
  "inactive" -> handleInactive(),
  _ -> handleUnknown(),
}
```

## Matching on Result

```floe
match fetchUser(id) {
  Ok(user) -> renderProfile(user),
  Err(error) -> renderError(error),
}
```

Both `Ok` and `Err` must be handled. Missing a case is a compile error.

## Matching on Option

```floe
match findItem(id) {
  Some(item) -> renderItem(item),
  None -> renderNotFound(),
}
```

## Union Types

```floe
type Shape =
  | Circle(radius: number)
  | Rectangle(width: number, height: number)
  | Triangle(base: number, height: number)

fn area(shape: Shape) -> number {
  match shape {
    Circle(r) -> 3.14159 * r * r,
    Rectangle(w, h) -> w * h,
    Triangle(b, h) -> 0.5 * b * h,
  }
}
```

Adding a new variant to `Shape` without updating the `match` is a compile error.

## Range Patterns

```floe
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

```floe
match event {
  { type: "click", x, y } -> handleClick(x, y),
  { type: "keydown", key } -> handleKey(key),
  _ -> ignore(),
}
```

## Nested Patterns

```floe
match result {
  Ok(Some(value)) -> process(value),
  Ok(None) -> useDefault(),
  Err(e) -> handleError(e),
}
```

## String Patterns

Match strings with `{name}` captures to extract parts:

```floe
fn route(url: string) -> Page {
  match url {
    "/users/{id}" -> fetchUser(id),
    "/users/{id}/posts/{postId}" -> fetchPost(id, postId),
    "/about" -> aboutPage(),
    _ -> notFound(),
  }
}
```

Captured variables (`id`, `postId`) are bound as `string` in the match arm body. The pattern compiles to regex matching with capture groups.

This is useful for URL routing, string parsing, and any case where you need to extract structured data from strings.

## Wildcard

The `_` pattern matches anything. Place it last as a default:

```floe
match value {
  1 -> "one",
  2 -> "two",
  _ -> "other",
}
```

## Guards

Use `when` to add a condition to a match arm. The arm only matches if both the pattern matches and the guard expression is true.

```floe
match user {
  User(age) when age >= 65 -> "senior",
  User(age) when age >= 18 -> "adult",
  User(age) -> "minor",
}
```

Bindings from the pattern are available in the guard expression.

Guards work with any pattern, including wildcards:

```floe
match score {
  _ when score > 90 -> "excellent",
  _ when score > 70 -> "good",
  _ -> "needs improvement",
}
```

A guarded arm does not count as exhaustive coverage for its pattern. You still need an unguarded catch-all or complete coverage:

```floe
match value {
  Ok(x) when x > 0 -> handle(x),
  // This alone is NOT exhaustive - the guard might fail.
  // Add unguarded arms:
  Ok(x) -> handleNegative(x),
  Err(e) -> handleError(e),
}
```

## Exhaustiveness

The compiler checks that your match is exhaustive:

```floe
// Compile error: non-exhaustive match on boolean
match enabled {
  true -> "on",
  // missing: false
}
```

This applies to:
- **Booleans** - must handle `true` and `false`
- **Result** - must handle `Ok` and `Err`
- **Option** - must handle `Some` and `None`
- **Unions** - must handle every variant (or use `_`)

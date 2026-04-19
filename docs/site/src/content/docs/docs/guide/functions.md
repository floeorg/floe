---
title: Functions & Const
---

## Const Declarations

All bindings are immutable. Use `const`:

```floe
let name = "Floe"
let count = 42
let active = true
```

With type annotations:

```floe
let name: string = "Floe"
let count: number = 42
```

### Destructuring

```floe
let (left, right) = getPair()      // `()` — tuple
let { name, age } = getUser()      // `{}` — record
```

Array destructuring (`const [a, b] = arr`) is rejected in `const`
bindings — it lies about runtime length. Use `Array.get(arr, i) =>
Option<T>` or a match pattern:

```floe
match arr {
  [first, ..rest] -> ...,
  _ -> ...,
}
```

## Functions

```floe
let add(a: number, b: number) -> number = {
  a + b
}
```

The last expression in a function body is the return value. The `return` keyword is not used in Floe.

In multi-statement functions, `floe fmt` adds a blank line before the final expression to visually separate the return value:

```floe
let loadProfile(id: string) -> Result<Profile, ApiError> = {
    let user = fetchUser(id)?
    let posts = fetchPosts(user.id)?
    let stats = computeStats(posts)

    Profile(user, posts, stats)
}
```

Exported functions **must** have return type annotations:

```floe
export let greet(name: string) -> string = {
  `Hello, ${name}!`
}
```

### Generic Functions

Functions can declare type parameters using angle brackets after the function name:

```floe
let identity<T>(x: T) -> T = { x }

let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }

let mapResult<T, U, E>(r: Result<T, E>, f: (T) -> U) -> Result<U, E> = {
    match r {
        Ok(value) -> Ok(f(value)),
        Err(e) -> Err(e),
    }
}
```

Generic functions compile directly to TypeScript generics:

```typescript
function identity<T>(x: T): T { return x; }
function pair<A, B>(a: A, b: B): readonly [A, B] { return [a, b] as const; }
```

### Default Parameters

```floe
let greet(name: string = "world") -> string = {
  `Hello, ${name}!`
}
```

### Anonymous Functions (Closures)

Use `(x) => expr` for inline anonymous functions:

```floe
todos |> Array.map((t) -> t.text)
items |> Array.reduce((acc, x) -> acc + x.price, 0)
onClick={() -> setCount(count + 1)}
```

For simple field access, use dot shorthand:

```floe
todos |> Array.filter(.done == false)
todos |> Array.map(.text)
users |> Array.sortBy(.name)
```

**`const name = (x) => ...` is a compile error.** If it has a name, use `fn`:

```floe
// COMPILE ERROR
let double(x) = x * 2

// correct
let double(x: number) -> number = { x * 2 }
```

### Function Types

Use `->` to describe function types:

```floe
type Transform = (string) -> number
type Predicate = (Todo) -> boolean
type Callback = () -> ()
```

### Async Functions

A function is async when its body uses `|> await` (or `|> Promise.await`). The return type must be `Promise<T>` -- the compiler enforces this, just like `?` requires `Result<T, E>`:

```floe
let fetchUser(id: string) -> Promise<User> = {
  let response = fetch(`/api/users/${id}`) |> await
  response.json() |> await
}
```

For functions without a return type annotation, the compiler infers `Promise<T>` automatically.

**`async fn` sugar.** When the return type is verbose (e.g. `Promise<Result<Option<T>, Error>>`), use `async fn f() => T` to write the inner type directly. The compiler wraps it in `Promise<>`:

```floe
// Verbose
let findUser(id: string) -> Promise<Result<Option<User>, Error>> = {
  // ...
}

// Sugar — the Promise<> wrapper is implied
async let findUser(id: string) -> Result<Option<User>, Error> = {
  // ...
}
```

Both forms compile to the same `async function` in TypeScript. Callers still use `|> await` to unwrap. See the [Promise reference](/docs/reference/stdlib/promise/#async-fn-sugar) for details.

## Callback Flattening with `use`

The `use` keyword flattens nested callbacks. The rest of the block becomes the callback body:

```floe
// Without use — deeply nested
File.open(path, (file) ->
    File.readAll(file, (contents) ->
        contents |> String.toUpper
    )
)

// With use — flat and readable
use file <- File.open(path)
use contents <- File.readAll(file)
contents |> String.toUpper
```

Zero-binding form for callbacks that don't pass a value:

```floe
use <- Timer.delay(1000)
Console.log("step 1")
use <- Timer.delay(500)
Console.log("done")
```

`use` works with any function whose last parameter is a callback. It's complementary to `?` (which only works on Result/Option).

## What's Not Here

- **No `let` or `var`** - all bindings are `const`
- **No `class`** - use functions and records
- **No `this`** - functions are pure by default
- **No `function*` generators** - use arrays and pipes
- **No `=>` at the statement level** - `(x) => expr` is only for inline anonymous functions; `->` is used for function types like `(T) => U`
- **No `function` keyword** - use `fn` for named functions

These are removed intentionally. See the [introduction](/docs/guide/introduction) for the reasoning.

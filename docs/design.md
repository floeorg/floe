# Floe — Design Decisions

## Vision

A Gleam-inspired language that compiles to vanilla TypeScript + React. Familiar syntax for TS/React developers, but with pipes, exhaustive matching, no escape hatches, and compile-time safety that eliminates entire categories of bugs. Zero runtime dependencies — the compiler does all the work, the output is boring `.tsx`.

---

## Pipeline

```
.fl source → Lexer → Parser → AST → Type Checker → Codegen → .tsx output → tsc/swc → JS
```

The compiler is a single Rust binary (`floe`) that takes `.fl` files and emits `.tsx`. From there, the user's existing build toolchain (Vite, Next.js, etc.) picks it up like any other TypeScript file.

---

## Syntax Design

### Principle: "TypeScript, but stricter and with pipes"

A React developer should read Floe and understand it in 30 minutes. We keep familiar syntax and add targeted upgrades.

### Key Operators

```
(x) => arrow closures         (a) => a + 1
->    match arms, fn types    Ok(x) -> x, (T) -> U
|>    pipe data through       data |> transform
?     unwrap Result/Option    fetchUser(id)?
.x    dot shorthand           .name (implicit closure for field access)
```

All four of TypeScript's `?` uses (`?.`, `??`, `?:`, `? :`) are removed. `?` now means exactly one thing: unwrap or short-circuit.

### What Stays from TypeScript

- `const`, `export`, `import`, type annotations
- `fn` for named/exported functions
- Closures `(x) => expr` for inline/anonymous functions
- Dot shorthand `.field` for implicit field-access closures
- JSX / TSX (full support, including member expressions like `<Select.Trigger />`)
- Generics (types and functions), template literals
- Async via `|> await` (or `|> Promise.await`) — return type must be `Promise<T>`, or use `async fn f() -> T` sugar
- Destructuring, spread, rest params
- `||` (boolean OR), `&&`, `!` (boolean operators)
- `==` (but only between same types — structural equality on objects)
- Unit type `()` instead of `void`

### What's Added

| Feature | Syntax | Compiles To |
|---------|--------|-------------|
| Pipe operator | `a \|> b \|> c` | `c(b(a))` |
| Pipe w/ placeholder | `a \|> f(x, _, y)` | `f(x, a, y)` |
| Pipe into match | `a \|> match { ... }` | `match a { ... }` (syntax sugar) |
| Pipe-unwrap | `a \|>? f` | `(a \|> f)?` (pipe then unwrap Result/Option) |
| Partial application | `add(10, _)` | `(x) => add(10, x)` |
| Match expression | `match x { ... }` | exhaustive if/else chain |
| Match with ranges | `match n { 1..10 -> ... }` | range check |
| Match with string patterns | `"/users/{id}" -> f(id)` | regex-based matching with captures |
| Match with destructuring | `Click(el, { x, y }) -> ...` | nested destructuring |
| Result type | built-in `Ok(v)` / `Err(e)` | `{ ok: true, value } / { ok: false, error }` |
| Option type | built-in `Some(v)` / `None` | `v / undefined` |
| `todo` | placeholder, type `never` | `(() => { throw new Error("not implemented"); })()` |
| `unreachable` | assert unreachable, type `never` | `(() => { throw new Error("unreachable"); })()` |
| `?` operator | `fetchUser(id)?` | early return on Err/None |
| Newtypes | `type UserId(string)` | `string` at runtime (single-variant wrapper) |
| Opaque types | `opaque type HashedPw(string)` | `string`, but only the defining module can create/read |
| Tagged unions | `type Route { \| Home \| Profile(string) }` | discriminated union (positional or named fields) |
| String literal unions | `type Method = "GET" \| "POST" \| "PUT"` | `"GET" \| "POST" \| "PUT"` (pass-through for npm interop) |
| Nested unions | `type ApiError { \| Network(NetworkError) \| NotFound }` | nested discriminated union (compiler generates tags) |
| Multi-depth match | `Network(Timeout(ms)) -> ...` | nested if/else with tag checks |
| Type constructors | `User(name: "Ryan", email: e)` | `{ name: "Ryan", email: e }` (compiler adds tags for unions) |
| Record spread | `User(..user, name: "New")` | `{ ...user, name: "New" }` |
| Named arguments | `fetchUsers(page: 3, limit: 50)` | `fetchUsers(3, 50)` (labels erased) |
| Closures | `(x) => x + 1` | `(x) => x + 1` |
| Dot shorthand | `.name` in callback position | `(x) => x.name` |
| Dot shorthand (predicate) | `.id != id` in callback position | `(x) => x.id != id` |
| Qualified variant | `Type.Variant` for disambiguation | `{ tag: "Variant" }` (same as bare) |
| Variant as function | `Validation` (bare, non-unit) | `(errors) => ({ tag: "Validation", errors })` |
| Generic functions | `fn identity<T>(x: T) -> T { x }` | `function identity<T>(x: T): T { return x; }` |
| Default values | `fn f(x: number = 10)` | caller can omit, compiler fills in |
| Structural equality | `==` on objects compares by value | deep equality check |
| Unit type | `()` as return type, usable in generics | `undefined` / `void` in TS |
| Tuple types | `(number, string)`, `(1, "a")` | `readonly [number, string]`, `[1, "a"] as const` |
| Tuple destructuring | `const (x, y) = pair` — `[x, y]` is for arrays, not tuples | `const [x, y] = pair` |
| Tuple match patterns | `(0, _) -> ...` | index-based match conditions |
| Array match patterns | `[first, ..rest] -> ...` | length check + index/slice access |
| `use` callback flattening | `use x <- f(arg)` | `f(arg, (x) => { rest })` (Gleam-style) |
| tap | `x \|> tap(Console.log)` | IIFE: calls fn, returns value unchanged |
| Immutable sort | `Array.sort` returns new array | sorted copy, no mutation |
| Immutable maps | `Map.set`, `Map.remove` return new maps | `new Map([...old, [k, v]])` |
| Record access | `Record.get`, `Record.keys`, etc. for TS `Record<K, V>` | `obj[key]`, `Object.keys(obj)` |
| Immutable sets | `Set.add`, `Set.remove` return new sets | `new Set([...old, val])` |
| Date construction | `Date.now()`, `Date.from("...")` | `new Date()`, `new Date("...")` |
| Strict parse | `Number.parse("123")` returns `Result` | no silent `NaN` or partial parse |
| Http module | `Http.get(url)`, `Http.post(url, body)` | IIFE wrapping `fetch` in `Promise<Result>` |
| Promise module | `Promise.all`, `Promise.allSettled` | `allSettled` returns `Array<Result<T, Error>>` |
| Array.mapResult | `Array.mapResult(arr, fn)` | map fallible fn, short-circuit on Err |
| Array.filterMap | `Array.filterMap(arr, fn)` | map + filter in one pass (fn returns Option) |
| Array.partition | `Array.partition(arr, fn)` | split array into (matching, non-matching) |
| Array.flatten | `Array.flatten(arr)` | flatten nested arrays |
| Array.intersperse | `Array.intersperse(arr, sep)` | insert separator between elements |
| Bool.guard | `use <- Bool.guard(cond, fallback)` | linear early-return via `use` (Gleam-style) |
| Option.guard | `use val <- Option.guard(opt, fallback)` | unwrap-or-bail via `use` |
| Result.guard | `use val <- Result.guard(res, onErr)` | unwrap-or-bail via `use` |
| Number separators | `1_000_000`, `3.141_592`, `0xFF_FF` | underscores stripped in output |
| Mock data | `mock<User>`, `mock<User>(name: "Alice")` | object literal with generated test data |

### What's Removed (compile errors)

| Banned | Why | Alternative |
|--------|-----|-------------|
| `enum` | Broken TS feature, emits runtime code | Use `type` with `\|` variants |
| `any` | Type safety escape hatch | Use `unknown` + narrowing |
| `as` (type assertion) | Lies to the compiler | Use type guards or `match` |
| `let` | Mutable bindings | `const` only |
| `null` | Billion dollar mistake | `Option<T>` with `Some`/`None` |
| `undefined` | Two nothings is one too many | `Option<T>` with `Some`/`None` |
| `throw` | Untyped, invisible errors | Return `Result<T, E>` |
| `class` | OOP escape hatch | Use functions + types |
| `?.` | Optional chaining | `match`, `Option.map`, or `?` |
| `??` | Nullish coalescing | `Option.unwrapOr` |
| `? :` | Ternary | `match` |
| `x?: T` | Optional fields | `x: Option<T>` |
| `+` on strings | Silent coercion bugs | Template literals only (warning) |
| `void` | Not a real type, can't use in generics | Unit type `()` — a real value |
| `=>` in expressions | Two syntaxes for functions is one too many | `fn(x) expr` for anonymous functions; `->` is used for function types like `(T) -> U` |
| `function` | Verbose keyword | `fn` |
| `if`/`else` | Redundant control flow | `match` expression |
| `return` | Implicit returns — last expression is the value | Omit `return`; the last expression in a block is the return value |

---

## Syntax Examples

### Pipe Operator

```floe
// Default: piped value goes to first argument
users
  |> filter(.active)                   // filter(users, x => x.active)
  |> sortBy(.name)                     // sortBy(result, x => x.name)
  |> take(10)                          // take(result, 10)

// Need a different position? Use _ placeholder
"hello" |> String.padStart(_, 10)      // padStart("hello", 10)
42 |> wrap("[", _, "]")                // wrap("[", 42, "]")

// _ also works outside pipes — partial application
const addTen = add(10, _)              // (x) => add(10, x)
[1, 2, 3] |> map(multiply(_, 2))      // [2, 4, 6]

// Dot shorthand — .field creates an implicit closure
todos |> Array.filter(.id != id)       // filter(todos, x => x.id != id)
todos |> Array.map(.text)              // map(todos, x => x.text)

// Closures — arrow syntax for when you need a named param
todos |> Array.map((t) => Todo(..t, done: true))
items |> Array.reduce((acc, x) => acc + x.price, 0)

// tap — call a function for side effects, pass value through
orders
  |> Array.filter(.active)
  |> tap(Console.log)              // logs filtered orders, passes through
  |> Array.map(.total)

// Pipes in JSX
<ul>
  {users
    |> filter(.active)
    |> sortBy(.name)
    |> map((u) => <li key={u.id}>{u.name}</li>)
  }
</ul>
```

Pipe rules:

1. No `_` in the call → insert piped value as first arg: `a |> f(b, c)` → `f(a, b, c)`
2. Has `_` → replace `_` with piped value: `a |> f(b, _, c)` → `f(b, a, c)`
3. `_` outside a pipe → create partial function: `f(b, _, c)` → `(x) => f(b, x, c)`
4. Only ONE `_` allowed per call — compile error on `f(_, _)`
5. `match` as pipe target → `a |> match { ... }` desugars to `match a { ... }`

```floe
// Pipe into match — pipe the value directly into pattern matching
const label = product
    |> effectivePrice
    |> match {
        _ when _ < 10 -> "cheap",
        _ when _ < 100 -> "moderate",
        _ -> "expensive",
    }

// Equivalent to:
const price = product |> effectivePrice
const label = match price {
    _ when price < 10 -> "cheap",
    _ when price < 100 -> "moderate",
    _ -> "expensive",
}
```

### Match Expressions

```floe
// Match on union types — exhaustive
match route {
  Home          -> <HomePage />
  Profile(id)   -> <ProfilePage id={id} />
  Settings(tab) -> <SettingsPage tab={tab} />
}
// Add a new Route variant? Compiler flags every match that doesn't handle it.

// Match on Result
match fetchUser(id) {
  Ok(user)          -> <Profile user={user} />
  Err(NotFound)     -> <NotFoundPage />
  Err(Network(msg)) -> <ErrorBanner msg={msg} />
}

// Match with ranges
match response.status {
  200..299 -> Ok(response)
  401      -> Err(Unauthorized)
  404      -> Err(NotFound)
  s        -> Err(ServerError(s))
}

// Match with nested destructuring
match action {
  Click(el, { x, y })          -> handleClick(el, x, y)
  KeyPress("s", { ctrl: true }) -> save()
  KeyPress(key, _)              -> insertChar(key)
}

// String pattern matching with captures
match url {
  "/users/{id}"          -> fetchUser(id)
  "/users/{id}/posts"    -> fetchPosts(id)
  "/about"               -> aboutPage()
  _                      -> notFound()
}

// Inline match in JSX props
<Button
  disabled={match state { Submitting -> true, _ -> false }}
>
  {match state { Submitting -> "Sending...", _ -> "Send" }}
</Button>

// Optional rendering — Option<JSX.Element> works as a JSX child
// None compiles to undefined, which React ignores
{user.nickname |> Option.map((nick) => <Badge>{nick}</Badge>)}

```

Match uses `->` for arms (not `=>`), so it's visually distinct from closures.

### Array Pattern Matching

Match on array structure with head/tail destructuring:

```floe
match items {
    [] -> "empty",
    [only] -> `just one: ${only}`,
    [first, second] -> "exactly two",
    [first, ..rest] -> `first is ${first}, ${rest |> Array.length} more`,
}
```

| Pattern | Matches | Binds |
|---|---|---|
| `[]` | Empty array | Nothing |
| `[a]` | Exactly 1 element | `a` |
| `[a, b]` | Exactly 2 elements | `a`, `b` |
| `[first, ..rest]` | 1 or more elements | `first` (head), `rest` (tail array) |
| `[first, second, ..rest]` | 2 or more elements | `first`, `second`, `rest` |
| `[_, ..rest]` | 1 or more, ignore head | `rest` |

**Exhaustiveness:** `[]` + `[_, ..rest]` covers all cases (empty + non-empty). `[a]` alone is not exhaustive.

**Codegen:**

```floe
match items {
    [] -> "empty",
    [first, ..rest] -> first,
}
```

Emits:

```typescript
items.length === 0 ? "empty" : items.length >= 1 ? (() => { const first = items[0]; const rest = items.slice(1); return first; })() : (() => { throw new Error("non-exhaustive match"); })()
```

- Empty pattern `[]` checks `subject.length === 0`
- Exact patterns `[a, b]` check `subject.length === N`
- Rest patterns `[a, ..rest]` check `subject.length >= N` where N is the count of fixed elements
- Element bindings use index access: `subject[0]`, `subject[1]`
- Rest bindings use slice: `subject.slice(N)`

### Match Arm Guards

A `when` clause on a match arm adds a condition that must be true for the arm to match. Bindings from the pattern are in scope in the guard expression.

```floe
match user {
  User(age) when age > 18 -> "adult",
  User(age) -> "minor",
}

match request {
  Request(method, path) when method == "GET" -> handleGet(path),
  Request(method, path) when method == "POST" -> handlePost(path),
  _ -> notAllowed(),
}
```

**Exhaustiveness:** A guarded arm does not fully cover its pattern. The compiler treats guarded arms as partial matches, so a catch-all (`_`) or unguarded arm is still required for exhaustiveness.

**Codegen:** Guards become additional conditions in the emitted ternary chain:
- Without bindings: `pattern_condition && guard ? body : ...`
- With bindings: pattern check, then IIFE with `if (guard) { return body; }` to fall through on guard failure

### The `?` Operator (Result/Option Unwrap)

```floe
// On Result<T, E> — gives you T, or returns Err(E) from function
fn loadProfile(id: UserId): Result<Profile, AppError> {
  const user  = fetchUser(id)?
  const posts = fetchPosts(user.id)?
  const stats = fetchStats(user.id)?
  Ok({ user, posts, stats })
}

// On Option<T> — gives you T, or returns None from function
fn getDisplayName(userId: UserId): Option<string> {
  const user     = findUser(userId)?
  const nickname = user.nickname?
  Some(toUpperCase(nickname))
}

// In a pipe
const name = fetchUser(id)? |> getName |> toUpperCase

// Compiler enforces: function must return Result or Option to use ?
fn greet(): string {
  const user = fetchUser(id)?  // COMPILE ERROR: can't use ? in non-Result function
}
```

Compiles to simple early returns:

```typescript
// fetchUser(id)?  becomes:
const _r0 = fetchUser(id)
if (!_r0.ok) return _r0
const user = _r0.value
```

### The `collect` Block (Error Accumulation)

Inside a `collect {}` block, `?` does NOT short-circuit. Each `?` that hits an Err records the error and continues. If any failed, the block returns `Err(Array<E>)` with all collected errors. If all succeeded, returns `Ok(last_expression)`.

```floe
fn validateForm(input: FormInput) -> Result<ValidForm, Array<ValidationError>> {
    collect {
        const name = input.name |> validateName?
        const email = input.email |> validateEmail?
        const age = input.age |> validateAge?

        ValidForm(name, email, age)
    }
}
```

Compiles to an IIFE with error accumulation:

```typescript
(() => {
    const __errors: Array<ValidationError> = [];
    const _r0 = validateName(input.name);
    if (!_r0.ok) __errors.push(_r0.error);
    const name = _r0.ok ? _r0.value : undefined as any;
    const _r1 = validateEmail(input.email);
    if (!_r1.ok) __errors.push(_r1.error);
    const email = _r1.ok ? _r1.value : undefined as any;
    if (__errors.length > 0) return { ok: false, error: __errors };
    return { ok: true, value: { name, email } };
})()
```

The return type of `collect { ... }` is `Result<T, Array<E>>` where:
- `T` is the type of the last expression in the block
- `E` is the error type from `?` operations in the block

#### Full Result helpers

| Function | Type | Description |
|---|---|---|
| `map` | `Result<T, E>, (T) -> U -> Result<U, E>` | Transform Ok value |
| `mapErr` | `Result<T, E>, (E) -> F -> Result<T, F>` | Transform Err value |
| `flatMap` | `Result<T, E>, (T) -> Result<U, E> -> Result<U, E>` | Chain result-returning ops |
| `filter` | `Result<T, E>, (T) -> boolean, E -> Result<T, E>` | Keep Ok if predicate passes |
| `unwrapOr` | `Result<T, E>, T -> T` | Extract Ok or default |
| `or` | `Result<T, E>, Result<T, E> -> Result<T, E>` | First Ok, else second |
| `values` | `Array<Result<T, E>> -> Array<T>` | Extract all Ok values |
| `partition` | `Array<Result<T, E>> -> (Array<T>, Array<E>)` | Split into Ok and Err arrays |
| `mapOr` | `Result<T, E>, U, (T) -> U -> U` | Map + default |
| `flatten` | `Result<Result<T, E>, E> -> Result<T, E>` | Unwrap nested |
| `zip` | `Result<T, E>, Result<U, E> -> Result<(T, U), E>` | Combine two Results |
| `inspect` | `Result<T, E>, (T) -> () -> Result<T, E>` | Side-effect on Ok |
| `inspectErr` | `Result<T, E>, (E) -> () -> Result<T, E>` | Side-effect on Err |
| `isOk` | `Result<T, E> -> boolean` | Check Ok |
| `isErr` | `Result<T, E> -> boolean` | Check Err |
| `toOption` | `Result<T, E> -> Option<T>` | Drop error |
| `all` | `Array<Result<T, E>> -> Result<Array<T>, E>` | Collect, fail on first Err |
| `any` | `Array<Result<T, E>> -> Result<T, Array<E>>` | First Ok, or all Errs |
| `guard` | `Result<T, E>, (E) -> U, (T) -> U -> U` | Bail with fallback on Err, continue with Ok (for `use`) |
| `orElse` | `Result<T, E>, (E) -> Result<T, F> -> Result<T, F>` | Lazy fallback chain |

### Option<T> - No Null, No Undefined

```floe
type User {
  name: string                  // always present
  nickname: Option<string>      // might not exist
  avatar: Option<Url>           // might not exist
}

// Must handle the None case
const display = match user.nickname {
  Some(nick) -> nick
  None       -> user.name
}

// Or use Option helpers in pipes
const display = user.nickname |> Option.unwrapOr(user.name)

// Transform inside without unwrapping
const upper: Option<string> = user.nickname |> Option.map((n) => toUpperCase(n))

// Chain
const avatar = user.nickname |> Option.flatMap((n) => findAvatar(n))

// Filter
const longNick = user.nickname |> Option.filter((n) => String.length(n) > 3)

// Zip two Options
const pair = Option.zip(firstName, lastName)  // Option<(string, string)>

// Collect an array of Options
const allNames = Option.all([Some("a"), Some("b")])  // Some(["a", "b"])

// Handle { data, error } pattern (Supabase, TanStack Query, etc.)
error |> Option.toErr?              // bail if error is Some
const rows = data |> Option.unwrapOr([])
```

#### Full Option helpers

| Function | Type | Description |
|---|---|---|
| `map` | `Option<T>, (T) -> U -> Option<U>` | Transform inner value |
| `flatMap` | `Option<T>, (T) -> Option<U> -> Option<U>` | Chain option-returning ops |
| `filter` | `Option<T>, (T) -> boolean -> Option<T>` | Keep Some if predicate passes |
| `unwrapOr` | `Option<T>, T -> T` | Extract or default |
| `or` | `Option<T>, Option<T> -> Option<T>` | First Some, else second |
| `values` | `Array<Option<T>> -> Array<T>` | Extract all Some values |
| `mapOr` | `Option<T>, U, (T) -> U -> U` | Map + default |
| `flatten` | `Option<Option<T>> -> Option<T>` | Unwrap nested |
| `zip` | `Option<T>, Option<U> -> Option<(T, U)>` | Combine two Options |
| `inspect` | `Option<T>, (T) -> () -> Option<T>` | Side-effect |
| `isSome` | `Option<T> -> boolean` | Check present |
| `isNone` | `Option<T> -> boolean` | Check absent |
| `toResult` | `Option<T>, E -> Result<T, E>` | Convert to Result |
| `toErr` | `Option<E> -> Result<(), E>` | Convert to Err (for `{ data, error }`) |
| `all` | `Array<Option<T>> -> Option<Array<T>>` | Collect, None if any missing |
| `any` | `Array<Option<T>> -> Option<T>` | First Some |
| `guard` | `Option<T>, U, (T) -> U -> U` | Bail with fallback on None, continue with Some (for `use`) |
| `orElse` | `Option<T>, () -> Option<T> -> Option<T>` | Lazy fallback chain |

### Settable<T> - Three-State Fields for Partial Updates

For TypeScript interop where a field is both optional (`?`) and nullable (`| null`), `Settable<T>` captures three states:

```floe
type UpdateUser {
  name: Settable<string> = Unchanged,
  email: Settable<string> = Unchanged,
}

// Value(x) — set the field
// Clear    — send null (clear the field)
// Unchanged — omit the key entirely (don't touch it)
const update = UpdateUser(name: Value("Ryan"), email: Clear)
// Compiles to: { name: "Ryan", email: null }

const partial = UpdateUser(name: Value("Ryan"))
// Compiles to: { name: "Ryan" }
```

`Settable<T>` compiles to `T | null | undefined` in TypeScript. `Value(x)` desugars to `x`, `Clear` desugars to `null`, and `Unchanged` causes the field to be omitted from the object literal.

### Type System — Just `type`, No `enum`

`type` does everything. No `|` = record. Has `|` = union. Unions nest infinitely.

```floe
// Record type
type User {
  id: UserId
  name: string
  email: Email
}

// Record type composition with spread
type BaseProps {
  className: string,
  disabled: boolean,
}

type ButtonProps {
  ...BaseProps,
  onClick: () -> (),
  label: string,
}
// ButtonProps has: className, disabled, onClick, label

// Multiple spreads
type A { x: number }
type B { y: string }
type C { ...A, ...B, z: boolean }
// C has: x, y, z

// Spread with generic types and typeof (npm interop)
type CardProps {
    ...VariantProps<typeof cardVariants>,
    className: string,
}
// Compiles to: type CardProps = VariantProps<typeof cardVariants> & { className: string };

// Simple union type (has |)
type Route {
  | Home
  | Profile { id: string }
  | Settings { tab: string }
  | NotFound
}

// String literal union (for npm interop)
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

// Union types can contain other union types — nest as deep as you want
type NetworkError {
  | Timeout { ms: number }
  | DnsFailure { host: string }
  | ConnectionRefused { port: number }
}

type ValidationError {
  | Required { field: string }
  | InvalidFormat { field: string, expected: string }
  | TooLong { field: string, max: number }
}

type AuthError {
  | InvalidCredentials
  | TokenExpired { expiredAt: Date }
  | InsufficientRole { required: Role, actual: Role }
}

// Parent union containing sub-unions
type ApiError {
  | Network(NetworkError)
  | Validation(ValidationError)
  | Auth(AuthError)
  | NotFound
  | ServerError { status: number, body: string }
}

// Go deeper — a full app error hierarchy
type HttpError {
  | Network(NetworkError)
  | Status { code: number, body: string }
  | Decode(JsonError)
}

type UserError {
  | Http(HttpError)
  | NotFound { id: UserId }
  | Banned { reason: string }
}

type PaymentError {
  | Http(HttpError)
  | InsufficientFunds { needed: number, available: number }
  | CardDeclined { reason: string }
}

type AppError {
  | User(UserError)
  | Payment(PaymentError)
  | Auth(AuthError)
}
```

### Multi-Depth Matching

Match at any depth in one expression. Mix shallow and deep arms freely.

```floe
// Level 1 — broad categories
match error {
  Network(_)        -> "Connection problem"
  Validation(_)     -> "Invalid input"
  Auth(_)           -> "Access denied"
  NotFound          -> "Not found"
  ServerError(_, _) -> "Server broke"
}

// Level 2 — drill into sub-variants
match error {
  Network(Timeout(ms))         -> `Timed out after ${ms}ms`
  Network(DnsFailure(host))    -> `Can't resolve ${host}`
  Network(ConnectionRefused(_)) -> "Server not running"
  Auth(TokenExpired(_))        -> "Session expired, please log in again"
  Auth(_)                      -> "Access denied"
  Validation(e)                -> describeValidation(e)
  NotFound                     -> "Not found"
  ServerError(s, _)            -> `Server error ${s}`
}

// Level 3+ — deep nested matching
match appError {
  User(Http(Network(Timeout(ms)))) ->
    `User fetch timed out after ${ms}ms`

  User(Http(Decode(MissingField(f)))) ->
    `API response missing field: ${f}`

  Payment(InsufficientFunds(needed, have)) ->
    `Need $${needed} but only have $${have}`

  Payment(CardDeclined(reason)) ->
    `Card declined: ${reason}`

  Auth(TokenExpired(_)) ->
    refreshToken()

  _ -> showGenericError(appError)
}

// Pass sub-types to specialist functions
fn handleError(err: ApiError) {
  match err {
    Network(netErr) -> retryNetwork(netErr)   // pass NetworkError to specialist
    Auth(authErr)   -> handleAuth(authErr)     // pass AuthError to specialist
    _               -> showGenericError(err)
  }
}

// Sub-type IS the parent type
const err: ApiError = Network(Timeout(3000))
// Network(Timeout(3000)) is both a NetworkError and an ApiError

```

The compiler generates discrimination tags in the emitted TypeScript — you never write `type: "blahblah"` manually. Exhaustiveness checking works at every nesting level.

### Newtypes (Single-Variant Wrappers)

```floe
type UserId(string)
type Email(string)

const id = UserId("abc123")
sendEmail(id, "hello")  // COMPILE ERROR: UserId is not Email
```

### Opaque Types

```floe
// auth/password.fl
opaque type HashedPassword(string)

export fn hash(raw: string): HashedPassword {
  bcrypt(raw)  // only this module can create one
}

export fn verify(raw: string, hashed: HashedPassword): boolean {
  bcryptCompare(raw, hashed)  // only this module can read it
}

// other_file.fl
const pw: HashedPassword = hash("secret")
// pw + "abc"   COMPILE ERROR — it's not a string to you

```

### Callback Flattening (`use`)

The `use` keyword flattens nested callbacks, inspired by Gleam. The rest of the block becomes the callback body:

```floe
// Without use
File.open(path, fn(file)
    File.readAll(file, fn(contents)
        contents |> String.toUpper
    )
)

// With use
use file <- File.open(path)
use contents <- File.readAll(file)
contents |> String.toUpper
```

**Syntax:** `use <binding> <- <call>` — desugars to `<call>(fn(<binding>) { rest of block })`

Zero-binding form for callbacks that don't pass a value:

```floe
use <- Timer.delay(1000)
Console.log("done")
// Desugars to: Timer.delay(1000, fn() { Console.log("done") })
```

**Codegen:** Pure syntactic sugar. `use x <- f(a)` compiles to `f(a, (x) => { ... })`. No runtime cost.

#### Guard Pattern (Early Return via `use`)

The `guard` stdlib functions combine with `use` to give linear, non-nested early-return flow without `if` or `return`:

```floe
// Bool.guard — bail if condition is false
export fn AdminPage(state: AuthState) -> JSX.Element {
    use <- Bool.guard(state.isAdmin, <Forbidden />)
    use <- Bool.guard(state.isVerified, <VerifyPrompt />)

    <AdminPanel />
}

// Option.guard — bail if None, continue with unwrapped value
use user <- Option.guard(maybeUser, <LoginPage />)
<ProfilePage user={user} />

// Result.guard — bail if Err, continue with Ok value
use data <- Result.guard(fetchResult, (e) => <ErrorPage error={e} />)
<Dashboard data={data} />
```

Under the hood these are just functions — no new syntax:

| Function | Type | Description |
|---|---|---|
| `Bool.guard` | `boolean, T, () -> T -> T` | Continue if true, bail with fallback if false |
| `Option.guard` | `Option<T>, U, (T) -> U -> U` | Continue with unwrapped T if Some, bail if None |
| `Result.guard` | `Result<T, E>, (E) -> U, (T) -> U -> U` | Continue with T if Ok, bail with onErr if Err |

Inspired by Gleam's `bool.guard`. The key insight: `use` isn't just for callbacks — it's the language's early return mechanism.

### Tuples

Anonymous lightweight product types. Use parenthesized syntax for types, construction, destructuring, and pattern matching.

```floe
// Type annotation
const point: (number, number) = (10, 20)
const entry: (string, number, boolean) = ("key", 42, true)

// Construction
const pair = (1, 2)

// Destructuring
const (x, y) = pair

// Function signatures
fn divmod(a: number, b: number) -> (number, number) {
  (a / b, a % b)
}

// Pattern matching
match divmod(10, 3) {
  (_, 0) -> "divides evenly",
  (q, r) -> `${q} remainder ${r}`,
}
```

**Codegen:** Tuples compile to plain TypeScript arrays/tuples:
- `(10, 20)` -> `[10, 20] as const`
- `(number, number)` -> `readonly [number, number]`
- `const (x, y) = pair` -> `const [x, y] = pair`

### Constructors, Named Arguments, and Defaults

Records and functions use the same call syntax: `Name(args)` with optional labels. No `new`, no `{ }` for construction.

```floe
// --- Record Construction ---

type User {
  id: UserId
  name: string
  email: Email
  nickname: Option<string>
}

// Positional — args in field order
const user = User(UserId("1"), "Ryan", Email("r@test.com"), None)

// Named — any order, self-documenting
const user = User(
  name: "Ryan",
  id: UserId("1"),
  email: Email("r@test.com"),
  nickname: None,
)

// Update with spread — ..existing then override
const updated = User(..user, nickname: Some("Ry"))
const updated = User(..user,
  name: "Ryan Updated",
  nickname: Some("Ry"),
)
// Compiles to: { ...user, name: "Ryan Updated", nickname: "Ry" }

// --- Named Arguments on Functions ---

fn createUser(name: string, email: Email, role: Role): Result<User, ApiError> {
  // ...
}

// Call positionally
createUser("Ryan", Email("r@test.com"), Admin)

// Or with labels — great when args are same type
createUser(name: "Ryan", email: Email("r@test.com"), role: Admin)

// Mix: positional first, then named
createUser("Ryan", role: Admin, email: Email("r@test.com"))

// Call rules (enforced by the checker):
//   1. Positional must precede named
//   2. Named may appear in any order — reordered to match declaration
//   3. Every required param must be provided (positional or named)
//   4. No slot may be covered twice
//   5. Defaulted params must be passed by name, not positionally
//      — prevents silent slot-shifts when a default is skipped

// --- Default Values ---

// On record types
type Config {
  baseUrl: string                    // required — no default
  timeout: number = 5000             // default value
  retries: number = 3                // default value
  headers: Option<Headers> = None    // default value
}

// Only specify what you need
const config = Config(baseUrl: "https://api.example.com")
// timeout=5000, retries=3, headers=None — all defaulted

const config = Config(baseUrl: "https://api.example.com", timeout: 10000)
// timeout overridden, rest defaulted

// On functions
fn fetchUsers(
  page: number = 1,
  limit: number = 20,
  sort: SortOrder = Ascending,
): Result<Array<User>, ApiError> {
  // page, limit, sort are always concrete values — never Option, never undefined
}

fetchUsers()                                     // all defaults
fetchUsers(page: 3)                              // override one
fetchUsers(limit: 50, sort: Descending)          // override two

// On React component props
type ButtonProps {
  label: string                      // required
  onClick: () -> ()                  // required
  variant: Variant = Primary         // default
  size: Size = Medium                // default
  disabled: boolean = false             // default
  loading: boolean = false              // default
  icon: Option<Icon> = None          // default
}

export fn Button(props: ButtonProps) {
  <button>{props.label}</button>
}

// .fl — only specify what matters
<Button label="Save" onClick={handleSave} />
<Button label="Delete" onClick={handleDelete} variant={Danger} icon={Some(TrashIcon)} />
```

Default value rules:

1. Defaults must be compile-time constants or constructors (no function calls)
2. Defaulted fields must use named args when called (not positional) — prevents ambiguity
3. Required fields come first in the type definition — compiler error otherwise
4. The type is always concrete — `variant` is `Variant`, not `Option<Variant>`. It's `Primary` if you don't specify it.

### For Blocks — Grouping Functions Under a Type

`for` blocks group functions under a type. `self` is an explicit first parameter whose type is inferred from the block. No magic, no implicit context — `self` is just a named parameter.

Two syntactic forms are supported:

**Block form** — group multiple functions:

```floe
type User { name: string, age: number, active: boolean }

for User {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }

  fn isAdult(self) -> boolean {
    self.age >= 18
  }

  fn greet(self, greeting: string) -> string {
    `${greeting}, ${self.name}!`
  }
}
```

Export is per-function — `export` goes before `fn` inside the block.

```floe
// Works with generic types too
for Array<User> {
  fn adults(self) -> Array<User> {
    self |> Array.filter(.age >= 18)
  }
}

// Use in pipes — self is the first argument
user |> display             // display(user)
user |> greet("Hello")      // greet(user, "Hello")
users |> adults             // adults(users)
```

### Importing For-Block Functions

When for-block functions are in a different file from the type definition, import them with `import { for Type }`:

```floe
import { for User } from "./user-helpers"
import { for Array, for Map } from "./todo"

// Can be mixed with regular imports
import { Todo, Filter, for Array, for string } from "./todo"
```

For generic types, use the base type only — no type params in imports. `import { for Array }` brings ALL `for Array<T>` extensions from that file.

Same-file rule unchanged: importing a type still auto-imports its for-block functions from that file.

For block rules:

1. `self` is the explicit first parameter — type inferred from the `for` block
2. No `this`, no implicit context
3. Multiple `for` blocks per type allowed, even across files
4. Importing a type imports same-file `for` blocks automatically
5. Cross-file `for` blocks use `import { for Type }` syntax
6. Compiles to standalone functions with `self` explicitly typed
7. Only block syntax is supported (`for Type { ... }`)

### Traits — Type-Directed Behavioral Contracts

Traits define behavioral contracts that types can implement via `for` blocks. They are compile-time only — erased entirely in codegen.

```floe
// Define a trait with required method signatures
trait Display {
  fn display(self) -> string
}

// Traits can have default implementations
trait Eq {
  fn eq(self, other: Self) -> boolean
  fn neq(self, other: Self) -> boolean {
    !(self |> eq(other))
  }
}

// Implement a trait for a type using `for Type: Trait`
for User: Display {
  fn display(self) -> string {
    `${self.name} (${self.age})`
  }
}

for User: Eq {
  fn eq(self, other: User) -> boolean {
    self.id == other.id
  }
  // neq is inherited from the default implementation
}
```

Trait rules:

1. Traits contain method signatures with optional default bodies
2. `for Type: Trait { ... }` implements a trait — all required methods must be provided
3. Methods with default bodies are optional — implementors inherit them unless overridden
4. Traits are erased at compile time — `for Type: Trait` emits the same code as `for Type`
5. No orphan rules — scoping via imports handles conflicts
6. No associated types — generics + structural typing cover those cases
7. No trait objects / dynamic dispatch — traits are a static checking tool

### Deriving Traits

Record types can auto-derive trait implementations with `deriving`:

```floe
type User {
  id: string,
  name: string,
  email: string,
} deriving (Display)
```

This generates the same code as a handwritten `for` block with no runtime cost.

**Note:** `Eq` is not derivable — structural equality is built-in for all types via `==` (emits `__floeEq` deep comparison). Writing `deriving (Eq)` is a compile error.

**Derivable traits:**

| Trait | Generated implementation |
|---|---|
| `Display` | String representation: `fn display(self) -> string` producing `TypeName(field1: val1, field2: val2)` |

**Codegen output** for `deriving (Display)`:

```typescript
function display(self: User): string {
  return `User(id: ${self.id}, name: ${self.name}, email: ${self.email})`;
}
```

Deriving rules:

1. `deriving` only works on record types — compile error on unions, aliases, or string literal unions
2. A handwritten `for` block overrides a derived implementation
3. Only `Display` is derivable — attempting to derive anything else (including `Eq`) is a compile error

### Inline Test Blocks

`test` blocks let you write tests co-located with the code they test. They are type-checked but stripped from production output.

```floe
fn add(a: number, b: number) -> number { a + b }

test "addition" {
  assert add(1, 2) == 3
  assert add(-1, 1) == 0
}

test "edge cases" {
  assert add(0, 0) == 0
}
```

Test block rules:

1. `test` is a contextual keyword - it starts a test block only when followed by a string literal
2. `assert` is a keyword that is only valid inside test blocks
3. `assert` expressions must evaluate to `boolean`
4. `floe check` and `floe build` type-check test blocks but strip them from output
5. `floe test` discovers and runs all test blocks, emitting pass/fail results
6. Test blocks cannot be exported

Compiles to (test mode only):

```typescript
// test: addition
(function() {
  const __testName = "addition";
  let __passed = 0;
  let __failed = 0;
  try { if (!(add(1, 2) === 3)) { __failed++; } else { __passed++; } } catch (e) { __failed++; }
  try { if (!(add(-1, 1) === 0)) { __failed++; } else { __passed++; } } catch (e) { __failed++; }
  if (__failed > 0) { console.error(`FAIL ${__testName}: ${__passed} passed, ${__failed} failed`); process.exitCode = 1; }
  else { console.log(`PASS ${__testName}: ${__passed} passed`); }
})();
```

### Function Conventions

```floe
// Named/exported functions use `fn`
export fn TodoApp() -> JSX.Element { ... }
export fn fetchUser(id: UserId) -> Result<User, ApiError> { ... }

// Inline/anonymous uses arrow closures
todos |> Array.map((t) => t.name)
onClick={() => setCount(count + 1)}
items |> Array.reduce((acc, x) => acc + x.price, 0)

// Dot shorthand for simple field access
todos |> Array.filter(.done == false)
todos |> Array.map(.text)

// Named args and defaults
fn greet(name: string, greeting: string = "Hello") -> string {
  `${greeting}, ${name}!`
}
greet("Ryan")                    // "Hello, Ryan!"
greet("Ryan", greeting: "Hey")  // "Hey, Ryan!"

// COMPILE ERROR: const + closure — use fn instead
const double = (x) => x * 2        // ERROR: Use `fn double(x) -> ...`
fn double(x: number) -> number { x * 2 }  // correct
```

### Full Component Example

```floe
import { useState } from "react"

type Todo {
  id: string
  text: string
  done: boolean
}

type Tab {
  | Overview
  | Team
  | Analytics
}

export fn Dashboard(userId: UserId) -> JSX.Element {
  const (tab, setTab) = useState<Tab>(Overview)
  const user = useAsync(() => fetchUser(userId))

  <Layout>
    <Sidebar>
      <NavItem
        active={match tab { Overview -> true, _ -> false }}
        onClick={() => setTab(Overview)}>
        Overview
      </NavItem>
    </Sidebar>

    <Main>
      {match tab {
        Overview  -> <OverviewPanel user={user.state} />
        Team      -> <TeamPanel />
        Analytics -> <AnalyticsPanel userId={userId} />
      }}
    </Main>
  </Layout>
}

fn OverviewPanel(user: AsyncState<User, ApiError>) -> JSX.Element {
  match user {
    Idle         -> <EmptyState>Click to load</EmptyState>
    Loading      -> <Skeleton lines={6} />
    Failure(err) -> <ErrorCard>{describeError(err)}</ErrorCard>
    Success(u)   ->
      <div>
        <WelcomeBanner name={u.name} />
        <StatGrid>
          <Stat label="Role" value={roleLabel(u.role)} />
          <Stat label="Email" value={u.email} />
        </StatGrid>
      </div>
  }
}

fn describeError(err: ApiError) -> string {
  match err {
    Network(msg)    -> `Connection failed: ${msg}`
    NotFound        -> "User not found"
    Unauthorized    -> "Please log in"
    BadRequest(e)   -> e |> join(", ")
    ServerError(s)  -> `Server error (${s})`
  }
}
```

---

## Compiler Strictness Rules

These are enforced at compile time with clear error messages.

| Rule | Error | Fix |
|------|-------|-----|
| Exported functions must declare return types | `ERROR: missing return type` | Add `-> ReturnType` |
| `const name = (x) => ...` | `ERROR: use fn instead` | `fn name(x) -> T { ... }` |
| No unused variables | `ERROR: x is never used` | Remove or prefix with `_` |
| No unused imports | `ERROR: useRef is never used` | Remove the import |
| No implicit type widening | `ERROR: mixed array needs explicit type` | Add type annotation |
| No floating promises/results | `ERROR: unhandled Result` | Use `?`, `match`, or assign to `_` |
| No property access on unnarrowed unions | `ERROR: result is Result, not Ok` | `match` first |
| No mutation of function parameters | `ERROR: cannot mutate parameter` | Return new value with spread |
| Array index returns `Option<T>` | — | Must handle `None` case |
| No `==` between different types | `ERROR: cannot compare number with string` | Convert first |
| IO functions must return `Result` | `ERROR: fetch can fail — return Result` | Declare error type |
| Dead code after exhaustive match | `ERROR: unreachable code` | Remove dead code |
| String concat with `+` | `WARNING: use template literal` | Use `` `${x}` `` |
| Non-unit function body is empty or has no final expression | `ERROR: missing return value` | Add an expression as the last line of the block |
| Spread with overlapping keys | `WARNING: 'y' from 'a' is overwritten by 'b'` | Reorder or remove duplicate |
| `void` keyword | `ERROR: use () instead of void` | Replace with `()` |
| `todo` usage | `WARNING: placeholder that will panic at runtime` | Replace with actual implementation |

---

## `parse<T>` - Compiler Built-in for Runtime Type Validation

`parse<T>` is a compiler built-in that generates runtime type validators directly from Floe type definitions. No runtime library (e.g., Zod) is needed - the compiler inlines the validation code.

### Syntax

```floe
// Direct call
const user = parse<User>(json)

// Pipe usage (most common)
const user = json |> parse<User>?

// With array types
const items = data |> parse<Array<Product>>?

// With inline record types
const point = raw |> parse<{ x: number, y: number }>?
```

### Return Type

`parse<T>(value)` always returns `Result<T, Error>`. Use `?` to unwrap.

### Codegen

For `parse<{ name: string, age: number }>(json)`, the compiler emits:

```typescript
(() => {
  const __v = json;
  if (typeof __v !== "object" || __v === null) return { ok: false as const, error: new Error("expected object, got " + typeof __v) };
  if (typeof (__v as any).name !== "string") return { ok: false as const, error: new Error("field 'name': expected string, got " + typeof (__v as any).name) };
  if (typeof (__v as any).age !== "number") return { ok: false as const, error: new Error("field 'age': expected number, got " + typeof (__v as any).age) };
  return { ok: true as const, value: __v as { name: string; age: number } };
})()
```

### Supported Types

| Type | Validation emitted |
|---|---|
| `string` | `typeof x === "string"` |
| `number` | `typeof x === "number"` |
| `boolean` | `typeof x === "boolean"` |
| Record types | `typeof x === "object"` + recursive field checks |
| `Array<T>` | `Array.isArray(x)` + element validation loop |
| `Option<T>` | Allow `undefined` or validate inner type |
| Named types | `typeof x === "object"` (structural check) |

### AST Node

```
ExprKind::Parse { type_arg: TypeExpr, value: Box<Expr> }
```

In pipe context (`json |> parse<T>`), value is a `Placeholder` that gets substituted with the piped expression.

---

## `mock<T>` - Compiler Built-in for Test Data Generation

`mock<T>` is a compiler built-in that generates test data from Floe type definitions. No runtime library (e.g., faker.js) is needed - the compiler emits object literals directly. Always in sync with the type - add a field, mock data updates automatically.

### Syntax

```floe
// Basic - compiler generates realistic mock data from the type
const testUser = mock<User>
// { id: "mock-id-1", name: "mock-name-2", age: 3 }

// Override specific fields
const admin = mock<User>(role: Admin)
// Random everything else, but role is Admin

// Inline record types
const point = mock<{ x: number, y: number }>
// { x: 1, y: 2 }
```

### Return Type

`mock<T>` returns `T` directly (not wrapped in Result).

### Generation Rules

| Type | Generated Value |
|------|----------------|
| `string` | `"mock-fieldname-N"` (uses field name for context) |
| `number` | Sequential integers starting from 1 |
| `boolean` | Alternates true/false |
| `Array<T>` | Array with 1 mock element |
| Record types | Recursively mock each field |
| Unions | Pick first variant |
| `Option<T>` | `Some(mock<T>)` (the inner value, not undefined) |
| String literal unions | First variant string |
| Named/newtypes | Mock the inner type |

### Codegen

`mock<User>` where `type User { id: string, name: string, age: number }` emits:

```typescript
{ id: "mock-id-1", name: "mock-name-2", age: 3 }
```

`mock<User>(name: "Alice")` emits:

```typescript
{ id: "mock-id-1", name: "Alice", age: 2 }
```

### AST Node

```
ExprKind::Mock { type_arg: TypeExpr, overrides: Vec<Arg> }
```

Overrides are named arguments that replace the generated value for specific fields.

---

## Compiler Architecture (Rust)

### Crate Structure

```
floe/
├── crates/
│   ├── floe_lexer/        # Tokenizer
│   ├── floe_parser/       # Recursive descent parser → AST
│   ├── floe_checker/      # Type checker, exhaustiveness, newtypes, opaques
│   ├── floe_codegen/      # AST → .tsx emitter
│   ├── floe_lsp/          # Language server (tower-lsp)
│   └── floe_cli/          # CLI binary (floe)
├── runtime/                 # ZERO runtime — intentionally empty
├── tests/
│   ├── fixtures/            # .fl input files
│   └── snapshots/           # expected .tsx outputs
└── Cargo.toml
```

### Lexer (`floe_lexer`)

Key tokens beyond standard TypeScript:

| Token | Lexeme |
|-------|--------|
| `Pipe` | `\|>` |
| `PipeUnwrap` | `\|>?` (pipe then unwrap) |
| `Arrow` | `->` (match arms, return types, function types) |
| `FatArrow` | `=>` (closures) |
| `Question` | `?` (postfix, Result/Option unwrap) |
| `Underscore` | `_` (placeholder/partial application) |
| `PipePipe` | `\|\|` (boolean OR) |
| `Match` | `match` keyword |
| `Fn` | `fn` keyword |
| `Some` | `Some` keyword |
| `None` | `None` keyword |
| `Todo` | `todo` keyword |
| `Unreachable` | `unreachable` keyword |
| `Ok` | `Ok` keyword |
| `Err` | `Err` keyword |
| `When` | `when` keyword (match arm guard) |
| `Opaque` | `opaque` keyword |
| `For` | `for` keyword (for blocks) |
| `SelfKw` | `self` keyword (explicit receiver in for blocks) |
| `Trait` | `trait` keyword (trait declarations) |
| `Assert` | `assert` keyword (only valid inside test blocks) |
| `Collect` | `collect` keyword (error accumulation block) |

Number literals support underscore separators for readability: `1_000_000`, `3.141_592`, `0xFF_FF`. Underscores can appear between any two digits but not at the start, end, or adjacent to a decimal point. The lexer strips underscores before emitting the token value.

Banned tokens (immediate compile errors with helpful messages):

- `any` → "Use a concrete type or generic"
- `as` → "Use a type guard or match expression"
- `let` → "Use const — all bindings are immutable"
- `class` → "Use functions and types"
- `throw` → "Return a Result<T, E> instead"
- `null` → "Use Option<T> with Some/None"
- `undefined` → "Use Option<T> with Some/None"
- `enum` → "Use type with | variants"
- `void` → "Use the unit type () instead"
- `function` → "Use fn instead"
- `=>` after `fn(x)` → "Use fn(x) for closures — function types use -> like (T) -> U"

### Parser (`floe_parser`)

Handwritten recursive descent. Key AST nodes:

```rust
enum Expr {
    // Standard
    Literal(Literal),
    Identifier(String),
    BinaryOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Arg> },
    Lambda { params: Vec<Param>, body: Box<Expr> },  // (x) => expr
    DotShorthand(Box<Expr>),                          // .field or .field op expr
    Jsx(JsxElement),

    // Floe additions
    Pipe { left: Box<Expr>, right: Box<Expr> },
    Match { subject: Box<Expr>, arms: Vec<MatchArm> },
    Unwrap(Box<Expr>),         // the ? operator
    Collect(Vec<Item>),        // collect { ... } error accumulation
    Construct {                 // Type(field: value, ..spread)
        type_name: String,
        spread: Option<Box<Expr>>,
        args: Vec<Arg>,
    },
    Ok(Box<Expr>),
    Err(Box<Expr>),
    Some(Box<Expr>),
    None,
    Placeholder,               // _ in partial application
}

// Top-level items include ForBlock, TraitDecl, and TestBlock
// FunctionDecl includes optional type parameters for generic functions
// fn identity<T>(x: T) -> T { x } → type_params: vec!["T"]
// fn pair<A, B>(a: A, b: B) -> (A, B) { (a, b) } → type_params: vec!["A", "B"]
struct FunctionDecl {
    name: String,
    type_params: Vec<String>,  // <T, U> — empty for non-generic functions
    params: Vec<Param>,
    return_type: Option<TypeExpr>,
    body: Vec<Item>,
}

enum ItemKind {
    Import, Const, Function(FunctionDecl), TypeDecl,
    ForBlock {                 // for Type { fn f(self) ... }
        type_name: TypeExpr,
        trait_name: Option<String>,  // for Type: Trait { ... }
        functions: Vec<FunctionDecl>,
    },
    TraitDecl {                // trait Name { fn method(self) ... }
        name: String,
        methods: Vec<TraitMethod>,
    },
    TestBlock {                // test "name" { assert expr ... }
        name: String,
        body: Vec<TestStatement>,
    },
    Expr,
}

// Args can be positional, named, or placeholder
enum Arg {
    Positional(Expr),
    Named { label: String, value: Expr },
    Placeholder,
}

// Params support defaults
struct Param {
    name: String,
    type_ann: Type,
    default: Option<Expr>,     // = defaultValue
}

struct MatchArm {
    pattern: Pattern,
    guard: Option<Expr>,       // when condition
    body: Expr,
}

enum Pattern {
    Literal(Literal),
    Range { start: Literal, end: Literal },
    Variant { name: String, bindings: Vec<Pattern> },  // recursive — enables multi-depth matching
    Record { fields: Vec<(String, Pattern)> },
    StringPattern { segments: Vec<StringPatternSegment> },  // "/users/{id}" — regex-based matching
    Wildcard,
}

enum StringPatternSegment {
    Literal(String),   // static text: "/users/"
    Capture(String),   // variable binding: "id"
}
```

### Type Checker (`floe_checker`)

The heart of the compiler:

1. **Type inference** — Hindley-Milner with TypeScript-flavored annotations
2. **Newtype enforcement** — `UserId` and `Email` are distinct at compile time, erased at runtime
3. **Opaque enforcement** — only the defining module can construct/destructure opaque types
4. **Exhaustiveness checking** — every `match` must cover all variants (including ranges)
5. **Result/Option tracking** — `?` only allowed in functions returning `Result`/`Option`
6. **IO detection** — functions calling `fetch`, file IO, JSON parse etc must return `Result`
7. **Mutation detection** — parameters cannot be mutated
8. **Equality checking** — `==` only between same types
9. **Unused detection** — variables, imports must be used
10. **Dead code detection** — unreachable code after exhaustive returns

### npm / .d.ts Interop

Strategy: shell out to `tsc` for module resolution, parse `.d.ts` files, and wrap at the boundary.

Automatic conversions at import boundary:

- `T | null` → `Option<T>`
- `T | undefined` → `Option<T>`
- `T | null | undefined` → `Option<T>`
- External `any` → `unknown` (forces narrowing)
- npm imports are **untrusted by default** — calls are auto-wrapped in `Result<T, Error>`
- `trusted` modifier marks imports that are safe to call directly (no Result wrapping)
- Nullable/optional types at the boundary are converted to `Option<T>`

#### Default npm imports (untrusted)

npm imports are untrusted by default — calls are wrapped in `Result<T, Error>`. Async functions (returning `Promise<T>`) are wrapped in `Promise<Result<T, Error>>` — the caller uses `|> await` to unwrap the Promise:

```floe
import { parseYaml } from "yaml-lib"
import { fetchUser } from "api-client"

const data = parseYaml(input)          // Result<unknown, Error> — sync
const user = fetchUser(id) |> await    // Promise<Result<User, Error>> → Result<User, Error>
```

#### trusted imports

For npm functions known to be safe, mark them with `trusted`. The compiler allows direct calls without Result wrapping:

```floe
// Whole module:
import trusted { useState, useEffect } from "react"
useState(0)                  // direct call, no wrapping

// Per-function:
import { trusted capitalize, fetchUser } from "some-ts-lib"
capitalize("hello")          // string, direct call
fetchUser(id) |> await       // Promise<Result<User, Error>> → Result<User, Error>
```

#### Untrusted imports with Result and ?

Untrusted calls return `Result<T, Error>` automatically. Use `?` to unwrap:

```floe
// JSON.parse is stdlib — already returns Result
const result = JSON.parse(input)
// result: Result<T, ParseError>

// npm imports are untrusted by default:
import { parseYaml } from "yaml-lib"
const data = parseYaml(input)
// data: Result<unknown, Error>

// Async untrusted: returns Promise<Result<T, Error>>, use |> await to unwrap
import { fetchUser } from "api-client"
const user = fetchUser(id) |> await
// user: Result<User, Error>

// Compose with |> await? for concise async error handling.
// `async fn f() -> T` is sugar for `fn f() -> Promise<T>` — write the inner type:
async fn loadProfile(id: string) -> Result<Profile, Error> {
  const user = fetchUser(id) |> await?
  const posts = fetchPosts(user.id) |> await?
  Ok(Profile(user, posts))
}
```

#### Null boundary

```floe
import { findElement } from "some-dom-lib"
// .d.ts says: findElement(id: string): Element | null
// Floe sees: findElement(id: string) -> Option<Element>

match findElement("app") {
  Some(el) -> render(el),
  None -> panic("no #app element"),
}
```

#### Codegen

```typescript
// import { parseYaml } from "yaml-lib"; parseYaml(input) → (untrusted, auto-wrapped)
(() => { try { return { ok: true as const, value: parseYaml(input) }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()
```

### Ambient Types (Browser/Runtime Globals)

Floe loads ambient types (globals like `window`, `document`, `process`, `fetch`) from the project's `tsconfig.json` rather than hardcoding them.

#### How it works

1. **`compilerOptions.lib`** determines which TS lib files to load:
   - `["ES2020", "DOM"]` → browser project (gets `window`, `document`, `navigator`, etc.)
   - `["ESNext"]` → server project (gets `Date`, `Promise`, `Map`, etc. but no DOM)
   - Lib files are loaded transitively via `/// <reference lib="..." />` directives

2. **`compilerOptions.types`** determines which `@types/*` packages to load:
   - `["node"]` → loads `@types/node` (gets `process`, `Buffer`, `__dirname`, etc.)
   - `["@cloudflare/workers-types"]` → loads Cloudflare Workers globals
   - When `types` is not set, all installed `@types/*` packages are auto-included (TS default)

3. **`declare global { ... }` blocks** are parsed to extract globals from packages like `@types/node` that use this pattern

4. **Fallback**: when no `tsconfig.json` is found, defaults to `lib.es5` + `lib.dom` (browser)

#### Implementation

The `interop/ambient.rs` module:
- Parses `tsconfig.json` for `compilerOptions.lib` and `compilerOptions.types`
- Locates TypeScript lib files in `node_modules/typescript/lib/`
- Follows `/// <reference lib="..." />` and `/// <reference path="..." />` directives
- Parses `declare var`, `declare function`, and `interface` definitions using `oxc_parser`
- Converts TS types to Floe types via the existing `wrap_boundary_type()` pipeline
- Registers globals in the checker's type environment, replacing hardcoded fallbacks

Constructor globals (`declare var Window: { prototype: Window; new(): Window }`) are filtered out to prevent shadowing interface definitions used for member access resolution.

### Code Generator (`floe_codegen`)

Emits clean, readable `.tsx`. Zero runtime imports.

| Floe | Emitted TypeScript |
|------------|-------------------|
| `a \|> f(b, c)` | `f(a, b, c)` |
| `a \|> f(b, _, c)` | `f(b, a, c)` |
| `add(10, _)` | `(x) => add(10, x)` |
| `(x) => x + 1` | `(x) => x + 1` |
| `.name` (in callback) | `(x) => x.name` |
| `.id != id` (in callback) | `(x) => x.id != id` |
| `Type.Variant` (qualified) | `{ tag: "Variant" }` (same as bare) |
| `fn f(x: T) -> U { ... }` | `function f(x: T): U { ... }` |
| `fn f<T>(x: T) -> T { ... }` | `function f<T>(x: T): T { ... }` |
| untrusted npm call (e.g. `parseYaml(input)`) | `(() => { try { return { ok: true, value: parseYaml(input) }; } catch (_e) { return { ok: false, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()` |
| `match x { A -> ..., B -> ... }` | `x.tag === "A" ? ... : x.tag === "B" ? ... : absurd(x)` |
| `match x { A(v) when v > 0 -> ... }` | `x.tag === "A" ? (() => { const v = x.value; if (v > 0) { return ...; } ... })()` |
| `match url { "/users/{id}" -> f(id) }` | `url.match(/^\/users\/([^/]+)$/) ? (() => { const _m = url.match(...); const id = _m![1]; return f(id); })() : ...` |
| `fetchUser(id)?` | `const _r = fetchUser(id); if (!_r.ok) return _r; const val = _r.value;` |
| `Ok(value)` | `{ ok: true, value }` |
| `Err(error)` | `{ ok: false, error }` |
| `Some(value)` | `value` |
| `None` | `undefined` |
| `type Method = "GET" \| "POST"` | `type Method = "GET" \| "POST"` (pass-through) |
| `match m { "GET" -> ... }` | `m === "GET" ? ...` (string comparison, no tag) |
| `User(name: "Ry", email: e)` | `{ name: "Ry", email: e }` (+ tag for unions) |
| `User(..user, name: "New")` | `{ ...user, name: "New" }` |
| `use x <- f(a)` | `f(a, (x) => { rest of block })` |
| `use <- f()` | `f(() => { rest of block })` |
| `f(name: "x", limit: 10)` | `f("x", 10)` (labels erased, reordered to match definition) |
| `f(x: number = 10)` | caller omits → compiler inserts `10` at call site |
| `a == b` (objects) | deep structural equality check |
| `()` (unit value) | `undefined` |
| `fn f() -> ()` (unit return) | `function f(): void` |
| `Array.sort(arr)` | `[...arr].sort((a, b) => a - b)` |
| `Array.any(arr, pred)` | `arr.some(pred)` |
| `Array.all(arr, pred)` | `arr.every(pred)` |
| `Array.sum(arr)` | `arr.reduce((a, b) => a + b, 0)` |
| `Array.join(arr, sep)` | `arr.join(sep)` |
| `Array.isEmpty(arr)` | `arr.length === 0` |
| `Array.chunk(arr, n)` | slice loop |
| `Array.unique(arr)` | `[...new Set(arr)]` |
| `Array.uniqueBy(arr, fn)` | filter with Set on key |
| `Array.indexOf(arr, val)` | `arr.indexOf(val)` → `Option<number>` |
| `Array.groupBy(arr, fn)` | `Object.groupBy(arr, fn)` |
| `Number.parse("123")` | strict parse returning `Result` |
| `type UserId(string)` | `string` (newtype erased) |
| `opaque type X(T)` | `T` (erased, access controlled at compile time) |
| `for User { fn display(self) -> string { ... } }` | `function display(self: User): string { ... }` |
| `trait Display { fn display(self) -> string }` | *(erased — no output)* |
| `for User: Display { fn display(self) -> string { ... } }` | `function display(self: User): string { ... }` (same as plain for block) |
| `test "name" { assert expr }` | stripped in build mode; self-executing test in test mode |

---

## Language Server (`floe_lsp`)

Built on `tower-lsp` (Rust LSP framework).

### Killer Feature: Pipe-Aware Autocomplete

When the user types `|>` after an expression, the LSP:
1. Resolves the type of the left-hand expression (e.g. `Option<string>`)
2. Searches all functions in scope (imported + local) where that type is a valid first argument
3. Ranks by relevance — functions from the matching module first (e.g. `Option.map`, `Option.unwrapOr`) then general functions that accept the type
4. Presents completions with type signatures

This gives `.method()` discoverability to a pipe-first language — the main DX complaint about Gleam, Elixir, and F# is that you have to KNOW which module has the function. Floe shows you.

Example: user types `Option<string>` value then `|>`
```

Suggestions:
  Option.map(fn)           Option<string> -> Option<U>
  Option.unwrapOr(default) Option<string> -> string
  Option.flatMap(fn)       Option<string> -> Option<U>
  Option.isSome            Option<string> -> boolean
  Option.isNone            Option<string> -> boolean
  toUpperCase              string -> string  (if unwrapped)
  ...

```

### Phase 1 — Diagnostics (weeks)
- Red squiggles for banned keywords
- Exhaustiveness warnings on incomplete match
- Type errors from the checker
- `?` .fl in non-Result/Option functions

### Phase 2 — Navigation (weeks)
- Go-to-definition (within .fl files)
- Hover for type info
- Find references

### Phase 3 — Intelligence (months)
- **Pipe-aware autocomplete** (the killer feature above)
- General autocompletion
- Auto-import suggestions
- npm type completions (via .d.ts)

### Phase 4 — Refactoring (ongoing)
- Extract to pipe
- Convert ternary to match (for migrating TS code)
- Add missing match arms

---

## Build Integration

### Vite Plugin (primary target)

```typescript
export default function floe(): Plugin {
  return {
    name: "floe",
    transform(code, id) {
      if (!id.endsWith(".fl")) return
      const tsx = compileZen(code)  // shell to floe or WASM
      return { code: tsx, map: null }
    }
  }
}
```

### CLI

```bash
floe build src/           # compile all .fl → .tsx
floe check src/           # type-check only, no output
floe fmt src/             # format .fl files in place
floe test src/            # run inline test blocks
floe watch src/           # watch mode
floe init                 # scaffold new project
floe migrate file.tsx     # attempt to convert .tsx to .fl
```

### Formatter (`floe fmt`)

The formatter enforces a canonical style. Key conventions:

- **Blank line before final expression:** In multi-statement blocks (2+ statements/expressions), the formatter inserts a blank line before the last expression (the implicit return value). Single-expression bodies are unaffected.

```floe
// Multi-statement: blank line before final expression
fn loadProfile(id: string) -> Result<Profile, ApiError> {
    const user = fetchUser(id)?
    const posts = fetchPosts(user.id)?
    const stats = computeStats(posts)

    Profile(user, posts, stats)
}

// Single expression: no blank line
fn add(a: number, b: number) -> number {
    a + b
}
```

This applies to `fn` bodies, `for`-block functions, match arms with block bodies, and closures with block bodies.

---

## Implementation Roadmap

### Phase 1: Proof of concept (2-4 weeks)

- [ ] Lexer with pipe, match, `?` tokens and banned keyword errors
- [ ] Parser for: const, fn, (x) => closures, .field shorthand, pipes, basic expressions
- [ ] Parser: `Type(field: value)` constructor syntax and `..spread`
- [ ] Parser: named arguments at call sites
- [ ] Parser: default values on function params and record fields
- [ ] Codegen: pipe lowering (`a |> f(b)` → `f(a, b)`)
- [ ] Codegen: `_` placeholder lowering
- [ ] Codegen: constructor → object literal, spread → `{ ...x }`
- [ ] CLI: `floe build` on a single file
- [ ] One working example: `.fl` file → valid `.tsx`

### Phase 2: Match + Result + Option (2-3 weeks)

- [ ] Match expression parsing with `->` arms
- [ ] Pattern matching: literals, variants, ranges, wildcards, nested variants
- [ ] Multi-depth matching: `Network(Timeout(ms)) -> ...`
- [ ] Result type (Ok/Err) built-in
- [ ] Option type (Some/None) built-in
- [ ] `?` operator parsing and codegen
- [ ] Exhaustiveness checking (including nested union depth)
- [ ] Codegen: match → if/else chains with auto-generated tags, `?` → early returns

### Phase 3: JSX + React (2-3 weeks)

- [ ] JSX parsing
- [ ] Pipes inside JSX expressions
- [ ] Match expressions inside JSX (inline and block)
- [ ] Component function detection

### Phase 4: Type System (4-8 weeks)

- [ ] Basic type inference
- [ ] Newtypes (compile-time only, erased in output)
- [ ] Opaque types (module-scoped construction)
- [ ] Union type exhaustiveness in match
- [ ] `?` only in Result/Option-returning functions
- [ ] IO detection (fetch etc. must return Result)
- [ ] Parameter mutation detection
- [ ] Same-type equality enforcement
- [ ] Array index returns Option
- [ ] .d.ts ingestion for npm interop (via tsc)
- [ ] Boundary auto-wrapping (null → Option, any → unknown)

### Phase 5: LSP + DX (2-4 weeks)

- [ ] LSP with diagnostics
- [ ] Pipe-aware autocomplete (the killer feature)
- [ ] VSCode extension (syntax highlighting + LSP client)
- [ ] Vite plugin
- [ ] Source maps (.fl → .tsx)

### Phase 6: Polish (ongoing)

- [ ] Elm/Gleam quality error messages
- [ ] LSP go-to-definition, hover, find references
- [ ] Playground (WASM compiler in browser)
- [ ] Documentation site
- [ ] `floe migrate` for converting .tsx → .fl

---

## JS/TS Footgun Eliminations

Beyond the banned keywords, Floe eliminates several categories of subtle runtime bugs that TypeScript allows.

### Structural Equality (`==` on objects)

In JS, `{a: 1} === {a: 1}` is `false` because objects compare by reference. Floe uses structural (deep) equality by default.

```floe
const a = User(name: "Ryan", email: Email("r@test.com"))
const b = User(name: "Ryan", email: Email("r@test.com"))

a == b  // true — compares fields, not references
```

This is safe because Floe has no `class` (no identity semantics) and all bindings are `const` (immutable). Consistent with Gleam, OCaml, and Elixir.

**Codegen:** `==` on objects compiles to a deep structural comparison in the emitted TypeScript.

### Unit Type `()` (replaces `void`)

TypeScript's `void` is not a real type — you can't use it in generics like `Result<void, Error>`. Floe uses the unit type `()`, which is a real value.

```floe
// Functions with no meaningful return value
fn log(msg: string) -> () {
  console.log(msg)
}

// Works naturally in generics
fn deleteUser(id: UserId) -> Result<(), ApiError> {
  // ...
  Ok(())
}

// Callbacks
type ButtonProps {
  onClick: () -> ()
}
```

**Codegen:** `()` compiles to `undefined` in value positions and `void` in type positions in the emitted TypeScript.

### Immutable Array Sort

JS `Array.sort()` mutates in place AND sorts lexicographically by default (`[10, 2, 1].sort()` gives `[1, 10, 2]`).

Floe's sort:
- Returns a new sorted array (no mutation)
- Sorts numerically by default for number arrays
- Requires an explicit comparator for non-primitive types

```floe
const nums = [10, 2, 1]
const sorted = nums |> Array.sort              // [1, 2, 10] — new array, numeric default
nums                                            // [10, 2, 1] — unchanged

const users = [u1, u2] |> Array.sortBy(.name)  // explicit comparator
```

**Codegen:** compiles to `[...arr].sort((a, b) => a - b)` for numbers, `[...arr].sort(comparator)` for custom.

### Implicit Returns

Floe uses implicit returns — the last expression in a block is the return value. The `return` keyword is banned.

```floe
fn getName(user: User) -> string {
  user.name    // this is the return value
}

fn log(msg: string) -> () {
  Console.log(msg)    // unit functions — last expression is discarded
}

// COMPILE ERROR: empty non-unit function body
fn broken(user: User) -> string {
}
```

### Safe Iteration

JS `for...in` iterates over inherited prototype keys. Floe loop constructs only iterate own values.

**Codegen:** compiles to `Object.entries()` or index-based iteration, never `for...in`.

### Strict Numeric Parsing

JS `parseInt("123abc")` silently returns `123`, `Number("")` returns `0`, and `parseInt("08")` has octal weirdness.

Floe provides strict parse functions that return `Result`:

```floe
const n = Number.parse("123")       // Ok(123)
const n = Number.parse("123abc")    // Err(ParseError)
const n = Number.parse("")          // Err(ParseError)
```

### Overlapping Spread Warning

`{...a, ...b}` silently overwrites keys from `a` when `b` has the same keys. Floe warns when spreading objects with statically-known overlapping keys.

```floe
const a = { x: 1, y: 2 }
const b = { y: 3, z: 4 }
const c = { ...a, ...b }    // WARNING: 'y' from 'a' is overwritten by 'b'
```

---

## Resolved Design Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Syntax style | TS keywords + Gleam match/pipe | Familiar to React devs, 30min learning curve |
| Function style | `fn` for named, `(x) => expr` for inline closures, `.field` for shorthand | One keyword, two closure forms, no overlap |
| Arrow `->` | Match arms, return types, function types | `(T) -> U` for function types, `match x { A -> ... }` for match arms, `fn f() -> T` for return types |
| `const name = (x) => ...` | Compile error | If it has a name, use `fn`. No two ways to name a function. |
| Dot shorthand | `.field` in callback position creates implicit closure | Covers 80% of inline callbacks (filter, map, sort) |
| Qualified variants | `Type.Variant` when ambiguous, bare when unambiguous | Compiler errors on ambiguous bare variants with helpful suggestion |
| Pipe semantics | First-arg default, `_` placeholder | Gleam approach — clean 90% of the time |
| Partial application | `f(a, _)` creates `(x) => f(a, x)` | Free bonus from `_` placeholder |
| Result unwrap | `?` operator (Rust-style) | Cleaner than `use x <- f()`, less new syntax |
| Null handling | No null/undefined, `Option<T>` only | Gleam approach — one concept for "might not exist" |
| npm null interop | Auto-wrap to `Option` at boundary | Transparent — devs never see null |
| Boolean operators | Keep `||`, `&&`, `!` | Everyone knows them, no coercion issues |
| Compiler target | Pure vanilla .tsx, zero dependencies | Eject-friendly, no runtime cost |
| Type keyword | `type` for everything, no `enum` | `enum` is broken in TS; `type` with `|` covers unions, records, brands, opaques |
| Nested unions | Unions can contain other union types, match at any depth | More powerful than Gleam and TS; compiler generates discrimination tags |
| Constructors | `Type(field: value)` — parens, not braces | Same syntax for records, unions, and function calls. Consistent. |
| Record updates | `Type(..existing, field: newValue)` | Gleam-style spread with `..` — compiles to `{ ...existing, field: newValue }` |
| Named arguments | Optional labels at call site: `f(name: "x")` | Self-documenting, order-independent when labelled |
| Default values | `field: Type = default` on types and functions | Required for React DX (component props). Constants only, named-arg-only, required fields first. |
| Object equality | Structural (deep) equality by default | No `class`, all `const` — reference equality is meaningless |
| Unit type | `()` replaces `void` | Real value, works in generics like `Result<(), E>` |
| Array sort | Returns new array, numeric default | No mutation footgun, no lexicographic surprise |
| Numeric parsing | `Number.parse` returns `Result` | No silent `NaN`, no partial parse, no octal weirdness |
| Iteration | Own values only, no prototype chain | `for...in` prototype leakage is eliminated |
| Implicit return | Last expression in a block is the return value; no `return` keyword | No silent `undefined` returns, less noise |
| Spread overlap | Warning on statically-known key overlap | Catches silent overwrites at compile time |
| Compiler language | Rust | Fast, WASM-ready for browser playground, good LSP story |
| Inline tests | `test "name" { assert expr }` co-located with code | Gleam/Rust-inspired; type-checked always, stripped from production output |
| Type definitions | `type Foo { fields }` for records, `type Foo { \| A \| B }` for unions | Unified syntax: all nominal types use `type Name { ... }`. `=` only for aliases and string literal unions |
| For blocks | `for Type { fn f(self) ... }` groups functions under a type | Rust/Swift-like method chaining DX without OOP. `self` is explicit, no `this` magic |

---

## Key Insight

The entire value proposition is: **all the checking happens at compile time, and the output is the simplest possible TypeScript.** There is no runtime. There is no framework. There is no dependency. Just a compiler that turns nice syntax into boring, correct code.

If you eject from Floe, you have normal TypeScript. That's the escape hatch, and it's the most reassuring one possible.

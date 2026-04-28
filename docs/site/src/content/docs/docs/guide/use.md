---
title: Callback Flattening & Guards
---

The `use` keyword flattens nested callbacks into linear code. Combined with guard functions, it gives you early returns without `if`, `return`, or nesting.

## Basic `use`

`use` takes the rest of the block and passes it as a callback to the function on the right:

```floe
use file <- File.open(path)
use contents <- File.readAll(file)
contents |> String.toUpper
```

This compiles to:

```typescript
File.open(path, (file) => {
  File.readAll(file, (contents) => {
    return String.toUpper(contents);
  });
});
```

`use` works with **any function whose last parameter is a callback**. No special trait or interface required.

## Zero-binding form

When the callback takes no arguments, omit the binding:

```floe
use <- Timer.delay(1000)
Console.log("done")
```

Compiles to:

```typescript
Timer.delay(1000, () => {
  console.log("done");
});
```

## Guards

Guards are the killer use case for `use`. They give you React-style early returns in a flat, linear flow -- no nesting, no `if`, no `return` keyword.

### The problem

In TypeScript, components with preconditions look like this:

```typescript
function AdminPage({ auth, user, data }) {
    if (!auth.isAdmin) return <Forbidden />;
    if (!auth.isVerified) return <VerifyPrompt />;
    if (!user) return <LoginPage />;
    if (data.error) return <ErrorPage error={data.error} />;

    return <Dashboard user={user} data={data.value} />;
}
```

This works, but it's all imperative control flow with early returns.

### The Floe way

```floe
type AdminPageProps = {
    auth: Auth,
    maybeUser: Option<User>,
    data: Result<Data, AppError>,
}

export let AdminPage(props: AdminPageProps) -> JSX.Element = {
    use <- Bool.guard(props.auth.isAdmin, <Forbidden />)
    use <- Bool.guard(props.auth.isVerified, <VerifyPrompt />)
    use user <- Option.guard(props.maybeUser, <LoginPage />)
    use data <- Result.guard(props.data, (e) -> <ErrorPage error={e} />)

    // by here: admin, verified, user unwrapped, data unwrapped
    <Dashboard user={user} data={data} />
}
```

Same linear flow, but each guard also **narrows the type**:
- `Bool.guard` -- continues if the condition is true, bails with fallback if false
- `Option.guard` -- unwraps `Some(value)`, bails with fallback on `None`
- `Result.guard` -- unwraps `Ok(value)`, bails with error handler on `Err`

### `Bool.guard`

Continue if true, bail with a fallback value if false:

```floe
use <- Bool.guard(condition, fallbackValue)
// only runs if condition is true
```

```floe
type PremiumContentProps = { isPaid: boolean }

export let PremiumContent(props: PremiumContentProps) -> JSX.Element = {
    use <- Bool.guard(props.isPaid, <UpgradePage />)

    <PremiumDashboard />
}
```

### `Option.guard`

Unwrap `Some`, bail on `None`:

```floe
use value <- Option.guard(optionValue, fallbackValue)
// value is unwrapped here
```

```floe
type ProfileProps = { maybeUser: Option<User> }

export let Profile(props: ProfileProps) -> JSX.Element = {
    use user <- Option.guard(props.maybeUser, <LoginPrompt />)

    <ProfileCard name={user.name} />
}
```

### `Result.guard`

Unwrap `Ok`, bail on `Err` with an error handler:

```floe
use value <- Result.guard(resultValue, (err) -> fallbackValue)
// value is the Ok value here
```

```floe
type DataPageProps = { result: Result<Data, ApiError> }

export let DataPage(props: DataPageProps) -> JSX.Element = {
    use data <- Result.guard(props.result, (e) -> <ErrorBanner error={e} />)

    <DataTable rows={data.rows} />
}
```

## Chaining guards

Guards compose naturally. Each one narrows the type for everything below it:

```floe
type OrderPageProps = {
    auth: Auth,
    maybeOrder: Option<Order>,
    paymentResult: Result<Payment, PaymentError>,
}

export let OrderPage(props: OrderPageProps) -> JSX.Element = {
    use <- Bool.guard(props.auth.isLoggedIn, <LoginPage />)
    use order <- Option.guard(props.maybeOrder, <p>Order not found</p>)
    use payment <- Result.guard(props.paymentResult, (e) ->
        <PaymentError message={e.message} />
    )

    <OrderConfirmation order={order} payment={payment} />
}
```

## How it works

Guards are just stdlib functions -- no new syntax. `Bool.guard` has this signature:

```
Bool.guard(condition: boolean, fallback: T, continuation: () => T) => T
```

When you write `use <- Bool.guard(cond, fallback)`, the `use` keyword takes everything after that line and passes it as the `continuation` callback. If `cond` is false, `fallback` is returned without calling the continuation.

The same pattern works for `Option.guard` and `Result.guard` -- they just unwrap the value and pass it to the continuation.

## `use` vs `?`

Both handle early exits, but for different situations:

| | `?` | `use` + guard |
|---|---|---|
| Works with | `Result` and `Option` | Any type (booleans, Options, Results) |
| Returns | `Err` / `None` to the caller | Any fallback value |
| Best for | Propagating errors up the call chain | Rendering different UI for different states |
| Requires | Function returns `Result` or `Option` | Nothing -- works in any function |

Use `?` when you want to bubble errors up. Use guards when you want to handle conditions inline with a specific fallback.

## Object-destructured bindings

When a callback's single parameter is a record, you can destructure it inline:

```floe
use { user, session } <- Context.guard(ctx, <LoginPage />)
// user and session are unwrapped from the Context result
```

Renames work the same way as in `let`:

```floe
use { user: currentUser, session: sess } <- Context.guard(ctx, fallback)
```

This lowers to a single-parameter callback with a destructure pattern:

```typescript
Context.guard(ctx, fallback, ({ user, session }) => { ... });
```

Note the distinction between the two parenthesised forms:

- `use (a, b) <- f(...)` — **multi-parameter callback**: lowers to `f(..., (a, b) -> { ... })`
- `use { a, b } <- f(...)` — **single parameter, object-destructured**: lowers to `f(..., ({ a, b }) -> { ... })`

## Compatibility with React's `use()` hook

`use` is a **contextual keyword** in Floe. It is only treated as the callback-flattening keyword when it appears at the start of a statement followed by `<-` (with or without a binding). In every other position it parses as a plain identifier, so you can still call React 19's `use()` hook:

```floe
import { use } from "react"

export let AsyncLabel(props: { promise: Promise<string> }) -> JSX.Element = {
    let label = use(props.promise)
    <span>{label}</span>
}
```

The rule is simple: `use <- ...`, `use x <- ...`, and `use (a, b) <- ...` are bind statements. Anything else -- `use(...)`, `.use`, `{ use }` -- is a regular identifier.

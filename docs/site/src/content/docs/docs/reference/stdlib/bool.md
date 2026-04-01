---
title: Bool
sidebar:
  order: 4
---

Functions for working with `boolean` values.

## Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `Bool.guard` | `boolean, T, () -> T -> T` | Continue if true, bail with fallback if false (for `use`) |

## Guard Pattern

`Bool.guard` combines with [`use`](/docs/guide/use/) to give linear early-return flow. If the condition is false, the fallback value is returned. If true, execution continues:

```floe
type AdminPageProps { auth: Auth }

export fn AdminPage(props: AdminPageProps) -> JSX.Element {
    use <- Bool.guard(props.auth.isAdmin, <Forbidden />)
    use <- Bool.guard(props.auth.isVerified, <VerifyPrompt />)

    <AdminPanel />
}
```

This replaces the TypeScript pattern of nested `if` checks with early returns:

```typescript
// TypeScript
function AdminPage({ auth }) {
    if (!auth.isAdmin) return <Forbidden />;
    if (!auth.isVerified) return <VerifyPrompt />;

    return <AdminPanel />;
}
```

`Bool.guard` is just a stdlib function — no new syntax. Inspired by Gleam's `bool.guard`.

See the [Callback Flattening & Guards](/docs/guide/use/) guide for the full pattern including `Option.guard` and `Result.guard`.

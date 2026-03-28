---
title: JSX & React
---

Floe supports JSX natively. Write React components with Floe's type system.

## Components

```floe
import trusted { useState } from "react"

export fn Counter() -> JSX.Element {
  const [count, setCount] = useState(0)

  <div>
    <h1>Count: {count}</h1>
    <button onClick={() => setCount(count + 1)}>Increment</button>
  </div>
}
```

Components are exported `fn` declarations with a `JSX.Element` return type. The last expression is the return value.

## Props

```floe
type ButtonProps {
  label: string,
  onClick: fn() -> (),
  disabled: boolean,
}

export fn Button(props: ButtonProps) -> JSX.Element {
  <button
    onClick={props.onClick}
    disabled={props.disabled}
  >
    {props.label}
  </button>
}
```

## Conditional Rendering

Use `match` expressions:

```floe
<div>
  {match isLoggedIn {
    true -> <UserProfile user={user} />,
    false -> <LoginForm />,
  }}
</div>
```

## Lists

Use pipes with `map`:

```floe
<ul>
  {items |> map((item) => <li key={item.id}>{item.name}</li>)}
</ul>
```

## Fragments

```floe
<>
  <Header />
  <Main />
  <Footer />
</>
```

## JSX Detection

The compiler automatically emits `.tsx` when JSX is detected, and `.ts` otherwise. No configuration needed.

## What's Different from React + TypeScript

- No `class` components - only function components
- No `any` in props - every prop must be typed
- Pipes instead of method chaining for data transformations
- Pattern matching instead of ternaries for complex conditionals

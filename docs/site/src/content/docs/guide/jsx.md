---
title: JSX & React
---

Floe has first-class JSX support. Write React components with all the safety guarantees.

## Components

```floe
import { useState, JSX } from "react"

export fn Counter() -> JSX.Element {
  const [count, setCount] = useState(0)

  return <div>
    <h1>Count: {count}</h1>
    <button onClick={setCount}>Increment</button>
  </div>
}
```

Components are just exported `fn` declarations that return `JSX.Element`.

## Props

```floe
type ButtonProps = {
  label: string,
  onClick: () -> (),
  disabled: boolean,
}

export fn Button(props: ButtonProps) -> JSX.Element {
  return <button
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
return <div>
  {match isLoggedIn {
    true -> <UserProfile user={user} />,
    false -> <LoginForm />,
  }}
</div>
```

## Lists

Use pipes with `map`:

```floe
return <ul>
  {items |> map(|item| <li key={item.id}>{item.name}</li>)}
</ul>
```

## Fragments

```floe
return <>
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

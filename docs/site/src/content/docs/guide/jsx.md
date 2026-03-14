---
title: JSX & React
---

ZenScript has first-class JSX support. Write React components with all the safety guarantees.

## Components

```zenscript
import { useState } from "react"

export function Counter(): JSX.Element {
  const [count, setCount] = useState(0)

  return <div>
    <h1>Count: {count}</h1>
    <button onClick={setCount}>Increment</button>
  </div>
}
```

Components are just exported functions that return `JSX.Element`.

## Props

```zenscript
type ButtonProps = {
  label: string,
  onClick: () => void,
  disabled: bool,
}

export function Button(props: ButtonProps): JSX.Element {
  return <button
    onClick={props.onClick}
    disabled={props.disabled}
  >
    {props.label}
  </button>
}
```

## Conditional Rendering

Use `if`/`else` expressions:

```zenscript
return <div>
  {if isLoggedIn {
    <UserProfile user={user} />
  } else {
    <LoginForm />
  }}
</div>
```

## Lists

Use pipes with `map`:

```zenscript
return <ul>
  {items |> map(item => <li key={item.id}>{item.name}</li>)}
</ul>
```

## Fragments

```zenscript
return <>
  <Header />
  <Main />
  <Footer />
</>
```

## JSX Detection

The compiler automatically emits `.tsx` when JSX is detected, and `.ts` otherwise. No configuration needed.

## What's Different from React + TypeScript

- No `class` components — only function components
- No `any` in props — every prop must be typed
- Pipes instead of method chaining for data transformations
- Pattern matching instead of ternaries for complex conditionals

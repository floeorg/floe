---
title: JSX & React
---

Floe supports JSX natively. Write React components with Floe's type system.

## Components

```floe
import { useState } from "react"

export let Counter() -> JSX.Element = {
  let (count, setCount) = useState(0)

  <div>
    <h1>Count: {count}</h1>
    <button onClick={() -> setCount(count + 1)}>Increment</button>
  </div>
}
```

Components are exported `let` declarations with a `JSX.Element` return type. The last expression is the return value.

## Props

```floe
type ButtonProps = {
  label: string,
  onClick: () -> (),
  disabled: boolean,
}

export let Button(props: ButtonProps) -> JSX.Element = {
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

### Optional Rendering

For the common "render if present, nothing if absent" pattern, use `Option.map`. Since `None` compiles to `undefined` and React ignores `undefined` children, `Option<JSX.Element>` works directly in JSX:

```floe
// Instead of matching with a None -> <></> arm:
<div>
  {user.nickname |> Option.map((nick) -> <span className="badge">{nick}</span>)}
</div>
```

This replaces the TypeScript `{x && <Component />}` pattern, without the [truthiness pitfalls](https://react.dev/learn/conditional-rendering#logical-and-operator-) (e.g. `{count && <Tag />}` rendering `0`).

## Lists

Use pipes with `map`:

```floe
<ul>
  {items |> map((item) -> <li key={item.id}>{item.name}</li>)}
</ul>
```

## Spread Attributes

Forward all props to a child element:

```floe
export let Card(props: CardProps) -> JSX.Element = {
    <div {...props} className="card" />
}
```

Compiles to `<div {...props} className={"card"} />` in TypeScript.

## Member Expressions

Compound component patterns using dot notation are supported:

```floe
import { Select, ListBox } from "ui"

export let Picker() -> JSX.Element = {
    <Select>
        <Select.Trigger>Choose...</Select.Trigger>
        <Select.Value />
        <Select.Popover>
            <ListBox>
                <ListBox.Item key="a">Alpha</ListBox.Item>
                <ListBox.Item key="b">Beta</ListBox.Item>
            </ListBox>
        </Select.Popover>
    </Select>
}
```

This is common in component libraries like React Aria, Radix UI, and similar. The tag name compiles as-is to TypeScript.

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

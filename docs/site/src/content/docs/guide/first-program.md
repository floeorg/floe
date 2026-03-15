---
title: Your First Program
---

## Hello World

Create a file called `hello.fl`:

```floe
export fn greet(name: string) -> string {
  return `Hello, ${name}!`
}

greet("world") |> console.log
```

Compile it:

```bash
floe build hello.fl
```

This produces `hello.ts`:

```typescript
export function greet(name: string): string {
  return `Hello, ${name}!`;
}

console.log(greet("world"));
```

## A React Component

Create `counter.fl`:

```floe
import { useState } from "react"

export fn Counter() -> JSX.Element {
  const [count, setCount] = useState(0)

  return <div>
    <p>Count: {count}</p>
    <button onClick={setCount}>+1</button>
  </div>
}
```

Compile it:

```bash
floe build counter.fl
```

This produces `counter.tsx`, a standard React component that works with any React setup.

## Using Pipes

Pipes let you read transformations left-to-right instead of inside-out:

```floe
// Without pipes (nested calls)
const result = toString(add(multiply(value, 2), 1))

// With pipes (left to right)
const result = value
  |> multiply(_, 2)
  |> add(_, 1)
  |> toString
```

The `_` placeholder marks where the piped value goes.

## Type Checking

Run the type checker without generating output:

```bash
floe check src/
```

This catches errors like:
- Using `any` (use `unknown` instead)
- Nullable values without `Option<T>`
- Non-exhaustive pattern matches
- Unused variables and imports

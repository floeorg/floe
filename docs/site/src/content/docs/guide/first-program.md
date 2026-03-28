---
title: Your First Program
---

## Hello World

Create a file called `hello.fl`:

```floe
export fn greet(name: string) -> string {
  `Hello, ${name}!`
}

greet("world") |> Console.log
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

  <div>
    <p>Count: {count}</p>
    <button onClick={() => setCount(count + 1)}>+1</button>
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
  |> multiply(2)
  |> add(1)
  |> toString
```

By default, the piped value is inserted as the first argument. Use `_` when you need it in a different position: `value |> f(other, _)` becomes `f(other, value)`.

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

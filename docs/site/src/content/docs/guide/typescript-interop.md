---
title: TypeScript Interop
---

Floe compiles to TypeScript, so you can use any existing TypeScript or React library directly. No bindings, no wrappers, no code generation.

## Importing npm packages

Import from npm packages the same way you would in TypeScript:

```floe
import { useState, useEffect } from "react"
import { z } from "zod"
import { clsx } from "clsx"
```

The compiler reads `.d.ts` type definitions to understand the types of imported values.

## `trusted` imports

By default, Floe treats npm imports as potentially throwing. The compiler requires you to wrap calls in `try`, which returns a `Result<T, Error>`:

```floe
import { parseYaml } from "yaml-lib"

// parseYaml might throw, so you must use try
const result = try parseYaml(input)
match result {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

For libraries you know won't throw, mark the import as `trusted` to skip the `try` requirement:

```floe
import trusted { useState, useEffect } from "react"
import trusted { clsx } from "clsx"

// No try needed - these are trusted
const [count, setCount] = useState(0)
const classes = clsx("btn", active)
```

You can also trust individual functions from a module:

```floe
import { trusted capitalize, fetchData } from "some-lib"

capitalize("hello")             // trusted, no try needed
const data = try fetchData()    // not trusted, try required
```

## String literal unions

Many TypeScript libraries use string literal unions for configuration and options:

```typescript
// React
type HTMLInputTypeAttribute = "text" | "password" | "email" | "number";

// API clients
type Method = "GET" | "POST" | "PUT" | "DELETE";
```

Floe supports these natively:

```floe
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

fn describe(method: HttpMethod) -> string {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
```

The match is exhaustive -- if you miss a variant, the compiler tells you. The type compiles directly to the same TypeScript string union (no tags, no wrapping).

## Nullable and optional type conversion

Floe has no `null` or `undefined`. When importing from TypeScript, the compiler converts nullable and optional types automatically:

| TypeScript type | Floe type |
|----------------|-----------|
| `T \| null` | `Option<T>` |
| `T \| undefined` | `Option<T>` |
| `T \| null \| undefined` | `Option<T>` |
| `x?: T` (function param) | `x: Option<T> = None` |
| `any` | `unknown` |

Optional parameters (`?`) become `Option<T>` with a default of `None`, so you can omit them when calling:

```floe
import trusted { getElementById } from "some-dom-lib"
// .d.ts says: getElementById(id: string): Element | null
// Floe sees: getElementById(id: string) -> Option<Element>

match getElementById("app") {
  Some(el) -> render(el),
  None -> Console.error("element not found"),
}
```

## Using React hooks

React hooks work directly. Use `trusted` since hooks don't throw:

```floe
import trusted { useState, useEffect, useCallback } from "react"

export fn Counter() -> JSX.Element {
  const [count, setCount] = useState(0)

  useEffect(() => {
    Console.log("count changed:", count)
  }, [count])

  <button onClick={() => setCount(count + 1)}>
    {`Count: ${count}`}
  </button>
}
```

## Using React component libraries

Third-party React components work as regular JSX:

```floe
import trusted { Button, Dialog } from "@radix-ui/react"

export fn MyPage() -> JSX.Element {
  const [open, setOpen] = useState(false)

  <div>
    <Button onClick={() => setOpen(true)}>Open</Button>
    <Dialog open={open} onOpenChange={setOpen}>
      <p>Dialog content</p>
    </Dialog>
  </div>
}
```

## Output

Floe's compiled output is standard TypeScript. Your build tool (Vite, Next.js, etc.) processes it like any other `.ts` file. There is no Floe-specific runtime or framework to install.

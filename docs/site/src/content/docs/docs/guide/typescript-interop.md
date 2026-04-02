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

The compiler reads `.d.ts` type definitions to understand the types of imported values. These imports are callable directly -- no special syntax needed.

## `throws` imports

Some npm functions can throw exceptions at runtime (JSON parsers, API clients, file I/O). Mark these imports with `throws` so the compiler auto-wraps calls in `Result<T, Error>`:

```floe
import throws { parseYaml } from "yaml-lib"

// parseYaml is auto-wrapped — returns Result<T, Error>
const result = parseYaml(input)
match result {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

Use `?` to unwrap the result concisely:

```floe
const data = parseYaml(input)?  // unwraps or returns Err early
```

You can mark individual functions as throwing from a module:

```floe
import { capitalize, throws fetchData } from "some-lib"

capitalize("hello")             // direct call, no wrapping
const data = fetchData()        // Result<T, Error> — auto-wrapped
```

## Bridge types (`=` syntax)

When you need to reference TypeScript types, Floe uses the `=` syntax. This is distinct from `{ }` which creates new Floe types. See [Types](/docs/guide/types/#two-kinds-of-type-declarations) for the full mental model.

### String literal unions

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

### Type aliases and intersections

Alias TypeScript types or combine them with `&`:

```floe
import { ComponentProps } from "react"
import { tv, VariantProps } from "tailwind-variants"

type DivProps = ComponentProps<"div">

const cardVariants = tv({ base: "rounded-xl", variants: { size: { sm: "p-2" } } })
type CardProps = VariantProps<typeof cardVariants> & { className: string }
```

`&` intersections are only valid in `=` declarations. For Floe-native record composition, use `...Spread` in `{ }` definitions.

## Nullable and optional type conversion

Floe has no `null` or `undefined`. When importing from TypeScript, the compiler converts nullable and optional types automatically:

| TypeScript type | Floe type |
|----------------|-----------|
| `T \| null` | `Option<T>` |
| `T \| undefined` | `Option<T>` |
| `T \| null \| undefined` | `Option<T>` |
| `x?: T` (function param) | `x: Option<T> = None` |
| `x?: T \| null` | `Settable<T> = Unchanged` |
| `any` | `unknown` |

Optional parameters (`?`) become `Option<T>` with a default of `None`, so you can omit them when calling:

```floe
import { getElementById } from "some-dom-lib"
// .d.ts says: getElementById(id: string): Element | null
// Floe sees: getElementById(id: string) -> Option<Element>

match getElementById("app") {
  Some(el) -> render(el),
  None -> Console.error("element not found"),
}
```

## Using React hooks

React hooks work directly:

```floe
import { useState, useEffect, useCallback } from "react"

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
import { Button, Dialog } from "@radix-ui/react"

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

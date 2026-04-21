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

The compiler reads `.d.ts` type definitions to understand the types of imported values. npm imports are **untrusted by default** -- calls are auto-wrapped in `Result<T, Error>`.

## Untrusted imports (default)

All npm imports are untrusted by default. The compiler auto-wraps calls in `Result<T, Error>`:

```floe
import { parseYaml } from "yaml-lib"

// parseYaml is auto-wrapped — returns Result<T, Error>
let result = parseYaml(input)
match result {
  Ok(data) -> process(data),
  Err(e) -> Console.error(e),
}
```

Use `?` to unwrap the result concisely:

```floe
let data = parseYaml(input)?  // unwraps or returns Err early
```

## `trusted` imports

For npm functions known to be safe (like React hooks, utility libraries), mark them with `trusted` so they can be called directly without Result wrapping:

```floe
import trusted { useState, useEffect } from "react"

let (count, setCount) = useState(0)  // direct call, no wrapping
```

You can mark individual functions as trusted from a module:

```floe
import { trusted capitalize, fetchData } from "some-lib"

capitalize("hello")             // direct call, no wrapping (trusted)
let data = fetchData()        // Result<T, Error> — auto-wrapped (untrusted)
```

## Bridging TypeScript types

Every Floe type declaration has the shape `type Name = RHS`. For interop with TypeScript libraries, two utility types bridge the structural operators TS uses:

- `OneOf<A, B, ...>` compiles to `A | B | ...`
- `Intersect<A, B, ...>` compiles to `A & B & ...`

Plain aliases and TS utility types (`Partial`, `Pick`, `ReturnType`, ...) work directly on the RHS.

### String-literal unions

Many TypeScript libraries use string literal unions for configuration and options:

```typescript
// React
type HTMLInputTypeAttribute = "text" | "password" | "email" | "number";

// API clients
type Method = "GET" | "POST" | "PUT" | "DELETE";
```

In Floe, wrap them in `OneOf<>`:

```floe
type HttpMethod = OneOf<"GET", "POST", "PUT", "DELETE">

let describe(method: HttpMethod) -> string = {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
```

The match is exhaustive -- if you miss a variant, the compiler tells you. The type compiles directly to a TypeScript string union (no tags, no wrapping).

Writing a bare string-literal union (`type M = "a" | "b"`) is a compile error (**E201**). Top-level `|` always declares nominal variants in Floe; `OneOf<>` is how you ask for the structural form.

### Type aliases

Alias TypeScript types with plain `=`:

```floe
import { ComponentProps } from "react"

type DivProps = ComponentProps<"div">
type PartialUser = Partial<User>
type UserKeys = Pick<User, OneOf<"name", "email">>
```

### Intersections

Combine TypeScript types with `Intersect<>`:

```floe
import { tv, VariantProps } from "tailwind-variants"

let cardVariants = tv({ base: "rounded-xl", variants: { size: { sm: "p-2" } } })
type CardProps = Intersect<VariantProps<typeof cardVariants>, { className: string }>
```

For Floe-native record composition, prefer `...Spread` in a `{ }` record body:

```floe
type CardProps = {
  ...VariantProps<typeof cardVariants>,
  className: string,
}
```

### Function-type aliases

Use `->` for function types. Parameter labels are optional documentation:

```floe
import { Request, Response } from "express"

type Handler = (req: Request, res: Response) -> Promise<()>
```

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

export let Counter() -> JSX.Element = {
  let (count, setCount) = useState(0)

  useEffect(() -> {
    Console.log("count changed:", count)
  }, [count])

  <button onClick={() -> setCount(count + 1)}>
    {`Count: ${count}`}
  </button>
}
```

## Using React component libraries

Third-party React components work as regular JSX:

```floe
import { Button, Dialog } from "@radix-ui/react"

export let MyPage() -> JSX.Element = {
  let (open, setOpen) = useState(false)

  <div>
    <Button onClick={() -> setOpen(true)}>Open</Button>
    <Dialog open={open} onOpenChange={setOpen}>
      <p>Dialog content</p>
    </Dialog>
  </div>
}
```

## Globals (browser and runtime APIs)

Browser globals like `window`, `document`, `navigator`, and `fetch` are available automatically -- no imports needed. Floe reads your `tsconfig.json` to determine which globals exist:

```floe
// Browser project (lib includes "DOM")
let url = window.location.href
navigator.clipboard.writeText("hello") |> await
let width = window.innerWidth
```

For non-browser runtimes, configure `compilerOptions.lib` and `compilerOptions.types` in your `tsconfig.json`:

```json
// Node.js
{ "compilerOptions": { "lib": ["ES2020"], "types": ["node"] } }
```

```floe
// Now process, Buffer, etc. are available
let env = process.env
```

See [Configuration](/docs/reference/configuration/#lib-and-types---controlling-globals) for details.

## Output

Floe's compiled output is standard TypeScript. Your build tool (Vite, Next.js, etc.) processes it like any other `.ts` file. There is no Floe-specific runtime or framework to install.

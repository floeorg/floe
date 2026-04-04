---
title: Vite Plugin
---

The [`@floeorg/vite-plugin`](https://www.npmjs.com/package/@floeorg/vite-plugin) package lets Vite transform `.fl` files during development and production builds.

## Installation

```bash
npm install -D @floeorg/vite-plugin
```

Make sure `floe` is installed and available in your PATH.

## Configuration

```typescript
// vite.config.ts
import { defineConfig } from "vite"
import floe from "@floeorg/vite-plugin"

export default defineConfig({
  plugins: [floe()],
})
```

### Options

```typescript
floe({
  // Path to the floe binary (default: "floe")
  compiler: "/usr/local/bin/floe",
})
```

## TypeScript Setup

Add `allowArbitraryExtensions` and `rootDirs` to your `tsconfig.json` so TypeScript can resolve `.fl` imports:

```json
{
  "compilerOptions": {
    "allowArbitraryExtensions": true,
    "rootDirs": ["./src", "./.floe/src"]
  }
}
```

If you use path aliases (like `#/` or `@/`), also add `.floe/` lookups to your `paths`:

```json
{
  "compilerOptions": {
    "paths": {
      "#/*": ["./src/*", "./.floe/src/*"],
      "@/*": ["./src/*", "./.floe/src/*"]
    }
  }
}
```

The compiler generates `.d.fl.ts` type declarations in the `.floe/` directory, and `rootDirs` tells TypeScript to treat `src/` and `.floe/src/` as a single merged directory. This lets TypeScript resolve types for imports like `import { Header } from "./header.fl"` automatically, without polluting the source tree with generated files.

## How It Works

1. Vite encounters a `.fl` import
2. The plugin reads pre-compiled output from `.floe/` if `floe watch` is running and the output is fresh
3. Otherwise, it falls back to calling `floe build --emit-stdout` to compile on-demand
4. The TypeScript output is passed to Vite's normal pipeline
5. Hot Module Replacement works automatically

Running `floe watch src/` alongside Vite is optional but recommended -- it avoids per-file process spawns and shares the same `.floe/` output that other tools use.

## With React

```typescript
// vite.config.ts
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import floe from "@floeorg/vite-plugin"

export default defineConfig({
  plugins: [
    floe(),  // must come before React plugin
    react(),
  ],
})
```

## File Structure

```
my-app/
  src/
    App.fl          # Floe component
    utils.fl        # Floe utilities
    legacy.tsx      # Existing TypeScript (works alongside)
  vite.config.ts
  package.json
```

Floe files and TypeScript files coexist. Adopt incrementally.

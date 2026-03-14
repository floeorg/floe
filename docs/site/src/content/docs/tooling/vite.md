---
title: Vite Plugin
---

The `vite-plugin-zenscript` package lets Vite transform `.zs` files during development and production builds.

## Installation

```bash
npm install -D vite-plugin-zenscript
```

Make sure `zsc` is installed and available in your PATH.

## Configuration

```typescript
// vite.config.ts
import { defineConfig } from "vite"
import zenscript from "vite-plugin-zenscript"

export default defineConfig({
  plugins: [zenscript()],
})
```

### Options

```typescript
zenscript({
  // Path to the zsc binary (default: "zsc")
  compiler: "/usr/local/bin/zsc",
})
```

## How It Works

1. Vite encounters a `.zs` import
2. The plugin calls `zsc` to compile it to TypeScript
3. The TypeScript output is passed to Vite's normal pipeline
4. Hot Module Replacement works automatically

## With React

```typescript
// vite.config.ts
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import zenscript from "vite-plugin-zenscript"

export default defineConfig({
  plugins: [
    zenscript(),  // must come before React plugin
    react(),
  ],
})
```

## File Structure

```
my-app/
  src/
    App.zs          # ZenScript component
    utils.zs        # ZenScript utilities
    legacy.tsx      # Existing TypeScript (works alongside)
  vite.config.ts
  package.json
```

ZenScript files and TypeScript files coexist. Adopt incrementally.

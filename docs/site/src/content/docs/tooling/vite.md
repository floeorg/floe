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

## How It Works

1. Vite encounters a `.fl` import
2. The plugin calls `floe` to compile it to TypeScript
3. The TypeScript output is passed to Vite's normal pipeline
4. Hot Module Replacement works automatically

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

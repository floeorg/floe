---
title: Your First Project
---

## Vite + React Setup

Scaffold a React project and add Floe:

```bash
npm create vite@latest my-app -- --template react-ts
cd my-app
npm install
npm install -D @floeorg/vite-plugin
```

### Configure Vite

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

### Configure TypeScript

Add `allowArbitraryExtensions` and `rootDirs` to `tsconfig.json` so TypeScript can resolve `.fl` imports:

```json
{
  "compilerOptions": {
    "allowArbitraryExtensions": true,
    "rootDirs": ["./src", "./.floe/src"]
  }
}
```

### Write a Component

Create `src/Counter.fl`:

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

Import it from your existing TypeScript:

```typescript
// src/App.tsx
import { Counter } from "./Counter.fl"

function App() {
  return <Counter />
}
```

### Run the Dev Server

```bash
npm run dev
```

Vite compiles `.fl` files on the fly. HMR works automatically.

## Any Toolchain (without Vite)

`floe watch` compiles `.fl` files to `.floe/` and recompiles on change. Since the output is standard TypeScript, any tool works -- wrangler, node, bun, esbuild, webpack, etc.

### Configure TypeScript

Add `rootDirs` to your `tsconfig.json` so TypeScript resolves `.fl` imports through the `.floe/` output:

```json
{
  "compilerOptions": {
    "allowArbitraryExtensions": true,
    "rootDirs": ["./src", "./.floe/src"]
  }
}
```

### Add Scripts

```json
{
  "scripts": {
    "dev": "floe watch src/",
    "build": "floe build src/"
  }
}
```

### Development

Run `floe watch` alongside your dev server:

```bash
# Terminal 1
npm run dev

# Terminal 2 -- your backend/app server
wrangler dev     # or: node src/app.ts, bun src/app.ts, etc.
```

When you edit a `.fl` file, `floe watch` recompiles it to `.floe/`. Your dev server sees the `.ts` file change and hot-reloads.

### Production

Run `floe build` before your build step:

```bash
floe build src/
# then your normal build
```

### Type Checking

Run the checker without generating output:

```bash
floe check src/
```

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

## Node / Any Runtime (without Vite)

For backend apps, scripts, or non-Vite projects, use `@floeorg/register` to resolve `.fl` imports at runtime, and `floe watch` to keep compiled output fresh.

### Install

```bash
npm install -D @floeorg/register
```

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
    "dev": "floe watch src/ & node --import @floeorg/register src/app.ts",
    "build": "floe build src/"
  }
}
```

The `--import @floeorg/register` flag teaches Node how to resolve `.fl` imports. It redirects them to the compiled `.ts`/`.tsx` output in `.floe/`. Works with `node` (v22.14+), `tsx`, and any Node-based runtime.

### Development

`floe watch` recompiles `.fl` files to `.floe/` on change. Run it alongside your app:

```bash
# Single command (as in the script above)
npm run dev

# Or in separate terminals
floe watch src/              # Terminal 1
node --import @floeorg/register src/app.ts   # Terminal 2
```

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

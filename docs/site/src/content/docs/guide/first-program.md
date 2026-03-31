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
import trusted { useState } from "react"

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

## Standalone (without Vite)

For scripts or non-React projects, use `floe build` directly:

```bash
# Create a file
cat > hello.fl << 'EOF'
export fn greet(name: string) -> string {
  `Hello, ${name}!`
}

greet("world") |> Console.log
EOF

# Compile to TypeScript
floe build hello.fl

# Run the output
npx tsx hello.ts
```

### Type Checking

Run the checker without generating output:

```bash
floe check src/
```

<p align="center">
  <img src="assets/logo.svg" alt="Floe logo" width="128">
</p>

<p align="center">
  <a href="https://github.com/floeorg/floe/releases"><img src="https://img.shields.io/github/release/floeorg/floe" alt="GitHub release"></a>
  <a href="https://crates.io/crates/floe"><img src="https://img.shields.io/crates/v/floe" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/@floeorg/vite-plugin"><img src="https://img.shields.io/npm/v/@floeorg/vite-plugin" alt="npm"></a>
  <a href="https://open-vsx.org/extension/floeorg/floe"><img src="https://img.shields.io/open-vsx/v/floeorg/floe" alt="Open VSX"></a>
  <a href="https://marketplace.visualstudio.com/items?itemName=floeorg.floe"><img src="https://img.shields.io/visual-studio-marketplace/v/floeorg.floe?label=marketplace" alt="VS Code Marketplace"></a>
</p>

<!-- A spacer -->
<div>&nbsp;</div>

Floe is a functional language that compiles to TypeScript. Pipes, pattern matching, Result/Option types, and full npm interop. The compiler is written in Rust.

```floe
import trusted { useState } from "react"

type Todo {
  id: string,
  text: string,
  done: boolean,
}

export fn App() -> JSX.Element {
  const [todos, setTodos] = useState<Array<Todo>>([])

  const completed = todos
    |> filter(.done)
    |> length

  <div>
    <h1>Todos ({completed} done)</h1>
    {todos |> map((todo) => <p key={todo.id}>{todo.text}</p>)}
  </div>
}
```

## Install

```bash
cargo install floe
```

## Add to a Vite project

```bash
npm install -D @floeorg/vite-plugin
```

```ts
// vite.config.ts
import floe from "@floeorg/vite-plugin"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

export default defineConfig({
  plugins: [floe(), react()],
})
```

Write `.fl` files next to your `.ts` files. Import in either direction.

## Links

- [Documentation](https://floe-lang.dev)
- [Language Tour](https://floe-lang.dev/guide/tour/)
- [Your First Project](https://floe-lang.dev/guide/first-program/)
- [CLI Reference](https://floe-lang.dev/reference/cli/)
- [Changelog](CHANGELOG.md)

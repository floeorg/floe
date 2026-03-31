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

Floe is a strict, functional language that compiles to TypeScript. It works with
any TypeScript or React library. The compiler is written in Rust.

## Getting Started

```bash
# Install
cargo install floe

# Create a project
floe init my-app && cd my-app && npm install

# Build
floe build src/
```

## Editor Support

- **[VS Code](https://floeorg.github.io/floe/tooling/vscode/)** -- syntax highlighting, diagnostics, hover, go-to-definition
- **[Neovim](https://floeorg.github.io/floe/tooling/neovim/)** -- tree-sitter highlighting + LSP

## Vite Integration

```bash
npm install -D @floeorg/vite-plugin
```

```ts
import floe from "@floeorg/vite-plugin"
import { defineConfig } from "vite"

export default defineConfig({
  plugins: [floe()],
})
```

## Links

- [Documentation](https://floeorg.github.io/floe)
- [Language Tour](https://floeorg.github.io/floe/guide/tour/)
- [CLI Reference](https://floeorg.github.io/floe/tooling/cli/)
- [Changelog](CHANGELOG.md)

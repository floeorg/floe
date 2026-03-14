---
title: Installation
---

## Install the Compiler

ZenScript ships as a single Rust binary called `zsc`.

### From Source

```bash
# Clone and build
git clone https://github.com/milkyskies/zenscript
cd zenscript
cargo install --path .

# Verify
zsc --version
```

### Prerequisites

- [Rust](https://rustup.rs/) 1.85+ (for building from source)
- [Node.js](https://nodejs.org/) 18+ (for your project's build toolchain)

## Create a Project

```bash
# Scaffold a new ZenScript project
zsc init my-app
cd my-app

# Install npm dependencies
npm install

# Compile .zs files
zsc build src/

# Or watch for changes
zsc watch src/
```

## Editor Setup

### VS Code

Install the **ZenScript** extension from the VS Code marketplace, or build from source:

```bash
cd editors/vscode
npm install
npm run build
```

The extension provides:
- Syntax highlighting for `.zs` files
- LSP integration (diagnostics, hover)
- Code snippets

### Other Editors

ZenScript includes an LSP server. Start it with:

```bash
zsc lsp
```

Any editor with LSP support can connect to it.

## Next Steps

- [Write your first program](/guide/first-program)
- [Set up Vite integration](/tooling/vite)

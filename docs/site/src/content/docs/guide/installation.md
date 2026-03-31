---
title: Installation
---

## Install the Compiler

Floe ships as a single Rust binary called `floe`.

### From Source

```bash
# Clone and build
git clone https://github.com/floeorg/floe
cd floe
cargo install --path .

# Verify
floe --version
```

### Prerequisites

- [Rust](https://rustup.rs/) 1.85+ (for building from source)
- [Node.js](https://nodejs.org/) 18+ (for your project's build toolchain)

## Create a Project

```bash
# Scaffold a new Floe project
floe init my-app
cd my-app

# Install npm dependencies
npm install

# Compile .fl files
floe build src/

# Or watch for changes
floe watch src/
```

## Editor Setup

### VS Code

Install the **Floe** extension from the [VS Code marketplace](https://marketplace.visualstudio.com/) or [Open VSX](https://open-vsx.org/extension/floeorg/floe) -- search for "Floe". This gives you syntax highlighting, LSP diagnostics, hover types, completions, and code snippets.

### Other Editors

Floe includes an LSP server. Start it with:

```bash
floe lsp
```

Any editor with LSP support can connect to it.

## LLM Setup

LLMs don't know Floe yet. Give them the language reference so they can write Floe code for you:

**Claude Code / Cursor / Copilot** — add this to your project's AI context:

```
https://floeorg.github.io/floe/llms.txt
```

**Claude Code** — you can also fetch it directly in a conversation:

```
@url https://floeorg.github.io/floe/llms.txt
```

**Other tools** — download `llms.txt` and paste it into your system prompt or context window.

## Next Steps

- [Write your first program](/guide/first-program)
- [Set up Vite integration](/reference/vite)

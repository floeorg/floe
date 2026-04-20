---
title: Installation
---

:::caution[Experimental]
Floe is pre-1.0 software. The compiler may have bugs, and the language syntax, compiler output, and public APIs can change between releases. Pin the compiler version in CI and in any `package.json` that depends on `@floeorg/*` packages.
:::

## Install the Compiler

Floe ships as a single prebuilt binary called `floe`. The install script detects your OS and architecture and drops the binary into `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/floeorg/floe/main/install.sh | sh
```

Pin a specific version with `FLOE_VERSION`, or pick a different install directory with `INSTALL_DIR`:

```bash
curl -fsSL https://raw.githubusercontent.com/floeorg/floe/main/install.sh \
  | FLOE_VERSION=v0.5.4 INSTALL_DIR=/usr/local/bin sh
```

Supported targets: macOS (arm64 + x86_64) and Linux (x86_64 + aarch64). On Windows, download the zip from the [latest release](https://github.com/floeorg/floe/releases/latest) and add the extracted directory to your `PATH`.

### From Source

If you already have Rust installed and want to hack on the compiler itself:

```bash
git clone https://github.com/floeorg/floe
cd floe
cargo install --path crates/floe-cli

# Verify
floe --version
```

### Prerequisites

- [Node.js](https://nodejs.org/) 18+ (for your project's build toolchain)
- [Rust](https://rustup.rs/) 1.94+ (only if you're building from source)

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

## Next Steps

- [Write your first program](/docs/guide/first-program)
- [Set up Vite integration](/docs/reference/vite)

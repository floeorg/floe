<p align="center">
  <img src="assets/logo.svg" alt="Floe logo" width="128">
</p>

<p align="center">
  <a href="https://github.com/milkyskies/floe/releases"><img src="https://img.shields.io/github/release/milkyskies/floe" alt="GitHub release"></a>
</p>

<!-- A spacer -->
<div>&nbsp;</div>

Floe is a strict, functional language that compiles to TypeScript. It works with
any TypeScript or React library. The compiler is written in Rust.

For more information see [the docs](https://milkyskies.github.io/floe).

> **Status:** Early development. The compiler is functional but the language is not yet stable.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [pnpm](https://pnpm.io/) (v10+)
- [Node.js](https://nodejs.org/) (v20+)

### Build the compiler

```bash
cargo build
```

### Install JS dependencies

```bash
pnpm install
```

### Run the example app

```bash
pnpm dev:todo
```

## Documentation

Full language guide and reference: [milkyskies.github.io/floe](https://milkyskies.github.io/floe)

Example app: [`examples/todo-app/`](examples/todo-app/)

## Project Structure

```
├── src/                  # Compiler source (Rust)
│   ├── main.rs           # CLI entry point
│   ├── lexer.rs          # Tokenizer
│   ├── parser.rs         # Recursive descent parser
│   ├── checker.rs        # Type checker
│   ├── codegen.rs        # TypeScript code generation
│   ├── formatter.rs      # Code formatter
│   └── lsp.rs            # Language server
├── crates/
│   └── floe-wasm/        # WASM build for browser playground
├── integrations/
│   └── vite-plugin-floe/ # Vite plugin
├── examples/
│   └── todo-app/         # Example React + Floe app
├── editors/
│   ├── vscode/           # VS Code extension
│   └── neovim/           # Neovim support
├── docs/
│   ├── design.md         # Language specification
│   └── site/             # Documentation site (Astro)
├── playground/           # Browser-based playground
└── tests/                # Compiler test suite
```

## Development

### Building

```bash
cargo build               # Build the compiler
cargo build --release      # Release build
pnpm build                 # Build all JS packages
```

### Testing

```bash
cargo test                 # Run compiler tests
cargo clippy -- -D warnings  # Lint Rust code
cargo fmt -- --check       # Check Rust formatting
```

### Workspace Scripts

```bash
pnpm dev:todo              # Run the example todo app
pnpm dev:docs              # Run the docs site
pnpm build:plugin          # Build the Vite plugin
pnpm build                 # Build all JS packages
pnpm lint                  # Lint all JS packages
pnpm format                # Format all JS packages
pnpm clean                 # Clean all node_modules and dist
```

## Contributing

1. Check open issues for available work
2. Read `docs/design.md` - it's the language specification and source of truth
3. Run the quality gates before submitting:
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   RUSTFLAGS="-D warnings" cargo test
   ```
4. Open a PR targeting `main`

## License

MIT

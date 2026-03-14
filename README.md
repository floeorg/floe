# Floe

A programming language inspired by Gleam and Rust that compiles to vanilla TypeScript + React.

Familiar syntax for TS/React developers, but with pipes, exhaustive pattern matching, no escape hatches, and compile-time safety that eliminates entire categories of bugs. Zero runtime dependencies - the compiler does all the work, the output is boring `.tsx`.

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

This produces the `floe` CLI binary.

### Install JS dependencies

```bash
pnpm install
```

### Run the example app

```bash
pnpm dev:todo
```

This builds the Vite plugin, then starts the todo app with hot reload.

## Language Features

### Pipe Operator

```floe
users
  |> Array.filter(.active)
  |> Array.sortBy(.name)
  |> Array.take(10)

// Placeholder for non-first position
"hello" |> String.padStart(_, 10)

// Dot shorthand for field access
todos |> Array.filter(.done == false)
todos |> Array.map(.text)

// Pipe lambdas for more complex transforms
todos |> Array.map(|t| Todo(..t, done: true))
```

### Exhaustive Pattern Matching

```floe
match route {
  Home          -> <HomePage />
  Profile(id)   -> <ProfilePage id={id} />
  Settings(tab) -> <SettingsPage tab={tab} />
}

match fetchUser(id) {
  Ok(user)          -> <Profile user={user} />
  Err(NotFound)     -> <NotFoundPage />
  Err(Network(msg)) -> <ErrorBanner msg={msg} />
}
```

### Result and Option Types

```floe
// No null, no undefined, no throw
fn loadProfile(id: UserId) -> Result<Profile, AppError> {
  const user  = fetchUser(id)?
  const posts = fetchPosts(user.id)?
  Ok({ user, posts })
}

// Option for missing values
const display = match user.nickname {
  Some(nick) -> nick
  None       -> user.name
}
```

### Tagged Unions

```floe
type Filter =
  | All
  | Active
  | Completed

type Route =
  | Home
  | Profile(id: string)
  | Settings(tab: string)
  | NotFound

// Construct variants with qualified syntax
const f = Filter.All
const r = Route.Profile(id: "123")

// Match arms stay bare
match filter {
  All       -> todos,
  Active    -> todos |> Array.filter(.done == false),
  Completed -> todos |> Array.filter(.done == true),
}
```

### What's Removed

No `class`, `enum`, `any`, `null`, `undefined`, `throw`, `let`, or `as`. These are compile errors - use safer alternatives like `type` unions, `Option<T>`, `Result<T, E>`, `const`, and `match`.

## CLI Usage

```bash
floe build <file.fl>           # Compile .fl files to .tsx
floe build <file.fl> --out-dir dist  # Specify output directory
floe check <file.fl>           # Type-check without emitting
floe watch <dir> --out-dir dist     # Watch and recompile on change
floe fmt <file.fl>             # Format source files
floe fmt --check <file.fl>     # Check formatting (CI mode)
floe init [path]               # Scaffold a new Floe project
floe lsp                       # Start the language server
```

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

### Running the Docs Site

```bash
pnpm dev:docs
```

### VS Code Extension

For development, use the provided script:

```bash
./scripts/dev-vscode.sh
```

## How It Works

```
.fl source → Lexer → Parser → CST → Type Checker → Codegen → .tsx output
```

The compiler is a single Rust binary that takes `.fl` files and emits `.tsx`. From there, your existing build toolchain (Vite, Next.js, etc.) picks it up like any other TypeScript file.

The parser is handwritten recursive descent (no parser generators) for better error recovery and LSP integration.

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

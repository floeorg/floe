<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Rust Style & Conventions

## Module layout
- **No `mod.rs`** — use modern module layout (paired `.rs` file next to directory)

## Naming
- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`

## Control flow
- Early returns and guard clauses
- Use `?` operator for error propagation — never `.unwrap()` in production code
- Match arms should be concise

## Types
- Newtypes for domain IDs (e.g. `struct TaskId(String)`)
- Enums over stringly-typed fields — make invalid states unrepresentable
- Prefer `&str` over `String` in function parameters when ownership isn't needed

## Functions
- Keep short — one level of abstraction per function
- Prefer functional iterator chains but don't exceed 6 combinators

## Async
- Never block the tokio runtime — no `std::thread::sleep`, use `tokio::time::sleep`
- Use `tokio::spawn` for concurrent work, `JoinSet` for managing multiple tasks

## Imports
- Order: `std` -> external crates -> `crate::`
- Explicit imports, not glob (`use foo::*`)

## Error handling
- `thiserror` for domain/application/infrastructure error types
- `anyhow` only at edges (`main.rs`, CLI entry points)

## Local build config
- Do NOT commit `.cargo/config.toml` to projects. Pinning `linker` / `-fuse-ld=...` breaks CI (cross containers, release runners without mold, etc.).
- Devs who want fast linkers or a shared `target-dir` should put it in their user-level `~/.cargo/config.toml` — cargo merges user + project config, so per-machine setup stays out of the repo.

<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Monorepo Structure

## Rules

### Workspace root
The root `Cargo.toml` is a workspace definition only — no `[package]` section, no code.

### apps/
Each directory under `apps/` is a deployable artifact (API server, migration runner, client app, etc.).

### packages/
Shared library crates following clean architecture:
- **domain** — models, value objects, repository traits (no external dependencies)
- **application** — use cases, business logic (depends on domain)
- **infrastructure** — database entities/repositories, external API clients (depends on domain + application)

### cargo commands
Always specify the package or run from the workspace root:
```bash
cargo check --workspace             # Type check everything
cargo build --release -p api        # Release build of a specific app
```

### Frontend commands
Run from the app directory:
```bash
pnpm dev                            # Dev server
pnpm check                          # Lint + typecheck
pnpm lint:fix                       # Auto-fix lint issues
pnpm typecheck                      # TypeScript type check
```

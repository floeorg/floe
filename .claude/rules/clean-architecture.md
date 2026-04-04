<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Clean Architecture (Rust)

## Layer structure

```
packages/
├── domain/          # Models, value objects, repository traits, pure services
├── application/     # Use cases — orchestrate domain logic
└── infrastructure/  # DB, external APIs, implementations
apps/
└── api/src/
    └── presentation/  # HTTP handlers (axum) — thin layer
```

## Dependency rule

Dependencies point INWARD only:
- `domain` imports NOTHING from other layers
- `application` imports `domain` only
- `infrastructure` imports `domain` (and optionally `application`)
- `presentation` imports `application` and `domain`

Never import infrastructure from domain or application.

## Module convention

Use directory-based modules with a paired `.rs` file:
```
models/
  task.rs
  user.rs
models.rs          # pub mod task; pub mod user;
```

NOT `mod.rs` inside directories.

## Repository pattern (MANDATORY)

All database access goes through repository traits:
- **Trait** in `domain/repositories/` — defines the interface
- **Implementation** in `infrastructure/` — uses the ORM/driver
- **Handler** constructs the repository and calls methods — never raw SQL in handlers

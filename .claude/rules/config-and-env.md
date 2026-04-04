<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Configuration & Environment

- All configuration via environment variables
- `.env` is git-ignored, `.env.example` is committed with placeholders
- Rule: deployment environment (dev/staging/prod) -> `.env`, per-user config -> database
- Never put secrets in committed files, never default secrets in code

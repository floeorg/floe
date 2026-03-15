---
title: CLI Reference
---

The Floe compiler is a single binary called `floe`.

## Commands

### `floe build`

Compile `.fl` files to TypeScript.

```bash
# Compile a single file
floe build src/main.fl

# Compile a directory
floe build src/

# Specify output directory
floe build src/ --out-dir dist/
```

The compiler automatically chooses `.ts` or `.tsx` based on whether the file contains JSX.

### `floe check`

Type-check files without generating output.

```bash
floe check src/
floe check src/main.fl
```

### `floe test`

Run inline test blocks.

```bash
floe test src/
floe test src/math.fl
```

Discovers all `test` blocks in `.fl` files, compiles them in test mode, and executes them. Requires a TypeScript runner (`tsx`) to be installed.

```bash
npm install -g tsx
```

### `floe watch`

Watch files and recompile on change.

```bash
floe watch src/
floe watch src/ --out-dir dist/
```

### `floe init`

Scaffold a new Floe project.

```bash
# In current directory
floe init

# In a new directory
floe init my-app
```

Creates:
- `src/main.fl` — sample Floe file
- `tsconfig.json` — TypeScript configuration

### `floe lsp`

Start the language server on stdin/stdout.

```bash
floe lsp
```

Used by editor extensions. You don't typically run this directly.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Compilation error (parse or type error) |
| 2 | File not found or I/O error |

## Environment

| Variable | Description |
|----------|-------------|
| `FLOE_FILENAME` | Override the filename shown in diagnostics |

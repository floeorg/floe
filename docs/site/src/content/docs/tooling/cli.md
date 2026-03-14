---
title: CLI Reference
---

The ZenScript compiler is a single binary called `zsc`.

## Commands

### `zsc build`

Compile `.zs` files to TypeScript.

```bash
# Compile a single file
zsc build src/main.zs

# Compile a directory
zsc build src/

# Specify output directory
zsc build src/ --out-dir dist/
```

The compiler automatically chooses `.ts` or `.tsx` based on whether the file contains JSX.

### `zsc check`

Type-check files without generating output.

```bash
zsc check src/
zsc check src/main.zs
```

### `zsc watch`

Watch files and recompile on change.

```bash
zsc watch src/
zsc watch src/ --out-dir dist/
```

### `zsc init`

Scaffold a new ZenScript project.

```bash
# In current directory
zsc init

# In a new directory
zsc init my-app
```

Creates:
- `src/main.zs` — sample ZenScript file
- `tsconfig.json` — TypeScript configuration

### `zsc lsp`

Start the language server on stdin/stdout.

```bash
zsc lsp
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
| `ZSC_FILENAME` | Override the filename shown in diagnostics |

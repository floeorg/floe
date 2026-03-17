---
title: VS Code Extension
---

The Floe VS Code extension provides syntax highlighting, LSP integration, and code snippets.

## Installation

### From Marketplace

Search for "Floe" in the VS Code extensions panel.

### From Source

```bash
cd editors/vscode
npm install
npm run build
# Then install the .vsix file
```

## Features

### Syntax Highlighting

Full TextMate grammar for `.fl` files:
- Keywords (`const`, `fn`, `match`, `type`, etc.)
- Operators (`|>`, `->`, `fn()`, `?`)
- JSX elements and attributes
- Template literals with interpolation
- Banned keyword highlighting (visual warning for `let`, `class`, etc.)

### Language Server

Full IDE features powered by `floe lsp`:

- **Diagnostics** - parse errors, type errors, unused variable/import warnings
- **Hover** - type signatures and documentation
- **Completions** - symbols, keywords, builtins, cross-file with auto-import
- **Pipe-aware autocomplete** - type `|>` and see functions that match the piped type
- **Go to Definition** - jump to symbol definitions across files
- **Find References** - find all usages of a symbol
- **Document Symbols** - outline view of functions, types, and constants
- **Quick Fix: Add return type** - auto-insert inferred return type on exported functions

### Snippets

| Prefix | Description |
|--------|-------------|
| `fn` | Function declaration |
| `efn` | Exported function |
| `match` | Match expression |
| `matchr` | Match on Result |
| `matcho` | Match on Option |
| `type` | Record type |
| `union` | Union type |
| `comp` | React component |
| `imp` | Import statement |
| `pipe` | Pipe expression |
| `co` | Const declaration |
| `brand` | Brand type |
| `opaque` | Opaque type |

## Configuration

| Setting | Description | Default |
|---------|-------------|---------|
| `floe.serverPath` | Path to the `floe` binary | `"floe"` |

## Troubleshooting

**Diagnostics not showing:** Make sure `floe` is installed and in your PATH. Check `floe.serverPath` in settings.

**Extension not activating:** Ensure the file has a `.fl` extension. The extension activates on the `floe` language ID.

---
title: VS Code Extension
---

The ZenScript VS Code extension provides syntax highlighting, LSP integration, and code snippets.

## Installation

### From Marketplace

Search for "ZenScript" in the VS Code extensions panel.

### From Source

```bash
cd editors/vscode
npm install
npm run build
# Then install the .vsix file
```

## Features

### Syntax Highlighting

Full TextMate grammar for `.zs` files:
- Keywords (`const`, `function`, `match`, `type`, etc.)
- Operators (`|>`, `->`, `=>`, `?`)
- JSX elements and attributes
- Template literals with interpolation
- Banned keyword highlighting (visual warning for `let`, `class`, etc.)

### Language Server

Real-time diagnostics as you type:
- Parse errors
- Type errors
- Unused variable/import warnings
- Banned keyword errors

Hover information for built-in types and constructors.

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
| `zenscript.serverPath` | Path to the `zsc` binary | `"zsc"` |

## Troubleshooting

**Diagnostics not showing:** Make sure `zsc` is installed and in your PATH. Check `zenscript.serverPath` in settings.

**Extension not activating:** Ensure the file has a `.zs` extension. The extension activates on the `zenscript` language ID.

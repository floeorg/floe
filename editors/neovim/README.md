# Floe for Neovim

## Quick Setup

### 1. File detection

Add to your Neovim config (`init.lua` or a file in `after/ftdetect/`):

```lua
vim.filetype.add({
  extension = {
    fl = "floe",
  },
})
```

Or copy `ftdetect/floe.lua` into `~/.config/nvim/ftdetect/`.

### 2. LSP configuration

Using **nvim-lspconfig** (recommended):

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

-- Register the Floe LSP if not already defined
if not configs.floe then
  configs.floe = {
    default_config = {
      cmd = { "floe", "lsp" },
      filetypes = { "floe" },
      root_dir = lspconfig.util.root_pattern("floe.toml", ".git"),
      settings = {},
    },
  }
end

lspconfig.floe.setup({})
```

Without nvim-lspconfig (built-in `vim.lsp.start`):

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "floe",
  callback = function()
    vim.lsp.start({
      name = "floe",
      cmd = { "floe", "lsp" },
      root_dir = vim.fs.dirname(vim.fs.find({ "floe.toml", ".git" }, { upward = true })[1]),
    })
  end,
})
```

### 3. Syntax highlighting

#### Option A: Tree-sitter (recommended)

Neovim has native tree-sitter support via [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter). Since the Floe parser is not yet in the nvim-treesitter registry, you need to register it manually.

Add this to your Neovim config:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()

parser_config.floe = {
  install_info = {
    url = "https://github.com/milkyskies/zenscript",
    location = "editors/tree-sitter-floe",
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "floe",
}
```

Then install the parser:

```
:TSInstall floe
```

Finally, copy the query files into your Neovim runtime path so tree-sitter knows how to highlight Floe:

```bash
# From the repo root:
cp -r editors/neovim/queries/floe ~/.config/nvim/queries/floe
```

This copies `queries/floe/highlights.scm` which tells tree-sitter how to map AST nodes to highlight groups.

## Features

Once configured, you get:

- **Diagnostics** - parse and type errors shown inline
- **Hover** - type info and documentation on hover (`K`)
- **Completions** - symbols, keywords, builtins, cross-file with auto-import
- **Go to Definition** - jump to symbol definition (`gd`)
- **Find References** - find all usages (`gr`)
- **Document Symbols** - outline view (`:Telescope lsp_document_symbols` or similar)

## Requirements

- `floe` must be in your `$PATH` (install via `cargo install floe` or build from source)
- Neovim 0.8+ (for native LSP support)
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) (for tree-sitter highlighting)

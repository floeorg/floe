---
title: Neovim
---

Floe ships as a Neovim plugin, `floe.nvim`, that configures filetype detection, LSP, and tree-sitter highlighting in one step.

## Install

Using [lazy.nvim](https://github.com/folke/lazy.nvim):

```lua
{
  "floeorg/floe",
  dir = vim.fn.stdpath("data") .. "/lazy/floe/editors/neovim",
  opts = {},
  main = "floe",
  ft = "floe",
  dependencies = { "nvim-treesitter/nvim-treesitter" },
}
```

Using [packer.nvim](https://github.com/wbthomason/packer.nvim):

```lua
use({
  "floeorg/floe",
  rtp = "editors/neovim",
  config = function()
    require("floe").setup({})
  end,
  requires = { "nvim-treesitter/nvim-treesitter" },
})
```

Using [vim-plug](https://github.com/junegunn/vim-plug):

```vim
Plug 'floeorg/floe', { 'rtp': 'editors/neovim' }

lua require('floe').setup({})
```

The plugin lives under `editors/neovim/` inside the main Floe repository, so the plugin-manager specs above pin `rtp` / `dir` to that subdirectory.

`setup({})` replaces all of the manual steps that were previously required (filetype registration, LSP autocmd, tree-sitter parser configuration, query files, `:TSInstall`).

## Configuration

All options are optional:

```lua
require("floe").setup({
  cmd = { "floe", "lsp" },              -- command that starts the LSP
  root_markers = { "floe.toml", ".git" }, -- files used to locate the project root
  auto_install_parser = true,            -- run :TSInstall floe on first .fl open
  on_attach = nil,                       -- function(client, bufnr) for keymaps
  capabilities = nil,                    -- override LSP capabilities (e.g. nvim-cmp)
})
```

### Dev builds

Point `cmd` at a locally built compiler:

```lua
require("floe").setup({
  cmd = { "/path/to/floe/target/debug/floe", "lsp" },
})
```

### Integrating with nvim-cmp

```lua
require("floe").setup({
  capabilities = require("cmp_nvim_lsp").default_capabilities(),
  on_attach = function(_, bufnr)
    vim.keymap.set("n", "K", vim.lsp.buf.hover, { buffer = bufnr })
    vim.keymap.set("n", "gd", vim.lsp.buf.definition, { buffer = bufnr })
  end,
})
```

## Health check

Run `:checkhealth floe` to verify your setup. It reports the status of the `floe` binary, filetype registration, highlight queries on the runtime path, and the tree-sitter parser.

## Features

All LSP features work out of the box:

- **Diagnostics** - inline errors and warnings
- **Hover** (`K`) - type signatures and documentation
- **Completions** (`<C-x><C-o>`) - symbols, keywords, pipe-aware autocomplete
- **Go to Definition** (`gd`)
- **Find References** (`gr`)
- **Document Symbols** - works with Telescope, fzf-lua, etc.
- **Quick Fix** - auto-insert return types on exported functions
- **Syntax highlighting** via tree-sitter

## Requirements

- `floe` on your `$PATH`
- Neovim 0.9+
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter)

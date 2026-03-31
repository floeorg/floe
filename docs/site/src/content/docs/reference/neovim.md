---
title: Neovim
---

Floe works with Neovim's built-in LSP client. No plugins required beyond a standard Neovim setup.

## Setup

Add to your `init.lua`:

```lua
-- Register .fl files
vim.filetype.add({ extension = { fl = "floe" } })

-- Start the LSP
vim.api.nvim_create_autocmd("FileType", {
  pattern = "floe",
  callback = function()
    vim.lsp.start({
      name = "floe",
      cmd = { "floe", "lsp" },
      root_dir = vim.fs.dirname(
        vim.fs.find({ ".git" }, { upward = true })[1]
      ),
    })
  end,
})
```

### With [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

if not configs.floe then
  configs.floe = {
    default_config = {
      cmd = { "floe", "lsp" },
      filetypes = { "floe" },
      root_dir = lspconfig.util.root_pattern(".git"),
    },
  }
end

lspconfig.floe.setup({})
```

## Syntax Highlighting

Neovim uses tree-sitter for highlighting. Since the Floe parser is not yet in the nvim-treesitter registry, register it manually:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()

parser_config.floe = {
  install_info = {
    url = "https://github.com/floeorg/floe",
    location = "editors/tree-sitter-floe",
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "floe",
}
```

Install the parser and copy the highlight queries:

```bash
# In Neovim:
:TSInstall floe

# From the repo root:
cp -r editors/neovim/queries/floe ~/.config/nvim/queries/floe
```

## Features

All LSP features work out of the box:

- **Diagnostics** - inline errors and warnings
- **Hover** (`K`) - type signatures and docs
- **Completions** (`<C-x><C-o>`) - symbols, keywords, pipe-aware autocomplete
- **Go to Definition** (`gd`)
- **Find References** (`gr`)
- **Document Symbols** - works with Telescope, fzf, etc.
- **Quick Fix** - auto-insert return types on exported functions

## Requirements

- `floe` in your `$PATH` (`cargo install floe` or build from source)
- Neovim 0.8+
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) (for syntax highlighting)

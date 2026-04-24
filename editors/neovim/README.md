# floe.nvim

Neovim plugin for [Floe](https://floe.dev): filetype detection, LSP, and tree-sitter highlighting.

## Requirements

- Neovim 0.9+
- [`floe`](https://floe.dev) on your `$PATH`
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) (for syntax highlighting)

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

That single `setup({})` call replaces the five manual steps (filetype registration, LSP autocmd, tree-sitter parser config, query files, `:TSInstall`).

## Configuration

All options are optional. Defaults:

```lua
require("floe").setup({
  cmd = { "floe", "lsp" },             -- command to start the LSP
  root_markers = { "floe.toml", ".git" }, -- used to locate project root
  auto_install_parser = true,           -- run :TSInstall floe on first .fl open
  on_attach = nil,                      -- function(client, bufnr) for keymaps
  capabilities = nil,                   -- override LSP capabilities (e.g. nvim-cmp)
})
```

### Dev builds

Point `cmd` at a local build:

```lua
require("floe").setup({
  cmd = { "/path/to/floe/target/debug/floe", "lsp" },
})
```

### Integrate with nvim-cmp

```lua
require("floe").setup({
  capabilities = require("cmp_nvim_lsp").default_capabilities(),
  on_attach = function(_, bufnr)
    vim.keymap.set("n", "K", vim.lsp.buf.hover, { buffer = bufnr })
    vim.keymap.set("n", "gd", vim.lsp.buf.definition, { buffer = bufnr })
  end,
})
```

## Tree-sitter parser

### nvim-treesitter master branch (legacy)

`:TSInstall floe` — floe.nvim registers the parser with `nvim-treesitter` and `auto_install_parser = true` runs the install on first open.

### nvim-treesitter main branch (v1.x — current LazyVim default)

`nvim-treesitter` v1.x only installs parsers from its upstream registry (follow-up #1346 tracks submission). Until then, build and drop the parser on your runtime path:

```bash
cd <floe-repo>/editors/tree-sitter-floe
cc -shared -fPIC -I src -o ~/.local/share/nvim/site/parser/floe.so src/parser.c
```

floe.nvim will auto-attach tree-sitter highlighting via `vim.treesitter.start` once the parser is available on the runtime path — no nvim-treesitter-specific configuration needed.

## Health check

Run `:checkhealth floe` to verify:

- `floe` is on `$PATH` and executable
- `.fl` filetype is registered
- highlight queries are on the runtime path
- tree-sitter parser is installed

## Features

- **Diagnostics** - parse and type errors shown inline
- **Hover** (`K`) - type signatures and documentation
- **Completions** - symbols, keywords, builtins, cross-file auto-import
- **Go to Definition** (`gd`)
- **Find References** (`gr`)
- **Document Symbols** - outline for Telescope / fzf-lua
- **Syntax highlighting** via tree-sitter

## Structure

```
editors/neovim/
├── lua/floe/
│   ├── init.lua          - setup() entry point
│   ├── filetype.lua      - .fl filetype registration
│   ├── lsp.lua           - LSP autocmd + vim.lsp.start
│   ├── treesitter.lua    - nvim-treesitter parser registration
│   └── health.lua        - :checkhealth floe
├── ftdetect/floe.lua     - fallback filetype registration
├── plugin/floe.lua       - loaded on startup (pre-setup filetype)
└── queries/floe/highlights.scm
```

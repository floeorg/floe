local M = {}

local defaults = {
  cmd = { "floe", "lsp" },
  root_markers = { "floe.toml", ".git" },
  auto_install_parser = true,
  on_attach = nil,
  capabilities = nil,
}

M.config = vim.deepcopy(defaults)

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", vim.deepcopy(defaults), opts or {})

  require("floe.filetype").setup()
  require("floe.treesitter").setup(M.config)
  require("floe.lsp").setup(M.config)
end

return M

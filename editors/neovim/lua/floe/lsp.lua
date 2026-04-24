local M = {}

function M.setup(config)
  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("FloeLsp", { clear = true }),
    pattern = "floe",
    callback = function(args)
      local root_file = vim.fs.find(config.root_markers, {
        upward = true,
        path = vim.api.nvim_buf_get_name(args.buf),
      })[1]
      local root_dir = root_file and vim.fs.dirname(root_file) or vim.fn.getcwd()

      vim.lsp.start({
        name = "floe",
        cmd = config.cmd,
        root_dir = root_dir,
        capabilities = config.capabilities,
        on_attach = config.on_attach,
      })
    end,
  })
end

return M

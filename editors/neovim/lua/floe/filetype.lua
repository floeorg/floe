local M = {}

function M.setup()
  vim.filetype.add({
    extension = {
      fl = "floe",
    },
  })
end

return M

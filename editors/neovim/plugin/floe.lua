if vim.g.loaded_floe then
  return
end
vim.g.loaded_floe = 1

require("floe.filetype").setup()

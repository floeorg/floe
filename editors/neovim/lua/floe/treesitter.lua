local M = {}

local function register_parser()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return false
  end

  local configs = parsers.get_parser_configs()
  if configs.floe then
    return true
  end

  configs.floe = {
    install_info = {
      url = "https://github.com/floeorg/floe",
      location = "editors/tree-sitter-floe",
      files = { "src/parser.c" },
      branch = "main",
    },
    filetype = "floe",
  }
  return true
end

local function parser_installed()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return false
  end
  return parsers.has_parser("floe")
end

function M.setup(config)
  if not register_parser() then
    return
  end

  if not config.auto_install_parser then
    return
  end

  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("FloeTreesitter", { clear = true }),
    pattern = "floe",
    once = true,
    callback = function()
      if parser_installed() then
        return
      end
      vim.schedule(function()
        vim.cmd("TSInstall floe")
      end)
    end,
  })
end

return M

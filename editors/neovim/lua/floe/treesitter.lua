local M = {}

local install_info = {
  url = "https://github.com/floeorg/floe",
  location = "editors/tree-sitter-floe",
  files = { "src/parser.c" },
  branch = "main",
}

local function register_with_legacy_nvim_treesitter()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok or type(parsers.get_parser_configs) ~= "function" then
    return false
  end
  local configs = parsers.get_parser_configs()
  if not configs.floe then
    configs.floe = { install_info = install_info, filetype = "floe" }
  end
  return true
end

local function parser_available()
  return #vim.api.nvim_get_runtime_file("parser/floe.so", true) > 0
end

local function start_highlighting(bufnr)
  if not parser_available() then
    return
  end
  vim.treesitter.language.register("floe", "floe")
  pcall(vim.treesitter.start, bufnr, "floe")
end

function M.setup(config)
  local on_legacy = register_with_legacy_nvim_treesitter()

  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("FloeTreesitter", { clear = true }),
    pattern = "floe",
    callback = function(args)
      start_highlighting(args.buf)
    end,
  })

  if not on_legacy or not config.auto_install_parser or parser_available() then
    return
  end

  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("FloeTreesitterInstall", { clear = true }),
    pattern = "floe",
    once = true,
    callback = function()
      vim.schedule(function()
        pcall(vim.cmd, "TSInstall floe")
      end)
    end,
  })
end

return M

local M = {}

local install_info = {
  url = "https://github.com/floeorg/floe",
  location = "editors/tree-sitter-floe",
  files = { "src/parser.c" },
  branch = "main",
}

local function load_nvim_treesitter()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return nil, nil
  end
  if type(parsers.get_parser_configs) == "function" then
    return "master", parsers
  end
  return "main", parsers
end

local function register_parser()
  local flavor, parsers = load_nvim_treesitter()
  if not flavor then
    return false
  end

  if flavor == "master" then
    local configs = parsers.get_parser_configs()
    if not configs.floe then
      configs.floe = { install_info = install_info, filetype = "floe" }
    end
  elseif rawget(parsers, "floe") == nil then
    parsers.floe = { install_info = install_info, filetype = "floe" }
  end
  return true
end

local function parser_installed()
  return #vim.api.nvim_get_runtime_file("parser/floe.so", true) > 0
end

function M.setup(config)
  if not register_parser() then
    return
  end

  if not config.auto_install_parser or parser_installed() then
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
        local ok = pcall(vim.cmd, "TSInstall floe")
        if not ok then
          vim.notify(
            "floe.nvim: could not run :TSInstall floe automatically. Install the parser manually.",
            vim.log.levels.WARN
          )
        end
      end)
    end,
  })
end

return M

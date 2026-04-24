local M = {}

local health = vim.health or require("health")
local h_start = health.start or health.report_start
local h_ok = health.ok or health.report_ok
local h_warn = health.warn or health.report_warn
local h_error = health.error or health.report_error
local h_info = health.info or health.report_info

local function check_binary(cmd)
  local bin = cmd[1]
  if vim.fn.executable(bin) ~= 1 then
    h_error(bin .. " not found in $PATH")
    h_info("Install Floe from https://floe.dev or build from source")
    return false
  end

  local version = vim.fn.system({ bin, "--version" })
  if vim.v.shell_error ~= 0 then
    h_error(bin .. " is on $PATH but `" .. bin .. " --version` failed")
    return false
  end

  h_ok(bin .. " found: " .. vim.trim(version))
  return true
end

local function check_treesitter()
  local parser_files = vim.api.nvim_get_runtime_file("parser/floe.so", true)
  if #parser_files > 0 then
    h_ok("tree-sitter parser: " .. parser_files[1])
    return
  end

  local has_nts, parsers = pcall(require, "nvim-treesitter.parsers")
  if has_nts and type(parsers.get_parser_configs) == "function" then
    h_warn("tree-sitter parser for floe is not installed", {
      "Run `:TSInstall floe` to install it",
    })
    return
  end

  h_warn("tree-sitter parser for floe is not installed", {
    "nvim-treesitter v1.x (main branch) does not install out-of-registry parsers.",
    "Build manually: cd <floe-repo>/editors/tree-sitter-floe && cc -shared -fPIC -I src -o ~/.local/share/nvim/site/parser/floe.so src/parser.c",
    "Or install nvim-treesitter master branch and run :TSInstall floe",
  })
end

local function check_queries()
  local files = vim.api.nvim_get_runtime_file("queries/floe/highlights.scm", true)
  if #files == 0 then
    h_error("queries/floe/highlights.scm not found on runtime path")
    h_info("Ensure floe.nvim is installed and on the runtime path")
    return
  end
  h_ok("highlight queries found: " .. files[1])
end

local function check_filetype()
  local ft = vim.filetype.match({ filename = "example.fl" })
  if ft == "floe" then
    h_ok(".fl files are registered as floe filetype")
  else
    h_error(".fl files are not registered as the floe filetype")
    h_info("Call `require('floe').setup()` in your config")
  end
end

function M.check()
  h_start("floe.nvim")
  local config = require("floe").config
  check_binary(config.cmd)
  check_filetype()
  check_queries()
  check_treesitter()
end

return M

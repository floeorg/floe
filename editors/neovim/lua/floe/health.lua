local M = {}

local health = vim.health or require("health")
local start = health.start or health.report_start
local ok = health.ok or health.report_ok
local warn = health.warn or health.report_warn
local error = health.error or health.report_error
local info = health.info or health.report_info

local function check_binary(cmd)
  local bin = cmd[1]
  if vim.fn.executable(bin) ~= 1 then
    error(bin .. " not found in $PATH")
    info("Install Floe from https://floe.dev or build from source")
    return false
  end

  local version = vim.fn.system({ bin, "--version" })
  if vim.v.shell_error ~= 0 then
    error(bin .. " is on $PATH but `" .. bin .. " --version` failed")
    return false
  end

  ok(bin .. " found: " .. vim.trim(version))
  return true
end

local function check_treesitter()
  local has_nts, parsers = pcall(require, "nvim-treesitter.parsers")
  if not has_nts then
    warn("nvim-treesitter is not installed", {
      "Install nvim-treesitter for syntax highlighting: https://github.com/nvim-treesitter/nvim-treesitter",
    })
    return
  end

  if not parsers.get_parser_configs().floe then
    error("Floe tree-sitter parser is not registered")
    info("Call `require('floe').setup()` in your config")
    return
  end

  if parsers.has_parser("floe") then
    ok("tree-sitter parser for floe is installed")
  else
    warn("tree-sitter parser for floe is not installed", {
      "Run `:TSInstall floe` to install it",
    })
  end
end

local function check_queries()
  local files = vim.api.nvim_get_runtime_file("queries/floe/highlights.scm", true)
  if #files == 0 then
    error("queries/floe/highlights.scm not found on runtime path")
    info("Ensure floe.nvim is installed and on the runtime path")
    return
  end
  ok("highlight queries found: " .. files[1])
end

local function check_filetype()
  local ft = vim.filetype.match({ filename = "example.fl" })
  if ft == "floe" then
    ok(".fl files are registered as floe filetype")
  else
    error(".fl files are not registered as the floe filetype")
    info("Call `require('floe').setup()` in your config")
  end
end

function M.check()
  start("floe.nvim")
  local config = require("floe").config
  check_binary(config.cmd)
  check_filetype()
  check_queries()
  check_treesitter()
end

return M

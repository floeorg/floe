# Changelog

All notable changes to Floe will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.1](https://github.com/milkyskies/floe/compare/floe-v0.1.0...floe-v0.1.1) (2026-03-28)


### Features

* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/milkyskies/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/milkyskies/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/milkyskies/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/milkyskies/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#404](https://github.com/milkyskies/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/milkyskies/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/milkyskies/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/milkyskies/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/milkyskies/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/milkyskies/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/milkyskies/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* validate named argument labels in function calls ([778395d](https://github.com/milkyskies/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))

## [Unreleased]

### Added
- Pipe operator (`|>`) with first-arg default and `_` placeholder
- Exhaustive pattern matching with `match` expressions
- Result (`Ok`/`Err`) and Option (`Some`/`None`) types
- `?` operator for Result/Option unwrapping
- Tagged unions with multi-depth matching
- Branded and opaque types
- Type constructors with named arguments and defaults
- Pipe lambdas (`|x| expr`) and dot shorthand (`.field`)
- JSX support with inline match and pipe expressions
- Language server with diagnostics, completions, and go-to-definition
- Code formatter (`floe fmt`)
- Vite plugin for dev/build integration
- VS Code extension with syntax highlighting and LSP
- Browser playground (WASM)
- `floe init` project scaffolding
- `floe watch` for auto-recompilation

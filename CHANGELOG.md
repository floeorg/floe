# Changelog

All notable changes to Floe will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.9](https://github.com/milkyskies/floe/compare/v0.1.8...v0.1.9) (2026-03-28)


### Features

* [[#294](https://github.com/milkyskies/floe/issues/294)] Add mock&lt;T&gt; compiler built-in for test data generation ([#473](https://github.com/milkyskies/floe/issues/473)) ([3614d2f](https://github.com/milkyskies/floe/commit/3614d2fef13adf93303e196697af341620d6359c))

## [0.1.8](https://github.com/milkyskies/floe/compare/v0.1.7...v0.1.8) (2026-03-28)


### Bug Fixes

* npm trusted publishing and Open VSX publisher/LICENSE ([#453](https://github.com/milkyskies/floe/issues/453)) ([1507f92](https://github.com/milkyskies/floe/commit/1507f927d57b39ba28da1bd4727ad8a3a3226a0e))

## [0.1.7](https://github.com/milkyskies/floe/compare/v0.1.6...v0.1.7) (2026-03-28)


### Bug Fixes

* add VS Code icon, fix npm publish, bump action versions ([#451](https://github.com/milkyskies/floe/issues/451)) ([7133ba2](https://github.com/milkyskies/floe/commit/7133ba27ab9e1606393c1af813b0cdf1db1c9df9))

## [0.1.6](https://github.com/milkyskies/floe/compare/v0.1.5...v0.1.6) (2026-03-28)


### Bug Fixes

* pass tag name to release workflow for correct ref checkout ([#448](https://github.com/milkyskies/floe/issues/448)) ([019745a](https://github.com/milkyskies/floe/commit/019745afa8c1dae84a6d03c7f087511c0b4450ad))

## [0.1.5](https://github.com/milkyskies/floe/compare/v0.1.4...v0.1.5) (2026-03-28)


### Bug Fixes

* add id-token permission for npm trusted publishing ([#445](https://github.com/milkyskies/floe/issues/445)) ([5eec3f0](https://github.com/milkyskies/floe/commit/5eec3f079f52fa662cb318274f6f1811688ad900))
* trigger release workflow directly from release-please ([#444](https://github.com/milkyskies/floe/issues/444)) ([3ff34a2](https://github.com/milkyskies/floe/commit/3ff34a20f6d55bc0dd112e235234c9f3fc0614e0))
* VS Code extension publisher and engine version for Open VSX ([#447](https://github.com/milkyskies/floe/issues/447)) ([5475113](https://github.com/milkyskies/floe/commit/54751135a93729c3b87f926785610c92398cee3d))

## [0.1.4](https://github.com/milkyskies/floe/compare/v0.1.3...v0.1.4) (2026-03-28)


### Bug Fixes

* correct ignoreDeprecations value to 6.0 for TypeScript 7 ([#440](https://github.com/milkyskies/floe/issues/440)) ([505d6fb](https://github.com/milkyskies/floe/commit/505d6fb4d55ea06d7c986267fda086e508ed1d0b))

## [0.1.3](https://github.com/milkyskies/floe/compare/v0.1.2...v0.1.3) (2026-03-28)


### Features

* [[#422](https://github.com/milkyskies/floe/issues/422)] Generate .d.ts stubs so TS resolves .fl imports ([#429](https://github.com/milkyskies/floe/issues/429)) ([95c0f12](https://github.com/milkyskies/floe/commit/95c0f12f3132fd06ae029dab95f2e775250cb09c))


### Bug Fixes

* stop release workflow from overwriting release-please changelog ([#430](https://github.com/milkyskies/floe/issues/430)) ([b7d5d14](https://github.com/milkyskies/floe/commit/b7d5d14ba9497512e66da25cec0d7884a6f36fe7))

## [0.1.2](https://github.com/milkyskies/floe/compare/v0.1.1...v0.1.2) (2026-03-28)


### Features

* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/milkyskies/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/milkyskies/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/milkyskies/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/milkyskies/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#403](https://github.com/milkyskies/floe/issues/403)] Improve LSP hover information across the board ([03e512b](https://github.com/milkyskies/floe/commit/03e512b0e4a411387583fc69f0c0c8e20a9ed2bc))
* [[#404](https://github.com/milkyskies/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/milkyskies/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/milkyskies/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/milkyskies/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/milkyskies/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/milkyskies/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* improve LSP hover information across the board ([a9adeb7](https://github.com/milkyskies/floe/commit/a9adeb79809833e23cfaa8b3ff77a9308fd17fc4))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/milkyskies/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* use plain v* tags instead of floe-v* for releases ([#425](https://github.com/milkyskies/floe/issues/425)) ([bc53113](https://github.com/milkyskies/floe/commit/bc5311340d2450b1fa4883605c5528deb526dfa7))
* validate named argument labels in function calls ([778395d](https://github.com/milkyskies/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))

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

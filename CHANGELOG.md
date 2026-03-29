# Changelog

All notable changes to Floe will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.15](https://github.com/milkyskies/floe/compare/v0.1.14...v0.1.15) (2026-03-29)


### Bug Fixes

* [[#507](https://github.com/milkyskies/floe/issues/507)] LSP hover on 'from' keyword shows Array.from instead of import syntax ([#531](https://github.com/milkyskies/floe/issues/531)) ([95eaef6](https://github.com/milkyskies/floe/commit/95eaef694ac73ef99b4826172a77986789e70395))
* [[#533](https://github.com/milkyskies/floe/issues/533)] LSP completion gaps - dot-access fields, import paths, context suppression ([#535](https://github.com/milkyskies/floe/issues/535)) ([78251c1](https://github.com/milkyskies/floe/commit/78251c15c48cc5098b4b075c9406ebcba5f17f43))
* [[#536](https://github.com/milkyskies/floe/issues/536)] Resolve correct for-block overload based on receiver type ([#539](https://github.com/milkyskies/floe/issues/539)) ([09b454a](https://github.com/milkyskies/floe/commit/09b454af14e4b6d34d266b46d3c7b5ef55e4f5c9))
* [[#537](https://github.com/milkyskies/floe/issues/537)] Resolve typeof function types from npm imports ([#549](https://github.com/milkyskies/floe/issues/549)) ([588f91a](https://github.com/milkyskies/floe/commit/588f91ad9cf92ce48a295ea19f35b4c4602d428d))
* [[#538](https://github.com/milkyskies/floe/issues/538)] Validate argument count for stdlib method calls ([#541](https://github.com/milkyskies/floe/issues/541)) ([379f613](https://github.com/milkyskies/floe/commit/379f613388485c55d4d7b887c4f478d059b65b7e))
* [[#540](https://github.com/milkyskies/floe/issues/540)] Console.log/warn/error/info/debug should be variadic ([#545](https://github.com/milkyskies/floe/issues/545)) ([d119510](https://github.com/milkyskies/floe/commit/d11951001631cd83434a0db3a2b157fa5aee8408))
* [[#550](https://github.com/milkyskies/floe/issues/550)] LSP test script sleeps full timeout on every collect_notifications call ([#551](https://github.com/milkyskies/floe/issues/551)) ([7c74d16](https://github.com/milkyskies/floe/commit/7c74d1612e6ed38798a9e2c7cf57d36e23a6ba7f))

## [0.1.14](https://github.com/milkyskies/floe/compare/v0.1.13...v0.1.14) (2026-03-29)


### Features

* [[#294](https://github.com/milkyskies/floe/issues/294)] Add mock&lt;T&gt; compiler built-in for test data generation ([#473](https://github.com/milkyskies/floe/issues/473)) ([3614d2f](https://github.com/milkyskies/floe/commit/3614d2fef13adf93303e196697af341620d6359c))
* [[#422](https://github.com/milkyskies/floe/issues/422)] Generate .d.ts stubs so TS resolves .fl imports ([#429](https://github.com/milkyskies/floe/issues/429)) ([95c0f12](https://github.com/milkyskies/floe/commit/95c0f12f3132fd06ae029dab95f2e775250cb09c))
* [[#475](https://github.com/milkyskies/floe/issues/475)] Add default values for type fields ([#479](https://github.com/milkyskies/floe/issues/479)) ([57bd5b8](https://github.com/milkyskies/floe/commit/57bd5b821109ae813e73350da126d92ef8d054f1))
* [[#498](https://github.com/milkyskies/floe/issues/498)] Output compiled files to .floe/ directory instead of alongside source ([#502](https://github.com/milkyskies/floe/issues/502)) ([3821854](https://github.com/milkyskies/floe/commit/38218543c57c851bea3e2153d3129213163feeac))
* [[#499](https://github.com/milkyskies/floe/issues/499)] Auto-detect x?: T | null in .d.ts imports as Settable&lt;T&gt; ([#508](https://github.com/milkyskies/floe/issues/508)) ([7e166a7](https://github.com/milkyskies/floe/commit/7e166a7478b8ded5e5fc8c71071921264e25945a))
* [[#509](https://github.com/milkyskies/floe/issues/509)] Add Date module to stdlib ([#517](https://github.com/milkyskies/floe/issues/517)) ([e8fc12a](https://github.com/milkyskies/floe/commit/e8fc12a9c7a4def5ff5401f289439432edef29a1))
* [[#511](https://github.com/milkyskies/floe/issues/511)] Resolve types from local .ts/.tsx files imported in .fl files ([#515](https://github.com/milkyskies/floe/issues/515)) ([257a27a](https://github.com/milkyskies/floe/commit/257a27aac3d0e186d10a656c16c772ed26055d9b))
* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/milkyskies/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/milkyskies/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/milkyskies/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/milkyskies/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#403](https://github.com/milkyskies/floe/issues/403)] Improve LSP hover information across the board ([03e512b](https://github.com/milkyskies/floe/commit/03e512b0e4a411387583fc69f0c0c8e20a9ed2bc))
* [[#404](https://github.com/milkyskies/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/milkyskies/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/milkyskies/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/milkyskies/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* [[#480](https://github.com/milkyskies/floe/issues/480)] Fix docs build and Open VSX publish CI failures ([#481](https://github.com/milkyskies/floe/issues/481)) ([c95af9c](https://github.com/milkyskies/floe/commit/c95af9c2bc422f09fe5630b5e80ac960451b5f98))
* [[#486](https://github.com/milkyskies/floe/issues/486)] Widen vite-plugin peer dependency to support Vite 7 and 8 ([#487](https://github.com/milkyskies/floe/issues/487)) ([27eae45](https://github.com/milkyskies/floe/commit/27eae45a75a1f29bb3f7209e6aa2285c2c278cac))
* [[#489](https://github.com/milkyskies/floe/issues/489)] Bundle VS Code extension with esbuild, fix icon, add restart command ([#490](https://github.com/milkyskies/floe/issues/490)) ([abc9beb](https://github.com/milkyskies/floe/commit/abc9bebb75b8fd1f318c4ad93e2cba7876d8cd11))
* [[#491](https://github.com/milkyskies/floe/issues/491)] Support JSX comments {/* ... */} ([#497](https://github.com/milkyskies/floe/issues/497)) ([974f37f](https://github.com/milkyskies/floe/commit/974f37fe9fe62d6e185f32cebc5c3e8976ae47e9))
* [[#492](https://github.com/milkyskies/floe/issues/492)] Fix JSX formatter to add newlines around match expressions and multi-line tag children ([#500](https://github.com/milkyskies/floe/issues/500)) ([a7ac4d5](https://github.com/milkyskies/floe/commit/a7ac4d561d7c372913a22e2ada74d3bdaf2f1b9a))
* [[#494](https://github.com/milkyskies/floe/issues/494)] Add resolveId hook to vite plugin for .fl import resolution ([#495](https://github.com/milkyskies/floe/issues/495)) ([963c97e](https://github.com/milkyskies/floe/commit/963c97e1cdf409fb1970144407612c3b3831b824))
* [[#501](https://github.com/milkyskies/floe/issues/501)] Tell Vite that compiled .fl output is TypeScript ([#505](https://github.com/milkyskies/floe/issues/505)) ([4fd6475](https://github.com/milkyskies/floe/commit/4fd6475e2a748a9edb2e4719cb2af78323792dbf))
* [[#506](https://github.com/milkyskies/floe/issues/506)] LSP resolves tsconfig path aliases instead of reporting false errors ([#510](https://github.com/milkyskies/floe/issues/510)) ([2facdc6](https://github.com/milkyskies/floe/commit/2facdc6d39ec1737c77465e1fc83ec5ead56af76))
* [[#512](https://github.com/milkyskies/floe/issues/512)] Vite plugin cross-version type compatibility and .d.fl.ts output ([#514](https://github.com/milkyskies/floe/issues/514)) ([be4cb66](https://github.com/milkyskies/floe/commit/be4cb662e9def60060434f29dd2bd34082f8bed1))
* [[#512](https://github.com/milkyskies/floe/issues/512)] Write .d.fl.ts next to source and emit from --emit-stdout ([#519](https://github.com/milkyskies/floe/issues/519)) ([1dfda9d](https://github.com/milkyskies/floe/commit/1dfda9d142f8e352264225fdd7e22544bd12f127))
* [[#516](https://github.com/milkyskies/floe/issues/516)] For-block functions from different types clash when both imported ([#518](https://github.com/milkyskies/floe/issues/518)) ([3b418cb](https://github.com/milkyskies/floe/commit/3b418cb6ce5685ec352f82e768fd858ae37fcf85))
* [[#520](https://github.com/milkyskies/floe/issues/520)] Option match uses null checks, probe preserves nullability ([#523](https://github.com/milkyskies/floe/issues/523)) ([ef5ee9b](https://github.com/milkyskies/floe/commit/ef5ee9b69f2973e853fd9d8a8bdf86ece392c1ca))
* [[#521](https://github.com/milkyskies/floe/issues/521)] Formatter deletes // comments inside blocks ([#522](https://github.com/milkyskies/floe/issues/522)) ([389d6e0](https://github.com/milkyskies/floe/commit/389d6e042453a84a179fa2fd435d9c8434287714))
* [[#525](https://github.com/milkyskies/floe/issues/525)] Break after -&gt; in match arms when JSX body has multiline props ([#528](https://github.com/milkyskies/floe/issues/528)) ([f48ac28](https://github.com/milkyskies/floe/commit/f48ac289619660599645fb699f659355bbe64402))
* add id-token permission for npm trusted publishing ([#445](https://github.com/milkyskies/floe/issues/445)) ([5eec3f0](https://github.com/milkyskies/floe/commit/5eec3f079f52fa662cb318274f6f1811688ad900))
* add VS Code icon, fix npm publish, bump action versions ([#451](https://github.com/milkyskies/floe/issues/451)) ([7133ba2](https://github.com/milkyskies/floe/commit/7133ba27ab9e1606393c1af813b0cdf1db1c9df9))
* correct ignoreDeprecations value to 6.0 for TypeScript 7 ([#440](https://github.com/milkyskies/floe/issues/440)) ([505d6fb](https://github.com/milkyskies/floe/commit/505d6fb4d55ea06d7c986267fda086e508ed1d0b))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/milkyskies/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/milkyskies/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* improve LSP hover information across the board ([a9adeb7](https://github.com/milkyskies/floe/commit/a9adeb79809833e23cfaa8b3ff77a9308fd17fc4))
* npm trusted publishing and Open VSX publisher/LICENSE ([#453](https://github.com/milkyskies/floe/issues/453)) ([1507f92](https://github.com/milkyskies/floe/commit/1507f927d57b39ba28da1bd4727ad8a3a3226a0e))
* pass tag name to release workflow for correct ref checkout ([#448](https://github.com/milkyskies/floe/issues/448)) ([019745a](https://github.com/milkyskies/floe/commit/019745afa8c1dae84a6d03c7f087511c0b4450ad))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/milkyskies/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* stop release workflow from overwriting release-please changelog ([#430](https://github.com/milkyskies/floe/issues/430)) ([b7d5d14](https://github.com/milkyskies/floe/commit/b7d5d14ba9497512e66da25cec0d7884a6f36fe7))
* trigger release workflow directly from release-please ([#444](https://github.com/milkyskies/floe/issues/444)) ([3ff34a2](https://github.com/milkyskies/floe/commit/3ff34a20f6d55bc0dd112e235234c9f3fc0614e0))
* use plain v* tags instead of floe-v* for releases ([#425](https://github.com/milkyskies/floe/issues/425)) ([bc53113](https://github.com/milkyskies/floe/commit/bc5311340d2450b1fa4883605c5528deb526dfa7))
* validate named argument labels in function calls ([778395d](https://github.com/milkyskies/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))
* VS Code extension publisher and engine version for Open VSX ([#447](https://github.com/milkyskies/floe/issues/447)) ([5475113](https://github.com/milkyskies/floe/commit/54751135a93729c3b87f926785610c92398cee3d))

## [0.1.13](https://github.com/milkyskies/floe/compare/v0.1.12...v0.1.13) (2026-03-29)


### Bug Fixes

* [[#520](https://github.com/milkyskies/floe/issues/520)] Option match uses null checks, probe preserves nullability ([#523](https://github.com/milkyskies/floe/issues/523)) ([ef5ee9b](https://github.com/milkyskies/floe/commit/ef5ee9b69f2973e853fd9d8a8bdf86ece392c1ca))
* [[#521](https://github.com/milkyskies/floe/issues/521)] Formatter deletes // comments inside blocks ([#522](https://github.com/milkyskies/floe/issues/522)) ([389d6e0](https://github.com/milkyskies/floe/commit/389d6e042453a84a179fa2fd435d9c8434287714))

## [0.1.12](https://github.com/milkyskies/floe/compare/v0.1.11...v0.1.12) (2026-03-28)


### Features

* [[#498](https://github.com/milkyskies/floe/issues/498)] Output compiled files to .floe/ directory instead of alongside source ([#502](https://github.com/milkyskies/floe/issues/502)) ([3821854](https://github.com/milkyskies/floe/commit/38218543c57c851bea3e2153d3129213163feeac))
* [[#499](https://github.com/milkyskies/floe/issues/499)] Auto-detect x?: T | null in .d.ts imports as Settable&lt;T&gt; ([#508](https://github.com/milkyskies/floe/issues/508)) ([7e166a7](https://github.com/milkyskies/floe/commit/7e166a7478b8ded5e5fc8c71071921264e25945a))
* [[#509](https://github.com/milkyskies/floe/issues/509)] Add Date module to stdlib ([#517](https://github.com/milkyskies/floe/issues/517)) ([e8fc12a](https://github.com/milkyskies/floe/commit/e8fc12a9c7a4def5ff5401f289439432edef29a1))
* [[#511](https://github.com/milkyskies/floe/issues/511)] Resolve types from local .ts/.tsx files imported in .fl files ([#515](https://github.com/milkyskies/floe/issues/515)) ([257a27a](https://github.com/milkyskies/floe/commit/257a27aac3d0e186d10a656c16c772ed26055d9b))


### Bug Fixes

* [[#492](https://github.com/milkyskies/floe/issues/492)] Fix JSX formatter to add newlines around match expressions and multi-line tag children ([#500](https://github.com/milkyskies/floe/issues/500)) ([a7ac4d5](https://github.com/milkyskies/floe/commit/a7ac4d561d7c372913a22e2ada74d3bdaf2f1b9a))
* [[#501](https://github.com/milkyskies/floe/issues/501)] Tell Vite that compiled .fl output is TypeScript ([#505](https://github.com/milkyskies/floe/issues/505)) ([4fd6475](https://github.com/milkyskies/floe/commit/4fd6475e2a748a9edb2e4719cb2af78323792dbf))
* [[#506](https://github.com/milkyskies/floe/issues/506)] LSP resolves tsconfig path aliases instead of reporting false errors ([#510](https://github.com/milkyskies/floe/issues/510)) ([2facdc6](https://github.com/milkyskies/floe/commit/2facdc6d39ec1737c77465e1fc83ec5ead56af76))
* [[#512](https://github.com/milkyskies/floe/issues/512)] Vite plugin cross-version type compatibility and .d.fl.ts output ([#514](https://github.com/milkyskies/floe/issues/514)) ([be4cb66](https://github.com/milkyskies/floe/commit/be4cb662e9def60060434f29dd2bd34082f8bed1))
* [[#512](https://github.com/milkyskies/floe/issues/512)] Write .d.fl.ts next to source and emit from --emit-stdout ([#519](https://github.com/milkyskies/floe/issues/519)) ([1dfda9d](https://github.com/milkyskies/floe/commit/1dfda9d142f8e352264225fdd7e22544bd12f127))
* [[#516](https://github.com/milkyskies/floe/issues/516)] For-block functions from different types clash when both imported ([#518](https://github.com/milkyskies/floe/issues/518)) ([3b418cb](https://github.com/milkyskies/floe/commit/3b418cb6ce5685ec352f82e768fd858ae37fcf85))

## [0.1.11](https://github.com/milkyskies/floe/compare/v0.1.10...v0.1.11) (2026-03-28)


### Features

* [[#475](https://github.com/milkyskies/floe/issues/475)] Add default values for type fields ([#479](https://github.com/milkyskies/floe/issues/479)) ([57bd5b8](https://github.com/milkyskies/floe/commit/57bd5b821109ae813e73350da126d92ef8d054f1))


### Bug Fixes

* [[#486](https://github.com/milkyskies/floe/issues/486)] Widen vite-plugin peer dependency to support Vite 7 and 8 ([#487](https://github.com/milkyskies/floe/issues/487)) ([27eae45](https://github.com/milkyskies/floe/commit/27eae45a75a1f29bb3f7209e6aa2285c2c278cac))
* [[#489](https://github.com/milkyskies/floe/issues/489)] Bundle VS Code extension with esbuild, fix icon, add restart command ([#490](https://github.com/milkyskies/floe/issues/490)) ([abc9beb](https://github.com/milkyskies/floe/commit/abc9bebb75b8fd1f318c4ad93e2cba7876d8cd11))
* [[#491](https://github.com/milkyskies/floe/issues/491)] Support JSX comments {/* ... */} ([#497](https://github.com/milkyskies/floe/issues/497)) ([974f37f](https://github.com/milkyskies/floe/commit/974f37fe9fe62d6e185f32cebc5c3e8976ae47e9))
* [[#494](https://github.com/milkyskies/floe/issues/494)] Add resolveId hook to vite plugin for .fl import resolution ([#495](https://github.com/milkyskies/floe/issues/495)) ([963c97e](https://github.com/milkyskies/floe/commit/963c97e1cdf409fb1970144407612c3b3831b824))

## [0.1.10](https://github.com/milkyskies/floe/compare/v0.1.9...v0.1.10) (2026-03-28)


### Bug Fixes

* [[#480](https://github.com/milkyskies/floe/issues/480)] Fix docs build and Open VSX publish CI failures ([#481](https://github.com/milkyskies/floe/issues/481)) ([c95af9c](https://github.com/milkyskies/floe/commit/c95af9c2bc422f09fe5630b5e80ac960451b5f98))

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

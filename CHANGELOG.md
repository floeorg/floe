# Changelog

All notable changes to Floe will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.34](https://github.com/floeorg/floe/compare/v0.1.33...v0.1.34) (2026-03-31)


### Bug Fixes

* [[#804](https://github.com/floeorg/floe/issues/804)] Suggest try await instead of await try when Result wraps Promise ([#813](https://github.com/floeorg/floe/issues/813)) ([e377ed6](https://github.com/floeorg/floe/commit/e377ed605539ac0168eb72181eb0eadd563a42ae))
* [[#806](https://github.com/floeorg/floe/issues/806)] Object literals with variant values inferred as unknown ([#808](https://github.com/floeorg/floe/issues/808)) ([b8c3b50](https://github.com/floeorg/floe/commit/b8c3b503de0d9db0f6da7b231892b59a33c45ddc))
* [[#811](https://github.com/floeorg/floe/issues/811)] Reject non-type values in type annotations ([#814](https://github.com/floeorg/floe/issues/814)) ([1ecfeec](https://github.com/floeorg/floe/commit/1ecfeece1352ac8173041d42ce0e09703c92831d))

## [0.1.33](https://github.com/floeorg/floe/compare/v0.1.32...v0.1.33) (2026-03-31)


### Features

* [[#758](https://github.com/floeorg/floe/issues/758)] Add Record.get to stdlib for dynamic key access on plain objects ([#794](https://github.com/floeorg/floe/issues/794)) ([6ec22d2](https://github.com/floeorg/floe/commit/6ec22d2c6df3ffe540970f5b2407e50a56033e14))
* [[#799](https://github.com/floeorg/floe/issues/799)] Show default values for optional parameters in function signatures ([#801](https://github.com/floeorg/floe/issues/801)) ([e0ea91a](https://github.com/floeorg/floe/commit/e0ea91a2ed3df64cef41fba8e90e5ede92065bb4))


### Bug Fixes

* [[#733](https://github.com/floeorg/floe/issues/733)] Dot shorthand works as general function arguments ([#793](https://github.com/floeorg/floe/issues/793)) ([92baa16](https://github.com/floeorg/floe/commit/92baa1682792f28961ac1ba12649579facbe9437))
* [[#734](https://github.com/floeorg/floe/issues/734)] Checker silently accepts unknown where concrete types are expected ([#800](https://github.com/floeorg/floe/issues/800)) ([e50cdbc](https://github.com/floeorg/floe/commit/e50cdbc2ab8fcf8ec019087fd6bdaf360298682f))
* [[#795](https://github.com/floeorg/floe/issues/795)] Remove throwing unwrap functions, add or/values/partition helpers ([#798](https://github.com/floeorg/floe/issues/798)) ([f3cc9a7](https://github.com/floeorg/floe/commit/f3cc9a709b2dc78a4e4deaa0213619009f8415a9))

## [0.1.32](https://github.com/floeorg/floe/compare/v0.1.31...v0.1.32) (2026-03-31)


### Features

* [[#767](https://github.com/floeorg/floe/issues/767)] Add positional union variant fields and newtype paren syntax ([#777](https://github.com/floeorg/floe/issues/777)) ([62f5d67](https://github.com/floeorg/floe/commit/62f5d67e23b740a915799697223077743a2be407))
* [[#771](https://github.com/floeorg/floe/issues/771)] Refactor Option from compiler intrinsic to regular union type ([#786](https://github.com/floeorg/floe/issues/786)) ([c74e699](https://github.com/floeorg/floe/commit/c74e6990768bb2ef07120b80c8f3410cc390ec0a))
* [[#772](https://github.com/floeorg/floe/issues/772)] Refactor Result from compiler intrinsic to regular union type ([#788](https://github.com/floeorg/floe/issues/788)) ([973e591](https://github.com/floeorg/floe/commit/973e5918dae081c3a73b39c7d7d8d3f0723693a4))


### Bug Fixes

* [[#778](https://github.com/floeorg/floe/issues/778)] Formatter drops keyword member names after dot ([#782](https://github.com/floeorg/floe/issues/782)) ([052c86a](https://github.com/floeorg/floe/commit/052c86abb973bb63d73cf8a017c5ea23d51a90b7))
* [[#779](https://github.com/floeorg/floe/issues/779)] Imported TS functions with optional params incorrectly require all arguments ([#783](https://github.com/floeorg/floe/issues/783)) ([92e7f95](https://github.com/floeorg/floe/commit/92e7f95ea1a100cfff60b13c4081444a3fbc3521))
* [[#781](https://github.com/floeorg/floe/issues/781)] Imported TS union types resolve to unknown instead of strict union ([#787](https://github.com/floeorg/floe/issues/787)) ([dbb94e3](https://github.com/floeorg/floe/commit/dbb94e39588be9312750c26204f330e0e90130d9))
* use pnpm install in docs deploy workflow ([9664f5d](https://github.com/floeorg/floe/commit/9664f5d1e23e5a234ee84f266b7098f982e9ed8d))

## [0.1.31](https://github.com/floeorg/floe/compare/v0.1.30...v0.1.31) (2026-03-31)


### Bug Fixes

* [[#755](https://github.com/floeorg/floe/issues/755)] [[#756](https://github.com/floeorg/floe/issues/756)] Support destructured parameters with type annotations and error on unsupported syntax ([#759](https://github.com/floeorg/floe/issues/759)) ([548b139](https://github.com/floeorg/floe/commit/548b13986c72d4bc6cc7dc8d2aeefd84eee477dd))
* [[#757](https://github.com/floeorg/floe/issues/757)] break multiline JSX arrow bodies onto indented lines ([#764](https://github.com/floeorg/floe/issues/764)) ([6de16f8](https://github.com/floeorg/floe/commit/6de16f83692bc529747e5a3a593fbedafa59070a))
* [[#760](https://github.com/floeorg/floe/issues/760)] Render prop callback parameters inferred as function type instead of individual params ([#766](https://github.com/floeorg/floe/issues/766)) ([87c2aa8](https://github.com/floeorg/floe/commit/87c2aa894b24a91d055ef9048ed062f6c96bad92))
* [[#762](https://github.com/floeorg/floe/issues/762)] Extract shared destructure resolution helper and fix silent Unknown in arrow params ([#765](https://github.com/floeorg/floe/issues/765)) ([8e7ed73](https://github.com/floeorg/floe/commit/8e7ed73892af4fcea43ddcc8f2e36ac1332b743e))

## [0.1.30](https://github.com/floeorg/floe/compare/v0.1.29...v0.1.30) (2026-03-31)


### Bug Fixes

* use mangled name for bare for-block function calls ([#747](https://github.com/floeorg/floe/issues/747)) ([c4e475f](https://github.com/floeorg/floe/commit/c4e475fb69e19f609f0bded23ab8efac26d5a710))

## [0.1.29](https://github.com/floeorg/floe/compare/v0.1.28...v0.1.29) (2026-03-31)


### Features

* [[#728](https://github.com/floeorg/floe/issues/728)] Infer callback parameter types from npm union prop types ([#731](https://github.com/floeorg/floe/issues/731)) ([f9bd450](https://github.com/floeorg/floe/commit/f9bd450cefe04fad54b628b04a9689f9118dba34))


### Bug Fixes

* [[#729](https://github.com/floeorg/floe/issues/729)] Formatter inconsistently breaks array elements with constructor calls ([#730](https://github.com/floeorg/floe/issues/730)) ([9999106](https://github.com/floeorg/floe/commit/9999106f937fe4809e581dc705442e2e280588d7))

## [0.1.28](https://github.com/floeorg/floe/compare/v0.1.27...v0.1.28) (2026-03-31)


### Bug Fixes

* transform JSX to JS in vite plugin before import analysis ([#726](https://github.com/floeorg/floe/issues/726)) ([12533dc](https://github.com/floeorg/floe/commit/12533dca6866a9f39e8dbaba78c5e5bce05826e3))

## [0.1.27](https://github.com/floeorg/floe/compare/v0.1.26...v0.1.27) (2026-03-30)


### Features

* [[#155](https://github.com/floeorg/floe/issues/155)] LSP autocomplete for type-directed pipe resolution ([#724](https://github.com/floeorg/floe/issues/724)) ([670508a](https://github.com/floeorg/floe/commit/670508ad0e2721f8df32b4e010f870350db62ac8))

## [0.1.26](https://github.com/floeorg/floe/compare/v0.1.25...v0.1.26) (2026-03-30)


### Features

* [[#597](https://github.com/floeorg/floe/issues/597)] Enforce boolean-only operands for &&, ||, and ! ([#689](https://github.com/floeorg/floe/issues/689)) ([157cd6a](https://github.com/floeorg/floe/commit/157cd6ae313443dd596287eea9cad42089bfe88e))
* [[#690](https://github.com/floeorg/floe/issues/690)] Support Record&lt;K, V&gt; from TS interop as Map with bracket-access codegen ([#698](https://github.com/floeorg/floe/issues/698)) ([fe30639](https://github.com/floeorg/floe/commit/fe3063929eb3ea6a1183356ae8301a539e9fa69f))


### Bug Fixes

* [[#585](https://github.com/floeorg/floe/issues/585)] Go-to-definition not working for tsconfig path alias imports ([#688](https://github.com/floeorg/floe/issues/688)) ([a4e88a1](https://github.com/floeorg/floe/commit/a4e88a173048cd7e5b7b0f54a1b5027e67a9bbcc))
* [[#593](https://github.com/floeorg/floe/issues/593)] For-block methods resolve via member access ([#699](https://github.com/floeorg/floe/issues/699)) ([22a8e78](https://github.com/floeorg/floe/commit/22a8e78d7005d779b18e969da1390f09e2eba9dc))
* [[#600](https://github.com/floeorg/floe/issues/600)] Foreign type member access too lenient ([#697](https://github.com/floeorg/floe/issues/697)) ([ba0240d](https://github.com/floeorg/floe/commit/ba0240d94281daf9f3a17cf4396e60d34dcafe9c))
* [[#607](https://github.com/floeorg/floe/issues/607)] Pipe does not support qualified for-block syntax ([#700](https://github.com/floeorg/floe/issues/700)) ([7c5e2f6](https://github.com/floeorg/floe/commit/7c5e2f6c034d516be84339949ad485545a07f936))
* [[#635](https://github.com/floeorg/floe/issues/635)] Probe resolves chained calls without intermediate const ([#696](https://github.com/floeorg/floe/issues/696)) ([a3e0d12](https://github.com/floeorg/floe/commit/a3e0d1250af5b4d149e96026671eca37c2533dc1))
* [[#686](https://github.com/floeorg/floe/issues/686)] await in non-async function should be a compile error ([#687](https://github.com/floeorg/floe/issues/687)) ([876afe8](https://github.com/floeorg/floe/commit/876afe84be997cf5948fb6fa1dd22e8faf780302))
* [[#691](https://github.com/floeorg/floe/issues/691)] floe fmt should skip files with parse errors ([#694](https://github.com/floeorg/floe/issues/694)) ([c2b77d3](https://github.com/floeorg/floe/commit/c2b77d3a7233881e184e3328730cff5d50738316))
* [[#693](https://github.com/floeorg/floe/issues/693)] Bracket access is completely unchecked ([#695](https://github.com/floeorg/floe/issues/695)) ([4c84a53](https://github.com/floeorg/floe/commit/4c84a5374e576963b4c25c3b0b5fd357252d4a34))
* [[#701](https://github.com/floeorg/floe/issues/701)] Dot-access completions fall through to global symbols ([#704](https://github.com/floeorg/floe/issues/704)) ([abafa83](https://github.com/floeorg/floe/commit/abafa83a9fed13d5c8a90254ff8a4060ad15739b))
* [[#705](https://github.com/floeorg/floe/issues/705)] Dot-access completions do not resolve record type fields ([#713](https://github.com/floeorg/floe/issues/713)) ([3e355db](https://github.com/floeorg/floe/commit/3e355db6899c543a43b533419337c1eb83c1bfb9))

## [0.1.25](https://github.com/floeorg/floe/compare/v0.1.24...v0.1.25) (2026-03-30)


### Features

* [[#676](https://github.com/floeorg/floe/issues/676)] Add Option, Result, Promise stdlib and Array.mapResult ([#682](https://github.com/floeorg/floe/issues/682)) ([68e36aa](https://github.com/floeorg/floe/commit/68e36aa5b3aa446f2ba8ef41f3b6f90e79c71223))
* [[#679](https://github.com/floeorg/floe/issues/679)] Support destructuring rename syntax ([#684](https://github.com/floeorg/floe/issues/684)) ([9712df3](https://github.com/floeorg/floe/commit/9712df3764d77caa8ccbb42490d2af54ab03d9e5))


### Bug Fixes

* [[#671](https://github.com/floeorg/floe/issues/671)] [[#672](https://github.com/floeorg/floe/issues/672)] Hover for type-only imports and match pattern bindings ([#675](https://github.com/floeorg/floe/issues/675)) ([acf73a7](https://github.com/floeorg/floe/commit/acf73a77a88ff339406581b157c874e0132dc8b2))

## [0.1.24](https://github.com/floeorg/floe/compare/v0.1.23...v0.1.24) (2026-03-30)


### Features

* [[#651](https://github.com/floeorg/floe/issues/651)] Change opaque type syntax from = to { } ([#659](https://github.com/floeorg/floe/issues/659)) ([76130d6](https://github.com/floeorg/floe/commit/76130d6fa5f52f6f64178d15daf948b00c4fc7ad))
* [[#652](https://github.com/floeorg/floe/issues/652)] Restrict & intersection to = type aliases only ([#663](https://github.com/floeorg/floe/issues/663)) ([36b113b](https://github.com/floeorg/floe/commit/36b113be82b391eca635b3a5b73d023ae5af5856))


### Bug Fixes

* [[#584](https://github.com/floeorg/floe/issues/584)] Member access hover shows resolved field types ([#667](https://github.com/floeorg/floe/issues/667)) ([b78aab2](https://github.com/floeorg/floe/commit/b78aab2499d7cb8ac618237ccf61d660a2c7c6e2))
* [[#641](https://github.com/floeorg/floe/issues/641)] LSP hover shows nothing for record types with spreads ([#662](https://github.com/floeorg/floe/issues/662)) ([004e58b](https://github.com/floeorg/floe/commit/004e58b5dd2a0eed5dde6cfbc416e848ff1dc5e2))
* [[#648](https://github.com/floeorg/floe/issues/648)] Codegen emits value import for type-only exports used in for-block params ([#658](https://github.com/floeorg/floe/issues/658)) ([2702e5d](https://github.com/floeorg/floe/commit/2702e5d93b5254bfe3596dfcf4a98e63b5fe32d0))

## [0.1.23](https://github.com/floeorg/floe/compare/v0.1.22...v0.1.23) (2026-03-30)


### Features

* [[#650](https://github.com/floeorg/floe/issues/650)] Change function type arrow from =&gt; to -&gt; in type position ([#657](https://github.com/floeorg/floe/issues/657)) ([4b5c9c1](https://github.com/floeorg/floe/commit/4b5c9c15a8db3ed9bcea578e7d578b68e0b42593))


### Bug Fixes

* [[#594](https://github.com/floeorg/floe/issues/594)] Vite plugin resolves .fl file when .ts file was intended for extensionless imports ([#655](https://github.com/floeorg/floe/issues/655)) ([c22a1c9](https://github.com/floeorg/floe/commit/c22a1c92da2ec54096eb7c34820aa7079d23845b))
* [[#644](https://github.com/floeorg/floe/issues/644)] Codegen name collision for same-named functions in different for-blocks ([#647](https://github.com/floeorg/floe/issues/647)) ([18dd0f9](https://github.com/floeorg/floe/commit/18dd0f9b20c8979e3ec36c517e35361f447a3727))

## [0.1.22](https://github.com/floeorg/floe/compare/v0.1.21...v0.1.22) (2026-03-29)


### Bug Fixes

* [[#639](https://github.com/floeorg/floe/issues/639)] Cross-file import loses npm types referenced in function signatures ([#642](https://github.com/floeorg/floe/issues/642)) ([64b58b1](https://github.com/floeorg/floe/commit/64b58b18d2e8c1ec67034c3d6607d0c1de171af4))

## [0.1.21](https://github.com/floeorg/floe/compare/v0.1.20...v0.1.21) (2026-03-29)


### Features

* [[#617](https://github.com/floeorg/floe/issues/617)] Support JSX spread attributes ([#637](https://github.com/floeorg/floe/issues/637)) ([084261d](https://github.com/floeorg/floe/commit/084261d9f7cc38c9a0d3fd7ae6af4ecc6048fa5a))


### Bug Fixes

* [[#636](https://github.com/floeorg/floe/issues/636)] Canonicalize LSP import resolution paths ([#638](https://github.com/floeorg/floe/issues/638)) ([ce97f5f](https://github.com/floeorg/floe/commit/ce97f5f5a1c39bdd8688543d5f6229128f0e36d5))

## [0.1.20](https://github.com/floeorg/floe/compare/v0.1.19...v0.1.20) (2026-03-29)


### Features

* [[#616](https://github.com/floeorg/floe/issues/616)] Support string literal type arguments ([#634](https://github.com/floeorg/floe/issues/634)) ([f21a977](https://github.com/floeorg/floe/commit/f21a977e0c3106bb7277fa3ae60087b0a424c8a7))
* [[#618](https://github.com/floeorg/floe/issues/618)] Record spread with generic types and formatter fixes ([#628](https://github.com/floeorg/floe/issues/628)) ([9e3db72](https://github.com/floeorg/floe/commit/9e3db72a3c51cb607cf6a6663e9b491b593a812e))


### Bug Fixes

* [[#592](https://github.com/floeorg/floe/issues/592)] Prevent tsgo probe from emitting stray .d.ts files ([#609](https://github.com/floeorg/floe/issues/609)) ([fc96cf8](https://github.com/floeorg/floe/commit/fc96cf831d7d990ca17756208a917243b2a7445e))
* [[#612](https://github.com/floeorg/floe/issues/612)] Probe inlining drops intermediate member access in chained calls ([#613](https://github.com/floeorg/floe/issues/613)) ([cb79b72](https://github.com/floeorg/floe/commit/cb79b72db4c7d4e4a2eb019ff912fc6ac18ab328))
* [[#614](https://github.com/floeorg/floe/issues/614)] Use inlined probe results for destructured const bindings ([#620](https://github.com/floeorg/floe/issues/620)) ([b84512b](https://github.com/floeorg/floe/commit/b84512b4e963ccedd35d255a3f247a0399c7f955))
* [[#615](https://github.com/floeorg/floe/issues/615)] Intersection type fails after generic type application ([#619](https://github.com/floeorg/floe/issues/619)) ([1b8e7f0](https://github.com/floeorg/floe/commit/1b8e7f0b2bfef3d20532e4def11319013a8b3e43))
* [[#621](https://github.com/floeorg/floe/issues/621)] Probe counter blocks reuse of same-named probes across functions ([#622](https://github.com/floeorg/floe/issues/622)) ([e848ed1](https://github.com/floeorg/floe/commit/e848ed1552981339cc707a25a95b186e1255c39b))
* [[#623](https://github.com/floeorg/floe/issues/623)] Async fn return type check does not unwrap Promise ([#626](https://github.com/floeorg/floe/issues/626)) ([afb4776](https://github.com/floeorg/floe/commit/afb47765625b417ccd9f8495a08b1e1e9ae7eac4))
* [[#624](https://github.com/floeorg/floe/issues/624)] Expand opaque named types via per-field probe destructuring ([#627](https://github.com/floeorg/floe/issues/627)) ([e9f9c81](https://github.com/floeorg/floe/commit/e9f9c81178f5572dfb9584ab35ab76438f50a063))
* [[#629](https://github.com/floeorg/floe/issues/629)] Unify Result types with unknown params across match arms ([#633](https://github.com/floeorg/floe/issues/633)) ([f90b284](https://github.com/floeorg/floe/commit/f90b284fdb86d9b8f8a8dcd119fcb138fb6064c8))

## [0.1.19](https://github.com/floeorg/floe/compare/v0.1.18...v0.1.19) (2026-03-29)


### Features

* [[#544](https://github.com/floeorg/floe/issues/544)] Add type alias probes for complex .d.ts type resolution ([#577](https://github.com/floeorg/floe/issues/577)) ([d873a0b](https://github.com/floeorg/floe/commit/d873a0be3be20fd99aeebf05d3e09e8dbf8bb93e))


### Bug Fixes

* [[#553](https://github.com/floeorg/floe/issues/553)] LSP hover on None shows concrete type from context ([#579](https://github.com/floeorg/floe/issues/579)) ([94d85cd](https://github.com/floeorg/floe/commit/94d85cdb16d0dd311a1d40b28ea3746c8f3f9f23))
* [[#580](https://github.com/floeorg/floe/issues/580)] Await on Promise with union return type and foreign type member access ([#582](https://github.com/floeorg/floe/issues/582)) ([56643ce](https://github.com/floeorg/floe/commit/56643ce1e8083a2562de03e2fb622c692bbbf94a))
* [[#583](https://github.com/floeorg/floe/issues/583)] Match pattern narrowing and object destructuring for foreign types ([#587](https://github.com/floeorg/floe/issues/587)) ([1e94055](https://github.com/floeorg/floe/commit/1e940554d2236da0a18ac72d40f775e4236632f3))
* [[#590](https://github.com/floeorg/floe/issues/590)] Emit import type for type-only npm import specifiers ([#591](https://github.com/floeorg/floe/issues/591)) ([f4ffe05](https://github.com/floeorg/floe/commit/f4ffe0547446602cc7b71ba3ed842d104bad5fdc))
* [[#595](https://github.com/floeorg/floe/issues/595)] Lexer does not support unicode escape sequences in strings ([#598](https://github.com/floeorg/floe/issues/598)) ([65c8ac3](https://github.com/floeorg/floe/commit/65c8ac3dafc3b208a4c654a7d3f1149ed1a58b38))
* [[#602](https://github.com/floeorg/floe/issues/602)] [[#603](https://github.com/floeorg/floe/issues/603)] Pipe map lambda inference and for-block import of foreign types ([#604](https://github.com/floeorg/floe/issues/604)) ([caadcc9](https://github.com/floeorg/floe/commit/caadcc96df34f10b97450efe85f638d4434c66a3))

## [0.1.18](https://github.com/floeorg/floe/compare/v0.1.17...v0.1.18) (2026-03-29)


### Features

* [[#574](https://github.com/floeorg/floe/issues/574)] Log version, executable path, and project dir on LSP startup ([#575](https://github.com/floeorg/floe/issues/575)) ([1e78a6f](https://github.com/floeorg/floe/commit/1e78a6f9ed03e6e05f2738d7da763ddb89e52427))

## [0.1.17](https://github.com/floeorg/floe/compare/v0.1.16...v0.1.17) (2026-03-29)


### Features

* [[#543](https://github.com/floeorg/floe/issues/543)] Add intersection type syntax (A & B) ([#569](https://github.com/floeorg/floe/issues/569)) ([66be2b7](https://github.com/floeorg/floe/commit/66be2b75b9bd8b57008bf3931f9abe72f4a0964a))


### Bug Fixes

* [[#562](https://github.com/floeorg/floe/issues/562)] Allow concrete values to be assignable to Option&lt;T&gt; ([#570](https://github.com/floeorg/floe/issues/570)) ([22c23fe](https://github.com/floeorg/floe/commit/22c23fe9d532139c800ab5801776f4a737757241))
* [[#564](https://github.com/floeorg/floe/issues/564)] Forward tsconfig paths to probe and parse JSONC tsconfig files ([#573](https://github.com/floeorg/floe/issues/573)) ([4f1185f](https://github.com/floeorg/floe/commit/4f1185f9227fa5a4669bbf148890cb45905a3301))

## [0.1.16](https://github.com/floeorg/floe/compare/v0.1.15...v0.1.16) (2026-03-29)


### Features

* [[#542](https://github.com/floeorg/floe/issues/542)] Add typeof operator in type positions ([#566](https://github.com/floeorg/floe/issues/566)) ([97bf3ec](https://github.com/floeorg/floe/commit/97bf3ec71488fcb9927d11aef0f66ae22cb26fcc))
* [[#548](https://github.com/floeorg/floe/issues/548)] Support tsconfig paths aliases in import resolution ([#568](https://github.com/floeorg/floe/issues/568)) ([9d32dc2](https://github.com/floeorg/floe/commit/9d32dc2e9bf17d96940a2afd106cb183808e18e4))


### Bug Fixes

* [[#547](https://github.com/floeorg/floe/issues/547)] Resolve typeof for relative TS imports with inferred return types ([#561](https://github.com/floeorg/floe/issues/561)) ([9d129da](https://github.com/floeorg/floe/commit/9d129da7977de35427dd90a6f0564c118eb3b027))
* [[#552](https://github.com/floeorg/floe/issues/552)] Show type info on hover for lambda parameters ([#563](https://github.com/floeorg/floe/issues/563)) ([0f52812](https://github.com/floeorg/floe/commit/0f528127265886d4d085c6c7dcc353b239f37388))

## [0.1.15](https://github.com/floeorg/floe/compare/v0.1.14...v0.1.15) (2026-03-29)


### Bug Fixes

* [[#507](https://github.com/floeorg/floe/issues/507)] LSP hover on 'from' keyword shows Array.from instead of import syntax ([#531](https://github.com/floeorg/floe/issues/531)) ([95eaef6](https://github.com/floeorg/floe/commit/95eaef694ac73ef99b4826172a77986789e70395))
* [[#533](https://github.com/floeorg/floe/issues/533)] LSP completion gaps - dot-access fields, import paths, context suppression ([#535](https://github.com/floeorg/floe/issues/535)) ([78251c1](https://github.com/floeorg/floe/commit/78251c15c48cc5098b4b075c9406ebcba5f17f43))
* [[#536](https://github.com/floeorg/floe/issues/536)] Resolve correct for-block overload based on receiver type ([#539](https://github.com/floeorg/floe/issues/539)) ([09b454a](https://github.com/floeorg/floe/commit/09b454af14e4b6d34d266b46d3c7b5ef55e4f5c9))
* [[#537](https://github.com/floeorg/floe/issues/537)] Resolve typeof function types from npm imports ([#549](https://github.com/floeorg/floe/issues/549)) ([588f91a](https://github.com/floeorg/floe/commit/588f91ad9cf92ce48a295ea19f35b4c4602d428d))
* [[#538](https://github.com/floeorg/floe/issues/538)] Validate argument count for stdlib method calls ([#541](https://github.com/floeorg/floe/issues/541)) ([379f613](https://github.com/floeorg/floe/commit/379f613388485c55d4d7b887c4f478d059b65b7e))
* [[#540](https://github.com/floeorg/floe/issues/540)] Console.log/warn/error/info/debug should be variadic ([#545](https://github.com/floeorg/floe/issues/545)) ([d119510](https://github.com/floeorg/floe/commit/d11951001631cd83434a0db3a2b157fa5aee8408))
* [[#550](https://github.com/floeorg/floe/issues/550)] LSP test script sleeps full timeout on every collect_notifications call ([#551](https://github.com/floeorg/floe/issues/551)) ([7c74d16](https://github.com/floeorg/floe/commit/7c74d1612e6ed38798a9e2c7cf57d36e23a6ba7f))

## [0.1.14](https://github.com/floeorg/floe/compare/v0.1.13...v0.1.14) (2026-03-29)


### Features

* [[#294](https://github.com/floeorg/floe/issues/294)] Add mock&lt;T&gt; compiler built-in for test data generation ([#473](https://github.com/floeorg/floe/issues/473)) ([3614d2f](https://github.com/floeorg/floe/commit/3614d2fef13adf93303e196697af341620d6359c))
* [[#422](https://github.com/floeorg/floe/issues/422)] Generate .d.ts stubs so TS resolves .fl imports ([#429](https://github.com/floeorg/floe/issues/429)) ([95c0f12](https://github.com/floeorg/floe/commit/95c0f12f3132fd06ae029dab95f2e775250cb09c))
* [[#475](https://github.com/floeorg/floe/issues/475)] Add default values for type fields ([#479](https://github.com/floeorg/floe/issues/479)) ([57bd5b8](https://github.com/floeorg/floe/commit/57bd5b821109ae813e73350da126d92ef8d054f1))
* [[#498](https://github.com/floeorg/floe/issues/498)] Output compiled files to .floe/ directory instead of alongside source ([#502](https://github.com/floeorg/floe/issues/502)) ([3821854](https://github.com/floeorg/floe/commit/38218543c57c851bea3e2153d3129213163feeac))
* [[#499](https://github.com/floeorg/floe/issues/499)] Auto-detect x?: T | null in .d.ts imports as Settable&lt;T&gt; ([#508](https://github.com/floeorg/floe/issues/508)) ([7e166a7](https://github.com/floeorg/floe/commit/7e166a7478b8ded5e5fc8c71071921264e25945a))
* [[#509](https://github.com/floeorg/floe/issues/509)] Add Date module to stdlib ([#517](https://github.com/floeorg/floe/issues/517)) ([e8fc12a](https://github.com/floeorg/floe/commit/e8fc12a9c7a4def5ff5401f289439432edef29a1))
* [[#511](https://github.com/floeorg/floe/issues/511)] Resolve types from local .ts/.tsx files imported in .fl files ([#515](https://github.com/floeorg/floe/issues/515)) ([257a27a](https://github.com/floeorg/floe/commit/257a27aac3d0e186d10a656c16c772ed26055d9b))
* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/floeorg/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/floeorg/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/floeorg/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/floeorg/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#403](https://github.com/floeorg/floe/issues/403)] Improve LSP hover information across the board ([03e512b](https://github.com/floeorg/floe/commit/03e512b0e4a411387583fc69f0c0c8e20a9ed2bc))
* [[#404](https://github.com/floeorg/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/floeorg/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/floeorg/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/floeorg/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* [[#480](https://github.com/floeorg/floe/issues/480)] Fix docs build and Open VSX publish CI failures ([#481](https://github.com/floeorg/floe/issues/481)) ([c95af9c](https://github.com/floeorg/floe/commit/c95af9c2bc422f09fe5630b5e80ac960451b5f98))
* [[#486](https://github.com/floeorg/floe/issues/486)] Widen vite-plugin peer dependency to support Vite 7 and 8 ([#487](https://github.com/floeorg/floe/issues/487)) ([27eae45](https://github.com/floeorg/floe/commit/27eae45a75a1f29bb3f7209e6aa2285c2c278cac))
* [[#489](https://github.com/floeorg/floe/issues/489)] Bundle VS Code extension with esbuild, fix icon, add restart command ([#490](https://github.com/floeorg/floe/issues/490)) ([abc9beb](https://github.com/floeorg/floe/commit/abc9bebb75b8fd1f318c4ad93e2cba7876d8cd11))
* [[#491](https://github.com/floeorg/floe/issues/491)] Support JSX comments {/* ... */} ([#497](https://github.com/floeorg/floe/issues/497)) ([974f37f](https://github.com/floeorg/floe/commit/974f37fe9fe62d6e185f32cebc5c3e8976ae47e9))
* [[#492](https://github.com/floeorg/floe/issues/492)] Fix JSX formatter to add newlines around match expressions and multi-line tag children ([#500](https://github.com/floeorg/floe/issues/500)) ([a7ac4d5](https://github.com/floeorg/floe/commit/a7ac4d561d7c372913a22e2ada74d3bdaf2f1b9a))
* [[#494](https://github.com/floeorg/floe/issues/494)] Add resolveId hook to vite plugin for .fl import resolution ([#495](https://github.com/floeorg/floe/issues/495)) ([963c97e](https://github.com/floeorg/floe/commit/963c97e1cdf409fb1970144407612c3b3831b824))
* [[#501](https://github.com/floeorg/floe/issues/501)] Tell Vite that compiled .fl output is TypeScript ([#505](https://github.com/floeorg/floe/issues/505)) ([4fd6475](https://github.com/floeorg/floe/commit/4fd6475e2a748a9edb2e4719cb2af78323792dbf))
* [[#506](https://github.com/floeorg/floe/issues/506)] LSP resolves tsconfig path aliases instead of reporting false errors ([#510](https://github.com/floeorg/floe/issues/510)) ([2facdc6](https://github.com/floeorg/floe/commit/2facdc6d39ec1737c77465e1fc83ec5ead56af76))
* [[#512](https://github.com/floeorg/floe/issues/512)] Vite plugin cross-version type compatibility and .d.fl.ts output ([#514](https://github.com/floeorg/floe/issues/514)) ([be4cb66](https://github.com/floeorg/floe/commit/be4cb662e9def60060434f29dd2bd34082f8bed1))
* [[#512](https://github.com/floeorg/floe/issues/512)] Write .d.fl.ts next to source and emit from --emit-stdout ([#519](https://github.com/floeorg/floe/issues/519)) ([1dfda9d](https://github.com/floeorg/floe/commit/1dfda9d142f8e352264225fdd7e22544bd12f127))
* [[#516](https://github.com/floeorg/floe/issues/516)] For-block functions from different types clash when both imported ([#518](https://github.com/floeorg/floe/issues/518)) ([3b418cb](https://github.com/floeorg/floe/commit/3b418cb6ce5685ec352f82e768fd858ae37fcf85))
* [[#520](https://github.com/floeorg/floe/issues/520)] Option match uses null checks, probe preserves nullability ([#523](https://github.com/floeorg/floe/issues/523)) ([ef5ee9b](https://github.com/floeorg/floe/commit/ef5ee9b69f2973e853fd9d8a8bdf86ece392c1ca))
* [[#521](https://github.com/floeorg/floe/issues/521)] Formatter deletes // comments inside blocks ([#522](https://github.com/floeorg/floe/issues/522)) ([389d6e0](https://github.com/floeorg/floe/commit/389d6e042453a84a179fa2fd435d9c8434287714))
* [[#525](https://github.com/floeorg/floe/issues/525)] Break after -&gt; in match arms when JSX body has multiline props ([#528](https://github.com/floeorg/floe/issues/528)) ([f48ac28](https://github.com/floeorg/floe/commit/f48ac289619660599645fb699f659355bbe64402))
* add id-token permission for npm trusted publishing ([#445](https://github.com/floeorg/floe/issues/445)) ([5eec3f0](https://github.com/floeorg/floe/commit/5eec3f079f52fa662cb318274f6f1811688ad900))
* add VS Code icon, fix npm publish, bump action versions ([#451](https://github.com/floeorg/floe/issues/451)) ([7133ba2](https://github.com/floeorg/floe/commit/7133ba27ab9e1606393c1af813b0cdf1db1c9df9))
* correct ignoreDeprecations value to 6.0 for TypeScript 7 ([#440](https://github.com/floeorg/floe/issues/440)) ([505d6fb](https://github.com/floeorg/floe/commit/505d6fb4d55ea06d7c986267fda086e508ed1d0b))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/floeorg/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/floeorg/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* improve LSP hover information across the board ([a9adeb7](https://github.com/floeorg/floe/commit/a9adeb79809833e23cfaa8b3ff77a9308fd17fc4))
* npm trusted publishing and Open VSX publisher/LICENSE ([#453](https://github.com/floeorg/floe/issues/453)) ([1507f92](https://github.com/floeorg/floe/commit/1507f927d57b39ba28da1bd4727ad8a3a3226a0e))
* pass tag name to release workflow for correct ref checkout ([#448](https://github.com/floeorg/floe/issues/448)) ([019745a](https://github.com/floeorg/floe/commit/019745afa8c1dae84a6d03c7f087511c0b4450ad))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/floeorg/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* stop release workflow from overwriting release-please changelog ([#430](https://github.com/floeorg/floe/issues/430)) ([b7d5d14](https://github.com/floeorg/floe/commit/b7d5d14ba9497512e66da25cec0d7884a6f36fe7))
* trigger release workflow directly from release-please ([#444](https://github.com/floeorg/floe/issues/444)) ([3ff34a2](https://github.com/floeorg/floe/commit/3ff34a20f6d55bc0dd112e235234c9f3fc0614e0))
* use plain v* tags instead of floe-v* for releases ([#425](https://github.com/floeorg/floe/issues/425)) ([bc53113](https://github.com/floeorg/floe/commit/bc5311340d2450b1fa4883605c5528deb526dfa7))
* validate named argument labels in function calls ([778395d](https://github.com/floeorg/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))
* VS Code extension publisher and engine version for Open VSX ([#447](https://github.com/floeorg/floe/issues/447)) ([5475113](https://github.com/floeorg/floe/commit/54751135a93729c3b87f926785610c92398cee3d))

## [0.1.13](https://github.com/floeorg/floe/compare/v0.1.12...v0.1.13) (2026-03-29)


### Bug Fixes

* [[#520](https://github.com/floeorg/floe/issues/520)] Option match uses null checks, probe preserves nullability ([#523](https://github.com/floeorg/floe/issues/523)) ([ef5ee9b](https://github.com/floeorg/floe/commit/ef5ee9b69f2973e853fd9d8a8bdf86ece392c1ca))
* [[#521](https://github.com/floeorg/floe/issues/521)] Formatter deletes // comments inside blocks ([#522](https://github.com/floeorg/floe/issues/522)) ([389d6e0](https://github.com/floeorg/floe/commit/389d6e042453a84a179fa2fd435d9c8434287714))

## [0.1.12](https://github.com/floeorg/floe/compare/v0.1.11...v0.1.12) (2026-03-28)


### Features

* [[#498](https://github.com/floeorg/floe/issues/498)] Output compiled files to .floe/ directory instead of alongside source ([#502](https://github.com/floeorg/floe/issues/502)) ([3821854](https://github.com/floeorg/floe/commit/38218543c57c851bea3e2153d3129213163feeac))
* [[#499](https://github.com/floeorg/floe/issues/499)] Auto-detect x?: T | null in .d.ts imports as Settable&lt;T&gt; ([#508](https://github.com/floeorg/floe/issues/508)) ([7e166a7](https://github.com/floeorg/floe/commit/7e166a7478b8ded5e5fc8c71071921264e25945a))
* [[#509](https://github.com/floeorg/floe/issues/509)] Add Date module to stdlib ([#517](https://github.com/floeorg/floe/issues/517)) ([e8fc12a](https://github.com/floeorg/floe/commit/e8fc12a9c7a4def5ff5401f289439432edef29a1))
* [[#511](https://github.com/floeorg/floe/issues/511)] Resolve types from local .ts/.tsx files imported in .fl files ([#515](https://github.com/floeorg/floe/issues/515)) ([257a27a](https://github.com/floeorg/floe/commit/257a27aac3d0e186d10a656c16c772ed26055d9b))


### Bug Fixes

* [[#492](https://github.com/floeorg/floe/issues/492)] Fix JSX formatter to add newlines around match expressions and multi-line tag children ([#500](https://github.com/floeorg/floe/issues/500)) ([a7ac4d5](https://github.com/floeorg/floe/commit/a7ac4d561d7c372913a22e2ada74d3bdaf2f1b9a))
* [[#501](https://github.com/floeorg/floe/issues/501)] Tell Vite that compiled .fl output is TypeScript ([#505](https://github.com/floeorg/floe/issues/505)) ([4fd6475](https://github.com/floeorg/floe/commit/4fd6475e2a748a9edb2e4719cb2af78323792dbf))
* [[#506](https://github.com/floeorg/floe/issues/506)] LSP resolves tsconfig path aliases instead of reporting false errors ([#510](https://github.com/floeorg/floe/issues/510)) ([2facdc6](https://github.com/floeorg/floe/commit/2facdc6d39ec1737c77465e1fc83ec5ead56af76))
* [[#512](https://github.com/floeorg/floe/issues/512)] Vite plugin cross-version type compatibility and .d.fl.ts output ([#514](https://github.com/floeorg/floe/issues/514)) ([be4cb66](https://github.com/floeorg/floe/commit/be4cb662e9def60060434f29dd2bd34082f8bed1))
* [[#512](https://github.com/floeorg/floe/issues/512)] Write .d.fl.ts next to source and emit from --emit-stdout ([#519](https://github.com/floeorg/floe/issues/519)) ([1dfda9d](https://github.com/floeorg/floe/commit/1dfda9d142f8e352264225fdd7e22544bd12f127))
* [[#516](https://github.com/floeorg/floe/issues/516)] For-block functions from different types clash when both imported ([#518](https://github.com/floeorg/floe/issues/518)) ([3b418cb](https://github.com/floeorg/floe/commit/3b418cb6ce5685ec352f82e768fd858ae37fcf85))

## [0.1.11](https://github.com/floeorg/floe/compare/v0.1.10...v0.1.11) (2026-03-28)


### Features

* [[#475](https://github.com/floeorg/floe/issues/475)] Add default values for type fields ([#479](https://github.com/floeorg/floe/issues/479)) ([57bd5b8](https://github.com/floeorg/floe/commit/57bd5b821109ae813e73350da126d92ef8d054f1))


### Bug Fixes

* [[#486](https://github.com/floeorg/floe/issues/486)] Widen vite-plugin peer dependency to support Vite 7 and 8 ([#487](https://github.com/floeorg/floe/issues/487)) ([27eae45](https://github.com/floeorg/floe/commit/27eae45a75a1f29bb3f7209e6aa2285c2c278cac))
* [[#489](https://github.com/floeorg/floe/issues/489)] Bundle VS Code extension with esbuild, fix icon, add restart command ([#490](https://github.com/floeorg/floe/issues/490)) ([abc9beb](https://github.com/floeorg/floe/commit/abc9bebb75b8fd1f318c4ad93e2cba7876d8cd11))
* [[#491](https://github.com/floeorg/floe/issues/491)] Support JSX comments {/* ... */} ([#497](https://github.com/floeorg/floe/issues/497)) ([974f37f](https://github.com/floeorg/floe/commit/974f37fe9fe62d6e185f32cebc5c3e8976ae47e9))
* [[#494](https://github.com/floeorg/floe/issues/494)] Add resolveId hook to vite plugin for .fl import resolution ([#495](https://github.com/floeorg/floe/issues/495)) ([963c97e](https://github.com/floeorg/floe/commit/963c97e1cdf409fb1970144407612c3b3831b824))

## [0.1.10](https://github.com/floeorg/floe/compare/v0.1.9...v0.1.10) (2026-03-28)


### Bug Fixes

* [[#480](https://github.com/floeorg/floe/issues/480)] Fix docs build and Open VSX publish CI failures ([#481](https://github.com/floeorg/floe/issues/481)) ([c95af9c](https://github.com/floeorg/floe/commit/c95af9c2bc422f09fe5630b5e80ac960451b5f98))

## [0.1.9](https://github.com/floeorg/floe/compare/v0.1.8...v0.1.9) (2026-03-28)


### Features

* [[#294](https://github.com/floeorg/floe/issues/294)] Add mock&lt;T&gt; compiler built-in for test data generation ([#473](https://github.com/floeorg/floe/issues/473)) ([3614d2f](https://github.com/floeorg/floe/commit/3614d2fef13adf93303e196697af341620d6359c))

## [0.1.8](https://github.com/floeorg/floe/compare/v0.1.7...v0.1.8) (2026-03-28)


### Bug Fixes

* npm trusted publishing and Open VSX publisher/LICENSE ([#453](https://github.com/floeorg/floe/issues/453)) ([1507f92](https://github.com/floeorg/floe/commit/1507f927d57b39ba28da1bd4727ad8a3a3226a0e))

## [0.1.7](https://github.com/floeorg/floe/compare/v0.1.6...v0.1.7) (2026-03-28)


### Bug Fixes

* add VS Code icon, fix npm publish, bump action versions ([#451](https://github.com/floeorg/floe/issues/451)) ([7133ba2](https://github.com/floeorg/floe/commit/7133ba27ab9e1606393c1af813b0cdf1db1c9df9))

## [0.1.6](https://github.com/floeorg/floe/compare/v0.1.5...v0.1.6) (2026-03-28)


### Bug Fixes

* pass tag name to release workflow for correct ref checkout ([#448](https://github.com/floeorg/floe/issues/448)) ([019745a](https://github.com/floeorg/floe/commit/019745afa8c1dae84a6d03c7f087511c0b4450ad))

## [0.1.5](https://github.com/floeorg/floe/compare/v0.1.4...v0.1.5) (2026-03-28)


### Bug Fixes

* add id-token permission for npm trusted publishing ([#445](https://github.com/floeorg/floe/issues/445)) ([5eec3f0](https://github.com/floeorg/floe/commit/5eec3f079f52fa662cb318274f6f1811688ad900))
* trigger release workflow directly from release-please ([#444](https://github.com/floeorg/floe/issues/444)) ([3ff34a2](https://github.com/floeorg/floe/commit/3ff34a20f6d55bc0dd112e235234c9f3fc0614e0))
* VS Code extension publisher and engine version for Open VSX ([#447](https://github.com/floeorg/floe/issues/447)) ([5475113](https://github.com/floeorg/floe/commit/54751135a93729c3b87f926785610c92398cee3d))

## [0.1.4](https://github.com/floeorg/floe/compare/v0.1.3...v0.1.4) (2026-03-28)


### Bug Fixes

* correct ignoreDeprecations value to 6.0 for TypeScript 7 ([#440](https://github.com/floeorg/floe/issues/440)) ([505d6fb](https://github.com/floeorg/floe/commit/505d6fb4d55ea06d7c986267fda086e508ed1d0b))

## [0.1.3](https://github.com/floeorg/floe/compare/v0.1.2...v0.1.3) (2026-03-28)


### Features

* [[#422](https://github.com/floeorg/floe/issues/422)] Generate .d.ts stubs so TS resolves .fl imports ([#429](https://github.com/floeorg/floe/issues/429)) ([95c0f12](https://github.com/floeorg/floe/commit/95c0f12f3132fd06ae029dab95f2e775250cb09c))


### Bug Fixes

* stop release workflow from overwriting release-please changelog ([#430](https://github.com/floeorg/floe/issues/430)) ([b7d5d14](https://github.com/floeorg/floe/commit/b7d5d14ba9497512e66da25cec0d7884a6f36fe7))

## [0.1.2](https://github.com/floeorg/floe/compare/v0.1.1...v0.1.2) (2026-03-28)


### Features

* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/floeorg/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/floeorg/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/floeorg/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/floeorg/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#403](https://github.com/floeorg/floe/issues/403)] Improve LSP hover information across the board ([03e512b](https://github.com/floeorg/floe/commit/03e512b0e4a411387583fc69f0c0c8e20a9ed2bc))
* [[#404](https://github.com/floeorg/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/floeorg/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/floeorg/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/floeorg/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/floeorg/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/floeorg/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* improve LSP hover information across the board ([a9adeb7](https://github.com/floeorg/floe/commit/a9adeb79809833e23cfaa8b3ff77a9308fd17fc4))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/floeorg/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* use plain v* tags instead of floe-v* for releases ([#425](https://github.com/floeorg/floe/issues/425)) ([bc53113](https://github.com/floeorg/floe/commit/bc5311340d2450b1fa4883605c5528deb526dfa7))
* validate named argument labels in function calls ([778395d](https://github.com/floeorg/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))

## [0.1.1](https://github.com/floeorg/floe/compare/floe-v0.1.0...floe-v0.1.1) (2026-03-28)


### Features

* add LSP hover and integration tests for generic functions ([95728e9](https://github.com/floeorg/floe/commit/95728e9cd93a2090487c058cdbce1d9cf91cfa38))
* docs and syntax highlighting for generic functions ([719381c](https://github.com/floeorg/floe/commit/719381cccf3ed7a2914a4ffa14eb968690f57c67))


### Bug Fixes

* [[#384](https://github.com/floeorg/floe/issues/384)] Preserve user blank lines between statements in blocks ([906028f](https://github.com/floeorg/floe/commit/906028f2e71d0624d9b699dd22ed862719933957))
* [[#404](https://github.com/floeorg/floe/issues/404)] Checker - validate named arguments in function calls ([cb2e1e6](https://github.com/floeorg/floe/commit/cb2e1e645199ff2f05b39c7a29d734e6576ec5b1))
* [[#407](https://github.com/floeorg/floe/issues/407)] Formatter preserves trusted keyword and destructured params ([b6ff269](https://github.com/floeorg/floe/commit/b6ff269bbcad2347c688be3a54c1f9b58797beba))
* formatter preserves trusted keyword and destructured params ([4387307](https://github.com/floeorg/floe/commit/43873075b91d5b10cbe10eb1b9abd7c8ff5c630d))
* formatter preserves tuple index access and add pnpm install reminder ([f46f6e6](https://github.com/floeorg/floe/commit/f46f6e62ff59563038b5bf7f65c7af100430f994))
* preserve user blank lines between statements in blocks ([99bc8ed](https://github.com/floeorg/floe/commit/99bc8edee1ac80309390a6bb0f8b4c2252a13b7f))
* validate named argument labels in function calls ([778395d](https://github.com/floeorg/floe/commit/778395d529700434d1e1608bfb26ae4e41b060c8))

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

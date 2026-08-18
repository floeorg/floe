//! Is the npm package an import names actually installed?
//!
//! This module answers one question, and only that question: does a
//! `node_modules/<package>` directory exist at or above the project
//! directory? It never opens a declaration file. Finding the `.d.ts`
//! inside an installed package is [`super::package_exports`]'s job.
//! This module borrows that module's specifier parsing and its
//! directory layout, so the two agree both on what a package name is
//! and on where a package lives.
//!
//! The split matters, because the two failures need two answers:
//!
//! - The package is **not there**. Nothing can type it, and nothing can
//!   run it either. The checker reports **E013** and the build fails.
//! - The package **is there** but ships no declarations, or ships them
//!   somewhere this compiler cannot follow. Its symbols type as
//!   `Foreign`, and the checker warns **W004** on each call.
//!
//! `floe check` used to give the second answer to both situations while
//! the language server gave the first, so a build the editor called
//! broken passed on the command line (#1465).

use std::collections::HashMap;
use std::path::Path;

use super::package_exports::{
    is_valid_package_name, package_dir_candidates, split_specifier, types_package_name,
};
use crate::parser::ast::{ItemKind, Program};
use crate::resolve::{ResolvedImports, TsconfigPaths};

/// Declarations for the `node:` scheme live in `@types/node`, so an
/// import of `node:crypto` needs that package and no other.
const NODE_TYPES_PACKAGE: &str = "@types/node";

/// Node's own modules, which resolve with nothing installed.
///
/// `import { readFileSync } from "fs"` is the same module as
/// `node:fs`, and neither is a package. Reading `fs` as a package name
/// told people to run `npm install fs`, which installs a real,
/// deprecated stub and breaks the project (#1465, #1509 review). Both
/// spellings route to `@types/node`, which is the package that actually
/// types them.
///
/// A builtin is never E013. The module is there whatever is installed,
/// so a missing `@types/node` leaves it typed but unresolvable, which
/// is W004. See [`is_node_builtin`].
const NODE_BUILTINS: [&str; 44] = [
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "sea",
    "sqlite",
    "stream",
    "string_decoder",
    "sys",
    "test",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
];

/// The npm package a specifier needs, with any subpath removed.
///
/// - `"react"` needs `"react"`
/// - `"date-fns/format"` needs `"date-fns"`
/// - `"@scope/pkg/sub"` needs `"@scope/pkg"`
/// - every `"node:*"` specifier needs `"@types/node"`
///
/// Returns `None` when the specifier does not name an npm package at
/// all. No install fixes such an import, and naming a package to
/// install would be nonsense, so this module leaves it to the existing
/// unknown-type warning. Three kinds land there:
///
/// - a head that npm cannot publish, such as `..` or `https:`
/// - a Node subpath import, `#lib/helper`, which `package.json`
///   resolves through its own `imports` field
fn required_package(specifier: &str) -> Option<&str> {
    if is_node_builtin(specifier) {
        return Some(NODE_TYPES_PACKAGE);
    }
    // `package.json` `imports` owns every `#` specifier. It is a private
    // alias for a path inside the same package, never a package name.
    if specifier.starts_with('#') {
        return None;
    }
    let (package, _subpath) = split_specifier(specifier);
    if !is_valid_package_name(package) {
        return None;
    }

    Some(package)
}

/// True for a module Node itself supplies: `node:crypto`, `fs`,
/// `fs/promises`.
///
/// The checker asks this before it picks a severity. A builtin resolves
/// at run time with nothing installed, so "cannot find module" is
/// simply false about one. A missing `@types/node` leaves it untyped
/// instead, which is a warning, and a Bun or Deno project that never
/// adds `@types/node` still builds (#1465).
pub fn is_node_builtin(specifier: &str) -> bool {
    if specifier.starts_with("node:") {
        return true;
    }
    let (head, _subpath) = split_specifier(specifier);

    NODE_BUILTINS.contains(&head)
}

/// The advice printed under an E013 diagnostic. It names the command
/// that fixes the import, because "cannot find module" on its own
/// leaves the reader guessing between a missing install and a missing
/// `@types` package.
pub(crate) fn install_hint(package: &str) -> String {
    if package == NODE_TYPES_PACKAGE {
        return format!(
            "install the Node type declarations: `npm install --save-dev {NODE_TYPES_PACKAGE}`"
        );
    }

    format!(
        "install the package: `npm install {package}`. If it ships no type declarations, also add `npm install --save-dev @types/{}`",
        types_package_name(package)
    )
}

/// True when a `node_modules` directory at or above `project_dir`
/// holds `package`. Either the package itself or its `@types` companion
/// counts, since either one gives the import a meaning.
///
/// [`package_dir_candidates`] names the directories, so this answer and
/// the declaration lookup in [`super::package_exports`] read the same
/// two places.
///
/// The walk upward matters for a workspace: pnpm and npm both hoist a
/// dependency into the repository root, so the nearest `node_modules`
/// is often not the one holding the package.
pub(crate) fn is_installed(package: &str, project_dir: &Path) -> bool {
    project_dir.ancestors().any(|dir| {
        package_dir_candidates(&dir.join("node_modules"), package)
            .iter()
            .any(|candidate| candidate.is_dir())
    })
}

/// Every npm import in `program` whose package is not installed, as
/// `import specifier → package name`. The checker reads this map to
/// emit E013; it holds the package name so the diagnostic can name the
/// thing to install rather than re-deriving it.
///
/// **E013 says a package is absent, so this pass only speaks when it is
/// certain.** It stays silent whenever something other than
/// `node_modules` could resolve the specifier:
///
/// - a relative or `.fl` import, which has its own resolver
/// - a tsconfig `paths` alias or a `baseUrl` path, read from the
///   project directory and from the source file's own directory,
///   because a workspace package keeps its tsconfig below the hoisted
///   `node_modules` that `project_dir` points at
/// - a Node builtin or a `#` subpath import, which need no package
/// - a Yarn Plug'n'Play project, which keeps no `node_modules` at all
pub fn find_missing_packages(
    program: &Program,
    resolved_imports: &HashMap<String, ResolvedImports>,
    tsconfig_paths: &TsconfigPaths,
    source_dir: &Path,
    project_dir: &Path,
) -> HashMap<String, String> {
    npm_package_imports(
        program,
        resolved_imports,
        tsconfig_paths,
        source_dir,
        project_dir,
    )
    .into_iter()
    .filter(|(_, package)| !is_installed(package, project_dir))
    .map(|(specifier, package)| (specifier.to_string(), package.to_string()))
    .collect()
}

/// Every npm package `program` imports, paired with whether it is
/// installed right now. Sorted and deduplicated.
///
/// `floe check` stores this beside a module's cached diagnostics and
/// re-tests it on the next run. Source fingerprints cannot see
/// `node_modules`, so without this a clean result outlived the package
/// it depended on and `floe check` disagreed with `floe build` (#1465).
///
/// The pair carries the answer rather than assuming it. A clean module
/// can name a package that is absent: a `node:` import warns W004 and
/// stays clean while `@types/node` is missing, and installing
/// `@types/node` later must still invalidate it.
pub fn imported_package_state(
    program: &Program,
    resolved_imports: &HashMap<String, ResolvedImports>,
    tsconfig_paths: &TsconfigPaths,
    source_dir: &Path,
    project_dir: &Path,
) -> Vec<(String, bool)> {
    let mut packages: Vec<(String, bool)> = npm_package_imports(
        program,
        resolved_imports,
        tsconfig_paths,
        source_dir,
        project_dir,
    )
    .into_iter()
    .map(|(_, package)| (package.to_string(), is_installed(package, project_dir)))
    .collect();
    packages.sort_unstable();
    packages.dedup();

    packages
}

/// Every import in `program` that this module reads as an npm package,
/// as `(specifier, package)`. Everything another resolver owns has
/// already dropped out.
fn npm_package_imports<'a>(
    program: &'a Program,
    resolved_imports: &HashMap<String, ResolvedImports>,
    tsconfig_paths: &TsconfigPaths,
    source_dir: &Path,
    project_dir: &Path,
) -> Vec<(&'a str, &'a str)> {
    // Yarn Plug'n'Play resolves every package out of a zip index and
    // keeps no `node_modules` at all, so the walk below would call every
    // single import absent. Say nothing instead: an empty answer is the
    // honest one when this module cannot see how the project resolves.
    //
    // A project that simply has not been installed yet still reports,
    // because "the package is not there" is exactly true and `npm
    // install` is exactly the fix.
    if uses_plug_n_play(project_dir) {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Read only if some specifier gets this far, and only once.
    let mut source_tsconfig: Option<TsconfigPaths> = None;

    for item in &program.items {
        let ItemKind::Import(decl) = &item.kind else {
            continue;
        };
        let specifier = decl.source.as_str();
        if is_relative(specifier) {
            continue;
        }
        if resolved_imports.contains_key(specifier) || tsconfig_paths.claims(specifier) {
            continue;
        }
        if !seen.insert(specifier) {
            continue;
        }
        let Some(package) = required_package(specifier) else {
            continue;
        };
        let source_tsconfig =
            source_tsconfig.get_or_insert_with(|| TsconfigPaths::from_project_dir(source_dir));
        if source_tsconfig.claims(specifier) {
            continue;
        }

        found.push((specifier, package));
    }

    found
}

/// True for a specifier that names a path rather than a package.
fn is_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

/// The loader files Yarn writes for Plug'n'Play. Their presence means
/// packages resolve through the PnP index rather than `node_modules`.
const PLUG_N_PLAY_MARKERS: [&str; 3] = [".pnp.cjs", ".pnp.js", ".pnp.loader.mjs"];

/// True when a Yarn Plug'n'Play loader sits at or above `project_dir`.
///
/// Deno projects resolve their own way too and would need the same
/// treatment, but no fixture proves that path yet, so this names only
/// what it has tested.
fn uses_plug_n_play(project_dir: &Path) -> bool {
    project_dir.ancestors().any(|dir| {
        PLUG_N_PLAY_MARKERS
            .iter()
            .any(|marker| dir.join(marker).is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_package_keeps_a_plain_specifier() {
        assert_eq!(required_package("react"), Some("react"));
    }

    #[test]
    fn required_package_drops_a_subpath() {
        assert_eq!(required_package("date-fns/format"), Some("date-fns"));
    }

    #[test]
    fn required_package_keeps_both_segments_of_a_scoped_package() {
        assert_eq!(
            required_package("@tanstack/react-query"),
            Some("@tanstack/react-query")
        );
    }

    #[test]
    fn required_package_drops_a_subpath_under_a_scope() {
        assert_eq!(required_package("@scope/pkg/deep/path"), Some("@scope/pkg"));
    }

    #[test]
    fn required_package_routes_the_node_scheme_to_types_node() {
        assert_eq!(required_package("node:crypto"), Some("@types/node"));
    }

    #[test]
    fn required_package_routes_a_bare_node_builtin_to_types_node() {
        // `import { readFileSync } from "fs"` is the same module as
        // `node:fs`. Reading `fs` as a package name printed
        // "npm install fs", which installs a real deprecated stub.
        assert_eq!(required_package("fs"), Some("@types/node"));
        assert_eq!(required_package("path"), Some("@types/node"));
    }

    #[test]
    fn required_package_routes_a_builtin_subpath_to_types_node() {
        assert_eq!(required_package("fs/promises"), Some("@types/node"));
    }

    #[test]
    fn required_package_refuses_a_node_subpath_import() {
        // `package.json` `imports` owns `#`, so no install fixes it.
        assert_eq!(required_package("#lib/helper"), None);
    }

    #[test]
    fn required_package_refuses_a_url() {
        assert_eq!(required_package("https://esm.sh/preact"), None);
    }

    #[test]
    fn required_package_refuses_a_name_npm_cannot_publish() {
        // No install fixes `..`, and no diagnostic should tell a person
        // to try one.
        assert_eq!(required_package(".."), None);
    }

    #[test]
    fn install_hint_for_the_node_scheme_names_types_node() {
        let hint = install_hint("@types/node");
        assert!(hint.contains("@types/node"), "got: {hint}");
        assert!(!hint.contains("@types/@types"), "got: {hint}");
    }

    #[test]
    fn install_hint_names_both_the_package_and_its_types() {
        let hint = install_hint("react");
        assert!(hint.contains("npm install react"), "got: {hint}");
        assert!(hint.contains("@types/react"), "got: {hint}");
    }

    #[test]
    fn install_hint_collapses_a_scope_for_the_types_package() {
        let hint = install_hint("@scope/pkg");
        assert!(hint.contains("@types/scope__pkg"), "got: {hint}");
    }

    #[test]
    fn is_installed_walks_up_to_a_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/react")).unwrap();
        let app = root.path().join("apps/web");
        std::fs::create_dir_all(&app).unwrap();

        assert!(is_installed("react", &app));
    }

    #[test]
    fn is_installed_accepts_a_types_only_install() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/@types/react")).unwrap();

        assert!(is_installed("react", root.path()));
    }

    #[test]
    fn is_installed_collapses_a_scope_for_the_types_directory() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/@types/scope__pkg")).unwrap();

        assert!(is_installed("@scope/pkg", root.path()));
    }

    #[test]
    fn is_installed_finds_types_node_for_the_node_scheme() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/@types/node")).unwrap();

        let package = required_package("node:crypto").expect("node: needs @types/node");
        assert!(is_installed(package, root.path()));
    }

    #[test]
    fn is_installed_reports_an_absent_package() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/react")).unwrap();

        assert!(!is_installed("preact", root.path()));
    }

    #[test]
    fn is_installed_accepts_a_package_without_declarations() {
        // No `.d.ts` anywhere in it. Presence is the whole question here:
        // a package with no types is W004's problem, not E013's.
        let root = tempfile::tempdir().unwrap();
        let pkg = root.path().join("node_modules/no-types");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), r#"{"name":"no-types"}"#).unwrap();

        assert!(is_installed("no-types", root.path()));
    }

    fn program_of(source: &str) -> Program {
        crate::parser::Parser::new(source)
            .parse_program()
            .expect("fixture parses")
    }

    #[test]
    fn find_missing_packages_reports_an_uninstalled_package() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let program = program_of("import trusted { shout } from \"absent-package\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert_eq!(
            missing.get("absent-package").map(String::as_str),
            Some("absent-package")
        );
    }

    #[test]
    fn find_missing_packages_names_the_package_of_a_subpath_import() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let program = program_of("import trusted { format } from \"date-fns/format\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert_eq!(
            missing.get("date-fns/format").map(String::as_str),
            Some("date-fns")
        );
    }

    #[test]
    fn find_missing_packages_skips_an_installed_package() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/present-package")).unwrap();
        let program = program_of("import trusted { shout } from \"present-package\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }

    #[test]
    fn find_missing_packages_stays_silent_under_plug_n_play() {
        // Yarn PnP keeps no `node_modules`, so the disk walk would call
        // every import absent. It must say nothing at all instead.
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".pnp.cjs"), "// yarn pnp loader").unwrap();
        let program = program_of("import trusted { shout } from \"any-package\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }

    #[test]
    fn find_missing_packages_reports_a_project_that_was_never_installed() {
        // No `node_modules` and no PnP loader. The package really is not
        // there and `npm install` really is the fix, so this must report.
        let root = tempfile::tempdir().unwrap();
        let program = program_of("import trusted { shout } from \"absent-package\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert_eq!(
            missing.get("absent-package").map(String::as_str),
            Some("absent-package")
        );
    }

    #[test]
    fn find_missing_packages_skips_a_bare_builtin_when_types_node_is_installed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/@types/node")).unwrap();
        let program = program_of("import trusted { readFileSync } from \"fs\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }

    #[test]
    fn find_missing_packages_skips_a_node_subpath_import() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let program = program_of("import trusted { shout } from \"#lib/helper\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }

    #[test]
    fn find_missing_packages_reads_the_tsconfig_beside_the_source_file() {
        // A workspace package keeps its tsconfig below the hoisted
        // `node_modules`, so the project directory's tsconfig, which is
        // what `tsconfig_paths` came from, never sees the alias.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let app = root.path().join("app");
        std::fs::create_dir_all(app.join("src/lib")).unwrap();
        std::fs::write(
            app.join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@app/*":["src/*"]}}}"#,
        )
        .unwrap();
        std::fs::write(app.join("src/lib/helper.ts"), "export const shout = 1;").unwrap();
        let program = program_of("import trusted { shout } from \"@app/lib/helper\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            &app.join("src"),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }

    #[test]
    fn imported_package_state_lists_what_the_cache_must_re_test() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/present")).unwrap();
        let program = program_of(
            "import trusted { a } from \"present\"\nimport trusted { b } from \"present/sub\"\nimport { c } from \"./local\"\n",
        );

        let packages = imported_package_state(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert_eq!(packages, vec![("present".to_string(), true)]);
    }

    #[test]
    fn imported_package_state_records_a_package_that_is_absent() {
        // A `node:` import stays clean while `@types/node` is missing,
        // so the cache has to remember that it was missing.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let program = program_of("import trusted { randomUUID } from \"node:crypto\"\n");

        let packages = imported_package_state(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert_eq!(packages, vec![("@types/node".to_string(), false)]);
    }

    #[test]
    fn find_missing_packages_skips_relative_imports() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules")).unwrap();
        let program = program_of("import { Todo } from \"./types\"\n");

        let missing = find_missing_packages(
            &program,
            &HashMap::new(),
            &TsconfigPaths::default(),
            root.path(),
            root.path(),
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }
}

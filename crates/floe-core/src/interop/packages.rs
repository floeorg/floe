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

/// The npm package a specifier needs, with any subpath removed.
///
/// - `"react"` needs `"react"`
/// - `"date-fns/format"` needs `"date-fns"`
/// - `"@scope/pkg/sub"` needs `"@scope/pkg"`
/// - every `"node:*"` specifier needs `"@types/node"`
///
/// Returns `None` when the head of the specifier is not a name npm can
/// publish. No install fixes such an import, but naming a package to
/// install would be nonsense, so this module leaves it to the existing
/// unknown-type warning.
fn required_package(specifier: &str) -> Option<&str> {
    if specifier.starts_with("node:") {
        return Some(NODE_TYPES_PACKAGE);
    }
    let (package, _subpath) = split_specifier(specifier);
    if !is_valid_package_name(package) {
        return None;
    }

    Some(package)
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
fn is_installed(package: &str, project_dir: &Path) -> bool {
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
/// Relative imports, `.fl` imports and tsconfig path aliases are not
/// npm packages, so this pass leaves them alone. Their own resolvers
/// report them.
pub fn find_missing_packages(
    program: &Program,
    resolved_imports: &HashMap<String, ResolvedImports>,
    tsconfig_paths: &TsconfigPaths,
    project_dir: &Path,
) -> HashMap<String, String> {
    let mut missing = HashMap::new();
    for item in &program.items {
        let ItemKind::Import(decl) = &item.kind else {
            continue;
        };
        let specifier = decl.source.as_str();
        if specifier.starts_with("./") || specifier.starts_with("../") {
            continue;
        }
        if resolved_imports.contains_key(specifier) || tsconfig_paths.matches(specifier) {
            continue;
        }
        if missing.contains_key(specifier) {
            continue;
        }
        let Some(package) = required_package(specifier) else {
            continue;
        };
        if !is_installed(package, project_dir) {
            missing.insert(specifier.to_string(), package.to_string());
        }
    }

    missing
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
        );

        assert!(missing.is_empty(), "got: {missing:?}");
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
        );

        assert!(missing.is_empty(), "got: {missing:?}");
    }
}

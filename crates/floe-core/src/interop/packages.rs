//! Is the npm package an import names actually installed?
//!
//! This module answers one question, and only that question: does a
//! `node_modules/<package>` directory exist at or above the project
//! directory? It does not look for declaration files. Finding the
//! `.d.ts` inside an installed package is [`super::tsgo`]'s job.
//!
//! The split matters, because the two failures need two answers:
//!
//! - The package is **not there**. Nothing can type it, and nothing can
//!   run it either. The checker reports **E013** and the build fails.
//! - The package **is there** but ships no declarations. Its symbols
//!   type as `Foreign`, and the checker warns **W004** on each call.
//!
//! `floe check` used to give the second answer to both situations while
//! the language server gave the first, so a build the editor called
//! broken passed on the command line (#1465).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::parser::ast::{ItemKind, Program};
use crate::resolve::{ResolvedImports, TsconfigPaths};

/// Declarations for the `node:` scheme live in `@types/node`, so an
/// import of `node:crypto` needs that package and no other.
const NODE_TYPES_PACKAGE: &str = "@types/node";

/// The npm package a specifier imports from, with any subpath removed.
///
/// - `"react"` stays `"react"`
/// - `"date-fns/format"` becomes `"date-fns"`
/// - `"@scope/pkg/sub"` becomes `"@scope/pkg"`
/// - every `"node:*"` specifier becomes `"@types/node"`
pub fn package_name(specifier: &str) -> &str {
    if specifier.starts_with("node:") {
        return NODE_TYPES_PACKAGE;
    }
    if let Some(scoped) = specifier.strip_prefix('@') {
        let Some((scope, rest)) = scoped.split_once('/') else {
            return specifier;
        };
        let name = rest.split('/').next().unwrap_or(rest);

        // `@` + scope + `/` + name, counted rather than rebuilt so the
        // caller keeps borrowing the original specifier.
        return &specifier[..1 + scope.len() + 1 + name.len()];
    }

    specifier.split('/').next().unwrap_or(specifier)
}

/// The `@types` package that carries declarations for `package`.
/// A scope collapses into the name: `@scope/pkg` becomes
/// `@types/scope__pkg`, which is the convention DefinitelyTyped uses.
pub fn types_package_name(package: &str) -> String {
    if package.starts_with("@types/") {
        return package.to_string();
    }
    let Some(scoped) = package.strip_prefix('@') else {
        return format!("@types/{package}");
    };
    let Some((scope, name)) = scoped.split_once('/') else {
        return format!("@types/{package}");
    };

    format!("@types/{scope}__{name}")
}

/// The advice printed under an E013 diagnostic. It names the command
/// that fixes the import, because "cannot find module" on its own
/// leaves the reader guessing between a missing install and a missing
/// `@types` package.
pub fn install_hint(package: &str) -> String {
    if package == NODE_TYPES_PACKAGE {
        return format!(
            "install the Node type declarations: `npm install --save-dev {NODE_TYPES_PACKAGE}`"
        );
    }

    format!(
        "install the package: `npm install {package}`. If it ships no type declarations, also add `npm install --save-dev {}`",
        types_package_name(package)
    )
}

/// Find `package` in a `node_modules` directory at or above
/// `project_dir`. Either the package itself or its `@types` companion
/// counts as installed, since either one gives the import a meaning.
///
/// The walk upward matters for a workspace: pnpm and npm both hoist a
/// dependency into the repository root, so the nearest `node_modules`
/// is often not the one holding the package.
pub fn find_package_dir(package: &str, project_dir: &Path) -> Option<PathBuf> {
    let types_package = types_package_name(package);
    let mut dir = project_dir.to_path_buf();
    loop {
        let modules = dir.join("node_modules");
        for candidate in [modules.join(package), modules.join(&types_package)] {
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
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
        let package = package_name(specifier);
        if find_package_dir(package, project_dir).is_none() {
            missing.insert(specifier.to_string(), package.to_string());
        }
    }

    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_name_keeps_a_plain_specifier() {
        assert_eq!(package_name("react"), "react");
    }

    #[test]
    fn package_name_drops_a_subpath() {
        assert_eq!(package_name("date-fns/format"), "date-fns");
    }

    #[test]
    fn package_name_keeps_both_segments_of_a_scoped_package() {
        assert_eq!(
            package_name("@tanstack/react-query"),
            "@tanstack/react-query"
        );
    }

    #[test]
    fn package_name_drops_a_subpath_under_a_scope() {
        assert_eq!(package_name("@scope/pkg/deep/path"), "@scope/pkg");
    }

    #[test]
    fn package_name_routes_the_node_scheme_to_types_node() {
        assert_eq!(package_name("node:crypto"), "@types/node");
    }

    #[test]
    fn types_package_name_prefixes_a_plain_package() {
        assert_eq!(types_package_name("react"), "@types/react");
    }

    #[test]
    fn types_package_name_collapses_a_scope() {
        assert_eq!(types_package_name("@scope/pkg"), "@types/scope__pkg");
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
    fn find_package_dir_walks_up_to_a_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/react")).unwrap();
        let app = root.path().join("apps/web");
        std::fs::create_dir_all(&app).unwrap();

        assert!(find_package_dir("react", &app).is_some());
    }

    #[test]
    fn find_package_dir_accepts_a_types_only_install() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/@types/react")).unwrap();

        assert!(find_package_dir("react", root.path()).is_some());
    }

    #[test]
    fn find_package_dir_reports_an_absent_package() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/react")).unwrap();

        assert!(find_package_dir("preact", root.path()).is_none());
    }

    #[test]
    fn find_package_dir_accepts_a_package_without_declarations() {
        // No `.d.ts` anywhere in it. Presence is the whole question here:
        // a package with no types is W004's problem, not E013's.
        let root = tempfile::tempdir().unwrap();
        let pkg = root.path().join("node_modules/no-types");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), r#"{"name":"no-types"}"#).unwrap();

        assert!(find_package_dir("no-types", root.path()).is_some());
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

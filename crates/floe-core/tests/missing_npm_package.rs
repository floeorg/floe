//! End to end for #1465: a `node_modules` directory on disk decides
//! whether an import reports E013.
//!
//! The unit tests in `interop::packages` cover the disk walk and the
//! ones in `checker::tests` cover the diagnostic. This file joins the
//! two, because the bug was in the seam: the language server ran one
//! resolver and the compiler ran none.

use std::collections::HashMap;

use floe_core::analyse::{self, ExternTypes, ModuleInputs};
use floe_core::checker::ErrorCode;
use floe_core::diagnostic::{Diagnostic, Severity};
use floe_core::interop::packages;
use floe_core::parser::Parser;
use floe_core::resolve::TsconfigPaths;

/// Resolve `source` against a project directory holding `installed`
/// packages, and return the diagnostics the compiler reports.
fn diagnose(source: &str, installed: &[&str]) -> Vec<Diagnostic> {
    let project = tempfile::tempdir().expect("temp project");
    for package in installed {
        std::fs::create_dir_all(project.path().join("node_modules").join(package))
            .expect("install fixture package");
    }

    let program = Parser::new(source).parse_program().expect("fixture parses");
    let missing_npm_packages = packages::find_missing_packages(
        &program,
        &HashMap::new(),
        &TsconfigPaths::default(),
        project.path(),
        project.path(),
    );

    analyse::analyse_parsed(
        program,
        ModuleInputs {
            resolved_imports: HashMap::new(),
            externs: ExternTypes {
                missing_npm_packages,
                ..ExternTypes::default()
            },
        },
    )
    .diagnostics
}

/// Same as [`diagnose`], but the project always holds a `node_modules`
/// directory, so the Plug'n'Play guard never hides the answer.
fn diagnose_in(source: &str, installed: &[&str]) -> Vec<Diagnostic> {
    let project = tempfile::tempdir().expect("temp project");
    std::fs::create_dir_all(project.path().join("node_modules")).expect("node_modules");
    for package in installed {
        std::fs::create_dir_all(project.path().join("node_modules").join(package))
            .expect("install fixture package");
    }

    let program = Parser::new(source).parse_program().expect("fixture parses");
    let missing_npm_packages = packages::find_missing_packages(
        &program,
        &HashMap::new(),
        &TsconfigPaths::default(),
        project.path(),
        project.path(),
    );

    analyse::analyse_parsed(
        program,
        ModuleInputs {
            resolved_imports: HashMap::new(),
            externs: ExternTypes {
                missing_npm_packages,
                ..ExternTypes::default()
            },
        },
    )
    .diagnostics
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|d| d.code.clone())
        .collect::<Vec<_>>()
}

#[test]
fn an_absent_package_fails_the_check() {
    let diagnostics = diagnose(
        r#"
import trusted { shout } from "absent-package"
export let main() -> string = { shout("hello") }
"#,
        &[],
    );

    assert!(
        codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "expected E013, got: {:?}",
        codes(&diagnostics)
    );
    assert!(
        diagnostics.iter().any(|d| d.severity == Severity::Error),
        "E013 must fail the check, got: {:?}",
        diagnostics
    );
}

#[test]
fn an_installed_package_without_declarations_passes_the_check() {
    // The directory exists and holds nothing. Floe cannot type the
    // symbol, so it warns W004, and the check still exits 0.
    let diagnostics = diagnose(
        r#"
import trusted { whisper } from "present-package"
export let main() -> string = { whisper("hello") }
"#,
        &["present-package"],
    );

    assert!(
        !codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "an installed package must not report E013, got: {:?}",
        codes(&diagnostics)
    );
    assert!(
        codes(&diagnostics).contains(&ErrorCode::UncheckedForeignArguments.code().to_string()),
        "expected W004, got: {:?}",
        codes(&diagnostics)
    );
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "W004 must keep the check green, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_subpath_import_reads_the_package_directory() {
    let diagnostics = diagnose(
        r#"
import trusted { format } from "present-package/format"
export let main() -> string = { format("today") }
"#,
        &["present-package"],
    );

    assert!(
        !codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "a subpath of an installed package must not report E013, got: {:?}",
        codes(&diagnostics)
    );
}

#[test]
fn a_relative_import_is_not_an_npm_package() {
    let diagnostics = diagnose(
        r#"
import { Todo } from "./types"
export let main() -> string = { "ok" }
"#,
        &[],
    );

    assert!(
        !codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "a relative import must not report E013, got: {:?}",
        codes(&diagnostics)
    );
}

/// `floe check` caches a clean result per module. The cache fingerprints
/// `.fl` bytes only, so it used to serve that clean result after somebody
/// deleted the package: `floe check` said 0 and `floe build` said 1 about
/// the same tree, which is the split #1465 exists to close.
#[test]
fn removing_a_package_invalidates_a_clean_cached_check() {
    use floe_core::build::PackageCompiler;

    let project = tempfile::tempdir().expect("temp project");
    let root = project.path();
    let package = root.join("node_modules/present-package");
    std::fs::create_dir_all(&package).expect("install fixture package");
    std::fs::create_dir_all(root.join("src")).expect("src");
    let source = "import trusted { whisper } from \"present-package\"\nexport let main() -> string = { whisper(\"hello\") }\n";
    let module = root.join("src/main.fl");
    std::fs::write(&module, source).expect("write module");

    let compiler = PackageCompiler::new(root.to_path_buf()).with_cache(root.join(".floe/cache"));

    let first = compiler.check_file(&module, source);
    assert!(
        first.iter().all(|d| d.severity != Severity::Error),
        "the package is installed, so the first check must be clean, got: {first:?}"
    );

    // A second check with nothing changed reads the cache. It must stay
    // clean, or the cache does nothing at all.
    let cached = compiler.check_file(&module, source);
    assert!(
        cached.iter().all(|d| d.severity != Severity::Error),
        "an unchanged module must stay clean, got: {cached:?}"
    );

    std::fs::remove_dir_all(&package).expect("remove the package");

    let after = compiler.check_file(&module, source);
    assert!(
        codes(&after).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "removing the package must invalidate the cached result and report E013, got: {:?}",
        codes(&after)
    );
}

/// A bare Node builtin is not a package. Reporting one told people to run
/// `npm install fs`, which installs a real deprecated stub.
#[test]
fn a_bare_node_builtin_is_not_a_missing_package() {
    let diagnostics = diagnose_in(
        r#"
import trusted { readFileSync } from "fs"
export let main() -> string = { readFileSync("a.txt") }
"#,
        &["@types/node"],
    );

    assert!(
        !codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "a builtin typed by @types/node must not report E013, got: {:?}",
        codes(&diagnostics)
    );
}

/// `#lib/helper` is resolved by `package.json` `imports`, so no install
/// fixes it and E013 must not name one.
#[test]
fn a_node_subpath_import_is_not_a_missing_package() {
    let diagnostics = diagnose_in(
        r##"
import trusted { helper } from "#lib/helper"
export let main() -> string = { helper() }
"##,
        &[],
    );

    assert!(
        !codes(&diagnostics).contains(&ErrorCode::PackageNotFound.code().to_string()),
        "a `#` subpath import must not report E013, got: {:?}",
        codes(&diagnostics)
    );
}

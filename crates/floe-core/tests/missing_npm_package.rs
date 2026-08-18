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

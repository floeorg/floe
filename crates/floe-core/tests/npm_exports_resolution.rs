//! Integration tests for npm `exports` resolution (issue #1433).
//!
//! Each test installs a synthetic package into a temporary `node_modules`
//! tree, resolves its declaration file the way the compiler does, and runs the
//! checker over Floe code that imports from it. No network and no npm.

use std::collections::HashMap;
use std::path::Path;

use floe_core::checker::{Checker, ErrorCode};
use floe_core::interop::{DtsExport, find_package_dts, parse_dts_exports};
use floe_core::parser::Parser;

/// Write a package into a synthetic `node_modules` tree under `root`.
fn install(root: &Path, name: &str, manifest: &str, files: &[(&str, &str)]) {
    let pkg_dir = root.join("node_modules").join(name);
    std::fs::create_dir_all(&pkg_dir).expect("create package directory");
    std::fs::write(pkg_dir.join("package.json"), manifest).expect("write manifest");
    for (relative, content) in files {
        let path = pkg_dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).expect("create dir");
        std::fs::write(path, content).expect("write file");
    }
}

/// Resolve one specifier the way the compiler does and parse its exports.
fn dts_exports(root: &Path, specifier: &str) -> Option<Vec<DtsExport>> {
    let path = find_package_dts(root, specifier)?;

    parse_dts_exports(&path).ok()
}

/// Check Floe source against the declaration files of the named specifiers.
/// Returns the diagnostics and the inferred type of every top-level binding.
fn check_with_packages(
    root: &Path,
    specifiers: &[&str],
    source: &str,
) -> (
    Vec<floe_core::diagnostic::Diagnostic>,
    HashMap<String, String>,
) {
    let mut dts_imports = HashMap::new();
    for specifier in specifiers {
        let exports = dts_exports(root, specifier)
            .unwrap_or_else(|| panic!("specifier `{specifier}` should resolve"));
        dts_imports.insert((*specifier).to_string(), exports);
    }
    let program = Parser::new(source)
        .parse_program()
        .expect("fixture should parse");
    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, types, _, _) = checker.check_with_types(&program);

    (diags, types)
}

fn has_code(diags: &[floe_core::diagnostic::Diagnostic], code: ErrorCode) -> bool {
    diags
        .iter()
        .any(|diag| diag.code.as_deref() == Some(code.code()))
}

/// A `package.json` shaped like `date-fns-tz@3.2.0`: `exports` points at `.js`
/// files and declares no `types` condition.
const EXPORTS_WITHOUT_TYPES: &str = r#"{
    "name": "zoned",
    "exports": {
        "./package.json": "./package.json",
        ".": { "import": "./dist/esm/index.js", "require": "./dist/cjs/index.js" },
        "./toZonedTime": {
            "import": "./dist/esm/toZonedTime/index.js",
            "require": "./dist/cjs/toZonedTime/index.js"
        }
    }
}"#;

/// The declaration that both the root entry and the subpath entry ship.
const TO_ZONED_TIME: &str =
    "export declare function toZonedTime(date: string, tz: string): string;\n";

#[test]
fn exports_without_a_types_condition_type_checks_its_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    install(
        root,
        "zoned",
        EXPORTS_WITHOUT_TYPES,
        &[
            ("dist/esm/index.js", "export function toZonedTime() {}"),
            ("dist/esm/index.d.ts", TO_ZONED_TIME),
        ],
    );

    let (diags, types) = check_with_packages(
        root,
        &["zoned"],
        r#"
import trusted { toZonedTime } from "zoned"
let _shifted = toZonedTime("2024-01-01", "Asia/Tokyo")
"#,
    );

    assert!(
        !has_code(&diags, ErrorCode::UncheckedForeignArguments),
        "call should be type-checked, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(types.get("_shifted").map(String::as_str), Some("string"));
}

#[test]
fn a_subpath_import_type_checks_its_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    install(
        root,
        "zoned",
        EXPORTS_WITHOUT_TYPES,
        &[
            ("dist/esm/index.d.ts", TO_ZONED_TIME),
            ("dist/esm/toZonedTime/index.d.ts", TO_ZONED_TIME),
        ],
    );

    let (diags, types) = check_with_packages(
        root,
        &["zoned/toZonedTime"],
        r#"
import trusted { toZonedTime } from "zoned/toZonedTime"
let _shifted = toZonedTime("2024-01-01", "Asia/Tokyo")
"#,
    );

    assert!(
        !has_code(&diags, ErrorCode::UncheckedForeignArguments),
        "subpath call should be type-checked, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(types.get("_shifted").map(String::as_str), Some("string"));
}

#[test]
fn a_target_that_names_a_directory_type_checks_its_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    install(
        root,
        "dirpkg",
        r#"{ "name": "dirpkg", "exports": { ".": { "import": "./lib" } } }"#,
        &[(
            "lib/index.d.ts",
            "export declare function shout(text: string): string;\n",
        )],
    );

    let (diags, types) = check_with_packages(
        root,
        &["dirpkg"],
        r#"
import trusted { shout } from "dirpkg"
let _loud = shout("hey")
"#,
    );

    assert!(
        !has_code(&diags, ErrorCode::UncheckedForeignArguments),
        "call should be type-checked, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(types.get("_loud").map(String::as_str), Some("string"));
}

/// Regression for `date-fns@4.1.0`, which declares `types` conditions inside
/// nested `import` and `require` entries.
#[test]
fn explicit_types_conditions_still_type_check_their_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    install(
        root,
        "dates",
        r#"{
            "name": "dates",
            "exports": {
                ".": {
                    "require": { "types": "./index.d.cts", "default": "./index.cjs" },
                    "import": { "types": "./index.d.ts", "default": "./index.js" }
                }
            }
        }"#,
        &[
            (
                "index.d.ts",
                "export declare function addDays(date: string, amount: number): string;\n",
            ),
            ("index.d.cts", "export declare function addDays(): void;\n"),
        ],
    );

    let (diags, types) = check_with_packages(
        root,
        &["dates"],
        r#"
import trusted { addDays } from "dates"
let _later = addDays("2024-01-01", 1)
"#,
    );

    assert!(
        !has_code(&diags, ErrorCode::UncheckedForeignArguments),
        "call should be type-checked, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(types.get("_later").map(String::as_str), Some("string"));
}

/// The fallback must not invent a declaration file. A package that ships only
/// JavaScript stays unresolved, so the caller can report E013.
#[test]
fn a_package_without_any_declaration_file_stays_unresolved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    install(
        root,
        "jsonly",
        r#"{ "name": "jsonly", "exports": { ".": { "import": "./dist/index.js" } } }"#,
        &[("dist/index.js", "export const value = 1;\n")],
    );

    assert!(find_package_dts(root, "jsonly").is_none());
}

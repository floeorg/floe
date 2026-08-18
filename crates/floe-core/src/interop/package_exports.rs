//! Declaration file resolution for npm packages.
//!
//! Reads the `exports` field of a `package.json` the way TypeScript's
//! `node16`, `nodenext` and `bundler` resolvers read it, then finds the
//! `.d.ts` file that backs the target it names.
//!
//! Many packages point `exports` at a `.js` file and declare no `types`
//! condition. TypeScript still finds the declaration file, because it swaps
//! the JavaScript extension for a declaration extension on the resolved path.
//! This module does the same.
//!
//! The `exports` walk is pure: it reads a `serde_json::Value` and returns a
//! relative path. The filesystem probes live in separate functions below it.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Declaration file extensions, in the order TypeScript tries them.
const DTS_EXTENSIONS: [&str; 3] = [".d.ts", ".d.mts", ".d.cts"];

/// JavaScript file extensions that an `exports` target can carry.
const JS_EXTENSIONS: [&str; 3] = [".js", ".mjs", ".cjs"];

/// The condition that names a declaration file.
const TYPES_CONDITION: &str = "types";

/// Conditions in the order this resolver prefers them. `types` comes first at
/// every nesting level, so a declared `.d.ts` always wins over a runtime entry
/// point.
const CONDITION_ORDER: [&str; 4] = [TYPES_CONDITION, "import", "default", "require"];

/// Stops a hand-written `package.json` from nesting conditions without end.
const MAX_CONDITION_DEPTH: usize = 16;

/// The path an `exports` map names for one subpath.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportsTarget {
    /// The relative path the map named, such as `./dist/esm/index.js`.
    pub path: String,
    /// True when a `types` condition produced the path. The path is then a
    /// declaration file already, so no extension swap is needed.
    pub from_types_condition: bool,
}

/// Split an import specifier into its package name and its `exports` key.
///
/// `react` becomes `("react", ".")`, `date-fns-tz/toZonedTime` becomes
/// `("date-fns-tz", "./toZonedTime")`, and `@scope/name/sub` becomes
/// `("@scope/name", "./sub")`.
pub fn split_specifier(specifier: &str) -> (&str, String) {
    let name_segments = usize::from(specifier.starts_with('@')) + 1;
    let Some((index, _)) = specifier.match_indices('/').nth(name_segments - 1) else {
        return (specifier, ".".to_string());
    };

    (&specifier[..index], format!(".{}", &specifier[index..]))
}

/// The `node_modules/@types` directory name for a package. DefinitelyTyped
/// publishes `@scope/name` as `scope__name`.
pub fn types_package_name(package: &str) -> String {
    if let Some(rest) = package.strip_prefix('@') {
        return rest.replace('/', "__");
    }

    package.to_string()
}

/// Pick the target that an `exports` field names for one subpath.
///
/// `subpath` is an `exports` key: `.` for the package root, `./name` for a
/// subpath import. This function is pure, so it reads no files.
pub fn resolve_exports_target(exports: &Value, subpath: &str) -> Option<ExportsTarget> {
    let (entry, substitution) = select_subpath_entry(exports, subpath)?;
    let mut target = pick_conditional_target(entry, 0)?;
    if let Some(matched) = substitution {
        target.path = target.path.replace('*', &matched);
    }

    Some(target)
}

/// Find the `exports` entry for one subpath, plus the text a `*` pattern
/// matched. An `exports` field is either a bare string, a map of subpath keys
/// that start with `.`, or a map of conditions that describes the root only.
fn select_subpath_entry<'a>(
    exports: &'a Value,
    subpath: &str,
) -> Option<(&'a Value, Option<String>)> {
    let Value::Object(map) = exports else {
        return (subpath == ".").then_some((exports, None));
    };
    if !map.keys().any(|key| key.starts_with('.')) {
        return (subpath == ".").then_some((exports, None));
    }
    if let Some(entry) = map.get(subpath) {
        return Some((entry, None));
    }

    match_pattern_key(map, subpath)
}

/// Match a subpath against the `*` pattern keys of a subpath map. Node picks
/// the key with the longest literal text before the `*`.
fn match_pattern_key<'a>(
    map: &'a Map<String, Value>,
    subpath: &str,
) -> Option<(&'a Value, Option<String>)> {
    let mut best: Option<(usize, &Value, String)> = None;
    for (key, entry) in map {
        let Some((prefix, suffix)) = key.split_once('*') else {
            continue;
        };
        let Some(matched) = strip_pattern(subpath, prefix, suffix) else {
            continue;
        };
        if best
            .as_ref()
            .is_some_and(|(best, _, _)| *best >= prefix.len())
        {
            continue;
        }
        best = Some((prefix.len(), entry, matched.to_string()));
    }

    best.map(|(_, entry, matched)| (entry, Some(matched)))
}

/// Return the text a `*` matched between a pattern's prefix and suffix.
fn strip_pattern<'a>(subpath: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    if suffix.contains('*') {
        return None;
    }

    subpath.strip_prefix(prefix)?.strip_suffix(suffix)
}

/// Walk a conditional export value and take the first target that a condition
/// this resolver understands names. Conditions nest, so this recurses.
fn pick_conditional_target(value: &Value, depth: usize) -> Option<ExportsTarget> {
    if depth > MAX_CONDITION_DEPTH {
        return None;
    }
    match value {
        Value::String(path) => Some(ExportsTarget {
            path: path.clone(),
            from_types_condition: false,
        }),
        Value::Array(items) => items
            .iter()
            .find_map(|item| pick_conditional_target(item, depth + 1)),
        Value::Object(map) => CONDITION_ORDER
            .iter()
            .find_map(|condition| pick_condition(map, condition, depth)),
        Value::Null | Value::Bool(_) | Value::Number(_) => None,
    }
}

/// Read one condition out of a conditional export map.
fn pick_condition(
    map: &Map<String, Value>,
    condition: &str,
    depth: usize,
) -> Option<ExportsTarget> {
    let mut target = pick_conditional_target(map.get(condition)?, depth + 1)?;
    target.from_types_condition |= condition == TYPES_CONDITION;

    Some(target)
}

/// Find the declaration file that backs an import specifier.
///
/// Looks in `<project_dir>/node_modules/<package>` first, then in
/// `<project_dir>/node_modules/@types/<package>`.
pub fn find_package_dts(project_dir: &Path, specifier: &str) -> Option<PathBuf> {
    let node_modules = project_dir.join("node_modules");
    if let Some(submodule) = specifier.strip_prefix("node:") {
        return find_node_builtin_dts(&node_modules, submodule);
    }
    let (package, subpath) = split_specifier(specifier);
    let candidates = [
        node_modules.join(package),
        node_modules
            .join("@types")
            .join(types_package_name(package)),
    ];

    candidates
        .iter()
        .find_map(|pkg_dir| find_dts_in_package(pkg_dir, &subpath))
}

/// Resolve a `node:X` specifier. Those declarations live inside
/// `@types/node/X.d.ts`, or in the package index, as `declare module "node:X"`
/// blocks. Callers pair this with `parse_dts_exports_for_specifier`.
fn find_node_builtin_dts(node_modules: &Path, submodule: &str) -> Option<PathBuf> {
    let at_node = node_modules.join("@types").join("node");
    let sub_dts = at_node.join(format!("{submodule}.d.ts"));
    if sub_dts.is_file() {
        return Some(sub_dts);
    }
    let index_dts = at_node.join("index.d.ts");

    index_dts.is_file().then_some(index_dts)
}

/// Find the declaration file for one subpath inside an installed package.
///
/// Reads the `exports` map first, because that is what `node16`, `nodenext`
/// and `bundler` read. Falls back to the `types` and `typings` fields and to
/// the package root index, which is what `node10` reads.
pub fn find_dts_in_package(pkg_dir: &Path, subpath: &str) -> Option<PathBuf> {
    let manifest = read_manifest(pkg_dir);

    if let Some(json) = manifest.as_ref()
        && let Some(target) = resolve_exports_target(&json["exports"], subpath)
        && let Some(found) = probe_target(pkg_dir, &target)
    {
        return Some(found);
    }

    if subpath != "." {
        return probe_subpath_without_exports(pkg_dir, subpath);
    }

    if let Some(json) = manifest.as_ref()
        && let Some(found) = probe_types_fields(pkg_dir, json)
    {
        return Some(found);
    }

    probe_extensions(pkg_dir, "index")
}

/// Read and parse a package manifest. Returns `None` when the file is absent
/// or malformed.
fn read_manifest(pkg_dir: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(pkg_dir.join("package.json")).ok()?;

    serde_json::from_str(&content).ok()
}

/// Turn an `exports` target into the declaration file that backs it.
fn probe_target(pkg_dir: &Path, target: &ExportsTarget) -> Option<PathBuf> {
    let relative = target.path.trim_start_matches("./");
    let full = pkg_dir.join(relative);

    // A `types` condition, or a map that names a declaration file outright,
    // already points at the file we want.
    if (target.from_types_condition || is_declaration_path(relative)) && full.is_file() {
        return Some(full);
    }

    // The map named a JavaScript file, so swap its extension.
    if let Some(stem) = strip_js_extension(relative)
        && let Some(found) = probe_extensions(pkg_dir, stem)
    {
        return Some(found);
    }

    // The map named a directory, so read the index declaration file in it.
    if full.is_dir() {
        return probe_extensions(&full, "index");
    }

    None
}

/// Resolve a subpath the way `node10` does, for a package that ships no
/// `exports` map or that omits this subpath from it.
fn probe_subpath_without_exports(pkg_dir: &Path, subpath: &str) -> Option<PathBuf> {
    let relative = subpath.trim_start_matches("./");
    if let Some(found) = probe_extensions(pkg_dir, relative) {
        return Some(found);
    }
    let dir = pkg_dir.join(relative);
    if dir.is_dir() {
        return probe_extensions(&dir, "index");
    }

    None
}

/// Read the top-level `types` and `typings` fields of a package manifest.
fn probe_types_fields(pkg_dir: &Path, manifest: &Value) -> Option<PathBuf> {
    ["types", "typings"].iter().find_map(|field| {
        let declared = manifest[*field].as_str()?;
        let full = pkg_dir.join(declared.trim_start_matches("./"));

        full.is_file().then_some(full)
    })
}

/// Try each declaration extension against one path stem inside a directory.
fn probe_extensions(dir: &Path, stem: &str) -> Option<PathBuf> {
    DTS_EXTENSIONS
        .iter()
        .map(|extension| dir.join(format!("{stem}{extension}")))
        .find(|candidate| candidate.is_file())
}

/// True when a path already carries a declaration file extension.
fn is_declaration_path(path: &str) -> bool {
    DTS_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// Drop a JavaScript extension from a path, so the caller can add a
/// declaration extension in its place.
fn strip_js_extension(path: &str) -> Option<&str> {
    JS_EXTENSIONS
        .iter()
        .find_map(|extension| path.strip_suffix(extension))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        ExportsTarget, find_package_dts, resolve_exports_target, split_specifier,
        types_package_name,
    };

    fn json(source: &str) -> serde_json::Value {
        serde_json::from_str(source).expect("test fixture should parse")
    }

    fn target(path: &str, from_types_condition: bool) -> ExportsTarget {
        ExportsTarget {
            path: path.to_string(),
            from_types_condition,
        }
    }

    // ── split_specifier ────────────────────────────────────────

    #[test]
    fn bare_package_name_has_the_root_subpath() {
        assert_eq!(split_specifier("react"), ("react", ".".to_string()));
    }

    #[test]
    fn package_name_splits_from_its_subpath() {
        assert_eq!(
            split_specifier("date-fns-tz/toZonedTime"),
            ("date-fns-tz", "./toZonedTime".to_string())
        );
    }

    #[test]
    fn scoped_package_keeps_both_name_segments() {
        assert_eq!(
            split_specifier("@scope/name"),
            ("@scope/name", ".".to_string())
        );
    }

    #[test]
    fn scoped_package_splits_from_its_subpath() {
        assert_eq!(
            split_specifier("@scope/name/deep/sub"),
            ("@scope/name", "./deep/sub".to_string())
        );
    }

    #[test]
    fn scoped_package_maps_to_a_flattened_types_directory() {
        assert_eq!(types_package_name("@scope/name"), "scope__name");
        assert_eq!(types_package_name("react"), "react");
    }

    // ── exports shape: bare string ─────────────────────────────

    #[test]
    fn string_exports_serves_the_root_subpath() {
        let exports = json(r#""./dist/index.js""#);
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./dist/index.js", false))
        );
    }

    #[test]
    fn string_exports_serves_no_other_subpath() {
        let exports = json(r#""./dist/index.js""#);
        assert_eq!(resolve_exports_target(&exports, "./sub"), None);
    }

    // ── exports shape: subpath map ─────────────────────────────

    #[test]
    fn subpath_map_without_a_types_condition_yields_the_javascript_target() {
        let exports = json(
            r#"{
                "./package.json": "./package.json",
                ".":              { "import": "./dist/esm/index.js", "require": "./dist/cjs/index.js" },
                "./toZonedTime":  { "import": "./dist/esm/toZonedTime/index.js", "require": "./dist/cjs/toZonedTime/index.js" }
            }"#,
        );
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./dist/esm/index.js", false))
        );
        assert_eq!(
            resolve_exports_target(&exports, "./toZonedTime"),
            Some(target("./dist/esm/toZonedTime/index.js", false))
        );
    }

    #[test]
    fn subpath_map_returns_none_for_a_subpath_it_does_not_list() {
        let exports = json(r#"{ ".": "./index.js", "./one": "./one.js" }"#);
        assert_eq!(resolve_exports_target(&exports, "./two"), None);
    }

    #[test]
    fn subpath_map_matches_the_longest_star_pattern() {
        let exports = json(r#"{ "./*": "./dist/*.js", "./deep/*": "./dist/deep/*.js" }"#);
        assert_eq!(
            resolve_exports_target(&exports, "./deep/thing"),
            Some(target("./dist/deep/thing.js", false))
        );
        assert_eq!(
            resolve_exports_target(&exports, "./thing"),
            Some(target("./dist/thing.js", false))
        );
    }

    // ── exports shape: condition map for the root ──────────────

    #[test]
    fn condition_map_serves_the_root_subpath_only() {
        let exports = json(r#"{ "import": "./index.mjs", "require": "./index.cjs" }"#);
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./index.mjs", false))
        );
        assert_eq!(resolve_exports_target(&exports, "./sub"), None);
    }

    #[test]
    fn condition_map_prefers_a_types_condition_over_the_runtime_entry() {
        let exports = json(r#"{ "types": "./index.d.ts", "default": "./index.js" }"#);
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./index.d.ts", true))
        );
    }

    // ── nested conditions ──────────────────────────────────────

    #[test]
    fn nested_conditions_yield_the_import_types_entry() {
        let exports = json(
            r#"{
                ".": {
                    "require": { "types": "./index.d.cts", "default": "./index.cjs" },
                    "import":  { "types": "./index.d.ts",  "default": "./index.js" }
                }
            }"#,
        );
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./index.d.ts", true))
        );
    }

    #[test]
    fn nested_conditions_fall_through_to_default_without_a_types_condition() {
        let exports = json(r#"{ ".": { "browser": { "default": "./browser.js" } } }"#);
        assert_eq!(resolve_exports_target(&exports, "."), None);
    }

    #[test]
    fn an_array_target_takes_its_first_usable_entry() {
        let exports = json(r#"{ ".": [{ "unknown-condition": "./no.js" }, "./yes.js"] }"#);
        assert_eq!(
            resolve_exports_target(&exports, "."),
            Some(target("./yes.js", false))
        );
    }

    #[test]
    fn a_null_target_blocks_the_subpath() {
        let exports = json(r#"{ ".": "./index.js", "./private": null }"#);
        assert_eq!(resolve_exports_target(&exports, "./private"), None);
    }

    #[test]
    fn a_missing_exports_field_yields_no_target() {
        let manifest = json(r#"{ "name": "thing" }"#);
        assert_eq!(resolve_exports_target(&manifest["exports"], "."), None);
    }

    // ── filesystem probes ──────────────────────────────────────

    /// Write a package into a synthetic `node_modules` tree and return the
    /// project root that holds it.
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

    fn tail(path: &PathBuf, root: &Path) -> String {
        path.strip_prefix(root)
            .expect("resolved path sits under the project root")
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// The bug in #1433: `exports` names a `.js` file and declares no `types`
    /// condition, so the resolver has to swap the extension.
    #[test]
    fn exports_without_a_types_condition_resolves_the_sibling_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "date-fns-tz",
            r#"{ "name": "date-fns-tz", "exports": {
                ".": { "import": "./dist/esm/index.js", "require": "./dist/cjs/index.js" },
                "./toZonedTime": { "import": "./dist/esm/toZonedTime/index.js" }
            } }"#,
            &[
                ("dist/esm/index.js", ""),
                (
                    "dist/esm/index.d.ts",
                    "export declare function toZonedTime(): Date;",
                ),
            ],
        );

        let resolved = find_package_dts(root, "date-fns-tz").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/date-fns-tz/dist/esm/index.d.ts"
        );
    }

    #[test]
    fn a_subpath_import_resolves_the_declaration_file_for_that_subpath() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "date-fns-tz",
            r#"{ "name": "date-fns-tz", "exports": {
                ".": { "import": "./dist/esm/index.js" },
                "./toZonedTime": { "import": "./dist/esm/toZonedTime/index.js" }
            } }"#,
            &[
                ("dist/esm/index.d.ts", ""),
                (
                    "dist/esm/toZonedTime/index.d.ts",
                    "export declare function toZonedTime(): Date;",
                ),
            ],
        );

        let resolved =
            find_package_dts(root, "date-fns-tz/toZonedTime").expect("should resolve subpath");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/date-fns-tz/dist/esm/toZonedTime/index.d.ts"
        );
    }

    #[test]
    fn a_target_that_names_a_directory_resolves_its_index_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "dirpkg",
            r#"{ "name": "dirpkg", "exports": { ".": { "import": "./lib" } } }"#,
            &[("lib/index.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "dirpkg").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/dirpkg/lib/index.d.ts");
    }

    /// Regression for packages that do declare `types` conditions, such as
    /// `date-fns@4.1.0`. The nested `import` entry must still win.
    #[test]
    fn explicit_types_conditions_still_resolve() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "date-fns",
            r#"{ "name": "date-fns", "exports": {
                ".": {
                    "require": { "types": "./index.d.cts", "default": "./index.cjs" },
                    "import": { "types": "./index.d.ts", "default": "./index.js" }
                },
                "./addDays": {
                    "require": { "types": "./addDays.d.cts", "default": "./addDays.cjs" },
                    "import": { "types": "./addDays.d.ts", "default": "./addDays.js" }
                }
            } }"#,
            &[
                ("index.d.ts", "export declare function addDays(): Date;"),
                ("index.d.cts", ""),
                ("addDays.d.ts", "export declare function addDays(): Date;"),
            ],
        );

        let root_dts = find_package_dts(root, "date-fns").expect("should resolve root");
        assert_eq!(tail(&root_dts, root), "node_modules/date-fns/index.d.ts");
        let subpath_dts = find_package_dts(root, "date-fns/addDays").expect("should resolve");
        assert_eq!(
            tail(&subpath_dts, root),
            "node_modules/date-fns/addDays.d.ts"
        );
    }

    #[test]
    fn a_top_level_types_field_still_resolves() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "oldpkg",
            r#"{ "name": "oldpkg", "types": "./types/main.d.ts" }"#,
            &[("types/main.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "oldpkg").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/oldpkg/types/main.d.ts");
    }

    #[test]
    fn a_package_root_index_declaration_file_still_resolves() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "plainpkg",
            r#"{ "name": "plainpkg" }"#,
            &[("index.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "plainpkg").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/plainpkg/index.d.ts");
    }

    #[test]
    fn a_scoped_package_reads_its_flattened_types_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(root, "@scope/name", r#"{ "name": "@scope/name" }"#, &[]);
        install(
            root,
            "@types/scope__name",
            r#"{ "name": "@types/scope__name", "types": "./index.d.ts" }"#,
            &[("index.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "@scope/name").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/@types/scope__name/index.d.ts"
        );
    }

    /// A package that ships no declaration file at all must stay unresolved,
    /// so the caller reports E013 instead of typing the import as `unknown`.
    #[test]
    fn a_package_without_any_declaration_file_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "jsonly",
            r#"{ "name": "jsonly", "exports": { ".": { "import": "./dist/index.js" } } }"#,
            &[("dist/index.js", "export const value = 1;")],
        );

        assert_eq!(find_package_dts(root, "jsonly"), None);
    }

    #[test]
    fn a_package_that_is_not_installed_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(find_package_dts(dir.path(), "never-installed"), None);
    }
}

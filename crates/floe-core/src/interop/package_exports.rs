//! Declaration file resolution for npm packages.
//!
//! Reads the `exports` field of a `package.json` the way TypeScript's
//! `bundler` resolver reads it, then finds the `.d.ts` file that backs the
//! target it names.
//!
//! This module matches `bundler` only. `node16` and `nodenext` read the same
//! field in two ways that this module does not copy: they apply the `node`
//! condition, and they honour the order in which the `package.json` declares
//! its conditions. This module never applies `node`, and it walks a fixed
//! condition order instead of the declared one.
//!
//! Many packages point `exports` at a `.js` file and declare no `types`
//! condition. TypeScript still finds the declaration file, because it swaps
//! the JavaScript extension for a declaration extension on the resolved path.
//! This module does the same.
//!
//! A `package.json` inside `node_modules` is untrusted input. Every
//! filesystem probe therefore runs through `probe_file` or `probe_dir`, which
//! reject a path that leaves the package directory through `..`, through an
//! absolute path, or through a symlink.
//!
//! The `exports` walk is pure: it reads a `serde_json::Value` and returns a
//! relative path. The filesystem probes live in separate functions below it.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

/// Declaration file extensions, in the order TypeScript tries them.
const DTS_EXTENSIONS: [&str; 3] = [".d.ts", ".d.mts", ".d.cts"];

/// The declaration extension that TypeScript pairs with each JavaScript
/// extension. A target of `./index.mjs` names `./index.d.mts`, not
/// `./index.d.ts`.
const JS_TO_DTS_EXTENSION: [(&str, &str); 3] =
    [(".mjs", ".d.mts"), (".cjs", ".d.cts"), (".js", ".d.ts")];

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

/// Match a subpath against the `*` pattern keys of a subpath map.
///
/// This follows Node's `PATTERN_KEY_COMPARE`: the key with the longest
/// literal text before the `*` wins, and the longer whole key breaks a tie.
/// `./a*.js` therefore beats `./a*` for the subpath `./abc.js`.
fn match_pattern_key<'a>(
    map: &'a Map<String, Value>,
    subpath: &str,
) -> Option<(&'a Value, Option<String>)> {
    let mut best: Option<((usize, usize), &Value, String)> = None;
    for (key, entry) in map {
        let Some((prefix, suffix)) = key.split_once('*') else {
            continue;
        };
        let Some(matched) = strip_pattern(subpath, prefix, suffix) else {
            continue;
        };
        let rank = (prefix.len(), key.len());
        if best.as_ref().is_some_and(|(best, _, _)| *best >= rank) {
            continue;
        }
        best = Some((rank, entry, matched.to_string()));
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
    // The package segment names the directory that every later containment
    // check measures against, so a segment of `..` would move that directory
    // up to the project. A package supplies this string: `import("...")`
    // inside a `.d.ts` reaches here verbatim.
    if !is_valid_package_name(package) {
        return None;
    }
    // The `is_dir` guard keeps the walk-up in the language server cheap: an
    // ancestor that holds no such package costs one `stat`, not two file
    // reads.
    package_dir_candidates(&node_modules, package)
        .into_iter()
        .filter(|pkg_dir| pkg_dir.is_dir())
        .find_map(|pkg_dir| find_dts_in_package(&pkg_dir, &subpath))
}

/// Every directory inside one `node_modules` that can hold `package`:
/// the package itself, then its DefinitelyTyped companion.
///
/// This is the one place that says where a package lives, so
/// [`find_package_dts`], which reads the declarations, and
/// [`super::packages`], which only asks whether the package is there,
/// cannot drift apart about it (#1465).
///
/// A package already under `@types/` has no companion of its own, so
/// that candidate drops out rather than naming `@types/types__node`.
pub(super) fn package_dir_candidates(node_modules: &Path, package: &str) -> Vec<PathBuf> {
    let mut candidates = vec![node_modules.join(package)];
    if !package.starts_with("@types/") {
        candidates.push(
            node_modules
                .join("@types")
                .join(types_package_name(package)),
        );
    }

    candidates
}

/// Resolve a `node:X` specifier. Those declarations live inside
/// `@types/node/X.d.ts`, or in the package index, as `declare module "node:X"`
/// blocks. Callers pair this with `parse_dts_exports_for_specifier`.
fn find_node_builtin_dts(node_modules: &Path, submodule: &str) -> Option<PathBuf> {
    let at_node = node_modules.join("@types").join("node");
    if let Some(found) = probe_file(&at_node, &at_node, &format!("{submodule}.d.ts")) {
        return Some(found);
    }

    probe_file(&at_node, &at_node, "index.d.ts")
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
        && let Some(found) = probe_types_fields(pkg_dir, pkg_dir, json)
    {
        return Some(found);
    }

    probe_dts_extensions(pkg_dir, pkg_dir, "index", DTS_EXTENSIONS[0])
}

/// Read and parse a package manifest. Returns `None` when the file is absent
/// or malformed.
///
/// The read runs through `probe_file` like every other probe. That refuses a
/// `package.json` that is not a regular file, so a symlink to `/dev/zero` or
/// to a FIFO cannot make `read_to_string` run without end, and it refuses a
/// symlink that points at a file outside the package.
fn read_manifest(pkg_dir: &Path) -> Option<Value> {
    let manifest = probe_file(pkg_dir, pkg_dir, "package.json")?;
    let content = std::fs::read_to_string(manifest).ok()?;

    serde_json::from_str(&content).ok()
}

/// Turn an `exports` target into the declaration file that backs it.
fn probe_target(pkg_dir: &Path, target: &ExportsTarget) -> Option<PathBuf> {
    // Node rejects any target that does not start with `./`, and so does this
    // resolver. A `package.json` inside `node_modules` is untrusted input, so
    // a target of `../../../etc/passwd` must not reach the filesystem.
    let relative = target.path.strip_prefix("./")?;

    // A `types` condition, or a map that names a declaration file outright,
    // already points at the file we want.
    if (target.from_types_condition || is_declaration_path(relative))
        && let Some(found) = probe_file(pkg_dir, pkg_dir, relative)
    {
        return Some(found);
    }

    // The map named a JavaScript file, so swap its extension.
    if let Some((stem, dts_extension)) = strip_js_extension(relative)
        && let Some(found) = probe_dts_extensions(pkg_dir, pkg_dir, stem, dts_extension)
    {
        return Some(found);
    }

    // The map named a directory, so read the index declaration file in it.
    // tsgo refuses a directory target; issue #1466 owns that difference.
    let dir = probe_dir(pkg_dir, pkg_dir, relative)?;

    probe_dts_extensions(pkg_dir, &dir, "index", DTS_EXTENSIONS[0])
}

/// Resolve a subpath the way `node10` does, for a package that ships no
/// `exports` map or that omits this subpath from it.
fn probe_subpath_without_exports(pkg_dir: &Path, subpath: &str) -> Option<PathBuf> {
    let relative = subpath.trim_start_matches("./");
    if let Some(found) = probe_dts_extensions(pkg_dir, pkg_dir, relative, DTS_EXTENSIONS[0]) {
        return Some(found);
    }
    let dir = probe_dir(pkg_dir, pkg_dir, relative)?;

    // A subpath directory carries its own manifest in `firebase@8` and in
    // `date-fns@2`, and its `types` or `typings` field wins over the index
    // file beside it. `firebase/app/package.json` says `../index.d.ts`, so
    // the field can point outside its own directory but not outside the
    // package.
    if let Some(manifest) = read_manifest(&dir)
        && let Some(found) = probe_types_fields(pkg_dir, &dir, &manifest)
    {
        return Some(found);
    }

    probe_dts_extensions(pkg_dir, &dir, "index", DTS_EXTENSIONS[0])
}

/// Read the `types` and `typings` fields of a manifest. `base` is the
/// directory that holds that manifest: the package root for the package
/// manifest, or the subpath directory for a subpath manifest.
fn probe_types_fields(pkg_dir: &Path, base: &Path, manifest: &Value) -> Option<PathBuf> {
    ["types", "typings"].iter().find_map(|field| {
        let declared = manifest[*field].as_str()?;

        probe_file(pkg_dir, base, declared.trim_start_matches("./"))
    })
}

/// Try declaration extensions against one path stem inside a directory.
///
/// `preferred` goes first, because TypeScript maps the JavaScript extension
/// of a target onto one declaration extension: `.mjs` names `.d.mts` and
/// `.cjs` names `.d.cts`. The other extensions follow it, so a package that
/// ships only `.d.ts` beside an `.mjs` target still resolves.
fn probe_dts_extensions(
    pkg_dir: &Path,
    base: &Path,
    stem: &str,
    preferred: &str,
) -> Option<PathBuf> {
    std::iter::once(preferred)
        .chain(
            DTS_EXTENSIONS
                .iter()
                .copied()
                .filter(|extension| *extension != preferred),
        )
        .find_map(|extension| probe_file(pkg_dir, base, &format!("{stem}{extension}")))
}

/// Resolve `relative` against `base` and keep it when it names a file inside
/// `pkg_dir`.
fn probe_file(pkg_dir: &Path, base: &Path, relative: &str) -> Option<PathBuf> {
    probe_path(pkg_dir, base, relative, Path::is_file)
}

/// Resolve `relative` against `base` and keep it when it names a directory
/// inside `pkg_dir`.
fn probe_dir(pkg_dir: &Path, base: &Path, relative: &str) -> Option<PathBuf> {
    probe_path(pkg_dir, base, relative, Path::is_dir)
}

/// Resolve one package-relative path and reject it when it leaves the package
/// directory. The lexical check catches `..` and an absolute path, and the
/// real-path check catches a symlink that points out of the package.
fn probe_path(
    pkg_dir: &Path,
    base: &Path,
    relative: &str,
    accept: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let candidate = join_inside_package(pkg_dir, base, relative)?;
    if !accept(&candidate) {
        return None;
    }
    let (Ok(real_pkg_dir), Ok(real_candidate)) = (pkg_dir.canonicalize(), candidate.canonicalize())
    else {
        return None;
    };

    real_candidate
        .starts_with(real_pkg_dir)
        .then_some(candidate)
}

/// True when a string is a package name that npm can publish and that Node
/// accepts in a specifier.
///
/// This guard exists for safety, not for tidiness. `split_specifier` hands
/// the head of a specifier to `find_package_dts` as a directory name, and a
/// head of `..` or `.` points that directory somewhere other than a package.
pub(super) fn is_valid_package_name(package: &str) -> bool {
    let Some(scope_and_name) = package.strip_prefix('@') else {
        return is_valid_name_segment(package);
    };
    let Some((scope, name)) = scope_and_name.split_once('/') else {
        return false;
    };

    is_valid_name_segment(scope) && is_valid_name_segment(name)
}

/// True when one segment of a package name is safe to read as a directory
/// name. A segment is never empty, never starts with a dot, and never holds a
/// character that a path reads as structure.
fn is_valid_name_segment(segment: &str) -> bool {
    if segment.is_empty() || segment.starts_with('.') {
        return false;
    }

    !segment.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '/' | '\\' | '%' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
    })
}

/// Join a package-relative path onto a base directory, and reject the result
/// when it leaves the package directory. This check is lexical: it reads no
/// files, so it runs before the path reaches the filesystem.
fn join_inside_package(pkg_dir: &Path, base: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute() || relative.starts_with("/") {
        return None;
    }
    let joined = normalise(&base.join(relative));

    joined.starts_with(normalise(pkg_dir)).then_some(joined)
}

/// Remove every `.` and `..` component from a path without touching the
/// filesystem. `Path::join` and `trim_start_matches("./")` both leave `..` in
/// place, so the comparison above needs this first.
fn normalise(path: &Path) -> PathBuf {
    let mut normalised = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalised.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalised.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => normalised.push(component),
            },
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                normalised.push(component);
            }
        }
    }

    normalised
}

/// True when a path already carries a declaration file extension.
fn is_declaration_path(path: &str) -> bool {
    DTS_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// Drop a JavaScript extension from a path, and report the declaration
/// extension that TypeScript pairs with it.
fn strip_js_extension(path: &str) -> Option<(&str, &str)> {
    JS_TO_DTS_EXTENSION
        .iter()
        .find_map(|(js_extension, dts_extension)| {
            Some((path.strip_suffix(js_extension)?, *dts_extension))
        })
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

    /// The pure walk only. `find_dts_in_package` still probes the filesystem
    /// for this subpath and still finds a file, because it falls through to
    /// the `node10` probes. Issue #1466 tracks whether `exports` should block
    /// a subpath at all.
    #[test]
    fn the_walk_yields_no_target_for_a_subpath_of_a_string_exports() {
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

    /// The pure walk only. `find_dts_in_package` still probes the filesystem
    /// for `./two` and still finds a file, because it falls through to the
    /// `node10` probes. Issue #1466 tracks whether `exports` should block a
    /// subpath at all.
    #[test]
    fn the_walk_yields_no_target_for_an_unlisted_subpath() {
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

    /// The pure walk only. A map with no `.` key describes the root, so the
    /// walk yields no target for `./sub`. `find_dts_in_package` still probes
    /// the filesystem for `./sub` and still finds a file. Issue #1466 tracks
    /// whether `exports` should block a subpath at all.
    #[test]
    fn the_walk_reads_a_condition_map_as_the_root_entry() {
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

    /// `browser` is not in `CONDITION_ORDER`, so the walk never opens it and
    /// never sees the `default` inside it. The old name said the walk falls
    /// through to that `default`, which is the opposite of what it asserts.
    #[test]
    fn an_unapplied_condition_hides_the_default_nested_inside_it() {
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

    /// The pure walk only. A `null` target does not block the subpath:
    /// `find_dts_in_package` falls through to the `node10` probes and still
    /// finds a file. Issue #1466 tracks whether `exports` should block a
    /// subpath at all.
    #[test]
    fn the_walk_yields_no_target_for_a_null_entry() {
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

    /// tsgo disagrees with this test. Node and TypeScript both refuse an
    /// `exports` target that names a directory, and report the specifier as
    /// unresolved. This resolver accepts it. Issue #1466 owns that decision,
    /// so read this test as a record of today's behavior, not as a
    /// specification.
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
    ///
    /// Each `types` target names a file that the extension swap of its
    /// `default` sibling cannot reach, so the test fails if the resolver
    /// ignores the `types` condition.
    #[test]
    fn explicit_types_conditions_still_resolve() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "date-fns",
            r#"{ "name": "date-fns", "exports": {
                ".": {
                    "require": { "types": "./types/index.d.cts", "default": "./index.cjs" },
                    "import": { "types": "./types/index.d.ts", "default": "./index.js" }
                },
                "./addDays": {
                    "require": { "types": "./types/addDays.d.cts", "default": "./addDays.cjs" },
                    "import": { "types": "./types/addDays.d.ts", "default": "./addDays.js" }
                }
            } }"#,
            &[
                (
                    "types/index.d.ts",
                    "export declare function addDays(): Date;",
                ),
                ("types/index.d.cts", ""),
                (
                    "types/addDays.d.ts",
                    "export declare function addDays(): Date;",
                ),
                // Decoys. The extension swap of the `default` targets lands
                // here, so a resolver that skips the `types` condition picks
                // these instead.
                ("index.d.ts", "export declare const decoy: number;"),
                ("addDays.d.ts", "export declare const decoy: number;"),
            ],
        );

        let root_dts = find_package_dts(root, "date-fns").expect("should resolve root");
        assert_eq!(
            tail(&root_dts, root),
            "node_modules/date-fns/types/index.d.ts"
        );
        let subpath_dts = find_package_dts(root, "date-fns/addDays").expect("should resolve");
        assert_eq!(
            tail(&subpath_dts, root),
            "node_modules/date-fns/types/addDays.d.ts"
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

    // ── subpath directories that carry their own manifest ──────

    /// The shape of `firebase@8.10.1`. The package declares no `exports`,
    /// `firebase/app/package.json` says `"typings": "../index.d.ts"`, and
    /// `firebase/app/index.d.ts` does not exist. tsgo resolves
    /// `firebase/app` to `firebase/index.d.ts`.
    #[test]
    fn a_subpath_directory_manifest_points_the_resolver_back_up_the_package() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "firebase",
            r#"{ "name": "firebase", "typings": "index.d.ts" }"#,
            &[
                (
                    "index.d.ts",
                    "export declare function initializeApp(): void;",
                ),
                (
                    "app/package.json",
                    r#"{ "name": "firebase/app", "typings": "../index.d.ts" }"#,
                ),
                ("app/dist/index.cjs.js", ""),
            ],
        );

        let resolved = find_package_dts(root, "firebase/app").expect("should resolve subpath");
        assert_eq!(tail(&resolved, root), "node_modules/firebase/index.d.ts");
    }

    /// The shape of `date-fns@2.30.0`. `date-fns/addMonths/` holds both a
    /// `package.json` with `"typings": "../typings.d.ts"` and an
    /// `index.d.ts`. tsgo reads the manifest first and picks
    /// `date-fns/typings.d.ts`.
    #[test]
    fn a_subpath_directory_manifest_wins_over_the_index_file_beside_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "date-fns",
            r#"{ "name": "date-fns", "typings": "./typings.d.ts" }"#,
            &[
                ("typings.d.ts", "export declare function addMonths(): Date;"),
                (
                    "addMonths/package.json",
                    r#"{ "module": "../esm/addMonths/index.js", "typings": "../typings.d.ts" }"#,
                ),
                (
                    "addMonths/index.d.ts",
                    "export declare const decoy: number;",
                ),
            ],
        );

        let resolved = find_package_dts(root, "date-fns/addMonths").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/date-fns/typings.d.ts");
    }

    /// No manifest in the subpath directory, so the index file wins.
    #[test]
    fn a_subpath_directory_without_a_manifest_resolves_its_index_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "plainsub",
            r#"{ "name": "plainsub" }"#,
            &[("sub/index.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "plainsub/sub").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/plainsub/sub/index.d.ts"
        );
    }

    // ── field precedence ───────────────────────────────────────

    /// `exports` outranks a top-level `types`, which is what tsgo does. This
    /// changed the resolved file for `vue`, `tslib`, `entities` and others.
    #[test]
    fn an_exports_map_outranks_a_top_level_types_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "bothpkg",
            r#"{ "name": "bothpkg", "types": "./legacy.d.ts",
                 "exports": { ".": { "import": "./dist/index.js" } } }"#,
            &[
                ("legacy.d.ts", "export declare const legacy: number;"),
                ("dist/index.js", ""),
                ("dist/index.d.ts", "export declare const modern: number;"),
            ],
        );

        let resolved = find_package_dts(root, "bothpkg").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/bothpkg/dist/index.d.ts"
        );
    }

    /// A package that ships `typings` and no `types`.
    #[test]
    fn a_top_level_typings_field_still_resolves() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "typingspkg",
            r#"{ "name": "typingspkg", "typings": "./types/main.d.ts" }"#,
            &[("types/main.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "typingspkg").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/typingspkg/types/main.d.ts"
        );
    }

    // ── declaration extensions ─────────────────────────────────

    /// TypeScript maps `.mjs` onto `.d.mts`. Both declaration files sit in
    /// the package, so a resolver that always tries `.d.ts` first picks the
    /// CommonJS declarations.
    #[test]
    fn an_mjs_target_resolves_the_module_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "mjspkg",
            r#"{ "name": "mjspkg",
                 "exports": { ".": { "import": "./index.mjs", "require": "./index.cjs" } } }"#,
            &[
                ("index.mjs", ""),
                (
                    "index.d.mts",
                    "export declare function mfn(a: string): string;",
                ),
                (
                    "index.d.ts",
                    "export declare function mfn(a: number): number;",
                ),
            ],
        );

        let resolved = find_package_dts(root, "mjspkg").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/mjspkg/index.d.mts");
    }

    /// TypeScript maps `.cjs` onto `.d.cts`.
    #[test]
    fn a_cjs_target_resolves_the_commonjs_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "cjspkg",
            r#"{ "name": "cjspkg", "exports": { ".": { "require": "./index.cjs" } } }"#,
            &[
                ("index.cjs", ""),
                ("index.d.cts", "export declare const value: number;"),
                ("index.d.ts", "export declare const decoy: number;"),
            ],
        );

        let resolved = find_package_dts(root, "cjspkg").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/cjspkg/index.d.cts");
    }

    /// The mapped extension is a preference, not a requirement. A package
    /// that ships only `.d.ts` beside an `.mjs` target still resolves.
    #[test]
    fn an_mjs_target_falls_back_to_the_plain_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "mjsonly",
            r#"{ "name": "mjsonly", "exports": { ".": { "import": "./index.mjs" } } }"#,
            &[
                ("index.mjs", ""),
                ("index.d.ts", "export declare const value: number;"),
            ],
        );

        let resolved = find_package_dts(root, "mjsonly").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/mjsonly/index.d.ts");
    }

    // ── `*` pattern ranking ────────────────────────────────────

    /// Node's `PATTERN_KEY_COMPARE` breaks a prefix-length tie by taking the
    /// longer whole key, so `./a*.js` beats `./a*` for `./abc.js`.
    #[test]
    fn a_star_pattern_tie_breaks_on_the_longer_key() {
        let exports = json(r#"{ "./a*": "./short/*.js", "./a*.js": "./long/*.d.ts" }"#);
        assert_eq!(
            resolve_exports_target(&exports, "./abc.js"),
            Some(target("./long/bc.d.ts", false))
        );
    }

    #[test]
    fn a_star_pattern_tie_break_reaches_the_longer_keys_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "starpkg",
            r#"{ "name": "starpkg",
                 "exports": { "./a*": "./short/*.js", "./a*.js": "./long/*.d.ts" } }"#,
            &[
                ("short/bc.js.d.ts", "export declare const decoy: number;"),
                ("long/bc.d.ts", "export declare const value: number;"),
            ],
        );

        let resolved = find_package_dts(root, "starpkg/abc.js").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/starpkg/long/bc.d.ts");
    }

    // ── `node:` specifiers ─────────────────────────────────────

    #[test]
    fn a_node_scheme_specifier_resolves_the_submodule_declaration_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "@types/node",
            r#"{ "name": "@types/node", "types": "./index.d.ts" }"#,
            &[
                ("index.d.ts", "// index"),
                ("fs.d.ts", "declare module \"node:fs\" {}"),
            ],
        );

        let resolved = find_package_dts(root, "node:fs").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/@types/node/fs.d.ts");
    }

    #[test]
    fn a_node_scheme_specifier_falls_back_to_the_node_types_index() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "@types/node",
            r#"{ "name": "@types/node", "types": "./index.d.ts" }"#,
            &[("index.d.ts", "declare module \"node:test\" {}")],
        );

        let resolved = find_package_dts(root, "node:test").expect("should resolve");
        assert_eq!(tail(&resolved, root), "node_modules/@types/node/index.d.ts");
    }

    #[test]
    fn a_node_scheme_specifier_stays_unresolved_without_the_node_types() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(find_package_dts(dir.path(), "node:fs"), None);
    }

    // ── path escapes ───────────────────────────────────────────

    /// Write a declaration file outside every package directory. A
    /// `package.json` inside `node_modules` is untrusted input, so no target
    /// may reach this file.
    fn install_outsider(root: &Path) -> PathBuf {
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).expect("create outside dir");
        let secret = outside.join("secret.d.ts");
        std::fs::write(&secret, "export declare const leaked: number;").expect("write secret");

        secret
    }

    #[test]
    fn an_exports_target_that_climbs_out_of_the_package_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install_outsider(root);
        install(
            root,
            "escapepkg",
            r#"{ "name": "escapepkg", "exports": { ".": "../../outside/secret.d.ts" } }"#,
            &[],
        );

        assert_eq!(find_package_dts(root, "escapepkg"), None);
    }

    #[test]
    fn a_types_condition_that_climbs_out_of_the_package_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install_outsider(root);
        install(
            root,
            "escapetypes",
            r#"{ "name": "escapetypes",
                 "exports": { ".": { "types": "./../../outside/secret.d.ts" } } }"#,
            &[],
        );

        assert_eq!(find_package_dts(root, "escapetypes"), None);
    }

    #[test]
    fn an_absolute_exports_target_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let secret = install_outsider(root);
        let manifest = serde_json::json!({
            "name": "abspkg",
            "exports": { ".": secret.to_string_lossy() },
        });
        install(root, "abspkg", &manifest.to_string(), &[]);

        assert_eq!(find_package_dts(root, "abspkg"), None);
    }

    #[test]
    fn a_top_level_types_field_that_climbs_out_of_the_package_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install_outsider(root);
        install(
            root,
            "escapefield",
            r#"{ "name": "escapefield", "types": "../../outside/secret.d.ts" }"#,
            &[],
        );

        assert_eq!(find_package_dts(root, "escapefield"), None);
    }

    #[test]
    fn a_subpath_that_climbs_out_of_the_package_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install_outsider(root);
        install(root, "escapesub", r#"{ "name": "escapesub" }"#, &[]);

        assert_eq!(
            find_package_dts(root, "escapesub/../../outside/secret"),
            None
        );
    }

    // ── the specifier names the containment root ───────────────

    /// The package segment of a specifier picks the directory that every
    /// later containment check measures against. A segment of `..` moves
    /// that root up to the project, and then a `types` field inside the
    /// attacker's own package can climb out of `node_modules` and stay
    /// inside the moved root. A package supplies this string:
    /// `import("../node_modules/evil/app")` inside a `.d.ts` reaches
    /// `find_package_dts` verbatim.
    #[test]
    fn a_specifier_that_climbs_out_of_node_modules_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join(".env"), "AWS_SECRET_ACCESS_KEY=hunter2").expect("write .env");
        install(
            root,
            "evil",
            r#"{ "name": "evil" }"#,
            &[
                ("app/package.json", r#"{ "types": "../../../.env" }"#),
                (
                    "index.d.ts",
                    r#"export declare const x: import("../node_modules/evil/app").T;"#,
                ),
            ],
        );

        assert_eq!(find_package_dts(root, "../node_modules/evil/app"), None);
        // The same climb is already blocked when the specifier names the
        // package properly, because the root then stays on the package.
        assert_eq!(find_package_dts(root, "evil/app"), None);
    }

    #[test]
    fn a_specifier_without_a_valid_package_segment_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "real",
            r#"{ "name": "real" }"#,
            &[("index.d.ts", "export declare const value: number;")],
        );

        for specifier in [
            "..", ".", "../..", "", "/real", "./real", "../real", "@", "@scope",
        ] {
            assert_eq!(
                find_package_dts(root, specifier),
                None,
                "specifier `{specifier}` should stay unresolved"
            );
        }
        assert!(
            find_package_dts(root, "real").is_some(),
            "a valid package name should still resolve"
        );
    }

    #[test]
    fn a_scoped_subpath_specifier_still_resolves() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "@scope/name",
            r#"{ "name": "@scope/name" }"#,
            &[("sub/index.d.ts", "export declare const value: number;")],
        );

        let resolved = find_package_dts(root, "@scope/name/sub").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/@scope/name/sub/index.d.ts"
        );
    }

    // ── the manifest read is a probe like any other ────────────

    /// Run one resolution on a worker thread and fail when it does not
    /// answer. A `package.json` that never reaches end of file blocks
    /// `read_to_string` for ever, so the test must not wait for it.
    fn resolve_within(root: &Path, specifier: &str, limit: std::time::Duration) -> Option<PathBuf> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let root = root.to_path_buf();
        let specifier = specifier.to_string();
        std::thread::spawn(move || {
            let _ = sender.send(find_package_dts(&root, &specifier));
        });

        receiver
            .recv_timeout(limit)
            .expect("the resolver should answer inside the time limit")
    }

    /// A `package.json` that is a FIFO never reaches end of file, so an
    /// unguarded `read_to_string` hangs the compiler and the language
    /// server. The reported fixture symlinks `package.json` to `/dev/zero`;
    /// this test uses a FIFO instead, because `/dev/zero` grows the string
    /// without bound and a regression would end in an OOM kill rather than a
    /// failed assertion. Both are the same shape: a `package.json` that is
    /// not a regular file.
    #[cfg(unix)]
    #[test]
    fn a_package_json_that_never_ends_does_not_hang_the_resolver() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(
            root,
            "fifopkg",
            r#"{ "name": "fifopkg" }"#,
            &[("index.d.ts", "export declare const value: number;")],
        );
        let manifest = root
            .join("node_modules")
            .join("fifopkg")
            .join("package.json");
        std::fs::remove_file(&manifest).expect("remove the regular manifest");
        let made = std::process::Command::new("mkfifo")
            .arg(&manifest)
            .status()
            .expect("run mkfifo");
        assert!(made.success(), "mkfifo should create the fifo");

        let resolved = resolve_within(root, "fifopkg", std::time::Duration::from_secs(5));

        // The manifest is unreadable, so the resolver falls through to the
        // package root index.
        assert_eq!(
            tail(&resolved.expect("should resolve"), root),
            "node_modules/fifopkg/index.d.ts"
        );
    }

    /// The reported fixture, checked at the guard rather than through
    /// `find_package_dts`. Reading `/dev/zero` never ends and never stops
    /// allocating, so this test must not let the resolver open it.
    #[cfg(unix)]
    #[test]
    fn a_package_json_that_is_a_character_device_never_reaches_a_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        install(root, "devzero", r#"{ "name": "devzero" }"#, &[]);
        let pkg_dir = root.join("node_modules").join("devzero");
        let manifest = pkg_dir.join("package.json");
        std::fs::remove_file(&manifest).expect("remove the regular manifest");
        std::os::unix::fs::symlink("/dev/zero", &manifest).expect("link to /dev/zero");

        assert_eq!(super::probe_file(&pkg_dir, &pkg_dir, "package.json"), None);
    }

    /// A `package.json` symlinked at a file outside the package steers the
    /// result through its `types` field, so the manifest read is guarded the
    /// same way every other probe is.
    #[cfg(unix)]
    #[test]
    fn a_package_json_symlinked_out_of_the_package_stays_unread() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).expect("create outside dir");
        std::fs::write(
            outside.join("package.json"),
            r#"{ "types": "./steered.d.ts" }"#,
        )
        .expect("write the outside manifest");
        install(
            root,
            "linkedmanifest",
            r#"{ "name": "linkedmanifest" }"#,
            &[
                ("steered.d.ts", "export declare const steered: number;"),
                ("index.d.ts", "export declare const value: number;"),
            ],
        );
        let manifest = root
            .join("node_modules")
            .join("linkedmanifest")
            .join("package.json");
        std::fs::remove_file(&manifest).expect("remove the regular manifest");
        std::os::unix::fs::symlink(outside.join("package.json"), &manifest)
            .expect("link the manifest outside");

        let resolved = find_package_dts(root, "linkedmanifest").expect("should resolve");
        assert_eq!(
            tail(&resolved, root),
            "node_modules/linkedmanifest/index.d.ts"
        );
    }

    /// A symlink survives the lexical check, so the resolver compares the
    /// real paths as well.
    #[cfg(unix)]
    #[test]
    fn an_exports_target_that_leaves_the_package_through_a_symlink_stays_unresolved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let secret = install_outsider(root);
        install(
            root,
            "linkpkg",
            r#"{ "name": "linkpkg", "exports": { ".": "./linked.d.ts" } }"#,
            &[],
        );
        std::os::unix::fs::symlink(
            &secret,
            root.join("node_modules")
                .join("linkpkg")
                .join("linked.d.ts"),
        )
        .expect("create symlink");

        assert_eq!(find_package_dts(root, "linkpkg"), None);
    }
}

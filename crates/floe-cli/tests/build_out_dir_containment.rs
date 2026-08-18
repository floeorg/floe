//! End-to-end cover for the rule that `floe build` writes inside `--out-dir`.
//!
//! `out_dir.join(relative)` throws `out_dir` away when `relative` is
//! absolute, and the build used to hand it an absolute path whenever the
//! source sat outside the working directory. `floe build ../other/src/` then
//! wrote the emitted TypeScript beside the `.fl` source instead. That is how
//! the example apps grew 30 committed emitted files. See issue #1557.
//!
//! `--emit-stdout` is the exception, and the last two tests below pin it.
//! There the TypeScript on stdout is the output and the `.d.fl.ts` is a side
//! effect, so a declaration that cannot be placed is skipped rather than
//! fatal. The rule that matters, that nothing lands beside the source, holds
//! on every path.
//!
//! These tests run the real binary from a sibling directory and read the
//! source tree afterwards.

use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE: &str = "export type Todo = {\n    id: string,\n    done: boolean,\n}\n";

/// A temporary tree with an application beside an unrelated directory.
///
/// ```text
/// root/
///   app/src/todo.fl
///   other/
/// ```
struct Tree {
    root: tempfile::TempDir,
}

impl Tree {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("failed to create a temp directory");
        let src = root.path().join("app").join("src");
        std::fs::create_dir_all(&src).expect("failed to create the src directory");
        std::fs::write(src.join("todo.fl"), SOURCE).expect("failed to write the source file");
        std::fs::create_dir_all(root.path().join("other"))
            .expect("failed to create the sibling directory");

        Self { root }
    }

    fn app(&self) -> PathBuf {
        self.root.path().join("app")
    }

    fn other(&self) -> PathBuf {
        self.root.path().join("other")
    }
}

/// Run the real `floe` binary and return its status, stdout and stderr.
fn run_floe(current_dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_floe"))
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("failed to run floe");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(list_files(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();

    found
}

#[test]
fn a_build_from_a_sibling_directory_writes_nothing_into_the_source_tree() {
    let tree = Tree::new();
    let before = list_files(&tree.app());

    let (ok, _stdout, stderr) =
        run_floe(&tree.other(), &["build", "../app/src/", "--out-dir", "out"]);

    assert!(
        !ok,
        "a build that cannot place its output must fail, stderr: {stderr}"
    );
    assert_eq!(
        before,
        list_files(&tree.app()),
        "the build wrote into the source tree, stderr: {stderr}"
    );
}

#[test]
fn a_build_from_a_sibling_directory_names_the_source_and_the_fix() {
    let tree = Tree::new();

    let (_ok, _stdout, stderr) =
        run_floe(&tree.other(), &["build", "../app/src/", "--out-dir", "out"]);

    assert!(
        stderr.contains("todo.fl"),
        "the error must name the source file, got: {stderr}"
    );
    assert!(
        stderr.contains("outside the working directory"),
        "the error must say why it refused, got: {stderr}"
    );
    assert!(
        stderr.contains("Run `floe build` from a directory that contains the source."),
        "the error must name the fix, got: {stderr}"
    );
}

#[test]
fn an_emit_stdout_build_from_a_sibling_directory_writes_no_declarations_beside_the_source() {
    let tree = Tree::new();
    let before = list_files(&tree.app());

    let (ok, stdout, stderr) = run_floe(
        &tree.other(),
        &["build", "--emit-stdout", "../app/src/todo.fl"],
    );

    assert!(
        stdout.contains("Todo"),
        "the command must still print the compiled TypeScript, got: {stdout}"
    );
    // The caller asked for TypeScript on stdout and got it. The `.d.fl.ts`
    // is a side effect whose write already ignores its own failures, so a
    // declaration that cannot be placed skips and the run succeeds. The
    // Vite plugin compiles fixtures outside its working directory, and a
    // non-zero exit here breaks every one of them.
    assert!(
        ok,
        "an emit-stdout build must succeed when only the declaration cannot          be placed, stderr: {stderr}"
    );
    assert_eq!(
        before,
        list_files(&tree.app()),
        "the build wrote a declaration beside the source, stderr: {stderr}"
    );
}

#[test]
fn an_emit_stdout_build_from_a_sibling_directory_says_why_it_skipped_the_declaration() {
    let tree = Tree::new();

    let (_ok, _stdout, stderr) = run_floe(
        &tree.other(),
        &["build", "--emit-stdout", "../app/src/todo.fl"],
    );

    assert!(
        stderr.contains("todo.fl"),
        "the note must name the source file, got: {stderr}"
    );
    assert!(
        stderr.contains("no .d.fl.ts"),
        "the note must say what is missing, got: {stderr}"
    );
    assert!(
        stderr.contains("outside the working directory"),
        "the note must say why, got: {stderr}"
    );
}

#[test]
fn a_build_from_the_project_directory_writes_under_the_out_dir() {
    let tree = Tree::new();

    let (ok, stdout, stderr) = run_floe(&tree.app(), &["build", "src/", "--out-dir", "out"]);

    assert!(ok, "the build failed: {stderr}{stdout}");
    let written = list_files(&tree.app().join("out"));
    let names: Vec<String> = written
        .iter()
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(
        names.contains(&"todo.ts".to_string()),
        "the build must write the TypeScript under the out dir, found: {names:?}"
    );
    assert!(
        names.contains(&"todo.d.fl.ts".to_string()),
        "the build must write the declarations under the out dir, found: {names:?}"
    );
    assert!(
        !tree.app().join("src").join("todo.ts").exists(),
        "the build must not write beside the source"
    );
}

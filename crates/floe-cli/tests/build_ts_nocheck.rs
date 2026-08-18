//! End-to-end cover for the `// @ts-nocheck` header that `floe build` writes.
//!
//! The header switches TypeScript off over every emitted file, so a change
//! to the default silently stops all downstream type checking. These tests
//! run the real binary and read the file it wrote. See issue #1470.

use std::path::{Path, PathBuf};
use std::process::Command;

const HEADER: &str = "// @ts-nocheck";

const SOURCE: &str = "export type Todo = {\n    id: string,\n    done: boolean,\n}\n";

/// Run `floe build src/ --out-dir out` in a fresh directory and return the
/// text of the one emitted file.
fn build_and_read(extra_args: &[&str]) -> String {
    let project = tempfile::tempdir().expect("failed to create a temp directory");
    let src = project.path().join("src");
    std::fs::create_dir_all(&src).expect("failed to create the src directory");
    std::fs::write(src.join("todo.fl"), SOURCE).expect("failed to write the source file");

    let mut command = Command::new(env!("CARGO_BIN_EXE_floe"));
    command
        .current_dir(project.path())
        .arg("build")
        .arg("src/")
        .arg("--out-dir")
        .arg("out");
    for arg in extra_args {
        command.arg(arg);
    }
    let output = command.output().expect("failed to run floe build");
    assert!(
        output.status.success(),
        "floe build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let emitted = project.path().join("out").join("src").join("todo.ts");
    read_emitted(&emitted, project.path())
}

fn read_emitted(emitted: &Path, project: &Path) -> String {
    match std::fs::read_to_string(emitted) {
        Ok(text) => text,
        Err(error) => panic!(
            "failed to read {}: {error}. Emitted files: {:?}",
            emitted.display(),
            list_files(project)
        ),
    }
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

    found
}

#[test]
fn build_writes_the_ts_nocheck_header_by_default() {
    let emitted = build_and_read(&[]);
    assert!(
        emitted.starts_with(HEADER),
        "the default build must keep the header, got: {emitted}"
    );
}

#[test]
fn build_drops_the_ts_nocheck_header_with_the_flag() {
    let emitted = build_and_read(&["--no-ts-nocheck"]);
    assert!(
        !emitted.contains(HEADER),
        "--no-ts-nocheck must remove the header, got: {emitted}"
    );
    assert!(
        emitted.contains("Todo"),
        "the emitted file must still hold the compiled code, got: {emitted}"
    );
}

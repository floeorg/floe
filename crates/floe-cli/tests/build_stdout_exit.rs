//! End-to-end cover for the exit code of the two `floe build` paths that
//! print TypeScript to stdout.
//!
//! `@floeorg/core` runs `floe build --emit-stdout <file>` with
//! `execFileSync`, which throws on a non-zero exit only and drops stderr
//! when the exit is zero. The Vite and esbuild plugins hand whatever
//! comes back to the bundler. So a zero exit on a file codegen could not
//! emit serves `undefined` where a value belongs, and reports nothing.
//! These tests run the real binary and read its exit code. See issue
//! #1493.

use std::io::Write;
use std::process::{Command, Stdio};

/// `bogusName` resolves to nothing, so the checker reports E002 and marks
/// the call invalid. Codegen then has no TypeScript for it and reports
/// E059.
const BROKEN: &str = "export let main() -> number = { bogusName(1) }\n";

const SOUND: &str = "export let main() -> number = { 1 }\n";

/// Write `source` into a fresh directory and run
/// `floe build --emit-stdout` over the file.
fn build_emit_stdout(source: &str) -> std::process::Output {
    let project = tempfile::tempdir().expect("failed to create a temp directory");
    let file = project.path().join("main.fl");
    std::fs::write(&file, source).expect("failed to write the source file");

    Command::new(env!("CARGO_BIN_EXE_floe"))
        .current_dir(project.path())
        .arg("build")
        .arg("--emit-stdout")
        .arg("main.fl")
        .output()
        .expect("failed to run floe build --emit-stdout")
}

/// Pipe `source` into `floe build -` and return what the run produced.
fn build_stdin(source: &str) -> std::process::Output {
    let project = tempfile::tempdir().expect("failed to create a temp directory");

    let mut child = Command::new(env!("CARGO_BIN_EXE_floe"))
        .current_dir(project.path())
        .arg("build")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn floe build -");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(source.as_bytes())
        .expect("failed to write the source to stdin");

    child
        .wait_with_output()
        .expect("failed to run floe build -")
}

#[test]
fn build_emit_stdout_exits_non_zero_for_a_file_codegen_cannot_emit() {
    let output = build_emit_stdout(BROKEN);

    assert!(
        !output.status.success(),
        "the run must fail, because its caller reads the exit code and nothing else. stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E059"),
        "the run must say why it failed, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).is_empty(),
        "a file codegen cannot emit must reach no caller, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn build_emit_stdout_still_prints_a_sound_file_and_exits_zero() {
    let output = build_emit_stdout(SOUND);

    assert!(
        output.status.success(),
        "a sound file must still compile, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("function main"),
        "a sound file must still reach stdout, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn build_stdin_exits_non_zero_for_a_file_codegen_cannot_emit() {
    let output = build_stdin(BROKEN);

    assert!(
        !output.status.success(),
        "the stdin path carries the same contract as the file path. stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).is_empty(),
        "a file codegen cannot emit must reach no caller, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn build_stdin_still_prints_a_sound_file_and_exits_zero() {
    let output = build_stdin(SOUND);

    assert!(
        output.status.success(),
        "a sound file must still compile, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("function main"),
        "a sound file must still reach stdout, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

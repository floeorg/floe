//! The Floe name rule, measured against the pinned TypeScript compiler.
//!
//! Floe emits TypeScript, so a name Floe accepts and TypeScript refuses would
//! make the emitted file invalid. That direction is the dangerous one, and
//! this test fails on it.
//!
//! The two rules do not have to be equal. Floe reads the current Unicode
//! tables through `oxc_syntax`, and TypeScript ships its own tables, which
//! lag Unicode by a version or two. So Floe accepts some code points that
//! TypeScript has not caught up with yet. That direction is safe, because a
//! name a person writes today still passes both, and it closes by itself
//! when TypeScript updates. This test counts it and prints the count rather
//! than failing, so a person can watch the number shrink.
//!
//! Neither side holds a character range table. Floe's answer comes from
//! `floe_core::lexer`, and TypeScript's comes from TypeScript's own scanner
//! through `scripts/typescript-identifier-table.mjs`.
//!
//! Run `cargo test -p floe-core --test identifier_parity -- --nocapture` to
//! see the count. The test needs `node` and `pnpm install --frozen-lockfile`.

use std::path::{Path, PathBuf};
use std::process::Command;

use floe_core::lexer::{is_name_part, is_name_start};

const MAX_CODE_POINT: u32 = 0x0010_FFFF;
const BITSET_BYTES: usize = (MAX_CODE_POINT as usize + 8) >> 3;

/// One answer from the TypeScript scanner: may this code point start a name,
/// and may it continue one.
struct TypeScriptRule {
    version: String,
    start: Vec<u8>,
    part: Vec<u8>,
}

impl TypeScriptRule {
    fn starts(&self, code_point: u32) -> bool {
        Self::bit(&self.start, code_point)
    }

    fn continues(&self, code_point: u32) -> bool {
        Self::bit(&self.part, code_point)
    }

    fn bit(set: &[u8], code_point: u32) -> bool {
        let index = code_point as usize;
        set[index >> 3] & (1 << (index & 7)) != 0
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate sits two levels below the repository root")
}

/// Ask the pinned TypeScript compiler for its rule.
///
/// Answers `None` when the toolchain is not here, so `cargo test --workspace`
/// stays green before `pnpm install`. The CI job runs the script itself first,
/// so a missing toolchain fails there rather than skipping silently.
fn typescript_rule() -> Option<TypeScriptRule> {
    let root = repo_root();
    let script = root.join("scripts/typescript-identifier-table.mjs");
    if !root.join("node_modules/typescript").is_dir() {
        println!(
            "skipping: no node_modules/typescript. Run `pnpm install --frozen-lockfile` first."
        );
        return None;
    }

    let out_dir = tempfile::tempdir().expect("a temporary directory");
    let table = out_dir.path().join("identifier-table.bin");

    let output = Command::new("node")
        .arg(&script)
        .arg(&table)
        .output()
        .ok()?;
    assert!(
        output.status.success(),
        "{} failed: {}",
        script.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = std::fs::read(&table).expect("the script writes the table");
    assert_eq!(
        bytes.len(),
        BITSET_BYTES * 2,
        "the table must hold a start set and a part set"
    );
    let (start, part) = bytes.split_at(BITSET_BYTES);

    Some(TypeScriptRule {
        version: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        start: start.to_vec(),
        part: part.to_vec(),
    })
}

/// Every scalar value, which is every code point a source file can hold.
fn code_points() -> impl Iterator<Item = char> {
    (0..=MAX_CODE_POINT).filter_map(char::from_u32)
}

fn name_of(code_point: char) -> String {
    format!("U+{:04X}", code_point as u32)
}

/// Group a sorted list into contiguous runs, for a readable message.
fn ranges(code_points: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < code_points.len() {
        let mut end = index;
        while end + 1 < code_points.len()
            && code_points[end + 1] as u32 == code_points[end] as u32 + 1
        {
            end += 1;
        }
        if index == end {
            out.push(name_of(code_points[index]));
        } else {
            out.push(format!(
                "{}..{}",
                name_of(code_points[index]),
                name_of(code_points[end])
            ));
        }
        index = end + 1;
    }

    out
}

#[test]
fn floe_accepts_every_name_typescript_accepts() {
    let Some(typescript) = typescript_rule() else {
        return;
    };

    let mut start_gap: Vec<char> = Vec::new();
    let mut part_gap: Vec<char> = Vec::new();
    for code_point in code_points() {
        if typescript.starts(code_point as u32) && !is_name_start(code_point) {
            start_gap.push(code_point);
        }
        if typescript.continues(code_point as u32) && !is_name_part(code_point) {
            part_gap.push(code_point);
        }
    }

    assert!(
        start_gap.is_empty(),
        "TypeScript {} starts a name with {} code point(s) that Floe refuses, so \
         a file `tsc` accepts would not lex. Add them to the Floe rule in \
         `crates/floe-core/src/lexer.rs`. First: {}",
        typescript.version,
        start_gap.len(),
        ranges(&start_gap)
            .into_iter()
            .take(20)
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        part_gap.is_empty(),
        "TypeScript {} continues a name with {} code point(s) that Floe refuses, so \
         a file `tsc` accepts would not lex. Add them to the Floe rule in \
         `crates/floe-core/src/lexer.rs`. First: {}",
        typescript.version,
        part_gap.len(),
        ranges(&part_gap)
            .into_iter()
            .take(20)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[test]
fn the_gap_over_typescript_is_reported_not_enforced() {
    let Some(typescript) = typescript_rule() else {
        return;
    };

    let start_gap = code_points()
        .filter(|&c| is_name_start(c) && !typescript.starts(c as u32))
        .count();
    let part_gap = code_points()
        .filter(|&c| is_name_part(c) && !typescript.continues(c as u32))
        .count();

    println!("Floe name rule against TypeScript {}:", typescript.version);
    println!("  start: Floe accepts {start_gap} code point(s) TypeScript refuses");
    println!("  part:  Floe accepts {part_gap} code point(s) TypeScript refuses");
    println!(
        "These are scripts Unicode added after TypeScript's tables. A name in \
         one of them passes `floe check` and then fails `tsc`. The count falls \
         to zero on its own when TypeScript updates."
    );
}

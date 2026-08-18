#!/usr/bin/env node
//
// Ask the pinned TypeScript compiler which code points may start a name and
// which may continue one, then write the pair as two bitsets.
//
// The answer comes from TypeScript's own scanner. Nobody writes a character
// range table here, and nobody keeps one up to date: bump the `typescript`
// dependency and this file answers the new tables on the next run.
//
// `crates/floe-core/tests/identifier_parity.rs` reads the file this writes
// and compares it against the Floe rule in `floe_core::lexer`.
//
// Usage:
//   node scripts/typescript-identifier-table.mjs <out-file>
//
// The output is `2 * BITSET_BYTES` bytes: the start set, then the part set.
// Bit `cp & 7` of byte `cp >> 3` answers code point `cp`.

import fs from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ts = require("typescript");

const MAX_CODE_POINT = 0x10ffff;
const BITSET_BYTES = (MAX_CODE_POINT + 8) >> 3;

// A surrogate is not a scalar value, so no source file can hold one and Rust
// cannot name one with `char`. Leave both bits clear.
const FIRST_SURROGATE = 0xd800;
const LAST_SURROGATE = 0xdfff;

// Floe emits `"target": "ES2022"`, and TypeScript reads one table for every
// target at ES2015 or above. `ESNext` names that table.
const TARGET = ts.ScriptTarget.ESNext;

const outPath = process.argv[2];
if (!outPath) {
  console.error("usage: node scripts/typescript-identifier-table.mjs <out-file>");
  process.exit(2);
}

const start = new Uint8Array(BITSET_BYTES);
const part = new Uint8Array(BITSET_BYTES);

for (let cp = 0; cp <= MAX_CODE_POINT; cp++) {
  if (cp >= FIRST_SURROGATE && cp <= LAST_SURROGATE) {
    continue;
  }
  if (ts.isIdentifierStart(cp, TARGET)) {
    start[cp >> 3] |= 1 << (cp & 7);
  }
  if (ts.isIdentifierPart(cp, TARGET)) {
    part[cp >> 3] |= 1 << (cp & 7);
  }
}

fs.writeFileSync(outPath, Buffer.concat([Buffer.from(start), Buffer.from(part)]));
process.stdout.write(`${ts.version}\n`);

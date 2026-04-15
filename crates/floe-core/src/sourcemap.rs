//! Source map generation for Floe -> TypeScript compilation.
//!
//! Generates [Source Map v3](https://sourcemaps.info/spec.html) JSON that
//! maps positions in the emitted `.ts`/`.tsx` output back to the original
//! `.fl` source. This enables debugging in browser devtools with the
//! original Floe source.

use serde::Serialize;

/// A single mapping entry: one output position -> one source position.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    /// 0-based line in generated output
    pub gen_line: u32,
    /// 0-based column in generated output
    pub gen_col: u32,
    /// 0-based line in original source
    pub src_line: u32,
    /// 0-based column in original source
    pub src_col: u32,
}

/// Collects mappings and produces a Source Map v3 JSON string.
#[derive(Debug, Clone)]
pub struct SourceMapBuilder {
    source_file: String,
    mappings: Vec<Mapping>,
}

/// Source Map v3 JSON structure.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceMapV3 {
    version: u32,
    file: String,
    source_root: String,
    sources: Vec<String>,
    sources_content: Vec<String>,
    names: Vec<String>,
    mappings: String,
}

impl SourceMapBuilder {
    pub fn new(source_file: &str) -> Self {
        Self {
            source_file: source_file.to_string(),
            mappings: Vec::new(),
        }
    }

    /// Add a mapping from generated position to source position.
    ///
    /// Both line and column are 0-based.
    pub fn add_mapping(&mut self, mapping: Mapping) {
        self.mappings.push(mapping);
    }

    /// Add a mapping using 1-based line numbers (as from Span).
    /// Converts to 0-based internally.
    pub fn add_mapping_1based(&mut self, gen_line: u32, gen_col: u32, src_line: u32, src_col: u32) {
        self.add_mapping(Mapping {
            gen_line: gen_line.saturating_sub(1),
            gen_col: gen_col.saturating_sub(1),
            src_line: src_line.saturating_sub(1),
            src_col: src_col.saturating_sub(1),
        });
    }

    /// Generate the Source Map v3 JSON.
    pub fn build(&self, generated_file: &str, source_content: &str) -> String {
        let mappings_str = self.encode_mappings();

        let map = SourceMapV3 {
            version: 3,
            file: generated_file.to_string(),
            source_root: String::new(),
            sources: vec![self.source_file.clone()],
            sources_content: vec![source_content.to_string()],
            names: Vec::new(),
            mappings: mappings_str,
        };

        serde_json::to_string(&map).unwrap_or_default()
    }

    /// Generate pretty-printed Source Map v3 JSON (useful for debugging).
    pub fn build_pretty(&self, generated_file: &str, source_content: &str) -> String {
        let mappings_str = self.encode_mappings();

        let map = SourceMapV3 {
            version: 3,
            file: generated_file.to_string(),
            source_root: String::new(),
            sources: vec![self.source_file.clone()],
            sources_content: vec![source_content.to_string()],
            names: Vec::new(),
            mappings: mappings_str,
        };

        serde_json::to_string_pretty(&map).unwrap_or_default()
    }

    /// Encode all mappings into the VLQ-based mappings string.
    fn encode_mappings(&self) -> String {
        if self.mappings.is_empty() {
            return String::new();
        }

        let mut sorted = self.mappings.clone();
        sorted.sort_by(|a, b| a.gen_line.cmp(&b.gen_line).then(a.gen_col.cmp(&b.gen_col)));

        let mut result = String::new();
        let mut prev_gen_line: u32 = 0;
        let mut prev_gen_col: i64 = 0;
        let mut prev_src_line: i64 = 0;
        let mut prev_src_col: i64 = 0;
        let mut first_in_line = true;

        for mapping in &sorted {
            // Add semicolons for skipped lines
            while prev_gen_line < mapping.gen_line {
                result.push(';');
                prev_gen_line += 1;
                prev_gen_col = 0;
                first_in_line = true;
            }

            if !first_in_line {
                result.push(',');
            }
            first_in_line = false;

            // Field 1: generated column (relative to previous in same line)
            let gen_col_delta = mapping.gen_col as i64 - prev_gen_col;
            encode_vlq(&mut result, gen_col_delta);

            // Field 2: source index (always 0, delta from previous)
            encode_vlq(&mut result, 0);

            // Field 3: source line (relative to previous)
            let src_line_delta = mapping.src_line as i64 - prev_src_line;
            encode_vlq(&mut result, src_line_delta);

            // Field 4: source column (relative to previous)
            let src_col_delta = mapping.src_col as i64 - prev_src_col;
            encode_vlq(&mut result, src_col_delta);

            prev_gen_col = mapping.gen_col as i64;
            prev_src_line = mapping.src_line as i64;
            prev_src_col = mapping.src_col as i64;
        }

        result
    }
}

/// Base64 VLQ encoding as used in source maps.
const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode a signed integer as VLQ and append to the output string.
fn encode_vlq(out: &mut String, value: i64) {
    let mut vlq = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    };

    loop {
        let mut digit = (vlq & 0b11111) as u8;
        vlq >>= 5;
        if vlq > 0 {
            digit |= 0b100000; // continuation bit
        }
        out.push(BASE64_CHARS[digit as usize] as char);
        if vlq == 0 {
            break;
        }
    }
}

/// Decode a VLQ value from a source map mappings string (for testing).
#[cfg(test)]
fn decode_vlq(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<i64> {
    let mut value: i64 = 0;
    let mut shift: u32 = 0;

    loop {
        let ch = chars.next()?;
        let digit = BASE64_CHARS.iter().position(|&c| c == ch as u8)? as i64;
        value |= (digit & 0b11111) << shift;
        shift += 5;
        if digit & 0b100000 == 0 {
            break;
        }
    }

    if value & 1 == 1 {
        Some(-(value >> 1))
    } else {
        Some(value >> 1)
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_encode_zero() {
        let mut out = String::new();
        encode_vlq(&mut out, 0);
        assert_eq!(out, "A");
    }

    #[test]
    fn vlq_encode_positive() {
        let mut out = String::new();
        encode_vlq(&mut out, 1);
        assert_eq!(out, "C");
    }

    #[test]
    fn vlq_encode_negative() {
        let mut out = String::new();
        encode_vlq(&mut out, -1);
        assert_eq!(out, "D");
    }

    #[test]
    fn vlq_encode_large() {
        let mut out = String::new();
        encode_vlq(&mut out, 16);
        // 16 << 1 = 32 = 0b100000, needs continuation
        let mut chars = out.chars().peekable();
        let decoded = decode_vlq(&mut chars).unwrap();
        assert_eq!(decoded, 16);
    }

    #[test]
    fn vlq_roundtrip() {
        for value in [-100, -10, -1, 0, 1, 10, 100, 1000] {
            let mut encoded = String::new();
            encode_vlq(&mut encoded, value);
            let mut chars = encoded.chars().peekable();
            let decoded = decode_vlq(&mut chars).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for {value}");
        }
    }

    #[test]
    fn empty_mappings() {
        let builder = SourceMapBuilder::new("test.fl");
        let json = builder.build("test.ts", "");
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"mappings\":\"\""));
    }

    #[test]
    fn single_mapping() {
        let mut builder = SourceMapBuilder::new("test.fl");
        builder.add_mapping(Mapping {
            gen_line: 0,
            gen_col: 0,
            src_line: 0,
            src_col: 0,
        });
        let json = builder.build("test.ts", "const x = 1");
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"sources\":[\"test.fl\"]"));
        // AAAA = gen_col:0, source:0, src_line:0, src_col:0
        assert!(json.contains("\"mappings\":\"AAAA\""));
    }

    #[test]
    fn multiple_lines() {
        let mut builder = SourceMapBuilder::new("test.fl");
        builder.add_mapping(Mapping {
            gen_line: 0,
            gen_col: 0,
            src_line: 0,
            src_col: 0,
        });
        builder.add_mapping(Mapping {
            gen_line: 1,
            gen_col: 0,
            src_line: 1,
            src_col: 0,
        });
        let json = builder.build("test.ts", "line1\nline2");
        // Two lines separated by semicolon
        assert!(json.contains("\"mappings\":\"AAAA;AACA\""));
    }

    #[test]
    fn mapping_with_offset() {
        let mut builder = SourceMapBuilder::new("test.fl");
        // Source line 5, col 3 -> generated line 2, col 4
        builder.add_mapping(Mapping {
            gen_line: 2,
            gen_col: 4,
            src_line: 5,
            src_col: 3,
        });
        let json = builder.build("test.ts", "source");
        // Should have two leading semicolons for skipped lines 0,1
        assert!(json.contains("\"mappings\":\";;IAKG\""));
    }

    #[test]
    fn multiple_segments_same_line() {
        let mut builder = SourceMapBuilder::new("test.fl");
        builder.add_mapping(Mapping {
            gen_line: 0,
            gen_col: 0,
            src_line: 0,
            src_col: 0,
        });
        builder.add_mapping(Mapping {
            gen_line: 0,
            gen_col: 6,
            src_line: 0,
            src_col: 6,
        });
        let json = builder.build("test.ts", "const x = 1");
        // Two segments on same line, separated by comma
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mappings = parsed["mappings"].as_str().unwrap();
        assert!(mappings.contains(','));
    }

    #[test]
    fn add_mapping_1based_converts() {
        let mut builder = SourceMapBuilder::new("test.fl");
        builder.add_mapping_1based(1, 1, 1, 1);
        assert_eq!(builder.mappings.len(), 1);
        assert_eq!(builder.mappings[0].gen_line, 0);
        assert_eq!(builder.mappings[0].gen_col, 0);
        assert_eq!(builder.mappings[0].src_line, 0);
        assert_eq!(builder.mappings[0].src_col, 0);
    }

    #[test]
    fn source_content_included() {
        let builder = SourceMapBuilder::new("test.fl");
        let source = "const x = 42\nconst y = 10";
        let json = builder.build("test.ts", source);
        assert!(json.contains("const x = 42"));
        assert!(json.contains("sourcesContent"));
    }

    #[test]
    fn build_pretty_is_formatted() {
        let builder = SourceMapBuilder::new("test.fl");
        let json = builder.build_pretty("test.ts", "");
        assert!(json.contains('\n'));
        assert!(json.contains("  "));
    }

    #[test]
    fn valid_json_output() {
        let mut builder = SourceMapBuilder::new("hello.fl");
        builder.add_mapping(Mapping {
            gen_line: 0,
            gen_col: 0,
            src_line: 0,
            src_col: 0,
        });
        builder.add_mapping(Mapping {
            gen_line: 1,
            gen_col: 2,
            src_line: 3,
            src_col: 4,
        });
        let json = builder.build("hello.ts", "source code here");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], 3);
        assert_eq!(parsed["file"], "hello.ts");
        assert_eq!(parsed["sources"][0], "hello.fl");
    }
}

//! Extract Floe code samples from `.astro` source files.
//!
//! Astro pages embed code as JavaScript template literals — either inside
//! `<Code code={`...`} lang={floeLang} />` Astro components or as standalone
//! variables. Neither shape is a Markdown fence, so the markdown extractor
//! never sees them.
//!
//! Why a marker instead of auto-detecting `lang={floeLang}`: parsing JSX
//! attributes and template literals robustly without a real parser is
//! fiddly, and Floe samples can themselves contain backticks (tagged
//! templates from npm interop) which would confuse a regex. The marker is
//! one extra line per block and zero ambiguity.
//!
//! Unescaped `${...}` interpolation is replaced with a placeholder
//! identifier — real interpolation in a Floe sample would be an author
//! mistake, and a placeholder keeps the surrounding code parseable so the
//! rest of the block still gets checked.

use std::path::Path;

use floe_core::line_numbers::LineNumbers;

use crate::extract::CodeBlock;

const MARKER: &str = "@floe-check";

pub fn extract_astro_blocks(source: &str, path: &Path) -> Vec<CodeBlock> {
    let line_numbers = LineNumbers::new(source);
    let mut blocks = Vec::new();
    let mut search_from = 0;

    while let Some(rel) = source[search_from..].find(MARKER) {
        let marker_pos = search_from + rel;
        let after_marker = marker_pos + MARKER.len();

        let extracted = source[after_marker..]
            .find('`')
            .map(|off| after_marker + off)
            .and_then(|tick_pos| {
                decode_template_literal(source, tick_pos)
                    .map(|(code, end_pos)| (tick_pos, code, end_pos))
            });

        match extracted {
            Some((tick_pos, code, end_pos)) => {
                let start_line = line_numbers.line_number((tick_pos + 1) as u32) as usize;
                blocks.push(CodeBlock {
                    path: path.to_path_buf(),
                    code,
                    start_line,
                    info: "floe".to_string(),
                });
                search_from = end_pos;
            }
            None => {
                search_from = after_marker;
            }
        }
    }

    blocks
}

/// Walk a JavaScript template literal whose opening backtick is at
/// `source.as_bytes()[open]`. Returns the decoded inner text and the byte
/// index just past the closing backtick.
fn decode_template_literal(source: &str, open: usize) -> Option<(String, usize)> {
    debug_assert_eq!(source.as_bytes()[open], b'`');
    let mut out = String::new();
    let mut iter = source[open + 1..].char_indices().peekable();

    while let Some((rel, c)) = iter.next() {
        match c {
            '\\' => match iter.next() {
                Some((_, '`')) => out.push('`'),
                Some((_, '$')) => out.push('$'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, other)) => {
                    // Unknown escape — keep verbatim so we don't silently
                    // change the author's intent.
                    out.push('\\');
                    out.push(other);
                }
                None => return None,
            },
            '$' if iter.peek().map(|(_, c)| *c) == Some('{') => {
                iter.next();
                let mut depth = 1usize;
                for (_, c) in iter.by_ref() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                out.push_str("__interp__");
            }
            '`' => {
                let close_byte = open + 1 + rel;
                return Some((out, close_byte + 1));
            }
            other => out.push(other),
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> Vec<CodeBlock> {
        extract_astro_blocks(src, Path::new("test.astro"))
    }

    #[test]
    fn extracts_jsx_marker_block() {
        let src = "\
<div>
  {/* @floe-check */}
  <Code
    code={`let x = 1`}
    lang={floeLang}
  />
</div>
";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = 1");
    }

    #[test]
    fn extracts_line_comment_marker_block() {
        let src = "\
// @floe-check
let defaultCode = `let greet(name: string) -> string = { name }`;
";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].code,
            "let greet(name: string) -> string = { name }"
        );
    }

    #[test]
    fn unescapes_template_escapes() {
        let src = "\
// @floe-check
let s = `a \\` b \\$ c \\\\ d`;
";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "a ` b $ c \\ d");
    }

    #[test]
    fn replaces_interpolation_with_placeholder() {
        let src = "\
// @floe-check
let s = `let x = ${1 + 2}`;
";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = __interp__");
    }

    #[test]
    fn skips_blocks_without_marker() {
        let src = "\
let other = `let x = 1`;
<Code code={`let y = 2`} lang={floeLang} />
";
        let blocks = extract(src);
        assert!(blocks.is_empty());
    }

    #[test]
    fn extracts_multiple_blocks() {
        let src = "\
{/* @floe-check */}
<Code code={`let x = 1`} lang={floeLang} />
prose
// @floe-check
let s = `let y = 2`;
";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].code, "let x = 1");
        assert_eq!(blocks[1].code, "let y = 2");
    }

    #[test]
    fn line_number_points_at_first_code_line() {
        let src = "prose\n\
                   {/* @floe-check */}\n\
                   <Code\n\
                     code={`first\n\
                   second`}\n\
                     lang={floeLang}\n\
                   />\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_line, 4);
    }

    #[test]
    fn marker_without_template_is_ignored() {
        let src = "// @floe-check\nlet x = 1;\n";
        let blocks = extract(src);
        assert!(blocks.is_empty());
    }

    #[test]
    fn unterminated_template_is_ignored() {
        let src = "// @floe-check\nlet x = `unterminated\n";
        let blocks = extract(src);
        assert!(blocks.is_empty());
    }

    #[test]
    fn preserves_multibyte_utf8() {
        let src = "// @floe-check\nlet s = `let greeting = \"こんにちは\"`;\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let greeting = \"こんにちは\"");
    }
}

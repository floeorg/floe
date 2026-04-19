//! Extract ```floe fenced code blocks from Markdown.
//!
//! Follows the CommonMark fenced-code-block rules closely enough for our docs:
//!
//! * Opening fence is 3+ backticks, optionally preceded by up to 3 spaces of
//!   indentation. The info string (the text after the backticks) must start
//!   with the word `floe` followed by end-of-line, whitespace, or a comma.
//!   This accepts `floe`, `floe title="x"`, `floe,ignore`, etc.
//! * Closing fence is the same character with at least as many backticks as
//!   the opening fence, optionally preceded by up to 3 spaces of indentation,
//!   and nothing else on the line.
//! * The leading indentation of the opening fence is stripped from every body
//!   line so blocks nested in list items parse correctly.
//!
//! We deliberately ignore tildes — they're valid CommonMark but nothing in
//! the Floe docs uses them.

use std::path::{Path, PathBuf};

/// A single extracted ```floe block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub path: PathBuf,
    pub code: String,
    /// 1-based line number of the first code line (the line after the opening fence).
    pub start_line: usize,
    /// The info string after the opening backticks, e.g. `floe`, `floe,ignore`.
    pub info: String,
}

impl CodeBlock {
    /// `true` if the info string opts out of checking (e.g. `floe,ignore`).
    ///
    /// Used for intentionally-pseudo-code samples that should still be
    /// highlighted as Floe but aren't valid standalone programs.
    pub fn is_ignored(&self) -> bool {
        self.info
            .split(',')
            .skip(1)
            .map(str::trim)
            .any(|t| t.eq_ignore_ascii_case("ignore") || t.eq_ignore_ascii_case("skip"))
    }
}

/// Extract every ```floe block from `source`.
pub fn extract_blocks(source: &str, path: &Path) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = source.split_inclusive('\n').collect();

    let mut i = 0;
    while i < lines.len() {
        if let Some(fence) = parse_opening_fence(lines[i]) {
            let body_start = i + 1;
            let body_end = (body_start..lines.len())
                .find(|&j| is_closing_fence(lines[j], &fence))
                .unwrap_or(lines.len());

            if is_floe_info(&fence.info) {
                let code = lines[body_start..body_end]
                    .iter()
                    .map(|l| strip_indent(l, fence.indent))
                    .collect::<String>();
                blocks.push(CodeBlock {
                    path: path.to_path_buf(),
                    code,
                    start_line: body_start + 1,
                    info: fence.info,
                });
            }

            i = body_end + 1;
            continue;
        }
        i += 1;
    }

    blocks
}

struct Fence {
    indent: usize,
    length: usize,
    info: String,
}

/// Scan a line for the common fence shape: up to 3 spaces of indent, 3+
/// backticks, and whatever follows. Returns `None` for lines that can't be
/// any kind of fence.
fn scan_fence(line: &str) -> Option<(usize, usize, &str)> {
    let (indent, rest) = leading_indent(line);
    if indent > 3 {
        return None;
    }
    let ticks = rest.chars().take_while(|c| *c == '`').count();
    if ticks < 3 {
        return None;
    }
    Some((indent, ticks, &rest[ticks..]))
}

fn parse_opening_fence(line: &str) -> Option<Fence> {
    let (indent, ticks, tail) = scan_fence(line)?;
    let info = tail.trim_end_matches(['\n', '\r']).trim().to_string();
    // CommonMark forbids backticks in the info string of a backtick fence.
    if info.contains('`') {
        return None;
    }
    Some(Fence {
        indent,
        length: ticks,
        info,
    })
}

fn is_closing_fence(line: &str, opening: &Fence) -> bool {
    let Some((_, ticks, tail)) = scan_fence(line) else {
        return false;
    };
    ticks >= opening.length && tail.trim_end_matches(['\n', '\r']).trim().is_empty()
}

fn leading_indent(line: &str) -> (usize, &str) {
    let count = line.chars().take_while(|c| *c == ' ').count();
    (count, &line[count..])
}

fn strip_indent(line: &str, indent: usize) -> &str {
    let (n, rest) = leading_indent(line);
    if n >= indent {
        // The line is indented at least as far as the fence; trim exactly the
        // fence indent and keep any extra indentation as part of the code.
        &line[indent..]
    } else {
        // Less indentation than the fence (blank lines, mostly) — emit as-is.
        rest
    }
}

fn is_floe_info(info: &str) -> bool {
    let end = info
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(info.len());
    info[..end].eq_ignore_ascii_case("floe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> Vec<CodeBlock> {
        extract_blocks(src, Path::new("test.md"))
    }

    #[test]
    fn extracts_single_floe_block() {
        let src = "intro\n```floe\nlet x = 1\n```\noutro\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = 1\n");
        assert_eq!(blocks[0].start_line, 3);
        assert_eq!(blocks[0].info, "floe");
    }

    #[test]
    fn skips_non_floe_blocks() {
        let src = "```ts\nconst x = 1\n```\n```floe\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = 1\n");
    }

    #[test]
    fn accepts_floe_with_trailing_info() {
        let src = "```floe title=\"demo.fl\"\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].info, "floe title=\"demo.fl\"");
    }

    #[test]
    fn accepts_floe_comma_tag() {
        let src = "```floe,ignore\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn is_ignored_detects_ignore_tag() {
        let src = "```floe,ignore\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert!(blocks[0].is_ignored());
    }

    #[test]
    fn is_ignored_false_for_plain_floe() {
        let src = "```floe\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert!(!blocks[0].is_ignored());
    }

    #[test]
    fn extracts_multiple_blocks_with_correct_line_numbers() {
        // Line 1: prose
        // Line 2: ```floe     <- opening fence
        // Line 3: body A line 1
        // Line 4: ```          <- closing
        // Line 5: gap
        // Line 6: ```floe     <- opening fence
        // Line 7: body B line 1
        // Line 8: body B line 2
        // Line 9: ```          <- closing
        let src = "prose\n\
                   ```floe\n\
                   body A\n\
                   ```\n\
                   gap\n\
                   ```floe\n\
                   body B line 1\n\
                   body B line 2\n\
                   ```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].start_line, 3);
        assert_eq!(blocks[1].start_line, 7);
    }

    #[test]
    fn strips_indent_for_nested_list_blocks() {
        let src = "- item\n   ```floe\n   let x = 1\n   ```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = 1\n");
    }

    #[test]
    fn indent_of_four_or_more_is_not_a_fence() {
        let src = "    ```floe\n    let x = 1\n    ```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn unterminated_block_runs_to_eof() {
        let src = "```floe\nlet x = 1\nlet y = 2\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].code, "let x = 1\nlet y = 2\n");
    }

    #[test]
    fn closing_fence_can_be_longer_than_opening() {
        let src = "```floe\nlet x = 1\n`````\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn rejects_info_containing_backtick() {
        let src = "```floe `\nlet x = 1\n```\n";
        let blocks = extract(src);
        assert_eq!(blocks.len(), 0);
    }
}

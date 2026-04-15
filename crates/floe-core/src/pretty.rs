//! A Document combinator pretty-printer based on Lindig's "Strictly Pretty" (2000).
//!
//! Build a `Document` tree out of strings, breaks, groups, and nesting, then
//! call `pretty_print(width)` (or `pretty_print_to(width, writer)` to stream
//! into a buffer) to render it. Groups try to fit on one line; if they cannot,
//! every `Break` inside the group renders with its broken text instead of its
//! unbroken text.
//!
//! The formatter (#1116) and eventually the codegen refactor (#1119) sit on
//! top of this so they get correct-by-construction indentation and wrapping
//! without hand-rolled string math.

use std::fmt;

#[derive(Debug, Clone)]
pub enum Document {
    /// Verbatim text. Never wrapped. `width` is the grapheme-ish column count
    /// (currently `chars().count()`) computed once at construction so the
    /// fits/render hot path never re-scans the string.
    Str { value: String, width: u32 },

    /// A forced line break. Always renders as `\n` followed by the current
    /// indentation, regardless of mode.
    Line,

    /// A break point. Renders as `unbroken` when the enclosing `Group` fits
    /// on one line; as `broken` followed by a newline+indent otherwise.
    ///
    /// `broken_width` / `unbroken_width` are precomputed so fits/render don't
    /// re-scan the strings on every call.
    Break {
        broken: String,
        unbroken: String,
        broken_width: u32,
        unbroken_width: u32,
    },

    /// Sequence of documents concatenated with no separator.
    Vec(Vec<Document>),

    /// Wrap the inner document with an indentation delta that applies at
    /// every line break encountered inside it.
    Nest { amount: isize, doc: Box<Document> },

    /// Attempt to render the inner document on one line. If it does not fit
    /// in the remaining width, render with all inner `Break`s expanded.
    Group(Box<Document>),

    /// Force the enclosing `Group` to render as broken even if its content
    /// fits on one line.
    ForceBroken(Box<Document>),
}

impl Document {
    /// Render the document to a `String`, wrapping at `limit` columns.
    pub fn pretty_print(&self, limit: usize) -> String {
        // A byte-level heuristic: output is usually within a few x of the
        // width budget for small docs, and large docs dominated by text.
        // Over-allocating is cheap; under-allocating costs real realloc+memcpy.
        let mut out = String::with_capacity(limit.saturating_mul(2).max(64));
        self.pretty_print_to(limit, &mut out)
            .expect("String as fmt::Write never fails");
        out
    }

    /// Render the document into any `fmt::Write` sink, wrapping at `limit`
    /// columns. Lets codegen stream directly into a shared output buffer
    /// without a second allocation.
    pub fn pretty_print_to(&self, limit: usize, out: &mut impl fmt::Write) -> fmt::Result {
        render(out, limit as isize, self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Broken,
    Unbroken,
}

/// A chunk of spaces used by `write_indent` for cheap indentation writes.
/// Sized to cover typical compiler output depths with a single `write_str`.
const SPACES: &str = "                                                                "; // 64

fn write_indent(out: &mut impl fmt::Write, indent: isize) -> fmt::Result {
    let mut remaining = indent.max(0) as usize;
    while remaining > SPACES.len() {
        out.write_str(SPACES)?;
        remaining -= SPACES.len();
    }
    out.write_str(&SPACES[..remaining])
}

/// Decide whether the work queue fits in `limit - current` columns on the
/// current line. Short-circuits on committed newlines (`Line`, broken
/// `Break`) and on `ForceBroken`.
fn fits(limit: isize, mut current: isize, initial: Vec<(isize, Mode, &Document)>) -> bool {
    let mut stack = initial;

    while let Some((indent, mode, doc)) = stack.pop() {
        if current > limit {
            return false;
        }
        match doc {
            Document::Str { width, .. } => current += *width as isize,

            Document::Line => return true,

            Document::Break { unbroken_width, .. } => match mode {
                Mode::Broken => return true,
                Mode::Unbroken => current += *unbroken_width as isize,
            },

            Document::Vec(children) => {
                for child in children.iter().rev() {
                    stack.push((indent, mode, child));
                }
            }

            Document::Nest { amount, doc: inner } => {
                stack.push((indent + amount, mode, inner));
            }

            Document::Group(inner) => {
                stack.push((indent, Mode::Unbroken, inner));
            }

            Document::ForceBroken(_) => return false,
        }
    }
    current <= limit
}

fn render(out: &mut impl fmt::Write, limit: isize, top: &Document) -> fmt::Result {
    let mut stack: Vec<(isize, Mode, &Document)> = vec![(0, Mode::Unbroken, top)];
    let mut width: isize = 0;

    while let Some((indent, mode, doc)) = stack.pop() {
        match doc {
            Document::Str { value, width: w } => {
                out.write_str(value)?;
                width += *w as isize;
            }

            Document::Line => {
                out.write_char('\n')?;
                write_indent(out, indent)?;
                width = indent.max(0);
            }

            Document::Break {
                broken,
                unbroken,
                unbroken_width,
                ..
            } => match mode {
                Mode::Broken => {
                    out.write_str(broken)?;
                    out.write_char('\n')?;
                    write_indent(out, indent)?;
                    width = indent.max(0);
                }
                Mode::Unbroken => {
                    out.write_str(unbroken)?;
                    width += *unbroken_width as isize;
                }
            },

            Document::Vec(children) => {
                for child in children.iter().rev() {
                    stack.push((indent, mode, child));
                }
            }

            Document::Nest { amount, doc: inner } => {
                stack.push((indent + amount, mode, inner));
            }

            Document::Group(inner) => {
                let group_mode = if fits(limit, width, vec![(indent, Mode::Unbroken, inner)]) {
                    Mode::Unbroken
                } else {
                    Mode::Broken
                };
                stack.push((indent, group_mode, inner));
            }

            Document::ForceBroken(inner) => {
                // Outside a Group this is a structural no-op: the parent's
                // fits() never runs so nothing checks us. Inside a Group,
                // fits() has already short-circuited to false, putting the
                // parent into Broken mode, so we just pass the mode through.
                stack.push((indent, mode, inner));
            }
        }
    }
    Ok(())
}

// -- Combinators ------------------------------------------------------------

/// Empty document. Renders as nothing.
pub fn nil() -> Document {
    Document::Vec(Vec::new())
}

/// Verbatim text. Width is precomputed at construction.
pub fn str(s: impl Into<String>) -> Document {
    let value = s.into();
    let width = value.chars().count() as u32;
    Document::Str { value, width }
}

/// A forced line break (always renders as a newline + indent).
pub fn line() -> Document {
    Document::Line
}

/// A conditional break: renders `unbroken` text if the enclosing Group fits,
/// `broken` + newline + indent otherwise. Both widths are precomputed.
pub fn break_(broken: impl Into<String>, unbroken: impl Into<String>) -> Document {
    let broken = broken.into();
    let unbroken = unbroken.into();
    let broken_width = broken.chars().count() as u32;
    let unbroken_width = unbroken.chars().count() as u32;
    Document::Break {
        broken,
        unbroken,
        broken_width,
        unbroken_width,
    }
}

/// A soft space: single space when inline, newline+indent when broken.
pub fn soft_space() -> Document {
    break_("", " ")
}

/// Concatenate a sequence of documents with no separator.
pub fn concat(docs: impl IntoIterator<Item = Document>) -> Document {
    Document::Vec(docs.into_iter().collect())
}

/// Group the inner document: try to fit on one line, break all `Break`s if not.
pub fn group(doc: Document) -> Document {
    Document::Group(Box::new(doc))
}

/// Indent the inner document by `amount` columns at every line break.
/// Negative amounts are permitted (dedent); the effective indent floor is 0.
pub fn nest(amount: isize, doc: Document) -> Document {
    Document::Nest {
        amount,
        doc: Box::new(doc),
    }
}

/// Force the enclosing Group to render as broken.
pub fn force_broken(doc: Document) -> Document {
    Document::ForceBroken(Box::new(doc))
}

// -- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_renders_verbatim() {
        assert_eq!(str("hello").pretty_print(80), "hello");
    }

    #[test]
    fn empty_nil_renders_nothing() {
        assert_eq!(nil().pretty_print(80), "");
    }

    #[test]
    fn concat_joins_without_separator() {
        let doc = concat([str("abc"), str("def")]);
        assert_eq!(doc.pretty_print(80), "abcdef");
    }

    #[test]
    fn line_always_newlines() {
        let doc = concat([str("a"), line(), str("b")]);
        assert_eq!(doc.pretty_print(80), "a\nb");
    }

    #[test]
    fn break_is_unbroken_when_group_fits() {
        let doc = group(concat([str("a"), soft_space(), str("b")]));
        assert_eq!(doc.pretty_print(80), "a b");
    }

    #[test]
    fn break_is_broken_when_group_does_not_fit() {
        let doc = group(concat([str("aaaa"), soft_space(), str("bbbb")]));
        assert_eq!(doc.pretty_print(5), "aaaa\nbbbb");
    }

    #[test]
    fn nest_indents_at_inner_line_breaks() {
        let doc = group(concat([
            str("{"),
            nest(2, concat([line(), str("body")])),
            line(),
            str("}"),
        ]));
        assert_eq!(doc.pretty_print(80), "{\n  body\n}");
    }

    #[test]
    fn nested_group_can_fit_while_outer_breaks() {
        let doc = group(concat([
            str("outer"),
            soft_space(),
            group(concat([str("a"), soft_space(), str("b")])),
        ]));
        assert_eq!(doc.pretty_print(7), "outer\na b");
    }

    #[test]
    fn force_broken_prevents_inline_layout() {
        let doc = group(force_broken(concat([str("a"), soft_space(), str("b")])));
        assert_eq!(doc.pretty_print(80), "a\nb");
    }

    #[test]
    fn trailing_comma_break_renders_correctly() {
        let items = concat([
            str("a"),
            break_(",", ", "),
            str("b"),
            break_(",", ", "),
            str("c"),
        ]);
        let doc = group(concat([
            str("{"),
            nest(2, concat([break_("", " "), items])),
            break_("", " "),
            str("}"),
        ]));
        assert_eq!(doc.pretty_print(80), "{ a, b, c }");
        assert_eq!(doc.pretty_print(10), "{\n  a,\n  b,\n  c\n}");
    }

    #[test]
    fn break_unbroken_text_counts_toward_width() {
        let doc = group(concat([str("a"), soft_space(), str("b")]));
        assert_eq!(doc.pretty_print(3), "a b");
        assert_eq!(doc.pretty_print(2), "a\nb");
    }

    #[test]
    fn multibyte_width_counts_chars_not_bytes() {
        let doc = group(concat([str("é"), soft_space(), str("é")]));
        assert_eq!(doc.pretty_print(3), "é é");
    }

    #[test]
    fn nested_nest_accumulates_indent() {
        let doc = nest(2, nest(2, concat([line(), str("x")])));
        assert_eq!(doc.pretty_print(80), "\n    x");
    }

    #[test]
    fn negative_nest_clamps_indent_floor_to_zero() {
        // Dedent past the start should not underflow.
        let doc = nest(-4, concat([line(), str("x")]));
        assert_eq!(doc.pretty_print(80), "\nx");
    }

    #[test]
    fn empty_group_renders_nothing() {
        assert_eq!(group(nil()).pretty_print(0), "");
    }

    #[test]
    fn pretty_print_to_streams_into_fmt_write() {
        // Exercise the fmt::Write entry point used by codegen for streaming.
        let doc = group(concat([str("fn("), soft_space(), str("x"), str(")")]));
        let mut out = String::new();
        doc.pretty_print_to(80, &mut out).unwrap();
        assert_eq!(out, "fn( x)");
    }

    #[test]
    fn exact_width_boundary_fits() {
        // Limit equals unbroken width: should fit.
        let doc = group(concat([str("ab"), soft_space(), str("cd")]));
        assert_eq!(doc.pretty_print(5), "ab cd");
    }
}

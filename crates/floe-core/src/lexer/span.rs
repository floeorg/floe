/// A source location span tracking where a token appears in the source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Byte offset of the start of this span in the source.
    pub start: usize,
    /// Byte offset of the end of this span (exclusive) in the source.
    pub end: usize,
    /// 1-based line number where this span starts.
    pub line: usize,
    /// 1-based column number where this span starts.
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Length of this span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns true if `inner` is fully contained within this span.
    pub fn contains_span(&self, inner: Span) -> bool {
        inner.start >= self.start && inner.end <= self.end
    }
}

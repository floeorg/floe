//! Columns are measured in bytes (not characters) to match the rest of the
//! compiler's span representation. LSP clients that require UTF-16 code unit
//! columns must translate at the boundary.

/// Precomputed line start positions for a single source file. Allows
/// O(log n) resolution of a byte offset to a 1-based (line, column) pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumbers {
    /// Total length of the source in bytes.
    pub length: u32,
    /// Byte offsets where each line begins, in ascending order.
    ///
    /// Always contains `0` as the first entry. A newline at byte `i` causes
    /// `i + 1` to be recorded as the start of the next line, even if the file
    /// ends there — so an empty trailing line is represented explicitly.
    pub line_starts: Vec<u32>,
}

impl LineNumbers {
    pub fn new(src: &str) -> Self {
        let bytes = src.as_bytes();
        // Heuristic: assume ~40 bytes per line on average. Avoids repeated
        // reallocations for typical source files without over-reserving.
        let mut line_starts = Vec::with_capacity(bytes.len() / 40 + 1);
        line_starts.push(0);
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self {
            length: bytes.len() as u32,
            line_starts,
        }
    }

    /// Offsets past the end of the source return the last line number.
    pub fn line_number(&self, offset: u32) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => (idx + 1) as u32,
            Err(idx) => idx.max(1) as u32,
        }
    }

    /// Column is measured in bytes from the start of the line — a byte offset
    /// inside a multi-byte UTF-8 character yields a column pointing at that
    /// byte, so callers that care about grapheme clusters must translate.
    pub fn line_and_column_number(&self, offset: u32) -> (u32, u32) {
        let clamped = offset.min(self.length);
        let line = self.line_number(clamped);
        let line_start = self.line_starts[(line - 1) as usize];
        let column = clamped - line_start + 1;
        (line, column)
    }

    /// Out-of-range inputs clamp to the source bounds: `line == 0` behaves as
    /// `line == 1`, `column == 0` behaves as `column == 1`, and any position
    /// past the end of the source returns `self.length`.
    pub fn byte_index(&self, line: u32, column: u32) -> u32 {
        let line = line.max(1);
        let idx = (line - 1) as usize;
        if idx >= self.line_starts.len() {
            return self.length;
        }
        let line_start = self.line_starts[idx];
        let col_offset = column.saturating_sub(1);
        (line_start + col_offset).min(self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source() {
        let ln = LineNumbers::new("");
        assert_eq!(ln.length, 0);
        assert_eq!(ln.line_starts, vec![0]);
        assert_eq!(ln.line_and_column_number(0), (1, 1));
    }

    #[test]
    fn single_line_no_newline() {
        let ln = LineNumbers::new("hello");
        assert_eq!(ln.line_starts, vec![0]);
        assert_eq!(ln.line_and_column_number(0), (1, 1));
        assert_eq!(ln.line_and_column_number(1), (1, 2));
        assert_eq!(ln.line_and_column_number(4), (1, 5));
        // At EOF position
        assert_eq!(ln.line_and_column_number(5), (1, 6));
    }

    #[test]
    fn single_line_with_trailing_newline() {
        let ln = LineNumbers::new("hello\n");
        // Line 2 starts just after the newline, even though it's empty.
        assert_eq!(ln.line_starts, vec![0, 6]);
        assert_eq!(ln.line_and_column_number(5), (1, 6));
        assert_eq!(ln.line_and_column_number(6), (2, 1));
    }

    #[test]
    fn multi_line() {
        // "ab\ncde\nf"
        //  01 2 345 6 7
        let ln = LineNumbers::new("ab\ncde\nf");
        assert_eq!(ln.line_starts, vec![0, 3, 7]);

        assert_eq!(ln.line_and_column_number(0), (1, 1)); // 'a'
        assert_eq!(ln.line_and_column_number(1), (1, 2)); // 'b'
        assert_eq!(ln.line_and_column_number(2), (1, 3)); // '\n'
        assert_eq!(ln.line_and_column_number(3), (2, 1)); // 'c'
        assert_eq!(ln.line_and_column_number(4), (2, 2)); // 'd'
        assert_eq!(ln.line_and_column_number(5), (2, 3)); // 'e'
        assert_eq!(ln.line_and_column_number(6), (2, 4)); // '\n'
        assert_eq!(ln.line_and_column_number(7), (3, 1)); // 'f'
    }

    #[test]
    fn line_number_uses_binary_search() {
        // Many lines so the binary search branch actually matters.
        let src: String = (0..100).map(|_| "x\n").collect();
        let ln = LineNumbers::new(&src);
        assert_eq!(ln.line_starts.len(), 101);

        // Byte 0 is the start of line 1.
        assert_eq!(ln.line_number(0), 1);
        // Byte 50 is in line 26 (each line is 2 bytes).
        assert_eq!(ln.line_number(50), 26);
        // Byte 199 is the last 'x' on line 100.
        assert_eq!(ln.line_number(199), 100);
        // Byte 200 is the start of line 101 (past the final \n).
        assert_eq!(ln.line_number(200), 101);
    }

    #[test]
    fn multi_byte_utf8_column_counts_bytes() {
        // 'é' is 2 bytes in UTF-8 (0xC3 0xA9). 'a' 'é' 'b':
        //  byte 0: 'a'
        //  byte 1: 0xC3 (lead byte of 'é')
        //  byte 2: 0xA9 (continuation byte)
        //  byte 3: 'b'
        let src = "aéb";
        let ln = LineNumbers::new(src);
        assert_eq!(ln.length, 4);
        assert_eq!(ln.line_and_column_number(0), (1, 1));
        assert_eq!(ln.line_and_column_number(1), (1, 2)); // lead byte of 'é'
        assert_eq!(ln.line_and_column_number(2), (1, 3)); // continuation byte
        assert_eq!(ln.line_and_column_number(3), (1, 4)); // 'b'
    }

    #[test]
    fn multi_byte_across_lines() {
        let src = "é\né";
        // bytes: [0xC3, 0xA9, b'\n', 0xC3, 0xA9]  length 5
        let ln = LineNumbers::new(src);
        assert_eq!(ln.line_starts, vec![0, 3]);
        assert_eq!(ln.line_and_column_number(0), (1, 1));
        assert_eq!(ln.line_and_column_number(3), (2, 1));
        assert_eq!(ln.line_and_column_number(4), (2, 2));
    }

    #[test]
    fn offset_past_end_clamps() {
        let ln = LineNumbers::new("hi");
        assert_eq!(ln.line_and_column_number(100), (1, 3));
    }

    #[test]
    fn byte_index_round_trip() {
        let src = "ab\ncde\nf";
        let ln = LineNumbers::new(src);
        for offset in 0u32..(src.len() as u32) {
            let (line, col) = ln.line_and_column_number(offset);
            assert_eq!(ln.byte_index(line, col), offset, "offset {offset}");
        }
    }

    #[test]
    fn byte_index_handles_edge_inputs() {
        let ln = LineNumbers::new("ab\ncde");
        // Line 0 clamps to line 1.
        assert_eq!(ln.byte_index(0, 1), 0);
        // Column 0 clamps to column 1.
        assert_eq!(ln.byte_index(2, 0), 3);
        // Line past end clamps to source length.
        assert_eq!(ln.byte_index(99, 1), ln.length);
        // Column past end of file clamps to source length.
        assert_eq!(ln.byte_index(2, 999), ln.length);
    }

    #[test]
    fn consecutive_empty_lines() {
        // "a\n\n\nb" — three newlines, two empty lines in the middle.
        let ln = LineNumbers::new("a\n\n\nb");
        assert_eq!(ln.line_starts, vec![0, 2, 3, 4]);
        assert_eq!(ln.line_number(2), 2); // start of first empty line
        assert_eq!(ln.line_number(3), 3); // start of second empty line
        assert_eq!(ln.line_number(4), 4); // 'b'
    }
}

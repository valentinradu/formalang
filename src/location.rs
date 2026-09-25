/// Source code location information for error reporting and LSP
#[expect(clippy::exhaustive_structs, reason = "public API type")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Location {
    /// Byte offset from start of file
    pub offset: usize,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed, character-based)
    pub column: usize,
}

impl Location {
    #[must_use]
    pub const fn new(offset: usize, line: usize, column: usize) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }

    #[must_use]
    pub const fn start() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }
}

impl Default for Location {
    fn default() -> Self {
        Self::start()
    }
}

/// A span of source code between two locations
#[expect(clippy::exhaustive_structs, reason = "public API type")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span {
    pub start: Location,
    pub end: Location,
}

impl Span {
    #[must_use]
    pub const fn new(start: Location, end: Location) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn single(location: Location) -> Self {
        Self {
            start: location,
            end: location,
        }
    }

    /// Combine two spans into one that covers both
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            start: if self.start.offset < other.start.offset {
                self.start
            } else {
                other.start
            },
            end: if self.end.offset > other.end.offset {
                self.end
            } else {
                other.end
            },
        }
    }

    /// Create a span from byte offsets (for logos compatibility)
    /// Note: This creates a span with line=0, column=0. Use `from_range_with_source` to compute actual positions.
    #[must_use]
    pub const fn from_range(start: usize, end: usize) -> Self {
        Self {
            start: Location {
                offset: start,
                line: 0,
                column: 0,
            },
            end: Location {
                offset: end,
                line: 0,
                column: 0,
            },
        }
    }

    /// Create a span from byte offsets with proper line/column calculation
    #[must_use]
    pub fn from_range_with_source(start: usize, end: usize, source: &str) -> Self {
        Self {
            start: offset_to_location(start, source),
            end: offset_to_location(end, source),
        }
    }
}

/// A reusable byte-offset to line/column converter.
///
/// Building the index costs one pass over the source. Every lookup
/// after that is a binary search for the line plus a character count
/// inside that line.
///
/// Build one index and reuse it whenever you convert more than a
/// single offset. [`offset_to_location`] builds a throwaway index on
/// every call, so converting `n` offsets that way costs `O(n * len)`;
/// the lexer used to do exactly that, once per token, which made
/// lexing quadratic in the source length.
///
/// # Example
///
/// ```
/// use formalang::location::LineIndex;
///
/// let index = LineIndex::new("let x = 1\nlet y = 2\n");
/// let location = index.location(10);
/// assert_eq!((location.line, location.column), (2, 1));
/// ```
#[derive(Debug)]
pub struct LineIndex<'source> {
    source: &'source str,
    /// Byte offset of the first character of every line. Always
    /// starts with `0`, so the length is the number of lines.
    line_starts: Vec<usize>,
    /// The most recent lookup, as `(line index, byte offset, column)`.
    ///
    /// Callers convert offsets in increasing order — the lexer walks
    /// the source forwards — so a lookup usually continues from the
    /// previous one instead of restarting at the line start. Without
    /// it, a source that is one very long line would still cost
    /// `O(line length)` per lookup.
    cursor: std::cell::Cell<(usize, usize, usize)>,
}

impl<'source> LineIndex<'source> {
    /// Index `source`. Costs one pass over it.
    #[must_use]
    pub fn new(source: &'source str) -> Self {
        let mut line_starts = Vec::with_capacity(16);
        line_starts.push(0);
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset.saturating_add(1));
            }
        }
        Self {
            source,
            line_starts,
            cursor: std::cell::Cell::new((0, 0, 1)),
        }
    }

    /// The line and column of `offset`, both one-based.
    ///
    /// An offset past the end of the source reports the position of
    /// the end. An offset that falls inside a multi-byte character
    /// reports the column of the character that encloses it, which is
    /// what a malformed span needs.
    #[must_use]
    pub fn location(&self, offset: usize) -> Location {
        // The line is the last one that starts at or before `offset`.
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);

        // Floor the offset to a character boundary so the count below
        // stops at the character that encloses it.
        let mut end = offset.min(self.source.len());
        while end > line_start && !self.source.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }

        // Continue from the previous lookup when it sits on the same
        // line at or before this offset; otherwise start at the line.
        let (cursor_line, cursor_offset, cursor_column) = self.cursor.get();
        let (from, base_column) = if cursor_line == line_idx && cursor_offset <= end {
            (cursor_offset, cursor_column)
        } else {
            (line_start, 1)
        };

        let counted = self.source.get(from..end).map_or(0, |s| s.chars().count());
        let column = base_column.saturating_add(counted);
        self.cursor.set((line_idx, end, column));

        Location {
            offset,
            line: line_idx.saturating_add(1),
            column,
        }
    }

    /// Fill in the line and column of both ends of `span`.
    #[must_use]
    pub fn fill_span(&self, span: Span) -> Span {
        Span {
            start: self.location(span.start.offset),
            end: self.location(span.end.offset),
        }
    }
}

/// Convert a byte offset to a Location with line and column information.
///
/// If the offset falls inside a multi-byte codepoint (e.g., a malformed span),
/// the returned column is that of the enclosing codepoint rather than the
/// codepoint past it.
///
/// This builds a throwaway [`LineIndex`], so it costs one pass over
/// `source`. Build a [`LineIndex`] yourself when you convert more than
/// one offset.
#[must_use]
pub fn offset_to_location(offset: usize, source: &str) -> Location {
    LineIndex::new(source).location(offset)
}

/// Fill in line/column information for a span given the source text
///
/// Builds a throwaway [`LineIndex`]; use [`LineIndex::fill_span`] when
/// filling more than one span against the same source.
#[must_use]
pub fn fill_span_positions(span: Span, source: &str) -> Span {
    LineIndex::new(source).fill_span(span)
}

use serde::{Deserialize, Serialize};

/// Source code location information for error reporting and LSP
#[expect(clippy::exhaustive_structs, reason = "public API type")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    /// Byte offset from start of file
    pub offset: usize,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed, byte-based)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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

/// Convert a byte offset to a Location with line and column information
#[must_use]
pub fn offset_to_location(offset: usize, source: &str) -> Location {
    let mut line: usize = 1;
    let mut column: usize = 1;
    let mut current_offset: usize = 0;

    for ch in source.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }

        current_offset = current_offset.saturating_add(ch.len_utf8());
    }

    Location {
        offset,
        line,
        column,
    }
}

/// Fill in line/column information for a span given the source text
#[must_use]
pub fn fill_span_positions(span: Span, source: &str) -> Span {
    Span {
        start: offset_to_location(span.start.offset, source),
        end: offset_to_location(span.end.offset, source),
    }
}

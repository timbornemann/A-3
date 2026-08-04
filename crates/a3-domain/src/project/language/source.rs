use std::error::Error;
use std::fmt;

/// Zero-based row and byte-column within one source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourcePosition {
    row: u32,
    column: u32,
}

impl SourcePosition {
    /// Creates a source position already validated against parser output.
    #[must_use]
    pub const fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }

    /// Returns the zero-based row.
    #[must_use]
    pub const fn row(self) -> u32 {
        self.row
    }

    /// Returns the zero-based byte-column.
    #[must_use]
    pub const fn column(self) -> u32 {
        self.column
    }
}

/// Half-open byte and point range within one source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceRange {
    start_byte: u32,
    end_byte: u32,
    start_position: SourcePosition,
    end_position: SourcePosition,
}

impl SourceRange {
    /// Creates a range while rejecting overflow or inverted coordinates.
    pub fn new(
        start_byte: usize,
        end_byte: usize,
        start_position: SourcePosition,
        end_position: SourcePosition,
    ) -> Result<Self, SourceRangeError> {
        let start_byte = u32::try_from(start_byte).map_err(|_| SourceRangeError::OffsetTooLarge)?;
        let end_byte = u32::try_from(end_byte).map_err(|_| SourceRangeError::OffsetTooLarge)?;
        if start_byte > end_byte {
            return Err(SourceRangeError::InvertedBytes);
        }
        if start_position > end_position {
            return Err(SourceRangeError::InvertedPosition);
        }
        Ok(Self {
            start_byte,
            end_byte,
            start_position,
            end_position,
        })
    }

    /// Returns the inclusive start byte.
    #[must_use]
    pub const fn start_byte(self) -> u32 {
        self.start_byte
    }

    /// Returns the exclusive end byte.
    #[must_use]
    pub const fn end_byte(self) -> u32 {
        self.end_byte
    }

    /// Returns the start point.
    #[must_use]
    pub const fn start_position(self) -> SourcePosition {
        self.start_position
    }

    /// Returns the end point.
    #[must_use]
    pub const fn end_position(self) -> SourcePosition {
        self.end_position
    }

    /// Returns the byte length of the half-open range.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end_byte - self.start_byte
    }

    /// Returns whether the range is empty, as is valid for a missing syntax node.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start_byte == self.end_byte
    }

    /// Returns whether this range fully encloses another range.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.start_byte <= other.start_byte
            && self.end_byte >= other.end_byte
            && self.start_position <= other.start_position
            && self.end_position >= other.end_position
    }
}

/// Invalid source coordinates supplied by a parser adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRangeError {
    /// A byte offset exceeded the bounded 32-bit file representation.
    OffsetTooLarge,
    /// The byte start followed the byte end.
    InvertedBytes,
    /// The source start point followed the end point.
    InvertedPosition,
}

impl fmt::Display for SourceRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetTooLarge => formatter.write_str("source offset is too large"),
            Self::InvertedBytes => formatter.write_str("source byte range is inverted"),
            Self::InvertedPosition => formatter.write_str("source point range is inverted"),
        }
    }
}

impl Error for SourceRangeError {}

use a3_domain::{
    AgentFileInspection, AgentFileStartLine, FileRevision, ProjectIdentity, SourceRange,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_AGENT_SOURCE_PAGE_BYTES: usize = 12 * 1_024;

/// Future returned by the object-safe workspace source reader.
pub type AgentSourceReaderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AgentSourcePage, AgentSourceReadFailure>> + Send + 'a>>;

/// Cooperative cancellation boundary for one bounded source-page read.
pub trait AgentSourceReadControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning agent turn requested cancellation.
    fn is_cancelled(&self) -> bool;
}

/// Read-only workspace capability for a content-addressed source page.
pub trait AgentSourceReader: fmt::Debug + Send + Sync {
    /// Reads only requested complete lines after revalidating the full content hash.
    fn read_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_revision: &'a FileRevision,
        request: &'a AgentFileInspection,
        control: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a>;
}

/// Complete-line source page bounded independently from its enclosing tool preview.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentSourcePage {
    revision: FileRevision,
    range: SourceRange,
    start_line: AgentFileStartLine,
    text: String,
    next_start_line: Option<AgentFileStartLine>,
    truncated: bool,
}

impl AgentSourcePage {
    /// Validates normalized source text and a forward-only paging cursor.
    pub fn new(
        revision: FileRevision,
        range: SourceRange,
        start_line: AgentFileStartLine,
        text: String,
        next_start_line: Option<AgentFileStartLine>,
        truncated: bool,
    ) -> Result<Self, AgentSourcePageError> {
        if text.len() > MAX_AGENT_SOURCE_PAGE_BYTES {
            return Err(AgentSourcePageError::TooLarge { actual: text.len() });
        }
        if text.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        }) {
            return Err(AgentSourcePageError::InvalidCharacter);
        }
        if next_start_line.is_some_and(|next| next <= start_line) {
            return Err(AgentSourcePageError::InvalidNextPage);
        }
        if truncated != next_start_line.is_some() {
            return Err(AgentSourcePageError::InvalidTruncation);
        }
        Ok(Self {
            revision,
            range,
            start_line,
            text,
            next_start_line,
            truncated,
        })
    }

    /// Returns the full-content revision revalidated immediately before the read.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the exact byte and line range covered by the page.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }

    /// Returns the one-based first displayed line.
    #[must_use]
    pub const fn start_line(&self) -> AgentFileStartLine {
        self.start_line
    }

    /// Returns source text, possibly empty when the cursor is at EOF.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the next forward-only page cursor when more content exists.
    #[must_use]
    pub const fn next_start_line(&self) -> Option<AgentFileStartLine> {
        self.next_start_line
    }

    /// Returns whether more complete lines remain after this page.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for AgentSourcePage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSourcePage")
            .field("revision", &self.revision)
            .field("range", &self.range)
            .field("start_line", &self.start_line)
            .field("text_bytes", &self.text.len())
            .field("next_start_line", &self.next_start_line)
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// A source adapter returned an invalid page projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSourcePageError {
    /// Page text exceeded the fixed twelve-KiB boundary.
    TooLarge {
        /// Observed UTF-8 bytes.
        actual: usize,
    },
    /// Source text contained NUL or an unsupported control character.
    InvalidCharacter,
    /// A paging cursor did not move forward.
    InvalidNextPage,
    /// Truncation and next-page metadata disagreed.
    InvalidTruncation,
}

impl fmt::Display for AgentSourcePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { actual } => write!(
                formatter,
                "agent source page has {actual} bytes; maximum is {MAX_AGENT_SOURCE_PAGE_BYTES}"
            ),
            Self::InvalidCharacter => {
                formatter.write_str("agent source page contains an unsupported character")
            }
            Self::InvalidNextPage => {
                formatter.write_str("agent source page cursor must move forward")
            }
            Self::InvalidTruncation => {
                formatter.write_str("agent source page truncation metadata is inconsistent")
            }
        }
    }
}

impl Error for AgentSourcePageError {}

/// Stable source-read failure without paths, source content, or OS details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSourceReadFailure {
    /// The source path no longer exists or is not a regular file.
    Unavailable,
    /// Canonical path policy denied the requested source.
    Denied,
    /// Current full content did not match the published revision.
    Stale,
    /// The file crossed the four-MiB repository observation boundary.
    FileTooLarge,
    /// Source was not valid UTF-8 for safe context packing.
    InvalidEncoding,
    /// One complete requested line could not fit the source-page boundary.
    LineTooLong,
    /// The adapter could not construct a valid bounded page.
    InvalidPage,
    /// Cooperative cancellation interrupted the read.
    Cancelled,
}

impl fmt::Display for AgentSourceReadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "agent source file is unavailable",
            Self::Denied => "agent source path was denied",
            Self::Stale => "agent source revision is stale",
            Self::FileTooLarge => "agent source file exceeds the observation boundary",
            Self::InvalidEncoding => "agent source file is not valid UTF-8",
            Self::LineTooLong => "one agent source line exceeds the page boundary",
            Self::InvalidPage => "agent source page is invalid",
            Self::Cancelled => "agent source read was cancelled",
        })
    }
}

impl Error for AgentSourceReadFailure {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{ContentHash, RepositoryPath, SourcePosition};

    #[test]
    fn page_requires_forward_cursor_and_consistent_truncation() -> Result<(), Box<dyn Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?;
        let first = AgentFileStartLine::new(1)?;

        assert!(
            AgentSourcePage::new(
                revision.clone(),
                range,
                first,
                "one\n".to_owned(),
                Some(AgentFileStartLine::new(2)?),
                true,
            )
            .is_ok()
        );
        assert_eq!(
            AgentSourcePage::new(revision, range, first, String::new(), None, true),
            Err(AgentSourcePageError::InvalidTruncation)
        );
        Ok(())
    }
}

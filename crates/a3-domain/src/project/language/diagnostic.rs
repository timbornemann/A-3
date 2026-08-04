use super::SourceRange;
use std::error::Error;
use std::fmt;

const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 1_024;

/// Stable parser diagnostic category independent of one grammar payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParseDiagnosticCode {
    /// Tree-sitter produced an explicit error node.
    SyntaxError,
    /// Tree-sitter inserted a missing syntax node.
    MissingSyntax,
    /// Source text required for a named projection was not valid UTF-8.
    InvalidEncoding,
    /// Valid syntax was outside the adapter's structural contract.
    UnsupportedSyntax,
    /// A bounded output collection could not represent further artifacts.
    OutputTruncated,
}

/// User-visible severity of a stable parse diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParseDiagnosticSeverity {
    /// Structural output is incomplete for the indicated region.
    Error,
    /// Output remains useful but loses some precision.
    Warning,
    /// Informational parser observation.
    Information,
}

/// Bounded safe diagnostic text without source excerpts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagnosticMessage(String);

impl DiagnosticMessage {
    /// Validates a single-line message suitable for logs and UI boundaries.
    pub fn try_from_string(value: String) -> Result<Self, DiagnosticMessageError> {
        if value.is_empty() || value.len() > MAX_DIAGNOSTIC_MESSAGE_BYTES {
            return Err(DiagnosticMessageError::InvalidLength(value.len()));
        }
        if value.chars().any(char::is_control) {
            return Err(DiagnosticMessageError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the safe diagnostic message.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid safe diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticMessageError {
    /// Message was empty or exceeded its fixed limit.
    InvalidLength(usize),
    /// Message contained a control character.
    InvalidCharacter,
}

impl fmt::Display for DiagnosticMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(formatter, "diagnostic message has invalid length {length}")
            }
            Self::InvalidCharacter => {
                formatter.write_str("diagnostic message contains an invalid character")
            }
        }
    }
}

impl Error for DiagnosticMessageError {}

/// One bounded diagnostic tied to exact source evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParseDiagnostic {
    code: ParseDiagnosticCode,
    severity: ParseDiagnosticSeverity,
    range: SourceRange,
    message: DiagnosticMessage,
}

impl ParseDiagnostic {
    /// Creates a typed parser diagnostic.
    #[must_use]
    pub const fn new(
        code: ParseDiagnosticCode,
        severity: ParseDiagnosticSeverity,
        range: SourceRange,
        message: DiagnosticMessage,
    ) -> Self {
        Self {
            code,
            severity,
            range,
            message,
        }
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> ParseDiagnosticCode {
        self.code
    }

    /// Returns diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> ParseDiagnosticSeverity {
        self.severity
    }

    /// Returns the exact evidence range.
    #[must_use]
    pub const fn range(&self) -> SourceRange {
        self.range
    }

    /// Returns bounded safe diagnostic text.
    #[must_use]
    pub const fn message(&self) -> &DiagnosticMessage {
        &self.message
    }
}

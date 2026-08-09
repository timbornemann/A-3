use crate::{PolicyDecisionId, PolicyResourceId, SecretCandidateClassifierV1};
use std::error::Error;
use std::fmt;

const MAX_PROCESS_EVENT_CHUNK_BYTES: usize = 8 * 1_024;

/// One of the two independently capped process output streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProcessStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// BLAKE3 digest of every observed byte in one process stream, including discarded overflow.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessOutputDigest([u8; 32]);

impl ProcessOutputDigest {
    /// Constructs a digest computed by the process adapter.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the stable binary representation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for ProcessOutputDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProcessOutputDigest([REDACTED])")
    }
}

/// Content-free reason retained bytes cannot cross the process adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessOutputRedaction {
    /// Captured bytes were not valid UTF-8.
    InvalidUtf8,
    /// Captured text contained a possible credential.
    SecretCandidate,
    /// Captured text contained a terminal or unsupported control character.
    UnsafeControl,
}

/// Either bounded safe text or one content-free redaction reason.
#[derive(Clone, PartialEq, Eq)]
pub struct ProcessOutputContent {
    text: Option<String>,
    redaction: Option<ProcessOutputRedaction>,
}

impl ProcessOutputContent {
    /// Accepts bounded output text only after secret and control classification.
    pub fn text(value: String) -> Result<Self, ProcessOutputContentError> {
        validate_safe_output_text(&value)?;
        Ok(Self {
            text: Some(value),
            redaction: None,
        })
    }

    /// Records why captured bytes were withheld.
    #[must_use]
    pub const fn redacted(reason: ProcessOutputRedaction) -> Self {
        Self {
            text: None,
            redaction: Some(reason),
        }
    }

    /// Returns safe retained text when no redaction was necessary.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Returns the content-free redaction classification.
    #[must_use]
    pub const fn redaction(&self) -> Option<ProcessOutputRedaction> {
        self.redaction
    }
}

impl fmt::Debug for ProcessOutputContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessOutputContent")
            .field("text_bytes", &self.text.as_ref().map_or(0, String::len))
            .field("redaction", &self.redaction)
            .finish()
    }
}

/// Process output text failed the boundary classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputContentError {
    /// Output contained a possible credential.
    SecretCandidate,
    /// Output contained a terminal or unsupported control character.
    UnsafeControl,
}

impl fmt::Display for ProcessOutputContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SecretCandidate => "process output contains a possible secret",
            Self::UnsafeControl => "process output contains an unsupported control character",
        })
    }
}

impl Error for ProcessOutputContentError {}

/// Final bounded observation of one fully drained process stream.
#[derive(Clone, PartialEq, Eq)]
pub struct ProcessOutputCapture {
    stream: ProcessStream,
    content: ProcessOutputContent,
    observed_bytes: u64,
    retained_limit: u32,
    truncated: bool,
    digest: ProcessOutputDigest,
}

impl ProcessOutputCapture {
    /// Validates the relationship between retained safe text, overflow metadata, and the cap.
    pub fn new(
        stream: ProcessStream,
        content: ProcessOutputContent,
        observed_bytes: u64,
        retained_limit: u32,
        truncated: bool,
        digest: ProcessOutputDigest,
    ) -> Result<Self, ProcessOutputCaptureError> {
        if retained_limit == 0 {
            return Err(ProcessOutputCaptureError::InvalidLimit);
        }
        let retained_bytes = content.as_text().map_or(0usize, str::len);
        let retained_bytes_u64 = u64::try_from(retained_bytes)
            .map_err(|_| ProcessOutputCaptureError::InvalidByteCounts)?;
        if retained_bytes_u64 > u64::from(retained_limit)
            || retained_bytes_u64 > observed_bytes
            || (!truncated && content.redaction().is_none() && retained_bytes_u64 != observed_bytes)
            || (truncated && observed_bytes <= u64::from(retained_limit))
            || (content.redaction().is_some() && observed_bytes == 0)
        {
            return Err(ProcessOutputCaptureError::InvalidByteCounts);
        }
        Ok(Self {
            stream,
            content,
            observed_bytes,
            retained_limit,
            truncated,
            digest,
        })
    }

    /// Returns stdout or stderr.
    #[must_use]
    pub const fn stream(&self) -> ProcessStream {
        self.stream
    }

    /// Returns safe retained text or its content-free redaction.
    #[must_use]
    pub const fn content(&self) -> &ProcessOutputContent {
        &self.content
    }

    /// Returns all bytes drained from the pipe, including discarded overflow.
    #[must_use]
    pub const fn observed_bytes(&self) -> u64 {
        self.observed_bytes
    }

    /// Returns the maximum retained byte count from the specification.
    #[must_use]
    pub const fn retained_limit(&self) -> u32 {
        self.retained_limit
    }

    /// Returns whether bytes beyond the retention cap were drained and discarded.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Returns the full-stream digest.
    #[must_use]
    pub const fn digest(&self) -> ProcessOutputDigest {
        self.digest
    }
}

impl fmt::Debug for ProcessOutputCapture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessOutputCapture")
            .field("stream", &self.stream)
            .field("content", &self.content)
            .field("observed_bytes", &self.observed_bytes)
            .field("retained_limit", &self.retained_limit)
            .field("truncated", &self.truncated)
            .field("digest", &self.digest)
            .finish()
    }
}

/// Final stream capture was internally inconsistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputCaptureError {
    /// Retained limit was zero.
    InvalidLimit,
    /// Observed, retained, and truncated values could not describe one bounded stream.
    InvalidByteCounts,
}

impl fmt::Display for ProcessOutputCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimit => "process output capture limit is invalid",
            Self::InvalidByteCounts => "process output capture byte counts are inconsistent",
        })
    }
}

impl Error for ProcessOutputCaptureError {}

/// Safe, bounded text carried by one live process-output event.
#[derive(Clone, PartialEq, Eq)]
pub struct ProcessOutputChunk(String);

impl ProcessOutputChunk {
    /// Validates one non-empty event chunk independently of the final output capture.
    pub fn try_from_string(value: String) -> Result<Self, ProcessOutputChunkError> {
        if value.is_empty() || value.len() > MAX_PROCESS_EVENT_CHUNK_BYTES {
            return Err(ProcessOutputChunkError::InvalidLength {
                actual: value.len(),
            });
        }
        validate_safe_output_text(&value).map_err(ProcessOutputChunkError::UnsafeContent)?;
        Ok(Self(value))
    }

    /// Returns safe text for the immediate trusted-core consumer.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProcessOutputChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessOutputChunk")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Live output chunk crossed a size or content boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputChunkError {
    /// Chunk was empty or exceeded eight KiB.
    InvalidLength {
        /// Observed UTF-8 byte count.
        actual: usize,
    },
    /// Chunk contained a possible secret or unsafe control character.
    UnsafeContent(ProcessOutputContentError),
}

impl fmt::Display for ProcessOutputChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "process output event chunk length is invalid",
            Self::UnsafeContent(_) => "process output event chunk content is unsafe",
        })
    }
}

impl Error for ProcessOutputChunkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsafeContent(error) => Some(error),
            Self::InvalidLength { .. } => None,
        }
    }
}

/// Positive, per-run event sequence assigned after cross-stream ordering is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessEventSequence(u64);

impl ProcessEventSequence {
    /// Creates a sequence beginning at one.
    pub const fn new(value: u64) -> Result<Self, ProcessEventSequenceError> {
        if value == 0 {
            return Err(ProcessEventSequenceError);
        }
        Ok(Self(value))
    }

    /// Returns the stable primitive representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Process event sequence was zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessEventSequenceError;

impl fmt::Display for ProcessEventSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process event sequence must begin at one")
    }
}

impl Error for ProcessEventSequenceError {}

/// Direct child exit after the owned process group became empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessExit {
    code: Option<i32>,
    success: bool,
}

impl ProcessExit {
    /// Validates the portable equivalence between exit code zero and success.
    pub const fn new(code: Option<i32>, success: bool) -> Result<Self, ProcessExitError> {
        if success != matches!(code, Some(0)) {
            return Err(ProcessExitError);
        }
        Ok(Self { code, success })
    }

    /// Returns the platform exit code, or `None` for signal-like termination.
    #[must_use]
    pub const fn code(self) -> Option<i32> {
        self.code
    }

    /// Returns the platform's success classification.
    #[must_use]
    pub const fn success(self) -> bool {
        self.success
    }
}

/// Platform success and the portable zero exit code disagreed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessExitError;

impl fmt::Display for ProcessExitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process exit success must be equivalent to code zero")
    }
}

impl Error for ProcessExitError {}

/// Why the owned process group reached a terminal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessTermination {
    /// The process group completed without A^3 termination.
    Exited(ProcessExit),
    /// The specification deadline elapsed and the complete group was killed.
    TimedOut,
    /// The owner requested cancellation and the complete group was killed.
    Cancelled,
}

/// Monotonic runtime rounded down to milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessDuration(u64);

impl ProcessDuration {
    /// Constructs adapter-observed monotonic duration.
    #[must_use]
    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable millisecond representation.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

/// Ordered event kind emitted while one process group is owned by the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEventKind {
    /// Spawn and process-group attachment completed.
    Started,
    /// One safe bounded stdout or stderr text fragment became available.
    Output {
        /// Owning stream.
        stream: ProcessStream,
        /// Safe text; debug output exposes only its byte count.
        chunk: ProcessOutputChunk,
    },
    /// The stream cap was crossed; the adapter continues draining without further text events.
    OutputTruncated {
        /// Owning stream.
        stream: ProcessStream,
        /// Bytes observed when overflow first became visible.
        observed_bytes: u64,
    },
    /// Retained content was withheld before leaving the adapter boundary.
    OutputRedacted {
        /// Owning stream.
        stream: ProcessStream,
        /// Stable content-free classification.
        reason: ProcessOutputRedaction,
    },
    /// The complete process group reached its terminal state.
    Terminated(ProcessTermination),
}

/// One stable, specification-bound live process event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEvent {
    specification_id: PolicyResourceId,
    sequence: ProcessEventSequence,
    kind: ProcessEventKind,
}

impl ProcessEvent {
    /// Creates an event after the runner has assigned its cross-stream sequence.
    #[must_use]
    pub const fn new(
        specification_id: PolicyResourceId,
        sequence: ProcessEventSequence,
        kind: ProcessEventKind,
    ) -> Self {
        Self {
            specification_id,
            sequence,
            kind,
        }
    }

    /// Returns the exact process specification identity.
    #[must_use]
    pub const fn specification_id(&self) -> PolicyResourceId {
        self.specification_id
    }

    /// Returns the strictly increasing sequence.
    #[must_use]
    pub const fn sequence(&self) -> ProcessEventSequence {
        self.sequence
    }

    /// Returns the closed event kind.
    #[must_use]
    pub const fn kind(&self) -> &ProcessEventKind {
        &self.kind
    }
}

/// Final process result with exact authorization, terminal state, duration, and both streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRunResult {
    specification_id: PolicyResourceId,
    policy_decision_id: PolicyDecisionId,
    termination: ProcessTermination,
    duration: ProcessDuration,
    stdout: ProcessOutputCapture,
    stderr: ProcessOutputCapture,
}

impl ProcessRunResult {
    /// Requires exactly one final capture for stdout and stderr.
    pub fn new(
        specification_id: PolicyResourceId,
        policy_decision_id: PolicyDecisionId,
        termination: ProcessTermination,
        duration: ProcessDuration,
        stdout: ProcessOutputCapture,
        stderr: ProcessOutputCapture,
    ) -> Result<Self, ProcessRunResultError> {
        if stdout.stream() != ProcessStream::Stdout || stderr.stream() != ProcessStream::Stderr {
            return Err(ProcessRunResultError::StreamMismatch);
        }
        Ok(Self {
            specification_id,
            policy_decision_id,
            termination,
            duration,
            stdout,
            stderr,
        })
    }

    /// Returns the exact process specification identity.
    #[must_use]
    pub const fn specification_id(&self) -> PolicyResourceId {
        self.specification_id
    }

    /// Returns the central decision that opened this process boundary.
    #[must_use]
    pub const fn policy_decision_id(&self) -> PolicyDecisionId {
        self.policy_decision_id
    }

    /// Returns exit, timeout, or cancellation.
    #[must_use]
    pub const fn termination(&self) -> ProcessTermination {
        self.termination
    }

    /// Returns the monotonic observed runtime.
    #[must_use]
    pub const fn duration(&self) -> ProcessDuration {
        self.duration
    }

    /// Returns bounded stdout metadata and safe retained text.
    #[must_use]
    pub const fn stdout(&self) -> &ProcessOutputCapture {
        &self.stdout
    }

    /// Returns bounded stderr metadata and safe retained text.
    #[must_use]
    pub const fn stderr(&self) -> &ProcessOutputCapture {
        &self.stderr
    }
}

/// Final process result placed captures in the wrong slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRunResultError {
    /// Stdout and stderr were swapped or duplicated.
    StreamMismatch,
}

impl fmt::Display for ProcessRunResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process result stream captures are invalid")
    }
}

impl Error for ProcessRunResultError {}

fn validate_safe_output_text(value: &str) -> Result<(), ProcessOutputContentError> {
    if SecretCandidateClassifierV1::classify(value).is_some() {
        return Err(ProcessOutputContentError::SecretCandidate);
    }
    if value.chars().any(|character| {
        character == '\0' || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        return Err(ProcessOutputContentError::UnsafeControl);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_bounded_and_secret_safe() -> Result<(), Box<dyn Error>> {
        let digest = ProcessOutputDigest::from_bytes([1; 32]);
        let capture = ProcessOutputCapture::new(
            ProcessStream::Stdout,
            ProcessOutputContent::text("safe output\n".to_owned())?,
            12,
            16,
            false,
            digest,
        )?;
        assert_eq!(capture.content().as_text(), Some("safe output\n"));
        assert_eq!(capture.digest(), digest);
        assert_eq!(
            ProcessOutputContent::text("token=fixture-secret-value".to_owned()),
            Err(ProcessOutputContentError::SecretCandidate)
        );
        Ok(())
    }

    #[test]
    fn truncated_capture_requires_observed_overflow() -> Result<(), Box<dyn Error>> {
        let digest = ProcessOutputDigest::from_bytes([2; 32]);
        assert_eq!(
            ProcessOutputCapture::new(
                ProcessStream::Stderr,
                ProcessOutputContent::text("four".to_owned())?,
                4,
                4,
                true,
                digest,
            ),
            Err(ProcessOutputCaptureError::InvalidByteCounts)
        );
        assert!(
            ProcessOutputCapture::new(
                ProcessStream::Stderr,
                ProcessOutputContent::text("four".to_owned())?,
                10,
                4,
                true,
                digest,
            )
            .is_ok()
        );
        Ok(())
    }

    #[test]
    fn exit_success_is_equivalent_to_portable_code_zero() {
        assert!(ProcessExit::new(Some(0), true).is_ok());
        assert!(ProcessExit::new(Some(1), false).is_ok());
        assert!(ProcessExit::new(None, false).is_ok());
        assert_eq!(ProcessExit::new(Some(0), false), Err(ProcessExitError));
        assert_eq!(ProcessExit::new(Some(1), true), Err(ProcessExitError));
    }
}

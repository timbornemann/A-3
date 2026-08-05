use crate::JobContext;
use a3_domain::{
    ExploreTarget, ExplorerSearchAction, ModuleCardEvidenceId, ProjectIdentity, SnapshotId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const MAX_OBSERVATION_PREVIEW_BYTES: usize = 16_384;
const MAX_OBSERVATION_EVIDENCE_IDS: usize = 100;

/// Owned future returned by the object-safe read-only explorer tool port.
pub type DeepMapReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ExplorerObservation, DeepMapReadFailure>> + Send + 'a>>;

/// Cooperative cancellation visible to read-only explorer tool adapters.
pub trait DeepMapReadControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation requested cancellation.
    fn is_cancelled(&self) -> bool;
}

impl DeepMapReadControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }
}

/// Fixed bounded deadline applied to each read-only tool operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapReadTimeout(Duration);

impl DeepMapReadTimeout {
    /// Version-one local read deadline.
    pub const DEFAULT: Self = Self(Duration::from_secs(2));

    /// Returns the provider-neutral duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

/// Presence classification for one normalized read-only observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerObservationStatus {
    /// Current published evidence was returned.
    Found,
    /// The bounded read found no current evidence.
    NotFound,
}

/// Bounded normalized read result permitted to enter model context.
#[derive(Clone, PartialEq, Eq)]
pub struct ExplorerObservation {
    status: ExplorerObservationStatus,
    preview: String,
    evidence_ids: Vec<ModuleCardEvidenceId>,
    truncated: bool,
}

impl ExplorerObservation {
    /// Creates one non-empty evidence-bearing result.
    pub fn found(
        preview: String,
        mut evidence_ids: Vec<ModuleCardEvidenceId>,
        truncated: bool,
    ) -> Result<Self, ExplorerObservationError> {
        if preview.trim().is_empty() || preview.len() > MAX_OBSERVATION_PREVIEW_BYTES {
            return Err(ExplorerObservationError::InvalidPreviewBytes(preview.len()));
        }
        evidence_ids.sort_unstable();
        evidence_ids.dedup();
        if evidence_ids.is_empty() || evidence_ids.len() > MAX_OBSERVATION_EVIDENCE_IDS {
            return Err(ExplorerObservationError::InvalidEvidenceCount(
                evidence_ids.len(),
            ));
        }
        Ok(Self {
            status: ExplorerObservationStatus::Found,
            preview,
            evidence_ids,
            truncated,
        })
    }

    /// Creates an explicit empty result without inventing evidence.
    #[must_use]
    pub const fn not_found() -> Self {
        Self {
            status: ExplorerObservationStatus::NotFound,
            preview: String::new(),
            evidence_ids: Vec::new(),
            truncated: false,
        }
    }

    /// Returns whether current evidence was found.
    #[must_use]
    pub const fn status(&self) -> ExplorerObservationStatus {
        self.status
    }

    /// Returns the bounded normalized preview.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }

    /// Returns canonical evidence identities emitted by the trusted read adapter.
    #[must_use]
    pub fn evidence_ids(&self) -> &[ModuleCardEvidenceId] {
        &self.evidence_ids
    }

    /// Returns whether the adapter omitted a bounded tail.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for ExplorerObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExplorerObservation")
            .field("status", &self.status)
            .field("preview_bytes", &self.preview.len())
            .field("evidence_count", &self.evidence_ids.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// Normalized observation exceeded its fixed context boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerObservationError {
    /// Preview was empty or exceeded 16 KiB.
    InvalidPreviewBytes(usize),
    /// Evidence count was zero or exceeded 100 after deduplication.
    InvalidEvidenceCount(usize),
}

impl fmt::Display for ExplorerObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPreviewBytes(actual) => write!(
                formatter,
                "explorer observation preview has {actual} bytes and violates its boundary"
            ),
            Self::InvalidEvidenceCount(actual) => write!(
                formatter,
                "explorer observation has {actual} Evidence IDs and violates its boundary"
            ),
        }
    }
}

impl Error for ExplorerObservationError {}

/// Capability-safe explorer boundary: it exposes reads only, with no write or execute method.
pub trait DeepMapReadTools: fmt::Debug + Send + Sync {
    /// Inspects exactly one planner-owned immutable target from the published snapshot.
    fn inspect<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        target: &'a ExploreTarget,
        timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a>;

    /// Executes one typed exact, lexical, or graph read against the published snapshot.
    fn search<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        action: &'a ExplorerSearchAction,
        timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a>;
}

/// Stable read-tool failure without source text, SQL, paths, or provider payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapReadFailure {
    /// The published snapshot is no longer available.
    SnapshotUnavailable,
    /// The exact planned target is absent from the published snapshot.
    TargetUnavailable,
    /// The read adapter rejected a bounded typed query.
    Rejected,
    /// The adapter exceeded its fixed deadline.
    TimedOut,
    /// The owning operation cancelled the read.
    Cancelled,
    /// Adapter output violated the normalized observation contract.
    InvalidResponse,
}

impl fmt::Display for DeepMapReadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SnapshotUnavailable => "explorer snapshot is unavailable",
            Self::TargetUnavailable => "explorer target is unavailable",
            Self::Rejected => "explorer read tool rejected the request",
            Self::TimedOut => "explorer read tool timed out",
            Self::Cancelled => "explorer read tool was cancelled",
            Self::InvalidResponse => "explorer read tool returned an invalid response",
        })
    }
}

impl Error for DeepMapReadFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_are_bounded_canonical_and_redacted() -> Result<(), Box<dyn std::error::Error>> {
        let evidence = ModuleCardEvidenceId::from_bytes([1; 32]);
        let observation = ExplorerObservation::found(
            "private source preview".to_owned(),
            vec![evidence, evidence],
            true,
        )?;
        assert_eq!(observation.evidence_ids(), &[evidence]);
        assert!(!format!("{observation:?}").contains("private source preview"));
        assert_eq!(
            ExplorerObservation::not_found().status(),
            ExplorerObservationStatus::NotFound
        );
        assert!(ExplorerObservation::found("value".to_owned(), Vec::new(), false).is_err());
        Ok(())
    }
}

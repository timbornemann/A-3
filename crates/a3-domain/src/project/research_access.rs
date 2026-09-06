//! Content-free access receipts. A completed read is not a completed question or a fact.
use super::{ContentHash, ResearchQuestionId, ResearchWorkError};

/// Finite kind of an existing, read-only research capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchAccessKind {
    /// Original file page, independent of the source's display alias.
    Inspect,
    /// Bounded literal OR-search in safe current sources.
    LiteralSearch,
    /// Candidate selection through the existing index/lens.
    IndexSearch,
    /// Direct indexed directory entries.
    Directory,
    /// One closed static relationship class.
    Relations,
    /// Existing static function steps or values.
    Flow,
    /// Current working-change paths.
    Changes,
    /// Current parser/index diagnostics.
    Diagnostics,
    /// Bounded dependency topology.
    Dependencies,
    /// Bounded static test topology, never runtime coverage.
    Tests,
    /// Versioned local security candidates, never confirmed vulnerabilities.
    SecurityCandidates,
}

/// Core-observed outcome, separate from the attempt and any model interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchAccessOutcome {
    /// The bounded action returned; this does not imply new evidence or completeness.
    Completed,
    /// The exact completed search/listing found no match within its pinned scope.
    NoMatch,
    /// A target could not be uniquely resolved in the pinned index.
    Unresolved,
    /// An explicit resource/completeness limit was encountered.
    Limited,
    /// A read failed; this must never become negative evidence.
    Unavailable,
}

/// One bounded canonical access identity and its latest execution receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchAccessAttempt {
    /// Stable question that required the access.
    pub question: ResearchQuestionId,
    /// Hash of the pinned publication identity; old scopes cannot suppress new access.
    pub scope: ContentHash,
    /// Versioned hash of resolved arguments, not model prose or source aliases.
    pub key: ContentHash,
    /// Capability class, for typed diagnostics only.
    pub kind: ResearchAccessKind,
    /// Actual action starts, including interruptions, not inner retries or question progress.
    pub starts: u16,
    /// None means started but not acknowledged, e.g. after cancellation or a crash.
    pub outcome: Option<ResearchAccessOutcome>,
}

impl ResearchAccessAttempt {
    pub(super) fn validate(&self) -> Result<(), ResearchWorkError> {
        if self.starts == 0 || self.starts > 256 {
            return Err(ResearchWorkError::AttemptLimit);
        }
        if (self.outcome == Some(ResearchAccessOutcome::NoMatch)
            && !matches!(
                self.kind,
                ResearchAccessKind::LiteralSearch
                    | ResearchAccessKind::Directory
                    | ResearchAccessKind::SecurityCandidates
            ))
            || (self.outcome == Some(ResearchAccessOutcome::Unresolved)
                && !matches!(
                    self.kind,
                    ResearchAccessKind::Inspect | ResearchAccessKind::Flow
                ))
        {
            return Err(ResearchWorkError::InvalidTransition);
        }
        Ok(())
    }

    /// Only deterministic negative results suppress a later identical access.
    /// Successful content is deliberately reread when a volatile cache needs hydration.
    #[must_use]
    pub fn excludes_repeat(&self) -> bool {
        matches!(
            self.outcome,
            Some(ResearchAccessOutcome::NoMatch | ResearchAccessOutcome::Unresolved)
        )
    }
}

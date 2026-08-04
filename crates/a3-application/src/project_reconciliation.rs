use crate::ProjectPathDisplay;
use a3_domain::{ProjectId, RepositoryId, WorktreeAnchorId, WorktreeId};
use std::error::Error;
use std::fmt;

/// Evidence class that made one previous worktree an unambiguous move candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectReconciliationEvidence {
    /// Repository identity and repository-local worktree anchor both match.
    RepositoryAndWorktreeAnchor,
    /// A changed repository location retained both remote fingerprint and worktree anchor.
    RemoteAndWorktreeAnchor,
}

/// Positive catalog revision used to reject stale reconciliation proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectCatalogRevision(u64);

impl ProjectCatalogRevision {
    /// Creates a revision only from a positive durable open sequence.
    pub fn new(value: u64) -> Result<Self, ProjectCatalogRevisionError> {
        if value == 0 {
            return Err(ProjectCatalogRevisionError);
        }
        Ok(Self(value))
    }

    /// Returns the portable integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A zero value cannot identify a durable catalog observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectCatalogRevisionError;

impl fmt::Display for ProjectCatalogRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project catalog revision must be positive")
    }
}

impl Error for ProjectCatalogRevisionError {}

/// Exact previous observation that may be reconciled with a newly inspected worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReconciliationProposal {
    project_id: ProjectId,
    previous_repository_id: RepositoryId,
    previous_worktree_id: WorktreeId,
    previous_worktree_anchor_id: WorktreeAnchorId,
    previous_root_display: ProjectPathDisplay,
    expected_revision: ProjectCatalogRevision,
    evidence: ProjectReconciliationEvidence,
}

impl ProjectReconciliationProposal {
    /// Creates a proposal from adapter-validated, non-secret catalog evidence.
    #[must_use]
    pub const fn new(
        project_id: ProjectId,
        previous_repository_id: RepositoryId,
        previous_worktree_id: WorktreeId,
        previous_worktree_anchor_id: WorktreeAnchorId,
        previous_root_display: ProjectPathDisplay,
        expected_revision: ProjectCatalogRevision,
        evidence: ProjectReconciliationEvidence,
    ) -> Self {
        Self {
            project_id,
            previous_repository_id,
            previous_worktree_id,
            previous_worktree_anchor_id,
            previous_root_display,
            expected_revision,
            evidence,
        }
    }

    /// Returns the stable catalog identity retained after confirmation.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the repository identity persisted for the previous observation.
    #[must_use]
    pub const fn previous_repository_id(&self) -> RepositoryId {
        self.previous_repository_id
    }

    /// Returns the previous location-scoped worktree identity.
    #[must_use]
    pub const fn previous_worktree_id(&self) -> WorktreeId {
        self.previous_worktree_id
    }

    /// Returns the Git metadata anchor shared with the new observation.
    #[must_use]
    pub const fn previous_worktree_anchor_id(&self) -> WorktreeAnchorId {
        self.previous_worktree_anchor_id
    }

    /// Returns the safe previous path text used only for native confirmation.
    #[must_use]
    pub const fn previous_root_display(&self) -> &ProjectPathDisplay {
        &self.previous_root_display
    }

    /// Returns the catalog revision that must still be current at confirmation.
    #[must_use]
    pub const fn expected_revision(&self) -> ProjectCatalogRevision {
        self.expected_revision
    }

    /// Returns the evidence class that justified presenting the proposal.
    #[must_use]
    pub const fn evidence(&self) -> ProjectReconciliationEvidence {
        self.evidence
    }
}

/// Storage-side preparation result for one safely inspected project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectOpenPreparation {
    /// No prior location needs a user decision.
    Ready,
    /// An exact candidate requires native user confirmation before any mutation.
    ConfirmationRequired(ProjectReconciliationProposal),
    /// A prior native confirmation was durably prepared and may be resumed idempotently.
    ResumeConfirmed(ProjectReconciliationProposal),
}

#[cfg(test)]
mod tests {
    use super::{ProjectCatalogRevision, ProjectCatalogRevisionError};

    #[test]
    fn catalog_revision_rejects_zero() {
        assert_eq!(
            ProjectCatalogRevision::new(0),
            Err(ProjectCatalogRevisionError)
        );
        assert_eq!(
            ProjectCatalogRevision::new(1).map(ProjectCatalogRevision::get),
            Ok(1)
        );
    }
}

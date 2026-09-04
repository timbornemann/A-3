use std::fmt;

const ID_LENGTH: usize = 32;

macro_rules! stable_id {
    ($(#[$metadata:meta])* $name:ident) => {
        $(#[$metadata])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; ID_LENGTH]);

        impl $name {
            /// Constructs an ID from a versioned 256-bit derivation.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; ID_LENGTH]) -> Self {
                Self(bytes)
            }

            /// Returns the canonical binary representation used by derivation and persistence.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; ID_LENGTH] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(&self.0, formatter)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "("))?;
                write_hex(&self.0, formatter)?;
                formatter.write_str(")")
            }
        }
    };
}

stable_id!(
    /// Stable identity of one catalog project across confirmed location changes.
    ProjectId
);
stable_id!(
    /// Stable local identity of a logical Git repository.
    RepositoryId
);
stable_id!(
    /// Stable identity of one concrete worktree location within a repository.
    WorktreeId
);
stable_id!(
    /// Stable digest of Git's repository-local metadata anchor for one worktree.
    WorktreeAnchorId
);
stable_id!(
    /// Credential-free fingerprint of a repository's normalized primary remote.
    RemoteIdentity
);
stable_id!(
    /// Stable identity of one immutable observed worktree snapshot.
    SnapshotId
);
stable_id!(
    /// Stable identity of one deterministic index attempt.
    IndexRunId
);
stable_id!(
    /// Stable identity of one durable Deep-Map run.
    DeepMapRunId
);
stable_id!(
    /// Stable identity of one durable coding task across Goal Contract revisions and runs.
    TaskId
);
stable_id!(
    /// Stable identity of one project-bound Agent conversation.
    AgentSessionId
);
stable_id!(
    /// Opaque identity of one source disclosed by a bounded Ask research turn.
    AskResearchSourceId
);
stable_id!(
    /// Opaque identity of one evidence-bound diagram artifact in an Agent conversation.
    AgentDiagramArtifactId
);
stable_id!(
    /// Stable identity of one task-bearing work item within an Agent conversation.
    AgentWorkItemId
);
stable_id!(
    /// Stable identity of one acceptance criterion across Goal Contract revisions.
    AcceptanceCriterionId
);
stable_id!(
    /// Stable identity of one task-plan step across replans and attempts.
    TaskStepId
);
stable_id!(
    /// Stable identity of one evidence artifact attached to a task-step attempt.
    TaskEvidenceId
);
stable_id!(
    /// Stable identity of one immutable task-step verification specification.
    VerificationSpecId
);
stable_id!(
    /// Stable identity of one completed verification execution.
    StepVerificationId
);
stable_id!(
    /// Stable identity shared by every evidence artifact from one verification run.
    VerificationRunId
);
stable_id!(
    /// Stable identity of one controlled agent run.
    AgentRunId
);
stable_id!(
    /// Stable identity of one append-only event in an agent run journal.
    RunEventId
);
stable_id!(
    /// Stable identity of one bounded tool execution referenced by run audit.
    ToolRunId
);
stable_id!(
    /// Stable identity of one central policy evaluation.
    PolicyDecisionId
);
stable_id!(
    /// Stable identity of one explicit user-approval request.
    ApprovalRequestId
);
stable_id!(
    /// Stable identity of one granted, scope-bound approval.
    ApprovalId
);
stable_id!(
    /// Content-free identity of one external or executable policy resource.
    PolicyResourceId
);
stable_id!(
    /// Stable identity of one manifest-evidenced discovered project command.
    DiscoveredCommandId
);
stable_id!(
    /// Stable identity of one complete project command catalog revision.
    CommandCatalogId
);

fn write_hex(bytes: &[u8; ID_LENGTH], formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AcceptanceCriterionId, AgentDiagramArtifactId, AgentRunId, AgentSessionId, AgentWorkItemId,
        ApprovalId, ApprovalRequestId, AskResearchSourceId, CommandCatalogId, DeepMapRunId,
        DiscoveredCommandId, IndexRunId, PolicyDecisionId, PolicyResourceId, ProjectId,
        RemoteIdentity, RepositoryId, RunEventId, SnapshotId, StepVerificationId, TaskEvidenceId,
        TaskId, TaskStepId, ToolRunId, VerificationRunId, VerificationSpecId, WorktreeAnchorId,
        WorktreeId,
    };

    #[test]
    fn stable_ids_have_fixed_lowercase_hex_representation() {
        let bytes = [0xabu8; 32];

        assert_eq!(
            RepositoryId::from_bytes(bytes).to_string(),
            "abababababababababababababababababababababababababababababababab"
        );
        assert_eq!(WorktreeId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(WorktreeAnchorId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ProjectId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(RemoteIdentity::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(SnapshotId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(IndexRunId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(DeepMapRunId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(TaskId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AgentSessionId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AskResearchSourceId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AgentDiagramArtifactId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AgentWorkItemId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AcceptanceCriterionId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(TaskStepId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(TaskEvidenceId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(VerificationSpecId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(StepVerificationId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(VerificationRunId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(AgentRunId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(RunEventId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ToolRunId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(PolicyDecisionId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ApprovalRequestId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(ApprovalId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(PolicyResourceId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(DiscoveredCommandId::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(CommandCatalogId::from_bytes(bytes).as_bytes(), &bytes);
    }
}

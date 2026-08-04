//! Framework-independent domain model and invariants for A^3.

mod health;
mod job;
mod platform;
mod progress;
mod project;
mod version;

pub use health::Health;
pub use job::{JobId, JobOwner, JobStatus};
pub use platform::Platform;
pub use progress::{Progress, ProgressTransitionError, ProgressValueError};
pub use project::{
    CanonicalDirectory, CanonicalDirectoryError, ContentHash, GitHead, GitObjectId,
    GitObjectIdError, GitReferenceName, GitReferenceNameError, IndexLanguage, IndexRunId,
    IndexRunRecord, IndexRunSequence, IndexRunSequenceError, IndexRunStart, IndexRunStatus,
    IndexRunStatusError, IndexRunTerminalOutcome, IndexSchemaVersion, IndexSchemaVersionError,
    LanguageAdapterRevision, LanguageAdapterVersion, LanguageAdapterVersionError, ProjectId,
    ProjectIdentity, ProjectIdentityError, RankingPolicyVersion, RankingPolicyVersionError,
    RemoteIdentity, RepositoryId, RepositoryIdentity, RepositoryPath, RepositoryPathError,
    Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotError, SnapshotId, WorktreeAnchorId,
    WorktreeGeneration, WorktreeGenerationError, WorktreeId, WorktreeIdentity,
};
pub use version::{ApplicationVersion, ApplicationVersionError};

//! Exact existing v3 digest encoding shared by live preparation and closed replay.
//! These private inputs describe hash bytes, never an active execution anchor.
use crate::TaskLedgerStoreVersion;
use a3_domain::{
    AgentRunId, ContentHash, FileRevision, GoalContractReference, ModelProfileReference,
    RepositoryPath, RunEventSequence, SnapshotId, TaskLedgerRevision, TaskStepAttemptNumber,
    TaskStepId, VerificationSpecId, WorktreeId,
};

pub(crate) struct EditJobOwnerProjection {
    pub worktree: WorktreeId,
    pub goal: GoalContractReference,
    pub ledger: TaskLedgerRevision,
    pub version: TaskLedgerStoreVersion,
    pub run: AgentRunId,
    pub sequence: RunEventSequence,
    pub step: TaskStepId,
    pub attempt: TaskStepAttemptNumber,
    pub verification: VerificationSpecId,
    pub snapshot: SnapshotId,
    pub empty_published_index: bool,
    pub planned_file_addition_scope: Option<ContentHash>,
    pub model_profile: ModelProfileReference,
}

fn owner_hasher(owner: &EditJobOwnerProjection) -> blake3::Hasher {
    let mut hash = blake3::Hasher::new_derive_key("a3 edit phase window v3");
    hash.update(owner.worktree.as_bytes());
    hash.update(owner.goal.task_id().as_bytes());
    hash.update(&owner.goal.revision().get().to_le_bytes());
    hash.update(&owner.ledger.get().to_le_bytes());
    hash.update(&owner.version.get().to_le_bytes());
    hash.update(owner.run.as_bytes());
    hash.update(&owner.sequence.get().to_le_bytes());
    hash.update(owner.step.as_bytes());
    hash.update(&owner.attempt.get().to_le_bytes());
    hash.update(owner.verification.as_bytes());
    hash.update(owner.snapshot.as_bytes());
    hash.update(&[u8::from(owner.empty_published_index)]);
    if let Some(scope) = owner.planned_file_addition_scope {
        hash.update(b"planned file addition binding v1");
        hash.update(scope.as_bytes());
    }
    hash.update(owner.model_profile.id().as_bytes());
    hash.update(&owner.model_profile.version().get().to_le_bytes());
    hash
}

pub(crate) fn existing_target_digest(
    owner: &EditJobOwnerProjection,
    revision: &FileRevision,
    start: u64,
    end: u64,
    exact: &str,
    append: bool,
) -> blake3::Hash {
    let mut hash = owner_hasher(owner);
    hash.update(&[if append { 2 } else { 1 }]);
    hash.update(revision.content_hash().as_bytes());
    hash.update(&start.to_le_bytes());
    hash.update(&end.to_le_bytes());
    hash_part(&mut hash, revision.path().as_bytes());
    hash_part(&mut hash, exact.as_bytes());
    hash.finalize()
}

pub(crate) fn add_target_digest(
    owner: &EditJobOwnerProjection,
    path: &RepositoryPath,
) -> blake3::Hash {
    let mut hash = owner_hasher(owner);
    hash.update(&[3]);
    hash_part(&mut hash, path.as_bytes());
    hash.finalize()
}

fn hash_part(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

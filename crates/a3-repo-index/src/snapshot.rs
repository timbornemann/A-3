use crate::discovery::GitRepositoryDiscoverer;
use crate::hashing::hash_discovery;
use crate::repository::{
    RepositoryValidationError, inspect_head, inspect_index_checksum, open_validated,
};
use a3_application::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositoryDiscoveryFailure, RepositorySnapshotBuild, RepositorySnapshotBuilder,
    RepositorySnapshotControl, RepositorySnapshotFailure, RepositorySnapshotPolicy,
    SnapshotBaseline, SnapshotCompatibility,
};
use a3_domain::{
    DiscoveryPolicyVersion, GitHead, Snapshot, SnapshotDelta, SnapshotId, WorktreeGeneration,
};

/// Bounded local snapshot builder using full-content BLAKE3 digests.
#[derive(Debug, Clone, Default)]
pub struct Blake3RepositorySnapshotBuilder {
    discoverer: GitRepositoryDiscoverer,
}

impl Blake3RepositorySnapshotBuilder {
    /// Creates the V1 local builder with isolated Git discovery.
    #[must_use]
    pub fn new() -> Self {
        Self {
            discoverer: GitRepositoryDiscoverer::new(),
        }
    }
}

impl RepositorySnapshotBuilder for Blake3RepositorySnapshotBuilder {
    fn build_snapshot(
        &self,
        project: &a3_domain::ProjectIdentity,
        baseline: &SnapshotBaseline,
        compatibility: &SnapshotCompatibility,
        policy: RepositorySnapshotPolicy,
        control: &dyn RepositorySnapshotControl,
    ) -> Result<RepositorySnapshotBuild, RepositorySnapshotFailure> {
        ensure_active(control)?;
        report_indeterminate(control)?;
        validate_baseline(project, baseline)?;
        let repository = open_validated(project).map_err(map_repository_error)?;
        let head_before = inspect_head(&repository).map_err(map_repository_error)?;
        let index_before = inspect_index_checksum(&repository).map_err(map_repository_error)?;

        let discovery_control = SnapshotDiscoveryControl { inner: control };
        let discovery = self
            .discoverer
            .discover(project, policy.discovery(), &discovery_control)
            .map_err(map_discovery_error)?;
        if discovery.worktree_id() != project.worktree().id()
            || discovery.policy_version() != policy.discovery().version()
        {
            return Err(RepositorySnapshotFailure::IdentityMismatch);
        }
        let files = hash_discovery(
            project.worktree().root().as_path(),
            &discovery,
            policy,
            control,
        )?;

        let head_after = inspect_head(&repository).map_err(map_repository_error)?;
        let index_after = inspect_index_checksum(&repository).map_err(map_repository_error)?;
        if head_before != head_after || index_before != index_after {
            return Err(RepositorySnapshotFailure::WorktreeChanged);
        }

        let delta = SnapshotDelta::between(baseline.files(), &files);
        if baseline.latest_snapshot().is_some_and(|latest| {
            delta.is_empty()
                && latest.head() == &head_after
                && latest.index_schema_version() == compatibility.index_schema_version()
                && latest.adapter_revisions() == compatibility.adapter_revisions()
        }) {
            return Ok(RepositorySnapshotBuild::Unchanged { discovery, files });
        }

        let (parent_id, generation) = match baseline.latest_snapshot() {
            Some(latest) => (
                Some(latest.id()),
                latest
                    .generation()
                    .next()
                    .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?,
            ),
            None => (
                None,
                WorktreeGeneration::new(1)
                    .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?,
            ),
        };
        let changes = delta.snapshot_changes();
        let snapshot_id = derive_snapshot_id(
            project.worktree().id(),
            parent_id,
            generation,
            &head_after,
            discovery.policy_version(),
            compatibility,
            &changes,
        )?;
        let snapshot = Snapshot::new(
            snapshot_id,
            project.worktree().id(),
            parent_id,
            generation,
            head_after,
            compatibility.index_schema_version(),
            compatibility.adapter_revisions().to_vec(),
            changes,
        )
        .map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?;
        Ok(RepositorySnapshotBuild::Created {
            discovery,
            files,
            delta,
            snapshot: Box::new(snapshot),
        })
    }
}

fn validate_baseline(
    project: &a3_domain::ProjectIdentity,
    baseline: &SnapshotBaseline,
) -> Result<(), RepositorySnapshotFailure> {
    if baseline
        .latest_snapshot()
        .is_some_and(|snapshot| snapshot.worktree_id() != project.worktree().id())
    {
        return Err(RepositorySnapshotFailure::IdentityMismatch);
    }
    Ok(())
}

fn derive_snapshot_id(
    worktree_id: a3_domain::WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: WorktreeGeneration,
    head: &GitHead,
    discovery_version: DiscoveryPolicyVersion,
    compatibility: &SnapshotCompatibility,
    changes: &[a3_domain::SnapshotChange],
) -> Result<SnapshotId, RepositorySnapshotFailure> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3.snapshot.v1\0");
    hasher.update(worktree_id.as_bytes());
    match parent_id {
        Some(parent_id) => {
            hasher.update(&[1]);
            hasher.update(parent_id.as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    hasher.update(&generation.get().to_be_bytes());
    update_head(&mut hasher, head)?;
    hasher.update(&discovery_version.get().to_be_bytes());
    hasher.update(&compatibility.index_schema_version().get().to_be_bytes());
    update_length(&mut hasher, compatibility.adapter_revisions().len())?;
    for revision in compatibility.adapter_revisions() {
        update_bytes(&mut hasher, revision.language().as_str().as_bytes())?;
        update_bytes(&mut hasher, revision.version().as_str().as_bytes())?;
    }
    update_length(&mut hasher, changes.len())?;
    for change in changes {
        update_bytes(&mut hasher, change.path().as_bytes())?;
        hasher.update(&[match change.kind() {
            a3_domain::SnapshotChangeKind::Upsert => 1,
            a3_domain::SnapshotChangeKind::Delete => 2,
        }]);
        hasher.update(change.content_hash().as_bytes());
    }
    Ok(SnapshotId::from_bytes(*hasher.finalize().as_bytes()))
}

fn update_head(
    hasher: &mut blake3::Hasher,
    head: &GitHead,
) -> Result<(), RepositorySnapshotFailure> {
    match head {
        GitHead::Born {
            object_id,
            reference,
        } => {
            hasher.update(&[1]);
            update_bytes(hasher, object_id.as_str().as_bytes())?;
            match reference {
                Some(reference) => {
                    hasher.update(&[1]);
                    update_bytes(hasher, reference.as_str().as_bytes())?;
                }
                None => {
                    hasher.update(&[0]);
                }
            }
        }
        GitHead::Unborn { reference } => {
            hasher.update(&[2]);
            update_bytes(hasher, reference.as_str().as_bytes())?;
        }
    }
    Ok(())
}

fn update_bytes(
    hasher: &mut blake3::Hasher,
    bytes: &[u8],
) -> Result<(), RepositorySnapshotFailure> {
    update_length(hasher, bytes.len())?;
    hasher.update(bytes);
    Ok(())
}

fn update_length(
    hasher: &mut blake3::Hasher,
    length: usize,
) -> Result<(), RepositorySnapshotFailure> {
    let length = u64::try_from(length).map_err(|_| RepositorySnapshotFailure::InvalidSnapshot)?;
    hasher.update(&length.to_be_bytes());
    Ok(())
}

fn ensure_active(control: &dyn RepositorySnapshotControl) -> Result<(), RepositorySnapshotFailure> {
    if control.is_cancelled() {
        return Err(RepositorySnapshotFailure::Cancelled);
    }
    Ok(())
}

fn report_indeterminate(
    control: &dyn RepositorySnapshotControl,
) -> Result<(), RepositorySnapshotFailure> {
    control
        .report_progress(a3_domain::Progress::Indeterminate)
        .map_err(|_| RepositorySnapshotFailure::ProgressUnavailable)
}

fn map_repository_error(error: RepositoryValidationError) -> RepositorySnapshotFailure {
    match error {
        RepositoryValidationError::RootUnavailable => RepositorySnapshotFailure::Filesystem,
        RepositoryValidationError::InvalidRepository => {
            RepositorySnapshotFailure::InvalidRepository
        }
    }
}

fn map_discovery_error(error: RepositoryDiscoveryFailure) -> RepositorySnapshotFailure {
    match error {
        RepositoryDiscoveryFailure::Cancelled => RepositorySnapshotFailure::Cancelled,
        RepositoryDiscoveryFailure::RootUnavailable
        | RepositoryDiscoveryFailure::InvalidPath
        | RepositoryDiscoveryFailure::Filesystem => RepositorySnapshotFailure::Filesystem,
        RepositoryDiscoveryFailure::InvalidRepository => {
            RepositorySnapshotFailure::InvalidRepository
        }
        RepositoryDiscoveryFailure::ResourceLimitExceeded => {
            RepositorySnapshotFailure::ResourceLimitExceeded
        }
        RepositoryDiscoveryFailure::ProgressUnavailable => {
            RepositorySnapshotFailure::ProgressUnavailable
        }
        RepositoryDiscoveryFailure::InvalidResult => RepositorySnapshotFailure::InvalidSnapshot,
        RepositoryDiscoveryFailure::InvalidConfiguration => RepositorySnapshotFailure::Discovery,
    }
}

#[derive(Debug)]
struct SnapshotDiscoveryControl<'a> {
    inner: &'a dyn RepositorySnapshotControl,
}

impl RepositoryDiscoveryControl for SnapshotDiscoveryControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), RepositoryDiscoveryControlError> {
        Ok(())
    }
}

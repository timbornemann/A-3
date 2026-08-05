use crate::ContractResult;
use a3_domain::{
    CanonicalDirectory, ContentHash, GitHead, GitObjectId, GitReferenceName, IndexLanguage,
    IndexRunId, IndexRunStart, IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion,
    ProjectIdentity, RankingPolicyVersion, RemoteIdentity, RepositoryId, RepositoryIdentity,
    RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind, SnapshotId, WorktreeAnchorId,
    WorktreeGeneration, WorktreeId, WorktreeIdentity,
};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_TEMP_DIRECTORY_ATTEMPTS: u64 = 100;

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(crate) struct ContractWorkspace {
    path: PathBuf,
}

impl ContractWorkspace {
    pub(crate) fn new() -> io::Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..MAX_TEMP_DIRECTORY_ATTEMPTS {
            let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!(
                "a3-storage-contract-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique storage contract workspace",
        ))
    }

    pub(crate) fn app_data_root(&self, scenario: &str) -> PathBuf {
        self.path.join(format!("{scenario}-app-data"))
    }

    pub(crate) fn create_directory(&self, name: &str) -> ContractResult<CanonicalDirectory> {
        let path = self.path.join(name);
        fs::create_dir(&path)?;
        Ok(CanonicalDirectory::from_canonicalized(fs::canonicalize(
            path,
        )?)?)
    }
}

impl Drop for ContractWorkspace {
    fn drop(&mut self) {
        #[cfg(windows)]
        if std::env::var_os("A3_STORAGE_CONTRACT_RETAIN_WORKSPACE").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        if let Err(error) = fs::remove_dir_all(&self.path) {
            eprintln!(
                "could not remove storage contract workspace {}: {error}",
                self.path.display()
            );
        }
    }
}

pub(crate) fn project(
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    common_directory: &CanonicalDirectory,
    worktree_root: &CanonicalDirectory,
    head: GitHead,
) -> ContractResult<ProjectIdentity> {
    project_with_evidence(
        repository_id,
        worktree_id,
        ProjectEvidence {
            worktree_anchor_id: WorktreeAnchorId::from_bytes(*worktree_id.as_bytes()),
            main_remote: None,
        },
        common_directory,
        worktree_root,
        head,
    )
}

pub(crate) struct ProjectEvidence {
    pub(crate) worktree_anchor_id: WorktreeAnchorId,
    pub(crate) main_remote: Option<RemoteIdentity>,
}

pub(crate) fn project_with_evidence(
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    evidence: ProjectEvidence,
    common_directory: &CanonicalDirectory,
    worktree_root: &CanonicalDirectory,
    head: GitHead,
) -> ContractResult<ProjectIdentity> {
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository_id,
            common_directory.clone(),
            evidence.main_remote,
        ),
        WorktreeIdentity::new(
            worktree_id,
            evidence.worktree_anchor_id,
            repository_id,
            worktree_root.clone(),
        ),
        head,
    )?)
}

pub(crate) fn snapshot(
    id: [u8; 32],
    worktree_id: WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: u64,
    changes: Vec<SnapshotChange>,
) -> ContractResult<Snapshot> {
    Ok(Snapshot::new(
        SnapshotId::from_bytes(id),
        worktree_id,
        parent_id,
        WorktreeGeneration::new(generation)?,
        unborn_head()?,
        IndexSchemaVersion::new(1)?,
        vec![
            LanguageAdapterRevision::new(
                IndexLanguage::Rust,
                LanguageAdapterVersion::try_from_string("contract-rust-1".to_owned())?,
            ),
            LanguageAdapterRevision::new(
                IndexLanguage::Generic,
                LanguageAdapterVersion::try_from_string("contract-generic-1".to_owned())?,
            ),
        ],
        changes,
    )?)
}

pub(crate) fn change(
    path: &[u8],
    hash: [u8; 32],
    kind: SnapshotChangeKind,
) -> ContractResult<SnapshotChange> {
    Ok(SnapshotChange::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
        kind,
    ))
}

pub(crate) fn run(
    id: [u8; 32],
    snapshot_id: SnapshotId,
    ranking_policy_version: u32,
) -> ContractResult<IndexRunStart> {
    Ok(IndexRunStart::new(
        IndexRunId::from_bytes(id),
        snapshot_id,
        RankingPolicyVersion::new(ranking_policy_version)?,
    ))
}

pub(crate) fn unborn_head() -> ContractResult<GitHead> {
    Ok(GitHead::Unborn {
        reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
    })
}

pub(crate) fn born_head(hex: &str) -> ContractResult<GitHead> {
    Ok(GitHead::Born {
        object_id: GitObjectId::try_from_hex(hex)?,
        reference: Some(GitReferenceName::try_from_full_name("refs/heads/main")?),
    })
}

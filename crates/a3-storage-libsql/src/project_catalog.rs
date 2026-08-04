use crate::{CatalogDatabase, CatalogOpenError};
use a3_application::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectPathDisplay, RecentProject,
    RecentProjectLimit,
};
use a3_domain::{
    GitHead, GitObjectId, GitReferenceName, ProjectId, ProjectIdentity, RepositoryId, WorktreeId,
};
use blake3::Hasher;
use libsql::{Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::path::Path;

const PROJECT_ID_VERSION: &[u8] = b"a3.catalog-project-id.v1";
const SQLITE_CONSTRAINT: i32 = 19;
const SQLITE_CORRUPT: i32 = 11;
const SQLITE_NOT_A_DATABASE: i32 = 26;

impl KnowledgeStore for CatalogDatabase {
    fn record_opened_project<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeStoreFuture<'a, ProjectId> {
        Box::pin(async move {
            self.record_project(project)
                .await
                .map_err(ProjectCatalogError::classify)
        })
    }

    fn list_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
        Box::pin(async move {
            self.read_recent_projects(limit)
                .await
                .map_err(ProjectCatalogError::classify)
        })
    }
}

impl CatalogDatabase {
    async fn record_project(
        &self,
        project: &ProjectIdentity,
    ) -> Result<ProjectId, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;

        let result = record_project_in_transaction(&transaction, project).await;
        let project_id = match result {
            Ok(project_id) => project_id,
            Err(error) => {
                return match transaction.rollback().await {
                    Ok(()) => Err(error),
                    Err(source) => Err(ProjectCatalogError::Rollback(source)),
                };
            }
        };

        transaction
            .commit()
            .await
            .map_err(ProjectCatalogError::Commit)?;
        Ok(project_id)
    }

    async fn read_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> Result<Vec<RecentProject>, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let mut rows = connection
            .query(
                "SELECT recent.project_id, recent.repository_id, recent.worktree_id,\n\
                 recent.worktree_root_display, recent.head_kind, recent.head_object_id,\n\
                 recent.head_reference, projects.repository_id\n\
                 FROM recent_worktrees AS recent\n\
                 LEFT JOIN projects ON projects.project_id = recent.project_id\n\
                 ORDER BY recent.last_open_sequence DESC\n\
                 LIMIT ?1",
                [i64::from(limit.get())],
            )
            .await
            .map_err(ProjectCatalogError::Read)?;
        let mut projects = Vec::with_capacity(usize::from(limit.get()));
        while let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? {
            projects.push(recent_project_from_row(&row)?);
        }
        Ok(projects)
    }
}

async fn record_project_in_transaction(
    transaction: &Transaction,
    project: &ProjectIdentity,
) -> Result<ProjectId, ProjectCatalogError> {
    let sequence = next_open_sequence(transaction).await?;
    let repository_id = project.repository().id();
    let project_id = match worktree_ownership(transaction, project.worktree().id()).await? {
        Some((stored_project_id, stored_repository_id)) => {
            if stored_repository_id != repository_id {
                return Err(ProjectCatalogError::IdentityConflict);
            }
            stored_project_id
        }
        None => match project_for_repository(transaction, repository_id).await? {
            Some(existing) => existing,
            None => {
                let created = derive_project_id(repository_id);
                insert_project(transaction, created, project, sequence).await?;
                created
            }
        },
    };

    update_project(transaction, project_id, project, sequence).await?;
    upsert_worktree(transaction, project_id, project, sequence).await?;
    Ok(project_id)
}

async fn next_open_sequence(transaction: &Transaction) -> Result<i64, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(last_open_sequence), 0) FROM (\n\
             SELECT last_open_sequence FROM projects\n\
             UNION ALL\n\
             SELECT last_open_sequence FROM recent_worktrees\n\
             )",
            (),
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let current: i64 = row.get(0).map_err(ProjectCatalogError::Read)?;
    current
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or(ProjectCatalogError::SequenceExhausted)
}

async fn worktree_ownership(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<(ProjectId, RepositoryId)>, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT project_id, repository_id FROM recent_worktrees WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(None);
    };
    let project_id = ProjectId::from_bytes(read_stable_id(&row, 0)?);
    let repository_id = RepositoryId::from_bytes(read_stable_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(Some((project_id, repository_id)))
}

async fn project_for_repository(
    transaction: &Transaction,
    repository_id: RepositoryId,
) -> Result<Option<ProjectId>, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT project_id FROM projects WHERE repository_id = ?1",
            [repository_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(None);
    };
    let project_id = ProjectId::from_bytes(read_stable_id(&row, 0)?);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(Some(project_id))
}

async fn insert_project(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let common_directory = encode_path(project.repository().common_directory().as_path());
    let remote = project
        .repository()
        .main_remote()
        .map(|identity| identity.as_bytes().to_vec());
    transaction
        .execute(
            "INSERT INTO projects (\n\
             project_id, repository_id, repository_common_directory, repository_path_encoding,\n\
             main_remote_id, created_open_sequence, last_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                common_directory.bytes,
                common_directory.encoding,
                remote,
                sequence,
                sequence
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    Ok(())
}

async fn update_project(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let common_directory = encode_path(project.repository().common_directory().as_path());
    let remote = project
        .repository()
        .main_remote()
        .map(|identity| identity.as_bytes().to_vec());
    let affected = transaction
        .execute(
            "UPDATE projects SET\n\
             repository_common_directory = ?1, repository_path_encoding = ?2,\n\
             main_remote_id = ?3, last_open_sequence = ?4\n\
             WHERE project_id = ?5 AND repository_id = ?6",
            params![
                common_directory.bytes,
                common_directory.encoding,
                remote,
                sequence,
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if affected != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn upsert_worktree(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let root = encode_path(project.worktree().root().as_path());
    let display = ProjectPathDisplay::from_path(project.worktree().root().as_path());
    let head = HeadFields::from(project.head());
    transaction
        .execute(
            "INSERT INTO recent_worktrees (\n\
             worktree_id, project_id, repository_id, worktree_root, worktree_path_encoding,\n\
             worktree_root_display, head_kind, head_object_id, head_reference, last_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)\n\
             ON CONFLICT(worktree_id) DO UPDATE SET\n\
             project_id = excluded.project_id, repository_id = excluded.repository_id,\n\
             worktree_root = excluded.worktree_root,\n\
             worktree_path_encoding = excluded.worktree_path_encoding,\n\
             worktree_root_display = excluded.worktree_root_display,\n\
             head_kind = excluded.head_kind, head_object_id = excluded.head_object_id,\n\
             head_reference = excluded.head_reference,\n\
             last_open_sequence = excluded.last_open_sequence",
            params![
                project.worktree().id().as_bytes().to_vec(),
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                root.bytes,
                root.encoding,
                display.as_str(),
                head.kind,
                head.object_id,
                head.reference,
                sequence
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    Ok(())
}

fn recent_project_from_row(row: &libsql::Row) -> Result<RecentProject, ProjectCatalogError> {
    let project_id = ProjectId::from_bytes(read_stable_id(row, 0)?);
    let repository_id = RepositoryId::from_bytes(read_stable_id(row, 1)?);
    let project_repository_id = RepositoryId::from_bytes(read_optional_stable_id(row, 7)?);
    if repository_id != project_repository_id {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    let worktree_id = WorktreeId::from_bytes(read_stable_id(row, 2)?);
    let display: String = row.get(3).map_err(ProjectCatalogError::Read)?;
    let display = ProjectPathDisplay::try_from_stored(display)
        .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
    let kind: String = row.get(4).map_err(ProjectCatalogError::Read)?;
    let object_id: Option<String> = row.get(5).map_err(ProjectCatalogError::Read)?;
    let reference: Option<String> = row.get(6).map_err(ProjectCatalogError::Read)?;
    let head = parse_head(&kind, object_id, reference)?;
    Ok(RecentProject::new(
        project_id,
        repository_id,
        worktree_id,
        display,
        head,
    ))
}

fn parse_head(
    kind: &str,
    object_id: Option<String>,
    reference: Option<String>,
) -> Result<GitHead, ProjectCatalogError> {
    match (kind, object_id, reference) {
        ("born", Some(object_id), reference) => Ok(GitHead::Born {
            object_id: GitObjectId::try_from_hex(object_id)
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
            reference: reference
                .map(GitReferenceName::try_from_full_name)
                .transpose()
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
        }),
        ("unborn", None, Some(reference)) => Ok(GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name(reference)
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
        }),
        _ => Err(ProjectCatalogError::InvalidStoredData),
    }
}

fn read_stable_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ProjectCatalogError> {
    let bytes: Vec<u8> = row.get(index).map_err(ProjectCatalogError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ProjectCatalogError::InvalidStoredData)
}

fn read_optional_stable_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ProjectCatalogError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(ProjectCatalogError::Read)?;
    bytes
        .ok_or(ProjectCatalogError::InvalidStoredData)?
        .try_into()
        .map_err(|_| ProjectCatalogError::InvalidStoredData)
}

fn derive_project_id(repository_id: RepositoryId) -> ProjectId {
    let mut hasher = Hasher::new();
    hasher.update(PROJECT_ID_VERSION);
    hasher.update(repository_id.as_bytes());
    ProjectId::from_bytes(*hasher.finalize().as_bytes())
}

struct EncodedPath {
    encoding: &'static str,
    bytes: Vec<u8>,
}

#[cfg(unix)]
fn encode_path(path: &Path) -> EncodedPath {
    use std::os::unix::ffi::OsStrExt;

    EncodedPath {
        encoding: "unix-bytes-v1",
        bytes: path.as_os_str().as_bytes().to_vec(),
    }
}

#[cfg(windows)]
fn encode_path(path: &Path) -> EncodedPath {
    use std::os::windows::ffi::OsStrExt;

    EncodedPath {
        encoding: "windows-utf16le-v1",
        bytes: path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect(),
    }
}

#[cfg(not(any(unix, windows)))]
fn encode_path(path: &Path) -> EncodedPath {
    EncodedPath {
        encoding: "utf8-lossy-v1",
        bytes: path.to_string_lossy().into_owned().into_bytes(),
    }
}

struct HeadFields {
    kind: &'static str,
    object_id: Option<String>,
    reference: Option<String>,
}

impl From<&GitHead> for HeadFields {
    fn from(head: &GitHead) -> Self {
        match head {
            GitHead::Born {
                object_id,
                reference,
            } => Self {
                kind: "born",
                object_id: Some(object_id.as_str().to_owned()),
                reference: reference
                    .as_ref()
                    .map(|reference| reference.as_str().to_owned()),
            },
            GitHead::Unborn { reference } => Self {
                kind: "unborn",
                object_id: None,
                reference: Some(reference.as_str().to_owned()),
            },
        }
    }
}

#[derive(Debug)]
enum ProjectCatalogError {
    Open(CatalogOpenError),
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Rollback(libsql::Error),
    Commit(libsql::Error),
    InvalidStoredData,
    IdentityConflict,
    SequenceExhausted,
}

impl ProjectCatalogError {
    fn classify(self) -> KnowledgeStoreFailure {
        match self {
            Self::Open(
                CatalogOpenError::CorruptDatabase | CatalogOpenError::IntegrityCheckFailed,
            ) => KnowledgeStoreFailure::Corrupt,
            Self::Read(ref source) | Self::Write(ref source) if is_corruption(source) => {
                KnowledgeStoreFailure::Corrupt
            }
            Self::Open(CatalogOpenError::NewerSchema { .. }) => {
                KnowledgeStoreFailure::UnsupportedSchema
            }
            Self::Open(
                CatalogOpenError::MigrationHistoryMismatch { .. }
                | CatalogOpenError::UnexpectedSchemaVersion { .. }
                | CatalogOpenError::ConnectionPolicyMismatch,
            )
            | Self::InvalidStoredData => KnowledgeStoreFailure::InvalidStoredData,
            Self::IdentityConflict => KnowledgeStoreFailure::IdentityConflict,
            Self::Write(ref source) if sqlite_primary_code(source) == Some(SQLITE_CONSTRAINT) => {
                KnowledgeStoreFailure::IdentityConflict
            }
            Self::Open(_)
            | Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Rollback(_)
            | Self::Commit(_)
            | Self::SequenceExhausted => KnowledgeStoreFailure::Unavailable,
        }
    }
}

impl fmt::Display for ProjectCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(_) => formatter.write_str("could not open a catalog operation connection"),
            Self::Begin(_) => formatter.write_str("could not begin a catalog write transaction"),
            Self::Read(_) => formatter.write_str("could not read project catalog data"),
            Self::Write(_) => formatter.write_str("could not write project catalog data"),
            Self::Rollback(_) => formatter.write_str("could not roll back project catalog data"),
            Self::Commit(_) => formatter.write_str("could not commit project catalog data"),
            Self::InvalidStoredData => formatter.write_str("project catalog data is invalid"),
            Self::IdentityConflict => formatter.write_str("project catalog identity conflicts"),
            Self::SequenceExhausted => formatter.write_str("project open sequence is exhausted"),
        }
    }
}

impl Error for ProjectCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Open(source) => Some(source),
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Rollback(source)
            | Self::Commit(source) => Some(source),
            Self::InvalidStoredData | Self::IdentityConflict | Self::SequenceExhausted => None,
        }
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn is_corruption(error: &libsql::Error) -> bool {
    matches!(
        sqlite_primary_code(error),
        Some(SQLITE_CORRUPT | SQLITE_NOT_A_DATABASE)
    )
}

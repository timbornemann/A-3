use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, ModuleRemapQueueFailure, PendingRemapQueue, RemapQueueControl,
    RemapQueueLimit,
};
use a3_domain::{
    IndexRunId, InvalidationReason, ModuleCardId, ModuleId, RemapPriority, RemapRequest,
    SnapshotId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_REMAP_QUEUE_READ_DURATION: Duration = Duration::from_secs(2);

pub(crate) async fn load_pending(
    connection: &Connection,
    worktree_id: WorktreeId,
    limit: RemapQueueLimit,
    control: &dyn RemapQueueControl,
) -> Result<PendingRemapQueue, ModuleRemapQueueRepositoryError> {
    let guard = QueueReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleRemapQueueRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let (target_run_id, target_snapshot_id) =
            latest_publication(&transaction, worktree_id).await?;
        let query_limit = i64::from(limit.get())
            .checked_add(1)
            .ok_or(ModuleRemapQueueRepositoryError::ResourceLimit)?;
        let mut rows = transaction
            .query(
                "SELECT source_index_run_id, card_id, module_id, target_index_run_id,\n\
                 target_snapshot_id, priority, reason\n\
                 FROM module_remap_queue ORDER BY priority, module_id LIMIT ?1",
                [query_limit],
            )
            .await
            .map_err(ModuleRemapQueueRepositoryError::Read)?;
        let mut entries = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(ModuleRemapQueueRepositoryError::Read)?
        {
            if entries.len().is_multiple_of(32) {
                guard.checkpoint()?;
            }
            let priority = match read_i64(&row, 5)? {
                0 => RemapPriority::Direct,
                1 => RemapPriority::Dependent,
                _ => return Err(ModuleRemapQueueRepositoryError::InvalidStoredProjection),
            };
            let reason = match read_text(&row, 6)?.as_str() {
                "evidence-changed" => InvalidationReason::EvidenceChanged,
                "parser-version-changed" => InvalidationReason::ParserVersionChanged,
                "mapper-version-changed" => InvalidationReason::MapperVersionChanged,
                "direct-dependency-changed" => InvalidationReason::DirectDependencyChanged,
                _ => return Err(ModuleRemapQueueRepositoryError::InvalidStoredProjection),
            };
            let request = RemapRequest::from_persisted(
                IndexRunId::from_bytes(read_id(&row, 0)?),
                ModuleCardId::from_bytes(read_id(&row, 1)?),
                ModuleId::from_bytes(read_id(&row, 2)?),
                IndexRunId::from_bytes(read_id(&row, 3)?),
                SnapshotId::from_bytes(read_id(&row, 4)?),
                priority,
                reason,
            )
            .map_err(|_| ModuleRemapQueueRepositoryError::InvalidStoredProjection)?;
            if request.target_index_run_id() != target_run_id
                || request.target_snapshot_id() != target_snapshot_id
            {
                return Err(ModuleRemapQueueRepositoryError::InvalidStoredProjection);
            }
            entries.push(request);
        }
        let truncated = entries.len() > usize::from(limit.get());
        if truncated {
            entries.pop();
        }
        guard.checkpoint()?;
        PendingRemapQueue::new(target_run_id, target_snapshot_id, entries, truncated)
            .map_err(|_| ModuleRemapQueueRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(queue) => {
            transaction
                .commit()
                .await
                .map_err(ModuleRemapQueueRepositoryError::Commit)?;
            Ok(queue)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<(IndexRunId, SnapshotId), ModuleRemapQueueRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleRemapQueueRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleRemapQueueRepositoryError::Read)?
    else {
        return Err(ModuleRemapQueueRepositoryError::IndexUnavailable);
    };
    let target = (
        IndexRunId::from_bytes(read_id(&row, 0)?),
        SnapshotId::from_bytes(read_id(&row, 1)?),
    );
    if rows
        .next()
        .await
        .map_err(ModuleRemapQueueRepositoryError::Read)?
        .is_some()
    {
        return Err(ModuleRemapQueueRepositoryError::InvalidStoredProjection);
    }
    Ok(target)
}

struct QueueReadGuard<'a> {
    control: &'a dyn RemapQueueControl,
    started_at: Instant,
}

impl<'a> QueueReadGuard<'a> {
    fn new(control: &'a dyn RemapQueueControl) -> Result<Self, ModuleRemapQueueRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleRemapQueueRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleRemapQueueRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_REMAP_QUEUE_READ_DURATION {
            return Err(ModuleRemapQueueRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleRemapQueueRepositoryError,
) -> Result<T, ModuleRemapQueueRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleRemapQueueRepositoryError::Rollback(source)),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleRemapQueueRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(ModuleRemapQueueRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ModuleRemapQueueRepositoryError::InvalidStoredProjection)
}

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, ModuleRemapQueueRepositoryError> {
    row.get(index)
        .map_err(ModuleRemapQueueRepositoryError::Read)
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, ModuleRemapQueueRepositoryError> {
    row.get(index)
        .map_err(ModuleRemapQueueRepositoryError::Read)
}

#[derive(Debug)]
pub(crate) enum ModuleRemapQueueRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    IndexUnavailable,
    InvalidStoredProjection,
    ResourceLimit,
    Cancelled,
    TimedOut,
}

impl ModuleRemapQueueRepositoryError {
    pub(crate) fn classify(&self) -> ModuleRemapQueueFailure {
        match self {
            Self::IndexUnavailable => ModuleRemapQueueFailure::IndexUnavailable,
            Self::InvalidStoredProjection | Self::ResourceLimit => {
                ModuleRemapQueueFailure::InvalidStoredProjection
            }
            Self::Cancelled => ModuleRemapQueueFailure::Cancelled,
            Self::TimedOut => ModuleRemapQueueFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleRemapQueueFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleRemapQueueFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleRemapQueueRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Begin(_) => "could not begin remap queue read",
            Self::Read(_) => "could not read remap queue",
            Self::Commit(_) => "could not commit remap queue read",
            Self::Rollback(_) => "could not roll back remap queue read",
            Self::IndexUnavailable => "published index is unavailable",
            Self::InvalidStoredProjection => "stored remap queue projection is invalid",
            Self::ResourceLimit => "remap queue read exceeded a fixed bound",
            Self::Cancelled => "remap queue read was cancelled",
            Self::TimedOut => "remap queue read timed out",
        };
        formatter.write_str(message)
    }
}

impl Error for ModuleRemapQueueRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::IndexUnavailable
            | Self::InvalidStoredProjection
            | Self::ResourceLimit
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}

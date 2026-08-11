use crate::catalog::is_corruption;
use a3_application::{
    KnowledgeStoreFailure, ModuleCardFreshness, ModuleCardFreshnessControl,
    ModuleCardFreshnessFailure, ModuleCardFreshnessReasonCount, ModuleCardFreshnessStatus,
};
use a3_domain::{IndexRunId, InvalidationReason, SnapshotId, WorktreeId};
use libsql::{Connection, Transaction, TransactionBehavior};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_FRESHNESS_READ_DURATION: Duration = Duration::from_secs(2);

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    control: &dyn ModuleCardFreshnessControl,
) -> Result<Option<ModuleCardFreshness>, ModuleCardFreshnessRepositoryError> {
    let guard = FreshnessReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleCardFreshnessRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some((index_run_id, snapshot_id)) =
            latest_publication(&transaction, worktree_id).await?
        else {
            return Ok(None);
        };
        let mut rows = transaction
            .query(
                "WITH ranked_cards AS (\n\
                   SELECT card.source_index_run_id, card.card_id,\n\
                     ROW_NUMBER() OVER (\n\
                       PARTITION BY card.module_id\n\
                       ORDER BY source_run.run_sequence DESC, card.card_id DESC\n\
                     ) AS card_rank\n\
                   FROM module_cards card\n\
                   JOIN index_runs source_run\n\
                     ON source_run.index_run_id = card.source_index_run_id\n\
                   WHERE source_run.worktree_id = ?1\n\
                     AND source_run.status = 'published'\n\
                     AND card.snapshot_id = source_run.snapshot_id\n\
                 ), latest_cards AS (\n\
                   SELECT source_index_run_id, card_id FROM ranked_cards WHERE card_rank = 1\n\
                 )\n\
                 SELECT lifecycle.status, lifecycle.reason, COUNT(*)\n\
                 FROM latest_cards card\n\
                 LEFT JOIN module_card_lifecycle lifecycle\n\
                   ON lifecycle.source_index_run_id = card.source_index_run_id\n\
                  AND lifecycle.card_id = card.card_id\n\
                 GROUP BY lifecycle.status, lifecycle.reason\n\
                 ORDER BY CASE lifecycle.status\n\
                   WHEN 'published' THEN 0 WHEN 'stale' THEN 1 WHEN 'needs-review' THEN 2\n\
                   ELSE 3 END,\n\
                   CASE lifecycle.reason\n\
                     WHEN 'evidence-changed' THEN 0 WHEN 'module-removed' THEN 1\n\
                     WHEN 'parser-version-changed' THEN 2\n\
                     WHEN 'mapper-version-changed' THEN 3\n\
                     WHEN 'direct-dependency-changed' THEN 4 ELSE 5 END",
                [worktree_id.as_bytes().to_vec()],
            )
            .await
            .map_err(ModuleCardFreshnessRepositoryError::Read)?;
        let mut published_count = 0_u64;
        let mut stale_count = 0_u64;
        let mut needs_review_count = 0_u64;
        let mut reason_counts = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(ModuleCardFreshnessRepositoryError::Read)?
        {
            guard.checkpoint()?;
            let status = read_optional_text(&row, 0)?;
            let reason = read_optional_text(&row, 1)?;
            let count = read_count(&row, 2)?;
            match (status.as_deref(), reason.as_deref()) {
                (Some("published"), None) => {
                    published_count = published_count
                        .checked_add(count)
                        .ok_or(ModuleCardFreshnessRepositoryError::InvalidStoredProjection)?;
                }
                (Some("stale"), Some(reason)) => {
                    stale_count = stale_count
                        .checked_add(count)
                        .ok_or(ModuleCardFreshnessRepositoryError::InvalidStoredProjection)?;
                    reason_counts.push(reason_count(
                        ModuleCardFreshnessStatus::Stale,
                        reason,
                        count,
                    )?);
                }
                (Some("needs-review"), Some(reason)) => {
                    needs_review_count = needs_review_count
                        .checked_add(count)
                        .ok_or(ModuleCardFreshnessRepositoryError::InvalidStoredProjection)?;
                    reason_counts.push(reason_count(
                        ModuleCardFreshnessStatus::NeedsReview,
                        reason,
                        count,
                    )?);
                }
                _ => return Err(ModuleCardFreshnessRepositoryError::InvalidStoredProjection),
            }
        }
        guard.checkpoint()?;
        ModuleCardFreshness::new(
            index_run_id,
            snapshot_id,
            published_count,
            stale_count,
            needs_review_count,
            reason_counts,
        )
        .map(Some)
        .map_err(|_| ModuleCardFreshnessRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(freshness) => {
            transaction
                .commit()
                .await
                .map_err(ModuleCardFreshnessRepositoryError::Commit)?;
            Ok(freshness)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<(IndexRunId, SnapshotId)>, ModuleCardFreshnessRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleCardFreshnessRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardFreshnessRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let target = (
        IndexRunId::from_bytes(read_id(&row, 0)?),
        SnapshotId::from_bytes(read_id(&row, 1)?),
    );
    if rows
        .next()
        .await
        .map_err(ModuleCardFreshnessRepositoryError::Read)?
        .is_some()
    {
        return Err(ModuleCardFreshnessRepositoryError::InvalidStoredProjection);
    }
    Ok(Some(target))
}

fn reason_count(
    status: ModuleCardFreshnessStatus,
    reason: &str,
    count: u64,
) -> Result<ModuleCardFreshnessReasonCount, ModuleCardFreshnessRepositoryError> {
    let reason = match reason {
        "evidence-changed" => InvalidationReason::EvidenceChanged,
        "module-removed" => InvalidationReason::ModuleRemoved,
        "parser-version-changed" => InvalidationReason::ParserVersionChanged,
        "mapper-version-changed" => InvalidationReason::MapperVersionChanged,
        "direct-dependency-changed" => InvalidationReason::DirectDependencyChanged,
        _ => return Err(ModuleCardFreshnessRepositoryError::InvalidStoredProjection),
    };
    ModuleCardFreshnessReasonCount::new(status, reason, count)
        .map_err(|_| ModuleCardFreshnessRepositoryError::InvalidStoredProjection)
}

struct FreshnessReadGuard<'a> {
    control: &'a dyn ModuleCardFreshnessControl,
    started_at: Instant,
}

impl<'a> FreshnessReadGuard<'a> {
    fn new(
        control: &'a dyn ModuleCardFreshnessControl,
    ) -> Result<Self, ModuleCardFreshnessRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleCardFreshnessRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleCardFreshnessRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_FRESHNESS_READ_DURATION {
            return Err(ModuleCardFreshnessRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleCardFreshnessRepositoryError,
) -> Result<T, ModuleCardFreshnessRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleCardFreshnessRepositoryError::Rollback(source)),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleCardFreshnessRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(ModuleCardFreshnessRepositoryError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ModuleCardFreshnessRepositoryError::InvalidStoredProjection)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, ModuleCardFreshnessRepositoryError> {
    row.get(index)
        .map_err(ModuleCardFreshnessRepositoryError::Read)
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ModuleCardFreshnessRepositoryError> {
    let count: i64 = row
        .get(index)
        .map_err(ModuleCardFreshnessRepositoryError::Read)?;
    u64::try_from(count).map_err(|_| ModuleCardFreshnessRepositoryError::InvalidStoredProjection)
}

#[derive(Debug)]
pub(crate) enum ModuleCardFreshnessRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl ModuleCardFreshnessRepositoryError {
    pub(crate) fn classify(&self) -> ModuleCardFreshnessFailure {
        match self {
            Self::InvalidStoredProjection => ModuleCardFreshnessFailure::InvalidStoredProjection,
            Self::Cancelled => ModuleCardFreshnessFailure::Cancelled,
            Self::TimedOut => ModuleCardFreshnessFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleCardFreshnessFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleCardFreshnessFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleCardFreshnessRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin module-card freshness read",
            Self::Read(_) => "could not read module-card freshness",
            Self::Commit(_) => "could not commit module-card freshness read",
            Self::Rollback(_) => "could not roll back module-card freshness read",
            Self::InvalidStoredProjection => "stored module-card freshness projection is invalid",
            Self::Cancelled => "module-card freshness read was cancelled",
            Self::TimedOut => "module-card freshness read timed out",
        })
    }
}

impl Error for ModuleCardFreshnessRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::InvalidStoredProjection | Self::Cancelled | Self::TimedOut => None,
        }
    }
}

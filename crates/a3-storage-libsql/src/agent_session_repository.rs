use crate::catalog::is_corruption;
use a3_application::{
    AgentSessionDetail, AgentSessionListQuery, AgentSessionPage, AgentSessionStoreFailure,
};
use a3_domain::{
    AgentSession, AgentSessionEntry, AgentSessionEntryKind, AgentSessionId, AgentSessionMode,
    AgentSessionRevision, AgentSessionSequence, AgentSessionState, AgentSessionText,
    AgentSessionTimestamp, AgentSessionTitle, AgentWorkItem, AgentWorkItemId, TaskId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};

pub(crate) async fn create(
    connection: &Connection,
    worktree_id: WorktreeId,
    session: &AgentSession,
    first_entry: Option<&AgentSessionEntry>,
) -> Result<(), AgentSessionRepositoryError> {
    if session.revision() != AgentSessionRevision::INITIAL
        || session.presentation_deleted()
        || first_entry.is_some_and(|entry| {
            entry.session_id() != session.id()
                || entry.sequence() != AgentSessionSequence::FIRST
                || session.latest_sequence() != Some(AgentSessionSequence::FIRST)
        })
        || (first_entry.is_none() && session.latest_sequence().is_some())
    {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        insert_revision(&transaction, worktree_id, session).await?;
        if let Some(entry) = first_entry {
            insert_entry(&transaction, worktree_id, session.revision(), entry).await?;
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn append(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected: AgentSessionRevision,
    session: &AgentSession,
    entry: Option<&AgentSessionEntry>,
) -> Result<(), AgentSessionRepositoryError> {
    if session.revision().get() != expected.get().saturating_add(1)
        || entry.is_some_and(|entry| {
            entry.session_id() != session.id()
                || session.latest_sequence() != Some(entry.sequence())
        })
    {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        require_latest_revision(&transaction, worktree_id, session.id(), expected).await?;
        insert_revision(&transaction, worktree_id, session).await?;
        if let Some(entry) = entry {
            insert_entry(&transaction, worktree_id, session.revision(), entry).await?;
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn delete_presentation(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    expected: AgentSessionRevision,
    tombstone: &AgentSession,
) -> Result<(), AgentSessionRepositoryError> {
    if tombstone.id() != session_id
        || !tombstone.presentation_deleted()
        || tombstone.revision().get() != expected.get().saturating_add(1)
        || tombstone.latest_sequence().is_some()
    {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        require_latest_revision(&transaction, worktree_id, session_id, expected).await?;
        insert_revision(&transaction, worktree_id, tombstone).await?;
        transaction
            .execute(
                "DELETE FROM agent_session_entries WHERE worktree_id = ?1 AND session_id = ?2",
                params![
                    worktree_id.as_bytes().to_vec(),
                    session_id.as_bytes().to_vec()
                ],
            )
            .await
            .map_err(AgentSessionRepositoryError::Write)?;
        Ok(())
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn list(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &AgentSessionListQuery,
) -> Result<AgentSessionPage, AgentSessionRepositoryError> {
    let fetch_limit = i64::from(query.limit()) + 1;
    let cursor = query
        .before_updated_at_unix_millis()
        .map(i64::try_from)
        .transpose()
        .map_err(|_| AgentSessionRepositoryError::InvalidInput)?;
    let search = query.search().unwrap_or("");
    let mut rows = connection
        .query(
            "WITH latest AS (
               SELECT session_id, MAX(revision) AS revision
               FROM agent_session_revisions WHERE worktree_id = ?1 GROUP BY session_id
             )
             SELECT r.session_id, r.revision, r.title, r.mode, r.state,
               r.created_at_unix_millis, r.updated_at_unix_millis, r.latest_sequence,
               r.active_work_item_id, r.active_task_id, r.active_work_item_mode,
               r.current_plan_revision, r.presentation_deleted
             FROM agent_session_revisions r
             JOIN latest l ON l.session_id = r.session_id AND l.revision = r.revision
             WHERE r.worktree_id = ?1 AND r.presentation_deleted = 0
               AND (?2 = 1 OR r.state <> 'archived')
               AND (?3 IS NULL OR r.updated_at_unix_millis < ?3)
               AND (?4 = '' OR instr(lower(r.title), lower(?4)) > 0)
             ORDER BY r.updated_at_unix_millis DESC, r.session_id ASC LIMIT ?5",
            params![
                worktree_id.as_bytes().to_vec(),
                i64::from(query.include_archived()),
                cursor,
                search,
                fetch_limit
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let mut sessions = Vec::with_capacity(usize::from(query.limit()) + 1);
    while let Some(row) = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        sessions.push(decode_session(&row)?);
    }
    let has_more = sessions.len() > usize::from(query.limit());
    sessions.truncate(usize::from(query.limit()));
    AgentSessionPage::new(sessions, has_more)
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    before_sequence: Option<u64>,
    limit: u16,
) -> Result<Option<AgentSessionDetail>, AgentSessionRepositoryError> {
    if limit == 0 || limit > 128 {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let mut rows = connection
        .query(
            "SELECT session_id, revision, title, mode, state,
             created_at_unix_millis, updated_at_unix_millis, latest_sequence,
             active_work_item_id, active_task_id, active_work_item_mode,
             current_plan_revision, presentation_deleted
             FROM agent_session_revisions
             WHERE worktree_id = ?1 AND session_id = ?2
             ORDER BY revision DESC LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let session = decode_session(&row)?;
    if session.presentation_deleted() {
        return Ok(None);
    }
    let before = before_sequence
        .map(i64::try_from)
        .transpose()
        .map_err(|_| AgentSessionRepositoryError::InvalidInput)?;
    let mut entry_rows = connection
        .query(
            "SELECT sequence, kind, content, created_at_unix_millis,
             work_item_id, task_id, plan_revision
             FROM agent_session_entries
             WHERE worktree_id = ?1 AND session_id = ?2
               AND (?3 IS NULL OR sequence < ?3)
             ORDER BY sequence DESC LIMIT ?4",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                before,
                i64::from(limit) + 1
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let mut entries = Vec::with_capacity(usize::from(limit) + 1);
    while let Some(row) = entry_rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        entries.push(decode_entry(session_id, &row)?);
    }
    let has_older = entries.len() > usize::from(limit);
    entries.truncate(usize::from(limit));
    entries.reverse();
    AgentSessionDetail::new(session, entries, has_older)
        .map(Some)
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

async fn require_latest_revision(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    expected: AgentSessionRevision,
) -> Result<(), AgentSessionRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT revision FROM agent_session_revisions
             WHERE worktree_id = ?1 AND session_id = ?2 ORDER BY revision DESC LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let current = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
        .map(|row| read_u64(&row, 0))
        .transpose()?;
    if current != Some(expected.get()) {
        return Err(AgentSessionRepositoryError::Conflict);
    }
    Ok(())
}

async fn insert_revision(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session: &AgentSession,
) -> Result<(), AgentSessionRepositoryError> {
    let work_item = session.active_work_item();
    transaction
        .execute(
            "INSERT INTO agent_session_revisions (
             worktree_id, session_id, revision, title, mode, state,
             created_at_unix_millis, updated_at_unix_millis, latest_sequence,
             active_work_item_id, active_task_id, active_work_item_mode,
             current_plan_revision, presentation_deleted
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                worktree_id.as_bytes().to_vec(),
                session.id().as_bytes().to_vec(),
                u64_to_i64(session.revision().get())?,
                session.title().as_str(),
                encode_mode(session.mode()),
                encode_state(session.state()),
                u64_to_i64(session.created_at().unix_millis())?,
                u64_to_i64(session.updated_at().unix_millis())?,
                session
                    .latest_sequence()
                    .map(|value| u64_to_i64(value.get()))
                    .transpose()?,
                work_item.map(|value| value.id().as_bytes().to_vec()),
                work_item.map(|value| value.task_id().as_bytes().to_vec()),
                work_item.map(|value| encode_mode(value.mode())),
                session.current_plan_revision().map(i64::from),
                i64::from(session.presentation_deleted())
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Write)?;
    Ok(())
}

async fn insert_entry(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_revision: AgentSessionRevision,
    entry: &AgentSessionEntry,
) -> Result<(), AgentSessionRepositoryError> {
    transaction
        .execute(
            "INSERT INTO agent_session_entries (
             worktree_id, session_id, session_revision, sequence, kind, content,
             created_at_unix_millis, work_item_id, task_id, plan_revision
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                worktree_id.as_bytes().to_vec(),
                entry.session_id().as_bytes().to_vec(),
                u64_to_i64(session_revision.get())?,
                u64_to_i64(entry.sequence().get())?,
                encode_kind(entry.kind()),
                entry.text().as_str(),
                u64_to_i64(entry.created_at().unix_millis())?,
                entry.work_item_id().map(|value| value.as_bytes().to_vec()),
                entry.task_id().map(|value| value.as_bytes().to_vec()),
                entry.plan_revision().map(i64::from)
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Write)?;
    Ok(())
}

fn decode_session(row: &libsql::Row) -> Result<AgentSession, AgentSessionRepositoryError> {
    let work_item_id = read_optional_id(row, 8)?.map(AgentWorkItemId::from_bytes);
    let task_id = read_optional_id(row, 9)?.map(TaskId::from_bytes);
    let work_item_mode = read_optional_string(row, 10)?
        .map(|value| decode_mode(&value))
        .transpose()?;
    let active_work_item = match (work_item_id, task_id, work_item_mode) {
        (Some(id), Some(task), Some(mode)) => Some(AgentWorkItem::new(id, task, mode)),
        (None, None, None) => None,
        _ => return Err(AgentSessionRepositoryError::InvalidStoredData),
    };
    Ok(AgentSession::from_parts(
        AgentSessionId::from_bytes(read_id(row, 0)?),
        AgentSessionRevision::new(read_u64(row, 1)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        AgentSessionTitle::try_from_string(read_string(row, 2)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        decode_mode(&read_string(row, 3)?)?,
        decode_state(&read_string(row, 4)?)?,
        AgentSessionTimestamp::from_unix_millis(read_u64(row, 5)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        AgentSessionTimestamp::from_unix_millis(read_u64(row, 6)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        read_optional_u64(row, 7)?
            .map(AgentSessionSequence::new)
            .transpose()
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        active_work_item,
        read_optional_u64(row, 11)?
            .map(u32::try_from)
            .transpose()
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        read_bool(row, 12)?,
    ))
}

fn decode_entry(
    session_id: AgentSessionId,
    row: &libsql::Row,
) -> Result<AgentSessionEntry, AgentSessionRepositoryError> {
    let work_item_id = read_optional_id(row, 4)?.map(AgentWorkItemId::from_bytes);
    let task_id = read_optional_id(row, 5)?.map(TaskId::from_bytes);
    if work_item_id.is_some() != task_id.is_some() {
        return Err(AgentSessionRepositoryError::InvalidStoredData);
    }
    Ok(AgentSessionEntry::new(
        session_id,
        AgentSessionSequence::new(read_u64(row, 0)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        decode_kind(&read_string(row, 1)?)?,
        AgentSessionText::try_from_string(read_string(row, 2)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        AgentSessionTimestamp::from_unix_millis(read_u64(row, 3)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
        work_item_id,
        task_id,
        read_optional_u64(row, 6)?
            .map(u32::try_from)
            .transpose()
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
    ))
}

const fn encode_mode(value: AgentSessionMode) -> &'static str {
    match value {
        AgentSessionMode::Ask => "ask",
        AgentSessionMode::Plan => "plan",
        AgentSessionMode::Agent => "agent",
    }
}

fn decode_mode(value: &str) -> Result<AgentSessionMode, AgentSessionRepositoryError> {
    match value {
        "ask" => Ok(AgentSessionMode::Ask),
        "plan" => Ok(AgentSessionMode::Plan),
        "agent" => Ok(AgentSessionMode::Agent),
        _ => Err(AgentSessionRepositoryError::InvalidStoredData),
    }
}

const fn encode_state(value: AgentSessionState) -> &'static str {
    match value {
        AgentSessionState::Draft => "draft",
        AgentSessionState::Running => "running",
        AgentSessionState::AwaitingUser => "awaiting_user",
        AgentSessionState::AwaitingPlanReview => "awaiting_plan_review",
        AgentSessionState::AwaitingApproval => "awaiting_approval",
        AgentSessionState::Paused => "paused",
        AgentSessionState::Completed => "completed",
        AgentSessionState::Failed => "failed",
        AgentSessionState::Cancelled => "cancelled",
        AgentSessionState::Archived => "archived",
    }
}

fn decode_state(value: &str) -> Result<AgentSessionState, AgentSessionRepositoryError> {
    match value {
        "draft" => Ok(AgentSessionState::Draft),
        "running" => Ok(AgentSessionState::Running),
        "awaiting_user" => Ok(AgentSessionState::AwaitingUser),
        "awaiting_plan_review" => Ok(AgentSessionState::AwaitingPlanReview),
        "awaiting_approval" => Ok(AgentSessionState::AwaitingApproval),
        "paused" => Ok(AgentSessionState::Paused),
        "completed" => Ok(AgentSessionState::Completed),
        "failed" => Ok(AgentSessionState::Failed),
        "cancelled" => Ok(AgentSessionState::Cancelled),
        "archived" => Ok(AgentSessionState::Archived),
        _ => Err(AgentSessionRepositoryError::InvalidStoredData),
    }
}

const fn encode_kind(value: AgentSessionEntryKind) -> &'static str {
    match value {
        AgentSessionEntryKind::UserMessage => "user_message",
        AgentSessionEntryKind::AssistantSummary => "assistant_summary",
        AgentSessionEntryKind::Plan => "plan",
        AgentSessionEntryKind::FinalReport => "final_report",
        AgentSessionEntryKind::Activity => "activity",
    }
}

fn decode_kind(value: &str) -> Result<AgentSessionEntryKind, AgentSessionRepositoryError> {
    match value {
        "user_message" => Ok(AgentSessionEntryKind::UserMessage),
        "assistant_summary" => Ok(AgentSessionEntryKind::AssistantSummary),
        "plan" => Ok(AgentSessionEntryKind::Plan),
        "final_report" => Ok(AgentSessionEntryKind::FinalReport),
        "activity" => Ok(AgentSessionEntryKind::Activity),
        _ => Err(AgentSessionRepositoryError::InvalidStoredData),
    }
}

fn read_string(row: &libsql::Row, index: i32) -> Result<String, AgentSessionRepositoryError> {
    row.get(index).map_err(AgentSessionRepositoryError::Read)
}

fn read_optional_string(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, AgentSessionRepositoryError> {
    row.get(index).map_err(AgentSessionRepositoryError::Read)
}

fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, AgentSessionRepositoryError> {
    let value: i64 = row.get(index).map_err(AgentSessionRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

fn read_optional_u64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, AgentSessionRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(AgentSessionRepositoryError::Read)?;
    value
        .map(u64::try_from)
        .transpose()
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], AgentSessionRepositoryError> {
    let bytes: Vec<u8> = row.get(index).map_err(AgentSessionRepositoryError::Read)?;
    <[u8; 32]>::try_from(bytes).map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, AgentSessionRepositoryError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(AgentSessionRepositoryError::Read)?;
    bytes
        .map(<[u8; 32]>::try_from)
        .transpose()
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, AgentSessionRepositoryError> {
    match read_u64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(AgentSessionRepositoryError::InvalidStoredData),
    }
}

fn u64_to_i64(value: u64) -> Result<i64, AgentSessionRepositoryError> {
    i64::try_from(value).map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, AgentSessionRepositoryError>,
) -> Result<T, AgentSessionRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(AgentSessionRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(AgentSessionRepositoryError::Rollback(source)),
        },
    }
}

#[derive(Debug)]
pub(crate) enum AgentSessionRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Conflict,
    InvalidInput,
    InvalidStoredData,
}

impl AgentSessionRepositoryError {
    pub(crate) fn classify(&self) -> AgentSessionStoreFailure {
        match self {
            Self::Conflict => AgentSessionStoreFailure::Conflict,
            Self::InvalidInput => AgentSessionStoreFailure::InvalidInput,
            Self::InvalidStoredData => AgentSessionStoreFailure::InvalidStoredData,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    AgentSessionStoreFailure::InvalidStoredData
                } else {
                    AgentSessionStoreFailure::Unavailable
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentSessionRepositoryError, append, create, delete_presentation, list, load};
    use a3_application::AgentSessionListQuery;
    use a3_domain::{
        AgentSession, AgentSessionEntry, AgentSessionEntryKind, AgentSessionId, AgentSessionMode,
        AgentSessionRevision, AgentSessionSequence, AgentSessionState, AgentSessionText,
        AgentSessionTimestamp, AgentSessionTitle,
    };

    #[test]
    fn session_history_is_append_only_revision_checked_and_presentation_deletable()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let worktree_id = a3_domain::WorktreeId::from_bytes([2; 32]);
            crate::migration::migrate_knowledge(&connection, &[1; 32], worktree_id.as_bytes())
                .await?;
            let session_id = AgentSessionId::from_bytes([3; 32]);
            let first = session(
                session_id,
                1,
                AgentSessionState::Running,
                10,
                Some(1),
                None,
                false,
            )?;
            let user = entry(
                session_id,
                1,
                AgentSessionEntryKind::UserMessage,
                "Implementiere den Agent Workspace",
                10,
                None,
            )?;
            create(&connection, worktree_id, &first, Some(&user))
                .await
                .map_err(|error| error.classify())?;

            let second = session(
                session_id,
                2,
                AgentSessionState::AwaitingPlanReview,
                11,
                Some(2),
                Some(1),
                false,
            )?;
            let plan = entry(
                session_id,
                2,
                AgentSessionEntryKind::Plan,
                "## Summary\nExakter Plan",
                11,
                Some(1),
            )?;
            append(
                &connection,
                worktree_id,
                AgentSessionRevision::INITIAL,
                &second,
                Some(&plan),
            )
            .await
            .map_err(|error| error.classify())?;
            assert!(matches!(
                append(
                    &connection,
                    worktree_id,
                    AgentSessionRevision::INITIAL,
                    &second,
                    None,
                )
                .await,
                Err(AgentSessionRepositoryError::Conflict)
            ));

            let detail = load(&connection, worktree_id, session_id, None, 1)
                .await
                .map_err(|error| error.classify())?
                .ok_or("session missing")?;
            assert_eq!(detail.session().revision().get(), 2);
            assert_eq!(detail.entries().len(), 1);
            assert_eq!(detail.entries()[0].kind(), AgentSessionEntryKind::Plan);
            assert!(detail.has_older_entries());
            let page = list(
                &connection,
                worktree_id,
                &AgentSessionListQuery::new(Some("agent".to_owned()), false, None, 10)?,
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(page.sessions().len(), 1);

            let tombstone = session(
                session_id,
                3,
                AgentSessionState::Archived,
                12,
                None,
                Some(1),
                true,
            )?;
            delete_presentation(
                &connection,
                worktree_id,
                session_id,
                AgentSessionRevision::new(2)?,
                &tombstone,
            )
            .await
            .map_err(|error| error.classify())?;
            assert!(
                load(&connection, worktree_id, session_id, None, 10)
                    .await
                    .map_err(|error| error.classify())?
                    .is_none()
            );
            assert!(
                list(
                    &connection,
                    worktree_id,
                    &AgentSessionListQuery::new(None, true, None, 10)?,
                )
                .await
                .map_err(|error| error.classify())?
                .sessions()
                .is_empty()
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    fn session(
        id: AgentSessionId,
        revision: u64,
        state: AgentSessionState,
        updated_at: u64,
        latest_sequence: Option<u64>,
        plan_revision: Option<u32>,
        deleted: bool,
    ) -> Result<AgentSession, Box<dyn std::error::Error>> {
        Ok(AgentSession::from_parts(
            id,
            AgentSessionRevision::new(revision)?,
            AgentSessionTitle::try_from_string("Agent Workspace".to_owned())?,
            AgentSessionMode::Plan,
            state,
            AgentSessionTimestamp::from_unix_millis(10)?,
            AgentSessionTimestamp::from_unix_millis(updated_at)?,
            latest_sequence.map(AgentSessionSequence::new).transpose()?,
            None,
            plan_revision,
            deleted,
        ))
    }

    fn entry(
        session_id: AgentSessionId,
        sequence: u64,
        kind: AgentSessionEntryKind,
        text: &str,
        created_at: u64,
        plan_revision: Option<u32>,
    ) -> Result<AgentSessionEntry, Box<dyn std::error::Error>> {
        Ok(AgentSessionEntry::new(
            session_id,
            AgentSessionSequence::new(sequence)?,
            kind,
            AgentSessionText::try_from_string(text.to_owned())?,
            AgentSessionTimestamp::from_unix_millis(created_at)?,
            None,
            None,
            plan_revision,
        ))
    }
}

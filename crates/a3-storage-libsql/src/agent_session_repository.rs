use crate::catalog::is_corruption;
use a3_application::{
    AgentSessionCommandPresentation, AgentSessionDetail, AgentSessionListQuery, AgentSessionPage,
    AgentSessionQueue, AgentSessionStoreFailure,
};
use a3_domain::{
    AgentQueuedMessage, AgentQueuedMessageId, AgentQueuedMessageState,
    AgentQueuedResearchSelection, AgentResearchDepth, AgentSession, AgentSessionEntry,
    AgentSessionEntryKind, AgentSessionId, AgentSessionMode, AgentSessionQueueRevision,
    AgentSessionRevision, AgentSessionSequence, AgentSessionState, AgentSessionText,
    AgentSessionTimestamp, AgentSessionTitle, AgentWorkItem, AgentWorkItemId, SlashCommand,
    SlashCommandCatalogVersion, SlashCommandEmptyInput, SlashCommandInvocation, SlashCommandLens,
    TaskId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};

pub(crate) async fn create(
    connection: &Connection,
    worktree_id: WorktreeId,
    session: &AgentSession,
    first_entry: Option<&AgentSessionEntry>,
    command: Option<&SlashCommandInvocation>,
) -> Result<(), AgentSessionRepositoryError> {
    if session.revision() != AgentSessionRevision::INITIAL
        || session.presentation_deleted()
        || first_entry.is_some_and(|entry| {
            entry.session_id() != session.id()
                || entry.sequence() != AgentSessionSequence::FIRST
                || session.latest_sequence() != Some(AgentSessionSequence::FIRST)
        })
        || (first_entry.is_none() && session.latest_sequence().is_some())
        || command.is_some_and(|_| {
            first_entry.is_none_or(|entry| entry.kind() != AgentSessionEntryKind::UserMessage)
        })
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
            if let Some(command) = command {
                insert_slash_command(&transaction, worktree_id, entry, command).await?;
            }
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
    command: Option<&SlashCommandInvocation>,
) -> Result<(), AgentSessionRepositoryError> {
    if session.revision().get() != expected.get().saturating_add(1)
        || entry.is_some_and(|entry| {
            entry.session_id() != session.id()
                || session.latest_sequence() != Some(entry.sequence())
        })
        || command.is_some_and(|_| {
            entry.is_none_or(|entry| entry.kind() != AgentSessionEntryKind::UserMessage)
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
            if let Some(command) = command {
                insert_slash_command(&transaction, worktree_id, entry, command).await?;
            }
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

async fn insert_slash_command(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    entry: &AgentSessionEntry,
    command: &SlashCommandInvocation,
) -> Result<(), AgentSessionRepositoryError> {
    let subject_behavior = if command.subject().is_empty() {
        match command.empty_input_behavior() {
            SlashCommandEmptyInput::RepositoryWide => "repository_wide",
            SlashCommandEmptyInput::WorkingChanges => "working_changes",
            SlashCommandEmptyInput::Clarify => "clarify",
            SlashCommandEmptyInput::Reject => {
                return Err(AgentSessionRepositoryError::InvalidInput);
            }
        }
    } else {
        "provided"
    };
    let depth = match command.depth() {
        AgentResearchDepth::Standard => "standard",
        AgentResearchDepth::Thorough => "thorough",
    };
    transaction
        .execute(
            "INSERT INTO agent_slash_command_invocations (
               worktree_id, session_id, user_sequence, catalog_version,
               primary_command, effective_depth, subject_behavior
             ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
            params![
                worktree_id.as_bytes().to_vec(),
                entry.session_id().as_bytes().to_vec(),
                i64::try_from(entry.sequence().get())
                    .map_err(|_| AgentSessionRepositoryError::InvalidInput)?,
                command.primary().name(),
                depth,
                subject_behavior,
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Write)?;
    for (index, lens) in command.lenses().iter().enumerate() {
        let position = i64::try_from(index.saturating_add(1))
            .map_err(|_| AgentSessionRepositoryError::InvalidInput)?;
        transaction
            .execute(
                "INSERT INTO agent_slash_command_lenses (
                   worktree_id, session_id, user_sequence, lens_position, lens_kind
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    worktree_id.as_bytes().to_vec(),
                    entry.session_id().as_bytes().to_vec(),
                    i64::try_from(entry.sequence().get())
                        .map_err(|_| AgentSessionRepositoryError::InvalidInput)?,
                    position,
                    lens.name(),
                ],
            )
            .await
            .map_err(AgentSessionRepositoryError::Write)?;
    }
    Ok(())
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
    let result =
        async {
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
            transaction
            .execute(
                "DELETE FROM agent_message_queue_items WHERE worktree_id = ?1 AND session_id = ?2",
                params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec()],
            )
            .await
            .map_err(AgentSessionRepositoryError::Write)?;
            Ok(())
        }
        .await;
    close(transaction, result).await
}

pub(crate) async fn enqueue_message(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_session_revision: AgentSessionRevision,
    message: &AgentQueuedMessage,
) -> Result<AgentSessionQueue, AgentSessionRepositoryError> {
    if message.state() != AgentQueuedMessageState::Queued || message.ordinal() == 0 {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        require_latest_revision(
            &transaction,
            worktree_id,
            message.session_id(),
            expected_session_revision,
        )
        .await?;
        let (session_count, session_bytes, worktree_count, worktree_bytes) =
            queue_usage(&transaction, worktree_id, message.session_id()).await?;
        if session_count >= 16
            || session_bytes.saturating_add(message.text().as_str().len() as u64) > 1_048_576
            || worktree_count >= 64
            || worktree_bytes.saturating_add(message.text().as_str().len() as u64) > 4_194_304
        {
            return Err(AgentSessionRepositoryError::InvalidInput);
        }
        let expected_ordinal =
            next_queue_ordinal(&transaction, worktree_id, message.session_id()).await?;
        transaction
            .execute(
                "INSERT INTO agent_message_queue_items (
                   worktree_id, session_id, queue_item_id, ordinal, target_mode,
                   research_selection, message, enqueued_at_unix_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    worktree_id.as_bytes().to_vec(),
                    message.session_id().as_bytes().to_vec(),
                    message.id().as_bytes().to_vec(),
                    u64_to_i64(expected_ordinal)?,
                    encode_mode(message.target_mode()),
                    encode_queue_research(message.research()),
                    message.text().as_str(),
                    u64_to_i64(message.enqueued_at().unix_millis())?,
                ],
            )
            .await
            .map_err(AgentSessionRepositoryError::Write)?;
        append_queue_event(
            &transaction,
            worktree_id,
            message.session_id(),
            Some(message.id()),
            "queued",
            message.enqueued_at().unix_millis(),
        )
        .await
    }
    .await;
    close(transaction, result).await?;
    load_message_queue(connection, worktree_id, message.session_id()).await
}

pub(crate) async fn load_message_queue(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<AgentSessionQueue, AgentSessionRepositoryError> {
    let revision = current_queue_revision_connection(connection, worktree_id, session_id).await?;
    let paused = current_queue_paused(connection, worktree_id, session_id).await?;
    let mut rows = connection
        .query(
            "WITH latest AS (
               SELECT queue_item_id, MAX(queue_revision) AS queue_revision
               FROM agent_message_queue_events
               WHERE worktree_id = ?1 AND session_id = ?2 AND queue_item_id IS NOT NULL
               GROUP BY queue_item_id
             )
             SELECT i.queue_item_id, i.ordinal, i.target_mode, i.research_selection,
               i.message, i.enqueued_at_unix_millis
             FROM agent_message_queue_items i
             JOIN latest l ON l.queue_item_id = i.queue_item_id
             JOIN agent_message_queue_events e
               ON e.worktree_id = i.worktree_id AND e.session_id = i.session_id
               AND e.queue_item_id = i.queue_item_id AND e.queue_revision = l.queue_revision
             WHERE i.worktree_id = ?1 AND i.session_id = ?2 AND e.state = 'queued'
             ORDER BY i.ordinal ASC LIMIT 17",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let mut messages = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        let text = AgentSessionText::try_from_string(read_string(&row, 4)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?;
        messages.push(AgentQueuedMessage::from_parts(
            AgentQueuedMessageId::from_bytes(read_id(&row, 0)?),
            session_id,
            read_u64(&row, 1)?,
            decode_mode(&read_string(&row, 2)?)?,
            decode_queue_research(&read_string(&row, 3)?)?,
            text,
            AgentSessionTimestamp::from_unix_millis(read_u64(&row, 5)?)
                .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?,
            AgentQueuedMessageState::Queued,
        ));
    }
    AgentSessionQueue::new(revision, paused, messages)
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

pub(crate) async fn transition_queued_message(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    expected_queue_revision: AgentSessionQueueRevision,
    message_id: AgentQueuedMessageId,
    state: AgentQueuedMessageState,
    occurred_at_unix_millis: u64,
) -> Result<AgentSessionQueue, AgentSessionRepositoryError> {
    let state = match state {
        AgentQueuedMessageState::Started => "started",
        AgentQueuedMessageState::Removed => "removed",
        AgentQueuedMessageState::Queued => "queued",
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        require_queue_revision(
            &transaction,
            worktree_id,
            session_id,
            expected_queue_revision,
        )
        .await?;
        require_queue_item_state(
            &transaction,
            worktree_id,
            session_id,
            message_id,
            if state == "queued" {
                "started"
            } else {
                "queued"
            },
        )
        .await?;
        append_queue_event(
            &transaction,
            worktree_id,
            session_id,
            Some(message_id),
            state,
            occurred_at_unix_millis,
        )
        .await
    }
    .await;
    close(transaction, result).await?;
    load_message_queue(connection, worktree_id, session_id).await
}

pub(crate) async fn set_message_queue_paused(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    expected_queue_revision: AgentSessionQueueRevision,
    paused: bool,
    occurred_at_unix_millis: u64,
) -> Result<AgentSessionQueue, AgentSessionRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AgentSessionRepositoryError::Begin)?;
    let result = async {
        require_queue_revision(
            &transaction,
            worktree_id,
            session_id,
            expected_queue_revision,
        )
        .await?;
        append_queue_event(
            &transaction,
            worktree_id,
            session_id,
            None,
            if paused { "paused" } else { "resumed" },
            occurred_at_unix_millis,
        )
        .await
    }
    .await;
    close(transaction, result).await?;
    load_message_queue(connection, worktree_id, session_id).await
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

pub(crate) async fn load_commands(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    before_sequence: Option<u64>,
    limit: u16,
) -> Result<Vec<AgentSessionCommandPresentation>, AgentSessionRepositoryError> {
    if limit == 0 || limit > 128 {
        return Err(AgentSessionRepositoryError::InvalidInput);
    }
    let before = before_sequence
        .map(i64::try_from)
        .transpose()
        .map_err(|_| AgentSessionRepositoryError::InvalidInput)?;
    let mut rows = connection
        .query(
            "SELECT c.user_sequence, c.catalog_version, c.primary_command,
                    c.effective_depth, l.lens_position, l.lens_kind
             FROM (
               SELECT user_sequence, catalog_version, primary_command, effective_depth
               FROM agent_slash_command_invocations
               WHERE worktree_id = ?1 AND session_id = ?2
                 AND (?3 IS NULL OR user_sequence < ?3)
               ORDER BY user_sequence DESC LIMIT ?4
             ) AS c
             LEFT JOIN agent_slash_command_lenses AS l
               ON l.worktree_id = ?1 AND l.session_id = ?2
              AND l.user_sequence = c.user_sequence
             ORDER BY c.user_sequence, l.lens_position",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                before,
                i64::from(limit)
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;

    struct Pending {
        sequence: AgentSessionSequence,
        primary: SlashCommand,
        depth: AgentResearchDepth,
        lenses: Vec<SlashCommandLens>,
    }
    fn finish(
        pending: Pending,
    ) -> Result<AgentSessionCommandPresentation, AgentSessionRepositoryError> {
        AgentSessionCommandPresentation::restore(
            pending.sequence,
            SlashCommandCatalogVersion::V1,
            pending.primary,
            pending.lenses,
            pending.depth,
        )
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
    }

    let mut result = Vec::new();
    let mut pending: Option<Pending> = None;
    while let Some(row) = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        let sequence = AgentSessionSequence::new(read_u64(&row, 0)?)
            .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)?;
        let catalog_version = read_u64(&row, 1)?;
        if catalog_version != 1 {
            return Err(AgentSessionRepositoryError::InvalidStoredData);
        }
        if pending
            .as_ref()
            .is_some_and(|current| current.sequence != sequence)
        {
            let completed = pending
                .take()
                .ok_or(AgentSessionRepositoryError::InvalidStoredData)?;
            result.push(finish(completed)?);
        }
        if pending.is_none() {
            let primary = SlashCommand::from_stable_name(&read_string(&row, 2)?)
                .ok_or(AgentSessionRepositoryError::InvalidStoredData)?;
            let depth = match read_string(&row, 3)?.as_str() {
                "standard" => AgentResearchDepth::Standard,
                "thorough" => AgentResearchDepth::Thorough,
                _ => return Err(AgentSessionRepositoryError::InvalidStoredData),
            };
            pending = Some(Pending {
                sequence,
                primary,
                depth,
                lenses: Vec::new(),
            });
        }
        let position = read_optional_u64(&row, 4)?;
        let lens_name = read_optional_string(&row, 5)?;
        match (position, lens_name) {
            (None, None) => {}
            (Some(position), Some(name)) => {
                let current = pending
                    .as_mut()
                    .ok_or(AgentSessionRepositoryError::InvalidStoredData)?;
                if position != u64::try_from(current.lenses.len() + 1).unwrap_or(u64::MAX) {
                    return Err(AgentSessionRepositoryError::InvalidStoredData);
                }
                current.lenses.push(
                    SlashCommandLens::from_stable_name(&name)
                        .ok_or(AgentSessionRepositoryError::InvalidStoredData)?,
                );
            }
            _ => return Err(AgentSessionRepositoryError::InvalidStoredData),
        }
    }
    if let Some(completed) = pending {
        result.push(finish(completed)?);
    }
    Ok(result)
}

pub(crate) async fn require_latest_revision(
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

pub(crate) async fn insert_revision(
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

pub(crate) async fn insert_entry(
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
    AgentSessionEntry::try_new(
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
    )
    .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
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

const fn encode_queue_research(value: AgentQueuedResearchSelection) -> &'static str {
    match value {
        AgentQueuedResearchSelection::Standard => "standard",
        AgentQueuedResearchSelection::Thorough => "thorough",
        AgentQueuedResearchSelection::Command => "command",
    }
}

fn decode_queue_research(
    value: &str,
) -> Result<AgentQueuedResearchSelection, AgentSessionRepositoryError> {
    match value {
        "standard" => Ok(AgentQueuedResearchSelection::Standard),
        "thorough" => Ok(AgentQueuedResearchSelection::Thorough),
        "command" => Ok(AgentQueuedResearchSelection::Command),
        _ => Err(AgentSessionRepositoryError::InvalidStoredData),
    }
}

async fn queue_usage(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<(u64, u64, u64, u64), AgentSessionRepositoryError> {
    let mut rows = transaction
        .query(
            "WITH latest AS (
               SELECT session_id, queue_item_id, MAX(queue_revision) AS queue_revision
               FROM agent_message_queue_events WHERE worktree_id = ?1 AND queue_item_id IS NOT NULL
               GROUP BY session_id, queue_item_id
             ), pending AS (
               SELECT i.session_id, length(CAST(i.message AS BLOB)) AS message_bytes
               FROM agent_message_queue_items i
               JOIN latest l ON l.session_id = i.session_id AND l.queue_item_id = i.queue_item_id
               JOIN agent_message_queue_events e
                 ON e.worktree_id = i.worktree_id AND e.session_id = i.session_id
                 AND e.queue_item_id = i.queue_item_id AND e.queue_revision = l.queue_revision
               WHERE i.worktree_id = ?1 AND e.state = 'queued'
             )
             SELECT
               COALESCE(SUM(CASE WHEN session_id = ?2 THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN session_id = ?2 THEN message_bytes ELSE 0 END), 0),
               COUNT(*), COALESCE(SUM(message_bytes), 0)
             FROM pending",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
        .ok_or(AgentSessionRepositoryError::InvalidStoredData)?;
    Ok((
        read_u64(&row, 0)?,
        read_u64(&row, 1)?,
        read_u64(&row, 2)?,
        read_u64(&row, 3)?,
    ))
}

async fn next_queue_ordinal(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<u64, AgentSessionRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(ordinal), 0) FROM agent_message_queue_items
             WHERE worktree_id = ?1 AND session_id = ?2",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let value = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
        .ok_or(AgentSessionRepositoryError::InvalidStoredData)
        .and_then(|row| read_u64(&row, 0))?;
    value
        .checked_add(1)
        .ok_or(AgentSessionRepositoryError::InvalidInput)
}

async fn append_queue_event(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    message_id: Option<AgentQueuedMessageId>,
    state: &str,
    occurred_at_unix_millis: u64,
) -> Result<(), AgentSessionRepositoryError> {
    let revision = current_queue_revision_transaction(transaction, worktree_id, session_id)
        .await?
        .get()
        .checked_add(1)
        .ok_or(AgentSessionRepositoryError::InvalidInput)?;
    transaction
        .execute(
            "INSERT INTO agent_message_queue_events (
               worktree_id, session_id, queue_revision, queue_item_id, state,
               occurred_at_unix_millis
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_to_i64(revision)?,
                message_id.map(|id| id.as_bytes().to_vec()),
                state,
                u64_to_i64(occurred_at_unix_millis)?,
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Write)?;
    Ok(())
}

async fn require_queue_revision(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    expected: AgentSessionQueueRevision,
) -> Result<(), AgentSessionRepositoryError> {
    if current_queue_revision_transaction(transaction, worktree_id, session_id).await? == expected {
        Ok(())
    } else {
        Err(AgentSessionRepositoryError::Conflict)
    }
}

async fn require_queue_item_state(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    message_id: AgentQueuedMessageId,
    expected_state: &str,
) -> Result<(), AgentSessionRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT state FROM agent_message_queue_events
             WHERE worktree_id = ?1 AND session_id = ?2 AND queue_item_id = ?3
             ORDER BY queue_revision DESC LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                message_id.as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    match rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        Some(row) if read_string(&row, 0)? == expected_state => Ok(()),
        _ => Err(AgentSessionRepositoryError::Conflict),
    }
}

async fn current_queue_revision_transaction(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<AgentSessionQueueRevision, AgentSessionRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(queue_revision), 0) FROM agent_message_queue_events
             WHERE worktree_id = ?1 AND session_id = ?2",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let revision = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
        .ok_or(AgentSessionRepositoryError::InvalidStoredData)
        .and_then(|row| read_u64(&row, 0))?;
    AgentSessionQueueRevision::new(revision)
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

async fn current_queue_revision_connection(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<AgentSessionQueueRevision, AgentSessionRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT COALESCE(MAX(queue_revision), 0) FROM agent_message_queue_events
             WHERE worktree_id = ?1 AND session_id = ?2",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    let revision = rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
        .ok_or(AgentSessionRepositoryError::InvalidStoredData)
        .and_then(|row| read_u64(&row, 0))?;
    AgentSessionQueueRevision::new(revision)
        .map_err(|_| AgentSessionRepositoryError::InvalidStoredData)
}

async fn current_queue_paused(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
) -> Result<bool, AgentSessionRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT state FROM agent_message_queue_events
             WHERE worktree_id = ?1 AND session_id = ?2 AND state IN ('paused', 'resumed')
             ORDER BY queue_revision DESC LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AgentSessionRepositoryError::Read)?;
    match rows
        .next()
        .await
        .map_err(AgentSessionRepositoryError::Read)?
    {
        Some(row) => Ok(read_string(&row, 0)? == "paused"),
        None => Ok(false),
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

pub(crate) async fn close<T>(
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
    use super::{
        AgentSessionRepositoryError, append, create, delete_presentation, enqueue_message, list,
        load, load_commands, load_message_queue, set_message_queue_paused,
        transition_queued_message,
    };
    use a3_application::AgentSessionListQuery;
    use a3_domain::{
        AgentQueuedMessage, AgentQueuedMessageId, AgentQueuedMessageState,
        AgentQueuedResearchSelection, AgentSession, AgentSessionEntry, AgentSessionEntryKind,
        AgentSessionId, AgentSessionMode, AgentSessionQueueRevision, AgentSessionRevision,
        AgentSessionSequence, AgentSessionState, AgentSessionText, AgentSessionTimestamp,
        AgentSessionTitle, ParsedSlashCommand, parse_slash_command,
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
                "/review /security Authentifizierung",
                10,
                None,
            )?;
            let command = match parse_slash_command(AgentSessionMode::Plan, user.text().as_str())? {
                ParsedSlashCommand::Command(command) => command,
                ParsedSlashCommand::Plain(_) => return Err("command was not parsed".into()),
            };
            create(
                &connection,
                worktree_id,
                &first,
                Some(&user),
                Some(&command),
            )
            .await
            .map_err(|error| error.classify())?;
            let mut command_rows = connection
                .query(
                    "SELECT primary_command, effective_depth FROM agent_slash_command_invocations WHERE worktree_id = ?1 AND session_id = ?2",
                    libsql::params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec()],
                )
                .await?;
            let command_row = command_rows.next().await?.ok_or("command missing")?;
            assert_eq!(command_row.get::<String>(0)?, "review");
            assert_eq!(command_row.get::<String>(1)?, "thorough");
            let mut lens_rows = connection
                .query(
                    "SELECT lens_kind FROM agent_slash_command_lenses WHERE worktree_id = ?1 AND session_id = ?2 ORDER BY lens_position",
                    libsql::params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec()],
                )
                .await?;
            assert_eq!(
                lens_rows
                    .next()
                    .await?
                    .ok_or("lens missing")?
                    .get::<String>(0)?,
                "security"
            );
            let commands = load_commands(&connection, worktree_id, session_id, None, 128)
                .await
                .map_err(|error| error.classify())?;
            assert_eq!(commands.len(), 1);
            assert_eq!(commands[0].sequence(), AgentSessionSequence::FIRST);
            assert_eq!(commands[0].primary().name(), "review");
            assert_eq!(commands[0].lenses()[0].name(), "security");

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
                None,
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
            let mut command_count = connection
                .query(
                    "SELECT COUNT(*) FROM agent_slash_command_invocations WHERE worktree_id = ?1 AND session_id = ?2",
                    libsql::params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec()],
                )
                .await?;
            assert_eq!(
                command_count
                    .next()
                    .await?
                    .ok_or("count missing")?
                    .get::<i64>(0)?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn durable_queue_preserves_fifo_supports_retry_and_requires_exact_revisions()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let worktree_id = a3_domain::WorktreeId::from_bytes([12; 32]);
            crate::migration::migrate_knowledge(&connection, &[11; 32], worktree_id.as_bytes())
                .await?;
            let session_id = AgentSessionId::from_bytes([13; 32]);
            let completed = session(
                session_id,
                1,
                AgentSessionState::Completed,
                10,
                None,
                None,
                false,
            )?;
            create(&connection, worktree_id, &completed, None, None)
                .await
                .map_err(|error| error.classify())?;

            let first = queued_message(session_id, 1, 14, AgentSessionMode::Ask, "Erste Frage")?;
            let first_queue = enqueue_message(
                &connection,
                worktree_id,
                AgentSessionRevision::INITIAL,
                &first,
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(first_queue.revision().get(), 1);
            let second =
                queued_message(session_id, 2, 15, AgentSessionMode::Plan, "Zweiter Auftrag")?;
            let second_queue = enqueue_message(
                &connection,
                worktree_id,
                AgentSessionRevision::INITIAL,
                &second,
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(
                second_queue
                    .messages()
                    .iter()
                    .map(AgentQueuedMessage::id)
                    .collect::<Vec<_>>(),
                vec![first.id(), second.id()]
            );

            let claimed = transition_queued_message(
                &connection,
                worktree_id,
                session_id,
                second_queue.revision(),
                first.id(),
                AgentQueuedMessageState::Started,
                13,
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(claimed.messages().len(), 1);
            assert_eq!(claimed.messages()[0].id(), second.id());
            assert!(matches!(
                transition_queued_message(
                    &connection,
                    worktree_id,
                    session_id,
                    AgentSessionQueueRevision::new(2)?,
                    second.id(),
                    AgentQueuedMessageState::Removed,
                    14,
                )
                .await,
                Err(AgentSessionRepositoryError::Conflict)
            ));

            let retried = transition_queued_message(
                &connection,
                worktree_id,
                session_id,
                claimed.revision(),
                first.id(),
                AgentQueuedMessageState::Queued,
                15,
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(retried.messages().len(), 2);
            let paused = set_message_queue_paused(
                &connection,
                worktree_id,
                session_id,
                retried.revision(),
                true,
                16,
            )
            .await
            .map_err(|error| error.classify())?;
            assert!(paused.paused());
            assert_eq!(
                load_message_queue(&connection, worktree_id, session_id)
                    .await
                    .map_err(|error| error.classify())?,
                paused
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    fn queued_message(
        session_id: AgentSessionId,
        ordinal: u64,
        id_byte: u8,
        target_mode: AgentSessionMode,
        text: &str,
    ) -> Result<AgentQueuedMessage, Box<dyn std::error::Error>> {
        Ok(AgentQueuedMessage::from_parts(
            AgentQueuedMessageId::from_bytes([id_byte; 32]),
            session_id,
            ordinal,
            target_mode,
            AgentQueuedResearchSelection::Standard,
            AgentSessionText::try_from_string(text.to_owned())?,
            AgentSessionTimestamp::from_unix_millis(ordinal.saturating_add(10))?,
            AgentQueuedMessageState::Queued,
        ))
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
        Ok(AgentSessionEntry::try_new(
            session_id,
            AgentSessionSequence::new(sequence)?,
            kind,
            AgentSessionText::try_from_string(text.to_owned())?,
            AgentSessionTimestamp::from_unix_millis(created_at)?,
            None,
            None,
            plan_revision,
        )?)
    }
}

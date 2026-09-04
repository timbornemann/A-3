use crate::agent_session_repository;
use crate::catalog::is_corruption;
use a3_application::{
    AskResearchDetail, AskResearchEvent, AskResearchProjection, AskResearchPublicFindingKind,
    AskResearchPublicNote, AskResearchSource, AskResearchSourcePage, AskResearchStoreFailure,
    AskResearchTurn, AskResearchTurnPage, EvidenceDiagramArtifact, EvidenceDiagramKind,
    ResearchHandoff, SessionEvidenceDiagramArtifact,
};
use a3_domain::{
    AgentDiagramArtifactId, AgentResearchDepth, AgentSession, AgentSessionEntry, AgentSessionId,
    AgentSessionMode, AgentSessionRevision, AgentSessionSequence, AgentSessionTimestamp,
    AskResearchCompleteness, AskResearchPhase, AskResearchSelectionReason, AskResearchSourceId,
    AskResearchSourceKind, AskResearchState, ContentHash, FileRevision, IndexRunId,
    ParsedSlashCommand, RepositoryPath, SnapshotId, SourcePosition, SourceRange, TaskId,
    WorktreeId, parse_slash_command,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::collections::BTreeMap;

pub(crate) async fn begin(
    connection: &Connection,
    worktree_id: WorktreeId,
    turn: &AskResearchTurn,
    first_event: &AskResearchEvent,
) -> Result<(), AskResearchRepositoryError> {
    if first_event.session_id() != turn.session_id()
        || first_event.user_sequence() != turn.user_sequence()
        || first_event.sequence() != 1
        || first_event.state() != AskResearchState::Running
    {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        transaction.execute(
            "INSERT INTO agent_work_trace_turns (worktree_id, session_id, user_sequence, mode, depth, index_run_id, snapshot_id, started_at_unix_millis) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![worktree_id.as_bytes().to_vec(), turn.session_id().as_bytes().to_vec(), u64_i64(turn.user_sequence().get())?, encode_mode(turn.mode()), encode_depth(turn.depth()), turn.index_run_id().as_bytes().to_vec(), turn.snapshot_id().as_bytes().to_vec(), u64_i64(turn.started_at().unix_millis())?],
        ).await.map_err(AskResearchRepositoryError::Write)?;
        insert_event(&transaction, worktree_id, first_event).await
    }.await;
    close(transaction, result).await
}

pub(crate) async fn append_event(
    connection: &Connection,
    worktree_id: WorktreeId,
    event: &AskResearchEvent,
) -> Result<(), AskResearchRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        require_next_event(&transaction, worktree_id, event).await?;
        insert_event(&transaction, worktree_id, event).await
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn append_sources(
    connection: &Connection,
    worktree_id: WorktreeId,
    sources: &[AskResearchSource],
) -> Result<(), AskResearchRepositoryError> {
    if sources.is_empty() || sources.len() > 200 {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let first = &sources[0];
    if sources.iter().enumerate().any(|(offset, source)| {
        source.session_id() != first.session_id()
            || source.user_sequence() != first.user_sequence()
            || source.ordinal()
                != first
                    .ordinal()
                    .saturating_add(u32::try_from(offset).unwrap_or(u32::MAX))
    }) {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        let current = max_source_ordinal(
            &transaction,
            worktree_id,
            first.session_id(),
            first.user_sequence(),
        )
        .await?;
        if first.ordinal() != current.saturating_add(1)
            || usize::try_from(current)
                .unwrap_or(usize::MAX)
                .saturating_add(sources.len())
                > 200
        {
            return Err(AskResearchRepositoryError::Conflict);
        }
        for source in sources {
            insert_source(&transaction, worktree_id, source).await?;
        }
        Ok(())
    }
    .await;
    close(transaction, result).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn complete(
    connection: &Connection,
    worktree_id: WorktreeId,
    expected_session_revision: AgentSessionRevision,
    session: &AgentSession,
    answer: &AgentSessionEntry,
    event: &AskResearchEvent,
    citations: &[AskResearchSourceId],
    diagrams: &[EvidenceDiagramArtifact],
) -> Result<(), AskResearchRepositoryError> {
    if answer.session_id() != event.session_id()
        || !matches!(
            event.state(),
            AskResearchState::Completed | AskResearchState::AwaitingContinuation
        )
        || citations.len() > 200
        || diagrams.len() > 3
        || (answer.task_id().is_some() && session.mode() != AgentSessionMode::Agent)
    {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        agent_session_repository::require_latest_revision(&transaction, worktree_id, session.id(), expected_session_revision).await.map_err(AskResearchRepositoryError::Session)?;
        require_next_event(&transaction, worktree_id, event).await?;
        agent_session_repository::insert_revision(&transaction, worktree_id, session).await.map_err(AskResearchRepositoryError::Session)?;
        agent_session_repository::insert_entry(&transaction, worktree_id, session.revision(), answer).await.map_err(AskResearchRepositoryError::Session)?;
        insert_event(&transaction, worktree_id, event).await?;
        for (index, source_id) in citations.iter().enumerate() {
            transaction.execute(
                "INSERT INTO agent_work_trace_citations (worktree_id, session_id, user_sequence, citation_position, source_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![worktree_id.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?, i64::try_from(index + 1).map_err(|_| AskResearchRepositoryError::InvalidInput)?, source_id.as_bytes().to_vec()],
            ).await.map_err(AskResearchRepositoryError::Write)?;
        }
        for (index, diagram) in diagrams.iter().enumerate() {
            transaction.execute(
                "INSERT INTO agent_diagram_artifacts (
                   worktree_id, session_id, user_sequence, artifact_id, artifact_position,
                   diagram_kind, title, description, mermaid_source
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    worktree_id.as_bytes().to_vec(),
                    event.session_id().as_bytes().to_vec(),
                    u64_i64(event.user_sequence().get())?,
                    diagram.id().as_bytes().to_vec(),
                    i64::try_from(index.saturating_add(1)).map_err(|_| AskResearchRepositoryError::InvalidInput)?,
                    diagram.kind().name(),
                    diagram.title(),
                    diagram.description(),
                    diagram.mermaid(),
                ],
            ).await.map_err(AskResearchRepositoryError::Write)?;
            for (source_index, source_id) in diagram.source_ids().iter().enumerate() {
                transaction.execute(
                    "INSERT INTO agent_diagram_artifact_sources (
                       worktree_id, session_id, user_sequence, artifact_id, source_position, source_id
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        worktree_id.as_bytes().to_vec(),
                        event.session_id().as_bytes().to_vec(),
                        u64_i64(event.user_sequence().get())?,
                        diagram.id().as_bytes().to_vec(),
                        i64::try_from(source_index.saturating_add(1)).map_err(|_| AskResearchRepositoryError::InvalidInput)?,
                        source_id.as_bytes().to_vec(),
                    ],
                ).await.map_err(AskResearchRepositoryError::Write)?;
            }
        }
        if let Some(task_id) = answer.task_id() {
            require_turn_mode(
                &transaction,
                worktree_id,
                event.session_id(),
                event.user_sequence(),
                AgentSessionMode::Agent,
            )
            .await?;
            insert_task_links(
                &transaction,
                worktree_id,
                event.session_id(),
                event.user_sequence(),
                task_id,
            )
            .await?;
        }
        Ok(())
    }.await;
    close(transaction, result).await
}

async fn require_turn_mode(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    expected: AgentSessionMode,
) -> Result<(), AskResearchRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT mode FROM agent_work_trace_turns WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Err(AskResearchRepositoryError::InvalidInput);
    };
    if decode_mode(&read_string(&row, 0)?)? != expected {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    Ok(())
}

pub(crate) async fn link_task_to_turn(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    task_id: TaskId,
) -> Result<(), AskResearchRepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        require_turn_mode(
            &transaction,
            worktree_id,
            session_id,
            user_sequence,
            AgentSessionMode::Plan,
        )
        .await?;
        insert_task_links(
            &transaction,
            worktree_id,
            session_id,
            user_sequence,
            task_id,
        )
        .await
    }
    .await;
    close(transaction, result).await
}

pub(crate) async fn load_linked_task(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<Option<TaskId>, AskResearchRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT link_id FROM agent_work_trace_links WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 AND link_kind = 'task' LIMIT 1",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    rows.next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
        .map(|row| read_id(&row, 0).map(TaskId::from_bytes))
        .transpose()
}

async fn insert_task_links(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    task_id: TaskId,
) -> Result<(), AskResearchRepositoryError> {
    let mut existing = transaction.query(
        "SELECT 1 FROM agent_work_trace_links WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 AND link_kind = 'task' LIMIT 1",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    if existing
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
        .is_some()
    {
        return Err(AskResearchRepositoryError::Conflict);
    }
    transaction.execute(
        "INSERT INTO agent_work_trace_links (worktree_id, session_id, user_sequence, link_position, link_kind, link_id) VALUES (?1, ?2, ?3, 1, 'task', ?4)",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, task_id.as_bytes().to_vec()],
    ).await.map_err(AskResearchRepositoryError::Write)?;
    let mut run_rows = transaction.query(
        "SELECT run_id FROM agent_runs WHERE task_id = ?1 ORDER BY created_at_unix_millis DESC, run_id DESC LIMIT 1",
        params![task_id.as_bytes().to_vec()],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    if let Some(run_row) = run_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        transaction.execute(
            "INSERT INTO agent_work_trace_links (worktree_id, session_id, user_sequence, link_position, link_kind, link_id) VALUES (?1, ?2, ?3, 2, 'run', ?4)",
            params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, read_id(&run_row, 0)?.to_vec()],
        ).await.map_err(AskResearchRepositoryError::Write)?;
    }
    Ok(())
}

pub(crate) async fn list_turns(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    limit: u16,
) -> Result<AskResearchTurnPage, AskResearchRepositoryError> {
    if limit == 0 || limit > 32 {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let mut rows = connection.query(
        "SELECT user_sequence FROM (\n\
           SELECT user_sequence FROM agent_work_trace_turns WHERE worktree_id = ?1 AND session_id = ?2\n\
           UNION SELECT user_sequence FROM agent_ask_research_turns WHERE worktree_id = ?1 AND session_id = ?2\n\
         ) ORDER BY user_sequence DESC LIMIT ?3",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), i64::from(limit)],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    let mut sequences = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        sequences.push(read_u64(&row, 0)?);
    }
    let mut details = Vec::with_capacity(sequences.len());
    for sequence in sequences {
        let sequence = AgentSessionSequence::new(sequence)
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?;
        let detail = load_detail(connection, worktree_id, session_id, sequence)
            .await?
            .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
        details.push(detail);
    }
    AskResearchTurnPage::new(details).map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

pub(crate) async fn load_detail(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<Option<AskResearchDetail>, AskResearchRepositoryError> {
    let mut rows = connection.query(
        "SELECT index_run_id, snapshot_id, started_at_unix_millis, mode, depth, 0 AS legacy FROM agent_work_trace_turns WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
         UNION ALL\n\
         SELECT index_run_id, snapshot_id, started_at_unix_millis, 'ask', 'standard', 1 FROM agent_ask_research_turns WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
         LIMIT 1",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let mut turn = AskResearchTurn::new_for_mode(
        session_id,
        user_sequence,
        IndexRunId::from_bytes(read_id(&row, 0)?),
        SnapshotId::from_bytes(read_id(&row, 1)?),
        AgentSessionTimestamp::from_unix_millis(read_u64(&row, 2)?)
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
        decode_mode(&read_string(&row, 3)?)?,
        decode_depth(&read_string(&row, 4)?)?,
    );
    let legacy = read_u64(&row, 5)? != 0;
    if legacy {
        turn = turn.as_legacy();
    }
    let event_query = if legacy {
        "SELECT event_sequence, phase, state, action, query_text, completeness, occurred_at_unix_millis FROM agent_ask_research_events WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 ORDER BY event_sequence"
    } else {
        "SELECT event_sequence, phase, state, action, query_text, completeness, occurred_at_unix_millis FROM agent_work_trace_events WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 ORDER BY event_sequence"
    };
    let mut event_rows = connection
        .query(
            event_query,
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut events = Vec::new();
    while let Some(row) = event_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        let mut event = AskResearchEvent::new(
            session_id,
            user_sequence,
            read_u32(&row, 0)?,
            decode_phase(&read_string(&row, 1)?)?,
            decode_state(&read_string(&row, 2)?)?,
            read_string(&row, 3)?,
            read_optional_string(&row, 4)?,
            decode_completeness(&read_string(&row, 5)?)?,
            AgentSessionTimestamp::from_unix_millis(read_u64(&row, 6)?)
                .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
        )
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?;
        if !legacy
            && let Some(note) = load_note(
                connection,
                worktree_id,
                session_id,
                user_sequence,
                event.sequence(),
            )
            .await?
        {
            event = event.with_public_note(note);
        }
        events.push(event);
    }
    let citation_query = if legacy {
        "SELECT source_id FROM agent_ask_research_citations WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 ORDER BY citation_position"
    } else {
        "SELECT source_id FROM agent_work_trace_citations WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 ORDER BY citation_position"
    };
    let mut citation_rows = connection
        .query(
            citation_query,
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut citations = Vec::new();
    while let Some(row) = citation_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        citations.push(AskResearchSourceId::from_bytes(read_id(&row, 0)?));
    }
    AskResearchDetail::new(turn, events, citations)
        .map(Some)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

pub(crate) async fn list_sources(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    after_ordinal: Option<u32>,
    limit: u16,
) -> Result<AskResearchSourcePage, AskResearchRepositoryError> {
    if limit == 0 || limit > 50 {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let mut rows = connection.query(
        "SELECT source_id, ordinal, path, content_hash, start_byte, end_byte, start_line, start_column, end_line, end_column, symbol, source_kind, selection_reason FROM (\n\
           SELECT * FROM agent_work_trace_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
           UNION ALL SELECT * FROM agent_ask_research_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
         ) WHERE ordinal > ?4 ORDER BY ordinal LIMIT ?5",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, i64::from(after_ordinal.unwrap_or(0)), i64::from(limit) + 1],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    let mut sources = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        sources.push(decode_source(session_id, user_sequence, &row)?);
    }
    let has_more = sources.len() > usize::from(limit);
    sources.truncate(usize::from(limit));
    AskResearchSourcePage::new(sources, has_more)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

pub(crate) async fn load_projection(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    source_limit: u16,
) -> Result<Option<AskResearchProjection>, AskResearchRepositoryError> {
    connection
        .execute("BEGIN DEFERRED", ())
        .await
        .map_err(AskResearchRepositoryError::Begin)?;
    let result = async {
        let Some(detail) = load_detail(connection, worktree_id, session_id, user_sequence).await?
        else {
            return Ok(None);
        };
        let sources = list_sources(
            connection,
            worktree_id,
            session_id,
            user_sequence,
            None,
            source_limit,
        )
        .await?;
        let source_count =
            count_sources(connection, worktree_id, session_id, user_sequence).await?;
        AskResearchProjection::new(detail, sources, source_count)
            .map(Some)
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
    }
    .await;
    match result {
        Ok(value) => {
            connection
                .execute("COMMIT", ())
                .await
                .map_err(AskResearchRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match connection.execute("ROLLBACK", ()).await {
            Ok(_) => Err(error),
            Err(source) => Err(AskResearchRepositoryError::Rollback(source)),
        },
    }
}

async fn count_sources(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<u16, AskResearchRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT COUNT(*) FROM (\n\
               SELECT source_id FROM agent_work_trace_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
               UNION ALL SELECT source_id FROM agent_ask_research_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
             )",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
        .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
    let count = read_u64(&row, 0)?;
    if count > 200 {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    }
    u16::try_from(count).map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

pub(crate) async fn load_source(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    source_id: AskResearchSourceId,
) -> Result<Option<AskResearchSource>, AskResearchRepositoryError> {
    let mut rows = connection.query(
        "SELECT source_id, ordinal, path, content_hash, start_byte, end_byte, start_line, start_column, end_line, end_column, symbol, source_kind, selection_reason FROM (\n\
           SELECT * FROM agent_work_trace_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
           UNION ALL SELECT * FROM agent_ask_research_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3\n\
         ) WHERE source_id = ?4 LIMIT 1",
        params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, source_id.as_bytes().to_vec()],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    rows.next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
        .map(|row| decode_source(session_id, user_sequence, &row))
        .transpose()
}

pub(crate) async fn list_diagrams(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<Vec<EvidenceDiagramArtifact>, AskResearchRepositoryError> {
    let rows = connection
        .query(
            "SELECT d.user_sequence, d.artifact_id, d.diagram_kind, d.title, d.description,
                d.mermaid_source, s.source_id
         FROM agent_diagram_artifacts AS d
         INNER JOIN agent_diagram_artifact_sources AS s
           ON s.worktree_id = d.worktree_id AND s.session_id = d.session_id
          AND s.user_sequence = d.user_sequence AND s.artifact_id = d.artifact_id
         WHERE d.worktree_id = ?1 AND d.session_id = ?2 AND d.user_sequence = ?3
         ORDER BY d.artifact_position, s.source_position",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    collect_diagrams(rows)
        .await
        .map(|values| values.into_iter().map(|(_, diagram)| diagram).collect())
}

pub(crate) async fn list_session_diagrams(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    before_sequence: Option<u64>,
    user_turn_limit: u16,
) -> Result<Vec<SessionEvidenceDiagramArtifact>, AskResearchRepositoryError> {
    if user_turn_limit == 0 || user_turn_limit > 128 {
        return Err(AskResearchRepositoryError::InvalidInput);
    }
    let before = before_sequence
        .map(i64::try_from)
        .transpose()
        .map_err(|_| AskResearchRepositoryError::InvalidInput)?;
    let rows = connection
        .query(
            "SELECT d.user_sequence, d.artifact_id, d.diagram_kind, d.title, d.description,
                    d.mermaid_source, s.source_id
             FROM agent_diagram_artifacts AS d
             INNER JOIN (
               SELECT user_sequence FROM agent_work_trace_turns
               WHERE worktree_id = ?1 AND session_id = ?2
                 AND (?3 IS NULL OR user_sequence < ?3)
               ORDER BY user_sequence DESC
               LIMIT ?4
             ) AS visible ON visible.user_sequence = d.user_sequence
             INNER JOIN agent_diagram_artifact_sources AS s
               ON s.worktree_id = d.worktree_id AND s.session_id = d.session_id
              AND s.user_sequence = d.user_sequence AND s.artifact_id = d.artifact_id
             WHERE d.worktree_id = ?1 AND d.session_id = ?2
             ORDER BY d.user_sequence, d.artifact_position, s.source_position",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                before,
                i64::from(user_turn_limit)
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let artifacts = collect_diagrams(rows).await?;
    let mut anchor_rows = connection
        .query(
            "SELECT user_sequence, index_run_id, snapshot_id
             FROM agent_work_trace_turns
             WHERE worktree_id = ?1 AND session_id = ?2
               AND (?3 IS NULL OR user_sequence < ?3)
             ORDER BY user_sequence DESC LIMIT ?4",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                before,
                i64::from(user_turn_limit)
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut anchors = BTreeMap::new();
    while let Some(row) = anchor_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        let sequence = AgentSessionSequence::new(read_u64(&row, 0)?)
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?;
        anchors.insert(
            sequence,
            (
                IndexRunId::from_bytes(read_id(&row, 1)?),
                SnapshotId::from_bytes(read_id(&row, 2)?),
            ),
        );
    }
    artifacts
        .into_iter()
        .map(|(sequence, artifact)| {
            let (index_run_id, snapshot_id) = anchors
                .get(&sequence)
                .copied()
                .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
            Ok(SessionEvidenceDiagramArtifact::new(
                sequence,
                index_run_id,
                snapshot_id,
                artifact,
            ))
        })
        .collect()
}

pub(crate) async fn load_diagram(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    artifact_id: AgentDiagramArtifactId,
) -> Result<Option<(AgentSessionSequence, EvidenceDiagramArtifact)>, AskResearchRepositoryError> {
    let rows = connection
        .query(
            "SELECT d.user_sequence, d.artifact_id, d.diagram_kind, d.title, d.description,
                d.mermaid_source, s.source_id
         FROM agent_diagram_artifacts AS d
         INNER JOIN agent_diagram_artifact_sources AS s
           ON s.worktree_id = d.worktree_id AND s.session_id = d.session_id
          AND s.user_sequence = d.user_sequence AND s.artifact_id = d.artifact_id
         WHERE d.worktree_id = ?1 AND d.session_id = ?2 AND d.artifact_id = ?3
         ORDER BY s.source_position",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                artifact_id.as_bytes().to_vec()
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut values = collect_diagrams(rows).await?;
    if values.len() > 1 {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    }
    Ok(values.pop())
}

async fn collect_diagrams(
    mut rows: libsql::Rows,
) -> Result<Vec<(AgentSessionSequence, EvidenceDiagramArtifact)>, AskResearchRepositoryError> {
    struct PendingDiagram {
        sequence: AgentSessionSequence,
        id: AgentDiagramArtifactId,
        kind: EvidenceDiagramKind,
        title: String,
        description: String,
        mermaid: String,
        sources: Vec<AskResearchSourceId>,
    }
    fn finish(
        pending: PendingDiagram,
    ) -> Result<(AgentSessionSequence, EvidenceDiagramArtifact), AskResearchRepositoryError> {
        EvidenceDiagramArtifact::restore(
            pending.id,
            pending.kind,
            pending.title,
            pending.description,
            pending.mermaid,
            pending.sources,
        )
        .map(|diagram| (pending.sequence, diagram))
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
    }

    let mut result = Vec::new();
    let mut pending: Option<PendingDiagram> = None;
    while let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        let id = AgentDiagramArtifactId::from_bytes(read_id(&row, 1)?);
        if pending.as_ref().is_some_and(|value| value.id != id) {
            let completed = pending
                .take()
                .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
            result.push(finish(completed)?);
        }
        if pending.is_none() {
            pending = Some(PendingDiagram {
                sequence: AgentSessionSequence::new(read_u64(&row, 0)?)
                    .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
                id,
                kind: EvidenceDiagramKind::from_name(&read_string(&row, 2)?)
                    .ok_or(AskResearchRepositoryError::InvalidStoredData)?,
                title: read_string(&row, 3)?,
                description: read_string(&row, 4)?,
                mermaid: read_string(&row, 5)?,
                sources: Vec::new(),
            });
        }
        let source_id = AskResearchSourceId::from_bytes(read_id(&row, 6)?);
        let current = pending
            .as_mut()
            .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
        if current.sources.contains(&source_id) {
            return Err(AskResearchRepositoryError::InvalidStoredData);
        }
        current.sources.push(source_id);
    }
    if let Some(completed) = pending {
        result.push(finish(completed)?);
    }
    if result.len() > 3 {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    }
    Ok(result)
}

pub(crate) async fn load_handoff_for_task(
    connection: &Connection,
    worktree_id: WorktreeId,
    task_id: TaskId,
) -> Result<Option<ResearchHandoff>, AskResearchRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT t.session_id, t.user_sequence, t.index_run_id, t.snapshot_id \
             FROM agent_work_trace_links AS l \
             INNER JOIN agent_work_trace_turns AS t \
               ON t.worktree_id = l.worktree_id \
              AND t.session_id = l.session_id \
              AND t.user_sequence = l.user_sequence \
             WHERE l.worktree_id = ?1 AND l.link_kind = 'task' AND l.link_id = ?2 \
             ORDER BY t.started_at_unix_millis DESC LIMIT 1",
            params![worktree_id.as_bytes().to_vec(), task_id.as_bytes().to_vec()],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let session_id = AgentSessionId::from_bytes(read_id(&row, 0)?);
    let user_sequence = AgentSessionSequence::new(read_u64(&row, 1)?)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?;
    let index_run_id = IndexRunId::from_bytes(read_id(&row, 2)?);
    let snapshot_id = SnapshotId::from_bytes(read_id(&row, 3)?);
    drop(rows);
    let command = load_handoff_command(connection, worktree_id, session_id, user_sequence).await?;

    let mut source_rows = connection
        .query(
            "SELECT path, content_hash FROM agent_work_trace_sources \
             WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 \
             ORDER BY ordinal LIMIT 200",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut revisions = Vec::new();
    while let Some(source_row) = source_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(read_bytes(&source_row, 0)?)
                .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
            ContentHash::from_bytes(read_id(&source_row, 1)?),
        );
        if !revisions.contains(&revision) {
            revisions.push(revision);
        }
    }
    let handoff = ResearchHandoff::new(index_run_id, snapshot_id, revisions)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?;
    Ok(Some(match command {
        Some(command) => handoff.with_command(command),
        None => handoff,
    }))
}

async fn load_handoff_command(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<Option<a3_domain::SlashCommandInvocation>, AskResearchRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT c.catalog_version, c.primary_command, c.effective_depth, \
                    l.lens_position, l.lens_kind, e.content, t.mode \
             FROM agent_slash_command_invocations AS c \
             INNER JOIN agent_session_entries AS e \
               ON e.worktree_id = c.worktree_id AND e.session_id = c.session_id \
              AND e.sequence = c.user_sequence \
             INNER JOIN agent_work_trace_turns AS t \
               ON t.worktree_id = c.worktree_id AND t.session_id = c.session_id \
              AND t.user_sequence = c.user_sequence \
             LEFT JOIN agent_slash_command_lenses AS l \
               ON l.worktree_id = c.worktree_id AND l.session_id = c.session_id \
              AND l.user_sequence = c.user_sequence \
             WHERE c.worktree_id = ?1 AND c.session_id = ?2 AND c.user_sequence = ?3 \
             ORDER BY l.lens_position",
            params![
                worktree_id.as_bytes().to_vec(),
                session_id.as_bytes().to_vec(),
                u64_i64(user_sequence.get())?
            ],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut persisted: Option<(String, String, String, AgentSessionMode)> = None;
    let mut lenses = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        if read_u64(&row, 0)? != 1 {
            return Err(AskResearchRepositoryError::InvalidStoredData);
        }
        let current = (
            read_string(&row, 1)?,
            read_string(&row, 2)?,
            read_string(&row, 5)?,
            decode_mode(&read_string(&row, 6)?)?,
        );
        if persisted.as_ref().is_some_and(|value| value != &current) {
            return Err(AskResearchRepositoryError::InvalidStoredData);
        }
        if persisted.is_none() {
            persisted = Some(current);
        }
        if let Some(position) = read_optional_u64(&row, 3)? {
            if position != u64::try_from(lenses.len().saturating_add(1)).unwrap_or(u64::MAX) {
                return Err(AskResearchRepositoryError::InvalidStoredData);
            }
            lenses.push(
                read_optional_string(&row, 4)?
                    .ok_or(AskResearchRepositoryError::InvalidStoredData)?,
            );
        } else if read_optional_string(&row, 4)?.is_some() {
            return Err(AskResearchRepositoryError::InvalidStoredData);
        }
    }
    let Some((primary, depth, content, mode)) = persisted else {
        return Ok(None);
    };
    let ParsedSlashCommand::Command(command) = parse_slash_command(mode, &content)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?
    else {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    };
    let expected_depth = match command.depth() {
        AgentResearchDepth::Standard => "standard",
        AgentResearchDepth::Thorough => "thorough",
    };
    if command.primary().name() != primary
        || expected_depth != depth
        || command
            .lenses()
            .iter()
            .map(|lens| lens.name())
            .ne(lenses.iter().map(String::as_str))
    {
        return Err(AskResearchRepositoryError::InvalidStoredData);
    }
    Ok(Some(command))
}

async fn require_next_event(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    event: &AskResearchEvent,
) -> Result<(), AskResearchRepositoryError> {
    let mut rows = transaction.query(
        "SELECT event_sequence, state FROM agent_work_trace_events WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 ORDER BY event_sequence DESC LIMIT 1",
        params![worktree_id.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?],
    ).await.map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Err(AskResearchRepositoryError::Conflict);
    };
    let current = read_u32(&row, 0)?;
    if current.saturating_add(1) != event.sequence()
        || decode_state(&read_string(&row, 1)?)? != AskResearchState::Running
    {
        return Err(AskResearchRepositoryError::Conflict);
    }
    Ok(())
}

async fn max_source_ordinal(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
) -> Result<u32, AskResearchRepositoryError> {
    let mut rows = transaction.query("SELECT COALESCE(MAX(ordinal), 0) FROM agent_work_trace_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3", params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?]).await.map_err(AskResearchRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
        .ok_or(AskResearchRepositoryError::InvalidStoredData)?;
    read_u32(&row, 0)
}

async fn insert_event(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    event: &AskResearchEvent,
) -> Result<(), AskResearchRepositoryError> {
    transaction.execute(
        "INSERT INTO agent_work_trace_events (worktree_id, session_id, user_sequence, event_sequence, phase, state, action, query_text, completeness, occurred_at_unix_millis) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![worktree_id.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?, i64::from(event.sequence()), encode_phase(event.phase()), encode_state(event.state()), event.action(), event.query(), encode_completeness(event.completeness()), u64_i64(event.occurred_at().unix_millis())?],
    ).await.map_err(AskResearchRepositoryError::Write)?;
    if let Some(note) = event.public_note() {
        insert_note(transaction, worktree_id, event, note).await?;
    }
    Ok(())
}

async fn insert_note(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    event: &AskResearchEvent,
    note: &AskResearchPublicNote,
) -> Result<(), AskResearchRepositoryError> {
    transaction
        .execute(
            "INSERT INTO agent_work_trace_notes (worktree_id, session_id, user_sequence, event_sequence, goal, finding_kind, finding, gap, next_step) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![worktree_id.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?, i64::from(event.sequence()), note.goal(), encode_finding_kind(note.finding_kind()), note.finding(), note.gap(), note.next_step()],
        )
        .await
        .map_err(AskResearchRepositoryError::Write)?;
    for (index, source_id) in note.source_ids().iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO agent_work_trace_note_sources (worktree_id, session_id, user_sequence, event_sequence, source_position, source_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![worktree_id.as_bytes().to_vec(), event.session_id().as_bytes().to_vec(), u64_i64(event.user_sequence().get())?, i64::from(event.sequence()), i64::try_from(index + 1).map_err(|_| AskResearchRepositoryError::InvalidInput)?, source_id.as_bytes().to_vec()],
            )
            .await
            .map_err(AskResearchRepositoryError::Write)?;
    }
    Ok(())
}

async fn load_note(
    connection: &Connection,
    worktree_id: WorktreeId,
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    event_sequence: u32,
) -> Result<Option<AskResearchPublicNote>, AskResearchRepositoryError> {
    let mut rows = connection
        .query(
            "SELECT goal, finding_kind, finding, gap, next_step FROM agent_work_trace_notes WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 AND event_sequence = ?4",
            params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, i64::from(event_sequence)],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let goal = read_string(&row, 0)?;
    let finding_kind = decode_finding_kind(&read_string(&row, 1)?)?;
    let finding = read_string(&row, 2)?;
    let gap = read_string(&row, 3)?;
    let next_step = read_string(&row, 4)?;
    let mut source_rows = connection
        .query(
            "SELECT source_id FROM agent_work_trace_note_sources WHERE worktree_id = ?1 AND session_id = ?2 AND user_sequence = ?3 AND event_sequence = ?4 ORDER BY source_position",
            params![worktree_id.as_bytes().to_vec(), session_id.as_bytes().to_vec(), u64_i64(user_sequence.get())?, i64::from(event_sequence)],
        )
        .await
        .map_err(AskResearchRepositoryError::Read)?;
    let mut source_ids = Vec::new();
    while let Some(source_row) = source_rows
        .next()
        .await
        .map_err(AskResearchRepositoryError::Read)?
    {
        source_ids.push(AskResearchSourceId::from_bytes(read_id(&source_row, 0)?));
    }
    AskResearchPublicNote::new(goal, finding_kind, finding, source_ids, gap, next_step)
        .map(Some)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

async fn insert_source(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    source: &AskResearchSource,
) -> Result<(), AskResearchRepositoryError> {
    let (start_byte, end_byte, start_line, start_column, end_line, end_column) =
        match source.range() {
            Some(range) => (
                Some(i64::from(range.start_byte())),
                Some(i64::from(range.end_byte())),
                Some(i64::from(range.start_position().row())),
                Some(i64::from(range.start_position().column())),
                Some(i64::from(range.end_position().row())),
                Some(i64::from(range.end_position().column())),
            ),
            None => (None, None, None, None, None, None),
        };
    transaction.execute(
        "INSERT INTO agent_work_trace_sources (worktree_id, session_id, user_sequence, source_id, ordinal, path, content_hash, start_byte, end_byte, start_line, start_column, end_line, end_column, symbol, source_kind, selection_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![worktree_id.as_bytes().to_vec(), source.session_id().as_bytes().to_vec(), u64_i64(source.user_sequence().get())?, source.id().as_bytes().to_vec(), i64::from(source.ordinal()), source.revision().path().as_bytes().to_vec(), source.revision().content_hash().as_bytes().to_vec(), start_byte, end_byte, start_line, start_column, end_line, end_column, source.symbol(), encode_source_kind(source.kind()), encode_reason(source.reason())],
    ).await.map_err(AskResearchRepositoryError::Write)?;
    Ok(())
}

fn decode_source(
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    row: &libsql::Row,
) -> Result<AskResearchSource, AskResearchRepositoryError> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(read_bytes(row, 2)?)
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
        ContentHash::from_bytes(read_id(row, 3)?),
    );
    let range = match read_optional_u64(row, 4)? {
        Some(start_byte) => Some(
            SourceRange::new(
                usize::try_from(start_byte)
                    .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
                usize::try_from(read_u64(row, 5)?)
                    .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
                SourcePosition::new(read_u32(row, 6)?, read_u32(row, 7)?),
                SourcePosition::new(read_u32(row, 8)?, read_u32(row, 9)?),
            )
            .map_err(|_| AskResearchRepositoryError::InvalidStoredData)?,
        ),
        None => None,
    };
    AskResearchSource::new(
        session_id,
        user_sequence,
        AskResearchSourceId::from_bytes(read_id(row, 0)?),
        read_u32(row, 1)?,
        revision,
        range,
        read_optional_string(row, 10)?,
        decode_source_kind(&read_string(row, 11)?)?,
        decode_reason(&read_string(row, 12)?)?,
    )
    .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}

fn encode_phase(value: AskResearchPhase) -> &'static str {
    match value {
        AskResearchPhase::Preparing => "preparing",
        AskResearchPhase::Locating => "locating",
        AskResearchPhase::Deciding => "deciding",
        AskResearchPhase::Reading => "reading",
        AskResearchPhase::Evaluating => "evaluating",
        AskResearchPhase::AnsweringOrPlanning => "answering_or_planning",
        AskResearchPhase::SelectingEvidence => "selecting_evidence",
        AskResearchPhase::SearchingSource => "searching_source",
        AskResearchPhase::InspectingSource => "inspecting_source",
        AskResearchPhase::Answering => "answering",
        AskResearchPhase::Completed => "completed",
    }
}
fn decode_phase(value: &str) -> Result<AskResearchPhase, AskResearchRepositoryError> {
    match value {
        "preparing" => Ok(AskResearchPhase::Preparing),
        "locating" => Ok(AskResearchPhase::Locating),
        "deciding" => Ok(AskResearchPhase::Deciding),
        "reading" => Ok(AskResearchPhase::Reading),
        "evaluating" => Ok(AskResearchPhase::Evaluating),
        "answering_or_planning" => Ok(AskResearchPhase::AnsweringOrPlanning),
        "selecting_evidence" => Ok(AskResearchPhase::SelectingEvidence),
        "searching_source" => Ok(AskResearchPhase::SearchingSource),
        "inspecting_source" => Ok(AskResearchPhase::InspectingSource),
        "answering" => Ok(AskResearchPhase::Answering),
        "completed" => Ok(AskResearchPhase::Completed),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
fn encode_state(value: AskResearchState) -> &'static str {
    match value {
        AskResearchState::Running => "running",
        AskResearchState::Completed => "completed",
        AskResearchState::Failed => "failed",
        AskResearchState::Cancelled => "cancelled",
        AskResearchState::AwaitingContinuation => "awaiting_continuation",
    }
}
fn decode_state(value: &str) -> Result<AskResearchState, AskResearchRepositoryError> {
    match value {
        "running" => Ok(AskResearchState::Running),
        "completed" => Ok(AskResearchState::Completed),
        "failed" => Ok(AskResearchState::Failed),
        "cancelled" => Ok(AskResearchState::Cancelled),
        "awaiting_continuation" => Ok(AskResearchState::AwaitingContinuation),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}

const fn encode_mode(value: AgentSessionMode) -> &'static str {
    match value {
        AgentSessionMode::Ask => "ask",
        AgentSessionMode::Plan => "plan",
        AgentSessionMode::Agent => "agent",
    }
}
fn decode_mode(value: &str) -> Result<AgentSessionMode, AskResearchRepositoryError> {
    match value {
        "ask" => Ok(AgentSessionMode::Ask),
        "plan" => Ok(AgentSessionMode::Plan),
        "agent" => Ok(AgentSessionMode::Agent),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
const fn encode_depth(value: AgentResearchDepth) -> &'static str {
    match value {
        AgentResearchDepth::Standard => "standard",
        AgentResearchDepth::Thorough => "thorough",
    }
}
fn decode_depth(value: &str) -> Result<AgentResearchDepth, AskResearchRepositoryError> {
    match value {
        "standard" => Ok(AgentResearchDepth::Standard),
        "thorough" => Ok(AgentResearchDepth::Thorough),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
const fn encode_finding_kind(value: AskResearchPublicFindingKind) -> &'static str {
    match value {
        AskResearchPublicFindingKind::Observation => "observation",
        AskResearchPublicFindingKind::Hypothesis => "hypothesis",
        AskResearchPublicFindingKind::Conclusion => "conclusion",
    }
}
fn decode_finding_kind(
    value: &str,
) -> Result<AskResearchPublicFindingKind, AskResearchRepositoryError> {
    match value {
        "observation" => Ok(AskResearchPublicFindingKind::Observation),
        "hypothesis" => Ok(AskResearchPublicFindingKind::Hypothesis),
        "conclusion" => Ok(AskResearchPublicFindingKind::Conclusion),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
fn encode_completeness(value: AskResearchCompleteness) -> &'static str {
    match value {
        AskResearchCompleteness::Complete => "complete",
        AskResearchCompleteness::Limited => "limited",
        AskResearchCompleteness::NotApplicable => "not_applicable",
    }
}
fn decode_completeness(value: &str) -> Result<AskResearchCompleteness, AskResearchRepositoryError> {
    match value {
        "complete" => Ok(AskResearchCompleteness::Complete),
        "limited" => Ok(AskResearchCompleteness::Limited),
        "not_applicable" => Ok(AskResearchCompleteness::NotApplicable),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
fn encode_source_kind(value: AskResearchSourceKind) -> &'static str {
    match value {
        AskResearchSourceKind::File => "file",
        AskResearchSourceKind::Symbol => "symbol",
        AskResearchSourceKind::Relationship => "relationship",
        AskResearchSourceKind::VerifiedClaim => "verified_claim",
    }
}
fn decode_source_kind(value: &str) -> Result<AskResearchSourceKind, AskResearchRepositoryError> {
    match value {
        "file" => Ok(AskResearchSourceKind::File),
        "symbol" => Ok(AskResearchSourceKind::Symbol),
        "relationship" => Ok(AskResearchSourceKind::Relationship),
        "verified_claim" => Ok(AskResearchSourceKind::VerifiedClaim),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}
fn encode_reason(value: AskResearchSelectionReason) -> &'static str {
    match value {
        AskResearchSelectionReason::ExactNameOrPath => "exact_name_or_path",
        AskResearchSelectionReason::IndexedText => "indexed_text",
        AskResearchSelectionReason::Relationship => "relationship",
        AskResearchSelectionReason::Test => "test",
        AskResearchSelectionReason::VerifiedModuleKnowledge => "verified_module_knowledge",
        AskResearchSelectionReason::SemanticCandidate => "semantic_candidate",
        AskResearchSelectionReason::SourceText => "source_text",
    }
}
fn decode_reason(value: &str) -> Result<AskResearchSelectionReason, AskResearchRepositoryError> {
    match value {
        "exact_name_or_path" => Ok(AskResearchSelectionReason::ExactNameOrPath),
        "indexed_text" => Ok(AskResearchSelectionReason::IndexedText),
        "relationship" => Ok(AskResearchSelectionReason::Relationship),
        "test" => Ok(AskResearchSelectionReason::Test),
        "verified_module_knowledge" => Ok(AskResearchSelectionReason::VerifiedModuleKnowledge),
        "semantic_candidate" => Ok(AskResearchSelectionReason::SemanticCandidate),
        "source_text" => Ok(AskResearchSelectionReason::SourceText),
        _ => Err(AskResearchRepositoryError::InvalidStoredData),
    }
}

fn read_bytes(row: &libsql::Row, index: i32) -> Result<Vec<u8>, AskResearchRepositoryError> {
    row.get(index).map_err(AskResearchRepositoryError::Read)
}
fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], AskResearchRepositoryError> {
    <[u8; 32]>::try_from(read_bytes(row, index)?)
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}
fn read_string(row: &libsql::Row, index: i32) -> Result<String, AskResearchRepositoryError> {
    row.get(index).map_err(AskResearchRepositoryError::Read)
}
fn read_optional_string(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, AskResearchRepositoryError> {
    row.get(index).map_err(AskResearchRepositoryError::Read)
}
fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, AskResearchRepositoryError> {
    let value: i64 = row.get(index).map_err(AskResearchRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}
fn read_optional_u64(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, AskResearchRepositoryError> {
    let value: Option<i64> = row.get(index).map_err(AskResearchRepositoryError::Read)?;
    value
        .map(u64::try_from)
        .transpose()
        .map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}
fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, AskResearchRepositoryError> {
    u32::try_from(read_u64(row, index)?).map_err(|_| AskResearchRepositoryError::InvalidStoredData)
}
fn u64_i64(value: u64) -> Result<i64, AskResearchRepositoryError> {
    i64::try_from(value).map_err(|_| AskResearchRepositoryError::InvalidInput)
}

async fn close<T>(
    transaction: Transaction,
    result: Result<T, AskResearchRepositoryError>,
) -> Result<T, AskResearchRepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(AskResearchRepositoryError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(AskResearchRepositoryError::Rollback(source)),
        },
    }
}

#[derive(Debug)]
pub(crate) enum AskResearchRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Session(agent_session_repository::AgentSessionRepositoryError),
    Conflict,
    InvalidInput,
    InvalidStoredData,
}
impl AskResearchRepositoryError {
    pub(crate) fn classify(&self) -> AskResearchStoreFailure {
        match self {
            Self::Conflict => AskResearchStoreFailure::Conflict,
            Self::InvalidInput => AskResearchStoreFailure::InvalidInput,
            Self::InvalidStoredData => AskResearchStoreFailure::InvalidStoredData,
            Self::Session(error) => match error.classify() {
                a3_application::AgentSessionStoreFailure::Conflict => {
                    AskResearchStoreFailure::Conflict
                }
                a3_application::AgentSessionStoreFailure::InvalidInput => {
                    AskResearchStoreFailure::InvalidInput
                }
                a3_application::AgentSessionStoreFailure::InvalidStoredData => {
                    AskResearchStoreFailure::InvalidStoredData
                }
                a3_application::AgentSessionStoreFailure::Unavailable => {
                    AskResearchStoreFailure::Unavailable
                }
            },
            Self::Begin(error)
            | Self::Read(error)
            | Self::Write(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    AskResearchStoreFailure::InvalidStoredData
                } else {
                    AskResearchStoreFailure::Unavailable
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_event, append_sources, begin, complete, list_diagrams, list_session_diagrams,
        list_sources, load_detail, load_diagram, load_handoff_for_task, load_linked_task,
        load_projection,
    };
    use crate::agent_session_repository;
    use a3_application::{
        AskResearchEvent, AskResearchPublicFindingKind, AskResearchPublicNote, AskResearchSource,
        EvidenceDiagramArtifact, EvidenceDiagramKind,
    };
    use a3_domain::{
        AgentDiagramArtifactId, AgentResearchDepth, AgentSession, AgentSessionEntry,
        AgentSessionEntryKind, AgentSessionId, AgentSessionMode, AgentSessionRevision,
        AgentSessionSequence, AgentSessionState, AgentSessionText, AgentSessionTimestamp,
        AgentSessionTitle, AgentWorkItemId, AskResearchCompleteness, AskResearchPhase,
        AskResearchSelectionReason, AskResearchSourceId, AskResearchSourceKind, AskResearchState,
        ContentHash, FileRevision, IndexRunId, ParsedSlashCommand, RepositoryPath, SlashCommand,
        SlashCommandLens, SnapshotId, TaskId, WorktreeId, parse_slash_command,
    };

    #[test]
    fn answer_event_citations_and_session_revision_commit_atomically_and_delete_together()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let worktree_id = WorktreeId::from_bytes([2; 32]);
            crate::migration::migrate_knowledge(&connection, &[1; 32], worktree_id.as_bytes())
                .await?;
            let session_id = AgentSessionId::from_bytes([3; 32]);
            let user_sequence = AgentSessionSequence::new(1)?;
            let running = session(
                session_id,
                1,
                AgentSessionState::Running,
                10,
                Some(user_sequence),
                false,
            )?;
            let user = entry(
                session_id,
                user_sequence,
                AgentSessionEntryKind::UserMessage,
                "/review /security Authentifizierung",
                10,
            )?;
            let ParsedSlashCommand::Command(command) =
                parse_slash_command(AgentSessionMode::Agent, user.text().as_str())?
            else {
                return Err("command was not parsed".into());
            };
            agent_session_repository::create(
                &connection,
                worktree_id,
                &running,
                Some(&user),
                Some(&command),
            )
            .await
            .map_err(|error| error.classify())?;
            assert_eq!(
                load_projection(&connection, worktree_id, session_id, user_sequence, 50)
                    .await
                    .map_err(|error| error.classify())?,
                None
            );

            let first_event = AskResearchEvent::new(
                session_id,
                user_sequence,
                1,
                AskResearchPhase::Preparing,
                AskResearchState::Running,
                "Projektstand binden".to_owned(),
                None,
                AskResearchCompleteness::NotApplicable,
                AgentSessionTimestamp::from_unix_millis(11)?,
            )?;
            let turn = a3_application::AskResearchTurn::new_for_mode(
                session_id,
                user_sequence,
                IndexRunId::from_bytes([4; 32]),
                SnapshotId::from_bytes([5; 32]),
                AgentSessionTimestamp::from_unix_millis(11)?,
                AgentSessionMode::Agent,
                AgentResearchDepth::Thorough,
            );
            begin(&connection, worktree_id, &turn, &first_event)
                .await
                .map_err(|error| error.classify())?;
            let source_id = AskResearchSourceId::from_bytes([6; 32]);
            let source = AskResearchSource::new(
                session_id,
                user_sequence,
                source_id,
                1,
                FileRevision::new(
                    RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
                    ContentHash::from_bytes([7; 32]),
                ),
                None,
                None,
                AskResearchSourceKind::File,
                AskResearchSelectionReason::SourceText,
            )?;
            append_sources(&connection, worktree_id, std::slice::from_ref(&source))
                .await
                .map_err(|error| error.classify())?;
            let note = AskResearchPublicNote::new(
                "TODOs lokalisieren".to_owned(),
                AskResearchPublicFindingKind::Observation,
                "Ein aktueller Treffer wurde gelesen".to_owned(),
                vec![source_id],
                "Weitere Aufrufstellen sind offen".to_owned(),
                "Direkte Beziehungen prüfen".to_owned(),
            )?;
            let note_event = AskResearchEvent::new(
                session_id,
                user_sequence,
                2,
                AskResearchPhase::Evaluating,
                AskResearchState::Running,
                "Zwischenbefund auswerten".to_owned(),
                None,
                AskResearchCompleteness::NotApplicable,
                AgentSessionTimestamp::from_unix_millis(11)?,
            )?
            .with_public_note(note.clone());
            append_event(&connection, worktree_id, &note_event)
                .await
                .map_err(|error| error.classify())?;

            let completed = session(
                session_id,
                2,
                AgentSessionState::Completed,
                12,
                Some(AgentSessionSequence::new(2)?),
                false,
            )?;
            let task_id = TaskId::from_bytes([9; 32]);
            let answer = AgentSessionEntry::try_new(
                session_id,
                AgentSessionSequence::new(2)?,
                AgentSessionEntryKind::FinalReport,
                AgentSessionText::try_from_string("Ein TODO wurde gefunden.".to_owned())?,
                AgentSessionTimestamp::from_unix_millis(12)?,
                Some(AgentWorkItemId::from_bytes([10; 32])),
                Some(task_id),
                None,
            )?;
            let terminal = AskResearchEvent::new(
                session_id,
                user_sequence,
                3,
                AskResearchPhase::Completed,
                AskResearchState::Completed,
                "Antwort veröffentlicht".to_owned(),
                None,
                AskResearchCompleteness::Complete,
                AgentSessionTimestamp::from_unix_millis(12)?,
            )?;
            let diagram_id = AgentDiagramArtifactId::from_bytes([11; 32]);
            let diagram = EvidenceDiagramArtifact::restore(
                diagram_id,
                EvidenceDiagramKind::Flowchart,
                "Aufgabenfluss".to_owned(),
                "Belegter Ablauf".to_owned(),
                "flowchart TD\n  n0[\"Aufgabe\"]\n".to_owned(),
                vec![source_id],
            )?;

            assert!(
                complete(
                    &connection,
                    worktree_id,
                    AgentSessionRevision::INITIAL,
                    &completed,
                    &answer,
                    &terminal,
                    &[AskResearchSourceId::from_bytes([8; 32])],
                    &[],
                )
                .await
                .is_err()
            );
            let unchanged =
                agent_session_repository::load(&connection, worktree_id, session_id, None, 10)
                    .await
                    .map_err(|error| error.classify())?
                    .ok_or("session missing")?;
            assert_eq!(
                unchanged.session().revision(),
                AgentSessionRevision::INITIAL
            );
            let trace = load_detail(&connection, worktree_id, session_id, user_sequence)
                .await
                .map_err(|error| error.classify())?
                .ok_or("trace missing")?;
            assert_eq!(trace.events().len(), 2);
            assert_eq!(trace.events()[1].public_note(), Some(&note));
            assert!(trace.cited_sources().is_empty());

            complete(
                &connection,
                worktree_id,
                AgentSessionRevision::INITIAL,
                &completed,
                &answer,
                &terminal,
                &[source_id],
                std::slice::from_ref(&diagram),
            )
            .await
            .map_err(|error| error.classify())?;
            let trace = load_detail(&connection, worktree_id, session_id, user_sequence)
                .await
                .map_err(|error| error.classify())?
                .ok_or("trace missing")?;
            assert_eq!(trace.events().len(), 3);
            assert_eq!(trace.cited_sources(), &[source_id]);
            let projection =
                load_projection(&connection, worktree_id, session_id, user_sequence, 50)
                    .await
                    .map_err(|error| error.classify())?
                    .ok_or("projection missing")?;
            assert_eq!(projection.detail(), &trace);
            assert_eq!(projection.source_count(), 1);
            assert_eq!(
                projection.sources().sources(),
                std::slice::from_ref(&source)
            );
            assert_eq!(
                list_diagrams(&connection, worktree_id, session_id, user_sequence)
                    .await
                    .map_err(|error| error.classify())?,
                vec![diagram.clone()]
            );
            let session_diagrams =
                list_session_diagrams(&connection, worktree_id, session_id, None, 128)
                    .await
                    .map_err(|error| error.classify())?;
            assert_eq!(session_diagrams.len(), 1);
            assert_eq!(session_diagrams[0].user_sequence(), user_sequence);
            assert_eq!(session_diagrams[0].index_run_id(), turn.index_run_id());
            assert_eq!(session_diagrams[0].snapshot_id(), turn.snapshot_id());
            assert_eq!(session_diagrams[0].artifact(), &diagram);
            assert_eq!(
                load_diagram(&connection, worktree_id, session_id, diagram_id)
                    .await
                    .map_err(|error| error.classify())?,
                Some((user_sequence, diagram.clone()))
            );
            let handoff = load_handoff_for_task(&connection, worktree_id, task_id)
                .await
                .map_err(|error| error.classify())?
                .ok_or("research handoff missing")?;
            assert_eq!(handoff.index_run_id(), turn.index_run_id());
            assert_eq!(handoff.snapshot_id(), turn.snapshot_id());
            assert_eq!(handoff.revisions(), &[source.revision().clone()]);
            let command = handoff.command().ok_or("command profile missing")?;
            assert_eq!(command.primary(), SlashCommand::Review);
            assert_eq!(command.lenses(), &[SlashCommandLens::Security]);
            assert_eq!(
                load_linked_task(&connection, worktree_id, session_id, user_sequence)
                    .await
                    .map_err(|error| error.classify())?,
                Some(task_id)
            );
            assert_eq!(
                list_sources(
                    &connection,
                    worktree_id,
                    session_id,
                    user_sequence,
                    None,
                    50
                )
                .await
                .map_err(|error| error.classify())?
                .sources(),
                &[source]
            );

            let tombstone = session(session_id, 3, AgentSessionState::Archived, 13, None, true)?;
            agent_session_repository::delete_presentation(
                &connection,
                worktree_id,
                session_id,
                AgentSessionRevision::new(2)?,
                &tombstone,
            )
            .await
            .map_err(|error| error.classify())?;
            assert!(
                load_detail(&connection, worktree_id, session_id, user_sequence)
                    .await
                    .map_err(|error| error.classify())?
                    .is_none()
            );
            assert!(
                load_handoff_for_task(&connection, worktree_id, task_id)
                    .await
                    .map_err(|error| error.classify())?
                    .is_none()
            );
            assert_eq!(
                load_linked_task(&connection, worktree_id, session_id, user_sequence)
                    .await
                    .map_err(|error| error.classify())?,
                None
            );
            assert!(
                list_diagrams(&connection, worktree_id, session_id, user_sequence)
                    .await
                    .map_err(|error| error.classify())?
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
        latest_sequence: Option<AgentSessionSequence>,
        deleted: bool,
    ) -> Result<AgentSession, Box<dyn std::error::Error>> {
        Ok(AgentSession::from_parts(
            id,
            AgentSessionRevision::new(revision)?,
            AgentSessionTitle::try_from_string("Ask research".to_owned())?,
            AgentSessionMode::Agent,
            state,
            AgentSessionTimestamp::from_unix_millis(10)?,
            AgentSessionTimestamp::from_unix_millis(updated_at)?,
            latest_sequence,
            None,
            None,
            deleted,
        ))
    }

    fn entry(
        session_id: AgentSessionId,
        sequence: AgentSessionSequence,
        kind: AgentSessionEntryKind,
        text: &str,
        created_at: u64,
    ) -> Result<AgentSessionEntry, Box<dyn std::error::Error>> {
        Ok(AgentSessionEntry::try_new(
            session_id,
            sequence,
            kind,
            AgentSessionText::try_from_string(text.to_owned())?,
            AgentSessionTimestamp::from_unix_millis(created_at)?,
            None,
            None,
            None,
        )?)
    }
}

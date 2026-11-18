//! The candidate's join between a charged read and one observed Edit wire.

use crate::{
    LibsqlKnowledgeStore, agent_recovery_repository, edit_phase_reservation_repository as edits,
    run_journal_repository::RunJournalRepositoryError,
};
use a3_application::{
    AcquiredSourceRead, BoundEditSource, CallerRelocalization, CallerRelocalizationAdmission,
    CallerRelocalizationOutcome, CallerRelocalizationReceipt, ContextToolResultStatus,
    EditPhaseAnchor, EditPhaseCallKind, EditPhaseReservationFailure as EditFailure,
    RecordedAgentRead, SourceAcquisition, SourceDeliveryProgress, SourceDeliveryScope,
    SourceReadAdmission, SourceReadFailure as Failure, SourceReadFuture, SourceReadState,
    SourceReadStore, SourceWireDelivery,
};
use a3_domain::{
    AgentRun, AgentRunId, AgentRunTimestamp, AgentToolEvidence, ContentHash, EvidenceRef,
    FileRevision, GoalContractReference, GraphEndpoint, ProjectIdentity, RepositoryPath,
    RunEventSequence, SnapshotId, SourcePosition, SourceRange, SymbolId, TaskLedgerRevision,
    TaskStepAttemptNumber, TaskStepId, ToolRunId, VerificationSpecId, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, Value, params};

pub(crate) const SCHEMA_VERSION: u32 = 53;
pub(crate) const CALLER_SCHEMA_VERSION: u32 = 54;
const MAX_READS: usize = 8;

impl SourceReadStore for LibsqlKnowledgeStore {
    fn begin_source_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
        source: &'a BoundEditSource,
        tool: ToolRunId,
        now: AgentRunTimestamp,
    ) -> SourceReadFuture<'a, SourceReadAdmission> {
        Box::pin(async move {
            if source.worktree_id() != anchor.worktree()
                || source.snapshot_id() != anchor.snapshot()
            {
                return Err(Failure::Reservation(EditFailure::StaleAnchor));
            }
            let connection = open(self, project, anchor).await?;
            let tx = transaction(&connection, TransactionBehavior::Immediate).await?;
            let result = async {
                require_admission_capacity(&tx, anchor, now).await?;
                crate::http_correction_plan_repository::no_change_window_index::guard_bound_source(
                    &tx, anchor, source,
                )
                .await
                .map_err(Failure::Reservation)?;
                let mut progress = replay(&tx, anchor).await?.progress;
                let scope = SourceDeliveryScope::from_edit_anchor(anchor);
                let required = source.required();
                let ordinal = progress
                    .begin(scope, SourceAcquisition::Read(required.revision().clone()))
                    .map_err(Failure::Progress)?
                    .get();
                let attempt = agent_recovery_repository::begin_tool_attempt_in_transaction(
                    &tx,
                    anchor.worktree(),
                    anchor.run(),
                    anchor.snapshot(),
                    tool,
                    now,
                )
                .await
                .map_err(|error| Failure::ToolAttempt(error.classify()))?;
                let key = source_key(anchor, ordinal);
                tx.execute(
                    "INSERT INTO source_read_acquisitions (
                       source_read_id,scope_digest,policy_version,run_id,step_id,attempt_number,
                       ordinal,tool_run_id,tool_attempt,after_sequence,repository_path,content_hash,
                       required_range
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                    params![
                        bytes(key),
                        bytes(scope_digest(anchor)),
                        i64::from(SourceDeliveryProgress::POLICY_VERSION),
                        anchor.run().as_bytes().to_vec(),
                        anchor.step().as_bytes().to_vec(),
                        i64::from(anchor.attempt().get()),
                        i64::from(ordinal),
                        tool.as_bytes().to_vec(),
                        i64::from(attempt.attempt().get()),
                        sequence(anchor.sequence())?,
                        required.revision().path().as_bytes().to_vec(),
                        bytes(required.revision().content_hash()),
                        encode_range(required.range()).to_vec(),
                    ],
                )
                .await
                .map_err(unavailable)?;
                Ok(SourceReadAdmission::new(key, attempt))
            }
            .await;
            close(tx, result).await
        })
    }

    fn begin_caller_relocalization<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        job: &'a CallerRelocalization,
        tool: ToolRunId,
        now: AgentRunTimestamp,
    ) -> SourceReadFuture<'a, CallerRelocalizationAdmission> {
        Box::pin(async move {
            let anchor = job.anchor();
            let connection = open(self, project, anchor).await?;
            if crate::migration::read_user_version(&connection)
                .await
                .map_err(unavailable)?
                < CALLER_SCHEMA_VERSION
            {
                return Err(Failure::UnsupportedSchema);
            }
            let tx = transaction(&connection, TransactionBehavior::Immediate).await?;
            let result = async {
                let deadline = require_admission_capacity(&tx, anchor, now).await?;
                let mut progress = replay(&tx, anchor).await?.progress;
                let scope = SourceDeliveryScope::from_edit_anchor(anchor);
                let callee = job.callee();
                let ordinal = progress
                    .begin(scope, caller_acquisition(callee))
                    .map_err(Failure::Progress)?
                    .get();
                let attempt = agent_recovery_repository::begin_tool_attempt_in_transaction(
                    &tx,
                    anchor.worktree(),
                    anchor.run(),
                    anchor.snapshot(),
                    tool,
                    now,
                )
                .await
                .map_err(|error| Failure::ToolAttempt(error.classify()))?;
                let key = source_key(anchor, ordinal);
                tx.execute(
                    "INSERT INTO source_caller_relocalizations (
                       acquisition_id,scope_digest,policy_version,run_id,step_id,attempt_number,
                       ordinal,tool_run_id,tool_attempt,after_sequence,callee_id
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![
                        bytes(key),
                        bytes(scope_digest(anchor)),
                        i64::from(SourceDeliveryProgress::POLICY_VERSION),
                        anchor.run().as_bytes().to_vec(),
                        anchor.step().as_bytes().to_vec(),
                        i64::from(anchor.attempt().get()),
                        i64::from(ordinal),
                        tool.as_bytes().to_vec(),
                        i64::from(attempt.attempt().get()),
                        sequence(anchor.sequence())?,
                        callee.as_bytes().to_vec(),
                    ],
                )
                .await
                .map_err(unavailable)?;
                Ok(CallerRelocalizationAdmission::new(
                    SourceReadAdmission::new(key, attempt),
                    anchor,
                    callee,
                    deadline,
                ))
            }
            .await;
            close(tx, result).await
        })
    }

    fn load_source_read_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
    ) -> SourceReadFuture<'a, SourceReadState> {
        Box::pin(async move {
            let connection = open(self, project, anchor).await?;
            let tx = transaction(&connection, TransactionBehavior::Deferred).await?;
            let result = load_recorded_state_in_transaction(&tx, anchor).await;
            close(tx, result).await
        })
    }

    fn abandon_source_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        anchor: EditPhaseAnchor,
        key: ContentHash,
    ) -> SourceReadFuture<'a, ()> {
        Box::pin(async move {
            let connection = open(self, project, anchor).await?;
            let tx = transaction(&connection, TransactionBehavior::Immediate).await?;
            let result = async {
                edits::validate_live(&tx, anchor).await.map_err(Failure::Reservation)?;
                let item = replay(&tx, anchor).await?.pending.filter(|item| item.key == key && item.can_abandon)
                    .ok_or(Failure::NotPending)?;
                let changed = tx.execute(
                    "UPDATE source_read_acquisitions SET abandoned=1 WHERE source_read_id=?1 AND abandoned=0",
                    params![bytes(item.key)],
                ).await.map_err(unavailable)?;
                if changed != 1 {
                    return Err(Failure::NotPending);
                }
                Ok(())
            }.await;
            close(tx, result).await
        })
    }
}

/// Same-read-view replay for the concrete retained HTTP-origin consumer; no additional admission.
pub(crate) async fn load_recorded_state_in_transaction(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
) -> Result<SourceReadState, Failure> {
    edits::validate_live(tx, anchor)
        .await
        .map_err(Failure::Reservation)?;
    let replayed = replay(tx, anchor).await?;
    Ok(SourceReadState::new(
        replayed.progress,
        replayed.pending.and_then(|item| item.acquired),
        replayed.caller_relocalizations,
    ))
}

async fn require_admission_capacity(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
    now: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, Failure> {
    let run = edits::validate_live(tx, anchor)
        .await
        .map_err(Failure::Reservation)?;
    if now < run.updated_at() {
        return Err(Failure::Reservation(EditFailure::TimestampRegressed));
    }
    let deadline = edits::run_deadline(&run).map_err(Failure::Reservation)?;
    if now >= deadline {
        return Err(Failure::Reservation(EditFailure::DeadlineReached));
    }
    let (usage, _) = edits::read_run_usage(tx, anchor.worktree(), &run)
        .await
        .map_err(Failure::Reservation)?;
    if usage.unobserved_calls != 0 || usage.unknown_calls != 0 {
        return Err(Failure::Reservation(EditFailure::UncertainProviderCall));
    }
    Ok(deadline)
}

async fn open(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    anchor: EditPhaseAnchor,
) -> Result<Connection, Failure> {
    if anchor.worktree() != project.worktree().id() {
        return Err(Failure::Reservation(EditFailure::StaleAnchor));
    }
    let db = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| Failure::Unavailable)?;
    if db.schema_version().get() < SCHEMA_VERSION {
        return Err(Failure::UnsupportedSchema);
    }
    db.connection_for_operation()
        .await
        .map_err(|_| Failure::Unavailable)
}

/// Installs the original marker only in the transaction committing the exact ToolAction.
pub(crate) async fn record_original(
    tx: &Transaction,
    worktree: WorktreeId,
    previous: RunEventSequence,
    attempt: i64,
    read: &RecordedAgentRead,
) -> Result<(), RunJournalRepositoryError> {
    use RunJournalRepositoryError as JournalFailure;
    let event = read.event();
    let result = read.context_result();
    let tool = result.tool_run_id().as_bytes().to_vec();
    let mut rows = tx
        .query(
            "SELECT run_id,after_sequence,repository_path,content_hash,required_range
           FROM source_read_acquisitions WHERE tool_run_id=?1 AND tool_attempt=?2",
            params![tool.clone(), attempt],
        )
        .await
        .map_err(JournalFailure::Read)?;
    let Some(row) = rows.next().await.map_err(JournalFailure::Read)? else {
        return Ok(());
    };
    let decode = |_| JournalFailure::InvalidStoredData;
    let owner = blob(&row, 0).map_err(decode)?;
    if owner.as_slice() != event.run_id().as_bytes()
        || integer(&row, 1).map_err(decode)? != sequence(previous).map_err(decode)?
        || agent_recovery_repository::latest_published_snapshot(tx, worktree)
            .await
            .map_err(|_| JournalFailure::InvalidStoredData)?
            != Some(event.snapshot_id())
    {
        return Err(JournalFailure::InvalidInput);
    }
    let Some(id) = read.original_evidence() else {
        return Ok(());
    };
    let required = required_evidence(&row, 2).map_err(decode)?;
    crate::http_correction_plan_repository::no_change_window_index::guard_original_commit(
        tx,
        worktree,
        event.run_id(),
        previous,
        &required,
    )
    .await
    .map_err(JournalFailure::NoChangeGuard)?;
    if result.status() != ContextToolResultStatus::Succeeded
        || !read.evidence().evidence().iter().any(|item| {
            item.id() == id
                && item.location().revision() == required.revision()
                && item
                    .location()
                    .range()
                    .is_some_and(|range| !range.is_empty() && range.contains(required.range()))
        })
    {
        return Err(JournalFailure::InvalidInput);
    }
    let changed = tx
        .execute(
            "UPDATE source_read_acquisitions SET original_evidence_id=?1
         WHERE tool_run_id=?2 AND tool_attempt=?3 AND original_evidence_id IS NULL AND abandoned=0",
            params![id.as_bytes().to_vec(), tool, attempt],
        )
        .await
        .map_err(JournalFailure::Write)?;
    if changed != 1 {
        return Err(JournalFailure::InvalidInput);
    }
    Ok(())
}

/// Records the closed result in the transaction committing the actual Graph ToolAction.
pub(crate) async fn record_caller_relocalization(
    tx: &Transaction,
    worktree: WorktreeId,
    previous: RunEventSequence,
    run: &AgentRun,
    attempt: i64,
    read: &RecordedAgentRead,
) -> Result<(), RunJournalRepositoryError> {
    use RunJournalRepositoryError as JournalFailure;
    let tool = read.context_result().tool_run_id().as_bytes().to_vec();
    let mut rows = tx
        .query(
            "SELECT acquisition_id,callee_id,run_id,after_sequence
         FROM source_caller_relocalizations WHERE tool_run_id=?1 AND tool_attempt=?2",
            params![tool, attempt],
        )
        .await
        .map_err(JournalFailure::Read)?;
    let row = rows.next().await.map_err(JournalFailure::Read)?;
    let Some(receipt) = read.caller_relocalization() else {
        return if row.is_none() {
            Ok(())
        } else {
            Err(JournalFailure::InvalidInput)
        };
    };
    let row = row.ok_or(JournalFailure::InvalidInput)?;
    let decode = |_| JournalFailure::InvalidStoredData;
    let event = read.event();
    let context = read.context_result();
    let expected = graph_observation(context.truncated(), !read.evidence().evidence().is_empty());
    if hash(&row, 0).map_err(decode)? != receipt.key()
        || symbol(&row, 1).map_err(decode)? != receipt.callee()
        || blob(&row, 2).map_err(decode)?.as_slice() != event.run_id().as_bytes()
        || integer(&row, 3).map_err(decode)? != sequence(previous).map_err(decode)?
        || agent_recovery_repository::latest_published_snapshot(tx, worktree)
            .await
            .map_err(|_| JournalFailure::InvalidStoredData)?
            != Some(event.snapshot_id())
        || event.occurred_at()
            >= edits::run_deadline(run).map_err(|_| JournalFailure::InvalidStoredData)?
        || context.status() != ContextToolResultStatus::Succeeded
        || read.original_evidence().is_some()
        || receipt.outcome() != expected
    {
        return Err(JournalFailure::InvalidInput);
    }
    let changed = tx
        .execute(
            "UPDATE source_caller_relocalizations SET observation=?1
         WHERE acquisition_id=?2 AND observation IS NULL",
            params![
                observation_text(expected).ok_or(JournalFailure::InvalidInput)?,
                bytes(receipt.key())
            ],
        )
        .await
        .map_err(JournalFailure::Write)?;
    if changed != 1 {
        return Err(JournalFailure::InvalidInput);
    }
    Ok(())
}

fn caller_acquisition(callee: SymbolId) -> SourceAcquisition {
    SourceAcquisition::Relocalize {
        need: a3_application::EditEvidenceNeed::Caller,
        target: GraphEndpoint::Symbol(callee),
    }
}

fn graph_observation(truncated: bool, has_evidence: bool) -> CallerRelocalizationOutcome {
    if truncated {
        CallerRelocalizationOutcome::Incomplete
    } else if has_evidence {
        CallerRelocalizationOutcome::CandidateMetadata
    } else {
        CallerRelocalizationOutcome::NoMatch
    }
}

fn observation_text(outcome: CallerRelocalizationOutcome) -> Option<&'static str> {
    match outcome {
        CallerRelocalizationOutcome::NoMatch => Some("no_match"),
        CallerRelocalizationOutcome::CandidateMetadata => Some("candidate_metadata"),
        CallerRelocalizationOutcome::Incomplete => Some("incomplete"),
        CallerRelocalizationOutcome::Interrupted | CallerRelocalizationOutcome::ToolFailed => None,
    }
}

struct ReplayedSources {
    progress: SourceDeliveryProgress,
    pending: Option<PendingRead>,
    caller_relocalizations: Vec<CallerRelocalizationReceipt>,
}

struct PendingRead {
    key: ContentHash,
    acquired: Option<AcquiredSourceRead>,
    can_abandon: bool,
}

/// Admission checks the stamp and the existing call kind without granting another read.
pub(crate) async fn validate_reservation(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
    job: ContentHash,
    kind: EditPhaseCallKind,
    acquired: Option<&AcquiredSourceRead>,
) -> Result<(), EditFailure> {
    let replayed = replay(tx, anchor).await.map_err(|error| match error {
        Failure::Reservation(failure) => failure,
        Failure::Unavailable => EditFailure::Unavailable,
        _ => EditFailure::InvalidStoredData,
    })?;
    let pending = replayed.pending;
    if pending.as_ref().and_then(|item| item.acquired.as_ref()) != acquired
        || (pending.is_some() && acquired.is_none())
    {
        return Err(EditFailure::StaleAnchor);
    }
    if kind == EditPhaseCallKind::StructuralRepair {
        let mut rows = tx
            .query(
                "SELECT source_read_id FROM edit_phase_reservations
             WHERE run_id=?1 AND job_digest=?2 AND call_kind='initial'",
                params![anchor.run().as_bytes().to_vec(), bytes(job)],
            )
            .await
            .map_err(|_| EditFailure::Unavailable)?;
        let row = rows
            .next()
            .await
            .map_err(|_| EditFailure::Unavailable)?
            .ok_or(EditFailure::RepairNotAvailable)?;
        let initial: Option<Vec<u8>> =
            optional_blob(&row, 0).map_err(|_| EditFailure::InvalidStoredData)?;
        if initial != acquired.map(|item| bytes(item.key())) {
            return Err(EditFailure::RepairNotAvailable);
        }
    }
    Ok(())
}

/// Exact known Original for the narrow terminal NoChange consumer. Unlike replay this
/// never invents an accepted wire/delivery and never admits an abandoned/historical read.
pub(crate) async fn no_change_original(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
    key: ContentHash,
) -> Result<EvidenceRef, EditFailure> {
    let result = async {
        let sql = format!("{READ_ROWS} AND s.source_read_id=?4 LIMIT 2");
        let mut rows = tx
            .query(
                &sql,
                params![
                    anchor.run().as_bytes().to_vec(),
                    anchor.step().as_bytes().to_vec(),
                    i64::from(anchor.attempt().get()),
                    bytes(key)
                ],
            )
            .await
            .map_err(unavailable)?;
        let row = rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(Failure::InvalidStoredData)?;
        let ordinal = u8::try_from(integer(&row, 3)?).map_err(|_| Failure::InvalidStoredData)?;
        let after = integer(&row, 26)?;
        // Native libSQL Row reads the shared statement cursor. Decode and check
        // this complete row before stepping the cursor for the uniqueness lookahead.
        if hash(&row, 0)? != key
            || hash(&row, 1)? != scope_digest(anchor)
            || integer(&row, 2)? != i64::from(SourceDeliveryProgress::POLICY_VERSION)
            || !(1..=MAX_READS as u8).contains(&ordinal)
            || key != source_key(anchor, ordinal)
            || integer(&row, 7)? != 0
            || blob(&row, 9)?.as_slice() != anchor.run().as_bytes()
            || blob(&row, 10)?.as_slice() != anchor.snapshot().as_bytes()
            || after.checked_add(1) != Some(sequence(anchor.sequence())?)
        {
            return Err(Failure::InvalidStoredData);
        }
        let original = original_evidence(&row, anchor, &string(&row, 8)?, after)?;
        let required = required_evidence(&row, 4)?;
        if original.revision() != required.revision()
            || original.range().is_empty()
            || !original.range().contains(required.range())
        {
            return Err(Failure::InvalidStoredData);
        }
        if rows.next().await.map_err(unavailable)?.is_some() {
            return Err(Failure::InvalidStoredData);
        }
        drop((row, rows));
        let mut event = tx
            .query(
                "SELECT 1 FROM run_events j JOIN source_read_acquisitions s
            ON s.run_id=j.run_id AND j.event_sequence=s.after_sequence+1
            WHERE s.source_read_id=?1 AND j.event_kind='tool_action'
              AND j.subject_kind='tool' AND j.subject_id=s.tool_run_id
              AND j.payload_outcome='succeeded' AND j.snapshot_id=?2",
                params![bytes(key), anchor.snapshot().as_bytes().to_vec()],
            )
            .await
            .map_err(unavailable)?;
        if event.next().await.map_err(unavailable)?.is_none() {
            return Err(Failure::InvalidStoredData);
        }
        Ok(original)
    }
    .await;
    result.map_err(|error| match error {
        Failure::Reservation(failure) => failure,
        Failure::Unavailable => EditFailure::Unavailable,
        _ => EditFailure::InvalidStoredData,
    })
}

/// Metadata only: no accepted SourceWireDelivery or acquisition capability is returned.
pub(crate) struct ClosedNoChangeOriginal {
    pub(crate) original: EvidenceRef,
    pub(crate) tool: ToolRunId,
}

/// The only abandoned-read purpose here. Its constructor is private to the SQL
/// closed-fact loader and requires the actual A closure before this function runs.
pub(crate) async fn closed_no_change_original(
    tx: &Transaction,
    purpose: &crate::http_correction_plan_repository::closed_no_change::ClosedReadPurpose<'_>,
) -> Result<ClosedNoChangeOriginal, EditFailure> {
    let result = async {
        let fact = purpose.fact(); let a = fact.owner(); let key = fact.source_read_key();
        let fields = SourceScopeFields { worktree:a.worktree,goal:a.goal,ledger:a.ledger,run:a.run,
            step:a.step,attempt:a.attempt,verification:a.verification,snapshot:a.snapshot };
        let scope = scope_digest_fields(&fields);
        let mut rows = tx.query(&format!("{READ_ROWS} AND s.source_read_id=?4 LIMIT 2"), params![
            a.run.as_bytes().to_vec(),a.step.as_bytes().to_vec(),i64::from(a.attempt.get()),bytes(key)])
            .await.map_err(unavailable)?;
        let row = rows.next().await.map_err(unavailable)?.ok_or(Failure::InvalidStoredData)?;
        let after = integer(&row,26)?;
        if hash(&row,0)?!=key || hash(&row,1)?!=scope
            || integer(&row,2)?!=i64::from(SourceDeliveryProgress::POLICY_VERSION)
            || integer(&row,3)?!=1 || key!=source_key_from_scope(scope,1)
            || integer(&row,7)?!=1 || blob(&row,9)?.as_slice()!=a.run.as_bytes()
            || blob(&row,10)?.as_slice()!=a.snapshot.as_bytes()
            || after.checked_add(1)!=Some(sequence(a.sequence)?)
            || a.sequence.get().checked_add(1)!=Some(purpose.closure().sequence().get()) {
            return Err(Failure::InvalidStoredData);
        }
        let original=original_evidence_for_owner(&row,a.run,a.snapshot,&string(&row,8)?,after)?;
        let required=required_evidence(&row,4)?;
        if original!=required || &original!=fact.window().original() { return Err(Failure::InvalidStoredData); }
        // All first-row values are decoded before advancing the shared native cursor.
        if rows.next().await.map_err(unavailable)?.is_some() { return Err(Failure::InvalidStoredData); }
        drop((row,rows));
        let mut rows=tx.query("SELECT s.tool_run_id,
            (SELECT count(*) FROM source_read_acquisitions WHERE run_id=s.run_id AND step_id=s.step_id),
            (SELECT count(*) FROM source_caller_relocalizations WHERE run_id=s.run_id AND step_id=s.step_id)
            FROM source_read_acquisitions s JOIN run_events j ON j.run_id=s.run_id
            AND j.event_sequence=s.after_sequence+1
            JOIN run_events b ON b.run_id=s.run_id AND b.event_sequence=s.after_sequence
            JOIN tool_run_attempts a ON a.tool_run_id=s.tool_run_id AND a.attempt_sequence=s.tool_attempt
            WHERE s.source_read_id=?1 AND s.abandoned=1
            AND a.started_at_unix_millis>=b.occurred_at_unix_millis
            AND a.updated_at_unix_millis=j.occurred_at_unix_millis
            AND a.started_at_unix_millis<=a.updated_at_unix_millis
            AND j.event_kind='tool_action' AND j.subject_kind='tool'
            AND j.subject_id=s.tool_run_id AND j.payload_outcome='succeeded' AND j.snapshot_id=?2",
            params![bytes(key),a.snapshot.as_bytes().to_vec()]).await.map_err(unavailable)?;
        let row=rows.next().await.map_err(unavailable)?.ok_or(Failure::InvalidStoredData)?;
        if integer(&row,1)?!=1 || integer(&row,2)?!=0 { return Err(Failure::InvalidStoredData); }
        let tool=ToolRunId::from_bytes(blob(&row,0)?.try_into().map_err(|_|Failure::InvalidStoredData)?);
        Ok(ClosedNoChangeOriginal {original,tool})
    }.await;
    result.map_err(|error| match error {
        Failure::Reservation(error) => error,
        Failure::Unavailable => EditFailure::Unavailable,
        _ => EditFailure::InvalidStoredData,
    })
}

const READ_ROWS: &str =
    "SELECT s.source_read_id,s.scope_digest,s.policy_version,s.ordinal,
            s.repository_path,s.content_hash,s.required_range,s.abandoned,
            a.status,a.run_id,a.snapshot_id,s.original_evidence_id,
            t.run_id,t.event_sequence,t.status,t.snapshot_before_id,t.snapshot_after_id,
            e.location_kind,e.repository_path,e.content_hash,
            e.start_byte,e.end_byte,e.start_row,e.start_column,e.end_row,e.end_column,
            s.after_sequence,'read',NULL,NULL,NULL,NULL
     FROM source_read_acquisitions s
     LEFT JOIN tool_run_attempts a ON a.tool_run_id=s.tool_run_id AND a.attempt_sequence=s.tool_attempt
     LEFT JOIN tool_runs t ON t.tool_run_id=s.tool_run_id
     LEFT JOIN tool_evidence e ON e.tool_run_id=s.tool_run_id AND e.evidence_id=s.original_evidence_id
     WHERE s.run_id=?1 AND s.step_id=?2 AND s.attempt_number=?3";

const CALLER_ROWS: &str =
    "UNION ALL SELECT s.acquisition_id,s.scope_digest,s.policy_version,s.ordinal,
            NULL,NULL,NULL,NULL,
            a.status,a.run_id,a.snapshot_id,NULL,
            t.run_id,t.event_sequence,t.status,t.snapshot_before_id,t.snapshot_after_id,
            NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,
            s.after_sequence,'caller',s.callee_id,s.observation,t.result_truncated,
            EXISTS(SELECT 1 FROM tool_evidence e WHERE e.tool_run_id=s.tool_run_id)
     FROM source_caller_relocalizations s
     LEFT JOIN tool_run_attempts a ON a.tool_run_id=s.tool_run_id AND a.attempt_sequence=s.tool_attempt
     LEFT JOIN tool_runs t ON t.tool_run_id=s.tool_run_id
     WHERE s.run_id=?1 AND s.step_id=?2 AND s.attempt_number=?3";

async fn replay(tx: &Transaction, anchor: EditPhaseAnchor) -> Result<ReplayedSources, Failure> {
    let scope = SourceDeliveryScope::from_edit_anchor(anchor);
    let mut progress = SourceDeliveryProgress::new(scope);
    let mut pending = None;
    let mut caller_relocalizations = Vec::new();
    let caller_rows = if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        >= CALLER_SCHEMA_VERSION
    {
        CALLER_ROWS
    } else {
        ""
    };
    let mut rows = tx
        .query(
            &format!("{READ_ROWS} {caller_rows} ORDER BY 4 LIMIT ?4"),
            params![
                anchor.run().as_bytes().to_vec(),
                anchor.step().as_bytes().to_vec(),
                i64::from(anchor.attempt().get()),
                (MAX_READS + 1) as i64,
            ],
        )
        .await
        .map_err(unavailable)?;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        if pending.is_some() || progress.receipts().len() >= MAX_READS {
            return Err(Failure::InvalidStoredData);
        }
        if hash(&row, 1)? != scope_digest(anchor) {
            return Err(Failure::Reservation(EditFailure::StaleAnchor));
        }
        if integer(&row, 2)? != i64::from(SourceDeliveryProgress::POLICY_VERSION)
            || blob(&row, 9)?.as_slice() != anchor.run().as_bytes()
            || blob(&row, 10)?.as_slice() != anchor.snapshot().as_bytes()
        {
            return Err(Failure::InvalidStoredData);
        }
        let kind = string(&row, 27)?;
        let (acquisition, required, callee) = match kind.as_str() {
            "read" => {
                let required = required_evidence(&row, 4)?;
                (
                    SourceAcquisition::Read(required.revision().clone()),
                    Some(required),
                    None,
                )
            }
            "caller" => {
                let callee = symbol(&row, 28)?;
                (caller_acquisition(callee), None, Some(callee))
            }
            _ => return Err(Failure::InvalidStoredData),
        };
        let ordinal = progress
            .begin(scope, acquisition)
            .map_err(|_| Failure::InvalidStoredData)?;
        let key = hash(&row, 0)?;
        if integer(&row, 3)? != i64::from(ordinal.get()) || key != source_key(anchor, ordinal.get())
        {
            return Err(Failure::InvalidStoredData);
        }
        let status = string(&row, 8)?;
        let terminal = match status.as_str() {
            "in_flight" => false,
            "succeeded" | "failed" | "cancelled" | "denied" | "interrupted" => true,
            _ => return Err(Failure::InvalidStoredData),
        };
        let after = integer(&row, 26)?;
        if let Some(callee) = callee {
            if let Some(outcome) = replay_caller_observation(&row, anchor, &status, after)? {
                progress
                    .finish(scope, ordinal, None)
                    .map_err(|_| Failure::InvalidStoredData)?;
                caller_relocalizations.push(CallerRelocalizationReceipt::new(key, callee, outcome));
            } else {
                pending = Some(PendingRead {
                    key,
                    acquired: None,
                    can_abandon: false,
                });
            }
            continue;
        }
        let required = required.ok_or(Failure::InvalidStoredData)?;
        let acquired = if optional_blob(&row, 11)?.is_some() {
            Some(AcquiredSourceRead::from_recorded(
                key,
                scope,
                required,
                original_evidence(&row, anchor, &status, after)?,
            )?)
        } else {
            None
        };
        let wire = linked_wires(tx, anchor, key, after).await?;
        let abandoned = integer(&row, 7)?;
        if abandoned == 1 {
            if !terminal || wire.accepted.is_some() || wire.uncertain {
                return Err(Failure::InvalidStoredData);
            }
            progress
                .finish(scope, ordinal, None)
                .map_err(|_| Failure::InvalidStoredData)?;
        } else if let Some(digest) = wire.accepted {
            let acquired = acquired.ok_or(Failure::InvalidStoredData)?;
            let delivery = SourceWireDelivery::from_recorded_dispatch(
                anchor.snapshot(),
                acquired.original().clone(),
                digest,
            );
            progress
                .finish(scope, ordinal, Some(delivery))
                .map_err(|_| Failure::InvalidStoredData)?;
        } else {
            if abandoned != 0 || (wire.calls != 0 && acquired.is_none()) {
                return Err(Failure::InvalidStoredData);
            }
            pending = Some(PendingRead {
                key,
                acquired: if (wire.calls == 0 || wire.repair_allowed)
                    && after.checked_add(1) == Some(sequence(anchor.sequence())?)
                {
                    acquired
                } else {
                    None
                },
                can_abandon: terminal && !wire.uncertain,
            });
        }
    }
    Ok(ReplayedSources {
        progress,
        pending,
        caller_relocalizations,
    })
}

fn replay_caller_observation(
    row: &Row,
    anchor: EditPhaseAnchor,
    status: &str,
    after: i64,
) -> Result<Option<CallerRelocalizationOutcome>, Failure> {
    let observation = optional_string(row, 29)?;
    if let Some(observation) = observation {
        if status != "succeeded"
            || string(row, 14)? != status
            || blob(row, 12)?.as_slice() != anchor.run().as_bytes()
            || after.checked_add(1) != Some(integer(row, 13)?)
            || blob(row, 15)?.as_slice() != anchor.snapshot().as_bytes()
            || blob(row, 16)?.as_slice() != anchor.snapshot().as_bytes()
        {
            return Err(Failure::InvalidStoredData);
        }
        let outcome = graph_observation(integer(row, 30)? == 1, integer(row, 31)? == 1);
        if observation_text(outcome) != Some(observation.as_str()) {
            return Err(Failure::InvalidStoredData);
        }
        return Ok(Some(outcome));
    }
    match status {
        "in_flight" => Ok(None),
        "interrupted" => Ok(Some(CallerRelocalizationOutcome::Interrupted)),
        "failed" | "cancelled" | "denied" => Ok(Some(CallerRelocalizationOutcome::ToolFailed)),
        _ => Err(Failure::InvalidStoredData),
    }
}

#[derive(Default)]
struct LinkedWires {
    calls: u8,
    accepted: Option<ContentHash>,
    uncertain: bool,
    repair_allowed: bool,
}

async fn linked_wires(
    tx: &Transaction,
    anchor: EditPhaseAnchor,
    key: ContentHash,
    after: i64,
) -> Result<LinkedWires, Failure> {
    let mut rows = tx
        .query(
            "SELECT call_kind,job_digest,outcome,wire_digest,
           worktree_id=?2 AND run_id=?3 AND task_id=?4 AND goal_revision=?5
             AND ledger_revision=?6 AND step_id=?7 AND attempt_number=?8
             AND verification_id=?9 AND snapshot_id=?10 AND run_sequence=?11
         FROM edit_phase_reservations WHERE source_read_id=?1
         ORDER BY call_kind LIMIT 3",
            params![
                bytes(key),
                anchor.worktree().as_bytes().to_vec(),
                anchor.run().as_bytes().to_vec(),
                anchor.goal().task_id().as_bytes().to_vec(),
                i64::from(anchor.goal().revision().get()),
                i64::from(anchor.ledger().get()),
                anchor.step().as_bytes().to_vec(),
                i64::from(anchor.attempt().get()),
                anchor.verification().as_bytes().to_vec(),
                anchor.snapshot().as_bytes().to_vec(),
                after.checked_add(1).ok_or(Failure::InvalidStoredData)?,
            ],
        )
        .await
        .map_err(unavailable)?;
    let mut wire = LinkedWires::default();
    let mut initial_job = None;
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        let kind = string(&row, 0)?;
        let job = hash(&row, 1)?;
        if integer(&row, 4)? != 1 {
            return Err(Failure::InvalidStoredData);
        }
        match (wire.calls, kind.as_str()) {
            (0, "initial") => initial_job = Some(job),
            (1, "structural_repair") if wire.repair_allowed && initial_job == Some(job) => {}
            _ => return Err(Failure::InvalidStoredData),
        }
        let outcome = optional_string(&row, 2)?;
        wire.repair_allowed = wire.calls == 0 && outcome.as_deref() == Some("invalid_structure");
        wire.calls += 1;
        match outcome.as_deref() {
            Some("accepted_reply") => wire.accepted = Some(hash(&row, 3)?),
            None | Some("unknown") => wire.uncertain = true,
            Some(
                "invalid_structure" | "rejected_reply" | "provider_failure" | "cancelled" | "stale"
                | "budget_overrun",
            ) => {}
            _ => return Err(Failure::InvalidStoredData),
        }
    }
    Ok(wire)
}

fn required_evidence(row: &Row, first: i32) -> Result<EvidenceRef, Failure> {
    let revision = revision(row, first)?;
    let encoded: [u8; 24] = blob(row, first + 2)?
        .try_into()
        .map_err(|_| Failure::InvalidStoredData)?;
    let mut values = [0_u32; 6];
    for (out, chunk) in values.iter_mut().zip(encoded.chunks_exact(4)) {
        *out = u32::from_le_bytes(chunk.try_into().map_err(|_| Failure::InvalidStoredData)?);
    }
    Ok(EvidenceRef::new(revision, range(values)?))
}

fn original_evidence(
    row: &Row,
    anchor: EditPhaseAnchor,
    status: &str,
    after: i64,
) -> Result<EvidenceRef, Failure> {
    original_evidence_for_owner(row, anchor.run(), anchor.snapshot(), status, after)
}

fn original_evidence_for_owner(
    row: &Row,
    run: AgentRunId,
    snapshot: SnapshotId,
    status: &str,
    after: i64,
) -> Result<EvidenceRef, Failure> {
    if status != "succeeded"
        || string(row, 14)? != status
        || blob(row, 12)?.as_slice() != run.as_bytes()
        || after.checked_add(1) != Some(integer(row, 13)?)
        || blob(row, 15)?.as_slice() != snapshot.as_bytes()
        || blob(row, 16)?.as_slice() != snapshot.as_bytes()
        || string(row, 17)? != "span"
    {
        return Err(Failure::InvalidStoredData);
    }
    let mut values = [0_u32; 6];
    for (index, value) in (20..26).zip(values.iter_mut()) {
        *value = u32::try_from(integer(row, index)?).map_err(|_| Failure::InvalidStoredData)?;
    }
    let evidence = EvidenceRef::new(revision(row, 18)?, range(values)?);
    if blob(row, 11)?.as_slice()
        != AgentToolEvidence::for_span(evidence.clone())
            .id()
            .as_bytes()
    {
        return Err(Failure::InvalidStoredData);
    }
    Ok(evidence)
}

fn revision(row: &Row, index: i32) -> Result<FileRevision, Failure> {
    let path = RepositoryPath::try_from_bytes(blob(row, index)?)
        .map_err(|_| Failure::InvalidStoredData)?;
    Ok(FileRevision::new(path, hash(row, index + 1)?))
}

fn range(
    [start, end, start_row, start_column, end_row, end_column]: [u32; 6],
) -> Result<SourceRange, Failure> {
    SourceRange::new(
        start as usize,
        end as usize,
        SourcePosition::new(start_row, start_column),
        SourcePosition::new(end_row, end_column),
    )
    .map_err(|_| Failure::InvalidStoredData)
}

fn encode_range(range: SourceRange) -> [u8; 24] {
    let fields = [
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column(),
    ];
    let mut output = [0; 24];
    for (dest, field) in output.chunks_exact_mut(4).zip(fields) {
        dest.copy_from_slice(&field.to_le_bytes());
    }
    output
}

struct SourceScopeFields {
    worktree: WorktreeId,
    goal: GoalContractReference,
    ledger: TaskLedgerRevision,
    run: AgentRunId,
    step: TaskStepId,
    attempt: TaskStepAttemptNumber,
    verification: VerificationSpecId,
    snapshot: SnapshotId,
}
fn scope_digest(anchor: EditPhaseAnchor) -> ContentHash {
    scope_digest_fields(&SourceScopeFields {
        worktree: anchor.worktree(),
        goal: anchor.goal(),
        ledger: anchor.ledger(),
        run: anchor.run(),
        step: anchor.step(),
        attempt: anchor.attempt(),
        verification: anchor.verification(),
        snapshot: anchor.snapshot(),
    })
}
fn scope_digest_fields(a: &SourceScopeFields) -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3 source acquisition scope v1");
    hash.update(a.worktree.as_bytes());
    hash.update(a.goal.task_id().as_bytes());
    hash.update(&a.goal.revision().get().to_le_bytes());
    hash.update(&a.ledger.get().to_le_bytes());
    hash.update(a.run.as_bytes());
    hash.update(a.step.as_bytes());
    hash.update(&a.attempt.get().to_le_bytes());
    hash.update(a.verification.as_bytes());
    hash.update(a.snapshot.as_bytes());
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}
fn source_key(anchor: EditPhaseAnchor, ordinal: u8) -> ContentHash {
    source_key_from_scope(scope_digest(anchor), ordinal)
}
fn source_key_from_scope(scope: ContentHash, ordinal: u8) -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3 source acquisition ordinal v1");
    hash.update(scope.as_bytes());
    hash.update(&[ordinal]);
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}

fn sequence(sequence: RunEventSequence) -> Result<i64, Failure> {
    i64::try_from(sequence.get()).map_err(|_| Failure::InvalidStoredData)
}

fn bytes(hash: ContentHash) -> Vec<u8> {
    hash.as_bytes().to_vec()
}

fn symbol(row: &Row, index: i32) -> Result<SymbolId, Failure> {
    blob(row, index)?
        .try_into()
        .map(SymbolId::from_bytes)
        .map_err(|_| Failure::InvalidStoredData)
}

fn hash(row: &Row, index: i32) -> Result<ContentHash, Failure> {
    blob(row, index)?
        .try_into()
        .map(ContentHash::from_bytes)
        .map_err(|_| Failure::InvalidStoredData)
}

fn integer(row: &Row, index: i32) -> Result<i64, Failure> {
    match row.get_value(index).map_err(unavailable)? {
        Value::Integer(value) => Ok(value),
        _ => Err(Failure::InvalidStoredData),
    }
}

fn blob(row: &Row, index: i32) -> Result<Vec<u8>, Failure> {
    optional_blob(row, index)?.ok_or(Failure::InvalidStoredData)
}

fn optional_blob(row: &Row, index: i32) -> Result<Option<Vec<u8>>, Failure> {
    match row.get_value(index).map_err(unavailable)? {
        Value::Blob(value) => Ok(Some(value)),
        Value::Null => Ok(None),
        _ => Err(Failure::InvalidStoredData),
    }
}

fn string(row: &Row, index: i32) -> Result<String, Failure> {
    optional_string(row, index)?.ok_or(Failure::InvalidStoredData)
}

fn optional_string(row: &Row, index: i32) -> Result<Option<String>, Failure> {
    match row.get_value(index).map_err(unavailable)? {
        Value::Text(value) => Ok(Some(value)),
        Value::Null => Ok(None),
        _ => Err(Failure::InvalidStoredData),
    }
}

fn unavailable(_: libsql::Error) -> Failure {
    Failure::Unavailable
}

async fn transaction(
    connection: &Connection,
    behavior: TransactionBehavior,
) -> Result<Transaction, Failure> {
    connection
        .transaction_with_behavior(behavior)
        .await
        .map_err(unavailable)
}

async fn close<T>(tx: Transaction, result: Result<T, Failure>) -> Result<T, Failure> {
    match result {
        Ok(value) => {
            tx.commit().await.map_err(unavailable)?;
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.map_err(unavailable)?;
            Err(error)
        }
    }
}

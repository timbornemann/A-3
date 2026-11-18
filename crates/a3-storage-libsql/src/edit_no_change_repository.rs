//! Narrow content-free NoChange fact and its existing atomic failure CAS guard.
use crate::{
    LibsqlKnowledgeStore, edit_phase_reservation_repository as edits, goal_contract_repository,
    source_read_repository, task_ledger_repository,
};
use a3_application::{
    EditNoChangeReceipt, EditNoChangeWindow, EditPhaseAnchor, EditPhaseCallKind,
    EditPhaseObservation, EditPhaseObservationOutcome as Outcome, EditPhaseReservation,
    EditPhaseReservationFailure as Failure, EditPhaseWireCost, PreparedEditPhase,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentBudgetDimension, AgentRun, AgentRunTimestamp, ContentHash, EvidenceRef, GoalContract,
    GoalOriginalDigest, IndexRunId, ModelProfile, ModelProfileReference, ProjectIdentity,
    PublishedIndex, RunEvent, RunEventSequence, TaskLedger, WorktreeId,
};
use libsql::{Connection, Row, Transaction, TransactionBehavior, params};
use std::time::Instant;

const SCHEMA: u32 = 67;

struct Metadata<'a> {
    window: &'a EditNoChangeWindow,
    original: &'a EvidenceRef,
    index: IndexRunId,
    profile: ModelProfileReference,
    generation: u32,
    context: u32,
    original_goal: GoalOriginalDigest,
}
impl<'a> Metadata<'a> {
    fn receipt(receipt: &'a EditNoChangeReceipt) -> Self {
        Self {
            window: receipt.window(),
            original: receipt.original(),
            index: receipt.index(),
            profile: receipt.profile(),
            generation: receipt.generation_limit(),
            context: receipt.context_limit(),
            original_goal: receipt.original_goal(),
        }
    }
}

async fn open(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
) -> Result<Connection, Failure> {
    let db = store
        .open_project_knowledge_for_run_journal(project)
        .await
        .map_err(|_| Failure::Unavailable)?;
    if db.schema_version().get() < SCHEMA {
        return Err(Failure::UnsupportedSchema);
    }
    db.connection_for_operation()
        .await
        .map_err(|_| Failure::Unavailable)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    permit: EditPhaseReservation,
    observation: EditPhaseObservation,
    window: &EditNoChangeWindow,
    current: &PublishedIndex,
    profile: &ModelProfile,
    started: Instant,
) -> Result<(), Failure> {
    if observation.outcome() != Outcome::NoChange
        || permit.anchor().worktree() != project.worktree().id()
    {
        return Err(Failure::ObservationConflict);
    }
    if observation.finished_at() < permit.reserved_at() {
        return Err(Failure::TimestampRegressed);
    }
    let origin = observation.finished_at();
    let connection = open(store, project).await?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(unavailable)?;
    let result = async {
        ensure_anchor(&tx, permit.anchor(), permit.job_digest(), permit.kind()).await?;
        let stored = read_permit_row(
            &tx,
            permit.anchor().run(),
            permit.job_digest(),
            permit.kind(),
        )
        .await?
        .ok_or(Failure::ObservationConflict)?;
        if !stored.matches_permit(&permit) || stored.outcome.is_some() || stored.no_change {
            return Err(Failure::ObservationConflict);
        }
        // The request fingerprint is only recorded provenance, never a reconstructed parent pass.
        let observed_at = edits::current_time(origin, Some(started))?;
        let observation = EditPhaseObservation::new(
            Outcome::NoChange,
            observation.reported_prompt_tokens(),
            observation.reported_output_tokens(),
            observed_at,
        );
        let checked = EditPhaseObservation::new(
            Outcome::AcceptedReply,
            observation.reported_prompt_tokens(),
            observation.reported_output_tokens(),
            observed_at,
        );
        let (classified, mut reject) = if edits::known_terminal(observation) {
            edits::classify_observation(&tx, &permit, checked).await?
        } else {
            ("unknown", Some(Failure::IncompleteProviderUsage))
        };
        let mut outcome = if classified == "accepted_reply" {
            "rejected_reply"
        } else {
            classified
        };
        let mut metadata = None;
        if classified == "accepted_reply" {
            let authenticated = async {
                let a = permit.anchor();
                let run = edits::validate_live(&tx, a).await?;
                let original = source_read_repository::no_change_original(
                    &tx,
                    a,
                    permit.source_read_key().ok_or(Failure::InvalidStoredData)?,
                )
                .await?;
                let goal = goal(&tx, a).await?;
                let original_goal = goal.original().ok_or(Failure::InvalidStoredData)?.digest();
                let m = Metadata {
                    window,
                    original: &original,
                    index: current.run().id(),
                    profile: profile.reference(),
                    generation: profile.settings().output_limit().get(),
                    context: profile.settings().context_limit().get(),
                    original_goal,
                };
                check_shape(&permit, observation, &m)?;
                if !profile.executable_actions_enabled()
                    || profile.reference() != a.model_profile()
                    || current.run().snapshot_id() != a.snapshot()
                    || !current
                        .publication()
                        .graph()
                        .files()
                        .contains(window.revision())
                {
                    return Err(Failure::StaleAnchor);
                }
                ensure_index(&tx, a, m.index, window).await?;
                check_prior(&tx, &permit, m.generation, m.context).await?;
                check_shared(
                    &tx,
                    &run,
                    a.worktree(),
                    edits::current_time(origin, Some(started))?,
                    None,
                    1,
                )
                .await?;
                Ok((original, original_goal))
            }
            .await;
            match authenticated {
                Ok(parts) => metadata = Some(parts),
                Err(error) => {
                    outcome = match error {
                        Failure::StaleAnchor
                        | Failure::DeadlineReached
                        | Failure::TimestampRegressed
                        | Failure::MutationRecoveryRequired => "stale",
                        _ => "unknown",
                    };
                    reject = Some(error);
                }
            }
        }
        let before_cas = edits::current_time(origin, Some(started))?;
        if before_cas >= permit.deadline() && reject.is_none() && edits::known_terminal(observation)
        {
            outcome = "stale";
            reject = Some(Failure::DeadlineReached);
            metadata = None;
        }
        tx.execute("SAVEPOINT edit_no_change_terminal", ())
            .await
            .map_err(unavailable)?;
        // Only an authenticated receipt may touch the append-once terminal tag.
        let sql = if metadata.is_some() {
            "UPDATE edit_phase_reservations SET outcome=?1,finished_at=?2,
             reported_prompt=?3,reported_output=?4,terminal_reply_kind='no_change'
             WHERE run_id=?5 AND job_digest=?6 AND call_kind=?7 AND outcome IS NULL"
        } else {
            "UPDATE edit_phase_reservations SET outcome=?1,finished_at=?2,
             reported_prompt=?3,reported_output=?4
             WHERE run_id=?5 AND job_digest=?6 AND call_kind=?7 AND outcome IS NULL"
        };
        let changed = tx
            .execute(
                sql,
                params![
                    outcome,
                    time(before_cas)?,
                    observation.reported_prompt_tokens().map(i64::from),
                    observation.reported_output_tokens().map(i64::from),
                    permit.anchor().run().as_bytes().to_vec(),
                    bytes(permit.job_digest()),
                    edits::call_kind_text(permit.kind())
                ],
            )
            .await
            .map_err(unavailable)?;
        if changed != 1 {
            return Err(Failure::ObservationConflict);
        }
        if let Some((original, original_goal)) = metadata {
            let m = Metadata {
                window,
                original: &original,
                index: current.run().id(),
                profile: profile.reference(),
                generation: profile.settings().output_limit().get(),
                context: profile.settings().context_limit().get(),
                original_goal,
            };
            insert_metadata(&tx, &permit, &m).await?;
        }
        let last_at = edits::current_time(origin, Some(started))?;
        if last_at >= permit.deadline() && reject.is_none() && edits::known_terminal(observation) {
            // Preserve append-once triggers by discarding only this uncommitted finish.
            tx.execute("ROLLBACK TO edit_no_change_terminal", ())
                .await
                .map_err(unavailable)?;
            tx.execute("RELEASE edit_no_change_terminal", ())
                .await
                .map_err(unavailable)?;
            edits::settle_stale(&tx, &permit, observation, last_at).await?;
            reject = Some(Failure::DeadlineReached);
        }
        // COMMIT closes any still-owned savepoint without another await after the last clock check.
        Ok(reject)
    }
    .await;
    close(tx, result).await?.map_or(Ok(()), Err)
}

pub(crate) async fn load(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    phase: &PreparedEditPhase,
    profile: &ModelProfile,
    current: &PublishedIndex,
    now: AgentRunTimestamp,
) -> Result<Option<EditNoChangeReceipt>, Failure> {
    let a = phase.anchor();
    if a.worktree() != project.worktree().id() {
        return Err(Failure::StaleAnchor);
    }
    let connection = open(store, project).await?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(unavailable)?;
    let result=async {
        let run=edits::validate_live(&tx,a).await?;
        let mut rows=tx.query("SELECT e.call_kind,n.run_id IS NOT NULL FROM edit_phase_reservations e
            LEFT JOIN edit_phase_no_change_receipts n ON n.run_id=e.run_id AND n.job_digest=e.job_digest
                AND n.call_kind=e.call_kind WHERE e.run_id=?1 AND e.job_digest=?2
                AND (e.terminal_reply_kind='no_change' OR n.run_id IS NOT NULL)
            ORDER BY e.call_kind LIMIT 3",
            params![a.run().as_bytes().to_vec(),bytes(phase.job_digest())]).await.map_err(unavailable)?;
        let Some(row)=rows.next().await.map_err(unavailable)? else{return Ok(None)};
        let kind=kind(&row,0)?;
        if number(&row,1)?!=1{return Err(Failure::InvalidStoredData)}
        if rows.next().await.map_err(unavailable)?.is_some(){return Err(Failure::InvalidStoredData)}
        drop((row,rows));
        ensure_anchor(&tx,a,phase.job_digest(),kind).await?;
        let row=read_permit_row(&tx,a.run(),phase.job_digest(),kind).await?.ok_or(Failure::InvalidStoredData)?;
        if row.outcome.as_deref()!=Some("rejected_reply") || !row.no_change{return Err(Failure::InvalidStoredData)}
        let permit=row.permit(phase,kind)?;
        let observed=row.observation()?;
        let original=source_read_repository::no_change_original(&tx,a,
            permit.source_read_key().ok_or(Failure::InvalidStoredData)?).await?;
        let goal=goal(&tx,a).await?;
        let original_goal=goal.original().ok_or(Failure::InvalidStoredData)?.digest();
        let receipt=EditNoChangeReceipt::from_recorded(phase,&permit,observed,original,profile,current,original_goal)?;
        let m=Metadata::receipt(&receipt);
        check_shape(&permit,observed,&m)?;
        ensure_metadata(&tx,&permit,&m).await?;
        ensure_index(&tx,a,receipt.index(),receipt.window()).await?;
        check_prior(&tx,&permit,receipt.generation_limit(),receipt.context_limit()).await?;
        check_shared(&tx,&run,a.worktree(),now,Some(receipt.finished_at()),0).await?;
        Ok(Some(receipt))
    }.await;
    close(tx, result).await
}

/// The existing Immediate Ledger/Journal transaction invokes this before any
/// projection is changed. Its own publication/version/sequence CAS remains intact.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn guard_and_abandon(
    tx: &Transaction,
    worktree: WorktreeId,
    version: TaskLedgerStoreVersion,
    sequence: RunEventSequence,
    ledger: &TaskLedger,
    run: &AgentRun,
    event: &RunEvent,
    receipt: &EditNoChangeReceipt,
) -> Result<(), Failure> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < SCHEMA
    {
        return Err(Failure::UnsupportedSchema);
    }
    let a = receipt.anchor();
    if a.worktree() != worktree || a.version() != version || a.sequence() != sequence {
        return Err(Failure::StaleAnchor);
    }
    let old_run = edits::validate_live(tx, a).await?;
    let old = task_ledger_repository::load_from_transaction(tx, worktree, a.goal().task_id())
        .await
        .map_err(|_| Failure::InvalidStoredData)?
        .ok_or(Failure::StaleAnchor)?;
    let original_goal = goal(tx, a).await?;
    let selected = read_permit_row(tx, a.run(), receipt.job_digest(), receipt.kind())
        .await?
        .ok_or(Failure::InvalidStoredData)?;
    ensure_anchor(tx, a, receipt.job_digest(), receipt.kind()).await?;
    if !selected.matches_receipt(receipt)
        || selected.outcome.as_deref() != Some("rejected_reply")
        || !selected.no_change
    {
        return Err(Failure::InvalidStoredData);
    }
    let original =
        source_read_repository::no_change_original(tx, a, receipt.source_read_key()).await?;
    if &original != receipt.original()
        || original_goal.original().map(|o| o.digest()) != Some(receipt.original_goal())
    {
        return Err(Failure::InvalidStoredData);
    }
    let m = Metadata::receipt(receipt);
    ensure_metadata_key(
        tx,
        a.run(),
        receipt.job_digest(),
        receipt.kind(),
        receipt.source_read_key(),
        &m,
    )
    .await?;
    ensure_index(tx, a, receipt.index(), receipt.window()).await?;
    check_prior_key(
        tx,
        a,
        receipt.job_digest(),
        receipt.kind(),
        receipt.source_read_key(),
        receipt.wire(),
        receipt.reserved_at(),
        receipt.deadline(),
        receipt.generation_limit(),
        receipt.context_limit(),
    )
    .await?;
    check_shared(
        tx,
        &old_run,
        a.worktree(),
        event.occurred_at(),
        Some(receipt.finished_at()),
        0,
    )
    .await?;
    let expected = a3_application::derive_edit_no_change_closure(
        &original_goal,
        &old_run,
        old.ledger(),
        receipt,
        event.occurred_at(),
    )
    .map_err(|_| Failure::InvalidStoredData)?;
    if expected.ledger() != ledger || expected.run() != run || expected.event() != event {
        return Err(Failure::InvalidStoredData);
    }
    // No accepted-delivery credit is fabricated. Failure/abandon is one existing CAS.
    let changed = tx
        .execute(
            "UPDATE source_read_acquisitions SET abandoned=1
        WHERE source_read_id=?1 AND abandoned=0 AND original_evidence_id IS NOT NULL",
            params![bytes(receipt.source_read_key())],
        )
        .await
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(Failure::ObservationConflict);
    }
    Ok(())
}

fn check_shape(
    permit: &EditPhaseReservation,
    observed: EditPhaseObservation,
    m: &Metadata<'_>,
) -> Result<(), Failure> {
    let reserve = u32::try_from((u64::from(m.context) * 22).div_ceil(100))
        .map_err(|_| Failure::InvalidStoredData)?
        .max(m.generation);
    if observed.outcome() != Outcome::NoChange
        || !m.window.matches_original(m.original)
        || m.profile != permit.anchor().model_profile()
        || permit.source_read_key().is_none()
        || permit.anchor().empty_published_index()
        || permit.anchor().planned_file_addition_scope().is_some()
        || m.generation == 0
        || m.context == 0
        || permit.wire().output_reserve() != reserve
        || permit
            .wire()
            .prompt_estimate()
            .checked_add(reserve)
            .is_none_or(|n| n > m.context)
        || observed.finished_at() < permit.reserved_at()
        || observed.finished_at() >= permit.deadline()
        || observed
            .reported_prompt_tokens()
            .is_none_or(|n| n == 0 || n > permit.wire().prompt_estimate())
        || observed.reported_output_tokens().is_none_or(|n| n == 0)
    {
        return Err(Failure::InvalidStoredData);
    }
    if observed
        .reported_output_tokens()
        .is_some_and(|n| n > m.generation)
    {
        return Err(Failure::BudgetExhausted(AgentBudgetDimension::OutputTokens));
    }
    Ok(())
}
async fn goal(tx: &Transaction, a: EditPhaseAnchor) -> Result<GoalContract, Failure> {
    goal_contract_repository::load_revision_from_transaction(
        tx,
        a.worktree(),
        a.goal().task_id(),
        a.goal().revision(),
    )
    .await
    .map_err(|_| Failure::InvalidStoredData)?
    .ok_or(Failure::StaleAnchor)
}
async fn ensure_index(
    tx: &Transaction,
    a: EditPhaseAnchor,
    index: IndexRunId,
    window: &EditNoChangeWindow,
) -> Result<(), Failure> {
    let mut rows=tx.query("WITH latest AS (SELECT index_run_id,snapshot_id FROM index_runs
        WHERE worktree_id=?1 AND status='published' ORDER BY run_sequence DESC LIMIT 1)
        SELECT 1 FROM latest l JOIN file_revisions f ON f.index_run_id=l.index_run_id
        WHERE l.index_run_id=?2 AND l.snapshot_id=?3 AND f.repository_path=?4 AND f.content_hash=?5",
        params![a.worktree().as_bytes().to_vec(),index.as_bytes().to_vec(),a.snapshot().as_bytes().to_vec(),
            window.revision().path().as_bytes().to_vec(),bytes(window.revision().content_hash())]).await.map_err(unavailable)?;
    if rows.next().await.map_err(unavailable)?.is_none() {
        return Err(Failure::StaleAnchor);
    }
    Ok(())
}
async fn ensure_anchor(
    tx: &Transaction,
    a: EditPhaseAnchor,
    job: ContentHash,
    kind: EditPhaseCallKind,
) -> Result<(), Failure> {
    let mut rows = tx
        .query(
            "SELECT worktree_id=?4 AND task_id=?5 AND goal_revision=?6
        AND ledger_revision=?7 AND ledger_store_version=?8 AND run_sequence=?9 AND step_id=?10
        AND attempt_number=?11 AND verification_id=?12 AND snapshot_id=?13
        FROM edit_phase_reservations WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",
            params![
                a.run().as_bytes().to_vec(),
                bytes(job),
                edits::call_kind_text(kind),
                a.worktree().as_bytes().to_vec(),
                a.goal().task_id().as_bytes().to_vec(),
                i64::from(a.goal().revision().get()),
                i64::from(a.ledger().get()),
                i64::try_from(a.version().get()).map_err(|_| Failure::InvalidStoredData)?,
                i64::try_from(a.sequence().get()).map_err(|_| Failure::InvalidStoredData)?,
                a.step().as_bytes().to_vec(),
                i64::from(a.attempt().get()),
                a.verification().as_bytes().to_vec(),
                a.snapshot().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::ObservationConflict)?;
    if number(&row, 0)? != 1 {
        return Err(Failure::InvalidStoredData);
    }
    Ok(())
}

struct PermitRow {
    wire: EditPhaseWireCost,
    reserved: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    outcome: Option<String>,
    no_change: bool,
    finished: Option<AgentRunTimestamp>,
    prompt: Option<u32>,
    output: Option<u32>,
    source: Option<ContentHash>,
    request: Option<ContentHash>,
}
impl PermitRow {
    fn matches_permit(&self, p: &EditPhaseReservation) -> bool {
        self.wire == p.wire()
            && self.reserved == p.reserved_at()
            && self.deadline == p.deadline()
            && self.source == p.source_read_key()
            && self.request == p.request_digest()
    }
    fn matches_receipt(&self, r: &EditNoChangeReceipt) -> bool {
        self.wire == r.wire()
            && self.reserved == r.reserved_at()
            && self.deadline == r.deadline()
            && self.source == Some(r.source_read_key())
            && self.finished == Some(r.finished_at())
            && self.prompt == Some(r.reported_prompt())
            && self.output == Some(r.reported_output())
            && self.request == r.request_digest()
    }
    fn permit(
        &self,
        phase: &PreparedEditPhase,
        kind: EditPhaseCallKind,
    ) -> Result<EditPhaseReservation, Failure> {
        let permit =
            EditPhaseReservation::issue(phase, kind, self.wire, self.reserved, self.deadline)
                .with_source_read_key(self.source.ok_or(Failure::InvalidStoredData)?);
        Ok(match self.request {
            Some(digest) => permit.with_request_digest(digest),
            None => {
                phase
                    .require_unqualified_scope()
                    .map_err(|_| Failure::InvalidStoredData)?;
                permit
            }
        })
    }

    fn observation(&self) -> Result<EditPhaseObservation, Failure> {
        Ok(EditPhaseObservation::new(
            Outcome::NoChange,
            self.prompt,
            self.output,
            self.finished.ok_or(Failure::InvalidStoredData)?,
        ))
    }
}
async fn read_permit_row(
    tx: &Transaction,
    run: a3_domain::AgentRunId,
    job: ContentHash,
    kind: EditPhaseCallKind,
) -> Result<Option<PermitRow>, Failure> {
    let mut rows=tx.query("SELECT wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,
        outcome,finished_at,reported_prompt,reported_output,source_read_id,terminal_reply_kind FROM edit_phase_reservations
        WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",params![run.as_bytes().to_vec(),bytes(job),edits::call_kind_text(kind)])
        .await.map_err(unavailable)?;
    let Some(row) = rows.next().await.map_err(unavailable)? else {
        return Ok(None);
    };
    let outcome: Option<String> = row.get(6).map_err(|_| Failure::InvalidStoredData)?;
    let finished: Option<i64> = row.get(7).map_err(|_| Failure::InvalidStoredData)?;
    let prompt: Option<i64> = row.get(8).map_err(|_| Failure::InvalidStoredData)?;
    let output: Option<i64> = row.get(9).map_err(|_| Failure::InvalidStoredData)?;
    let source: Option<Vec<u8>> = row.get(10).map_err(|_| Failure::InvalidStoredData)?;
    let tag: Option<String> = row.get(11).map_err(|_| Failure::InvalidStoredData)?;
    if tag.as_deref().is_some_and(|text| text != "no_change") {
        return Err(Failure::InvalidStoredData);
    }
    let mut permit = PermitRow {
        wire: EditPhaseWireCost::new(
            hash(&row, 0)?,
            count(&row, 1)?,
            count(&row, 2)?,
            count(&row, 3)?,
        )?,
        reserved: at(number(&row, 4)?)?,
        deadline: at(number(&row, 5)?)?,
        outcome,
        no_change: tag.is_some(),
        finished: finished.map(at).transpose()?,
        prompt: prompt.map(as_count).transpose()?,
        output: output.map(as_count).transpose()?,
        source: source
            .map(|b| {
                b.try_into()
                    .map(ContentHash::from_bytes)
                    .map_err(|_| Failure::InvalidStoredData)
            })
            .transpose()?,
        request: None,
    };
    drop(row);
    drop(rows);
    permit.request = edits::recorded_request_digest(tx, run, job, kind).await?;
    Ok(Some(permit))
}
async fn check_prior(
    tx: &Transaction,
    p: &EditPhaseReservation,
    generation: u32,
    context: u32,
) -> Result<(), Failure> {
    check_prior_key(
        tx,
        p.anchor(),
        p.job_digest(),
        p.kind(),
        p.source_read_key().ok_or(Failure::InvalidStoredData)?,
        p.wire(),
        p.reserved_at(),
        p.deadline(),
        generation,
        context,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn check_prior_key(
    tx: &Transaction,
    a: EditPhaseAnchor,
    job: ContentHash,
    kind: EditPhaseCallKind,
    source: ContentHash,
    wire: EditPhaseWireCost,
    reserved: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    generation: u32,
    context: u32,
) -> Result<(), Failure> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        >= 70
    {
        let mut job_rows = tx.query("SELECT generation_cap,context_cap FROM edit_phase_jobs WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",
            params![a.run().as_bytes().to_vec(),bytes(job),edits::call_kind_text(kind)]).await.map_err(unavailable)?;
        if let Some(job_row) = job_rows.next().await.map_err(unavailable)?
            && (count(&job_row, 0)? != generation || count(&job_row, 1)? != context)
        {
            return Err(Failure::InvalidStoredData);
        }
    }
    let mut rows = tx
        .query(
            "SELECT count(*) FROM edit_phase_reservations WHERE run_id=?1 AND job_digest=?2",
            params![a.run().as_bytes().to_vec(), bytes(job)],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::InvalidStoredData)?;
    let expected = if kind == EditPhaseCallKind::Initial {
        1
    } else {
        2
    };
    if number(&row, 0)? != expected {
        return Err(Failure::InvalidStoredData);
    }
    if kind == EditPhaseCallKind::StructuralRepair {
        ensure_anchor(tx, a, job, EditPhaseCallKind::Initial).await?;
        let initial = read_permit_row(tx, a.run(), job, EditPhaseCallKind::Initial)
            .await?
            .ok_or(Failure::InvalidStoredData)?;
        if initial.outcome.as_deref() != Some("invalid_structure")
            || initial.no_change
            || initial.source != Some(source)
            || initial.wire.digest() == wire.digest()
            || initial.deadline != deadline
            || initial
                .finished
                .is_none_or(|t| t < initial.reserved || t > reserved || t >= deadline)
            || initial
                .prompt
                .is_none_or(|n| n == 0 || n > initial.wire.prompt_estimate())
            || initial
                .output
                .is_none_or(|n| n == 0 || n > generation || n > initial.wire.output_reserve())
            || initial.wire.output_reserve() != wire.output_reserve()
            || initial
                .wire
                .prompt_estimate()
                .checked_add(initial.wire.output_reserve())
                .is_none_or(|n| n > context)
        {
            return Err(Failure::InvalidStoredData);
        }
    }
    Ok(())
}
async fn check_shared(
    tx: &Transaction,
    run: &AgentRun,
    worktree: WorktreeId,
    now: AgentRunTimestamp,
    terminal: Option<AgentRunTimestamp>,
    expected_unobserved: u32,
) -> Result<(), Failure> {
    let (usage, latest) = edits::read_run_usage(tx, worktree, run).await?;
    if usage.unobserved_calls != expected_unobserved || usage.unknown_calls != 0 {
        return Err(Failure::UncertainProviderCall);
    }
    if now < run.updated_at() || latest.is_some_and(|t| now < t) {
        return Err(Failure::TimestampRegressed);
    }
    if terminal.is_some_and(|t| latest != Some(t)) {
        return Err(Failure::StaleAnchor);
    }
    if now >= edits::run_deadline(run)? {
        return Err(Failure::DeadlineReached);
    }
    for (dimension, used, reserved, limit) in [
        (
            AgentBudgetDimension::Turns,
            u64::from(run.usage().turn_count()),
            u64::from(usage.calls),
            u64::from(run.budget().turn_limit().get()),
        ),
        (
            AgentBudgetDimension::PromptTokens,
            run.usage().prompt_tokens(),
            usage.prompt_tokens,
            run.budget().prompt_token_limit().get(),
        ),
        (
            AgentBudgetDimension::OutputTokens,
            run.usage().output_tokens(),
            usage.output_tokens,
            run.budget().output_token_limit().get(),
        ),
        (
            AgentBudgetDimension::Repairs,
            u64::from(run.usage().repair_count()),
            u64::from(usage.repairs),
            u64::from(run.budget().repair_limit().get()),
        ),
    ] {
        if used.checked_add(reserved).is_none_or(|sum| sum > limit) {
            return Err(Failure::BudgetExhausted(dimension));
        }
    }
    Ok(())
}

async fn insert_metadata(
    tx: &Transaction,
    p: &EditPhaseReservation,
    m: &Metadata<'_>,
) -> Result<(), Failure> {
    tx.execute("INSERT INTO edit_phase_no_change_receipts (run_id,job_digest,call_kind,record_version,
        index_run_id,profile_id,profile_version,generation_limit,context_limit,original_goal_digest,
        repository_path,content_hash,window_start,window_end,window_digest,source_read_id,original_range)
        VALUES (?1,?2,?3,1,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![p.anchor().run().as_bytes().to_vec(),bytes(p.job_digest()),edits::call_kind_text(p.kind()),
            m.index.as_bytes().to_vec(),m.profile.id().as_bytes().to_vec(),i64::from(m.profile.version().get()),
            i64::from(m.generation),i64::from(m.context),m.original_goal.as_bytes().to_vec(),
            m.window.revision().path().as_bytes().to_vec(),bytes(m.window.revision().content_hash()),
            i64::from(m.window.start_byte()),i64::from(m.window.end_byte()),bytes(m.window.digest()),
            bytes(p.source_read_key().ok_or(Failure::InvalidStoredData)?),range_bytes(m.original.range())])
        .await.map_err(unavailable)?;
    Ok(())
}
async fn ensure_metadata(
    tx: &Transaction,
    p: &EditPhaseReservation,
    m: &Metadata<'_>,
) -> Result<(), Failure> {
    ensure_metadata_key(
        tx,
        p.anchor().run(),
        p.job_digest(),
        p.kind(),
        p.source_read_key().ok_or(Failure::InvalidStoredData)?,
        m,
    )
    .await
}
async fn ensure_metadata_key(
    tx: &Transaction,
    run: a3_domain::AgentRunId,
    job: ContentHash,
    kind: EditPhaseCallKind,
    source: ContentHash,
    m: &Metadata<'_>,
) -> Result<(), Failure> {
    let mut rows = tx
        .query(
            "SELECT record_version=1 AND index_run_id=?4 AND profile_id=?5 AND profile_version=?6
        AND generation_limit=?7 AND context_limit=?8 AND original_goal_digest=?9
        AND repository_path=?10 AND content_hash=?11 AND window_start=?12 AND window_end=?13
        AND window_digest=?14 AND source_read_id=?15 AND original_range=?16
        FROM edit_phase_no_change_receipts WHERE run_id=?1 AND job_digest=?2 AND call_kind=?3",
            params![
                run.as_bytes().to_vec(),
                bytes(job),
                edits::call_kind_text(kind),
                m.index.as_bytes().to_vec(),
                m.profile.id().as_bytes().to_vec(),
                i64::from(m.profile.version().get()),
                i64::from(m.generation),
                i64::from(m.context),
                m.original_goal.as_bytes().to_vec(),
                m.window.revision().path().as_bytes().to_vec(),
                bytes(m.window.revision().content_hash()),
                i64::from(m.window.start_byte()),
                i64::from(m.window.end_byte()),
                bytes(m.window.digest()),
                bytes(source),
                range_bytes(m.original.range())
            ],
        )
        .await
        .map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(Failure::InvalidStoredData)?;
    if number(&row, 0)? != 1 {
        return Err(Failure::InvalidStoredData);
    }
    Ok(())
}
fn range_bytes(r: a3_domain::SourceRange) -> Vec<u8> {
    [
        r.start_byte(),
        r.end_byte(),
        r.start_position().row(),
        r.start_position().column(),
        r.end_position().row(),
        r.end_position().column(),
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}
fn kind(row: &Row, index: i32) -> Result<EditPhaseCallKind, Failure> {
    let text: String = row.get(index).map_err(|_| Failure::InvalidStoredData)?;
    match text.as_str() {
        "initial" => Ok(EditPhaseCallKind::Initial),
        "structural_repair" => Ok(EditPhaseCallKind::StructuralRepair),
        _ => Err(Failure::InvalidStoredData),
    }
}
fn hash(row: &Row, index: i32) -> Result<ContentHash, Failure> {
    let bytes: Vec<u8> = row.get(index).map_err(|_| Failure::InvalidStoredData)?;
    bytes
        .try_into()
        .map(ContentHash::from_bytes)
        .map_err(|_| Failure::InvalidStoredData)
}
fn number(row: &Row, index: i32) -> Result<i64, Failure> {
    row.get(index).map_err(|_| Failure::InvalidStoredData)
}
fn as_count(n: i64) -> Result<u32, Failure> {
    u32::try_from(n).map_err(|_| Failure::InvalidStoredData)
}
fn count(row: &Row, index: i32) -> Result<u32, Failure> {
    as_count(number(row, index)?)
}
fn at(n: i64) -> Result<AgentRunTimestamp, Failure> {
    AgentRunTimestamp::from_unix_millis(u64::try_from(n).map_err(|_| Failure::InvalidStoredData)?)
        .map_err(|_| Failure::InvalidStoredData)
}
fn time(n: AgentRunTimestamp) -> Result<i64, Failure> {
    i64::try_from(n.unix_millis()).map_err(|_| Failure::InvalidStoredData)
}
fn bytes(h: ContentHash) -> Vec<u8> {
    h.as_bytes().to_vec()
}
fn unavailable(_: libsql::Error) -> Failure {
    Failure::Unavailable
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

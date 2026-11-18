//! One V6 reservation history for one authentic first closed HTTP NoChange.
//! No live/Historical boolean, new Run, source credit, refund or controller loop.
use super::origin::{OriginFrame, SqlOriginFrame};
use super::*;
use a3_application::{
    ClosedHttpNoChangeOrigin, HttpNoChangeReplanAnchor, HttpNoChangeReplanDecision,
    HttpNoChangeReplanResume, HttpNoChangeReplanResumeState, HttpNoChangeReplanSettlement,
    PreparedHttpNoChangeReplanV6Job, StoredTaskLedger,
};
#[path = "http_no_change_anchor_codec.rs"]
mod codec;
const SCHEMA: u32 = 69;

// Constructible only after this exact transaction authenticated all old and own
// phase charges/history. It cannot widen a public B1 or live entrance.
pub(super) struct AuthenticatedReplanHistory {
    worktree: WorktreeId,
    run: AgentRunId,
    version: a3_application::TaskLedgerStoreVersion,
    sequence: RunEventSequence,
    index: IndexRunId,
    snapshot: SnapshotId,
    profile: ModelProfileReference,
    now: AgentRunTimestamp,
}
impl AuthenticatedReplanHistory {
    pub(super) fn require_frame(&self, frame: &SqlOriginFrame<'_>) -> Result<(), E> {
        if self.worktree != frame.project.worktree().id()
            || self.run != frame.run.id()
            || self.version != frame.stored.version()
            || self.sequence != frame.run.last_event_sequence()
            || self.index != frame.index
            || self.snapshot != frame.snapshot
            || self.profile != frame.profile.reference()
            || self.now != frame.now
        {
            return Err(E::StaleAnchor);
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Call {
    anchor: HttpNoChangeReplanAnchor,
    job: ContentHash,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    outcome: Option<String>,
    finished: Option<AgentRunTimestamp>,
    prompt: Option<u32>,
    output: Option<u32>,
    decision: Option<HttpNoChangeReplanDecision>,
}
impl Call {
    fn matches(&self, p: &HttpCorrectionPlanReservation) -> bool {
        self.job == p.job
            && self.kind == p.kind
            && self.wire == p.wire
            && self.reserved == p.reserved_at
            && self.deadline == p.deadline
    }
    fn settlement(&self) -> Result<HttpNoChangeReplanSettlement, E> {
        let outcome = match self.outcome.as_deref() {
            Some("accepted_reply" | "no_progress") => Outcome::AcceptedReply,
            Some("invalid_structure") => Outcome::InvalidStructure,
            Some("provider_failure") => Outcome::ProviderFailure,
            Some("cancelled") => Outcome::Cancelled,
            Some("rejected_reply" | "stale") => Outcome::RejectedReply,
            None | Some("unknown" | "budget_overrun") => return Err(E::UncertainProviderCall),
            _ => return Err(E::InvalidStoredData),
        };
        HttpNoChangeReplanSettlement::from_recorded(
            self.job,
            self.kind,
            self.wire,
            self.reserved,
            self.deadline,
            EditPhaseObservation::new(
                outcome,
                self.prompt,
                self.output,
                self.finished.ok_or(E::InvalidStoredData)?,
            ),
            self.decision.clone(),
        )
        .map_err(|_| E::InvalidStoredData)
    }
}
#[derive(Clone, Copy)]
enum ReadPurpose<'a> {
    Resume,
    Reserve(EditPhaseCallKind),
    Finish(&'a HttpCorrectionPlanReservation),
}
pub(super) struct Loaded {
    pub(super) closed: ClosedHttpNoChangeOrigin,
    calls: Vec<Call>,
    usage: EditPhaseReservedUsage,
}

pub(super) fn load<'a>(
    store: &'a LibsqlKnowledgeStore,
    frame: OriginFrame<'a>,
) -> EditPhaseReservationFuture<'a, Option<HttpNoChangeReplanResume>> {
    Box::pin(async move {
        let tx = open_tx(store, frame.project, TransactionBehavior::Deferred).await?;
        let result = async {
            let Some(loaded) = load_in_transaction(&tx, frame.into(), ReadPurpose::Resume).await?
            else {
                return Ok(None);
            };
            let state = match loaded.calls.last() {
                None => HttpNoChangeReplanResumeState::Unreserved,
                Some(call) => HttpNoChangeReplanResumeState::Observed(Box::new(call.settlement()?)),
            };
            HttpNoChangeReplanResume::from_recorded(loaded.closed, state)
                .map(Some)
                .map_err(|_| E::InvalidStoredData)
        }
        .await;
        close(tx, result).await
    })
}
async fn load_in_transaction(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
    purpose: ReadPurpose<'_>,
) -> Result<Option<Loaded>, E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < SCHEMA
    {
        return Err(E::UnsupportedSchema);
    }
    let Some(auth) = super::closed_no_change::authenticate_fact(tx, frame).await? else {
        return Ok(None);
    };
    let calls = read_calls(tx, frame.goal, &auth).await?;
    match purpose {
        ReadPurpose::Resume => {
            if calls.iter().any(|c| {
                matches!(
                    c.outcome.as_deref(),
                    None | Some("unknown" | "budget_overrun")
                )
            }) {
                return Err(E::UncertainProviderCall);
            }
        }
        ReadPurpose::Reserve(kind) => {
            if kind == EditPhaseCallKind::Initial && !calls.is_empty() {
                return Err(E::AlreadyReserved);
            }
            if kind == EditPhaseCallKind::StructuralRepair
                && (calls.len() != 1 || calls[0].outcome.as_deref() != Some("invalid_structure"))
            {
                return Err(E::AlreadyReserved);
            }
        }
        ReadPurpose::Finish(permit) => {
            if calls
                .last()
                .is_none_or(|c| !c.matches(permit) || c.outcome.is_some())
            {
                return Err(E::ObservationConflict);
            }
        }
    }
    let pending = u32::from(matches!(purpose, ReadPurpose::Finish(_)));
    let (usage, latest) =
        edit::read_run_usage(tx, frame.project.worktree().id(), frame.run).await?;
    let expected_latest = calls
        .iter()
        .map(|c| c.finished.unwrap_or(c.reserved))
        .chain([auth.fact().call().observation().finished_at()])
        .max();
    if latest != expected_latest {
        return Err(E::StaleAnchor);
    }
    check_clock_budget(frame.run, frame.now, usage, latest, pending)?;
    let history = AuthenticatedReplanHistory {
        worktree: frame.project.worktree().id(),
        run: frame.run.id(),
        version: frame.stored.version(),
        sequence: frame.run.last_event_sequence(),
        index: frame.index,
        snapshot: frame.snapshot,
        profile: frame.profile.reference(),
        now: frame.now,
    };
    let parent = super::origin::load_for_closed_replan(tx, frame, &auth, &history).await?;
    let closed = ClosedHttpNoChangeOrigin::from_recorded(
        parent,
        auth.fact().clone(),
        auth.closure().clone(),
        frame.stored.version(),
    )?;
    let expected =
        HttpNoChangeReplanAnchor::from_closed(&closed).map_err(|_| E::InvalidStoredData)?;
    if calls.iter().any(|c| c.anchor != expected) {
        return Err(E::InvalidStoredData);
    }
    Ok(Some(Loaded {
        closed,
        calls,
        usage,
    }))
}

async fn read_calls(
    tx: &Transaction,
    goal: &GoalContract,
    auth: &super::closed_no_change::AuthenticatedClosedNoChange,
) -> Result<Vec<Call>, E> {
    let fact = auth.fact();
    let o = fact.owner();
    let mut rows=tx.query("SELECT worktree_id,closed_job_digest,closed_call_kind,call_kind,job_digest,
        protocol_version,anchor_version,CASE WHEN typeof(anchor)='blob' AND length(anchor) BETWEEN 512 AND 133000 THEN anchor ELSE x'' END,
        closure_sequence,closure_event_id,wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline,
        outcome,finished_at,reported_prompt,reported_output,
        CASE WHEN proposal IS NULL THEN NULL WHEN typeof(proposal)='blob' AND length(proposal) BETWEEN 1 AND 512 THEN proposal ELSE x'' END,proposal_digest
        FROM http_no_change_replan_reservations WHERE run_id=?1
        ORDER BY CASE call_kind WHEN 'initial' THEN 0 ELSE 1 END LIMIT 3",params![o.run.as_bytes().to_vec()]).await.map_err(unavailable)?;
    let mut calls: Vec<Call> = Vec::new();
    while let Some(row) = rows.next().await.map_err(unavailable)? {
        let anchor = codec::decode(&blob(&row, 7)?, goal)?;
        let kind = call_kind(&text(&row, 3)?)?;
        let job = hash(&row, 4)?;
        let wire = EditPhaseWireCost::new(
            hash(&row, 10)?,
            count(&row, 11)?,
            count(&row, 12)?,
            count(&row, 13)?,
        )?;
        let reserved = timestamp(n(&row, 14)?)?;
        let deadline = timestamp(n(&row, 15)?)?;
        let outcome: Option<String> = row.get(16).map_err(|_| E::InvalidStoredData)?;
        let finished = optional_time(&row, 17)?;
        let prompt = optional_count(&row, 18)?;
        let output = optional_count(&row, 19)?;
        let canonical: Option<Vec<u8>> = row.get(20).map_err(|_| E::InvalidStoredData)?;
        let proposal_digest: Option<Vec<u8>> = row.get(21).map_err(|_| E::InvalidStoredData)?;
        let decision = canonical
            .as_ref()
            .map(|b| {
                HttpNoChangeReplanDecision::decode_retained(job, &anchor, b)
                    .map_err(|_| E::InvalidStoredData)
            })
            .transpose()?;
        let call = Call {
            anchor,
            job,
            kind,
            wire,
            reserved,
            deadline,
            outcome,
            finished,
            prompt,
            output,
            decision,
        };
        if calls.len() >= 2
            || blob(&row, 0)?.as_slice() != o.worktree.as_bytes()
            || hash(&row, 1)? != fact.job_digest()
            || call_kind(&text(&row, 2)?)? != fact.call().kind()
            || n(&row, 5)? != 6
            || n(&row, 6)? != 1
            || n(&row, 8)? != auth.closure().sequence().get()
            || blob(&row, 9)?.as_slice() != auth.closure().id().as_bytes()
            || call.anchor.closed_owner != *o
            || call.anchor.window != *fact.window()
            || call.anchor.old_job != fact.job_digest()
            || call.anchor.old_source_read != fact.source_read_key()
            || call.anchor.old_call_kind != fact.call().kind()
            || call.anchor.closure != auth.closure().id()
            || call.anchor.closed_sequence != auth.closure().sequence()
            || o.version.get().checked_add(1) != Some(call.anchor.closed_version.get())
            || deadline != fact.call().deadline()
            || reserved < auth.closure().occurred_at()
            || reserved >= deadline
            || wire.output_reserve() != output_reserve(fact.limits())?
            || wire
                .prompt_estimate()
                .checked_add(wire.output_reserve())
                .is_none_or(|v| v > fact.limits().context_limit().get())
            || call.outcome.is_some() != call.finished.is_some()
            || call.finished.is_some_and(|v| v < reserved)
            || call.outcome.is_none()
                && (prompt.is_some()
                    || output.is_some()
                    || canonical.is_some()
                    || proposal_digest.is_some())
            || canonical.is_some() != proposal_digest.is_some()
            || canonical.as_ref().is_some_and(|b| {
                proposal_digest.as_deref() != Some(blake3::hash(b).as_bytes().as_slice())
            })
        {
            return Err(E::InvalidStoredData);
        }
        let semantic = matches!(
            call.outcome.as_deref(),
            Some("accepted_reply" | "no_progress")
        );
        if semantic != call.decision.is_some()
            || (call.outcome.as_deref() == Some("no_progress"))
                != matches!(
                    call.decision,
                    Some(HttpNoChangeReplanDecision::NoProgress { .. })
                )
            || matches!(
                call.outcome.as_deref(),
                Some("accepted_reply" | "no_progress" | "invalid_structure")
            ) && (finished.is_none_or(|v| v >= deadline)
                || prompt.is_none_or(|v| v == 0 || v > wire.prompt_estimate())
                || output.is_none_or(|v| v == 0 || v > fact.limits().generation_limit().get()))
        {
            return Err(E::InvalidStoredData);
        }
        match calls.as_slice() {
            [] if kind == EditPhaseCallKind::Initial => {}
            [first]
                if kind == EditPhaseCallKind::StructuralRepair
                    && first.outcome.as_deref() == Some("invalid_structure")
                    && first.job == job
                    && first.anchor == call.anchor
                    && first.wire.digest() != wire.digest()
                    && first.finished.is_some_and(|v| v <= reserved) => {}
            _ => return Err(E::InvalidStoredData),
        }
        calls.push(call);
    }
    Ok(calls)
}

fn check_clock_budget(
    run: &AgentRun,
    now: AgentRunTimestamp,
    usage: EditPhaseReservedUsage,
    latest: Option<AgentRunTimestamp>,
    pending: u32,
) -> Result<(), E> {
    if now >= edit::run_deadline(run)? {
        return Err(E::DeadlineReached);
    }
    if now < run.updated_at() || latest.is_some_and(|v| now < v) {
        return Err(E::TimestampRegressed);
    }
    if usage.unknown_calls != 0 || usage.unobserved_calls != pending {
        return Err(E::UncertainProviderCall);
    }
    for (dimension, used, charged, limit) in [
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
        if used.checked_add(charged).is_none_or(|v| v > limit) {
            return Err(E::BudgetExhausted(dimension));
        }
    }
    Ok(())
}
fn output_reserve(limits: a3_application::ClosedNoChangeProfileLimits) -> Result<u32, E> {
    Ok(
        u32::try_from((u64::from(limits.context_limit().get()) * 22).div_ceil(100))
            .map_err(|_| E::InvalidStoredData)?
            .max(limits.generation_limit().get()),
    )
}
fn hash(row: &Row, at: i32) -> Result<ContentHash, E> {
    Ok(ContentHash::from_bytes(
        blob(row, at)?
            .try_into()
            .map_err(|_| E::InvalidStoredData)?,
    ))
}
fn count(row: &Row, at: i32) -> Result<u32, E> {
    u32::try_from(n(row, at)?).map_err(|_| E::InvalidStoredData)
}
fn optional_count(row: &Row, at: i32) -> Result<Option<u32>, E> {
    row.get::<Option<i64>>(at)
        .map_err(|_| E::InvalidStoredData)?
        .map(|v| u32::try_from(v).map_err(|_| E::InvalidStoredData))
        .transpose()
}
fn optional_time(row: &Row, at: i32) -> Result<Option<AgentRunTimestamp>, E> {
    row.get::<Option<i64>>(at)
        .map_err(|_| E::InvalidStoredData)?
        .map(|v| {
            u64::try_from(v)
                .map_err(|_| E::InvalidStoredData)
                .and_then(timestamp)
        })
        .transpose()
}
fn timestamp(v: u64) -> Result<AgentRunTimestamp, E> {
    AgentRunTimestamp::from_unix_millis(v).map_err(|_| E::InvalidStoredData)
}
fn number(v: u64) -> Result<i64, E> {
    i64::try_from(v).map_err(|_| E::InvalidStoredData)
}
fn text(row: &Row, at: i32) -> Result<String, E> {
    row.get(at).map_err(|_| E::InvalidStoredData)
}
fn call_kind(text: &str) -> Result<EditPhaseCallKind, E> {
    match text {
        "initial" => Ok(EditPhaseCallKind::Initial),
        "structural_repair" => Ok(EditPhaseCallKind::StructuralRepair),
        _ => Err(E::InvalidStoredData),
    }
}
async fn open_tx(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    behavior: TransactionBehavior,
) -> Result<Transaction, E> {
    let connection = open(store, project).await?;
    connection
        .transaction_with_behavior(behavior)
        .await
        .map_err(unavailable)
}

struct DurableFrame {
    goal: GoalContract,
    stored: StoredTaskLedger,
    run: AgentRun,
}
impl DurableFrame {
    fn sql<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        phase: &'a PreparedHttpNoChangeReplanV6Job,
        now: AgentRunTimestamp,
    ) -> SqlOriginFrame<'a> {
        let o = &phase.anchor().closed_owner;
        SqlOriginFrame {
            project,
            goal: &self.goal,
            stored: &self.stored,
            run: &self.run,
            index: o.index,
            snapshot: o.snapshot,
            profile: phase.profile(),
            now,
        }
    }
}
async fn durable_frame(
    tx: &Transaction,
    project: &ProjectIdentity,
    phase: &PreparedHttpNoChangeReplanV6Job,
) -> Result<DurableFrame, E> {
    let o = &phase.anchor().closed_owner;
    let worktree = project.worktree().id();
    let goal =
        goals::load_revision_from_transaction(tx, worktree, o.goal.task_id(), o.goal.revision())
            .await
            .map_err(|_| E::InvalidStoredData)?
            .ok_or(E::StaleAnchor)?;
    let stored = ledgers::load_from_transaction(tx, worktree, o.goal.task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let run = journal::load_run_from_transaction(tx, worktree, o.run)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    Ok(DurableFrame { goal, stored, run })
}
fn require_phase(phase: &PreparedHttpNoChangeReplanV6Job, loaded: &Loaded) -> Result<(), E> {
    let expected =
        HttpNoChangeReplanAnchor::from_closed(&loaded.closed).map_err(|_| E::InvalidStoredData)?;
    if phase.anchor() != &expected
        || phase.profile().reference() != expected.closed_owner.profile
        || !phase.profile().executable_actions_enabled()
        || loaded.calls.iter().any(|c| c.job != phase.job_digest())
    {
        return Err(E::StaleAnchor);
    }
    a3_application::replay_closed_no_change_edit_job(loaded.closed.fact(), phase.source_page())?;
    Ok(())
}

pub(super) fn reserve<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpNoChangeReplanV6Job,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    now: AgentRunTimestamp,
) -> EditPhaseReservationFuture<'a, HttpCorrectionPlanReservation> {
    Box::pin(async move {
        let tx = open_tx(store, project, TransactionBehavior::Immediate).await?;
        let result=async{
            let durable=durable_frame(&tx,project,phase).await?;
            let loaded=load_in_transaction(&tx,durable.sql(project,phase,now),ReadPurpose::Reserve(kind))
                .await?.ok_or(E::StaleAnchor)?;
            require_phase(phase,&loaded)?;
            let limits=loaded.closed.fact().limits();
            if wire.output_reserve()!=output_reserve(limits)?
                || wire.prompt_estimate().checked_add(wire.output_reserve()).is_none_or(|v|v>limits.context_limit().get())
                || wire.request_bytes()>16_777_216 {return Err(E::InvalidWireCost);}
            if loaded.calls.last().is_some_and(|c|c.wire.digest()==wire.digest()) {return Err(E::AlreadyReserved);}
            edit::check_budget(durable.run.budget(),durable.run.usage(),loaded.usage,kind,wire)?;
            let a=phase.anchor();let o=&a.closed_owner;let deadline=edit::run_deadline(&durable.run)?;
            tx.execute("INSERT INTO http_no_change_replan_reservations(worktree_id,run_id,closed_job_digest,
                closed_call_kind,call_kind,job_digest,protocol_version,anchor_version,anchor,closure_sequence,closure_event_id,
                wire_digest,wire_bytes,prompt_reserve,output_reserve,reserved_at,deadline)
                VALUES(?1,?2,?3,?4,?5,?6,6,1,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![
                o.worktree.as_bytes().to_vec(),o.run.as_bytes().to_vec(),a.old_job.as_bytes().to_vec(),edit::call_kind_text(a.old_call_kind),
                edit::call_kind_text(kind),phase.job_digest().as_bytes().to_vec(),codec::encode(a)?,number(a.closed_sequence.get())?,
                a.closure.as_bytes().to_vec(),wire.digest().as_bytes().to_vec(),i64::from(wire.request_bytes()),i64::from(wire.prompt_estimate()),
                i64::from(wire.output_reserve()),number(now.unix_millis())?,number(deadline.unix_millis())?]).await.map_err(unavailable)?;
            Ok(HttpCorrectionPlanReservation{job:phase.job_digest(),kind,wire,reserved_at:now,deadline})
        }.await;
        close(tx, result).await
    })
}

pub(super) fn finish<'a>(
    store: &'a LibsqlKnowledgeStore,
    project: &'a ProjectIdentity,
    phase: &'a PreparedHttpNoChangeReplanV6Job,
    permit: HttpCorrectionPlanReservation,
    observation: EditPhaseObservation,
    decision: Option<HttpNoChangeReplanDecision>,
) -> EditPhaseReservationFuture<'a, HttpNoChangeReplanSettlement> {
    Box::pin(async move {
        let tx = open_tx(store, project, TransactionBehavior::Immediate).await?;
        let result=async{
            if permit.job!=phase.job_digest() || observation.finished_at()<permit.reserved_at {return Err(E::ObservationConflict);}
            let(mut outcome,mut rejection)=match observation.outcome(){
                Outcome::AcceptedReply=>("accepted_reply",None),Outcome::InvalidStructure=>("invalid_structure",None),
                Outcome::RejectedReply=>("rejected_reply",None),Outcome::ProviderFailure=>("provider_failure",None),
                Outcome::Cancelled=>("cancelled",None),Outcome::Unknown=>("unknown",None),
                Outcome::NoChange | Outcome::DeadlineReached=>return Err(E::InvalidStoredData),
            };
            // An exact owned pending row is still settled after a stale frame/deadline;
            // none of these failures refunds a call or promotes its decision.
            let durable=durable_frame(&tx,project,phase).await?;
            match load_in_transaction(&tx,durable.sql(project,phase,observation.finished_at()),ReadPurpose::Finish(&permit)).await {
                Ok(Some(loaded))=>require_phase(phase,&loaded)?,
                Ok(None)=>return Err(E::StaleAnchor),
                Err(error@(E::StaleAnchor|E::MutationRecoveryRequired|E::DeadlineReached|E::TimestampRegressed))=>{
                    if outcome!="unknown"{outcome="stale";} rejection=Some(error);
                }
                Err(error)=>return Err(error),
            }
            if observation.finished_at()>=permit.deadline {if outcome!="unknown"{outcome="stale";}rejection=Some(E::DeadlineReached);}
            let prompt=observation.reported_prompt_tokens();let output=observation.reported_output_tokens();
            if prompt.is_some_and(|v|v>permit.wire.prompt_estimate()){
                outcome="budget_overrun";rejection=Some(E::BudgetExhausted(AgentBudgetDimension::PromptTokens));
            }else if output.is_some_and(|v|v>permit.wire.output_reserve()){
                outcome="budget_overrun";rejection=Some(E::BudgetExhausted(AgentBudgetDimension::OutputTokens));
            }else if matches!(outcome,"accepted_reply"|"invalid_structure")
                && output.is_some_and(|v|v>phase.profile().settings().output_limit().get()){
                outcome="unknown";rejection=Some(E::BudgetExhausted(AgentBudgetDimension::OutputTokens));
            }else if matches!(outcome,"accepted_reply"|"invalid_structure")
                && (prompt.is_none_or(|v|v==0)||output.is_none_or(|v|v==0)){
                outcome="unknown";rejection=Some(E::IncompleteProviderUsage);
            }
            let canonical=if outcome=="accepted_reply"{
                let selected=decision.as_ref().ok_or(E::InvalidStoredData)?;
                let bytes=phase.encode_proposal(selected).map_err(|_|E::InvalidStoredData)?;
                if matches!(selected,HttpNoChangeReplanDecision::NoProgress{..}){outcome="no_progress";}
                Some(bytes)
            }else{None};
            let digest=canonical.as_ref().map(|b|blake3::hash(b).as_bytes().to_vec());
            let a=phase.anchor();let o=&a.closed_owner;
            let changed=tx.execute("UPDATE http_no_change_replan_reservations SET outcome=?1,finished_at=?2,
                reported_prompt=?3,reported_output=?4,proposal=?5,proposal_digest=?6
                WHERE run_id=?7 AND closed_job_digest=?8 AND closed_call_kind=?9 AND call_kind=?10 AND job_digest=?11
                AND protocol_version=6 AND anchor_version=1 AND anchor=?12 AND closure_sequence=?13 AND closure_event_id=?14
                AND wire_digest=?15 AND wire_bytes=?16 AND prompt_reserve=?17 AND output_reserve=?18
                AND reserved_at=?19 AND deadline=?20 AND outcome IS NULL",params![outcome,number(observation.finished_at().unix_millis())?,
                prompt.map(i64::from),output.map(i64::from),canonical,digest,o.run.as_bytes().to_vec(),a.old_job.as_bytes().to_vec(),
                edit::call_kind_text(a.old_call_kind),edit::call_kind_text(permit.kind),permit.job.as_bytes().to_vec(),codec::encode(a)?,
                number(a.closed_sequence.get())?,a.closure.as_bytes().to_vec(),permit.wire.digest().as_bytes().to_vec(),
                i64::from(permit.wire.request_bytes()),i64::from(permit.wire.prompt_estimate()),i64::from(permit.wire.output_reserve()),
                number(permit.reserved_at.unix_millis())?,number(permit.deadline.unix_millis())?]).await.map_err(unavailable)?;
            if changed!=1{return Err(E::ObservationConflict);}
            let settlement=if rejection.is_none() && !matches!(outcome,"unknown"|"budget_overrun"){
                Some(HttpNoChangeReplanSettlement::from_recorded(permit.job,permit.kind,permit.wire,permit.reserved_at,permit.deadline,
                    observation,if matches!(outcome,"accepted_reply"|"no_progress"){decision}else{None}).map_err(|_|E::InvalidStoredData)?)
            }else{None};
            Ok((settlement,rejection))
        }.await;
        match result {
            Ok((settlement, rejection)) => {
                tx.commit().await.map_err(unavailable)?;
                match rejection {
                    Some(error) => Err(error),
                    None => settlement.ok_or(E::UncertainProviderCall),
                }
            }
            Err(error) => {
                tx.rollback().await.map_err(unavailable)?;
                Err(error)
            }
        }
    })
}

pub(crate) async fn usage(
    tx: &Transaction,
    worktree: WorktreeId,
    run: AgentRunId,
) -> Result<(EditPhaseReservedUsage, Option<AgentRunTimestamp>), E> {
    if crate::migration::read_user_version(tx)
        .await
        .map_err(unavailable)?
        < SCHEMA
    {
        return Ok((EditPhaseReservedUsage::default(), None));
    }
    let mut rows=tx.query("SELECT COUNT(*),COUNT(CASE WHEN call_kind='structural_repair' THEN 1 END),
        COALESCE(SUM(MAX(prompt_reserve,COALESCE(reported_prompt,0))),0),
        COALESCE(SUM(MAX(output_reserve,COALESCE(reported_output,0))),0),
        COUNT(CASE WHEN outcome IS NULL THEN 1 END),COUNT(CASE WHEN outcome IN ('unknown','budget_overrun') THEN 1 END),
        MAX(COALESCE(finished_at,reserved_at)),COUNT(CASE WHEN protocol_version IS NOT 6 OR anchor_version IS NOT 1 THEN 1 END)
        FROM http_no_change_replan_reservations WHERE worktree_id=?1 AND run_id=?2",params![worktree.as_bytes().to_vec(),run.as_bytes().to_vec()])
        .await.map_err(unavailable)?;
    let row = rows
        .next()
        .await
        .map_err(unavailable)?
        .ok_or(E::InvalidStoredData)?;
    if n(&row, 7)? != 0 {
        return Err(E::InvalidStoredData);
    }
    Ok((
        EditPhaseReservedUsage {
            calls: count(&row, 0)?,
            repairs: count(&row, 1)?,
            prompt_tokens: n(&row, 2)?,
            output_tokens: n(&row, 3)?,
            unobserved_calls: count(&row, 4)?,
            unknown_calls: count(&row, 5)?,
        },
        optional_time(&row, 6)?,
    ))
}

pub(super) struct AuthenticatedMaterial {
    pub(super) goal: GoalContract,
    pub(super) stored: StoredTaskLedger,
    pub(super) run: AgentRun,
    pub(super) material: a3_application::HttpNoChangeWindowMaterialProof,
}
pub(super) async fn load_for_material(
    tx: &Transaction,
    project: &ProjectIdentity,
    claimed: &a3_application::HttpNoChangeWindowMaterialProof,
    now: AgentRunTimestamp,
) -> Result<AuthenticatedMaterial, E> {
    let o = claimed.closed_origin().fact().owner();
    let worktree = project.worktree().id();
    let goal =
        goals::load_revision_from_transaction(tx, worktree, o.goal.task_id(), o.goal.revision())
            .await
            .map_err(|_| E::InvalidStoredData)?
            .ok_or(E::StaleAnchor)?;
    let stored = ledgers::load_from_transaction(tx, worktree, o.goal.task_id())
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let run = journal::load_run_from_transaction(tx, worktree, o.run)
        .await
        .map_err(|_| E::InvalidStoredData)?
        .ok_or(E::StaleAnchor)?;
    let frame = SqlOriginFrame {
        project,
        goal: &goal,
        stored: &stored,
        run: &run,
        index: o.index,
        snapshot: o.snapshot,
        profile: claimed.profile(),
        now,
    };
    let loaded = load_in_transaction(tx, frame, ReadPurpose::Resume)
        .await?
        .ok_or(E::StaleAnchor)?;
    let settlement = loaded.calls.last().ok_or(E::StaleAnchor)?.settlement()?;
    if !matches!(
        settlement.decision(),
        Some(HttpNoChangeReplanDecision::Window { .. })
    ) || &settlement != claimed.settlement()
        || HttpNoChangeReplanAnchor::from_closed(&loaded.closed)
            .map_err(|_| E::InvalidStoredData)?
            != HttpNoChangeReplanAnchor::from_closed(claimed.closed_origin())
                .map_err(|_| E::InvalidStoredData)?
    {
        return Err(E::InvalidStoredData);
    }
    let material = a3_application::HttpNoChangeWindowMaterialProof::from_recorded(
        loaded.closed,
        settlement,
        claimed.profile().clone(),
    )
    .map_err(|_| E::InvalidStoredData)?;
    Ok(AuthenticatedMaterial {
        goal,
        stored,
        run,
        material,
    })
}

// Applied-window purpose: verifies only the unchanged own V6 history/charges.
// The caller independently proves actual old closure and exact new material prefix.
pub(super) async fn accepted_window_for_applied(
    tx: &Transaction,
    frame: SqlOriginFrame<'_>,
    auth: &super::closed_no_change::AuthenticatedClosedNoChange,
) -> Result<HttpNoChangeReplanSettlement, E> {
    let calls = read_calls(tx, frame.goal, auth).await?;
    let (usage, latest) =
        edit::read_run_usage(tx, frame.project.worktree().id(), frame.run).await?;
    let expected_latest = calls
        .iter()
        .map(|c| c.finished.unwrap_or(c.reserved))
        .chain([auth.fact().call().observation().finished_at()])
        .max();
    if latest != expected_latest {
        return Err(E::StaleAnchor);
    }
    check_clock_budget(frame.run, frame.now, usage, latest, 0)?;
    let selected = calls.last().ok_or(E::StaleAnchor)?.settlement()?;
    if !matches!(
        selected.decision(),
        Some(HttpNoChangeReplanDecision::Window { .. })
    ) {
        return Err(E::InvalidStoredData);
    }
    Ok(selected)
}
pub(super) async fn verify_applied_anchor(
    tx: &Transaction,
    goal: &GoalContract,
    auth: &super::closed_no_change::AuthenticatedClosedNoChange,
    closed: &ClosedHttpNoChangeOrigin,
) -> Result<(), E> {
    let expected =
        HttpNoChangeReplanAnchor::from_closed(closed).map_err(|_| E::InvalidStoredData)?;
    let calls = read_calls(tx, goal, auth).await?;
    if calls.is_empty() || calls.iter().any(|c| c.anchor != expected) {
        return Err(E::InvalidStoredData);
    }
    Ok(())
}

// The actual B edit may already own a pending initial/repair reservation.
// Only this material-identity guard omits shared usage admission; normal reserve/finish
// retain their complete existing usage, deadline and uncertainty checks.
pub(super) async fn accepted_window_for_current_edit_index(
    tx: &Transaction,
    goal: &GoalContract,
    auth: &super::closed_no_change::AuthenticatedClosedNoChange,
) -> Result<HttpNoChangeReplanSettlement, E> {
    let calls = read_calls(tx, goal, auth).await?;
    let selected = calls.last().ok_or(E::StaleAnchor)?.settlement()?;
    if !matches!(
        selected.decision(),
        Some(HttpNoChangeReplanDecision::Window { .. })
    ) {
        return Err(E::StaleAnchor);
    }
    Ok(selected)
}

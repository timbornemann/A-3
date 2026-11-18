//! Exact smaller-window material after one authenticated HTTP NoChange closure.
use crate::{
    ClosedEditNoChangeFact, ClosedHttpNoChangeOrigin, EditPhaseObservationOutcome,
    HttpCorrectionPlanDecision, HttpCorrectionPlanIntent, HttpCorrectionProducer,
    HttpNoChangeReplanDecision, HttpNoChangeReplanSettlement,
};
use a3_domain::{
    ContentHash, DiffInvariantMode, DiffInvariantVerification, EvidenceRef, ExpectedTaskEvidence,
    ModelProfile, RunEvent, RunEventCode, RunEventId, RunEventKind, RunEventOutcome,
    StepDependency, TaskReplanReason, TaskStepDefinition, TaskStepId, TaskStepOutcome,
    TaskStepRationale, VerificationRequirement, VerificationSpec, VerificationSpecId,
    VerificationTarget,
};
use std::{error::Error, fmt};

/// Descriptive accepted V6 lineage, never database or source authority by itself.
#[derive(Debug, Clone)]
pub struct HttpNoChangeWindowMaterialProof {
    closed: ClosedHttpNoChangeOrigin,
    settlement: HttpNoChangeReplanSettlement,
    profile: ModelProfile,
    window: EvidenceRef,
    text_digest: ContentHash,
    intent: HttpCorrectionPlanIntent,
}
impl HttpNoChangeWindowMaterialProof {
    /// Shape only; the store must authenticate A closure, V6 history and currentness.
    pub fn from_recorded(
        closed: ClosedHttpNoChangeOrigin,
        settlement: HttpNoChangeReplanSettlement,
        profile: ModelProfile,
    ) -> Result<Self, HttpNoChangeWindowFailure> {
        use HttpNoChangeWindowFailure as E;
        let Some(HttpNoChangeReplanDecision::Window {
            first,
            last,
            original,
            text_digest,
            intent,
        }) = settlement.decision()
        else {
            return Err(E::NoWindow);
        };
        let owner = closed.fact().owner();
        let old = closed.fact().window().original();
        let span = original.range();
        let last_row = span
            .end_position()
            .row()
            .checked_add(u32::from(span.end_position().column() > 0))
            .ok_or(E::Binding)?;
        let context = profile.settings().context_limit().get();
        let generation = profile.settings().output_limit().get();
        let reserve = u32::try_from((u64::from(context) * 22).div_ceil(100))
            .map_err(|_| E::Binding)?
            .max(generation);
        if original.revision() != old.revision()
            || !old.range().contains(span)
            || span.is_empty()
            || span.len() >= old.range().len()
            || span.start_position().column() != 0
            || span.start_position().row().checked_add(1) != Some(first.get())
            || last.get() != last_row
            || first > last
            || !profile.executable_actions_enabled()
            || profile.reference() != owner.profile
            || profile.settings().context_limit() != closed.fact().limits().context_limit()
            || profile.settings().output_limit() != closed.fact().limits().generation_limit()
            || settlement.observation().outcome() != EditPhaseObservationOutcome::AcceptedReply
            || settlement.reserved_at() < closed.closure().occurred_at()
            || settlement.observation().finished_at() < settlement.reserved_at()
            || settlement.observation().finished_at() >= settlement.deadline()
            || settlement.deadline() != closed.fact().call().deadline()
            || settlement
                .observation()
                .reported_prompt_tokens()
                .is_none_or(|n| n == 0 || n > settlement.wire().prompt_estimate())
            || settlement
                .observation()
                .reported_output_tokens()
                .is_none_or(|n| n == 0 || n > generation)
            || settlement.wire().output_reserve() != reserve
            || settlement
                .wire()
                .prompt_estimate()
                .checked_add(reserve)
                .is_none_or(|n| n > context)
        {
            return Err(E::Binding);
        }
        let window = original.clone();
        let text_digest = *text_digest;
        let intent = intent.clone();
        Ok(Self {
            closed,
            settlement,
            profile,
            window,
            text_digest,
            intent,
        })
    }
    /// Actual first HTTP producer and exact A closure; still descriptive data.
    pub const fn closed_origin(&self) -> &ClosedHttpNoChangeOrigin {
        &self.closed
    }
    /// Fully settled real V6 call; no replay or dispatch permission.
    pub const fn settlement(&self) -> &HttpNoChangeReplanSettlement {
        &self.settlement
    }
    /// Complete unchanged model profile, not merely a reference.
    pub const fn profile(&self) -> &ModelProfile {
        &self.profile
    }
    /// Exact accepted strictly smaller full-line Original.
    pub const fn selected_window(&self) -> &EvidenceRef {
        &self.window
    }
    /// Digest of precisely selected currently supplied bytes.
    pub const fn text_digest(&self) -> ContentHash {
        self.text_digest
    }
    /// Unverified V6 intent; this is never implementation evidence.
    pub const fn intent(&self) -> &HttpCorrectionPlanIntent {
        &self.intent
    }
}

/// Pure exact definitions, retirement set and event IDs for the sole B material revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpNoChangeWindowMaterialPlan {
    change: TaskStepDefinition,
    proof: TaskStepDefinition,
    retired: [TaskStepId; 2],
    reason: TaskReplanReason,
    event: RunEventId,
    activation: [RunEventId; 3],
}
impl HttpNoChangeWindowMaterialPlan {
    /// New Addresses-only exact-path correction.
    pub const fn change(&self) -> &TaskStepDefinition {
        &self.change
    }
    /// Dependent full original HTTP Proves recipe, with no weakened check.
    pub const fn proof(&self) -> &TaskStepDefinition {
        &self.proof
    }
    /// Exactly the A-failed first correction and its unstarted dependent HTTP proof.
    pub const fn retired_steps(&self) -> &[TaskStepId; 2] {
        &self.retired
    }
    /// Exact deterministic plan-revision reason used by Application and transactional replay.
    pub const fn reason(&self) -> &TaskReplanReason {
        &self.reason
    }
    /// One immutable LedgerUpdated event at N+1.
    pub const fn event_id(&self) -> RunEventId {
        self.event
    }
    /// Existing three finite controller-edge IDs in execution order.
    pub const fn activation_events(&self) -> &[RunEventId; 3] {
        &self.activation
    }
    /// Moves only the normal plan-revision inputs; no source or execution authority.
    pub fn into_parts(self) -> ([TaskStepId; 2], Vec<TaskStepDefinition>, RunEventId) {
        (self.retired, vec![self.change, self.proof], self.event)
    }
}

/// Pure shape/definition derivation; persisted histories and live bytes remain separate duties.
pub fn derive_http_no_change_window_material_plan(
    material: &HttpNoChangeWindowMaterialProof,
    failed_correction: &TaskStepDefinition,
    pending_http_proof: &TaskStepDefinition,
) -> Result<HttpNoChangeWindowMaterialPlan, HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let owner = material.closed.fact().owner();
    let VerificationTarget::DiffInvariant(diff) = failed_correction.verification_spec().target()
    else {
        return Err(E::Binding);
    };
    let VerificationTarget::LoopbackHttp(recipe) = pending_http_proof.verification_spec().target()
    else {
        return Err(E::Binding);
    };
    if failed_correction.id() != owner.step
        || failed_correction.verification_spec().id() != owner.verification
        || failed_correction.addressed_criteria().is_empty()
        || !failed_correction.acceptance_criteria().is_empty()
        || failed_correction.parent_step_id().is_some()
        || !failed_correction.dependencies().is_empty()
        || failed_correction.planned_file_addition().is_some()
        || diff.mode() != DiffInvariantMode::ExactPaths
        || diff.paths() != std::slice::from_ref(material.window.revision().path())
        || pending_http_proof.parent_step_id().is_some()
        || pending_http_proof.dependencies() != [StepDependency::new(owner.step)]
        || pending_http_proof.acceptance_criteria() != failed_correction.addressed_criteria()
        || !pending_http_proof.addressed_criteria().is_empty()
        || pending_http_proof.planned_file_addition().is_some()
        || recipe.script() != material.window.revision().path()
        || recipe.id() != material.closed.parent().diagnostic.recipe_id()
    {
        return Err(E::UnsupportedGraph);
    }
    let id = |role| material_id(material, role);
    let correction = TaskStepId::from_bytes(id(1));
    let change = TaskStepDefinition::new(
        correction, None,
        TaskStepOutcome::try_from_string(material.intent.as_str().to_owned()).map_err(|_| E::Binding)?,
        TaskStepRationale::try_from_string(
            "Implement the genuinely smaller supplied window; the full original HTTP proof remains open".to_owned()
        ).map_err(|_| E::Binding)?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string("Current exact-path diff".to_owned()).map_err(|_| E::Binding)?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes(id(3)),
            VerificationRequirement::try_from_string("Change only the bound HTTP source".to_owned()).map_err(|_| E::Binding)?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths,
                vec![material.window.revision().path().clone()]).map_err(|_| E::Binding)?,
        ),
    ).and_then(|step| step.with_criterion_coverage(failed_correction.addressed_criteria().to_vec(), Vec::new()))
        .map_err(|_| E::Binding)?;
    let proof = TaskStepDefinition::new(
        TaskStepId::from_bytes(id(2)),
        None,
        pending_http_proof.intended_outcome().clone(),
        pending_http_proof.rationale().clone(),
        vec![StepDependency::new(correction)],
        pending_http_proof.expected_evidence().to_vec(),
        VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes(id(4)),
            pending_http_proof.verification_spec().requirement().clone(),
            recipe.clone(),
        ),
    )
    .and_then(|step| {
        step.with_acceptance_criteria(pending_http_proof.acceptance_criteria().to_vec())
    })
    .map_err(|_| E::Binding)?;
    Ok(HttpNoChangeWindowMaterialPlan {
        change,
        proof,
        retired: [failed_correction.id(), pending_http_proof.id()],
        reason: TaskReplanReason::try_from_string(
            "Replace the actual byte-identical failed correction with a strictly smaller source window and preserve the complete HTTP proof".to_owned()
        ).map_err(|_| E::Binding)?,
        event: RunEventId::from_bytes(id(5)),
        activation: [
            RunEventId::from_bytes(id(6)),
            RunEventId::from_bytes(id(7)),
            RunEventId::from_bytes(id(8)),
        ],
    })
}

fn material_id(material: &HttpNoChangeWindowMaterialProof, role: u8) -> [u8; 32] {
    material_id_from_parts(
        material.closed.fact(),
        &material.settlement,
        &material.window,
        material.text_digest,
        &material.intent,
        role,
    )
}

/// Derive content identities from descriptive records only. This authenticates no SQL history,
/// current publication, source bytes, reservation, or execution permission.
pub fn http_no_change_window_material_identity(
    fact: &ClosedEditNoChangeFact,
    settlement: &HttpNoChangeReplanSettlement,
) -> Result<([TaskStepId; 2], RunEventId, [RunEventId; 3]), HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let Some(HttpNoChangeReplanDecision::Window {
        first,
        last,
        original,
        text_digest,
        intent,
    }) = settlement.decision()
    else {
        return Err(E::NoWindow);
    };
    let old = fact.window().original();
    let range = original.range();
    if settlement.observation().outcome() != EditPhaseObservationOutcome::AcceptedReply
        || original.revision() != old.revision()
        || !old.range().contains(range)
        || range.is_empty()
        || range.len() >= old.range().len()
        || range.start_position().column() != 0
        || range.start_position().row().checked_add(1) != Some(first.get())
        || range
            .end_position()
            .row()
            .checked_add(u32::from(range.end_position().column() > 0))
            != Some(last.get())
        || first > last
    {
        return Err(E::Binding);
    }
    let id = |role| material_id_from_parts(fact, settlement, original, *text_digest, intent, role);
    Ok((
        [TaskStepId::from_bytes(id(1)), TaskStepId::from_bytes(id(2))],
        RunEventId::from_bytes(id(5)),
        [
            RunEventId::from_bytes(id(6)),
            RunEventId::from_bytes(id(7)),
            RunEventId::from_bytes(id(8)),
        ],
    ))
}

fn material_id_from_parts(
    fact: &ClosedEditNoChangeFact,
    settlement: &HttpNoChangeReplanSettlement,
    window: &EvidenceRef,
    text_digest: ContentHash,
    intent: &HttpCorrectionPlanIntent,
    role: u8,
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new_derive_key("a3.http-no-change-window-material.v1");
    hash.update(fact.owner().run.as_bytes());
    hash.update(fact.closure_event_id().as_bytes());
    hash.update(fact.job_digest().as_bytes());
    hash.update(fact.source_read_key().as_bytes());
    hash.update(settlement.job_digest().as_bytes());
    hash.update(settlement.wire().digest().as_bytes());
    hash.update(window.revision().content_hash().as_bytes());
    hash.update(text_digest.as_bytes());
    hash.update(&[match settlement.call_kind() {
        crate::EditPhaseCallKind::Initial => 0,
        crate::EditPhaseCallKind::StructuralRepair => 1,
    }]);
    let range = window.range();
    for value in [
        range.start_byte(),
        range.end_byte(),
        range.start_position().row(),
        range.start_position().column(),
        range.end_position().row(),
        range.end_position().column(),
    ] {
        hash.update(&value.to_le_bytes());
    }
    for bytes in [
        window.revision().path().as_bytes(),
        intent.as_str().as_bytes(),
    ] {
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.update(&[role]);
    *hash.finalize().as_bytes()
}

/// Material prefix reconstructed by the dedicated cold loader, without dispatch permission.
#[derive(Debug, Clone)]
pub struct AppliedHttpNoChangeWindowOrigin {
    material: HttpNoChangeWindowMaterialProof,
    applied_event: RunEvent,
}
impl AppliedHttpNoChangeWindowOrigin {
    /// Shape only; the adapter must prove the real accepted call and exact current prefix.
    pub fn from_recorded(
        material: HttpNoChangeWindowMaterialProof,
        applied_event: RunEvent,
    ) -> Result<Self, HttpNoChangeWindowFailure> {
        use HttpNoChangeWindowFailure as E;
        let owner = material.closed.fact().owner();
        let expected_revision = owner.ledger.get().checked_add(1).ok_or(E::Binding)?;
        if applied_event.id() != RunEventId::from_bytes(material_id(&material, 5))
            || applied_event.run_id() != owner.run
            || applied_event.snapshot_id() != owner.snapshot
            || material.closed.closure().sequence().get().checked_add(1)
                != Some(applied_event.sequence().get())
            || !matches!(applied_event.kind(), RunEventKind::LedgerUpdated { from, to }
                if from == owner.ledger && to.get() == expected_revision)
            || applied_event.occurred_at() < material.settlement.observation().finished_at()
            || applied_event.occurred_at() >= material.settlement.deadline()
            || applied_event.payload().code() != RunEventCode::ControllerDecision
            || applied_event.payload().outcome() != Some(RunEventOutcome::Succeeded)
            || applied_event.payload().redaction().is_some()
            || applied_event.subject().is_some()
            || applied_event.turn_charge().is_some()
        {
            return Err(E::Binding);
        }
        Ok(Self {
            material,
            applied_event,
        })
    }
    /// Same original A closure and accepted proper-subset call.
    pub const fn material(&self) -> &HttpNoChangeWindowMaterialProof {
        &self.material
    }
    /// One exact material LedgerUpdated event; not a successful implementation.
    pub const fn applied_event(&self) -> &RunEvent {
        &self.applied_event
    }
}

/// Checks the concrete first material's unchanged definitions before any B mutation.
pub(crate) fn original_material(
    closed: &ClosedHttpNoChangeOrigin,
    ledger: &a3_domain::TaskLedger,
) -> Result<crate::HttpCorrectionMaterialPlan, HttpNoChangeWindowFailure> {
    use HttpNoChangeWindowFailure as E;
    let parent = closed.parent();
    let original = ledger
        .step(parent.anchor.step)
        .ok_or(E::Binding)?
        .definition();
    let (job, focus, intent) = match &parent.producer {
        HttpCorrectionProducer::DirectV4Focus { job, proposal } => {
            let HttpCorrectionPlanDecision::FocusLine(focus) = proposal.decision() else {
                return Err(E::Binding);
            };
            (*job, focus, proposal.intent())
        }
        HttpCorrectionProducer::ContextV1Intent {
            anchor, job, reply, ..
        } => (*job, anchor.focus, reply.intent().ok_or(E::Binding)?),
    };
    let first =
        crate::derive_http_correction_material_plan(&parent.anchor, job, original, focus, intent)
            .map_err(|_| E::Binding)?;
    if first.event_id() != parent.applied_event.id()
        || first.change().id() != closed.fact().owner().step
        || ledger
            .step(first.change().id())
            .is_none_or(|step| step.definition() != first.change())
        || ledger
            .step(first.proof().id())
            .is_none_or(|step| step.definition() != first.proof())
    {
        return Err(E::Binding);
    }
    Ok(first)
}

/// Exact window continuation errors; none admits an automatic broader retry.
#[derive(Debug)]
pub enum HttpNoChangeWindowFailure {
    /// Cancellation won at a checked boundary.
    Cancelled,
    /// Current owner, actual lineage, source or chosen bytes disagree.
    Binding,
    /// Regular halt or terminal NoProgress cannot be materialized.
    NoWindow,
    /// The only supported isolated first-correction graph was not present.
    UnsupportedGraph,
    /// Original Run budget or deadline is exhausted.
    BudgetExhausted,
    /// Durable history could not be independently authenticated.
    Origin(crate::EditPhaseReservationFailure),
    /// Fresh bounded safe source read failed.
    Source(crate::AgentSourceReadFailure),
    /// Existing plan-revision boundary failed without exposing uncommitted copies.
    Replan(crate::ApplyAgentPlanRevisionError),
    /// Existing finite controller rejected an activation edge.
    Controller(crate::AgentControllerError),
    /// Existing journal CAS rejected a non-Execute edge.
    Journal(crate::RunJournalStoreFailure),
    /// Existing current-index Ledger/Journal CAS rejected the step start.
    Commit(crate::AgentActionStoreFailure),
}
impl fmt::Display for HttpNoChangeWindowFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HTTP NoChange window continuation could not be established")
    }
}
impl Error for HttpNoChangeWindowFailure {}

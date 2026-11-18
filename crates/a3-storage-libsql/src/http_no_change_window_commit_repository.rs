//! Material guard executed inside the existing Immediate Ledger/Journal/Index CAS.
use super::*;
use a3_application::{
    HttpCorrectionPlanDecision, HttpCorrectionProducer, HttpNoChangeWindowMaterialProof,
    derive_http_correction_material_plan, derive_http_no_change_window_material_plan,
};
pub(crate) struct MaterialCommitFrame<'a> {
    pub(crate) project: &'a ProjectIdentity,
    pub(crate) version: a3_application::TaskLedgerStoreVersion,
    pub(crate) sequence: RunEventSequence,
    pub(crate) ledger: &'a TaskLedger,
    pub(crate) run: &'a AgentRun,
    pub(crate) event: &'a RunEvent,
    pub(crate) proof: &'a HttpNoChangeWindowMaterialProof,
}
pub(crate) async fn guard(tx: &Transaction, frame: MaterialCommitFrame<'_>) -> Result<(), E> {
    let actual = super::no_change_replan::load_for_material(
        tx,
        frame.project,
        frame.proof,
        frame.event.occurred_at(),
    )
    .await?;
    if actual.stored.version() != frame.version
        || actual.run.last_event_sequence() != frame.sequence
        || frame.proof.closed_origin().closed_version() != frame.version
    {
        return Err(E::StaleAnchor);
    }
    let material = &actual.material;
    let closed = material.closed_origin();
    let parent = closed.parent();
    let original = actual
        .stored
        .ledger()
        .step(parent.anchor.step)
        .ok_or(E::InvalidStoredData)?
        .definition();
    let (job, focus, intent) = match &parent.producer {
        HttpCorrectionProducer::DirectV4Focus { job, proposal } => {
            let HttpCorrectionPlanDecision::FocusLine(focus) = proposal.decision() else {
                return Err(E::InvalidStoredData);
            };
            (*job, focus, proposal.intent())
        }
        HttpCorrectionProducer::ContextV1Intent {
            anchor, job, reply, ..
        } => (
            *job,
            anchor.focus,
            reply.intent().ok_or(E::InvalidStoredData)?,
        ),
    };
    let first = derive_http_correction_material_plan(&parent.anchor, job, original, focus, intent)
        .map_err(|_| E::InvalidStoredData)?;
    let ledger = actual.stored.ledger();
    let correction = first.change().id();
    let proof = first.proof().id();
    let failed = ledger.step(correction).ok_or(E::InvalidStoredData)?;
    let pending = ledger.step(proof).ok_or(E::InvalidStoredData)?;
    if first.event_id() != parent.applied_event.id()
        || correction != closed.fact().owner().step
        || failed.definition() != first.change()
        || pending.definition() != first.proof()
        || !failed.is_active_plan_step()
        || failed.status() != TaskStepStatus::Failed
        || failed.attempts().len() != 1
        || !pending.is_active_plan_step()
        || pending.status() != TaskStepStatus::Pending
        || !pending.attempts().is_empty()
        || ledger.steps().any(|step| {
            step.is_active_plan_step()
                && ![correction, proof].contains(&step.definition().id())
                && (step.status() != TaskStepStatus::Completed
                    || step
                        .definition()
                        .parent_step_id()
                        .is_some_and(|id| [correction, proof].contains(&id))
                    || step
                        .definition()
                        .dependencies()
                        .iter()
                        .any(|d| [correction, proof].contains(&d.prerequisite())))
        })
    {
        return Err(E::InvalidStoredData);
    }
    let plan = derive_http_no_change_window_material_plan(material, first.change(), first.proof())
        .map_err(|_| E::InvalidStoredData)?;
    a3_application::AdmitSourceTaskStep
        .execute(&actual.goal, plan.change())
        .map_err(|_| E::InvalidStoredData)?;
    a3_application::AdmitSourceTaskStep
        .execute(&actual.goal, plan.proof())
        .map_err(|_| E::InvalidStoredData)?;
    let mut expected_ledger = ledger.clone();
    let revision = expected_ledger
        .replan(
            plan.retired_steps().to_vec(),
            vec![plan.change().clone(), plan.proof().clone()],
            plan.reason().clone(),
            TaskLedgerTimestamp::from_unix_millis(frame.event.occurred_at().unix_millis())
                .map_err(|_| E::InvalidStoredData)?,
        )
        .map_err(|_| E::InvalidStoredData)?;
    let mut expected_run = actual.run;
    let expected_event = expected_run
        .record_ledger_update(
            plan.event_id(),
            revision,
            RunEventPayload::new(
                RunEventCode::ControllerDecision,
                Some(RunEventOutcome::Succeeded),
                None,
            ),
            expected_run.current_snapshot_id(),
            frame.event.occurred_at(),
        )
        .map_err(|_| E::InvalidStoredData)?;
    if &expected_ledger != frame.ledger
        || &expected_run != frame.run
        || &expected_event != frame.event
    {
        return Err(E::InvalidStoredData);
    }
    Ok(())
}

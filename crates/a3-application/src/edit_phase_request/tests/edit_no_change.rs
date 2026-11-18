//! Pure shape/aggregate tests. Real provenance and atomic CAS are covered natively.
use super::*;
use crate::{
    EditNoChangeFailure, EditNoChangeReceipt, EditPhaseCallKind, EditPhaseObservation,
    EditPhaseObservationOutcome, EditPhaseReservation, EditPhaseWireCost,
    derive_edit_no_change_closure,
};
use a3_domain::EvidenceRef;

fn at(n: u64) -> TestResult<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(n)?)
}
fn receipt(
    f: &Fixture,
    kind: EditPhaseCallKind,
    outcome: EditPhaseObservationOutcome,
    output: u32,
) -> TestResult<EditNoChangeReceipt> {
    let phase = PreparedEditPhase::update(f.state(&f.run)?, &f.page)?;
    let wire = EditPhaseWireCost::new(ContentHash::from_bytes([80; 32]), 200, 512, 1024)?;
    let permit = EditPhaseReservation::issue(
        &phase,
        kind,
        wire,
        at(11)?,
        at(f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis())?,
    )
    .with_source_read_key(ContentHash::from_bytes([81; 32]));
    Ok(EditNoChangeReceipt::from_recorded(
        &phase,
        &permit,
        EditPhaseObservation::new(outcome, Some(100), Some(output), at(12)?),
        EvidenceRef::new(f.page.revision().clone(), f.page.range()),
        &f.profile,
        &f.published,
        f.goal.original().ok_or("original missing")?.digest(),
    )?)
}

#[test]
fn no_change_receipt_has_exact_replace_scope_but_never_append_or_add() -> TestResult {
    let f = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        LATE,
        "value = 1\r\n",
    )?;
    let phase = PreparedEditPhase::update(f.state(&f.run)?, &f.page)?;
    let scope = phase.no_change_window().ok_or("replace scope missing")?;
    assert!(scope.matches_original(&EvidenceRef::new(f.page.revision().clone(), f.page.range())));
    assert_eq!(
        scope.digest().as_bytes(),
        blake3::hash(f.page.text().as_bytes()).as_bytes()
    );
    assert!(
        PreparedEditPhase::append(f.state(&f.run)?, &f.page)?
            .no_change_window()
            .is_none()
    );
    assert!(
        PreparedEditPhase::add(
            f.state(&f.run)?,
            RepositoryPath::try_from_bytes(b"new.py".to_vec())?
        )?
        .no_change_window()
        .is_none()
    );
    Ok(())
}

#[test]
fn generic_rejected_or_invalid_structure_is_not_an_authenticated_no_change() -> TestResult {
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    for outcome in [
        EditPhaseObservationOutcome::AcceptedReply,
        EditPhaseObservationOutcome::RejectedReply,
        EditPhaseObservationOutcome::InvalidStructure,
        EditPhaseObservationOutcome::Unknown,
        EditPhaseObservationOutcome::ProviderFailure,
        EditPhaseObservationOutcome::Cancelled,
    ] {
        assert!(receipt(&f, EditPhaseCallKind::Initial, outcome, 10).is_err());
    }
    assert!(
        receipt(
            &f,
            EditPhaseCallKind::Initial,
            EditPhaseObservationOutcome::NoChange,
            1025
        )
        .is_err()
    );
    assert!(
        receipt(
            &f,
            EditPhaseCallKind::Initial,
            EditPhaseObservationOutcome::NoChange,
            0
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn no_change_receipt_cannot_rebind_another_job_or_original() -> TestResult {
    let f = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        LATE,
        "value = 1\n",
    )?;
    let g = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        LATE,
        "value = 2\n",
    )?;
    let phase = PreparedEditPhase::update(f.state(&f.run)?, &f.page)?;
    let foreign = PreparedEditPhase::update(g.state(&g.run)?, &g.page)?;
    let wire = EditPhaseWireCost::new(ContentHash::from_bytes([80; 32]), 200, 512, 1024)?;
    let permit = EditPhaseReservation::issue(
        &foreign,
        EditPhaseCallKind::Initial,
        wire,
        at(11)?,
        at(900001)?,
    )
    .with_source_read_key(ContentHash::from_bytes([81; 32]));
    let observed = EditPhaseObservation::new(
        EditPhaseObservationOutcome::NoChange,
        Some(100),
        Some(10),
        at(12)?,
    );
    assert!(
        EditNoChangeReceipt::from_recorded(
            &phase,
            &permit,
            observed,
            EvidenceRef::new(f.page.revision().clone(), f.page.range()),
            &f.profile,
            &f.published,
            f.goal.original().ok_or("original missing")?.digest()
        )
        .is_err()
    );
    let permit = EditPhaseReservation::issue(
        &phase,
        EditPhaseCallKind::Initial,
        wire,
        at(11)?,
        at(900001)?,
    )
    .with_source_read_key(ContentHash::from_bytes([81; 32]));
    assert!(
        EditNoChangeReceipt::from_recorded(
            &phase,
            &permit,
            observed,
            EvidenceRef::new(g.page.revision().clone(), g.page.range()),
            &f.profile,
            &f.published,
            f.goal.original().ok_or("original missing")?.digest()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn no_change_closes_only_active_attempt_and_preserves_original_budget_and_proof_graph() -> TestResult
{
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let observed = receipt(
        &f,
        EditPhaseCallKind::Initial,
        EditPhaseObservationOutcome::NoChange,
        10,
    )?;
    let closure =
        derive_edit_no_change_closure(&f.goal, &f.run, f.stored.ledger(), &observed, at(13)?)?;
    assert_eq!(closure.run().id(), f.run.id());
    assert_eq!(closure.run().created_at(), f.run.created_at());
    assert_eq!(closure.run().budget(), f.run.budget());
    assert_eq!(closure.run().usage(), f.run.usage());
    assert_eq!(closure.run().state(), AgentControllerState::Replan);
    assert_eq!(closure.ledger().revision(), f.stored.ledger().revision());
    assert_eq!(
        closure.ledger().steps().count(),
        f.stored.ledger().steps().count()
    );
    let before = f
        .stored
        .ledger()
        .step(observed.anchor().step())
        .ok_or("step missing")?;
    let after = closure
        .ledger()
        .step(observed.anchor().step())
        .ok_or("closed step missing")?;
    assert_eq!(before.definition(), after.definition());
    assert_eq!(after.status(), TaskStepStatus::Failed);
    let [attempt] = after.attempts() else {
        return Err("exactly one original attempt required".into());
    };
    assert!(matches!(
        attempt.outcome(),
        TaskStepAttemptOutcome::Failed { .. }
    ));
    assert!(attempt.verification().is_none());
    assert_eq!(attempt.finished_at().map(|t| t.unix_millis()), Some(13));
    assert!(closure.ledger().replans().is_empty()); // no new decomposition or model job in this slice.
    assert!(matches!(
        derive_edit_no_change_closure(&f.goal, closure.run(), closure.ledger(), &observed, at(14)?),
        Err(EditNoChangeFailure::Binding)
    ));
    Ok(())
}

#[test]
fn no_change_closure_rejects_expiry_and_same_ref_different_goal_original() -> TestResult {
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let observed = receipt(
        &f,
        EditPhaseCallKind::Initial,
        EditPhaseObservationOutcome::NoChange,
        10,
    )?;
    assert!(
        derive_edit_no_change_closure(
            &f.goal,
            &f.run,
            f.stored.ledger(),
            &observed,
            observed.deadline()
        )
        .is_err()
    );
    let g = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        "A different mandatory constraint.",
        SOURCE,
    )?;
    assert_eq!(f.goal.reference(), g.goal.reference());
    assert!(matches!(
        derive_edit_no_change_closure(&g.goal, &f.run, f.stored.ledger(), &observed, at(13)?),
        Err(EditNoChangeFailure::Binding)
    ));
    Ok(())
}

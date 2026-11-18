//! Pure descriptive replay. Actual closed HTTP/SQL authority is a native contract.
use super::*;
use crate::{
    ClosedEditNoChangeCall, ClosedEditNoChangeFact, ClosedEditNoChangeOwner, ClosedEditWindow,
    ClosedNoChangeProfileLimits, EditNoChangeReceipt, EditPhaseCallKind, EditPhaseObservation,
    EditPhaseObservationOutcome, EditPhaseReservation, EditPhaseWireCost,
    derive_edit_no_change_closure, replay_closed_no_change_edit_job,
};
use a3_domain::{EvidenceRef, RunEventSequence};

fn at(n: u64) -> TestResult<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(n)?)
}
fn owner(f: &Fixture, phase: &PreparedEditPhase) -> TestResult<ClosedEditNoChangeOwner> {
    let a = phase.anchor();
    Ok(ClosedEditNoChangeOwner {
        worktree: a.worktree(),
        goal: a.goal(),
        original_goal: f.goal.original().ok_or("original missing")?.digest(),
        run: a.run(),
        ledger: a.ledger(),
        version: a.version(),
        sequence: a.sequence(),
        step: a.step(),
        attempt: a.attempt(),
        verification: a.verification(),
        index: f.published.run().id(),
        snapshot: a.snapshot(),
        profile: a.model_profile(),
    })
}
fn fact(
    f: &Fixture,
    page: &AgentSourcePage,
) -> TestResult<(ClosedEditNoChangeFact, PreparedEditPhase)> {
    let phase = PreparedEditPhase::update(f.state(&f.run)?, page)?;
    let window = phase.no_change_window().ok_or("window missing")?;
    let wire = EditPhaseWireCost::new(ContentHash::from_bytes([80; 32]), 200, 512, 1024)?;
    let call = ClosedEditNoChangeCall::from_recorded(
        EditPhaseCallKind::Initial,
        wire,
        at(11)?,
        at(f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis())?,
        EditPhaseObservation::new(
            EditPhaseObservationOutcome::NoChange,
            Some(100),
            Some(10),
            at(12)?,
        ),
    )?;
    let fact = ClosedEditNoChangeFact::from_recorded(
        owner(f, &phase)?,
        ClosedEditWindow::from_recorded(
            EvidenceRef::new(page.revision().clone(), page.range()),
            window.digest(),
        )?,
        phase.job_digest(),
        ContentHash::from_bytes([81; 32]),
        call,
        ClosedNoChangeProfileLimits::from_profile(&f.profile)?,
    )?;
    Ok((fact, phase))
}

#[test]
fn closed_replay_is_exact_v3_identity_without_a_current_execute_phase() -> TestResult {
    let f = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        LATE,
        "value = 1\r\n",
    )?;
    let (fact, phase) = fact(&f, &f.page)?;
    let permit = EditPhaseReservation::issue(
        &phase,
        fact.call().kind(),
        fact.call().wire(),
        fact.call().reserved_at(),
        fact.call().deadline(),
    )
    .with_source_read_key(fact.source_read_key());
    let live = EditNoChangeReceipt::from_recorded(
        &phase,
        &permit,
        fact.call().observation(),
        fact.window().original().clone(),
        &f.profile,
        &f.published,
        fact.owner().original_goal,
    )?;
    let closed = derive_edit_no_change_closure(&f.goal, &f.run, f.stored.ledger(), &live, at(13)?)?;
    assert_eq!(closed.event().id(), fact.closure_event_id());
    assert!(
        EditPhaseState::new(
            &f.project,
            &f.goal,
            &StoredTaskLedger::new(closed.ledger().clone(), TaskLedgerStoreVersion::new(2)?),
            closed.run(),
            &f.published
        )
        .is_err()
    );
    assert_eq!(
        replay_closed_no_change_edit_job(&fact, &f.page)?,
        phase.job_digest()
    );
    Ok(())
}

#[test]
fn closed_replay_accepts_complete_exact_interior_original_despite_more_file_lines() -> TestResult {
    let before = "head\r\nvalue = 1\r\ntail\r\n";
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, before)?;
    let first = "head\r\n".len();
    let text = "value = 1\r\n";
    let page = AgentSourcePage::new(
        f.page.revision().clone(),
        SourceRange::new(
            first,
            first + text.len(),
            SourcePosition::new(1, 0),
            SourcePosition::new(2, 0),
        )?,
        AgentFileStartLine::new(2)?,
        text.to_owned(),
        Some(AgentFileStartLine::new(3)?),
        true,
    )?;
    let (fact, phase) = fact(&f, &page)?;
    assert!(page.truncated()); // here: more file lines, not a missing part of this exact window.
    assert_eq!(
        replay_closed_no_change_edit_job(&fact, &page)?,
        phase.job_digest()
    );
    Ok(())
}

#[test]
fn closed_replay_rejects_wrong_window_bytes_coordinates_and_historical_job() -> TestResult {
    let f = Fixture::new(
        ModelPromptSchemaGrounding::FormatFieldOnly,
        LATE,
        "value = 1\n",
    )?;
    let (fact, _) = fact(&f, &f.page)?;
    let changed = AgentSourcePage::new(
        f.page.revision().clone(),
        f.page.range(),
        f.page.start_line(),
        "value = 2\n".to_owned(),
        None,
        false,
    )?;
    assert!(replay_closed_no_change_edit_job(&fact, &changed).is_err());
    let wrong_coordinates = AgentSourcePage::new(
        f.page.revision().clone(),
        SourceRange::new(
            0,
            f.page.text().len(),
            SourcePosition::new(1, 0),
            SourcePosition::new(2, 0),
        )?,
        AgentFileStartLine::new(2)?,
        f.page.text().to_owned(),
        None,
        false,
    )?;
    assert!(replay_closed_no_change_edit_job(&fact, &wrong_coordinates).is_err());
    let mut other_owner = fact.owner().clone();
    other_owner.sequence = RunEventSequence::new(other_owner.sequence.get() + 1)?;
    let foreign = ClosedEditNoChangeFact::from_recorded(
        other_owner,
        fact.window().clone(),
        fact.job_digest(),
        fact.source_read_key(),
        *fact.call(),
        fact.limits(),
    )?;
    assert!(replay_closed_no_change_edit_job(&foreign, &f.page).is_err());
    Ok(())
}

#[test]
fn closed_projection_cannot_claim_frame_authority_missing_from_legacy_v3() -> TestResult {
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let (fact, _) = fact(&f, &f.page)?;
    let mut owner = fact.owner().clone();
    owner.index = IndexRunId::from_bytes([79; 32]);
    let foreign = ClosedEditNoChangeFact::from_recorded(
        owner,
        fact.window().clone(),
        fact.job_digest(),
        fact.source_read_key(),
        *fact.call(),
        fact.limits(),
    )?;
    // Index identity was never part of v3. It stays a separate mandatory SQL
    // current-frame and A-closure binding; replay alone cannot authenticate it.
    assert_eq!(
        replay_closed_no_change_edit_job(&foreign, &f.page)?,
        fact.job_digest()
    );
    assert_ne!(foreign.closure_event_id(), fact.closure_event_id());
    Ok(())
}

#[test]
fn closed_shape_cannot_promote_generic_rejection_or_usage_over_actual_cap() -> TestResult {
    let f = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let (fact, _) = fact(&f, &f.page)?;
    assert!(
        ClosedEditNoChangeCall::from_recorded(
            fact.call().kind(),
            fact.call().wire(),
            fact.call().reserved_at(),
            fact.call().deadline(),
            EditPhaseObservation::new(
                EditPhaseObservationOutcome::RejectedReply,
                Some(100),
                Some(10),
                at(12)?
            )
        )
        .is_err()
    );
    let over = ClosedEditNoChangeCall::from_recorded(
        fact.call().kind(),
        fact.call().wire(),
        fact.call().reserved_at(),
        fact.call().deadline(),
        EditPhaseObservation::new(
            EditPhaseObservationOutcome::NoChange,
            Some(100),
            Some(1025),
            at(12)?,
        ),
    )?;
    assert!(
        ClosedEditNoChangeFact::from_recorded(
            fact.owner().clone(),
            fact.window().clone(),
            fact.job_digest(),
            fact.source_read_key(),
            over,
            fact.limits()
        )
        .is_err()
    );
    Ok(())
}

use super::*;
use a3_domain::PlannedFileAddition;

#[test]
fn selected_add_step_allows_only_its_add_and_binds_scope_against_none_downgrade() -> TestResult {
    let fixture = Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, SOURCE)?;
    let id = TaskStepId::from_bytes([3; 32]);
    let target = fixture.page.revision().path().clone();
    let addition = PlannedFileAddition::new(target.clone());
    let definition = fixture
        .stored
        .ledger()
        .step(id)
        .ok_or("step missing")?
        .definition()
        .clone()
        .with_planned_file_addition(addition.clone())?;
    let mut ledger = TaskLedger::new(
        fixture.goal.reference(),
        vec![definition],
        TaskLedgerTimestamp::from_unix_millis(1)?,
    )?;
    ledger.start_step(
        id,
        fixture.run.id(),
        TaskLedgerTimestamp::from_unix_millis(2)?,
    )?;
    let stored = StoredTaskLedger::new(ledger, fixture.stored.version());
    let empty = index_with_files(fixture.published.run().snapshot_id(), Vec::new())?;
    let bound = EditPhaseState::new(
        &fixture.project,
        &fixture.goal,
        &stored,
        &fixture.run,
        &empty,
    )?;
    let unbound = EditPhaseState::new(
        &fixture.project,
        &fixture.goal,
        &fixture.stored,
        &fixture.run,
        &empty,
    )?;
    assert_eq!(
        bound.anchor().planned_file_addition_scope(),
        Some(addition.scope_digest())
    );
    assert_ne!(bound.anchor(), unbound.anchor());
    let phase = PreparedEditPhase::add(bound, target.clone())?;
    assert!(
        PreparedEditPhase::add(
            bound,
            RepositoryPath::try_from_bytes(b"foreign.py".to_vec())?
        )
        .is_err()
    );
    let legacy = PreparedEditPhase::add(unbound, target)?;
    assert_ne!(phase.job_digest(), legacy.job_digest());
    assert!(matches!(
        build_edit_phase_request(
            &fixture.goal,
            &stored,
            &phase,
            unbound,
            fixture.profile.clone(),
            None,
            EditPhasePrerequisites::none()
        ),
        Err(EditPhaseRequestFailure::Stale)
    ));
    let bound_existing = EditPhaseState::new(
        &fixture.project,
        &fixture.goal,
        &stored,
        &fixture.run,
        &fixture.published,
    )?;
    assert!(PreparedEditPhase::update(bound_existing, &fixture.page).is_err());
    assert!(PreparedEditPhase::append(bound_existing, &fixture.page).is_err());
    let legacy_existing = fixture.state(&fixture.run)?;
    assert!(PreparedEditPhase::update(legacy_existing, &fixture.page).is_ok());
    assert!(PreparedEditPhase::append(legacy_existing, &fixture.page).is_ok());
    Ok(())
}

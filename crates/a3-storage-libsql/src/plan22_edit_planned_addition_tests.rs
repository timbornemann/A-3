//! Exact selected-operation identity is read from the current durable Ledger before dispatch.

use super::*;
use a3_domain::TaskLedger;
use a3_domain::{PlannedFileAddition, TaskStep, TaskStepMaterializedState};

#[test]
fn identical_ledger_geometry_cannot_drop_or_invent_selected_add_scope_before_dispatch() -> TestResult
{
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for stored_addition in [false, true] {
                let active = ActiveEdit::start_with_scope(false, stored_addition).await?;
                let correct = active.phase(&active.path)?;
                let ledger = active.ledger.ledger();
                let step = ledger.steps().next().ok_or("active step missing")?;
                let (_, base) = path_edit_goal(&active.path, b"Create only subject.py.\n")?;
                let forged_definition = if stored_addition {
                    base
                } else {
                    base.with_planned_file_addition(PlannedFileAddition::new(active.path.clone()))?
                };
                assert_ne!(step.definition(), &forged_definition);
                let forged_step = TaskStep::reconstruct(
                    forged_definition,
                    step.introduced_in_revision(),
                    TaskStepMaterializedState::new(
                        step.status(),
                        step.blocking_reason().cloned(),
                        step.stale_cause().cloned(),
                        step.retired_in_revision(),
                    ),
                    step.attempts().to_vec(),
                )?;
                let forged = StoredTaskLedger::new(
                    TaskLedger::reconstruct(
                        ledger.goal_contract(),
                        ledger.revision(),
                        vec![forged_step],
                        ledger.replans().to_vec(),
                        ledger.created_at(),
                        ledger.updated_at(),
                    )?,
                    active.ledger.version(),
                );
                let state = EditPhaseState::new(
                    &active.fixture.project,
                    &active.goal,
                    &forged,
                    &active.run,
                    &active.published,
                )?;
                let phase = PreparedEditPhase::add(state, active.path.clone())?;
                assert_eq!(phase.anchor().goal(), correct.anchor().goal());
                assert_eq!(phase.anchor().snapshot(), correct.anchor().snapshot());
                assert_eq!(phase.anchor().version(), correct.anchor().version());
                assert_eq!(phase.anchor().ledger(), correct.anchor().ledger());
                assert_eq!(phase.anchor().step(), correct.anchor().step());
                assert_eq!(phase.anchor().attempt(), correct.anchor().attempt());
                assert_ne!(
                    phase.anchor().planned_file_addition_scope(),
                    correct.anchor().planned_file_addition_scope()
                );
                assert_ne!(phase.job_digest(), correct.job_digest());
                for kind in [Initial, StructuralRepair] {
                    assert!(matches!(
                        active
                            .store
                            .reserve_edit_phase(
                                &active.fixture.project,
                                &phase,
                                kind,
                                wire(1)?,
                                at(8)?
                            )
                            .await,
                        Err(Failure::StaleAnchor)
                    ));
                }
                assert_eq!(active.usage().await?, EditPhaseReservedUsage::default());
                active.assert_run_and_ledger_unchanged().await?;
                let active = active.reopen().await?;
                let phase = active.phase(&active.path)?;
                assert_eq!(phase.job_digest(), correct.job_digest());
                let permit = active
                    .store
                    .reserve_edit_phase(&active.fixture.project, &phase, Initial, wire(1)?, at(8)?)
                    .await?;
                active
                    .store
                    .finish_edit_phase(
                        &active.fixture.project,
                        permit,
                        observation(Outcome::AcceptedReply, 9)?,
                    )
                    .await?;
                let usage = active.usage().await?;
                assert_eq!(
                    (
                        usage.calls,
                        usage.repairs,
                        usage.unobserved_calls,
                        usage.unknown_calls
                    ),
                    (1, 0, 0, 0)
                );
                active.assert_run_and_ledger_unchanged().await?;
                active.cleanup()?;
            }
            Ok(())
        })
    })
}

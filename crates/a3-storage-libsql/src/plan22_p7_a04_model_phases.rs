//! Actual bounded model jobs for the first empty-project implementation horizon.

use super::*;
use a3_application::{
    EditPhaseCallKind, EditPhaseExecutionFailure, EditPhaseFailure, EditPhaseOutcome,
    EditPhasePrerequisites, EditPhaseState, ExecuteEditPhase, build_edit_phase_request,
};
use a3_domain::PlannedFileAddition;

pub(super) struct A04Phases<'a> {
    pub(super) fixture: &'a Fixture,
    pub(super) store: &'a LibsqlKnowledgeStore,
    pub(super) provider: &'a OllamaModelProvider,
    pub(super) profile: &'a ModelProfile,
    pub(super) clock: &'a A04Clock,
    pub(super) dispatches: &'a Mutex<Vec<Value>>,
}

impl A04Phases<'_> {
    pub(super) async fn plan(&self, promoted: &Promoted) -> TestResult<StoredTaskLedger> {
        let goal = &promoted.data.goal;
        let [criterion] = goal.draft().acceptance_criteria() else {
            return Err("A04 requires the complete original-bound outcome".into());
        };
        let step = promoted.data.step.id();
        let diff = VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes(identity("p7.a04.change-spec", 0)),
            VerificationRequirement::try_from_string("Create only server.py".to_owned())?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![RepositoryPath::try_from_bytes(b"server.py".to_vec())?],
            )?,
        );
        let phase = PreparedPlanPhase::new(
            promoted.state()?,
            vec![
                PlanStepOffer::new(step, vec![criterion.id()], Vec::new(), diff.clone())
                    .with_planned_file_addition(PlannedFileAddition::new(
                        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
                    ))?,
            ],
            None,
        )?;
        let initial = self.observer("plan", INITIAL);
        let execute = |observer| {
            ExecuteInitialPlan::new(observer, self.store, self.store, &NoSourceReads, self.store)
        };
        let initial_cursor = dispatch_cursor(self.dispatches);
        let result = execute(&initial)
            .execute(
                &self.fixture.project,
                &phase,
                INITIAL,
                self.clock.now()?,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        record_plan_failure(self.dispatches, initial_cursor, INITIAL, &result);
        let report = match result {
            Err(error) if repairable_plan_reply(&error) => {
                let repair = self.observer("plan", EditPhaseCallKind::StructuralRepair);
                let repair_cursor = dispatch_cursor(self.dispatches);
                let result = execute(&repair)
                    .execute(
                        &self.fixture.project,
                        &phase,
                        EditPhaseCallKind::StructuralRepair,
                        self.clock.now()?,
                        ModelRequestTimeout::from_millis(180_000)?,
                        &ActiveControl,
                    )
                    .await;
                record_plan_failure(
                    self.dispatches,
                    repair_cursor,
                    EditPhaseCallKind::StructuralRepair,
                    &result,
                );
                result?
            }
            result => result?,
        };
        let stored = match report.outcome {
            InitialPlanExecutionOutcome::Materialized(stored) => stored,
            InitialPlanExecutionOutcome::Halted(reason) => {
                self.record_disposition(
                    "plan",
                    json!({"kind":"halted","reason":format!("{reason:?}")}),
                )?;
                return Err("A04 Plan returned a regular nonmutating decision halt".into());
            }
            _ => return Err("A04 Plan returned a regular nonmutating halt".into()),
        };
        let selected = stored
            .ledger()
            .step(step)
            .ok_or("A04 implementation step missing")?;
        assert_eq!(stored.ledger().steps().len(), 1);
        assert_eq!(selected.status(), TaskStepStatus::Ready);
        assert_eq!(
            selected.definition().addressed_criteria(),
            &[criterion.id()]
        );
        assert!(selected.definition().acceptance_criteria().is_empty());
        assert_eq!(selected.definition().verification_spec(), &diff);
        assert_eq!(
            selected.definition().intended_outcome().as_str(),
            "Create server.py"
        );
        assert_eq!(
            selected
                .definition()
                .planned_file_addition()
                .map(|addition| addition.target().as_bytes()),
            Some(b"server.py".as_slice())
        );
        self.record_disposition(
            "plan",
            json!({"kind":"steps","outcome":selected.definition().intended_outcome().as_str()}),
        )?;
        Ok(stored)
    }

    pub(super) async fn add(
        &self,
        goal: &GoalContract,
        stored: &StoredTaskLedger,
        run: &AgentRun,
        published: &PublishedIndex,
    ) -> TestResult<AgentAction> {
        assert!(published.publication().graph().files().is_empty());
        let state = EditPhaseState::new(&self.fixture.project, goal, stored, run, published)?;
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let phase = PreparedEditPhase::add(state, path.clone())?;
        let memory =
            RunMemoryCheckpoint::compile(goal, stored.ledger(), run, published, Vec::new())?;
        let compiled = build_edit_phase_request(
            goal,
            stored,
            &phase,
            state,
            self.profile.clone(),
            Some(&memory),
            EditPhasePrerequisites::none(),
        )?;
        let patch = WorkspacePatchAdapter::with_change_window_preview();
        let initial = self.observer("edit", INITIAL);
        let result = ExecuteEditPhase::new(&initial, self.store, &patch)
            .execute(
                phase,
                state,
                &compiled,
                self.clock.now()?,
                ModelRequestTimeout::from_millis(180_000)?,
                &ActiveControl,
            )
            .await;
        let report = match result {
            Err(EditPhaseExecutionFailure::OutputLimit) => {
                let repair = self.observer("edit", EditPhaseCallKind::StructuralRepair);
                let request = compiled.for_truncated_output_repair()?;
                ExecuteEditPhase::new(&repair, self.store, &patch)
                    .execute(
                        PreparedEditPhase::add(state, path)?,
                        state,
                        &request,
                        self.clock.now()?,
                        ModelRequestTimeout::from_millis(180_000)?,
                        &ActiveControl,
                    )
                    .await?
            }
            Err(EditPhaseExecutionFailure::Reply(
                EditPhaseFailure::InvalidReply
                | EditPhaseFailure::InvalidReplacement
                | EditPhaseFailure::TooLarge,
            )) => {
                let repair = self.observer("edit", EditPhaseCallKind::StructuralRepair);
                let request = compiled.for_structural_repair()?;
                ExecuteEditPhase::new(&repair, self.store, &patch)
                    .execute(
                        PreparedEditPhase::add(state, path)?,
                        state,
                        &request,
                        self.clock.now()?,
                        ModelRequestTimeout::from_millis(180_000)?,
                        &ActiveControl,
                    )
                    .await?
            }
            result => result?,
        };
        match report.outcome {
            EditPhaseOutcome::Proposal(action) => {
                self.record_disposition("edit", json!({"kind":"proposal"}))?;
                Ok(action)
            }
            EditPhaseOutcome::NeedEvidence(need) => {
                self.record_disposition(
                    "edit",
                    json!({"kind":"needEvidence","need":format!("{need:?}")}),
                )?;
                Err("A04 Edit requested more evidence without mutation".into())
            }
            EditPhaseOutcome::TooLarge => {
                self.record_disposition("edit", json!({"kind":"tooLarge"}))?;
                Err("A04 Edit requires a smaller scope without mutation".into())
            }
        }
    }

    fn observer(&self, phase: &'static str, kind: EditPhaseCallKind) -> ObservedPhaseProvider<'_> {
        let observer = ObservedPhaseProvider::new(self.provider, self.dispatches, phase, kind);
        if phase == "plan" {
            observer.with_a04_plan_reply_capture()
        } else {
            observer
        }
    }

    fn record_disposition(&self, phase: &str, value: Value) -> TestResult {
        let mut dispatches = self
            .dispatches
            .lock()
            .map_err(|_| "A04 phase recorder poisoned")?;
        let latest = dispatches
            .last_mut()
            .ok_or("A04 phase observation missing")?;
        if latest["phase"] != phase {
            return Err("A04 phase observation differs".into());
        }
        latest["disposition"] = value;
        Ok(())
    }
}

fn dispatch_cursor(dispatches: &Mutex<Vec<Value>>) -> Option<usize> {
    dispatches.lock().ok().map(|receipts| receipts.len())
}

fn record_plan_failure<T>(
    dispatches: &Mutex<Vec<Value>>,
    before: Option<usize>,
    kind: EditPhaseCallKind,
    result: &Result<T, a3_application::InitialPlanExecutionFailure>,
) {
    let Err(error) = result else { return };
    let Some(before) = before else { return };
    let Ok(mut receipts) = dispatches.lock() else {
        return;
    };
    if before.checked_add(1) != Some(receipts.len()) {
        return;
    }
    let Some(receipt) = receipts.get_mut(before) else {
        return;
    };
    let expected = match kind {
        EditPhaseCallKind::Initial => "initial",
        EditPhaseCallKind::StructuralRepair => "structural_repair",
    };
    if receipt["phase"] != "plan" || receipt["kind"] != expected {
        return;
    }
    let mut diagnostic = format!("{error:?}");
    let mut end = diagnostic.len().min(256);
    while !diagnostic.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = end < diagnostic.len();
    diagnostic.truncate(end);
    receipt["disposition"] = json!({"kind":"failure","error":diagnostic,"truncated":truncated});
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use a3_application::{InitialPlanExecutionFailure, PlanPhaseFailure};

    #[test]
    fn failure_before_dispatch_keeps_prior_metadata_and_the_original_error() -> TestResult {
        let receipts = Mutex::new(vec![json!({"phase":"plan","kind":"initial"})]);
        let before = dispatch_cursor(&receipts);
        let result: Result<(), _> = Err(InitialPlanExecutionFailure::Binding);
        record_plan_failure(
            &receipts,
            before,
            EditPhaseCallKind::StructuralRepair,
            &result,
        );
        assert!(matches!(result, Err(InitialPlanExecutionFailure::Binding)));
        assert_eq!(
            receipts
                .into_inner()
                .map_err(|_| "receipt mutex poisoned")?,
            vec![json!({"phase":"plan","kind":"initial"})]
        );
        Ok(())
    }

    #[test]
    fn failure_disposition_belongs_only_to_this_calls_plan_dispatch_and_kind() -> TestResult {
        let receipts = Mutex::new(vec![json!({"phase":"plan","kind":"initial"})]);
        let before = dispatch_cursor(&receipts);
        receipts
            .lock()
            .map_err(|_| "receipt mutex poisoned")?
            .push(json!({"phase":"plan","kind":"structural_repair"}));
        let result: Result<(), _> = Err(InitialPlanExecutionFailure::Reply(
            PlanPhaseFailure::InvalidReply,
        ));
        record_plan_failure(&receipts, before, INITIAL, &result);
        assert!(
            receipts.lock().map_err(|_| "receipt mutex poisoned")?[1]
                .get("disposition")
                .is_none()
        );
        record_plan_failure(
            &receipts,
            before,
            EditPhaseCallKind::StructuralRepair,
            &result,
        );
        let receipts = receipts
            .into_inner()
            .map_err(|_| "receipt mutex poisoned")?;
        assert!(receipts[0].get("disposition").is_none());
        assert_eq!(receipts[1]["disposition"]["error"], "Reply(InvalidReply)");
        Ok(())
    }
}

//! Core-bound A01 Intake and actual bounded Plan and Edit jobs.

#[path = "plan22_p5_a01_plan_reply_capture.rs"]
mod plan_reply_capture;

use super::{ActiveControl, Fixture, TestResult};
use crate::LibsqlKnowledgeStore;
use a3_application::{
    AgentSourcePage, AgentSourceReader, CaptureGoalIntake, EditPhaseCallKind,
    EditPhaseExecutionFailure, EditPhaseFailure, EditPhaseOutcome, EditPhasePrerequisites,
    EditPhaseState, ExecuteEditPhase, ExecuteInitialPlan, InitialPlanExecutionFailure,
    InitialPlanExecutionOutcome, IntakePhaseAccounting, IntakePhaseAnchor,
    IntakePhaseReservationStore, ModelFinishReason, ModelOperationControl, ModelProvider,
    ModelProviderFailure, ModelProviderFuture, ModelProviderRequest, ModelRequestTimeout,
    ModelWireProjection, ModelWireThinking, PlanPhaseFailure, PlanPhaseState, PlanStepOffer,
    PreparedEditPhase, PreparedPlanPhase, PromoteGoalIntake, ProviderEvent, ProviderEventStream,
    StoredTaskLedger, build_edit_phase_request, prepare_goal_from_intake_review,
};
use a3_domain::{
    AcceptanceCriterionRequirement, AgentAction, AgentFileInspection, AgentFileLineCount,
    AgentFileStartLine, AgentRun, AgentRunBudget, AgentRunTimestamp, CriterionEvidenceRequirement,
    DiffInvariantMode, DiffInvariantVerification, DiscoveredCommandId, GoalContract,
    GoalContractTimestamp, GoalIntakeFinding, GoalIntakeFindingKind, GoalIntakeReview,
    GoalIntakeReviewBatchProposal, MinimumTestCaseCount, ModelProfile, ModelProviderId,
    ProjectCommandCatalog, PublishedIndex, PythonIntegerTranslation, RepositoryPath,
    RunMemoryCheckpoint, StepDependency, TaskId, TaskStepId, TaskStepStatus, TestCaseSelector,
    VerificationRequirement, VerificationScope, VerificationSpec, VerificationSpecId,
};
use a3_provider::OllamaModelProvider;
use a3_workspace::{WorkspaceAgentSourceReader, WorkspacePatchAdapter};
use futures::StreamExt;
use plan_reply_capture::{BoundedPlanReplyCapture, PlanReplyCaptureMode};
use serde_json::{Value, json};
use std::{fmt, sync::Mutex, time::Instant};

pub(super) const TASK: TaskId = TaskId::from_bytes([221; 32]);
pub(super) const CHANGE_STEP: TaskStepId = TaskStepId::from_bytes([222; 32]);
pub(super) const CHECK_STEP: TaskStepId = TaskStepId::from_bytes([223; 32]);
const CAPTURE_MILLIS: u64 = 1;
const ALLOWANCE_MILLIS: u64 = 360_000;
const PROVIDER_TIMEOUT_MILLIS: u64 = 300_000;
const MAX_DISPATCHES: usize = 12;

// The frozen request explicitly supplies the behavior, protected test and exact argv.
// The trusted fixture binds their roles. The complete literal original is still stored.
const WINDOWS: [&str; 4] = [
    "Korrigiere subject.py: adjust(value) erh\u{f6}ht jede ganze Zahl exakt um eins. ",
    "Keine Konstantantworten; vorhandene Tests unver\u{e4}ndert lassen und ausf\u{fc}hren. ",
    "Keine neuen Tests oder Abh\u{e4}ngigkeiten.\n\n",
    "Zul\u{e4}ssige Pr\u{fc}faufrufe im Projektroot, mit exakten Argumentgrenzen:\n[\"python\", \"-B\", \"-m\", \"unittest\", \"discover\", \"-s\", \"tests\"]",
];

pub(super) struct LiveA01Phases<'a> {
    pub(super) fixture: &'a Fixture,
    pub(super) store: &'a LibsqlKnowledgeStore,
    pub(super) provider: &'a OllamaModelProvider,
    pub(super) profile: ModelProfile,
    pub(super) clock: Instant,
    pub(super) allowance: AgentRunBudget,
    pub(super) dispatches: &'a Mutex<Vec<Value>>,
}

impl LiveA01Phases<'_> {
    pub(super) async fn prepare(
        &self,
        published: &PublishedIndex,
        catalog: &ProjectCommandCatalog,
        original: &str,
        command: DiscoveredCommandId,
        translation: PythonIntegerTranslation,
    ) -> TestResult<(GoalContract, StoredTaskLedger, IntakePhaseAccounting)> {
        if WINDOWS.concat() != original
            || translation.path().as_bytes() != b"subject.py"
            || translation.function().as_str() != "adjust"
            || translation.delta() != 1
            || self.allowance.duration_limit().millis() != ALLOWANCE_MILLIS
        {
            return Err("frozen A01 phase scope or deadline differs".into());
        }
        let project = &self.fixture.project;
        let captured_at = GoalContractTimestamp::from_unix_millis(CAPTURE_MILLIS)?;
        let captured = CaptureGoalIntake::new(self.store)
            .execute(project, TASK, original.as_bytes().to_vec(), captured_at)
            .await?;
        let minimum_cases = MinimumTestCaseCount::new(1)?;
        let proof = CriterionEvidenceRequirement::ExistingTest {
            command,
            selector: TestCaseSelector::All,
            minimum_cases,
            scope: VerificationScope::Workspace,
            python_integer_translation: Some(translation.clone()),
        };
        let original_source = captured.pending().original();
        let entire = original_source.range(0, original.len())?;
        let roles = [
            GoalIntakeFindingKind::Outcome {
                requirement: AcceptanceCriterionRequirement::Must,
                evidence_requirement: proof,
            },
            GoalIntakeFindingKind::Constraint,
            GoalIntakeFindingKind::NonGoal,
            GoalIntakeFindingKind::Constraint,
        ];
        let mut findings = Vec::with_capacity(roles.len());
        let mut start = 0;
        for (expected, role) in WINDOWS.into_iter().zip(roles) {
            let end = start + expected.len();
            let range = original_source.range(start, end)?;
            findings.push(GoalIntakeFinding::new(
                role,
                range,
                original_source.resolve(range)?.to_owned(),
            )?);
            start = end;
        }
        let review = GoalIntakeReview::begin(
            captured.pending(),
            GoalIntakeReviewBatchProposal::new(entire, findings, Vec::new())?,
        )?;
        let anchor =
            IntakePhaseAnchor::new(project, &captured, None, entire, self.profile.reference())?;
        let accounting = self
            .store
            .commit_bound_intake(
                project,
                anchor,
                &review,
                self.allowance,
                observed_at(self.clock)?,
            )
            .await?;
        let goal = prepare_goal_from_intake_review(captured.pending(), &review)?;
        if goal
            .original()
            .map(a3_domain::GoalOriginalRequest::as_bytes)
            != Some(original.as_bytes())
        {
            return Err("prepared A01 Goal changed the original".into());
        }
        let reference = PromoteGoalIntake::new(self.store)
            .execute(project, TASK, captured.version(), &goal)
            .await?;
        if reference != goal.reference() {
            return Err("promoted A01 Goal differs from the prepared Goal".into());
        }
        let durable_accounting = self
            .store
            .load_intake_phase_accounting(project, TASK)
            .await?
            .ok_or("A01 Intake accounting missing")?;
        if durable_accounting != accounting
            || accounting.usage() != a3_application::EditPhaseReservedUsage::default()
            || accounting.budget() != self.allowance
            || accounting.profile() != self.profile.reference()
            || accounting.captured_at() != captured_at
            || accounting.deadline().unix_millis() != CAPTURE_MILLIS + ALLOWANCE_MILLIS
            || accounting.linked_run().is_some()
        {
            return Err("A01 Intake budget or absolute deadline changed".into());
        }
        let stored = self
            .plan(published, catalog, &goal, command, translation)
            .await?;
        Ok((goal, stored, accounting))
    }

    async fn plan(
        &self,
        published: &PublishedIndex,
        catalog: &ProjectCommandCatalog,
        goal: &GoalContract,
        command: DiscoveredCommandId,
        translation: PythonIntegerTranslation,
    ) -> TestResult<StoredTaskLedger> {
        let project = &self.fixture.project;
        let target = translation.path().clone();
        let page = self.read_target(published, &target).await?;
        let mut criteria = goal
            .draft()
            .acceptance_criteria()
            .iter()
            .map(|criterion| criterion.id())
            .collect::<Vec<_>>();
        criteria.sort_unstable();
        let diff = VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([222; 32]),
            VerificationRequirement::try_from_string("Only subject.py changes".to_owned())?,
            DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![target])?,
        );
        let test = VerificationSpec::test_with_python_integer_translation(
            VerificationSpecId::from_bytes([223; 32]),
            VerificationRequirement::try_from_string(
                "Run the unchanged unittest and check the exact integer translation".to_owned(),
            )?,
            command,
            TestCaseSelector::All,
            MinimumTestCaseCount::new(1)?,
            VerificationScope::Workspace,
            translation,
        );
        let state = PlanPhaseState::new(project, goal, published, &self.profile, Some(catalog))?;
        let phase = PreparedPlanPhase::new(
            state,
            vec![
                PlanStepOffer::new(CHANGE_STEP, criteria.clone(), Vec::new(), diff.clone()),
                PlanStepOffer::new(CHECK_STEP, Vec::new(), criteria.clone(), test.clone())
                    .with_required_offers(vec![CHANGE_STEP]),
            ],
            Some(&page),
        )?;
        let initial = self.observer("plan", EditPhaseCallKind::Initial);
        let timeout = ModelRequestTimeout::from_millis(PROVIDER_TIMEOUT_MILLIS)?;
        let result = ExecuteInitialPlan::new(
            &initial,
            self.store,
            self.store,
            &WorkspaceAgentSourceReader,
            self.store,
        )
        .execute(
            project,
            &phase,
            EditPhaseCallKind::Initial,
            observed_at(self.clock)?,
            timeout,
            &ActiveControl,
        )
        .await;
        let report = match result {
            Err(error)
                if matches!(
                    error,
                    InitialPlanExecutionFailure::Reply(
                        PlanPhaseFailure::InvalidReply
                            | PlanPhaseFailure::UnknownOffer
                            | PlanPhaseFailure::DuplicateOffer
                            | PlanPhaseFailure::InvalidDependency
                            | PlanPhaseFailure::Capacity,
                    )
                ) || (matches!(error, InitialPlanExecutionFailure::IncompleteReply)
                    && self.last_output_limit("plan")) =>
            {
                let repair = self.observer("plan", EditPhaseCallKind::StructuralRepair);
                ExecuteInitialPlan::new(
                    &repair,
                    self.store,
                    self.store,
                    &WorkspaceAgentSourceReader,
                    self.store,
                )
                .execute(
                    project,
                    &phase,
                    EditPhaseCallKind::StructuralRepair,
                    observed_at(self.clock)?,
                    timeout,
                    &ActiveControl,
                )
                .await?
            }
            result => result?,
        };
        let InitialPlanExecutionOutcome::Materialized(stored) = report.outcome else {
            return Err("A01 Plan did not materialize a ready Ledger".into());
        };
        let ledger = stored.ledger();
        let change = ledger.step(CHANGE_STEP).ok_or("A01 Plan omitted change")?;
        let check = ledger
            .step(CHECK_STEP)
            .ok_or("A01 Plan omitted guarded check")?;
        if ledger.steps().len() != 2
            || change.status() != TaskStepStatus::Ready
            || check.status() != TaskStepStatus::Pending
            || change.definition().addressed_criteria() != criteria.as_slice()
            || !change.definition().acceptance_criteria().is_empty()
            || !change.definition().dependencies().is_empty()
            || change.definition().verification_spec() != &diff
            || !check.definition().addressed_criteria().is_empty()
            || check.definition().acceptance_criteria() != criteria.as_slice()
            || check.definition().dependencies() != [StepDependency::new(CHANGE_STEP)]
            || check.definition().verification_spec() != &test
            || change.definition().intended_outcome() == check.definition().intended_outcome()
            || !change.attempts().is_empty()
            || !check.attempts().is_empty()
        {
            return Err("A01 Plan did not retain exact change and check responsibilities".into());
        }
        Ok(stored)
    }

    pub(super) async fn edit(
        &self,
        goal: &GoalContract,
        stored: &StoredTaskLedger,
        run: &AgentRun,
        published: &PublishedIndex,
        target: &RepositoryPath,
    ) -> TestResult<AgentAction> {
        if target.as_bytes() != b"subject.py" {
            return Err("A01 Edit target is outside the fixture".into());
        }
        let project = &self.fixture.project;
        let page = self.read_target(published, target).await?;
        let current = EditPhaseState::new(project, goal, stored, run, published)?;
        if current.anchor().step() != CHANGE_STEP {
            return Err("A01 Edit lacks the active change step".into());
        }
        let phase = PreparedEditPhase::update(current, &page)?;
        let memory =
            RunMemoryCheckpoint::compile(goal, stored.ledger(), run, published, Vec::new())?;
        let mut compiled = build_edit_phase_request(
            goal,
            stored,
            &phase,
            current,
            self.profile.clone(),
            Some(&memory),
            EditPhasePrerequisites::none(),
        )?;
        let patch = WorkspacePatchAdapter::with_change_window_preview();
        let initial = self.observer("edit", EditPhaseCallKind::Initial);
        let timeout = ModelRequestTimeout::from_millis(PROVIDER_TIMEOUT_MILLIS)?;
        let result = ExecuteEditPhase::new(&initial, self.store, &patch)
            .execute(
                Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                    phase,
                    &mut compiled,
                    current,
                    observed_at(self.clock)?,
                    &patch,
                    &ActiveControl,
                ))
                .await?,
                current,
                &compiled,
                observed_at(self.clock)?,
                timeout,
                &ActiveControl,
            )
            .await;
        let report = match result {
            Err(
                EditPhaseExecutionFailure::OutputLimit
                | EditPhaseExecutionFailure::Reply(
                    EditPhaseFailure::InvalidReply
                    | EditPhaseFailure::InvalidReplacement
                    | EditPhaseFailure::TooLarge,
                ),
            ) => {
                let repair = self.observer("edit", EditPhaseCallKind::StructuralRepair);
                let mut compiled = compiled.for_structural_repair()?;
                let phase = PreparedEditPhase::update(current, &page)?;
                ExecuteEditPhase::new(&repair, self.store, &patch)
                    .execute(
                        Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                            phase,
                            &mut compiled,
                            current,
                            observed_at(self.clock)?,
                            &patch,
                            &ActiveControl,
                        ))
                        .await?,
                        current,
                        &compiled,
                        observed_at(self.clock)?,
                        timeout,
                        &ActiveControl,
                    )
                    .await?
            }
            result => result?,
        };
        let EditPhaseOutcome::Proposal(action) = report.outcome else {
            return Err("A01 Edit did not propose an exact patch".into());
        };
        Ok(action)
    }

    async fn read_target(
        &self,
        published: &PublishedIndex,
        target: &RepositoryPath,
    ) -> TestResult<AgentSourcePage> {
        let revision = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| revision.path() == target)
            .ok_or("A01 target missing from the current publication")?;
        let inspection = AgentFileInspection::new(
            target.clone(),
            AgentFileStartLine::new(1)?,
            AgentFileLineCount::new(8)?,
        );
        let page = WorkspaceAgentSourceReader
            .read_page(&self.fixture.project, revision, &inspection, &ActiveControl)
            .await?;
        if page.truncated() || page.next_start_line().is_some() || page.range().is_empty() {
            return Err("A01 target does not fit an exact complete source page".into());
        }
        Ok(page)
    }

    fn observer(&self, phase: &'static str, kind: EditPhaseCallKind) -> ObservedPhaseProvider<'_> {
        ObservedPhaseProvider::new(self.provider, self.dispatches, phase, kind)
    }

    fn last_output_limit(&self, phase: &str) -> bool {
        self.dispatches.lock().is_ok_and(|receipts| {
            receipts
                .last()
                .is_some_and(|last| last["phase"] == phase && last["finish"] == "output_limit")
        })
    }
}

pub(super) fn observed_at(clock: Instant) -> TestResult<AgentRunTimestamp> {
    Ok(AgentRunTimestamp::from_unix_millis(
        CAPTURE_MILLIS
            .checked_add(u64::try_from(clock.elapsed().as_millis())?)
            .ok_or("A01 phase clock overflow")?,
    )?)
}

pub(in super::super) struct ObservedPhaseProvider<'a> {
    inner: &'a OllamaModelProvider,
    dispatches: &'a Mutex<Vec<Value>>,
    phase: &'static str,
    kind: EditPhaseCallKind,
    plan_reply_capture: PlanReplyCaptureMode,
}

impl fmt::Debug for ObservedPhaseProvider<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservedPhaseProvider")
    }
}

impl ModelProvider for ObservedPhaseProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.inner.provider_id()
    }

    fn stream<'a>(
        &'a self,
        _request: &'a ModelProviderRequest,
        _timeout: ModelRequestTimeout,
        _control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.inner.project_wire_request(request)
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let settings = request.profile().settings();
            if projection.context_tokens() != settings.context_limit().get()
                || projection.output_tokens() != settings.output_limit().get()
                || projection.thinking() != ModelWireThinking::Disabled
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let index = {
                let mut receipts = self
                    .dispatches
                    .lock()
                    .map_err(|_| ModelProviderFailure::Unavailable)?;
                let index = receipts.len();
                if index >= MAX_DISPATCHES {
                    return Err(ModelProviderFailure::Rejected);
                }
                receipts.push(json!({
                    "phase":self.phase,
                    "kind":match self.kind {
                        EditPhaseCallKind::Initial => "initial",
                        EditPhaseCallKind::StructuralRepair => "structural_repair",
                    },
                    "wire_blake3":blake3::Hash::from_bytes(*projection.digest().as_bytes()).to_hex().to_string(),
                    "request_bytes":projection.request_bytes(),
                    "num_ctx":projection.context_tokens(),
                    "num_predict":projection.output_tokens(),
                    "think":false,
                    "finish":"unobserved",
                    "prompt_tokens":null,
                    "output_tokens":null,
                }));
                index
            };
            let stream = match self
                .inner
                .stream_projected(request, projection, timeout, control)
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    self.finish(index, "provider_error", None, None, None)?;
                    return Err(error);
                }
            };
            let mut capture = self.plan_reply_capture.start(self.phase);
            let stream = stream.map(move |event| {
                match &event {
                    Ok(ProviderEvent::Completed(completion)) => {
                        let reason = match completion.reason() {
                            ModelFinishReason::Stop => "stop",
                            ModelFinishReason::OutputLimit => "output_limit",
                            ModelFinishReason::Other => "other",
                        };
                        let usage = completion.usage();
                        self.finish(
                            index,
                            reason,
                            usage.prompt_tokens(),
                            usage.output_tokens(),
                            capture.as_ref(),
                        )?;
                    }
                    Err(_) => self.finish(index, "provider_error", None, None, None)?,
                    Ok(ProviderEvent::OutputText(text)) => {
                        if let Some(capture) = &mut capture {
                            capture.push(text.as_str());
                        }
                    }
                }
                event
            });
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

impl ObservedPhaseProvider<'_> {
    pub(in super::super) fn new<'a>(
        inner: &'a OllamaModelProvider,
        dispatches: &'a Mutex<Vec<Value>>,
        phase: &'static str,
        kind: EditPhaseCallKind,
    ) -> ObservedPhaseProvider<'a> {
        ObservedPhaseProvider {
            inner,
            dispatches,
            phase,
            kind,
            plan_reply_capture: PlanReplyCaptureMode::Off,
        }
    }

    pub(in super::super) fn with_a04_plan_reply_capture(mut self) -> Self {
        self.plan_reply_capture = PlanReplyCaptureMode::A04Plan;
        self
    }

    fn finish(
        &self,
        index: usize,
        reason: &'static str,
        prompt: Option<u64>,
        output: Option<u64>,
        capture: Option<&BoundedPlanReplyCapture>,
    ) -> Result<(), ModelProviderFailure> {
        let mut receipts = self
            .dispatches
            .lock()
            .map_err(|_| ModelProviderFailure::Unavailable)?;
        let receipt = receipts
            .get_mut(index)
            .ok_or(ModelProviderFailure::InvalidResponse)?;
        if receipt["finish"] != "unobserved" {
            return Err(ModelProviderFailure::InvalidResponse);
        }
        receipt["finish"] = json!(reason);
        receipt["prompt_tokens"] = json!(prompt);
        receipt["output_tokens"] = json!(output);
        if let Some(capture) = capture {
            receipt["plan_reply"] = capture.value();
        }
        Ok(())
    }
}

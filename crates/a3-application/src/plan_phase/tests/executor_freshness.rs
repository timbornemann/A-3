use super::*;
use crate::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, AgentSourceReaderFuture,
    EditPhaseCallKind, EditPhaseReservedUsage, EditPhaseWireCost, ExecuteInitialPlan,
    GoalContractStore, GoalContractStoreFailure, GoalContractStoreFuture,
    InitialPlanExecutionFailure, InitialPlanFinish, InitialPlanRejection, InitialPlanReservation,
    InitialPlanReservationFailure, InitialPlanReservationFuture, InitialPlanReservationStore,
    ModelCancellationFuture, ModelFinishReason, ModelOutputChunk, ModelProviderCompletion,
    ModelProviderUsage, ProviderEvent, ProviderEventStream, StoredTaskLedger, TaskLensControl,
    TaskLensControlError, TaskLensIndexStore, TaskLensIndexStoreFuture,
};
use a3_domain::{AgentFileInspection, AgentRunTimestamp, GoalContractRevision, Progress};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, Mutex},
};

#[test]
fn stale_before_reservation_and_same_geometry_wrong_bytes_never_dispatch() -> TestResult {
    run_freshness_failure(Some(1), false, 0)?;
    run_freshness_failure(None, true, 0)
}

#[test]
fn stale_after_the_reply_settles_without_a_ledger_or_structure_repair() -> TestResult {
    run_freshness_failure(Some(2), false, 1)
}

fn run_freshness_failure(
    stale_at: Option<usize>,
    mismatched_page: bool,
    sends: usize,
) -> TestResult {
    let fixture = Fixture::new(SOURCE)?;
    let goal = fixture.goal(2, fixture.diff_requirement()?, CONSTRAINT)?;
    let phase = PreparedPlanPhase::new(
        fixture.state(&goal, None)?,
        fixture.offers(2)?,
        Some(&fixture.page),
    )?;
    let authority = Authority {
        goal,
        index: Arc::new(fixture.published.clone()),
    };
    let current = if mismatched_page {
        let changed = SOURCE.replace("mean", "calc");
        assert_eq!(changed.len(), SOURCE.len());
        page(fixture.page.revision().clone(), &changed)?
    } else {
        fixture.page.clone()
    };
    let source = Reader {
        current,
        stale_at,
        reads: AtomicUsize::new(0),
    };
    let reservations = Reservations::default();
    let provider = Provider {
        projector: Projector(fixture.profile.provider_id().clone()),
        sends: AtomicUsize::new(0),
        reader: &source,
        reservations: &reservations,
        reply: json!({
            "version":1,"job":phase.alias,
            "steps":[{"offer":"P1","outcome":"Correct the mean calculation"}],
        })
        .to_string(),
    };
    let result = futures::executor::block_on(
        ExecuteInitialPlan::new(&provider, &authority, &authority, &source, &reservations).execute(
            &fixture.project,
            &phase,
            EditPhaseCallKind::Initial,
            AgentRunTimestamp::from_unix_millis(1)?,
            ModelRequestTimeout::DEFAULT,
            &Control,
        ),
    );
    assert!(matches!(
        result,
        Err(InitialPlanExecutionFailure::Source(
            AgentSourceReadFailure::Stale
        ))
    ));
    assert_eq!(provider.sends.load(Ordering::SeqCst), sends);
    assert_eq!(reservations.charges.load(Ordering::SeqCst), sends);
    assert_eq!(source.reads.load(Ordering::SeqCst), sends + 1);
    let settled = reservations
        .settled
        .lock()
        .map_err(|_| "reservation spy lock poisoned")?;
    if sends == 0 {
        assert!(settled.is_empty());
    } else {
        assert_eq!(
            *settled,
            [(InitialPlanRejection::RejectedReply, Some(32), Some(12))]
        );
    }
    Ok(())
}

#[derive(Debug)]
struct Authority {
    goal: GoalContract,
    index: Arc<PublishedIndex>,
}

impl GoalContractStore for Authority {
    fn create_goal_contract<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async { Err(GoalContractStoreFailure::Unavailable) })
    }

    fn append_goal_contract_revision<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async { Err(GoalContractStoreFailure::Unavailable) })
    }

    fn load_current_goal_contract<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        task: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async move { Ok((task == self.goal.task_id()).then(|| self.goal.clone())) })
    }

    fn load_goal_contract_revision<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskId,
        _: GoalContractRevision,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async { Err(GoalContractStoreFailure::Unavailable) })
    }
}

impl TaskLensIndexStore for Authority {
    fn load_current_index<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a> {
        Box::pin(async { Ok(Some(self.index.clone())) })
    }
}

#[derive(Debug)]
struct Reader {
    current: AgentSourcePage,
    stale_at: Option<usize>,
    reads: AtomicUsize,
}

impl AgentSourceReader for Reader {
    fn read_page<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        expected: &'a FileRevision,
        request: &'a AgentFileInspection,
        _: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(async move {
            let read = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            if Some(read) == self.stale_at {
                return Err(AgentSourceReadFailure::Stale);
            }
            if expected != self.current.revision()
                || request.path() != expected.path()
                || request.start_line() != self.current.start_line()
                || usize::from(request.line_count().get()) != self.current.text().lines().count()
            {
                return Err(AgentSourceReadFailure::InvalidPage);
            }
            Ok(self.current.clone())
        })
    }
}

type SettledFailure = (InitialPlanRejection, Option<u32>, Option<u32>);

#[derive(Debug, Default)]
struct Reservations {
    charges: AtomicUsize,
    settled: Mutex<Vec<SettledFailure>>,
}

impl InitialPlanReservationStore for Reservations {
    fn reserve_initial_plan<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        phase: &'a PreparedPlanPhase,
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        now: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, InitialPlanReservation> {
        Box::pin(async move {
            self.charges.fetch_add(1, Ordering::SeqCst);
            let deadline = AgentRunTimestamp::from_unix_millis(120_000)
                .map_err(|_| InitialPlanReservationFailure::StalePlan)?;
            Ok(InitialPlanReservation::issue(
                phase, kind, wire, now, deadline,
            ))
        })
    }

    fn finish_initial_plan<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: InitialPlanReservation,
        outcome: InitialPlanFinish,
        _: AgentRunTimestamp,
    ) -> InitialPlanReservationFuture<'a, Option<StoredTaskLedger>> {
        Box::pin(async move {
            let InitialPlanFinish::ObservedFailure {
                kind,
                reported_prompt_tokens,
                reported_output_tokens,
            } = outcome
            else {
                return Err(InitialPlanReservationFailure::InvalidPlan);
            };
            self.settled
                .lock()
                .map_err(|_| InitialPlanReservationFailure::StalePlan)?
                .push((kind, reported_prompt_tokens, reported_output_tokens));
            Ok(None)
        })
    }

    fn load_initial_plan_usage<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskId,
    ) -> InitialPlanReservationFuture<'a, EditPhaseReservedUsage> {
        Box::pin(async { Err(InitialPlanReservationFailure::ScopeMissing) })
    }

    fn load_initial_plan_halt<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: TaskId,
    ) -> InitialPlanReservationFuture<'a, Option<crate::PlanPhaseHalt>> {
        Box::pin(async { Err(InitialPlanReservationFailure::ScopeMissing) })
    }
}

#[derive(Debug)]
struct Provider<'a> {
    projector: Projector,
    sends: AtomicUsize,
    reader: &'a Reader,
    reservations: &'a Reservations,
    reply: String,
}

impl ModelProvider for Provider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.projector.provider_id()
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async { Err(ModelProviderFailure::Rejected) })
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.projector.project_wire_request(request)
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.sends.fetch_add(1, Ordering::SeqCst);
            if self.reservations.charges.load(Ordering::SeqCst) != 1
                || self.reader.reads.load(Ordering::SeqCst) != 1
                || self.project_wire_request(request)? != projection
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let output = ModelOutputChunk::try_from_string(self.reply.clone())
                .map_err(|_| ModelProviderFailure::InvalidResponse)?;
            Ok(Box::pin(futures::stream::iter([
                Ok(ProviderEvent::OutputText(output)),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(32), Some(12)),
                ))),
            ])) as ProviderEventStream<'a>)
        })
    }
}

#[derive(Debug)]
struct Control;

impl ModelOperationControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(futures::future::pending())
    }
}

impl TaskLensControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl AgentSourceReadControl for Control {
    fn is_cancelled(&self) -> bool {
        false
    }
}

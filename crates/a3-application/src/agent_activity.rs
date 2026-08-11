use crate::{
    GetTaskLensTask, RunEventPageLimit, RunJournalStore, RunJournalStoreFailure,
    TaskLensTaskAnchor, TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
    TaskLensWorkspaceStore,
};
use a3_domain::{
    AgentRun, ProjectIdentity, RunEvent, RunEventSequence, TaskId, TaskStepAttempt,
    TaskStepAttemptOutcome, TaskStepId,
};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const RECENT_RUN_EVENT_LIMIT: u16 = 64;

/// One ledger-selected run and its bounded most-recent journal window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivityRun {
    step_id: TaskStepId,
    attempt_number: u32,
    run: AgentRun,
    events: Vec<RunEvent>,
    earlier_events_omitted: bool,
}

impl AgentActivityRun {
    /// Returns the step whose retained attempt selected this run.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step_id
    }

    /// Returns the one-based attempt number local to the selected step.
    #[must_use]
    pub const fn attempt_number(&self) -> u32 {
        self.attempt_number
    }

    /// Returns the independently materialized current run state.
    #[must_use]
    pub const fn run(&self) -> &AgentRun {
        &self.run
    }

    /// Returns at most the latest sixty-four contiguous journal events.
    #[must_use]
    pub fn events(&self) -> &[RunEvent] {
        &self.events
    }

    /// Returns whether older events precede the returned bounded window.
    #[must_use]
    pub const fn earlier_events_omitted(&self) -> bool {
        self.earlier_events_omitted
    }
}

/// Current durable task anchors and optional latest execution activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivity {
    anchor: TaskLensTaskAnchor,
    run: Option<AgentActivityRun>,
}

impl AgentActivity {
    /// Returns the consistent current Goal Contract and Task Ledger read.
    #[must_use]
    pub const fn anchor(&self) -> &TaskLensTaskAnchor {
        &self.anchor
    }

    /// Returns the active or latest retained step attempt, when one exists.
    #[must_use]
    pub const fn run(&self) -> Option<&AgentActivityRun> {
        self.run.as_ref()
    }
}

/// Expected states while loading one selected task's execution activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentActivityLoadResult {
    /// No current Goal Contract exists for the task.
    TaskNotFound,
    /// The task exists but has no Task Ledger yet.
    LedgerUnavailable,
    /// The ledger still refers to an earlier immutable Goal Contract revision.
    GoalRevisionMismatch {
        /// Current immutable Goal Contract revision.
        current_revision: u32,
        /// Revision still materialized by the Task Ledger.
        ledger_revision: u32,
    },
    /// The task anchor changed during the multi-store bounded read.
    ActivityChanged,
    /// Current ledger state and optional execution activity are consistent.
    Available(Box<AgentActivity>),
}

/// Loads bounded execution activity without accepting a WebView-selected run identity.
#[derive(Debug, Clone)]
pub struct GetAgentActivity {
    task: GetTaskLensTask,
    journal: Arc<dyn RunJournalStore>,
}

impl GetAgentActivity {
    /// Composes the existing atomic task workspace and run-journal ports.
    #[must_use]
    pub const fn new(
        workspace: Arc<dyn TaskLensWorkspaceStore>,
        journal: Arc<dyn RunJournalStore>,
    ) -> Self {
        Self {
            task: GetTaskLensTask::new(workspace),
            journal,
        }
    }

    /// Derives the relevant run from durable attempts and returns its latest bounded events.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<AgentActivityLoadResult, GetAgentActivityFailure> {
        let initial = match self.load_anchor(project, task_id, control).await? {
            LoadedAnchor::Expected(result) => return Ok(result),
            LoadedAnchor::Available(anchor) => anchor,
        };
        let selected_attempt = select_run_attempt(&initial);
        let run = match selected_attempt {
            Some(selected) => match self.load_activity_run(project, &initial, selected).await? {
                Some(run) => Some(run),
                None => return Ok(AgentActivityLoadResult::ActivityChanged),
            },
            None => None,
        };

        let current = match self.load_anchor(project, task_id, control).await? {
            LoadedAnchor::Available(anchor) if anchor == initial => anchor,
            LoadedAnchor::Available(_) | LoadedAnchor::Expected(_) => {
                return Ok(AgentActivityLoadResult::ActivityChanged);
            }
        };
        if let Some(activity_run) = &run {
            let latest_run = self
                .journal
                .load_agent_run(project, activity_run.run().id())
                .await
                .map_err(GetAgentActivityFailure::Journal)?
                .ok_or(GetAgentActivityFailure::InvalidRunAnchor)?;
            if latest_run != *activity_run.run() {
                return Ok(AgentActivityLoadResult::ActivityChanged);
            }
        }
        Ok(AgentActivityLoadResult::Available(Box::new(
            AgentActivity {
                anchor: *current,
                run,
            },
        )))
    }

    async fn load_anchor(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        control: &dyn TaskLensWorkspaceControl,
    ) -> Result<LoadedAnchor, GetAgentActivityFailure> {
        self.task
            .execute(project, task_id, control)
            .await
            .map_err(GetAgentActivityFailure::Workspace)
            .map(|result| match result {
                TaskLensTaskLoadResult::NotFound => {
                    LoadedAnchor::Expected(AgentActivityLoadResult::TaskNotFound)
                }
                TaskLensTaskLoadResult::LedgerUnavailable { .. } => {
                    LoadedAnchor::Expected(AgentActivityLoadResult::LedgerUnavailable)
                }
                TaskLensTaskLoadResult::GoalRevisionMismatch {
                    current_goal,
                    ledger_goal,
                } => LoadedAnchor::Expected(AgentActivityLoadResult::GoalRevisionMismatch {
                    current_revision: current_goal.revision().get(),
                    ledger_revision: ledger_goal.revision().get(),
                }),
                TaskLensTaskLoadResult::Available(anchor) => {
                    LoadedAnchor::Available(Box::new(anchor))
                }
            })
    }

    async fn load_activity_run(
        &self,
        project: &ProjectIdentity,
        anchor: &TaskLensTaskAnchor,
        selected: SelectedAttempt<'_>,
    ) -> Result<Option<AgentActivityRun>, GetAgentActivityFailure> {
        let run_id = selected.attempt.run_id();
        let run = self
            .journal
            .load_agent_run(project, run_id)
            .await
            .map_err(GetAgentActivityFailure::Journal)?
            .ok_or(GetAgentActivityFailure::InvalidRunAnchor)?;
        let ledger = anchor.task_ledger().ledger();
        if run.id() != run_id
            || run.goal_contract() != anchor.goal_contract().reference()
            || run.task_ledger_revision() > ledger.revision()
        {
            return Err(GetAgentActivityFailure::InvalidRunAnchor);
        }

        let last_sequence = run.last_event_sequence();
        let earlier_events_omitted = last_sequence.get() > u64::from(RECENT_RUN_EVENT_LIMIT);
        let after_sequence = if earlier_events_omitted {
            let value = last_sequence
                .get()
                .checked_sub(u64::from(RECENT_RUN_EVENT_LIMIT))
                .ok_or(GetAgentActivityFailure::InvalidRunAnchor)?;
            Some(
                RunEventSequence::new(value)
                    .map_err(|_| GetAgentActivityFailure::InvalidRunAnchor)?,
            )
        } else {
            None
        };
        let limit = RunEventPageLimit::new(RECENT_RUN_EVENT_LIMIT)
            .map_err(|_| GetAgentActivityFailure::InvalidConfiguration)?;
        let page = self
            .journal
            .load_run_events(project, run_id, after_sequence, limit)
            .await
            .map_err(GetAgentActivityFailure::Journal)?;
        if page.events().iter().any(|event| event.run_id() != run_id) {
            return Err(GetAgentActivityFailure::InvalidRunAnchor);
        }
        if page.has_more() || page.events().last().map(RunEvent::sequence) != Some(last_sequence) {
            return Ok(None);
        }
        Ok(Some(AgentActivityRun {
            step_id: selected.step_id,
            attempt_number: selected.attempt.number().get(),
            run,
            events: page.events().to_vec(),
            earlier_events_omitted,
        }))
    }
}

enum LoadedAnchor {
    Expected(AgentActivityLoadResult),
    Available(Box<TaskLensTaskAnchor>),
}

#[derive(Clone, Copy)]
struct SelectedAttempt<'a> {
    step_id: TaskStepId,
    attempt: &'a TaskStepAttempt,
}

fn select_run_attempt(anchor: &TaskLensTaskAnchor) -> Option<SelectedAttempt<'_>> {
    let attempts = anchor.task_ledger().ledger().steps().flat_map(|step| {
        step.attempts().iter().map(move |attempt| SelectedAttempt {
            step_id: step.definition().id(),
            attempt,
        })
    });
    let mut selected: Option<SelectedAttempt<'_>> = None;
    for candidate in attempts {
        let candidate_active =
            matches!(candidate.attempt.outcome(), TaskStepAttemptOutcome::Active);
        let replace = selected.is_none_or(|current| {
            let current_active =
                matches!(current.attempt.outcome(), TaskStepAttemptOutcome::Active);
            (
                candidate_active,
                candidate.attempt.started_at(),
                candidate.step_id,
                candidate.attempt.number(),
            ) > (
                current_active,
                current.attempt.started_at(),
                current.step_id,
                current.attempt.number(),
            )
        });
        if replace {
            selected = Some(candidate);
        }
    }
    selected
}

/// Stable failure classification for bounded Agent workspace activity reads.
#[derive(Debug)]
pub enum GetAgentActivityFailure {
    /// Current Goal Contract or Task Ledger storage failed.
    Workspace(TaskLensWorkspaceFailure),
    /// Materialized run or journal storage failed.
    Journal(RunJournalStoreFailure),
    /// A durable attempt, materialized run, or event page contradicted its anchor.
    InvalidRunAnchor,
    /// The compile-time event window was outside the application port's fixed range.
    InvalidConfiguration,
}

impl fmt::Display for GetAgentActivityFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Workspace(_) => "Agent activity task workspace failed",
            Self::Journal(_) => "Agent activity run journal failed",
            Self::InvalidRunAnchor => "Agent activity durable run anchor is invalid",
            Self::InvalidConfiguration => "Agent activity event window is invalid",
        })
    }
}

impl Error for GetAgentActivityFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::InvalidRunAnchor | Self::InvalidConfiguration => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentActivityLoadResult, GetAgentActivity};
    use crate::{
        RecordedAgentRead, RunEventPage, RunEventPageLimit, RunJournalStore,
        RunJournalStoreFailure, RunJournalStoreFuture, StoredTaskLedger, TaskLedgerStoreVersion,
        TaskLensWorkspaceControl, TaskLensWorkspaceFailure, TaskLensWorkspaceFuture,
        TaskLensWorkspaceGoalPage, TaskLensWorkspaceStore, TaskLensWorkspaceTask,
        TaskLensWorkspaceTaskLimit,
    };
    use a3_domain::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentRun,
        AgentRunId, AgentRunTimestamp, CanonicalDirectory, ExpectedTaskEvidence, GitHead,
        GitReferenceName, GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective,
        ModelProfileId, ModelProfileReference, ModelProfileVersion, ProjectIdentity, RepositoryId,
        RepositoryIdentity, RunEvent, RunEventId, RunEventKind, RunEventPayload, RunEventSequence,
        SnapshotId, SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp,
        TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod,
        VerificationRequirement, VerificationSpec, VerificationSpecId, WorktreeAnchorId,
        WorktreeId, WorktreeIdentity,
    };
    use std::error::Error;
    use std::sync::Arc;

    #[derive(Debug)]
    struct Control;

    impl TaskLensWorkspaceControl for Control {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    #[derive(Debug, Clone)]
    struct Store {
        task: TaskLensWorkspaceTask,
        run: AgentRun,
        events: Vec<RunEvent>,
    }

    impl TaskLensWorkspaceStore for Store {
        fn list_current_goal_contracts<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            limit: TaskLensWorkspaceTaskLimit,
            _control: &'a dyn TaskLensWorkspaceControl,
        ) -> TaskLensWorkspaceFuture<'a, TaskLensWorkspaceGoalPage> {
            Box::pin(async move {
                TaskLensWorkspaceGoalPage::new(
                    vec![self.task.goal_contract().clone()],
                    false,
                    limit,
                )
                .map_err(|_| TaskLensWorkspaceFailure::InvalidStoredData)
            })
        }

        fn load_current_task<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _task_id: TaskId,
            _control: &'a dyn TaskLensWorkspaceControl,
        ) -> TaskLensWorkspaceFuture<'a, Option<TaskLensWorkspaceTask>> {
            Box::pin(async move { Ok(Some(self.task.clone())) })
        }
    }

    impl RunJournalStore for Store {
        fn create_agent_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run: &'a AgentRun,
            _start_event: &'a RunEvent,
        ) -> RunJournalStoreFuture<'a, ()> {
            Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
        }

        fn append_run_event<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_last_sequence: RunEventSequence,
            _run: &'a AgentRun,
            _event: &'a RunEvent,
        ) -> RunJournalStoreFuture<'a, ()> {
            Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
        }

        fn append_agent_read<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_last_sequence: RunEventSequence,
            _run: &'a AgentRun,
            _read: &'a RecordedAgentRead,
        ) -> RunJournalStoreFuture<'a, ()> {
            Box::pin(async { Err(RunJournalStoreFailure::Unavailable) })
        }

        fn load_agent_run<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
        ) -> RunJournalStoreFuture<'a, Option<AgentRun>> {
            Box::pin(async move { Ok(Some(self.run.clone())) })
        }

        fn load_run_events<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _run_id: AgentRunId,
            after_sequence: Option<RunEventSequence>,
            limit: RunEventPageLimit,
        ) -> RunJournalStoreFuture<'a, RunEventPage> {
            Box::pin(async move {
                let after = after_sequence.map_or(0, RunEventSequence::get);
                let events = self
                    .events
                    .iter()
                    .filter(|event| event.sequence().get() > after)
                    .take(usize::from(limit.get()))
                    .cloned()
                    .collect();
                RunEventPage::new(after_sequence, limit, events, false)
                    .map_err(|_| RunJournalStoreFailure::InvalidStoredData)
            })
        }
    }

    #[test]
    fn task_derived_run_returns_revalidated_bounded_activity() -> Result<(), Box<dyn Error>> {
        let (goal, mut ledger, step_id) = anchors()?;
        let run_id = AgentRunId::from_bytes([94; 32]);
        ledger.start_step(step_id, run_id, TaskLedgerTimestamp::from_unix_millis(2)?)?;
        let (run, start_event) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            ModelProfileReference::new(
                ModelProfileId::from_bytes([95; 32]),
                ModelProfileVersion::V1,
            ),
            SnapshotId::from_bytes([96; 32]),
            RunEventId::from_bytes([97; 32]),
            AgentRunTimestamp::from_unix_millis(2)?,
        )?;
        let task = TaskLensWorkspaceTask::new(
            goal.clone(),
            Some(StoredTaskLedger::new(
                ledger,
                TaskLedgerStoreVersion::INITIAL,
            )),
        );
        let store = Arc::new(Store {
            task,
            run,
            events: vec![start_event],
        });
        let result =
            futures::executor::block_on(GetAgentActivity::new(store.clone(), store).execute(
                &project()?,
                goal.task_id(),
                &Control,
            ))?;
        let AgentActivityLoadResult::Available(activity) = result else {
            return Err("expected available Agent activity".into());
        };
        let Some(activity_run) = activity.run() else {
            return Err("expected ledger-selected run".into());
        };
        assert_eq!(activity_run.step_id(), step_id);
        assert_eq!(activity_run.run().id(), run_id);
        assert_eq!(activity_run.events().len(), 1);
        assert!(!activity_run.earlier_events_omitted());
        Ok(())
    }

    #[test]
    fn activity_returns_only_the_latest_sixty_four_contiguous_events()
    -> Result<(), Box<dyn Error>> {
        let (goal, mut ledger, step_id) = anchors()?;
        let run_id = AgentRunId::from_bytes([84; 32]);
        ledger.start_step(
            step_id,
            run_id,
            TaskLedgerTimestamp::from_unix_millis(2)?,
        )?;
        let snapshot_id = SnapshotId::from_bytes([86; 32]);
        let (mut run, start_event) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            ModelProfileReference::new(
                ModelProfileId::from_bytes([85; 32]),
                ModelProfileVersion::V1,
            ),
            snapshot_id,
            RunEventId::from_bytes([87; 32]),
            AgentRunTimestamp::from_unix_millis(2)?,
        )?;
        let mut events = vec![start_event];
        for offset in 0_u8..64 {
            events.push(run.record(
                RunEventId::from_bytes([offset; 32]),
                RunEventKind::Diagnostic,
                RunEventPayload::empty(),
                snapshot_id,
                None,
                AgentRunTimestamp::from_unix_millis(u64::from(offset) + 3)?,
            )?);
        }
        let task = TaskLensWorkspaceTask::new(
            goal.clone(),
            Some(StoredTaskLedger::new(
                ledger,
                TaskLedgerStoreVersion::INITIAL,
            )),
        );
        let store = Arc::new(Store { task, run, events });
        let result = futures::executor::block_on(
            GetAgentActivity::new(store.clone(), store).execute(
                &project()?,
                goal.task_id(),
                &Control,
            ),
        )?;
        let AgentActivityLoadResult::Available(activity) = result else {
            return Err("expected available Agent activity".into());
        };
        let Some(activity_run) = activity.run() else {
            return Err("expected ledger-selected run".into());
        };
        assert_eq!(activity_run.events().len(), 64);
        assert_eq!(
            activity_run.events().first().map(RunEvent::sequence),
            Some(RunEventSequence::new(2)?)
        );
        assert_eq!(
            activity_run.events().last().map(RunEvent::sequence),
            Some(RunEventSequence::new(65)?)
        );
        assert!(activity_run.earlier_events_omitted());
        Ok(())
    }

    fn anchors() -> Result<(GoalContract, TaskLedger, TaskStepId), Box<dyn Error>> {
        let goal = GoalContract::initial(
            TaskId::from_bytes([90; 32]),
            GoalContractDraft::new(
                GoalObjective::try_from_string("show durable agent activity".to_owned())?,
                vec![AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([91; 32]),
                    AcceptanceCriterionStatement::try_from_string(
                        "timeline stays grounded in the run journal".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string("run Agent activity tests".to_owned())?,
            )?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        let step_id = TaskStepId::from_bytes([92; 32]);
        let step = TaskStepDefinition::new(
            step_id,
            None,
            TaskStepOutcome::try_from_string("project the latest run".to_owned())?,
            TaskStepRationale::try_from_string(
                "the user needs current execution state".to_owned(),
            )?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "content-free journal metadata".to_owned(),
            )?],
            VerificationSpec::new(
                VerificationSpecId::from_bytes([93; 32]),
                VerificationMethod::Test,
                VerificationRequirement::try_from_string("run the use-case test".to_owned())?,
            ),
        )?;
        let ledger = TaskLedger::new(
            goal.reference(),
            vec![step],
            TaskLedgerTimestamp::from_unix_millis(1)?,
        )?;
        Ok((goal, ledger, step_id))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([2; 32]);
        Ok(ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                CanonicalDirectory::from_canonicalized(root.clone())?,
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([3; 32]),
                WorktreeAnchorId::from_bytes([4; 32]),
                repository_id,
                CanonicalDirectory::from_canonicalized(root)?,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main".to_owned())?,
            },
        )?)
    }
}

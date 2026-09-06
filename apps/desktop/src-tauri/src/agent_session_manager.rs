use crate::agent_conversation_runtime::{AgentConversationFailure, AgentConversationRuntime};
use crate::agent_run_manager::{AgentRunActivityState, AgentRunManager};
use crate::job_ids::DesktopJobIds;
use a3_application::{
    AdvanceAgentController, AgentControllerSignal, AgentRunExecutionRequest, AgentSessionDetail,
    AgentSessionListQuery, AgentSessionPage, AgentSessionQueue, AgentSessionStore,
    AgentSessionStoreFailure, AppendRunEvent, AskResearchAction, AskResearchEvent,
    AskResearchEvidenceStatus, AskResearchPublicFindingKind, AskResearchPublicNote,
    AskResearchRelation, AskResearchSource, AskResearchStore, AskResearchStoreFailure,
    AskResearchTurn, AskSourceSearchFailure, AskSourceSearcher, AskSourceTextSearch,
    BeginResearchDecision, BoundedResearchController, CompileTaskLens, CreateAgentRun,
    CreateGoalContract, CreateTaskLedger, DecodeAskResearchDecision, DecodeEvidenceDiagrams,
    EvidenceDiagramArtifact, GoalContractStore, JobCancellationError, JobCompletion, JobContext,
    JobSubmitter, KnowledgeIndexStore, KnowledgeSearchStore, ResearchHandoff,
    ResearchMemoryCheckpoint, ResearchMemoryFinding, ResearchMemoryFindingKind, RunJournalStore,
    SlashCommandExecutionProfile, TaskLedgerStore, TaskLensClaimStore, TaskLensControl,
    TaskLensControlError, TaskLensIndexStore, memory_finding_from_note,
    validate_agent_session_transition,
};
use a3_application::{
    AgentSourceReadFailure, AgentSourceReader, DiscoverProjectCommands, ModelMessageRole,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
    AgentDiagramArtifactId, AgentFileInspection, AgentFileLineCount, AgentFileStartLine,
    AgentQueuedMessage, AgentQueuedMessageId, AgentQueuedMessageState,
    AgentQueuedResearchSelection, AgentResearchDepth, AgentRun, AgentRunId, AgentRunTimestamp,
    AgentSession, AgentSessionEntry, AgentSessionEntryKind, AgentSessionId, AgentSessionMode,
    AgentSessionQueueRevision, AgentSessionRevision, AgentSessionSequence, AgentSessionState,
    AgentSessionText, AgentSessionTimestamp, AgentSessionTitle, AgentWorkItem, AgentWorkItemId,
    AgentWorkPlan, AgentWorkPlanVerificationIntent, AskResearchCompleteness, AskResearchPhase,
    AskResearchSelectionReason, AskResearchSourceId, AskResearchSourceKind, AskResearchState,
    DiscoveredCommandKind, ExpectedTaskEvidence, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, GraphEndpoint, JobId, JobOwner, ParsedSlashCommand,
    PolicyResourceId, Progress, ProjectIdentity, RunEventId, SecretCandidateClassifierV1,
    SlashCommand, SlashCommandEmptyInput, SlashCommandVerificationProfile, SourceChannel,
    StepDependency, SuccessVerification, SyntaxRelationKind, TaskId, TaskLedger,
    TaskLedgerTimestamp, TaskLensEntryReason, TaskLensSeedSet, TaskLensSeedText, TaskLensTarget,
    TaskLensTokenBudget, TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale,
    VerificationRequirement, VerificationScope, VerificationSpec, VerificationSpecId, WorktreeId,
    parse_slash_command,
};
use a3_workspace::{WorkspaceAgentSourceReader, WorkspaceAskSourceSearcher};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::error::Error;
use std::fmt;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SESSION_PAGE_LIMIT: u16 = 128;
const AGENT_CONVERSATION_JOB_OWNER: JobOwner = JobOwner::new(4);
const CONVERSATION_PROGRESS_TOTAL: u64 = 10;
const WORKING_CHANGES_BYTE_LIMIT: u64 = 2 * 1_024 * 1_024;
const WORKING_CHANGES_FILE_LIMIT: usize = 200;
const MAX_RESEARCH_READ_RETRIES: u8 = 4;
const MAX_INITIAL_TASK_LENS_SOURCES: usize = 12;
const MAX_REUSED_RESEARCH_SOURCES: usize = 8;
const MAX_ADAPTIVE_SEARCH_SOURCES: usize = 16;
const MAX_RESEARCH_MEMORY_FINDINGS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageResearchSelection {
    LegacyDepth(AgentResearchDepth),
    ExplicitDepth(AgentResearchDepth),
    Command,
}

/// Closed V4 acceptance result used by the protocol composition boundary.
pub(crate) enum AgentMessageSubmission {
    Started {
        detail: AgentSessionDetail,
        requires_plan_review: bool,
    },
    Queued {
        detail: AgentSessionDetail,
        queue: AgentSessionQueue,
    },
}

/// Core task persistence required to turn one reviewed plan into authoritative harness anchors.
#[derive(Clone)]
pub(crate) struct AgentTaskMaterializer {
    goals: Arc<dyn GoalContractStore>,
    ledgers: Arc<dyn TaskLedgerStore>,
    journal: Arc<dyn RunJournalStore>,
    index: Arc<dyn KnowledgeIndexStore>,
}

impl AgentTaskMaterializer {
    #[must_use]
    pub(crate) fn new(
        goals: Arc<dyn GoalContractStore>,
        ledgers: Arc<dyn TaskLedgerStore>,
        journal: Arc<dyn RunJournalStore>,
        index: Arc<dyn KnowledgeIndexStore>,
    ) -> Self {
        Self {
            goals,
            ledgers,
            journal,
            index,
        }
    }

    async fn materialize(
        &self,
        request: AgentTaskMaterialization<'_>,
    ) -> Result<MaterializedAgentTask, AgentSessionManagerFailure> {
        let AgentTaskMaterialization {
            project,
            objective,
            reviewed_plan,
            profile,
            research_handoff,
            verification_profile,
            control,
        } = request;
        let published = self
            .index
            .latest_published_index(project, control)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        if let Some(handoff) = research_handoff
            && !research_handoff_matches_index(handoff, &published)
        {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let task_id = TaskId::from_bytes(random_id()?);
        let base = now_millis()?;
        let work_plan = AgentWorkPlan::from_reviewed_markdown(reviewed_plan)
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        let criteria = work_plan
            .steps()
            .iter()
            .enumerate()
            .map(|(index, step)| {
                Ok(AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes(random_id()?),
                    AcceptanceCriterionStatement::try_from_string(format!(
                        "{}. {}",
                        index + 1,
                        step.outcome()
                    ))
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                ))
            })
            .collect::<Result<Vec<_>, AgentSessionManagerFailure>>()?;
        let criterion_ids = criteria
            .iter()
            .map(AcceptanceCriterion::id)
            .collect::<Vec<_>>();
        let goal = GoalContract::initial(
            task_id,
            GoalContractDraft::new(
                GoalObjective::try_from_string(bounded_text(objective, 8 * 1024))
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                criteria,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    "Aktuelle typisierte Verification-Evidence erfüllt das Muss-Kriterium."
                        .to_owned(),
                )
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
            GoalContractTimestamp::from_unix_millis(base)
                .map_err(|_| AgentSessionManagerFailure::Unavailable)?,
        );
        let catalog = DiscoverProjectCommands
            .execute(project.worktree().id(), &published)
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        let step_ids = (0..work_plan.steps().len())
            .map(|_| random_id().map(TaskStepId::from_bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let mut definitions = Vec::with_capacity(work_plan.steps().len());
        for (index, plan_step) in work_plan.steps().iter().enumerate() {
            let default_order = match plan_step.verification_intent() {
                AgentWorkPlanVerificationIntent::Change => [
                    DiscoveredCommandKind::Lint,
                    DiscoveredCommandKind::Build,
                    DiscoveredCommandKind::Test,
                ],
                AgentWorkPlanVerificationIntent::Test => [
                    DiscoveredCommandKind::Test,
                    DiscoveredCommandKind::Lint,
                    DiscoveredCommandKind::Build,
                ],
            };
            let preferred_verification = verification_profile
                .map(verification_command_order)
                .unwrap_or(&default_order);
            let verification_command = preferred_verification.iter().find_map(|kind| {
                catalog
                    .commands()
                    .iter()
                    .find(|command| command.kind() == *kind)
            });
            let spec_id = VerificationSpecId::from_bytes(random_id()?);
            let verification = match verification_command {
                Some(command) => VerificationSpec::command(
                    spec_id,
                    verification_requirement(
                        "Der deterministisch entdeckte Projektcheck besteht für diesen Änderungsschritt.",
                    )?,
                    command.id(),
                    VerificationScope::Workspace,
                ),
                None => VerificationSpec::user_confirm(
                    spec_id,
                    verification_requirement(
                        "Nutzer bestätigt diesen Änderungsschritt auf dem aktuellen Snapshot.",
                    )?,
                    PolicyResourceId::from_bytes(random_id()?),
                ),
            };
            let dependencies = index
                .checked_sub(1)
                .map(|previous| vec![StepDependency::new(step_ids[previous])])
                .unwrap_or_default();
            let definition = TaskStepDefinition::new(
                step_ids[index],
                None,
                TaskStepOutcome::try_from_string(bounded_text(plan_step.outcome(), 8 * 1024))
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                TaskStepRationale::try_from_string(plan_step.rationale().to_owned())
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                dependencies,
                vec![
                    ExpectedTaskEvidence::try_from_string(plan_step.expected_evidence().to_owned())
                        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                ],
                verification,
            )
            .and_then(|step| step.with_acceptance_criteria(vec![criterion_ids[index]]))
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
            definitions.push(definition);
        }
        let first_step_id = *step_ids
            .first()
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let run_id = AgentRunId::from_bytes(random_id()?);
        let mut ledger = TaskLedger::new(
            goal.reference(),
            definitions,
            TaskLedgerTimestamp::from_unix_millis(base.saturating_add(1))
                .map_err(|_| AgentSessionManagerFailure::Unavailable)?,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        ledger
            .start_step(
                first_step_id,
                run_id,
                TaskLedgerTimestamp::from_unix_millis(base.saturating_add(2))
                    .map_err(|_| AgentSessionManagerFailure::Unavailable)?,
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        CreateGoalContract::new(self.goals.as_ref())
            .execute(project, &goal)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        let ledger_version = CreateTaskLedger::new(self.ledgers.as_ref())
            .execute(project, &ledger)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .version();
        let (mut run, start_event) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            profile.reference(),
            published.run().snapshot_id(),
            RunEventId::from_bytes(random_id()?),
            agent_timestamp(base.saturating_add(3))?,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        CreateAgentRun::new(self.journal.as_ref())
            .execute(project, &run, &start_event)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        for (offset, signal) in [
            (0_u64, AgentControllerSignal::AnchorsAccepted),
            (1_u64, AgentControllerSignal::LocalizationComplete),
            (2_u64, AgentControllerSignal::PlanReady),
        ] {
            let expected = run.last_event_sequence();
            let observed = agent_timestamp(base.saturating_add(4).saturating_add(offset))?;
            let advance = AdvanceAgentController
                .execute(
                    &mut run,
                    signal,
                    RunEventId::from_bytes(random_id()?),
                    published.run().snapshot_id(),
                    observed,
                    false,
                )
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
            AppendRunEvent::new(self.journal.as_ref())
                .execute(project, expected, &run, advance.event())
                .await
                .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        }
        Ok(MaterializedAgentTask {
            work_item: AgentWorkItem::new(
                AgentWorkItemId::from_bytes(random_id()?),
                task_id,
                AgentSessionMode::Agent,
            ),
            request: AgentRunExecutionRequest::new(task_id, ledger.revision(), ledger_version),
        })
    }

    async fn adopt_interrupted(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        profile: a3_domain::ModelProfileReference,
        research_handoff: &ResearchHandoff,
        control: &dyn a3_application::IndexPersistenceControl,
    ) -> Result<MaterializedAgentTask, AgentSessionManagerFailure> {
        let published = self
            .index
            .latest_published_index(project, control)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        if !research_handoff_matches_index(research_handoff, &published) {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let goal = self
            .goals
            .load_current_goal_contract(project, task_id)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        let stored = self
            .ledgers
            .load_task_ledger(project, task_id)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        let ledger = stored.ledger();
        if ledger.goal_contract() != goal.reference() {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let mut active_steps = ledger
            .steps()
            .filter(|step| step.status() == a3_domain::TaskStepStatus::InProgress);
        let active_step = active_steps
            .next()
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        if active_steps.next().is_some() {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let run_id = active_step
            .attempts()
            .last()
            .map(a3_domain::TaskStepAttempt::run_id)
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        let run = self
            .journal
            .load_agent_run(project, run_id)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        if run.goal_contract() != goal.reference()
            || run.task_ledger_revision() != ledger.revision()
            || run.current_snapshot_id() != published.run().snapshot_id()
            || run.model_profile() != Some(profile)
            || run.state() != a3_domain::AgentControllerState::Execute
        {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        Ok(MaterializedAgentTask {
            work_item: AgentWorkItem::new(
                AgentWorkItemId::from_bytes(random_id()?),
                task_id,
                AgentSessionMode::Agent,
            ),
            request: AgentRunExecutionRequest::new(task_id, ledger.revision(), stored.version()),
        })
    }
}

struct AgentTaskMaterialization<'a> {
    project: &'a ProjectIdentity,
    objective: &'a str,
    reviewed_plan: &'a str,
    profile: a3_domain::ModelProfile,
    research_handoff: Option<&'a ResearchHandoff>,
    verification_profile: Option<SlashCommandVerificationProfile>,
    control: &'a dyn a3_application::IndexPersistenceControl,
}

impl fmt::Debug for AgentTaskMaterializer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentTaskMaterializer")
            .finish_non_exhaustive()
    }
}

struct MaterializedAgentTask {
    work_item: AgentWorkItem,
    request: AgentRunExecutionRequest,
}

/// Bounded evidence retrieval used only by Ask before the reporting model is invoked.
#[derive(Clone)]
pub(crate) struct AgentAskResearcher {
    flows: Option<a3_application::ExploreFunctionFlows>,
    index: Arc<dyn TaskLensIndexStore>,
    search: Arc<dyn KnowledgeSearchStore>,
    claims: Arc<dyn TaskLensClaimStore>,
    trace: Arc<dyn AskResearchStore>,
    source_searcher: Arc<dyn AskSourceSearcher>,
    continuation_from: Option<AgentSessionSequence>,
}

impl AgentAskResearcher {
    pub(crate) fn with_function_flows(
        mut self,
        flows: Option<a3_application::ExploreFunctionFlows>,
    ) -> Self {
        self.flows = flows;
        self
    }
    #[must_use]
    pub(crate) fn new(
        index: Arc<dyn TaskLensIndexStore>,
        search: Arc<dyn KnowledgeSearchStore>,
        claims: Arc<dyn TaskLensClaimStore>,
        trace: Arc<dyn AskResearchStore>,
    ) -> Self {
        Self {
            index,
            flows: None,
            search,
            claims,
            trace,
            source_searcher: Arc::new(WorkspaceAskSourceSearcher),
            continuation_from: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn research(
        &self,
        runtime: &impl ResearchModel,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        mode: AgentSessionMode,
        depth: AgentResearchDepth,
        query: &str,
        transcript: &[(ModelMessageRole, String)],
        command_profile: Option<&SlashCommandExecutionProfile>,
        control: &JobContext,
    ) -> Result<AskResearchResult, AgentSessionManagerFailure> {
        let lens_control = ConversationTaskLensControl { context: control };
        let published = self
            .index
            .load_current_index(project, &lens_control)
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let turn = AskResearchTurn::new_for_mode(
            session_id,
            user_sequence,
            published.run().id(),
            published.run().snapshot_id(),
            timestamp()?,
            mode,
            depth,
        );
        let initial_terms = task_lens_search_terms(query);
        let first = research_event(
            session_id,
            user_sequence,
            1,
            AskResearchPhase::Preparing,
            AskResearchState::Running,
            "Aktuellen Projektstand und explizite Verweise auflösen",
            (!initial_terms.is_empty()).then_some(initial_terms.as_str()),
            AskResearchCompleteness::NotApplicable,
        )?;
        self.trace.begin_turn(project, &turn, &first).await?;
        let result = self
            .research_after_begin(
                runtime,
                project,
                &published,
                &turn,
                query,
                transcript,
                command_profile,
                control,
            )
            .await;
        if result.is_err() {
            let detail = self
                .trace
                .load_detail(project, session_id, user_sequence)
                .await?;
            if let Some(detail) = detail
                && detail
                    .events()
                    .last()
                    .is_some_and(|event| event.state() == AskResearchState::Running)
            {
                let sequence = detail
                    .events()
                    .last()
                    .map_or(2, |event| event.sequence().saturating_add(1));
                let state = if control.cancellation_token().is_cancelled() {
                    AskResearchState::Cancelled
                } else {
                    AskResearchState::Failed
                };
                let action = if state == AskResearchState::Cancelled {
                    "Recherche abgebrochen; bereits gefundene Quellen bleiben sichtbar"
                } else {
                    "Recherche konnte nicht abgeschlossen werden; bereits gefundene Quellen bleiben sichtbar"
                };
                if let Ok(event) = research_event(
                    session_id,
                    user_sequence,
                    sequence,
                    AskResearchPhase::Answering,
                    state,
                    action,
                    None,
                    AskResearchCompleteness::Limited,
                ) {
                    let _ignored = self.trace.append_event(project, &event).await;
                }
            }
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn research_after_begin(
        &self,
        runtime: &impl ResearchModel,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        turn: &AskResearchTurn,
        query: &str,
        transcript: &[(ModelMessageRole, String)],
        command_profile: Option<&SlashCommandExecutionProfile>,
        control: &JobContext,
    ) -> Result<AskResearchResult, AgentSessionManagerFailure> {
        let started = Instant::now();
        let evidence_budget = runtime
            .research_evidence_budget(
                turn.mode(),
                command_profile
                    .map(|profile| profile.research_constraint(turn.mode()))
                    .as_deref(),
            )
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        let mut state = AskResearchWorkingSet::new(evidence_budget);
        if query.len().saturating_add(256) > evidence_budget {
            return awaiting_continuation(
                turn,
                &state,
                command_profile,
                ResearchStopReason::ContextLimit,
            );
        }
        let query_targets = resolve_query_targets(published, query);
        let referenced_revisions = resolved_target_revisions(&query_targets);
        state.work_required_revisions = referenced_revisions.clone();
        let lens_terms = task_lens_search_terms(query);
        state.event_sequence = state.event_sequence.saturating_add(1);
        self.append_running_event(
            project,
            turn,
            state.event_sequence,
            AskResearchPhase::SelectingEvidence,
            "Task Lens wählt relevante Dateien, Symbole, Beziehungen und verifiziertes Modulwissen",
            (!lens_terms.is_empty()).then_some(lens_terms.as_str()),
            AskResearchCompleteness::NotApplicable,
        )
        .await?;

        for revision in &referenced_revisions {
            self.add_and_read_prioritized_source(
                project,
                turn,
                &mut state,
                revision.clone(),
                None,
                None,
                AskResearchSourceKind::File,
                AskResearchSelectionReason::ExactNameOrPath,
                control,
            )
            .await?;
        }
        if !query_targets.is_empty() {
            state.event_sequence = state.event_sequence.saturating_add(1);
            let resolved = query_targets
                .iter()
                .filter(|target| target.revision.is_some())
                .count();
            let read = referenced_revisions
                .iter()
                .filter(|revision| state.covers(std::slice::from_ref(revision)))
                .count();
            self.append_running_event(
                project,
                turn,
                state.event_sequence,
                AskResearchPhase::InspectingSource,
                &format!(
                    "{resolved} von {} ausdrücklich genannten Repositoryzielen gegen den aktuellen Index aufgelöst; {read} sicher und vorrangig gelesen",
                    query_targets.len()
                ),
                None,
                if resolved == query_targets.len() && read == resolved {
                    AskResearchCompleteness::Complete
                } else {
                    AskResearchCompleteness::Limited
                },
            )
            .await?;
        }

        if self.continuation_from.is_some() {
            self.reuse_previous_sources(project, published, turn, query, &mut state, control)
                .await?;
        }

        // A continuation already has a specific frontier. Only localize from scratch when none
        // of its sources could be revalidated against the newly pinned index.
        if self.continuation_from.is_none() || state.sources.is_empty() {
            match self.compile_lens(project, published, query, control).await {
                Ok(trace) => {
                    if trace.lens().index_run_id() != turn.index_run_id()
                        || trace.lens().snapshot_id() != turn.snapshot_id()
                    {
                        return Err(AgentSessionManagerFailure::Conflict);
                    }
                    self.add_lens_sources(project, turn, &mut state, trace.lens(), control)
                        .await?;
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    let channel_summary = trace
                        .channels()
                        .iter()
                        .map(|channel| {
                            format!(
                                "{}: {} gefunden, {} ausgewählt{}",
                                source_channel_label(channel.channel()),
                                channel.candidates(),
                                channel.selected(),
                                if channel.truncated() {
                                    ", begrenzt"
                                } else {
                                    ""
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" · ");
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::SelectingEvidence,
                        &format!("Task-Lens-Auswahl abgeschlossen – {channel_summary}"),
                        None,
                        if trace.lens().truncated() {
                            AskResearchCompleteness::Limited
                        } else {
                            AskResearchCompleteness::Complete
                        },
                    )
                    .await?;
                }
                Err(error) => {
                    if control.cancellation_token().is_cancelled() {
                        return Err(error);
                    }
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                    project,
                    turn,
                    state.event_sequence,
                    AskResearchPhase::SelectingEvidence,
                    "Task Lens war vorübergehend nicht verfügbar; direkte Such- und Leseschritte bleiben aktiv",
                    None,
                    AskResearchCompleteness::Limited,
                )
                .await?;
                }
            }
        }

        // Historical context is intentionally last. Current named targets and the current Task
        // Lens must never be crowded out by a fresh but unrelated previous conversation turn.
        if self.continuation_from.is_none() && query_targets.is_empty() {
            self.reuse_previous_sources(project, published, turn, query, &mut state, control)
                .await?;
        }

        let literals = explicit_repository_literals(query);
        if !literals.is_empty() {
            self.search_source(project, published, turn, &mut state, literals, control)
                .await?;
        }

        if runtime.requires_work_contract()
            && turn.mode() != AgentSessionMode::Ask
            && state.work.is_none()
        {
            state
                .initialize_plan_work(query)
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
            self.append_work_checkpoint(project, turn, &mut state)
                .await?;
        }
        let conversation = bounded_conversation(transcript);
        let mut model_transcript = Vec::new();
        let mut feedback = state.continuation_feedback.clone();
        let mut controller = BoundedResearchController::new(turn.depth());
        if let Some(profile) = command_profile {
            let initial_actions = profile.initial_read_actions();
            if !initial_actions.is_empty() {
                let batch = controller
                    .prepare_actions(initial_actions)
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
                let before = state.evidence_revision;
                let action_results = self
                    .execute_actions(
                        project,
                        published,
                        turn,
                        &mut state,
                        batch.actions().to_vec(),
                        control,
                    )
                    .await?;
                controller.finish_round(before, state.evidence_revision);
                feedback = format!(
                    "CORE-REQUIRED READ-ONLY COMMAND BASELINE:\n{action_results}\n\nUse this evidence baseline and choose only necessary bounded follow-up reads."
                );
            }
        }
        let diagrams_requested = command_profile
            .is_some_and(|profile| profile.invocation().primary() == SlashCommand::Diagram);
        let mut citation_repair_pending = false;
        state.focus_hint(published, query);
        let (markdown, ordinals) = loop {
            if control.cancellation_token().is_cancelled() {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            // Core-only cache/frontier transitions must obey the owner deadline too.
            if elapsed_millis(started) >= controller.limits().duration_millis() {
                return awaiting_continuation(
                    turn,
                    &state,
                    command_profile,
                    ResearchStopReason::TimeLimit,
                );
            }
            let mut compiled_work_packet = None;
            if turn.mode() != AgentSessionMode::Ask
                && let Some(plan) = state.core_plan_answer()
            {
                let current = self
                    .index
                    .load_current_index(project, &ConversationTaskLensControl { context: control })
                    .await
                    .map_err(|_| AgentSessionManagerFailure::Unavailable)?
                    .ok_or(AgentSessionManagerFailure::Unavailable)?;
                if research_access::scope(&current) != research_access::scope(published) {
                    return Err(AgentSessionManagerFailure::IndexChanged);
                }
                state.evidence_guard(project).validate(control).await?;
                break plan;
            }
            let mut analysis_receipt = None;
            if state.work.is_some() {
                state.focus_work_cache(published);
                if state
                    .work_context()
                    .len()
                    .saturating_add(query.len())
                    .saturating_add(state.work_packet_reserve())
                    > state.evidence_limit
                {
                    return awaiting_continuation(
                        turn,
                        &state,
                        command_profile,
                        ResearchStopReason::ContextLimit,
                    );
                }
                let packet = state.model_evidence(query, &query_targets);
                compiled_work_packet = Some(packet);
                let key = state.work_packet_key();
                let next = state
                    .work
                    .as_ref()
                    .and_then(a3_domain::ResearchWorkState::next_question);
                if let Some(id) = next {
                    let repeated = state
                        .work
                        .as_ref()
                        .and_then(|w| w.question(id))
                        .is_some_and(|q| q.attempts().contains(&key));
                    if repeated {
                        // No model call is spent on the same unanswered question and packet.
                        // Try a different safe frontier under the unchanged outer budget.
                        if let Some(note) = state.work_followup_note() {
                            state.focus_hint(published, &note.gap);
                            let _packet = state.model_evidence(query, &query_targets);
                            if state.work_packet_key() != key {
                                continue;
                            }
                            let candidates =
                                research_followup::candidates(published, &state, &note);
                            if let Some(batch) = controller
                                .prepare_work_frontier(candidates, elapsed_millis(started))
                                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?
                            {
                                let before = state.evidence_revision;
                                feedback = self
                                    .execute_actions(
                                        project,
                                        published,
                                        turn,
                                        &mut state,
                                        batch.actions().to_vec(),
                                        control,
                                    )
                                    .await?;
                                controller.finish_round(before, state.evidence_revision);
                                continue;
                            }
                        }
                        if controller.actions_used() < controller.limits().read_actions()
                            && controller.decisions_used() < controller.limits().model_decisions()
                            && elapsed_millis(started) < controller.limits().duration_millis()
                            && !control.cancellation_token().is_cancelled()
                            && state
                                .close_investigated_boundary(research_access::scope(published))?
                        {
                            let current = self
                                .index
                                .load_current_index(
                                    project,
                                    &ConversationTaskLensControl { context: control },
                                )
                                .await
                                .map_err(|_| AgentSessionManagerFailure::Unavailable)?
                                .ok_or(AgentSessionManagerFailure::Unavailable)?;
                            if research_access::scope(&current) != research_access::scope(published)
                            {
                                return Err(AgentSessionManagerFailure::IndexChanged);
                            }
                            state.evidence_guard(project).validate(control).await?;
                            self.append_work_checkpoint(project, turn, &mut state)
                                .await?;
                            if turn.mode() == AgentSessionMode::Ask
                                && state
                                    .work
                                    .as_ref()
                                    .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
                            {
                                break state
                                    .work_answer()
                                    .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
                            }
                            continue;
                        }
                        if let Some(work) = &mut state.work {
                            work.block(id)
                                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
                        }
                        self.append_work_checkpoint(project, turn, &mut state)
                            .await?;
                        if let Some(partial) = state.work_answer() {
                            state.partial_answer = Some(partial);
                        }
                        feedback = format!(
                            "CORE: Q{} cannot advance with the available packet and remaining safe frontier. It remains unanswered. Continue independent required questions only.",
                            id.get()
                        );
                        continue;
                    }
                    // Deciding events account for starts. Only a valid admitted document may
                    // acknowledge this packet as analyzed; failed/cancelled calls cannot poison
                    // an explicit continuation with a fictitious same-packet result.
                    analysis_receipt = Some((id, key));
                } else if state.work.as_ref().is_some_and(|w| !w.ready_to_finish()) {
                    return awaiting_continuation(
                        turn,
                        &state,
                        command_profile,
                        ResearchStopReason::WorkBlocked,
                    );
                }
            }
            if state.work.is_none()
                && controller.is_stagnant()
                && self
                    .recover_research(
                        project,
                        published,
                        turn,
                        &mut state,
                        &mut controller,
                        started,
                        query,
                        &query_targets,
                        control,
                    )
                    .await?
            {
                feedback = "CORE RECOVERY: A new current evidence frontier was selected within the unchanged remaining budgets. Evaluate the current source window.".to_owned();
            }
            let permission = match if state.work.is_some() {
                controller.begin_work_decision(elapsed_millis(started))
            } else {
                controller.begin_decision(elapsed_millis(started))
            } {
                Ok(permission) => {
                    if citation_repair_pending
                        || state
                            .work
                            .as_ref()
                            .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
                    {
                        BeginResearchDecision::FinalOnly
                    } else {
                        permission
                    }
                }
                Err(error) => {
                    return awaiting_continuation(
                        turn,
                        &state,
                        command_profile,
                        ResearchStopReason::for_controller(error),
                    );
                }
            };
            state.event_sequence = state.event_sequence.saturating_add(1);
            self.append_running_event(
                project,
                turn,
                state.event_sequence,
                AskResearchPhase::Deciding,
                if permission == BeginResearchDecision::SearchAllowed {
                    "Evidence-Lücke prüfen und den nächsten begrenzten Schritt wählen"
                } else {
                    "Verfügbaren Evidence-Stand abschließend beantworten oder als offen kennzeichnen"
                },
                None,
                AskResearchCompleteness::NotApplicable,
            )
            .await?;
            model_transcript = research_decision_context(
                &conversation,
                &state.context_feedback(&feedback),
                compiled_work_packet.unwrap_or_else(|| state.model_evidence(query, &query_targets)),
            );
            let mut guard = state.evidence_guard(project);
            if runtime.requires_work_contract() || state.work.is_some() {
                guard.work = Some(research_work::WorkGuard::new(query, &state));
            }
            state.commit_delivery();
            let model_result = ask_decision(
                runtime,
                turn.mode(),
                permission,
                &mut model_transcript,
                control,
                &mut controller,
                started,
                state.sources.len(),
                command_profile,
                &guard,
                citation_repair_pending,
            )
            .await;
            if matches!(model_result, Err(AgentSessionManagerFailure::IndexChanged)) {
                if let Some(work) = &mut state.work {
                    work.revalidate(&[])
                        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
                }
                self.append_work_checkpoint(project, turn, &mut state)
                    .await?;
            }
            let (decision, diagnostics) = model_result?;
            for diagnostic in diagnostics {
                state.event_sequence = state.event_sequence.saturating_add(1);
                self.append_running_event(
                    project,
                    turn,
                    state.event_sequence,
                    AskResearchPhase::Deciding,
                    &diagnostic,
                    None,
                    AskResearchCompleteness::NotApplicable,
                )
                .await?;
            }
            let (mut decision, permission, document_repaired) = match decision {
                Ok(decision) => decision,
                Err(ResearchStopReason::InvalidDecision)
                    if !runtime.requires_work_contract()
                        && state.work.is_none()
                        && self
                            .recover_research(
                                project,
                                published,
                                turn,
                                &mut state,
                                &mut controller,
                                started,
                                query,
                                &query_targets,
                                control,
                            )
                            .await? =>
                {
                    feedback = "CORE RECOVERY: The invalid document and its failed single repair were discarded. Only previously validated goals and cached index anchors selected this new current evidence window. Choose a new decision; no invalid actions were executed.".to_owned();
                    continue;
                }
                Err(reason) => return awaiting_continuation(turn, &state, command_profile, reason),
            };
            let initializing_work = runtime.requires_work_contract() && state.work.is_none();
            let resolved_before = state
                .work
                .as_ref()
                .map_or(0, a3_domain::ResearchWorkState::resolved_count);
            let formatting = state
                .work
                .as_ref()
                .is_some_and(a3_domain::ResearchWorkState::ready_to_finish);
            state.accept_work(query, &decision, analysis_receipt)?;
            self.append_work_checkpoint(project, turn, &mut state)
                .await?;
            if initializing_work {
                feedback = "CORE: The question contract is frozen. Evaluate the cached original evidence for ACTIVE Q before any further discovery.".to_owned();
                continue;
            }
            if state
                .work
                .as_ref()
                .is_some_and(|w| w.resolved_count() > resolved_before)
            {
                controller.finish_round(0, 1);
                if state.work.as_ref().is_some_and(|w| !w.ready_to_finish()) {
                    feedback = "CORE: The previous question has an admitted result. Select the next active question from cached current evidence before new discovery.".to_owned();
                    continue;
                }
            }
            if state
                .work
                .as_ref()
                .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
                && matches!(
                    decision,
                    a3_application::AskResearchDecision::Research { .. }
                )
            {
                if turn.mode() == AgentSessionMode::Ask {
                    if let Some((markdown, source_ordinals)) = state.work_answer()
                        && let a3_application::AskResearchDecision::Research { note, .. } = decision
                    {
                        decision = a3_application::AskResearchDecision::Answer {
                            markdown,
                            source_ordinals,
                            note,
                            evidence_status: AskResearchEvidenceStatus::Sufficient,
                        };
                    }
                } else {
                    feedback = "CORE: Required research results are complete. Return the requested PLAN using these results; do not open optional research.".to_owned();
                    continue;
                }
            }
            if turn.mode() != AgentSessionMode::Ask
                && !formatting
                && state
                    .work
                    .as_ref()
                    .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
            {
                feedback = "CORE: All required results are admitted. Format the requested PLAN from these results without additional research.".to_owned();
                continue;
            }
            match decision {
                a3_application::AskResearchDecision::Answer {
                    mut markdown,
                    mut source_ordinals,
                    mut note,
                    evidence_status,
                } => {
                    if let Some(work) = &state.work {
                        if work.ready_to_finish() {
                            if let Some((answer, references)) = state.work_answer() {
                                if turn.mode() == AgentSessionMode::Ask {
                                    markdown = answer;
                                    source_ordinals = references;
                                } else {
                                    markdown.push_str("\n\n## Recherchegrundlage\n\n");
                                    markdown.push_str(&answer);
                                    for ordinal in references {
                                        if !source_ordinals.contains(&ordinal) {
                                            source_ordinals.push(ordinal);
                                        }
                                    }
                                }
                            }
                        } else if let Some(question) =
                            work.next_question().and_then(|id| work.question(id))
                        {
                            note.gap = utf8_prefix(
                                &format!(
                                    "Q{}: {}. Missing evidence: {}",
                                    question.id().get(),
                                    question.definition().outcome,
                                    note.gap
                                ),
                                1024,
                            )
                            .to_owned();
                        }
                    }
                    let mut cited_guard = state.evidence_guard(project);
                    for ordinal in source_ordinals.iter().chain(&note.source_ordinals) {
                        if let Some(source) =
                            state.sources.get(usize::from(ordinal.saturating_sub(1)))
                            && !cited_guard
                                .revisions
                                .iter()
                                .any(|(revision, _)| revision == source.revision())
                        {
                            cited_guard.revisions.push((
                                source.revision().clone(),
                                source.range().map_or(1, |range| {
                                    range.start_position().row().saturating_add(1)
                                }),
                            ));
                        }
                    }
                    cited_guard.validate(control).await?;
                    state.record_note(query, &note)?;
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    let public_note = public_note(&note, &state)?;
                    if state.work.is_none()
                        && controller.is_stagnant()
                        && answer_requires_deeper_research(
                            evidence_status,
                            state.covers(&referenced_revisions),
                        )
                    {
                        // The single Core recovery was already attempted before this decision.
                        state.partial_answer = Some((markdown, source_ordinals));
                        self.append_note_event(
                            project, turn, state.event_sequence, AskResearchPhase::Evaluating,
                            "Nach begrenzter Recovery bleibt die Evidence-Frontier ohne Fortschritt",
                            public_note,
                        ).await?;
                        return awaiting_continuation(
                            turn,
                            &state,
                            command_profile,
                            ResearchStopReason::Stagnation,
                        );
                    }
                    let missing_named_sources = !state.covers(&referenced_revisions);
                    if answer_requires_deeper_research(evidence_status, !missing_named_sources) {
                        state.partial_answer = Some((markdown.clone(), source_ordinals.clone()));
                        self.append_note_event(
                            project,
                            turn,
                            state.event_sequence,
                            AskResearchPhase::Evaluating,
                            if missing_named_sources {
                                "Benannte Dateien sind noch nicht vollständig belegt; Recherche wird vertieft"
                            } else {
                                "Gemeldete Evidence-Lücke bleibt offen; Recherche wird vertieft"
                            },
                            public_note,
                        )
                        .await?;
                        if permission == BeginResearchDecision::FinalOnly {
                            return awaiting_continuation(
                                turn,
                                &state,
                                command_profile,
                                ResearchStopReason::EvidenceIncomplete,
                            );
                        }
                        let delivery_before = state
                            .evidence_revision
                            .saturating_add(state.delivery_revision);
                        if state.focus_hint(published, &format!("{} {}", note.gap, note.next_step))
                        {
                            let _packet = state.model_evidence(query, &query_targets);
                            if state.progress_with_pending() > delivery_before {
                                controller
                                    .finish_round(delivery_before, state.progress_with_pending());
                                feedback = "CORE EVIDENCE FOLLOW-UP: The relevant indexed function is already safely cached. Its contiguous current source is now focused without another adaptive read.".to_owned();
                                continue;
                            }
                        }
                        // A validated gap already describes useful next evidence. Try the Core's
                        // existing safe reads directly instead of spending a decision asking the
                        // model to restate its next step as an action. No budget is reset.
                        let followups = research_followup::candidates(published, &state, &note);
                        let batch = match if state.work.is_some() {
                            controller.prepare_work_frontier(followups, elapsed_millis(started))
                        } else {
                            controller.prepare_followup_actions(followups, elapsed_millis(started))
                        } {
                            Ok(batch) => batch,
                            Err(error) => {
                                return awaiting_continuation(
                                    turn,
                                    &state,
                                    command_profile,
                                    ResearchStopReason::for_controller(error),
                                );
                            }
                        };
                        if let Some(batch) = batch {
                            let before = delivery_before;
                            let results = self
                                .execute_actions(
                                    project,
                                    published,
                                    turn,
                                    &mut state,
                                    batch.actions().to_vec(),
                                    control,
                                )
                                .await?;
                            let _packet = state.model_evidence(query, &query_targets);
                            controller.finish_round(before, state.progress_with_pending());
                            feedback = format!(
                                "CORE EVIDENCE FOLLOW-UP: The incomplete answer was not published. The Core followed its concrete gap using budgeted read-only actions, without user confirmation.\n{results}\nEvaluate the current excerpts. If the actual question is now supported, answer; otherwise choose a different precise read. These search candidates are not proof by themselves."
                            );
                            continue;
                        }
                        let _packet = state.model_evidence(query, &query_targets);
                        controller.finish_round(delivery_before, state.progress_with_pending());
                        feedback = if state.work.is_some() {
                            "CORE: The active question is still unresolved. Evaluate the delivered original evidence; return work.results only for supported answers. The Core, not model tool requests, controls the next read.".to_owned()
                        } else {
                            format!(
                                "CORE EVIDENCE GATE: The proposed answer is not final because material evidence is still missing{}. Return kind research now. Inspect named indexed files directly, continue large files with inspectPath start_line, search concrete symbols or literals, and follow relevant relations. Do not ask the user to provide files already present in the pinned index.",
                                if missing_named_sources {
                                    " and at least one explicitly named indexed file has not been read"
                                } else {
                                    ""
                                }
                            )
                        };
                        continue;
                    }
                    if response_requires_citations(turn.mode(), &markdown)
                        && !state.citations_cover(&source_ordinals, &referenced_revisions)
                    {
                        // The files are present. Missing answer attribution is an output defect,
                        // not permission to waste more read rounds on the same evidence.
                        state.partial_answer = Some((markdown, source_ordinals));
                        self.append_note_event(project, turn, state.event_sequence,
                            AskResearchPhase::AnsweringOrPlanning,
                            "Vorhandene Belege werden der Antwort zugeordnet; keine erneute Dateisuche nötig",
                            public_note).await?;
                        if document_repaired || controller.use_repair().is_err() {
                            return awaiting_continuation(
                                turn,
                                &state,
                                command_profile,
                                ResearchStopReason::CitationRepair,
                            );
                        }
                        citation_repair_pending = true;
                        feedback = "CORE CITATION REPAIR: All explicitly named indexed files have already been safely read. Evaluate the current excerpts. Return kind answer with justified citations for the named targets; markdown markers and source_refs must match. This is an attribution correction, not a new research round. If the evidence really cannot support the question, return an honest incomplete intermediate result. Do not invent support or request new reads.".to_owned();
                        continue;
                    }
                    self.append_note_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::AnsweringOrPlanning,
                        "Belegten Stand als Antwort, Rückfrage oder Plan formulieren",
                        public_note,
                    )
                    .await?;
                    break (markdown, source_ordinals);
                }
                a3_application::AskResearchDecision::Research { mut note, actions } => {
                    if let Some(hint) = state.active_work_hint() {
                        note.gap = hint;
                        note.next_step =
                            "Aktive Pflichtfrage anhand aktueller Originalquellen prüfen."
                                .to_owned();
                    }
                    if permission == BeginResearchDecision::FinalOnly {
                        return Err(AgentSessionManagerFailure::InvalidOutput);
                    }
                    // Refocus even deduplicated reads: the evidence may have left the small model
                    // window, but rereading the same bytes is not new research progress.
                    let before = state
                        .evidence_revision
                        .saturating_add(state.delivery_revision);
                    let new_actions = state.focus_actions(&actions);
                    let batch = match if new_actions.is_empty() {
                        Ok(None)
                    } else {
                        controller.prepare_actions(new_actions).map(Some)
                    } {
                        Ok(batch) => batch,
                        Err(error) => {
                            return awaiting_continuation(
                                turn,
                                &state,
                                command_profile,
                                ResearchStopReason::for_controller(error),
                            );
                        }
                    };
                    state.record_note(query, &note)?;
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    let public_note = public_note(&note, &state)?;
                    self.append_note_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::Evaluating,
                        if batch.as_ref().is_none_or(|batch| batch.duplicate_count() > 0) {
                            "Zwischenbefund auswerten; bereits geprüfte Aktionen werden nicht wiederholt"
                        } else {
                            "Zwischenbefund auswerten und nächste Evidence-Lücke festhalten"
                        },
                        public_note,
                    )
                    .await?;
                    let mut action_results = self
                        .execute_actions(
                            project,
                            published,
                            turn,
                            &mut state,
                            batch
                                .as_ref()
                                .map_or_else(Vec::new, |batch| batch.actions().to_vec()),
                            control,
                        )
                        .await?;
                    let duplicate_count = batch
                        .as_ref()
                        .map_or(actions.len(), |batch| usize::from(batch.duplicate_count()));
                    if duplicate_count > 0 {
                        action_results.push_str(&format!(
                            "\n{} identische Aktion(en) waren bereits geprüft und wurden nicht erneut ausgeführt.",
                            duplicate_count
                        ));
                    }
                    // Restore the complete explicit batch after reads. Gap hints must not
                    // overwrite an already selected interface with another single-file view.
                    state.focus_actions(&actions);
                    if state.focus.is_empty() {
                        state.focus_hint(published, &format!("{} {}", note.gap, note.next_step));
                    } else {
                        state.refine_action_focus(
                            published,
                            &format!("{} {}", note.gap, note.next_step),
                        );
                    }
                    let _packet = state.model_evidence(query, &query_targets);
                    let after = state.progress_with_pending();
                    let produced_evidence = after > before;
                    controller.finish_round(before, after);
                    feedback = format!(
                        "READ-ONLY ACTION RESULTS:\n{action_results}\n\n{}",
                        if state.work.is_none() && controller.is_stagnant() {
                            "CORE SYNTHESIS: No further reads are permitted after two rounds without new evidence. Evaluate the current excerpts now. Earlier public claims that a file was missing are historical observations, not requirements to reread it. Return the supported answer, or a concrete intermediate result and the genuinely unresolved question."
                        } else if produced_evidence {
                            "Continue from the current evidence. Follow the remaining gap with the most direct next read."
                        } else {
                            "CORE SEARCH RECOVERY: This round produced no new source evidence. Do not repeat a broad search variant. On the next decision, change access path: inspect a resolved named path, inspect a known S source, follow a relation, or list the narrow containing directory."
                        }
                    );
                }
            }
        };
        let citations = ordinals
            .into_iter()
            .map(|ordinal| {
                state
                    .sources
                    .get(usize::from(ordinal.saturating_sub(1)))
                    .map(AskResearchSource::id)
                    .ok_or(AgentSessionManagerFailure::InvalidOutput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bounded_only = turn.mode() == AgentSessionMode::Ask
            && state.work.as_ref().is_some_and(|work| {
                work.ready_to_finish()
                    && work
                        .questions()
                        .iter()
                        .filter(|q| {
                            q.definition().priority == a3_domain::ResearchQuestionPriority::Required
                        })
                        .all(|q| {
                            q.result().is_some_and(|r| {
                                r.kind() == a3_domain::ResearchResultKind::BoundedUnknown
                            })
                        })
            });
        if citations.is_empty()
            && response_requires_citations(turn.mode(), &markdown)
            && !bounded_only
        {
            return awaiting_continuation(
                turn,
                &state,
                command_profile,
                ResearchStopReason::CitationRepair,
            );
        }
        if diagrams_requested {
            model_transcript =
                research_decision_context(&[], "", state.model_evidence(query, &query_targets));
            model_transcript.insert(0, (ModelMessageRole::Assistant, format!("Validated text answer (format this result without reopening research):\n{}", utf8_prefix(&markdown, 4096))));
            state.event_sequence = state.event_sequence.saturating_add(1);
            self.append_running_event(
                project,
                turn,
                state.event_sequence,
                AskResearchPhase::AnsweringOrPlanning,
                "Belegte Elemente und Beziehungen in sichere Diagramme übersetzen",
                None,
                AskResearchCompleteness::NotApplicable,
            )
            .await?;
        }
        let diagram_outcome = compile_diagram_artifacts(
            runtime,
            command_profile,
            &mut model_transcript,
            &state,
            &mut controller,
            started,
            control,
            project,
        )
        .await?;
        for diagnostic in &diagram_outcome.diagnostics {
            state.event_sequence = state.event_sequence.saturating_add(1);
            self.append_running_event(
                project,
                turn,
                state.event_sequence,
                AskResearchPhase::AnsweringOrPlanning,
                diagnostic,
                None,
                AskResearchCompleteness::NotApplicable,
            )
            .await?;
        }
        let diagrams = diagram_outcome.artifacts;
        let diagram_incomplete = diagrams_requested && !diagram_outcome.complete;
        let limited_research = state.work.as_ref().is_some_and(|work| {
            work.questions()
                .iter()
                .any(|q| q.status() == a3_domain::ResearchQuestionStatus::Limited)
        });
        let terminal_event = research_event(
            turn.session_id(),
            turn.user_sequence(),
            state.event_sequence.saturating_add(1),
            AskResearchPhase::Completed,
            if diagram_incomplete {
                AskResearchState::AwaitingContinuation
            } else {
                AskResearchState::Completed
            },
            if diagram_incomplete {
                "Antwort und Quellen veröffentlicht; Diagrammerzeugung benötigt einen weiteren begrenzten Versuch"
            } else if limited_research {
                "Recherche mit ausdrücklich begrenzter Erkenntnis abgeschlossen"
            } else if diagrams.is_empty() {
                "Antwort und verwendete Quellen veröffentlicht"
            } else {
                "Antwort, Diagramme und verwendete Quellen veröffentlicht"
            },
            None,
            if diagram_incomplete || limited_research {
                AskResearchCompleteness::Limited
            } else {
                AskResearchCompleteness::NotApplicable
            },
        )?;
        Ok(AskResearchResult {
            markdown,
            citations,
            diagrams,
            terminal_event,
            awaiting_continuation: diagram_incomplete,
            handoff: research_handoff(turn, &state, command_profile)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn recover_research(
        &self,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        controller: &mut BoundedResearchController,
        started: Instant,
        query: &str,
        query_targets: &[ResolvedQueryTarget],
        control: &JobContext,
    ) -> Result<bool, AgentSessionManagerFailure> {
        if control.cancellation_token().is_cancelled() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        if controller.begin_recovery(elapsed_millis(started)).is_err() {
            return Ok(false);
        }
        let before = state
            .evidence_revision
            .saturating_add(state.delivery_revision);
        let hint = state.last_note.as_ref().map_or_else(
            || query.to_owned(),
            |note| format!("{} {}", note.gap, note.next_step),
        );
        if !state.focus_hint(published, &hint) {
            state.advance_cached_frontier();
        }
        let _packet = state.model_evidence(query, query_targets);
        if state.progress_with_pending() == before {
            let candidates = state.last_note.as_ref().map_or_else(Vec::new, |note| {
                research_followup::candidates(published, state, note)
            });
            let candidates = candidates
                .into_iter()
                .filter(|action| !state.focus_cached(action))
                .collect();
            if let Ok(Some(batch)) =
                controller.prepare_followup_actions(candidates, elapsed_millis(started))
            {
                self.execute_actions(
                    project,
                    published,
                    turn,
                    state,
                    batch.actions().to_vec(),
                    control,
                )
                .await?;
                let _packet = state.model_evidence(query, query_targets);
            }
        }
        let after = state.progress_with_pending();
        controller.finish_round(before, after);
        if after == before {
            // Preserve the terminal stagnation state if the only recovery found no new access.
            controller.finish_round(before, after);
        }
        state.event_sequence = state.event_sequence.saturating_add(1);
        self.append_running_event(
            project,
            turn,
            state.event_sequence,
            AskResearchPhase::SelectingEvidence,
            &research_model::diagnostic(
                if after > before {
                    "research-v1/recovery-progress"
                } else {
                    "research-v1/recovery-empty"
                },
                BeginResearchDecision::SearchAllowed,
                controller,
            ),
            None,
            AskResearchCompleteness::NotApplicable,
        )
        .await?;
        Ok(after > before)
    }

    async fn compile_lens(
        &self,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        query: &str,
        control: &JobContext,
    ) -> Result<a3_application::TaskLensCompilationTrace, AgentSessionManagerFailure> {
        let seeds = research_lens_seeds(query, &resolve_query_targets(published, query))?;
        let pinned_index = PinnedTaskLensIndex {
            published: Arc::clone(published),
        };
        let compiler =
            CompileTaskLens::new(&pinned_index, self.search.as_ref(), self.claims.as_ref());
        for attempt in 0..=1 {
            let result = compiler
                .execute_with_trace(
                    project,
                    seeds.clone(),
                    TaskLensTokenBudget::DEFAULT,
                    &ConversationTaskLensControl { context: control },
                )
                .await;
            match result {
                Ok(trace) => return Ok(trace),
                Err(_) if control.cancellation_token().is_cancelled() => {
                    return Err(AgentSessionManagerFailure::Unavailable);
                }
                Err(_) if attempt == 0 => {}
                Err(_) => return Err(AgentSessionManagerFailure::Unavailable),
            }
        }
        Err(AgentSessionManagerFailure::Unavailable)
    }

    async fn add_lens_sources(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        lens: &a3_domain::TaskLens,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        for entry in lens.entries().iter().take(MAX_INITIAL_TASK_LENS_SOURCES) {
            let reason = lens_selection_reason(entry.reason());
            let claim_kind = matches!(entry.reason(), TaskLensEntryReason::Claim(_));
            let candidate = match entry.target() {
                TaskLensTarget::Repository(_) => None,
                TaskLensTarget::Module(module) => module.manifests().first().map(|revision| {
                    (
                        revision.clone(),
                        None,
                        None,
                        if claim_kind {
                            AskResearchSourceKind::VerifiedClaim
                        } else {
                            AskResearchSourceKind::File
                        },
                    )
                }),
                TaskLensTarget::File(revision) => Some((
                    revision.clone(),
                    None,
                    None,
                    if claim_kind {
                        AskResearchSourceKind::VerifiedClaim
                    } else {
                        AskResearchSourceKind::File
                    },
                )),
                TaskLensTarget::Symbol(symbol) => Some((
                    symbol.revision().clone(),
                    Some(symbol.parsed().declaration_range()),
                    Some(symbol.parsed().name().as_str().to_owned()),
                    if claim_kind {
                        AskResearchSourceKind::VerifiedClaim
                    } else {
                        AskResearchSourceKind::Symbol
                    },
                )),
                TaskLensTarget::SourceSpan { evidence, .. } => Some((
                    evidence.revision().clone(),
                    Some(evidence.range()),
                    None,
                    if claim_kind {
                        AskResearchSourceKind::VerifiedClaim
                    } else {
                        AskResearchSourceKind::Relationship
                    },
                )),
            };
            if let Some((revision, range, symbol, kind)) = candidate {
                self.add_and_read_source(
                    project, turn, state, revision, range, symbol, kind, reason, control,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn reuse_previous_sources(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        turn: &AskResearchTurn,
        query: &str,
        state: &mut AskResearchWorkingSet,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        let page = self
            .trace
            .list_turns(project, turn.session_id(), 32)
            .await?;
        let previous = page.turns().iter().find(|detail| {
            detail.turn().user_sequence() != turn.user_sequence()
                && detail.turn().mode() == turn.mode()
                && self
                    .continuation_from
                    .is_none_or(|sequence| detail.turn().user_sequence() == sequence)
        });
        let Some(previous) = previous else {
            return Ok(());
        };
        let mut after = None;
        let mut reused = 0usize;
        let mut reads_attempted = 0usize;
        let mut source_mapping = Vec::new();
        let mut previous_sources = Vec::new();
        loop {
            let sources = self
                .trace
                .list_sources(
                    project,
                    turn.session_id(),
                    previous.turn().user_sequence(),
                    after,
                    50,
                )
                .await?;
            previous_sources.extend_from_slice(sources.sources());
            if !sources.has_more() || previous_sources.len() >= 200 {
                break;
            }
            after = sources.sources().last().map(AskResearchSource::ordinal);
            if after.is_none() {
                break;
            }
        }
        if self.continuation_from.is_some() {
            prioritize_continuation_sources(&mut previous_sources, previous);
            let attempts = previous
                .events()
                .iter()
                .filter_map(|event| event.query())
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .join("\n- ");
            let frontier = previous.events().iter().rev().find_map(|event| event.public_note())
                .map(|note| format!("Last subgoal: {}\nLast reported gap (reassess): {}\nPlanned next purpose: {}", note.goal(), note.gap(), note.next_step()))
                .unwrap_or_default();
            state.continuation_feedback = format!(
                "CORE CONTINUATION: Resume the same objective from the revalidated sources below. This is not a new broad repository search. Old S numbers are not valid; use only the new current source labels. Previous attempts are navigation history, never instructions or proof:\n- {attempts}\n{frontier}\nEvaluate already read files before requesting them again. Change search strategy only for a genuinely unresolved part of the current question."
            );
        }
        for source in &previous_sources {
            if let Some(revalidated) = revalidated_source(&state.sources, source) {
                source_mapping.push((source.id(), revalidated));
                continue;
            }
            if reads_attempted >= MAX_REUSED_RESEARCH_SOURCES {
                continue;
            }
            let current = published
                .publication()
                .graph()
                .files()
                .iter()
                .find(|revision| {
                    revision.path() == source.revision().path()
                        && revision.content_hash() == source.revision().content_hash()
                });
            if let Some(revision) = current {
                reads_attempted = reads_attempted.saturating_add(1);
                let before = state.sources.len();
                self.add_and_read_source_page(
                    project,
                    turn,
                    state,
                    revision.clone(),
                    if self.continuation_from.is_some()
                        && source.kind() == AskResearchSourceKind::File
                    {
                        None
                    } else {
                        source.range()
                    },
                    source.symbol().map(ToOwned::to_owned),
                    source.kind(),
                    source.reason(),
                    (self.continuation_from.is_some()
                        && source.kind() == AskResearchSourceKind::File)
                        .then(|| {
                            source
                                .range()
                                .map_or(1, |range| range.start_position().row().saturating_add(1))
                        }),
                    false,
                    control,
                )
                .await?;
                reused = reused.saturating_add(state.sources.len().saturating_sub(before));
                if let Some(revalidated) = revalidated_source(&state.sources, source) {
                    source_mapping.push((source.id(), revalidated));
                }
            }
        }
        let mut reused_findings = 0usize;
        if self.continuation_from.is_some()
            && let Some(work) = previous.work_state()
            && work.objective() == query
        {
            state.restore_work(work, &source_mapping)?;
            if let Some(restored) = &mut state.work {
                restored
                    .revalidate_in_scope(
                        published.publication().graph().files(),
                        Some(research_access::scope(published)),
                    )
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
            }
            self.append_work_checkpoint(project, turn, state).await?;
        }
        for event in previous.events().iter().rev().take(12).rev() {
            let Some(note) = event.public_note() else {
                continue;
            };
            let Some(mapped_note) = rebind_public_note(note, &source_mapping)? else {
                continue;
            };
            let mapped = mapped_note.source_ids().to_vec();
            if self.continuation_from.is_some() {
                state.event_sequence = state.event_sequence.saturating_add(1);
                self.append_note_event(project, turn, state.event_sequence, AskResearchPhase::Preparing,
                    "Arbeitsstand der vorherigen Recherche gegen den aktuellen Projektstand bestätigt", mapped_note).await?;
            }
            state.record_revalidated_note(query, note, mapped, self.continuation_from.is_some())?;
            reused_findings = reused_findings.saturating_add(1);
        }
        if reused > 0 {
            state.event_sequence = state.event_sequence.saturating_add(1);
            self.append_running_event(
                project,
                turn,
                state.event_sequence,
                AskResearchPhase::Preparing,
                &format!(
                    "{reused} frühere Quelle(n) und {reused_findings} Befund(e) gegen den aktuellen Index bestätigt"
                ),
                None,
                AskResearchCompleteness::Complete,
            )
            .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_and_read_source(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        revision: a3_domain::FileRevision,
        range: Option<a3_domain::SourceRange>,
        symbol: Option<String>,
        kind: AskResearchSourceKind,
        reason: AskResearchSelectionReason,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.add_and_read_source_page(
            project, turn, state, revision, range, symbol, kind, reason, None, false, control,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_and_read_prioritized_source(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        revision: a3_domain::FileRevision,
        range: Option<a3_domain::SourceRange>,
        symbol: Option<String>,
        kind: AskResearchSourceKind,
        reason: AskResearchSelectionReason,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.add_and_read_source_page(
            project, turn, state, revision, range, symbol, kind, reason, None, true, control,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_and_read_source_page(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        revision: a3_domain::FileRevision,
        range: Option<a3_domain::SourceRange>,
        symbol: Option<String>,
        kind: AskResearchSourceKind,
        reason: AskResearchSelectionReason,
        start_override: Option<u32>,
        prioritize_context: bool,
        control: &JobContext,
    ) -> Result<(), AgentSessionManagerFailure> {
        if range.is_some_and(|value| state.contains(&revision, Some(value)))
            || (range.is_none()
                && state.contains_page(
                    &revision,
                    start_override.unwrap_or(1),
                    if start_override.is_some() { 200 } else { 160 },
                    symbol.as_deref(),
                ))
        {
            state.focus_known_source(&revision, range, start_override.unwrap_or(1));
            return Ok(());
        }
        if state.sources.len() >= 200 {
            state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
            return Ok(());
        }
        let start = start_override.unwrap_or_else(|| {
            range.map_or(1, |value| {
                value
                    .start_position()
                    .row()
                    .saturating_sub(4)
                    .saturating_add(1)
            })
        });
        let line_count = range.map_or(if start_override.is_some() { 200 } else { 160 }, |value| {
            value
                .end_position()
                .row()
                .saturating_sub(value.start_position().row())
                .saturating_add(9)
                .clamp(1, 160)
        });
        let request = AgentFileInspection::new(
            revision.path().clone(),
            AgentFileStartLine::new(start)
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            AgentFileLineCount::new(
                u16::try_from(line_count).map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
        );
        let page = match WorkspaceAgentSourceReader
            .read_page(project, &revision, &request, control)
            .await
        {
            Ok(page) => page,
            Err(AgentSourceReadFailure::Cancelled) => {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            Err(AgentSourceReadFailure::Unavailable) if state.reserve_read_retry() => {
                state.event_sequence = state.event_sequence.saturating_add(1);
                self.append_running_event(
                    project,
                    turn,
                    state.event_sequence,
                    AskResearchPhase::InspectingSource,
                    "Vorübergehend fehlgeschlagenen Leseschritt erneut ausführen",
                    Some(&model_safe_path(revision.path())),
                    AskResearchCompleteness::NotApplicable,
                )
                .await?;
                match WorkspaceAgentSourceReader
                    .read_page(project, &revision, &request, control)
                    .await
                {
                    Ok(page) => page,
                    Err(AgentSourceReadFailure::Cancelled) => {
                        return Err(AgentSessionManagerFailure::Unavailable);
                    }
                    Err(_) => {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                        return Ok(());
                    }
                }
            }
            Err(_) => {
                state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                return Ok(());
            }
        };
        if range.is_none() {
            state.set_next_file_page(
                revision.path(),
                page.next_start_line().map(AgentFileStartLine::get),
            );
        }
        if page.start_line().get() == 1
            && page.next_start_line().is_none()
            && !state.complete_files.contains(&revision)
            && state.evidence_limit >= 64
        {
            state.complete_files.push(revision.clone());
        }
        let source_range = range.or(Some(page.range()));
        if state.contains(&revision, source_range) {
            state.focus_known_source(&revision, source_range, start);
            return Ok(());
        }
        let ordinal = u32::try_from(state.sources.len().saturating_add(1))
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        let source = AskResearchSource::new(
            turn.session_id(),
            turn.user_sequence(),
            AskResearchSourceId::from_bytes(random_id()?),
            ordinal,
            revision,
            source_range,
            symbol,
            kind,
            reason,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        if !state.render(
            &source,
            page.start_line().get(),
            page.text(),
            prioritize_context,
        ) {
            state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
            return Ok(());
        }
        self.trace
            .append_sources(project, std::slice::from_ref(&source))
            .await?;
        state.sources.push(source);
        Ok(())
    }

    async fn search_source(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        literals: Vec<String>,
        control: &JobContext,
    ) -> Result<String, AgentSessionManagerFailure> {
        state.event_sequence = state.event_sequence.saturating_add(1);
        let display = literals.join(", ");
        self.append_running_event(
            project,
            turn,
            state.event_sequence,
            AskResearchPhase::SearchingSource,
            "Aktuelle indexierte Dateien nach konkretem Text durchsuchen",
            Some(&display),
            AskResearchCompleteness::NotApplicable,
        )
        .await?;
        let search = AskSourceTextSearch::new(literals)
            .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
        let result = match self
            .source_searcher
            .search(project, published, &search, control)
            .await
        {
            Ok(result) => result,
            Err(AskSourceSearchFailure::Cancelled) => {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            Err(AskSourceSearchFailure::Unavailable) if state.reserve_read_retry() => {
                state.event_sequence = state.event_sequence.saturating_add(1);
                self.append_running_event(
                    project,
                    turn,
                    state.event_sequence,
                    AskResearchPhase::SearchingSource,
                    "Vorübergehend fehlgeschlagene Quelltextsuche erneut ausführen",
                    Some(&display),
                    AskResearchCompleteness::NotApplicable,
                )
                .await?;
                match self
                    .source_searcher
                    .search(project, published, &search, control)
                    .await
                {
                    Ok(result) => result,
                    Err(AskSourceSearchFailure::Cancelled) => {
                        return Err(AgentSessionManagerFailure::Unavailable);
                    }
                    Err(_) => {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                        return Ok("Quelltextsuche war vorübergehend nicht verfügbar; im nächsten Schritt muss ein anderer begrenzter Such- oder Lesepfad verwendet werden.".to_owned());
                    }
                }
            }
            Err(_) => {
                state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                return Ok("Quelltextsuche war nicht verfügbar; im nächsten Schritt muss ein anderer begrenzter Such- oder Lesepfad verwendet werden.".to_owned());
            }
        };
        state.observe_access(research_access::search_outcome(
            &result,
            published.publication().graph().files().len(),
        ));
        let before = state.sources.len();
        for hit in result.hits().iter().take(MAX_ADAPTIVE_SEARCH_SOURCES) {
            self.add_and_read_prioritized_source(
                project,
                turn,
                state,
                hit.revision().clone(),
                Some(hit.range()),
                None,
                AskResearchSourceKind::Symbol,
                AskResearchSelectionReason::SourceText,
                control,
            )
            .await?;
        }
        let summary = format!(
            "Quelltextsuche abgeschlossen: {} Treffer in {} sicher lesbaren Dateien; {} priorisierte aktuelle Quelle(n) für den nächsten Modellschritt bereitgestellt{}",
            result.hits().len(),
            result.files_examined(),
            state.sources.len().saturating_sub(before),
            if result.completeness() == AskResearchCompleteness::Limited {
                "; Suche wurde durch eine feste Grenze beendet"
            } else {
                ""
            }
        );
        Ok(summary)
    }

    async fn execute_actions(
        &self,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        actions: Vec<AskResearchAction>,
        control: &JobContext,
    ) -> Result<String, AgentSessionManagerFailure> {
        let mut summaries = Vec::new();
        for action in actions {
            if control.cancellation_token().is_cancelled() {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            let access = state
                .work
                .as_ref()
                .and_then(|w| w.next_question())
                .map(|question| {
                    research_access::identity(published, state, &action)
                        .map(|(key, kind)| (question, research_access::scope(published), key, kind))
                        .ok_or(AgentSessionManagerFailure::InvalidOutput)
                })
                .transpose()?;
            if let Some((question, scope, key, kind)) = access {
                let started = state
                    .work
                    .as_mut()
                    .ok_or(AgentSessionManagerFailure::InvalidOutput)?
                    .begin_access(question, scope, key, kind)
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
                if !started {
                    summaries.push("Identischer ergebnisloser Zugriff im selben Indexstand wurde nicht erneut ausgeführt; dies ist kein Beleg allgemeiner Nichtexistenz.".to_owned());
                    continue;
                }
                self.append_access_checkpoint(project, turn, state, kind, None)
                    .await?;
            }
            state.access_outcome = a3_domain::ResearchAccessOutcome::Completed;
            let result = self
                .execute_action_batch(project, published, turn, state, vec![action], control)
                .await;
            // Cancellation/crash leaves a durable unfinished attempt, never an invented result.
            if control.cancellation_token().is_cancelled() {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            if let Some((question, scope, key, kind)) = access {
                let outcome = if result.is_err() {
                    a3_domain::ResearchAccessOutcome::Unavailable
                } else {
                    state.access_outcome
                };
                state
                    .work
                    .as_mut()
                    .ok_or(AgentSessionManagerFailure::InvalidOutput)?
                    .finish_access(question, scope, key, outcome)
                    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
                self.append_access_checkpoint(project, turn, state, kind, Some(outcome))
                    .await?;
            }
            summaries.push(result?);
        }
        Ok(summaries.join("\n"))
    }

    async fn execute_action_batch(
        &self,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        actions: Vec<AskResearchAction>,
        control: &JobContext,
    ) -> Result<String, AgentSessionManagerFailure> {
        let mut results = Vec::new();
        for action in actions {
            if control.cancellation_token().is_cancelled() {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            match action {
                AskResearchAction::SearchSourceText(literals) => results.push(
                    self.search_source(project, published, turn, state, literals, control)
                        .await?,
                ),
                AskResearchAction::SearchIndex(query) => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::SelectingEvidence,
                        "Task Lens mit einer präziseren Suchfrage erneut zuschneiden",
                        Some(&query),
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let direct_targets = resolve_query_targets(published, &query);
                    let direct_before = state.sources.len();
                    for revision in resolved_target_revisions(&direct_targets) {
                        self.add_and_read_prioritized_source(
                            project,
                            turn,
                            state,
                            revision,
                            None,
                            None,
                            AskResearchSourceKind::File,
                            AskResearchSelectionReason::ExactNameOrPath,
                            control,
                        )
                        .await?;
                    }
                    let direct_added = state.sources.len().saturating_sub(direct_before);
                    let trace = match self.compile_lens(project, published, &query, control).await {
                        Ok(trace) => trace,
                        Err(error) if control.cancellation_token().is_cancelled() => {
                            return Err(error);
                        }
                        Err(_) => {
                            state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                            results.push("Die erneute Task-Lens-Auswahl war vorübergehend nicht verfügbar; wähle im nächsten Schritt eine direkte Pfad-, Text- oder Beziehungssuche.".to_owned());
                            continue;
                        }
                    };
                    if trace.lens().index_run_id() != turn.index_run_id()
                        || trace.lens().snapshot_id() != turn.snapshot_id()
                    {
                        return Err(AgentSessionManagerFailure::Conflict);
                    }
                    let before = state.sources.len();
                    self.add_lens_sources(project, turn, state, trace.lens(), control)
                        .await?;
                    results.push(format!(
                        "{} Dateiziel(e) wurden direkt aufgelöst, davon {direct_added} neu gelesen; Task Lens hat {} weitere aktuelle Quellen bereitgestellt.",
                        direct_targets
                            .iter()
                            .filter(|target| target.revision.is_some())
                            .count(),
                        state.sources.len().saturating_sub(before)
                    ));
                }
                AskResearchAction::InspectPath { path, start_line } => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::InspectingSource,
                        "Explizit angeforderten Pfad im gebundenen Index prüfen",
                        Some(&format!("{path} ab Zeile {start_line}")),
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let before = state.sources.len();
                    if let Some(revision) = resolve_index_path(published, &path) {
                        self.add_and_read_source_page(
                            project,
                            turn,
                            state,
                            revision.clone(),
                            None,
                            None,
                            AskResearchSourceKind::File,
                            AskResearchSelectionReason::ExactNameOrPath,
                            Some(start_line),
                            true,
                            control,
                        )
                        .await?;
                    } else {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Unresolved);
                    }
                    let known = state
                        .excerpts
                        .iter()
                        .find(|item| item.path == path && item.start_line == start_line);
                    results.push(if state.sources.len() > before { format!("Pfad {path} wurde ab Zeile {start_line} aktuell geprüft.") }
                        else if let Some(known) = known { format!("Pfad {path} ab Zeile {start_line} ist bereits als S{} gelesen. Werte den bereitgestellten Ausschnitt aus; identisches Lesen liefert keine neue Evidence.", known.ordinal) }
                        else { format!("Pfad {path} war im gebundenen Index nicht eindeutig auflösbar oder der angeforderte Abschnitt enthielt keine neue sicher lesbare Evidence.") });
                }
                AskResearchAction::InspectSource(ordinal) => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::InspectingSource,
                        "Bereits gefundene Quelle genauer lesen",
                        Some(&format!("Quelle S{ordinal}")),
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let source = state
                        .sources
                        .get(usize::from(ordinal.saturating_sub(1)))
                        .cloned()
                        .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
                    let request = AgentFileInspection::new(
                        source.revision().path().clone(),
                        AgentFileStartLine::new(source.range().map_or(1, |range| {
                            range
                                .start_position()
                                .row()
                                .saturating_sub(12)
                                .saturating_add(1)
                        }))
                        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                        AgentFileLineCount::new(200)
                            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
                    );
                    let page = match WorkspaceAgentSourceReader
                        .read_page(project, source.revision(), &request, control)
                        .await
                    {
                        Ok(page) => page,
                        Err(AgentSourceReadFailure::Cancelled) => {
                            return Err(AgentSessionManagerFailure::Unavailable);
                        }
                        Err(AgentSourceReadFailure::Unavailable) if state.reserve_read_retry() => {
                            state.event_sequence = state.event_sequence.saturating_add(1);
                            self.append_running_event(
                                project,
                                turn,
                                state.event_sequence,
                                AskResearchPhase::InspectingSource,
                                "Vorübergehend fehlgeschlagenen erweiterten Leseschritt erneut ausführen",
                                Some(&format!("Quelle S{ordinal}")),
                                AskResearchCompleteness::NotApplicable,
                            )
                            .await?;
                            match WorkspaceAgentSourceReader
                                .read_page(project, source.revision(), &request, control)
                                .await
                            {
                                Ok(page) => page,
                                Err(AgentSourceReadFailure::Cancelled) => {
                                    return Err(AgentSessionManagerFailure::Unavailable);
                                }
                                Err(_) => {
                                    state.observe_access(
                                        a3_domain::ResearchAccessOutcome::Unavailable,
                                    );
                                    results.push(format!("Quelle S{ordinal} konnte nicht erweitert werden; wähle im nächsten Schritt einen anderen Pfad- oder Suchzugang."));
                                    continue;
                                }
                            }
                        }
                        Err(_) => {
                            state.observe_access(a3_domain::ResearchAccessOutcome::Unavailable);
                            results.push(format!("Quelle S{ordinal} konnte nicht erweitert werden; wähle im nächsten Schritt einen anderen Pfad- oder Suchzugang."));
                            continue;
                        }
                    };
                    state.render_existing(ordinal, &source, page.start_line().get(), page.text());
                    results.push(format!(
                        "Quelle S{ordinal} wurde mit erweitertem Kontext gelesen."
                    ));
                }
                action @ AskResearchAction::InspectFunctionFlow { .. } => {
                    results.push(
                        self.read_function_flow(project, published, turn, state, &action, control)
                            .await?,
                    );
                }
                AskResearchAction::InspectRelations {
                    source_ordinal,
                    relation,
                } => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::Reading,
                        "Direkte Beziehungen einer bekannten Quelle im aktuellen Index verfolgen",
                        Some(&format!(
                            "Quelle S{source_ordinal} · {}",
                            relation_label(relation)
                        )),
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let before = state.sources.len();
                    let related = self
                        .inspect_relations(
                            project,
                            published,
                            turn,
                            state,
                            source_ordinal,
                            relation,
                            control,
                        )
                        .await?;
                    results.push(format!(
                        "{} Beziehung(en) geprüft; {} neue aktuelle Quelle(n) bereitgestellt.",
                        related,
                        state.sources.len().saturating_sub(before)
                    ));
                }
                AskResearchAction::ListDirectory(path) => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::Reading,
                        "Direkte Einträge eines Verzeichnisses aus dem gebundenen Index auflisten",
                        Some(if path.is_empty() { "." } else { &path }),
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let (children, limited) = list_index_directory(published, &path);
                    if limited {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
                    } else if children.is_empty() {
                        state.observe_access(a3_domain::ResearchAccessOutcome::NoMatch);
                    }
                    results.push(format!(
                        "Verzeichnis {}: {}{}",
                        if path.is_empty() { "." } else { &path },
                        if children.is_empty() {
                            "keine direkt auflösbaren Einträge".to_owned()
                        } else {
                            children.join(", ")
                        },
                        if limited {
                            " (auf 100 Einträge begrenzt)"
                        } else {
                            ""
                        }
                    ));
                }
                AskResearchAction::InspectWorkingChanges => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::Reading,
                        "Aktuelle lokale Änderungen begrenzt und ohne Quelltextpersistenz erfassen",
                        None,
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let changes = inspect_working_change_paths(project, control)?;
                    if changes.limited {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
                    }
                    let before = state.sources.len();
                    let mut unresolved = 0usize;
                    for path in &changes.paths {
                        let revision = published
                            .publication()
                            .graph()
                            .files()
                            .iter()
                            .find(|revision| revision.path().as_bytes() == path.as_slice());
                        if let Some(revision) = revision {
                            self.add_and_read_source(
                                project,
                                turn,
                                state,
                                revision.clone(),
                                None,
                                None,
                                AskResearchSourceKind::File,
                                AskResearchSelectionReason::ExactNameOrPath,
                                control,
                            )
                            .await?;
                        } else {
                            unresolved = unresolved.saturating_add(1);
                        }
                    }
                    results.push(format!(
                        "{} lokale Änderung(en) erfasst; {} aktuelle Quelle(n) gelesen; {} nicht im gebundenen Index auflösbar{}.",
                        changes.paths.len(),
                        state.sources.len().saturating_sub(before),
                        unresolved,
                        if changes.limited { " (auf 200 Dateien beziehungsweise 2 MiB begrenzt)" } else { "" }
                    ));
                }
                AskResearchAction::QueryIndexDiagnostics => {
                    state.event_sequence = state.event_sequence.saturating_add(1);
                    self.append_running_event(
                        project,
                        turn,
                        state.event_sequence,
                        AskResearchPhase::Reading,
                        "Aktuelle Parser- und Indexdiagnosen prüfen",
                        None,
                        AskResearchCompleteness::NotApplicable,
                    )
                    .await?;
                    let mut count = 0usize;
                    let before = state.sources.len();
                    'analyses: for analysis in published.publication().file_analyses() {
                        for diagnostic in analysis.diagnostics() {
                            if count == 100 {
                                break 'analyses;
                            }
                            self.add_and_read_source(
                                project,
                                turn,
                                state,
                                analysis.revision().clone(),
                                Some(diagnostic.range()),
                                None,
                                AskResearchSourceKind::Symbol,
                                AskResearchSelectionReason::IndexedText,
                                control,
                            )
                            .await?;
                            count = count.saturating_add(1);
                        }
                    }
                    results.push(format!(
                        "{count} aktuelle Diagnose(n) geprüft; {} Evidence-Quelle(n) bereitgestellt{}.",
                        state.sources.len().saturating_sub(before),
                        if count == 100 { " (auf 100 begrenzt)" } else { "" }
                    ));
                    if count == 100 {
                        state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
                    }
                }
                AskResearchAction::InspectDependencyGraph => {
                    let summary = self
                        .inspect_graph_topology(
                            project,
                            published,
                            turn,
                            state,
                            400,
                            |kind| {
                                matches!(
                                    kind,
                                    SyntaxRelationKind::Imports
                                        | SyntaxRelationKind::Exports
                                        | SyntaxRelationKind::Calls
                                        | SyntaxRelationKind::Builds
                                        | SyntaxRelationKind::Configures
                                )
                            },
                            "Aktuelle interne und manifestbelegte Abhängigkeiten verfolgen",
                            control,
                        )
                        .await?;
                    results.push(summary);
                }
                AskResearchAction::InspectTestTopology => {
                    let summary = self
                        .inspect_graph_topology(
                            project,
                            published,
                            turn,
                            state,
                            200,
                            |kind| kind == SyntaxRelationKind::Tests,
                            "Indexierte Testbeziehungen prüfen, ohne Laufzeit-Coverage zu behaupten",
                            control,
                        )
                        .await?;
                    results.push(summary);
                }
                AskResearchAction::ScanSecurityCandidates => {
                    results.push(
                        self.search_source(
                            project,
                            published,
                            turn,
                            state,
                            security_rule_literals(),
                            control,
                        )
                        .await?,
                    );
                }
            }
        }
        Ok(results.join("\n"))
    }

    #[allow(clippy::too_many_arguments)]
    async fn inspect_graph_topology(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        edge_limit: usize,
        include: impl Fn(SyntaxRelationKind) -> bool,
        action: &str,
        control: &JobContext,
    ) -> Result<String, AgentSessionManagerFailure> {
        state.event_sequence = state.event_sequence.saturating_add(1);
        self.append_running_event(
            project,
            turn,
            state.event_sequence,
            AskResearchPhase::Reading,
            action,
            None,
            AskResearchCompleteness::NotApplicable,
        )
        .await?;
        let before = state.sources.len();
        let mut matching = 0usize;
        let mut nodes = BTreeSet::new();
        let mut node_limited = false;
        for edge in published
            .publication()
            .graph()
            .edges()
            .iter()
            .filter(|edge| include(edge.kind()))
        {
            if matching == edge_limit {
                break;
            }
            let mut prospective_nodes = nodes.clone();
            prospective_nodes.insert(edge.source().clone());
            prospective_nodes.insert(edge.target().clone());
            if prospective_nodes.len() > 200 {
                node_limited = true;
                continue;
            }
            nodes = prospective_nodes;
            self.add_and_read_source(
                project,
                turn,
                state,
                edge.evidence().revision().clone(),
                Some(edge.evidence().range()),
                None,
                AskResearchSourceKind::Relationship,
                if edge.kind() == SyntaxRelationKind::Tests {
                    AskResearchSelectionReason::Test
                } else {
                    AskResearchSelectionReason::Relationship
                },
                control,
            )
            .await?;
            matching = matching.saturating_add(1);
        }
        let total = published
            .publication()
            .graph()
            .edges()
            .iter()
            .filter(|edge| include(edge.kind()))
            .count();
        if total > matching || node_limited {
            state.observe_access(a3_domain::ResearchAccessOutcome::Limited);
        }
        Ok(format!(
            "{matching} aktuelle Beziehung(en) geprüft; {} Evidence-Quelle(n) bereitgestellt{}.",
            state.sources.len().saturating_sub(before),
            if total > matching || node_limited {
                " (Ergebnis begrenzt)"
            } else {
                ""
            }
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn inspect_relations(
        &self,
        project: &ProjectIdentity,
        published: &a3_domain::PublishedIndex,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        source_ordinal: u16,
        relation: AskResearchRelation,
        control: &JobContext,
    ) -> Result<usize, AgentSessionManagerFailure> {
        let source = state
            .sources
            .get(usize::from(source_ordinal.saturating_sub(1)))
            .cloned()
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let graph = published.publication().graph();
        let symbol = graph.symbols().iter().find(|symbol| {
            symbol.revision() == source.revision()
                && source.symbol().is_some_and(|name| {
                    symbol.parsed().name().as_str() == name
                        && source
                            .range()
                            .is_none_or(|range| symbol.parsed().declaration_range() == range)
                })
        });
        let anchor = symbol
            .map(|symbol| GraphEndpoint::Symbol(symbol.id()))
            .unwrap_or_else(|| GraphEndpoint::File(source.revision().path().clone()));
        let matches_relation = |edge: &a3_domain::GraphEdge| match relation {
            AskResearchRelation::Callers => {
                edge.kind() == SyntaxRelationKind::Calls && edge.target() == &anchor
            }
            AskResearchRelation::Callees => {
                edge.kind() == SyntaxRelationKind::Calls && edge.source() == &anchor
            }
            AskResearchRelation::Imports => {
                edge.kind() == SyntaxRelationKind::Imports && edge.source() == &anchor
            }
            AskResearchRelation::Exports => {
                edge.kind() == SyntaxRelationKind::Exports && edge.source() == &anchor
            }
            AskResearchRelation::Tests => {
                edge.kind() == SyntaxRelationKind::Tests
                    && (edge.source() == &anchor || edge.target() == &anchor)
            }
        };
        let edges = graph
            .edges()
            .iter()
            .filter(|edge| matches_relation(edge))
            .take(50)
            .cloned()
            .collect::<Vec<_>>();
        for edge in &edges {
            self.add_and_read_source(
                project,
                turn,
                state,
                edge.evidence().revision().clone(),
                Some(edge.evidence().range()),
                None,
                AskResearchSourceKind::Relationship,
                if edge.kind() == SyntaxRelationKind::Tests {
                    AskResearchSelectionReason::Test
                } else {
                    AskResearchSelectionReason::Relationship
                },
                control,
            )
            .await?;
            let related_endpoint = if edge.source() == &anchor {
                edge.target()
            } else {
                edge.source()
            };
            if let GraphEndpoint::Symbol(symbol_id) = related_endpoint
                && let Some(related) = graph
                    .symbols()
                    .iter()
                    .find(|candidate| candidate.id() == *symbol_id)
            {
                self.add_and_read_source(
                    project,
                    turn,
                    state,
                    related.revision().clone(),
                    Some(related.parsed().declaration_range()),
                    Some(related.parsed().name().as_str().to_owned()),
                    AskResearchSourceKind::Symbol,
                    if edge.kind() == SyntaxRelationKind::Tests {
                        AskResearchSelectionReason::Test
                    } else {
                        AskResearchSelectionReason::Relationship
                    },
                    control,
                )
                .await?;
            }
        }
        Ok(edges.len())
    }

    #[allow(clippy::too_many_arguments)]
    async fn append_running_event(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        sequence: u32,
        phase: AskResearchPhase,
        action: &str,
        query: Option<&str>,
        completeness: AskResearchCompleteness,
    ) -> Result<(), AgentSessionManagerFailure> {
        let event = research_event(
            turn.session_id(),
            turn.user_sequence(),
            sequence,
            phase,
            AskResearchState::Running,
            action,
            query,
            completeness,
        )?;
        self.trace.append_event(project, &event).await?;
        Ok(())
    }

    async fn append_work_checkpoint(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
    ) -> Result<(), AgentSessionManagerFailure> {
        let Some(work) = &state.work else {
            return Ok(());
        };
        state.event_sequence = state.event_sequence.saturating_add(1);
        let event = research_event(turn.session_id(), turn.user_sequence(), state.event_sequence,
            AskResearchPhase::Evaluating, AskResearchState::Running,
            &format!("Recherche-Prüfstand gespeichert: {} von {} Teilfragen belegt oder begrenzt beantwortet", work.resolved_count(), work.questions().len()),
            None, AskResearchCompleteness::NotApplicable)?.with_work_state(work.clone());
        self.trace.append_event(project, &event).await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn append_note_event(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        sequence: u32,
        phase: AskResearchPhase,
        action: &str,
        note: AskResearchPublicNote,
    ) -> Result<(), AgentSessionManagerFailure> {
        let event = research_event(
            turn.session_id(),
            turn.user_sequence(),
            sequence,
            phase,
            AskResearchState::Running,
            action,
            None,
            AskResearchCompleteness::NotApplicable,
        )?
        .with_public_note(note);
        self.trace.append_event(project, &event).await?;
        Ok(())
    }
}

struct AskResearchResult {
    markdown: String,
    citations: Vec<AskResearchSourceId>,
    diagrams: Vec<EvidenceDiagramArtifact>,
    terminal_event: AskResearchEvent,
    awaiting_continuation: bool,
    handoff: ResearchHandoff,
}

#[cfg(test)]
#[path = "agent_research_followup_tests.rs"]
mod research_followup_tests;
#[cfg(test)]
#[path = "agent_research_regression_tests.rs"]
mod research_regression_tests;

#[path = "agent_research_context.rs"]
mod research_context;
#[path = "agent_research_flows.rs"]
mod research_flows;
#[path = "agent_research_followup.rs"]
mod research_followup;
use research_followup::ResearchStopReason;
#[path = "agent_research_access.rs"]
pub(crate) mod research_access;
#[path = "agent_research_model.rs"]
mod research_model;
#[path = "agent_research_work.rs"]
mod research_work;
use research_model::ResearchModel;

struct AskResearchWorkingSet {
    access_outcome: a3_domain::ResearchAccessOutcome,
    work: Option<a3_domain::ResearchWorkState>,
    work_required_revisions: Vec<a3_domain::FileRevision>,
    sources: Vec<AskResearchSource>,
    excerpts: Vec<ResearchSourceExcerpt>,
    evidence_limit: usize,
    event_sequence: u32,
    evidence_revision: usize,
    memory: Option<ResearchMemoryCheckpoint>,
    memory_findings: Vec<ResearchMemoryFinding>,
    memory_gaps: Vec<String>,
    read_retries_used: u8,
    next_file_pages: BTreeMap<a3_domain::RepositoryPath, u32>,
    complete_files: Vec<a3_domain::FileRevision>,
    read_coverage: Vec<research_context::CoveredRange>,
    delivered: Vec<research_context::CoveredRange>,
    current_delivery: Vec<research_context::CoveredRange>,
    current_source_delivery: Vec<research_context::DeliveredWindow>,
    delivery_revision: usize,
    focus: Vec<research_context::SourceFocus>,
    retained_units: Vec<research_context::CoveredRange>,
    continuation_feedback: String,
    partial_answer: Option<(String, Vec<u16>)>,
    last_note: Option<a3_application::AskResearchDecisionNote>,
}

struct ResearchSourceExcerpt {
    ordinal: u32,
    path: String,
    start_line: u32,
    text: String,
}

impl AskResearchWorkingSet {
    fn new(evidence_limit: usize) -> Self {
        Self {
            access_outcome: a3_domain::ResearchAccessOutcome::Completed,
            work: None,
            work_required_revisions: Vec::new(),
            sources: Vec::new(),
            excerpts: Vec::new(),
            evidence_limit,
            event_sequence: 1,
            evidence_revision: 0,
            memory: None,
            memory_findings: Vec::new(),
            memory_gaps: Vec::new(),
            read_retries_used: 0,
            next_file_pages: BTreeMap::new(),
            complete_files: Vec::new(),
            read_coverage: Vec::new(),
            delivered: Vec::new(),
            current_delivery: Vec::new(),
            current_source_delivery: Vec::new(),
            delivery_revision: 0,
            focus: Vec::new(),
            retained_units: Vec::new(),
            continuation_feedback: String::new(),
            partial_answer: None,
            last_note: None,
        }
    }
    fn contains(
        &self,
        revision: &a3_domain::FileRevision,
        range: Option<a3_domain::SourceRange>,
    ) -> bool {
        self.sources
            .iter()
            .any(|source| source.revision() == revision && source.range() == range)
    }
    fn contains_page(
        &self,
        revision: &a3_domain::FileRevision,
        start_line: u32,
        line_count: u32,
        symbol: Option<&str>,
    ) -> bool {
        self.sources.iter().any(|source| {
            source.revision() == revision
                && source.symbol() == symbol
                && source.range().is_some_and(|range| {
                    range.start_position().row().saturating_add(1) == start_line
                        && range.end_position().row().saturating_add(1)
                            >= start_line.saturating_add(line_count.saturating_sub(1))
                })
        })
    }
    fn covers(&self, revisions: &[a3_domain::FileRevision]) -> bool {
        revisions.iter().all(|revision| {
            self.sources
                .iter()
                .any(|source| source.revision() == revision)
        })
    }
    fn citations_cover(&self, ordinals: &[u16], revisions: &[a3_domain::FileRevision]) -> bool {
        revisions.iter().all(|revision| {
            ordinals.iter().any(|ordinal| {
                self.sources
                    .get(usize::from(ordinal.saturating_sub(1)))
                    .is_some_and(|source| source.revision() == revision)
            })
        })
    }
    fn reserve_read_retry(&mut self) -> bool {
        if self.read_retries_used >= MAX_RESEARCH_READ_RETRIES {
            return false;
        }
        self.read_retries_used = self.read_retries_used.saturating_add(1);
        true
    }
    fn set_next_file_page(
        &mut self,
        path: &a3_domain::RepositoryPath,
        next_start_line: Option<u32>,
    ) {
        if let Some(next_start_line) = next_start_line {
            self.next_file_pages.insert(path.clone(), next_start_line);
        } else {
            self.next_file_pages.remove(path);
        }
    }
    fn render(
        &mut self,
        source: &AskResearchSource,
        start_line: u32,
        text: &str,
        prioritize_context: bool,
    ) -> bool {
        if self.evidence_limit < 64 {
            return false;
        }
        let excerpt = ResearchSourceExcerpt {
            ordinal: source.ordinal(),
            path: model_safe_path(source.revision().path()),
            start_line,
            text: utf8_prefix(text, 32 * 1024).to_owned(),
        };
        if prioritize_context {
            self.excerpts.insert(0, excerpt);
        } else {
            self.excerpts.push(excerpt);
        }
        self.record_read_coverage(source, start_line, text);
        true
    }
    fn render_existing(
        &mut self,
        ordinal: u16,
        source: &AskResearchSource,
        start_line: u32,
        text: &str,
    ) {
        let text = utf8_prefix(text, 32 * 1024).to_owned();
        let previous = self
            .excerpts
            .iter()
            .position(|item| item.ordinal == u32::from(ordinal))
            .map(|index| self.excerpts.remove(index));
        if previous
            .as_ref()
            .is_none_or(|item| item.start_line != start_line || item.text != text)
        {
            self.record_read_coverage(source, start_line, &text);
        }
        self.excerpts.insert(
            0,
            ResearchSourceExcerpt {
                ordinal: u32::from(ordinal),
                path: model_safe_path(source.revision().path()),
                start_line,
                text,
            },
        );
    }

    fn focus_actions(&mut self, actions: &[AskResearchAction]) -> Vec<AskResearchAction> {
        self.focus.clear();
        let unread = actions
            .iter()
            .filter(|action| !self.focus_cached(action))
            .cloned()
            .collect();
        for action in actions.iter().rev() {
            let index = self.excerpts.iter().position(|item| match action {
                AskResearchAction::InspectSource(ordinal) => item.ordinal == u32::from(*ordinal),
                AskResearchAction::InspectPath { path, start_line } => {
                    item.path == *path && item.start_line == *start_line
                }
                _ => false,
            });
            if let Some(index) = index {
                let item = self.excerpts.remove(index);
                self.excerpts.insert(0, item);
            }
        }
        unread
    }

    fn focus_known_source(
        &mut self,
        revision: &a3_domain::FileRevision,
        range: Option<a3_domain::SourceRange>,
        start_line: u32,
    ) {
        self.focus.clear();
        self.focus_at(
            revision.clone(),
            a3_domain::SourcePosition::new(start_line.saturating_sub(1), 0),
        );
        let known = self
            .sources
            .iter()
            .find(|source| {
                source.revision() == revision
                    && range.map_or_else(
                        || {
                            source.range().is_some_and(|value| {
                                value.start_position().row().saturating_add(1) == start_line
                            })
                        },
                        |value| source.range() == Some(value),
                    )
            })
            .map(AskResearchSource::ordinal);
        if let Some(index) = self
            .excerpts
            .iter()
            .position(|item| Some(item.ordinal) == known)
        {
            let item = self.excerpts.remove(index);
            self.excerpts.insert(0, item);
        }
    }

    #[cfg(test)]
    fn evidence_window(&mut self) -> String {
        self.compile_evidence_window(&[], self.evidence_limit)
    }

    fn context_feedback(&self, action_feedback: &str) -> String {
        if self.work.is_some() {
            return action_feedback.to_owned();
        }
        let Some(gap) = self.memory_gaps.last() else {
            return action_feedback.to_owned();
        };
        let next = self
            .last_note
            .as_ref()
            .map_or("Reassess current sources", |note| note.next_step.as_str());
        format!(
            "PUBLIC RESEARCH FRONTIER (not evidence; may now be resolved):\nGap: {}\nNext: {}\n\n{action_feedback}",
            utf8_prefix(gap, 256),
            utf8_prefix(next, 128)
        )
    }

    fn model_evidence(&mut self, query: &str, query_targets: &[ResolvedQueryTarget]) -> String {
        let limit = self.evidence_limit;
        let metadata_limit = limit.saturating_sub(query.len().saturating_add(128));
        let memory_limit = (limit / 6).min(metadata_limit / 2).min(2048);
        let memory = if self.work.is_some() {
            self.work_context()
        } else {
            self.memory.as_ref().map_or_else(String::new, |checkpoint| {
                let findings = checkpoint
                    .findings()
                    .iter()
                    .rev()
                    .take(3)
                    .map(|finding| {
                        let references = finding
                            .sources
                            .iter()
                            .filter_map(|source_id| {
                                self.sources
                                    .iter()
                                    .find(|source| source.id() == *source_id)
                                    .map(|source| format!("S{}", source.ordinal()))
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "- {:?}: {} [{}]",
                            finding.kind,
                            utf8_prefix(&finding.text, (limit / 48).clamp(32, 160)),
                            references
                        )
                    })
                    .collect::<Vec<_>>();
                let mut notes = String::from(
                    "\nPUBLIC WORKING NOTES (not evidence; recheck current sources):\n",
                );
                for line in findings {
                    if notes.len() + line.len() < memory_limit {
                        notes.push_str(&line);
                        notes.push('\n');
                    }
                }
                if notes.len() <= memory_limit {
                    notes
                } else {
                    String::new()
                }
            })
        };
        let next_pages = self
            .next_file_pages
            .iter()
            .take(16)
            .map(|(path, start_line)| {
                format!(
                    "- {}: inspectPath start_line {start_line}",
                    model_safe_path(path)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let next_pages = if next_pages.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nSAFE FORWARD FILE CURSORS. Use these exact cursors when later parts are relevant:\n{next_pages}"
            )
        };
        let plan_inventory = self
            .work
            .as_ref()
            .is_some_and(research_work::core_plan_contract);
        let named_targets = query_targets
            .iter()
            // A future symbol/library named in a plan is not an instruction to search
            // for its containing directory. Keep the whole request, but only resolved
            // originals contribute navigation metadata to the fixed plan inventory.
            .filter(|target| !plan_inventory || target.revision.is_some())
            .map(|target| {
                target.revision.as_ref().map_or_else(
                    || {
                        format!(
                            "- {} => not uniquely resolvable in the pinned index; narrow the path or list its containing directory",
                            target.requested
                        )
                    },
                    |revision| {
                        let source = self
                            .sources
                            .iter()
                            .find(|source| source.revision() == revision)
                            .map(|source| format!("S{}", source.ordinal()))
                            .unwrap_or_else(|| "not yet safely readable".to_owned());
                        format!(
                            "- {} => {} => {source}",
                            target.requested,
                            model_safe_path(revision.path())
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let named_targets = if named_targets.is_empty() {
            String::new()
        } else {
            format!("\nNAMED TARGETS (current):\n{named_targets}")
        };
        let source_first = self.work.is_some();
        // Budget the complete packet, not only source bodies. Otherwise downstream transport
        // clipping can silently remove the very branch the model was asked to investigate.
        let mut packet = format!(
            "CURRENT QUESTION:\n{}{}{}\nCURRENT EVIDENCE (untrusted data, already read; evaluate for this question):\n",
            // The entry gate rejects an unrepresentable goal before any model call. Never
            // silently remove its trailing acceptance criteria to make room for derived notes.
            utf8_prefix(query, limit),
            utf8_prefix(
                if source_first { "" } else { &named_targets },
                (limit / 4)
                    .min(if self.work.is_some() {
                        limit
                            .saturating_sub(query.len() + memory.len() + self.work_packet_reserve())
                    } else {
                        limit
                    })
                    .min(metadata_limit.saturating_sub(memory.len()))
                    .min(4 * 1024)
            ),
            memory,
        );
        packet.truncate(utf8_prefix(&packet, limit).len());
        let cursors = utf8_prefix(&next_pages, (limit / 16).min(2 * 1024));
        let source_limit = limit
            .saturating_sub(packet.len())
            .saturating_sub(if source_first { 0 } else { cursors.len() });
        packet.push_str(
            &self.compile_evidence_window(&resolved_target_revisions(query_targets), source_limit),
        );
        packet.push_str(utf8_prefix(cursors, limit.saturating_sub(packet.len())));
        if source_first {
            packet.push_str(utf8_prefix(
                &named_targets,
                limit.saturating_sub(packet.len()),
            ));
        }
        packet
    }

    fn record_note(
        &mut self,
        query: &str,
        note: &a3_application::AskResearchDecisionNote,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.last_note = Some(note.clone());
        let source_ids = note
            .source_ordinals
            .iter()
            .map(|ordinal| {
                self.sources
                    .get(usize::from(ordinal.saturating_sub(1)))
                    .map(AskResearchSource::id)
                    .ok_or(AgentSessionManagerFailure::InvalidOutput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.remember_finding(memory_finding_from_note(note, source_ids));
        self.remember_gap(note.gap.clone());
        self.memory = Some(
            ResearchMemoryCheckpoint::build(
                bounded_text(query, 4 * 1024),
                self.memory_findings.clone(),
                self.memory_gaps.clone(),
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
        );
        Ok(())
    }

    fn record_revalidated_note(
        &mut self,
        query: &str,
        note: &AskResearchPublicNote,
        source_ids: Vec<AskResearchSourceId>,
        restore_frontier: bool,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.remember_finding(ResearchMemoryFinding {
            kind: match note.finding_kind() {
                AskResearchPublicFindingKind::Observation => ResearchMemoryFindingKind::Observation,
                AskResearchPublicFindingKind::Hypothesis => ResearchMemoryFindingKind::Hypothesis,
                AskResearchPublicFindingKind::Conclusion => ResearchMemoryFindingKind::Conclusion,
            },
            text: note.finding().to_owned(),
            sources: source_ids,
        });
        if restore_frontier {
            self.remember_gap(note.gap().to_owned());
        }
        self.memory = Some(
            ResearchMemoryCheckpoint::build(
                bounded_text(query, 4 * 1024),
                self.memory_findings.clone(),
                self.memory_gaps.clone(),
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
        );
        Ok(())
    }

    fn remember_finding(&mut self, finding: ResearchMemoryFinding) {
        if !self.memory_findings.contains(&finding) {
            self.memory_findings.push(finding);
            let overflow = self
                .memory_findings
                .len()
                .saturating_sub(MAX_RESEARCH_MEMORY_FINDINGS);
            if overflow > 0 {
                self.memory_findings.drain(..overflow);
            }
        }
    }

    fn remember_gap(&mut self, gap: String) {
        // The public journal preserves history; old reported gaps are not permanent obligations.
        self.memory_gaps.clear();
        self.memory_gaps.push(gap);
    }
}

impl fmt::Debug for AgentAskResearcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentAskResearcher")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct ConversationTaskLensControl<'a> {
    context: &'a JobContext,
}

#[derive(Debug)]
struct PinnedTaskLensIndex {
    published: Arc<a3_domain::PublishedIndex>,
}

impl TaskLensIndexStore for PinnedTaskLensIndex {
    fn load_current_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        control: &'a dyn TaskLensControl,
    ) -> a3_application::TaskLensIndexStoreFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                Err(a3_application::KnowledgeIndexFailure::Cancelled)
            } else {
                Ok(Some(Arc::clone(&self.published)))
            }
        })
    }
}

#[derive(Debug)]
struct ConversationIndexControl<'a> {
    context: &'a JobContext,
}

impl a3_application::IndexPersistenceControl for ConversationIndexControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}

#[derive(Debug)]
struct PlanStartIndexControl;

impl a3_application::IndexPersistenceControl for PlanStartIndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}

impl TaskLensControl for ConversationTaskLensControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.context.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        // A conversation may compile Task Lens repeatedly. Nested phase counters restart at zero,
        // so forwarding them would regress the owning job's monotone progress and abort the run.
        // The conversation reports its own coarse 0/8/9/10 progress around all research rounds.
        Ok(())
    }
}

/// Keeps the presentation link separate from the authoritative Agent Run state.
pub(crate) struct AgentSessionRunReporter {
    store: Arc<dyn AgentSessionStore>,
    links: Mutex<BTreeMap<TaskId, AgentSessionId>>,
    queue_wake: Mutex<Option<Weak<ConversationQueueWake>>>,
}

impl AgentSessionRunReporter {
    #[must_use]
    pub(crate) fn new(store: Arc<dyn AgentSessionStore>) -> Self {
        Self {
            store,
            links: Mutex::new(BTreeMap::new()),
            queue_wake: Mutex::new(None),
        }
    }

    fn bind_queue_wake(&self, wake: &Arc<ConversationQueueWake>) {
        *lock_recovering_poison(&self.queue_wake) = Some(Arc::downgrade(wake));
    }

    pub(crate) fn link(&self, task_id: TaskId, session_id: AgentSessionId) {
        lock_recovering_poison(&self.links).insert(task_id, session_id);
    }

    pub(crate) async fn report(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        state: AgentSessionState,
        message: &str,
    ) -> Result<(), AgentSessionManagerFailure> {
        let session_id = lock_recovering_poison(&self.links)
            .get(&task_id)
            .copied()
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        let detail = self
            .store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if detail.session().presentation_deleted() {
            return Ok(());
        }
        if detail.session().state() == state
            && matches!(
                state,
                AgentSessionState::AwaitingApproval | AgentSessionState::Paused
            )
        {
            return Ok(());
        }
        let sequence = next_sequence(detail.session().latest_sequence())?;
        let now = timestamp()?;
        let next = successor(
            detail.session(),
            SessionSuccessor {
                title: detail.session().title().as_str().to_owned(),
                mode: detail.session().mode(),
                state,
                updated_at: now,
                latest_sequence: Some(sequence),
                active_work_item: detail.session().active_work_item(),
                plan_revision: detail.session().current_plan_revision(),
                presentation_deleted: false,
            },
        )?;
        let kind = match state {
            AgentSessionState::Completed
            | AgentSessionState::Failed
            | AgentSessionState::Cancelled => AgentSessionEntryKind::FinalReport,
            AgentSessionState::AwaitingUser => AgentSessionEntryKind::AssistantSummary,
            _ => AgentSessionEntryKind::Activity,
        };
        let work_item = detail.session().active_work_item();
        let entry = AgentSessionEntry::try_new(
            session_id,
            sequence,
            kind,
            AgentSessionText::try_from_string(message.to_owned())
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            now,
            work_item.map(AgentWorkItem::id),
            Some(task_id),
            None,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        validate_agent_session_transition(detail.session(), &next)?;
        self.store
            .append_session_revision(
                project,
                detail.session().revision(),
                &next,
                Some(&entry),
                None,
            )
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        if matches!(
            state,
            AgentSessionState::Completed | AgentSessionState::Failed | AgentSessionState::Cancelled
        ) {
            let wake = lock_recovering_poison(&self.queue_wake)
                .as_ref()
                .and_then(Weak::upgrade);
            if let Some(wake) = wake {
                wake.after_agent_terminal(project, session_id, state).await;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for AgentSessionRunReporter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSessionRunReporter")
            .field("link_count", &lock_recovering_poison(&self.links).len())
            .finish_non_exhaustive()
    }
}

/// Owns durable conversation orchestration without becoming an Agent execution authority.
pub(crate) struct AgentSessionManager {
    store: Arc<dyn AgentSessionStore>,
    runtime: AgentConversationRuntime,
    submitter: JobSubmitter,
    job_ids: Arc<DesktopJobIds>,
    materializer: Option<AgentTaskMaterializer>,
    run_manager: Option<Arc<AgentRunManager>>,
    reporter: Option<Arc<AgentSessionRunReporter>>,
    researcher: Option<AgentAskResearcher>,
    research_store: Option<Arc<dyn AskResearchStore>>,
    active_session: Arc<Mutex<Option<ActiveConversation>>>,
    auto_dispatch_queues: Arc<Mutex<HashSet<(WorktreeId, AgentSessionId)>>>,
    _queue_wake: Option<Arc<ConversationQueueWake>>,
}

struct ConversationQueueWake {
    dependencies: AgentSessionManagerDependencies,
    active_session: Arc<Mutex<Option<ActiveConversation>>>,
    auto_dispatch_queues: Arc<Mutex<HashSet<(WorktreeId, AgentSessionId)>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueDispatchTrigger {
    Automatic,
    ExplicitResume,
}

impl ConversationQueueWake {
    async fn after_agent_terminal(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        state: AgentSessionState,
    ) {
        let dispatcher = AgentSessionManager::with_active_session(
            self.dependencies.clone(),
            Arc::clone(&self.active_session),
            Arc::clone(&self.auto_dispatch_queues),
        );
        if state == AgentSessionState::Completed {
            let _dispatched = dispatcher
                .dispatch_next_queued(project, QueueDispatchTrigger::Automatic)
                .await;
        } else if let Ok(queue) = dispatcher
            .store
            .load_message_queue(project, session_id)
            .await
            && !queue.messages().is_empty()
            && !queue.paused()
        {
            let _paused = dispatcher
                .store
                .set_message_queue_paused(project, session_id, queue.revision(), true)
                .await;
            lock_recovering_poison(&dispatcher.auto_dispatch_queues)
                .remove(&(project.worktree().id(), session_id));
        }
    }
}

/// Composition-root-owned dependencies for the conversation projection and its runtime bridges.
#[derive(Clone)]
pub(crate) struct AgentSessionManagerDependencies {
    pub(crate) store: Arc<dyn AgentSessionStore>,
    pub(crate) runtime: AgentConversationRuntime,
    pub(crate) submitter: JobSubmitter,
    pub(crate) job_ids: Arc<DesktopJobIds>,
    pub(crate) materializer: Option<AgentTaskMaterializer>,
    pub(crate) run_manager: Option<Arc<AgentRunManager>>,
    pub(crate) reporter: Option<Arc<AgentSessionRunReporter>>,
    pub(crate) researcher: Option<AgentAskResearcher>,
    pub(crate) research_store: Option<Arc<dyn AskResearchStore>>,
}

impl AgentSessionManager {
    #[must_use]
    pub(crate) fn new(dependencies: AgentSessionManagerDependencies) -> Self {
        let active_session = Arc::new(Mutex::new(None));
        let auto_dispatch_queues = Arc::new(Mutex::new(HashSet::new()));
        let queue_wake = Arc::new(ConversationQueueWake {
            dependencies: dependencies.clone(),
            active_session: Arc::clone(&active_session),
            auto_dispatch_queues: Arc::clone(&auto_dispatch_queues),
        });
        if let Some(reporter) = dependencies.reporter.as_ref() {
            reporter.bind_queue_wake(&queue_wake);
        }
        Self {
            store: dependencies.store,
            runtime: dependencies.runtime,
            submitter: dependencies.submitter,
            job_ids: dependencies.job_ids,
            materializer: dependencies.materializer,
            run_manager: dependencies.run_manager,
            reporter: dependencies.reporter,
            researcher: dependencies.researcher,
            research_store: dependencies.research_store,
            active_session,
            auto_dispatch_queues,
            _queue_wake: Some(queue_wake),
        }
    }

    fn with_active_session(
        dependencies: AgentSessionManagerDependencies,
        active_session: Arc<Mutex<Option<ActiveConversation>>>,
        auto_dispatch_queues: Arc<Mutex<HashSet<(WorktreeId, AgentSessionId)>>>,
    ) -> Self {
        Self {
            store: dependencies.store,
            runtime: dependencies.runtime,
            submitter: dependencies.submitter,
            job_ids: dependencies.job_ids,
            materializer: dependencies.materializer,
            run_manager: dependencies.run_manager,
            reporter: dependencies.reporter,
            researcher: dependencies.researcher,
            research_store: dependencies.research_store,
            active_session,
            auto_dispatch_queues,
            _queue_wake: None,
        }
    }

    fn dependencies(&self) -> AgentSessionManagerDependencies {
        AgentSessionManagerDependencies {
            store: Arc::clone(&self.store),
            runtime: self.runtime.clone(),
            submitter: self.submitter.clone(),
            job_ids: Arc::clone(&self.job_ids),
            materializer: self.materializer.clone(),
            run_manager: self.run_manager.clone(),
            reporter: self.reporter.clone(),
            researcher: self.researcher.clone(),
            research_store: self.research_store.clone(),
        }
    }

    pub(crate) async fn list(
        &self,
        project: &ProjectIdentity,
        query: &AgentSessionListQuery,
    ) -> Result<AgentSessionPage, AgentSessionManagerFailure> {
        self.release_terminal_job();
        let page = self
            .store
            .list_sessions(project, query)
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        let mut recovered = false;
        for session in page.sessions() {
            recovered |= self
                .recover_interrupted_preparation(project, session)
                .await?;
        }
        if recovered {
            self.store
                .list_sessions(project, query)
                .await
                .map_err(Into::into)
        } else {
            Ok(page)
        }
    }

    pub(crate) async fn recover_interrupted_preparations(
        &self,
        project: &ProjectIdentity,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.release_terminal_job();
        if lock_recovering_poison(&self.active_session).is_some() {
            return Err(AgentSessionManagerFailure::Busy);
        }
        let mut before_updated_at = None;
        loop {
            let query = AgentSessionListQuery::new(None, false, before_updated_at, 50)
                .map_err(AgentSessionManagerFailure::from)?;
            let page = self
                .store
                .list_sessions(project, &query)
                .await
                .map_err(AgentSessionManagerFailure::from)?;
            for session in page.sessions() {
                let _recovered = self
                    .recover_interrupted_preparation(project, session)
                    .await?;
            }
            if !page.has_more() {
                break;
            }
            before_updated_at = page
                .sessions()
                .last()
                .map(|session| session.updated_at().unix_millis());
            if before_updated_at.is_none() {
                return Err(AgentSessionManagerFailure::InvalidOutput);
            }
        }
        Ok(())
    }

    pub(crate) async fn load(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        limit: u16,
    ) -> Result<Option<AgentSessionDetail>, AgentSessionManagerFailure> {
        self.release_terminal_job();
        let mut detail = self
            .store
            .load_session(project, session_id, before_sequence, limit)
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        if let Some(current) = detail.as_ref()
            && self
                .recover_interrupted_preparation(project, current.session())
                .await?
        {
            detail = self
                .store
                .load_session(project, session_id, before_sequence, limit)
                .await
                .map_err(AgentSessionManagerFailure::from)?;
        }
        if let (Some(reporter), Some(work_item)) = (
            self.reporter.as_ref(),
            detail
                .as_ref()
                .and_then(|value| value.session().active_work_item()),
        ) {
            reporter.link(work_item.task_id(), session_id);
        }
        Ok(detail)
    }

    async fn recover_interrupted_preparation(
        &self,
        project: &ProjectIdentity,
        session: &AgentSession,
    ) -> Result<bool, AgentSessionManagerFailure> {
        if session.state() != AgentSessionState::Running
            || session.active_work_item().is_some()
            || lock_recovering_poison(&self.active_session)
                .as_ref()
                .is_some_and(|active| active.session_id == session.id())
        {
            return Ok(false);
        }
        let terminal = if session.mode() == AgentSessionMode::Agent
            && session.current_plan_revision().is_some()
        {
            ConversationTerminal::AgentStartInterrupted
        } else {
            ConversationTerminal::Failed(
                "Die vorherige Verarbeitung wurde unterbrochen. Der gespeicherte Stand bleibt erhalten; starte den Auftrag bitte erneut.",
            )
        };
        settle_unfinished_conversation(&self.store, project, session.id(), terminal).await?;
        Ok(true)
    }

    pub(crate) async fn load_commands(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        limit: u16,
    ) -> Result<Vec<a3_application::AgentSessionCommandPresentation>, AgentSessionManagerFailure>
    {
        self.store
            .load_session_commands(project, session_id, before_sequence, limit)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_queue(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
    ) -> Result<AgentSessionQueue, AgentSessionManagerFailure> {
        let queue = self
            .store
            .load_message_queue(project, session_id)
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        let key = (project.worktree().id(), session_id);
        if queue.messages().is_empty() || queue.paused() {
            if queue.messages().is_empty() {
                lock_recovering_poison(&self.auto_dispatch_queues).remove(&key);
            }
            return Ok(queue);
        }
        if lock_recovering_poison(&self.auto_dispatch_queues).contains(&key) {
            return Ok(queue);
        }
        self.store
            .set_message_queue_paused(project, session_id, queue.revision(), true)
            .await
            .map_err(Into::into)
    }

    /// Starts immediately when the capability is free; otherwise persists the validated message.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn submit_or_queue(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        target_mode: AgentSessionMode,
        explicit_depth: Option<AgentResearchDepth>,
        command_depth: bool,
        message: String,
    ) -> Result<AgentMessageSubmission, AgentSessionManagerFailure> {
        if session_id.is_none() {
            // A fresh Agent request has no reviewed plan lineage yet. Start its read-only
            // preparation in Plan and expose the explicit review stop before any task can exist.
            let (effective_mode, requires_plan_review) =
                resolve_next_message_mode(None, target_mode);
            let detail = self
                .submit_for_target_mode(
                    project,
                    None,
                    None,
                    effective_mode,
                    explicit_depth,
                    command_depth,
                    message,
                )
                .await?;
            return Ok(AgentMessageSubmission::Started {
                detail,
                requires_plan_review,
            });
        }
        let session_id = session_id.ok_or(AgentSessionManagerFailure::InvalidInput)?;
        let expected = expected_revision.ok_or(AgentSessionManagerFailure::InvalidInput)?;
        self.release_terminal_job();
        let current = self
            .store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if current.session().revision() != expected {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let should_queue = lock_recovering_poison(&self.active_session).is_some()
            || !matches!(
                current.session().state(),
                AgentSessionState::Draft
                    | AgentSessionState::AwaitingUser
                    | AgentSessionState::AwaitingPlanReview
                    | AgentSessionState::Completed
                    | AgentSessionState::Failed
                    | AgentSessionState::Cancelled
            );
        if !should_queue {
            let (effective_mode, requires_plan_review) =
                resolve_next_message_mode(Some(current.session().mode()), target_mode);
            let detail = self
                .submit_for_target_mode(
                    project,
                    Some(session_id),
                    Some(expected),
                    effective_mode,
                    explicit_depth,
                    command_depth,
                    message,
                )
                .await?;
            return Ok(AgentMessageSubmission::Started {
                detail,
                requires_plan_review,
            });
        }
        let (effective_mode, _) =
            resolve_next_message_mode(Some(current.session().mode()), target_mode);
        let selection = if command_depth {
            MessageResearchSelection::Command
        } else {
            MessageResearchSelection::ExplicitDepth(
                explicit_depth.ok_or(AgentSessionManagerFailure::InvalidInput)?,
            )
        };
        let _validated = resolve_submitted_message(effective_mode, selection, &message)?;
        let queue = self.store.load_message_queue(project, session_id).await?;
        let queued = AgentQueuedMessage::from_parts(
            AgentQueuedMessageId::from_bytes(random_id()?),
            session_id,
            queue
                .messages()
                .last()
                .map_or(1, |item| item.ordinal().saturating_add(1)),
            target_mode,
            match selection {
                MessageResearchSelection::LegacyDepth(AgentResearchDepth::Standard)
                | MessageResearchSelection::ExplicitDepth(AgentResearchDepth::Standard) => {
                    AgentQueuedResearchSelection::Standard
                }
                MessageResearchSelection::LegacyDepth(AgentResearchDepth::Thorough)
                | MessageResearchSelection::ExplicitDepth(AgentResearchDepth::Thorough) => {
                    AgentQueuedResearchSelection::Thorough
                }
                MessageResearchSelection::Command => AgentQueuedResearchSelection::Command,
            },
            AgentSessionText::try_from_string(message)
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
            timestamp()?,
            AgentQueuedMessageState::Queued,
        );
        let queue = self
            .store
            .enqueue_message(project, expected, &queued)
            .await?;
        lock_recovering_poison(&self.auto_dispatch_queues)
            .insert((project.worktree().id(), session_id));
        Ok(AgentMessageSubmission::Queued {
            detail: current,
            queue,
        })
    }

    pub(crate) async fn remove_queued_message(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        expected_queue_revision: AgentSessionQueueRevision,
        message_id: AgentQueuedMessageId,
    ) -> Result<AgentSessionQueue, AgentSessionManagerFailure> {
        let queue = self
            .store
            .transition_queued_message(
                project,
                session_id,
                expected_queue_revision,
                message_id,
                AgentQueuedMessageState::Removed,
            )
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        if queue.messages().is_empty() {
            lock_recovering_poison(&self.auto_dispatch_queues)
                .remove(&(project.worktree().id(), session_id));
        }
        Ok(queue)
    }

    pub(crate) async fn resume_queue(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        expected_queue_revision: AgentSessionQueueRevision,
    ) -> Result<AgentSessionQueue, AgentSessionManagerFailure> {
        self.store
            .set_message_queue_paused(project, session_id, expected_queue_revision, false)
            .await
            .map_err(AgentSessionManagerFailure::from)?;
        lock_recovering_poison(&self.auto_dispatch_queues)
            .insert((project.worktree().id(), session_id));
        self.dispatch_next_queued(project, QueueDispatchTrigger::ExplicitResume)
            .await?;
        self.store
            .load_message_queue(project, session_id)
            .await
            .map_err(Into::into)
    }

    async fn dispatch_next_queued(
        &self,
        project: &ProjectIdentity,
        trigger: QueueDispatchTrigger,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.release_terminal_job();
        if lock_recovering_poison(&self.active_session).is_some() {
            return Ok(());
        }
        let worktree_id = project.worktree().id();
        let session_ids = lock_recovering_poison(&self.auto_dispatch_queues)
            .iter()
            .filter_map(|(candidate_worktree, session_id)| {
                (*candidate_worktree == worktree_id).then_some(*session_id)
            })
            .collect::<Vec<_>>();
        let mut candidate = None;
        for session_id in session_ids {
            let Some(detail) = self
                .store
                .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                .await?
            else {
                lock_recovering_poison(&self.auto_dispatch_queues)
                    .remove(&(worktree_id, session_id));
                continue;
            };
            if !queue_dispatch_allows_state(trigger, detail.session().state()) {
                continue;
            }
            let queue = self.store.load_message_queue(project, session_id).await?;
            if queue.paused() || queue.messages().is_empty() {
                if queue.messages().is_empty() {
                    lock_recovering_poison(&self.auto_dispatch_queues)
                        .remove(&(worktree_id, session_id));
                }
                continue;
            }
            let message = queue.messages()[0].clone();
            let order = (
                message.enqueued_at().unix_millis(),
                message.ordinal(),
                message.id().as_bytes().to_owned(),
            );
            if candidate
                .as_ref()
                .is_none_or(|(current, _, _, _)| order < *current)
            {
                candidate = Some((order, detail, queue, message));
            }
        }
        let Some((_, detail, queue, message)) = candidate else {
            return Ok(());
        };
        let session_id = detail.session().id();
        self.store
            .transition_queued_message(
                project,
                session_id,
                queue.revision(),
                message.id(),
                AgentQueuedMessageState::Started,
            )
            .await?;
        let (depth, command_depth) = match message.research() {
            AgentQueuedResearchSelection::Standard => (Some(AgentResearchDepth::Standard), false),
            AgentQueuedResearchSelection::Thorough => (Some(AgentResearchDepth::Thorough), false),
            AgentQueuedResearchSelection::Command => (None, true),
        };
        let (effective_mode, _) =
            resolve_next_message_mode(Some(detail.session().mode()), message.target_mode());
        let submission = self
            .submit_for_target_mode(
                project,
                Some(session_id),
                Some(detail.session().revision()),
                effective_mode,
                depth,
                command_depth,
                message.text().as_str().to_owned(),
            )
            .await;
        if submission.is_err() {
            if let Ok(current) = self.store.load_message_queue(project, session_id).await
                && let Ok(requeued) = self
                    .store
                    .transition_queued_message(
                        project,
                        session_id,
                        current.revision(),
                        message.id(),
                        AgentQueuedMessageState::Queued,
                    )
                    .await
            {
                let _paused = self
                    .store
                    .set_message_queue_paused(project, session_id, requeued.revision(), true)
                    .await;
            }
            lock_recovering_poison(&self.auto_dispatch_queues).remove(&(worktree_id, session_id));
        }
        submission.map(|_| ())
    }

    pub(crate) async fn research_turns(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
    ) -> Result<a3_application::AskResearchTurnPage, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .list_turns(project, session_id, 32)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_detail(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> Result<Option<a3_application::AskResearchDetail>, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .load_detail(project, session_id, user_sequence)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_projection(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> Result<Option<a3_application::AskResearchProjection>, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .load_projection(project, session_id, user_sequence, 50)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_sources(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        after_ordinal: Option<u32>,
    ) -> Result<a3_application::AskResearchSourcePage, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .list_sources(project, session_id, user_sequence, after_ordinal, 50)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_source(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
        source_id: AskResearchSourceId,
    ) -> Result<Option<AskResearchSource>, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .load_source(project, session_id, user_sequence, source_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_diagrams(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        user_sequence: AgentSessionSequence,
    ) -> Result<Vec<a3_application::EvidenceDiagramArtifact>, AgentSessionManagerFailure> {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .list_diagrams(project, session_id, user_sequence)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn research_diagram(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        artifact_id: AgentDiagramArtifactId,
    ) -> Result<Option<(AgentSessionSequence, EvidenceDiagramArtifact)>, AgentSessionManagerFailure>
    {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .load_diagram(project, session_id, artifact_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn session_diagrams(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        user_turn_limit: u16,
    ) -> Result<Vec<a3_application::SessionEvidenceDiagramArtifact>, AgentSessionManagerFailure>
    {
        self.research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .list_session_diagrams(project, session_id, before_sequence, user_turn_limit)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn project_runtime_state(
        &self,
        project: &ProjectIdentity,
        task_id: TaskId,
        state: AgentSessionState,
    ) -> Result<(), AgentSessionManagerFailure> {
        let reporter = self
            .reporter
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let message = match state {
            AgentSessionState::Paused => {
                "Der Agentenlauf ist an einem dauerhaften Checkpoint pausiert."
            }
            AgentSessionState::Running => "Der Agentenlauf wird fortgesetzt.",
            _ => return Err(AgentSessionManagerFailure::InvalidInput),
        };
        reporter.report(project, task_id, state, message).await
    }

    /// Requests cooperative cancellation of the currently owned Ask/Plan preparation job.
    pub(crate) async fn cancel_conversation(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
    ) -> Result<(), AgentSessionManagerFailure> {
        self.release_terminal_job();
        let job_id = lock_recovering_poison(&self.active_session)
            .as_ref()
            .filter(|active| active.session_id == session_id)
            .and_then(|active| active.job_id);
        if let Some(job_id) = job_id {
            match self.submitter.cancel(job_id) {
                Ok(_) | Err(JobCancellationError::JobAlreadyFinished { .. }) => {}
                Err(
                    JobCancellationError::ShuttingDown | JobCancellationError::UnknownJob { .. },
                ) => return Err(AgentSessionManagerFailure::Unavailable),
            }
        }
        settle_unfinished_conversation(
            &self.store,
            project,
            session_id,
            ConversationTerminal::Cancelled,
        )
        .await?;
        let mut active = lock_recovering_poison(&self.active_session);
        let can_release = job_id.is_none_or(|job_id| {
            self.submitter
                .snapshot(job_id)
                .is_none_or(|snapshot| snapshot.status().is_terminal())
        });
        if can_release
            && active
                .as_ref()
                .is_some_and(|conversation| conversation.session_id == session_id)
        {
            *active = None;
        }
        Ok(())
    }

    pub(crate) async fn submit(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        message: String,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        self.submit_with_depth(
            project,
            session_id,
            expected_revision,
            start_mode,
            AgentResearchDepth::Standard,
            message,
        )
        .await
    }

    /// Submits one message with an explicit finite research profile.
    pub(crate) async fn submit_with_depth(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        depth: AgentResearchDepth,
        message: String,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        self.submit_with_selection(
            project,
            session_id,
            expected_revision,
            start_mode,
            None,
            MessageResearchSelection::LegacyDepth(depth),
            message,
        )
        .await
    }

    /// Submits one V3 message after resolving a command or explicit ordinary depth in the Core.
    pub(crate) async fn submit_with_command_selection(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        explicit_depth: Option<AgentResearchDepth>,
        message: String,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        let selection = explicit_depth.map_or(
            MessageResearchSelection::Command,
            MessageResearchSelection::ExplicitDepth,
        );
        self.submit_with_selection(
            project,
            session_id,
            expected_revision,
            start_mode,
            None,
            selection,
            message,
        )
        .await
    }

    /// Starts an independent work item in the explicitly selected capability envelope.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn submit_for_target_mode(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        target_mode: AgentSessionMode,
        explicit_depth: Option<AgentResearchDepth>,
        command_depth: bool,
        message: String,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        let selection = if command_depth {
            MessageResearchSelection::Command
        } else {
            MessageResearchSelection::ExplicitDepth(
                explicit_depth.ok_or(AgentSessionManagerFailure::InvalidInput)?,
            )
        };
        self.submit_with_selection(
            project,
            session_id,
            expected_revision,
            session_id.is_none().then_some(target_mode),
            Some(target_mode),
            selection,
            message,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn submit_with_selection(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        target_mode: Option<AgentSessionMode>,
        selection: MessageResearchSelection,
        message: String,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        self.submit_with_selection_from(
            project,
            session_id,
            expected_revision,
            start_mode,
            target_mode,
            selection,
            message,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn submit_with_selection_from(
        &self,
        project: &ProjectIdentity,
        session_id: Option<AgentSessionId>,
        expected_revision: Option<AgentSessionRevision>,
        start_mode: Option<AgentSessionMode>,
        target_mode: Option<AgentSessionMode>,
        selection: MessageResearchSelection,
        message: String,
        continuation_from: Option<AgentSessionSequence>,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        let generated_session_id = if session_id.is_none() {
            Some(AgentSessionId::from_bytes(random_id()?))
        } else {
            None
        };
        let operation_session_id = session_id
            .or(generated_session_id)
            .ok_or(AgentSessionManagerFailure::InvalidInput)?;
        let mut operation = self.acquire(operation_session_id)?;
        let now = timestamp()?;
        let (session, user_entry, depth, command_profile) = match session_id {
            Some(session_id) => {
                let expected = expected_revision.ok_or(AgentSessionManagerFailure::InvalidInput)?;
                if start_mode.is_some() {
                    return Err(AgentSessionManagerFailure::InvalidInput);
                }
                let current = self
                    .store
                    .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                    .await?
                    .ok_or(AgentSessionManagerFailure::NotFound)?;
                if current.session().revision() != expected
                    || !matches!(
                        current.session().state(),
                        AgentSessionState::Draft
                            | AgentSessionState::AwaitingUser
                            | AgentSessionState::AwaitingPlanReview
                            | AgentSessionState::Completed
                            | AgentSessionState::Failed
                            | AgentSessionState::Cancelled
                    )
                {
                    return Err(AgentSessionManagerFailure::Conflict);
                }
                let selected_mode = target_mode.unwrap_or(current.session().mode());
                let (effective_mode, _) =
                    resolve_next_message_mode(Some(current.session().mode()), selected_mode);
                let inherited_command = if selection == MessageResearchSelection::Command
                    || effective_mode != current.session().mode()
                {
                    None
                } else {
                    pending_clarification_command(self.store.as_ref(), project, &current, &message)
                        .await?
                };
                let (text, depth, command_profile) = inherited_command.map_or_else(
                    || resolve_submitted_message(effective_mode, selection, &message),
                    |profile| command_message(effective_mode, profile, &message),
                )?;
                let sequence = next_sequence(current.session().latest_sequence())?;
                let next = successor(
                    current.session(),
                    SessionSuccessor {
                        title: current.session().title().as_str().to_owned(),
                        mode: effective_mode,
                        state: AgentSessionState::Running,
                        updated_at: now,
                        latest_sequence: Some(sequence),
                        // Every user message starts an independent work item. Historical task and
                        // run links remain on their entries, but the operative sidebar must stay
                        // detached until this message materializes its own task.
                        active_work_item: None,
                        plan_revision: (effective_mode == current.session().mode())
                            .then(|| current.session().current_plan_revision())
                            .flatten(),
                        presentation_deleted: false,
                    },
                )?;
                let entry = AgentSessionEntry::try_new(
                    session_id,
                    sequence,
                    AgentSessionEntryKind::UserMessage,
                    text,
                    now,
                    None,
                    None,
                    None,
                )
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
                validate_agent_session_transition(current.session(), &next)?;
                self.store
                    .append_session_revision(
                        project,
                        expected,
                        &next,
                        Some(&entry),
                        command_profile
                            .as_ref()
                            .map(SlashCommandExecutionProfile::invocation),
                    )
                    .await?;
                (next, entry, depth, command_profile)
            }
            None => {
                if expected_revision.is_some() {
                    return Err(AgentSessionManagerFailure::InvalidInput);
                }
                let mode = start_mode.ok_or(AgentSessionManagerFailure::InvalidInput)?;
                let (text, depth, command_profile) =
                    resolve_submitted_message(mode, selection, &message)?;
                let session_id =
                    generated_session_id.ok_or(AgentSessionManagerFailure::InvalidInput)?;
                let session = AgentSession::from_parts(
                    session_id,
                    AgentSessionRevision::INITIAL,
                    title_from_message(text.as_str())?,
                    mode,
                    AgentSessionState::Running,
                    now,
                    now,
                    Some(AgentSessionSequence::FIRST),
                    None,
                    None,
                    false,
                );
                let entry = AgentSessionEntry::try_new(
                    session_id,
                    AgentSessionSequence::FIRST,
                    AgentSessionEntryKind::UserMessage,
                    text,
                    now,
                    None,
                    None,
                    None,
                )
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
                self.store
                    .create_session(
                        project,
                        &session,
                        Some(&entry),
                        command_profile
                            .as_ref()
                            .map(SlashCommandExecutionProfile::invocation),
                    )
                    .await?;
                (session, entry, depth, command_profile)
            }
        };
        let detail = self
            .store
            .load_session(project, session.id(), None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        let objective = command_profile.as_ref().map_or_else(
            || {
                if continuation_from.is_some() {
                    original_research_question(user_entry.text().as_str()).to_owned()
                } else {
                    user_entry.text().as_str().to_owned()
                }
            },
            |profile| profile.objective().to_owned(),
        );
        let transcript = detail
            .entries()
            .iter()
            .map(|entry| {
                let role = if entry.kind() == AgentSessionEntryKind::UserMessage {
                    ModelMessageRole::User
                } else {
                    ModelMessageRole::Assistant
                };
                let content = if entry.sequence() == user_entry.sequence() {
                    objective.clone()
                } else {
                    entry.text().as_str().to_owned()
                };
                (role, content)
            })
            .collect::<Vec<_>>();
        let job_id = match self.job_ids.allocate() {
            Ok(job_id) => job_id,
            Err(_) => {
                settle_unfinished_conversation(
                    &self.store,
                    project,
                    operation_session_id,
                    ConversationTerminal::Failed(
                        "Die lokale Laufzeit konnte keinen neuen Arbeitsauftrag anlegen. Starte A^3 neu und versuche es erneut.",
                    ),
                )
                .await?;
                return self
                    .store
                    .load_session(project, operation_session_id, None, SESSION_PAGE_LIMIT)
                    .await?
                    .ok_or(AgentSessionManagerFailure::NotFound);
            }
        };
        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let materializer = self.materializer.clone();
        let run_manager = self.run_manager.clone();
        let reporter = self.reporter.clone();
        let researcher = self.researcher.clone().map(|mut researcher| {
            researcher.continuation_from = continuation_from;
            researcher
        });
        let job_project = project.clone();
        let scheduled_session = session.clone();
        let user_sequence = user_entry.sequence();
        let queue_dependencies = self.dependencies();
        let queue_active_session = Arc::clone(&self.active_session);
        let queue_auto_dispatch = Arc::clone(&self.auto_dispatch_queues);
        let queue_project = project.clone();
        let scheduled = self.submitter.submit(
            job_id,
            AGENT_CONVERSATION_JOB_OWNER,
            move |context: JobContext| {
                let completion = tauri::async_runtime::block_on(complete_scheduled_session(
                    store,
                    runtime,
                    job_project,
                    scheduled_session,
                    user_sequence,
                    objective,
                    transcript,
                    materializer,
                    run_manager,
                    reporter,
                    researcher,
                    depth,
                    command_profile,
                    context,
                ));
                release_conversation_owner(&queue_active_session, operation_session_id);
                if completion == JobCompletion::Succeeded {
                    let dispatcher = AgentSessionManager::with_active_session(
                        queue_dependencies,
                        Arc::clone(&queue_active_session),
                        Arc::clone(&queue_auto_dispatch),
                    );
                    let _dispatched = tauri::async_runtime::block_on(
                        dispatcher
                            .dispatch_next_queued(&queue_project, QueueDispatchTrigger::Automatic),
                    );
                } else {
                    let dispatcher = AgentSessionManager::with_active_session(
                        queue_dependencies,
                        Arc::clone(&queue_active_session),
                        Arc::clone(&queue_auto_dispatch),
                    );
                    let _paused = tauri::async_runtime::block_on(async {
                        let queue = dispatcher
                            .store
                            .load_message_queue(&queue_project, operation_session_id)
                            .await?;
                        if queue.messages().is_empty() || queue.paused() {
                            return Ok::<(), AgentSessionStoreFailure>(());
                        }
                        dispatcher
                            .store
                            .set_message_queue_paused(
                                &queue_project,
                                operation_session_id,
                                queue.revision(),
                                true,
                            )
                            .await
                            .map(|_| ())
                    });
                    lock_recovering_poison(&dispatcher.auto_dispatch_queues)
                        .remove(&(queue_project.worktree().id(), operation_session_id));
                }
                completion
            },
        );
        if scheduled.is_err() {
            settle_unfinished_conversation(
                &self.store,
                project,
                operation_session_id,
                ConversationTerminal::Failed(
                    "Die lokale Laufzeit konnte die Verarbeitung nicht starten. Versuche es erneut, sobald der aktuelle Lauf beendet ist.",
                ),
            )
            .await?;
            return self
                .store
                .load_session(project, operation_session_id, None, SESSION_PAGE_LIMIT)
                .await?
                .ok_or(AgentSessionManagerFailure::NotFound);
        }
        operation.activate(job_id);
        Ok(detail)
    }

    /// Continues only the newest explicitly continuation-ready research turn.
    pub(crate) async fn continue_research(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        expected_revision: AgentSessionRevision,
        depth: AgentResearchDepth,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        let detail = self
            .store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if detail.session().revision() != expected_revision
            || detail.session().state() != AgentSessionState::AwaitingUser
        {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let user_entry = detail
            .entries()
            .iter()
            .rev()
            .find(|entry| entry.kind() == AgentSessionEntryKind::UserMessage)
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        let original_message = original_research_question(user_entry.text().as_str()).to_owned();
        let stored_command = self
            .store
            .load_session_commands(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .into_iter()
            .find(|command| command.sequence() == user_entry.sequence());
        let trace = self
            .research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .load_detail(project, session_id, user_entry.sequence())
            .await?
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        if !trace
            .events()
            .last()
            .is_some_and(|event| event.state() == AskResearchState::AwaitingContinuation)
        {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let selection = if let Some(stored_command) = stored_command {
            let _profile = restore_command_profile(
                detail.session().mode(),
                &original_message,
                &stored_command,
            )?;
            MessageResearchSelection::Command
        } else {
            MessageResearchSelection::LegacyDepth(depth)
        };
        let message = if selection == MessageResearchSelection::Command {
            original_message
        } else {
            format!("Recherche fortsetzen. Ursprüngliche Frage:\n{original_message}")
        };
        self.submit_with_selection_from(
            project,
            Some(session_id),
            Some(expected_revision),
            None,
            None,
            selection,
            message,
            Some(user_entry.sequence()),
        )
        .await
    }

    pub(crate) async fn update_presentation(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        expected: AgentSessionRevision,
        mutation: PresentationMutation,
    ) -> Result<Option<AgentSessionDetail>, AgentSessionManagerFailure> {
        self.release_terminal_job();
        if lock_recovering_poison(&self.active_session).is_some() {
            return Err(AgentSessionManagerFailure::Busy);
        }
        let detail = self
            .store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if detail.session().revision() != expected {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let pauses_queue = matches!(
            &mutation,
            PresentationMutation::Archive | PresentationMutation::Delete
        );
        if pauses_queue && !presentation_can_be_hidden(detail.session().state()) {
            return Err(AgentSessionManagerFailure::Busy);
        }
        let now = timestamp()?;
        let (title, mode, state, deleted) = match mutation {
            PresentationMutation::Rename(title) => (
                AgentSessionTitle::try_from_string(title)
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                detail.session().mode(),
                detail.session().state(),
                false,
            ),
            PresentationMutation::Archive => (
                detail.session().title().clone(),
                detail.session().mode(),
                AgentSessionState::Archived,
                false,
            ),
            PresentationMutation::Unarchive => {
                if detail.session().state() != AgentSessionState::Archived {
                    return Err(AgentSessionManagerFailure::InvalidInput);
                }
                (
                    detail.session().title().clone(),
                    detail.session().mode(),
                    AgentSessionState::Completed,
                    false,
                )
            }
            PresentationMutation::SwitchToPlan => {
                if detail.session().mode() != AgentSessionMode::Ask
                    || detail.session().state() == AgentSessionState::Archived
                {
                    return Err(AgentSessionManagerFailure::InvalidInput);
                }
                (
                    detail.session().title().clone(),
                    AgentSessionMode::Plan,
                    AgentSessionState::Completed,
                    false,
                )
            }
            PresentationMutation::Delete => (
                AgentSessionTitle::try_from_string("Deleted conversation".to_owned())
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                detail.session().mode(),
                AgentSessionState::Archived,
                true,
            ),
        };
        let next = successor(
            detail.session(),
            SessionSuccessor {
                title: title.as_str().to_owned(),
                mode,
                state,
                updated_at: now,
                latest_sequence: if deleted {
                    None
                } else {
                    detail.session().latest_sequence()
                },
                active_work_item: detail.session().active_work_item(),
                plan_revision: detail.session().current_plan_revision(),
                presentation_deleted: deleted,
            },
        )?;
        validate_agent_session_transition(detail.session(), &next)?;
        if deleted {
            self.store
                .delete_presentation(project, session_id, expected, &next)
                .await?;
            lock_recovering_poison(&self.auto_dispatch_queues)
                .remove(&(project.worktree().id(), session_id));
            Ok(None)
        } else {
            self.store
                .append_session_revision(project, expected, &next, None, None)
                .await?;
            if pauses_queue {
                lock_recovering_poison(&self.auto_dispatch_queues)
                    .remove(&(project.worktree().id(), session_id));
                if let Ok(queue) = self.store.load_message_queue(project, session_id).await
                    && !queue.messages().is_empty()
                    && !queue.paused()
                {
                    let _paused = self
                        .store
                        .set_message_queue_paused(project, session_id, queue.revision(), true)
                        .await;
                }
            }
            self.store
                .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                .await
                .map_err(Into::into)
        }
    }

    pub(crate) async fn implement_plan(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
        expected: AgentSessionRevision,
        plan_revision: u32,
    ) -> Result<AgentSessionDetail, AgentSessionManagerFailure> {
        let materializer = self
            .materializer
            .clone()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let run_manager = self
            .run_manager
            .clone()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let reporter = self
            .reporter
            .clone()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let research_store = self
            .research_store
            .clone()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let run_state = run_manager.activity().state();
        if agent_run_blocks_plan_start(run_state) {
            return Err(AgentSessionManagerFailure::Busy);
        }
        self.await_prior_conversation_release(session_id).await?;
        let mut operation = self.acquire(session_id)?;
        let detail = self
            .store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if detail.session().revision() != expected
            || detail.session().mode() != AgentSessionMode::Plan
            || detail.session().state() != AgentSessionState::AwaitingPlanReview
            || detail.session().current_plan_revision() != Some(plan_revision)
        {
            return Err(AgentSessionManagerFailure::Conflict);
        }
        let objective = detail
            .entries()
            .iter()
            .find(|entry| entry.kind() == AgentSessionEntryKind::UserMessage)
            .map(|entry| entry.text().as_str().to_owned())
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let plan_entry = detail
            .entries()
            .iter()
            .rev()
            .find(|entry| entry.plan_revision() == Some(plan_revision))
            .ok_or(AgentSessionManagerFailure::Conflict)?;
        let plan = plan_entry.text().as_str().to_owned();
        let triggering_user_entry = detail
            .entries()
            .iter()
            .rev()
            .find(|entry| {
                entry.sequence() < plan_entry.sequence()
                    && entry.kind() == AgentSessionEntryKind::UserMessage
            })
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let command_profile = self
            .store
            .load_session_commands(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .into_iter()
            .find(|command| command.sequence() == triggering_user_entry.sequence())
            .map(|command| {
                restore_command_profile(
                    AgentSessionMode::Plan,
                    triggering_user_entry.text().as_str(),
                    &command,
                )
            })
            .transpose()?;
        if let Some((_, handoff)) = latest_plan_research_handoff(
            research_store.as_ref(),
            project,
            session_id,
            command_profile.as_ref(),
        )
        .await?
        {
            let published = materializer
                .index
                .latest_published_index(project, &PlanStartIndexControl)
                .await
                .map_err(|_| AgentSessionManagerFailure::Unavailable)?
                .ok_or(AgentSessionManagerFailure::Unavailable)?;
            if !research_handoff_matches_index(&handoff, &published) {
                return Err(AgentSessionManagerFailure::IndexChanged);
            }
        }
        let objective = command_profile
            .as_ref()
            .map_or(objective, |profile| profile.objective().to_owned());
        let preparation_sequence = next_sequence(detail.session().latest_sequence())?;
        let preparation_started_at = timestamp()?;
        let running = successor(
            detail.session(),
            SessionSuccessor {
                title: detail.session().title().as_str().to_owned(),
                mode: AgentSessionMode::Agent,
                state: AgentSessionState::Running,
                updated_at: preparation_started_at,
                latest_sequence: Some(preparation_sequence),
                active_work_item: detail.session().active_work_item(),
                plan_revision: detail.session().current_plan_revision(),
                presentation_deleted: false,
            },
        )?;
        let preparation_entry = AgentSessionEntry::try_new(
            session_id,
            preparation_sequence,
            AgentSessionEntryKind::Activity,
            AgentSessionText::try_from_string(
                "Planfreigabe bestätigt. Projektstand und sichere Arbeitsanker werden geprüft."
                    .to_owned(),
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            preparation_started_at,
            None,
            None,
            None,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        validate_agent_session_transition(detail.session(), &running)?;
        self.store
            .append_session_revision(project, expected, &running, Some(&preparation_entry), None)
            .await?;
        let job_id = match self.job_ids.allocate() {
            Ok(job_id) => job_id,
            Err(_) => {
                settle_unfinished_conversation(
                    &self.store,
                    project,
                    session_id,
                    ConversationTerminal::Failed(
                        "Die lokale Laufzeit konnte keinen neuen Agentenlauf anlegen. Der geprüfte Plan bleibt in dieser Session erhalten.",
                    ),
                )
                .await?;
                return self
                    .store
                    .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                    .await?
                    .ok_or(AgentSessionManagerFailure::NotFound);
            }
        };
        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let job_project = project.clone();
        let scheduled_session = running.clone();
        let queue_dependencies = self.dependencies();
        let queue_active_session = Arc::clone(&self.active_session);
        let queue_auto_dispatch = Arc::clone(&self.auto_dispatch_queues);
        let queue_project = project.clone();
        let scheduled = self.submitter.submit(
            job_id,
            AGENT_CONVERSATION_JOB_OWNER,
            move |context: JobContext| {
                let completion = tauri::async_runtime::block_on(complete_plan_implementation(
                    store,
                    runtime,
                    materializer,
                    run_manager,
                    reporter,
                    research_store,
                    job_project,
                    scheduled_session,
                    objective,
                    plan,
                    command_profile,
                    context,
                ));
                release_conversation_owner(&queue_active_session, session_id);
                let dispatcher = AgentSessionManager::with_active_session(
                    queue_dependencies,
                    Arc::clone(&queue_active_session),
                    Arc::clone(&queue_auto_dispatch),
                );
                if completion == JobCompletion::Succeeded {
                    let _dispatched = tauri::async_runtime::block_on(
                        dispatcher
                            .dispatch_next_queued(&queue_project, QueueDispatchTrigger::Automatic),
                    );
                } else {
                    tauri::async_runtime::block_on(
                        dispatcher.pause_queue_after_failed_work(&queue_project, session_id),
                    );
                }
                completion
            },
        );
        if scheduled.is_err() {
            settle_unfinished_conversation(
                &self.store,
                project,
                session_id,
                ConversationTerminal::Failed(
                    "Die lokale Laufzeit konnte den Agentenlauf nicht starten. Der geprüfte Plan bleibt in dieser Session erhalten.",
                ),
            )
            .await?;
            return self
                .store
                .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                .await?
                .ok_or(AgentSessionManagerFailure::NotFound);
        }
        operation.activate(job_id);
        self.store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)
    }

    pub(crate) fn quiesce(&self) -> Result<(), AgentSessionManagerFailure> {
        // Auto-dispatch authority is process-local. A project switch or shutdown must never carry
        // it into a later activation; the durable queue remains visible and is paused on reload.
        lock_recovering_poison(&self.auto_dispatch_queues).clear();
        self.release_terminal_job();
        let job_id = lock_recovering_poison(&self.active_session)
            .as_ref()
            .and_then(|active| active.job_id);
        if let Some(job_id) = job_id {
            self.submitter
                .cancel(job_id)
                .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                if self
                    .submitter
                    .snapshot(job_id)
                    .is_none_or(|snapshot| snapshot.status().is_terminal())
                {
                    *lock_recovering_poison(&self.active_session) = None;
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            return Err(AgentSessionManagerFailure::Busy);
        }
        Ok(())
    }

    fn acquire(
        &self,
        session_id: AgentSessionId,
    ) -> Result<SessionPermit<'_>, AgentSessionManagerFailure> {
        self.release_terminal_job();
        let mut active = lock_recovering_poison(&self.active_session);
        if active.is_some() {
            return Err(AgentSessionManagerFailure::Busy);
        }
        *active = Some(ActiveConversation {
            session_id,
            job_id: None,
        });
        Ok(SessionPermit {
            active: &self.active_session,
            session_id,
            activated: false,
        })
    }

    fn release_terminal_job(&self) {
        let mut active = lock_recovering_poison(&self.active_session);
        let terminal = active
            .as_ref()
            .and_then(|conversation| conversation.job_id)
            .is_some_and(|job_id| {
                self.submitter
                    .snapshot(job_id)
                    .is_none_or(|snapshot| snapshot.status().is_terminal())
            });
        if terminal {
            *active = None;
        }
    }

    async fn await_prior_conversation_release(
        &self,
        session_id: AgentSessionId,
    ) -> Result<(), AgentSessionManagerFailure> {
        let deadline = Instant::now() + Duration::from_millis(750);
        loop {
            self.release_terminal_job();
            let active = *lock_recovering_poison(&self.active_session);
            match active {
                None => return Ok(()),
                Some(active) if active.session_id != session_id => {
                    return Err(AgentSessionManagerFailure::Busy);
                }
                Some(_) if Instant::now() >= deadline => {
                    return Err(AgentSessionManagerFailure::Busy);
                }
                Some(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    async fn pause_queue_after_failed_work(
        &self,
        project: &ProjectIdentity,
        session_id: AgentSessionId,
    ) {
        if let Ok(queue) = self.store.load_message_queue(project, session_id).await
            && !queue.messages().is_empty()
            && !queue.paused()
        {
            let _paused = self
                .store
                .set_message_queue_paused(project, session_id, queue.revision(), true)
                .await;
        }
        lock_recovering_poison(&self.auto_dispatch_queues)
            .remove(&(project.worktree().id(), session_id));
    }
}

#[allow(clippy::too_many_arguments)]
async fn complete_plan_implementation(
    store: Arc<dyn AgentSessionStore>,
    runtime: AgentConversationRuntime,
    materializer: AgentTaskMaterializer,
    run_manager: Arc<AgentRunManager>,
    reporter: Arc<AgentSessionRunReporter>,
    research_store: Arc<dyn AskResearchStore>,
    project: ProjectIdentity,
    session: AgentSession,
    objective: String,
    plan: String,
    command_profile: Option<SlashCommandExecutionProfile>,
    context: JobContext,
) -> JobCompletion {
    if context.cancellation_token().is_cancelled() {
        let _settled = settle_unfinished_conversation(
            &store,
            &project,
            session.id(),
            ConversationTerminal::Cancelled,
        )
        .await;
        return JobCompletion::Cancelled;
    }
    let recovery_store = Arc::clone(&store);
    let recovery_project = project.clone();
    let recovery_session_id = session.id();
    let result = async {
        let index_control = ConversationIndexControl { context: &context };
        let (_, profile) = runtime
            .execution_model()
            .await
            .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
        let prior_research = latest_plan_research_handoff(
            research_store.as_ref(),
            &project,
            session.id(),
            command_profile.as_ref(),
        )
        .await?;
        let linked_task_id = match prior_research.as_ref() {
            Some((user_sequence, _)) => {
                research_store
                    .load_linked_task(&project, session.id(), *user_sequence)
                    .await?
            }
            None => None,
        };
        let task = match (linked_task_id, prior_research.as_ref()) {
            (Some(task_id), Some((_, handoff))) => {
                materializer
                    .adopt_interrupted(
                        &project,
                        task_id,
                        profile.reference(),
                        handoff,
                        &index_control,
                    )
                    .await?
            }
            _ => {
                materializer
                    .materialize(AgentTaskMaterialization {
                        project: &project,
                        objective: &objective,
                        reviewed_plan: &plan,
                        profile,
                        research_handoff: prior_research.as_ref().map(|(_, handoff)| handoff),
                        verification_profile: command_profile
                            .as_ref()
                            .map(SlashCommandExecutionProfile::verification_profile),
                        control: &index_control,
                    })
                    .await?
            }
        };
        if linked_task_id.is_none()
            && let Some((user_sequence, _)) = prior_research
        {
            research_store
                .link_task_to_turn(
                    &project,
                    session.id(),
                    user_sequence,
                    task.work_item.task_id(),
                )
                .await?;
        }
        let sequence = next_sequence(session.latest_sequence())?;
        let now = timestamp()?;
        let linked = successor(
            &session,
            SessionSuccessor {
                title: session.title().as_str().to_owned(),
                mode: AgentSessionMode::Agent,
                state: AgentSessionState::Running,
                updated_at: now,
                latest_sequence: Some(sequence),
                active_work_item: Some(task.work_item),
                plan_revision: session.current_plan_revision(),
                presentation_deleted: false,
            },
        )?;
        let entry = AgentSessionEntry::try_new(
            session.id(),
            sequence,
            AgentSessionEntryKind::Activity,
            AgentSessionText::try_from_string(
                "Plan freigegeben. Der verifizierbare Agentenlauf wurde gestartet.".to_owned(),
            )
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            now,
            Some(task.work_item.id()),
            Some(task.work_item.task_id()),
            None,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        validate_agent_session_transition(&session, &linked)?;
        store
            .append_session_revision(&project, session.revision(), &linked, Some(&entry), None)
            .await?;
        reporter.link(task.work_item.task_id(), session.id());
        run_manager
            .start_attempt(task.request)
            .map_err(|_| AgentSessionManagerFailure::Busy)
    }
    .await;
    match result {
        Ok(()) => JobCompletion::Succeeded,
        Err(_) if context.cancellation_token().is_cancelled() => {
            let _settled = settle_unfinished_conversation(
                &recovery_store,
                &recovery_project,
                recovery_session_id,
                ConversationTerminal::Cancelled,
            )
            .await;
            JobCompletion::Cancelled
        }
        Err(error) => {
            let terminal = if error == AgentSessionManagerFailure::Conflict {
                ConversationTerminal::PlanNeedsRefresh
            } else {
                ConversationTerminal::Failed(safe_manager_failure_message(error))
            };
            let _settled = settle_unfinished_conversation(
                &recovery_store,
                &recovery_project,
                recovery_session_id,
                terminal,
            )
            .await;
            JobCompletion::Failed
        }
    }
}

async fn latest_plan_research_handoff(
    store: &dyn AskResearchStore,
    project: &ProjectIdentity,
    session_id: AgentSessionId,
    command_profile: Option<&SlashCommandExecutionProfile>,
) -> Result<Option<(AgentSessionSequence, ResearchHandoff)>, AgentSessionManagerFailure> {
    let turns = store.list_turns(project, session_id, 32).await?;
    let Some(detail) = turns.turns().iter().find(|detail| {
        detail.turn().mode() == AgentSessionMode::Plan
            && detail
                .events()
                .last()
                .is_some_and(|event| event.state() == AskResearchState::Completed)
    }) else {
        return Ok(None);
    };
    let mut after = None;
    let mut revisions = Vec::new();
    loop {
        let page = store
            .list_sources(
                project,
                session_id,
                detail.turn().user_sequence(),
                after,
                50,
            )
            .await?;
        for source in page.sources() {
            if !revisions.contains(source.revision()) {
                revisions.push(source.revision().clone());
            }
            after = Some(source.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    let mut handoff = ResearchHandoff::new(
        detail.turn().index_run_id(),
        detail.turn().snapshot_id(),
        revisions,
    )
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
    if let Some(work) = detail.work_state() {
        handoff = handoff.with_work_state(work.clone());
    }
    let handoff = match command_profile {
        Some(profile) => handoff.with_command(profile.invocation().clone()),
        None => handoff,
    };
    Ok(Some((detail.turn().user_sequence(), handoff)))
}

struct WorkingChangePaths {
    paths: Vec<Vec<u8>>,
    limited: bool,
}

fn inspect_working_change_paths(
    project: &ProjectIdentity,
    control: &JobContext,
) -> Result<WorkingChangePaths, AgentSessionManagerFailure> {
    let mut child = Command::new("git")
        .args([
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ])
        .current_dir(project.worktree().root().as_path())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(AgentSessionManagerFailure::Unavailable)?;
    let reader = thread::spawn(move || {
        read_bounded_process_output(stdout, WORKING_CHANGES_BYTE_LIMIT.saturating_add(1))
    });
    let started = Instant::now();
    let mut interrupted = false;
    let status = loop {
        if control.cancellation_token().is_cancelled()
            || started.elapsed() >= Duration::from_secs(30)
        {
            let _ignored = child.kill();
            interrupted = true;
            break child.wait().ok();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ignored = child.kill();
                interrupted = true;
                break child.wait().ok();
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
    if interrupted || !status.is_some_and(|status| status.success()) {
        return Err(AgentSessionManagerFailure::Unavailable);
    }
    Ok(parse_working_change_paths(&bytes))
}

fn read_bounded_process_output(
    mut reader: impl Read,
    retained_limit: u64,
) -> std::io::Result<Vec<u8>> {
    let capacity = usize::try_from(retained_limit).unwrap_or(usize::MAX);
    let mut retained = Vec::with_capacity(capacity.min(64 * 1_024));
    let mut buffer = [0_u8; 16 * 1_024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = capacity.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(retained)
}

fn parse_working_change_paths(bytes: &[u8]) -> WorkingChangePaths {
    let byte_limited = u64::try_from(bytes.len()).unwrap_or(u64::MAX) > WORKING_CHANGES_BYTE_LIMIT;
    let safe_length = usize::try_from(WORKING_CHANGES_BYTE_LIMIT)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let mut paths = BTreeSet::new();
    let fields = bytes[..safe_length]
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    let mut index = 0usize;
    while index < fields.len() && paths.len() <= WORKING_CHANGES_FILE_LIMIT {
        let record = fields[index];
        index = index.saturating_add(1);
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }
        let is_rename = matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C');
        let path = &record[3..];
        if !path.is_empty() {
            paths.insert(path.to_vec());
        }
        if is_rename {
            index = index.saturating_add(1);
        }
    }
    let file_limited = paths.len() > WORKING_CHANGES_FILE_LIMIT;
    WorkingChangePaths {
        paths: paths.into_iter().take(WORKING_CHANGES_FILE_LIMIT).collect(),
        limited: byte_limited || file_limited,
    }
}

fn security_rule_literals() -> Vec<String> {
    [
        "unsafe ",
        "Command::new",
        "std::process",
        "http://",
        "https://",
        "password",
        "secret",
        "api_key",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn resolve_submitted_message(
    mode: AgentSessionMode,
    selection: MessageResearchSelection,
    message: &str,
) -> Result<
    (
        AgentSessionText,
        AgentResearchDepth,
        Option<SlashCommandExecutionProfile>,
    ),
    AgentSessionManagerFailure,
> {
    match selection {
        MessageResearchSelection::LegacyDepth(depth) => Ok((
            AgentSessionText::try_from_string(message.to_owned())
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
            depth,
            None,
        )),
        MessageResearchSelection::ExplicitDepth(depth) => {
            let ParsedSlashCommand::Plain(text) = parse_slash_command(mode, message)
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?
            else {
                return Err(AgentSessionManagerFailure::InvalidInput);
            };
            Ok((
                AgentSessionText::try_from_string(text)
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                depth,
                None,
            ))
        }
        MessageResearchSelection::Command => {
            let ParsedSlashCommand::Command(invocation) = parse_slash_command(mode, message)
                .map_err(|_| AgentSessionManagerFailure::InvalidInput)?
            else {
                return Err(AgentSessionManagerFailure::InvalidInput);
            };
            let profile = SlashCommandExecutionProfile::resolve(invocation);
            let depth = profile.depth();
            Ok((
                AgentSessionText::try_from_string(message.trim().to_owned())
                    .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
                depth,
                Some(profile),
            ))
        }
    }
}

async fn pending_clarification_command(
    store: &dyn AgentSessionStore,
    project: &ProjectIdentity,
    current: &AgentSessionDetail,
    subject: &str,
) -> Result<Option<SlashCommandExecutionProfile>, AgentSessionManagerFailure> {
    if current.session().state() != AgentSessionState::AwaitingUser || subject.trim().is_empty() {
        return Ok(None);
    }
    let Some(latest_sequence) = current.session().latest_sequence() else {
        return Ok(None);
    };
    let Some(latest_entry) = current
        .entries()
        .iter()
        .find(|entry| entry.sequence() == latest_sequence)
    else {
        return Ok(None);
    };
    if latest_entry.kind() != AgentSessionEntryKind::AssistantSummary {
        return Ok(None);
    }
    let Some(stored) = store
        .load_session_commands(project, current.session().id(), None, 1)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    if stored.sequence().next().ok() != Some(latest_sequence) {
        return Ok(None);
    }
    let Some(entry) = current
        .entries()
        .iter()
        .find(|entry| entry.sequence() == stored.sequence())
    else {
        return Ok(None);
    };
    let profile =
        restore_command_profile(current.session().mode(), entry.text().as_str(), &stored)?;
    if !profile.invocation().subject().is_empty()
        || profile.invocation().empty_input_behavior() != SlashCommandEmptyInput::Clarify
    {
        return Ok(None);
    }
    Ok(Some(profile))
}

fn command_message(
    mode: AgentSessionMode,
    profile: SlashCommandExecutionProfile,
    subject: &str,
) -> Result<
    (
        AgentSessionText,
        AgentResearchDepth,
        Option<SlashCommandExecutionProfile>,
    ),
    AgentSessionManagerFailure,
> {
    let mut message = format!("/{}", profile.invocation().primary().name());
    for lens in profile.invocation().lenses() {
        message.push_str(&format!(" /{}", lens.name()));
    }
    message.push(' ');
    message.push_str(subject.trim());
    let ParsedSlashCommand::Command(invocation) = parse_slash_command(mode, &message)
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)?
    else {
        return Err(AgentSessionManagerFailure::InvalidInput);
    };
    let profile = SlashCommandExecutionProfile::resolve(invocation);
    let depth = profile.depth();
    Ok((
        AgentSessionText::try_from_string(message)
            .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
        depth,
        Some(profile),
    ))
}

async fn complete_command_clarification(
    store: &dyn AgentSessionStore,
    project: &ProjectIdentity,
    session: &AgentSession,
    user_sequence: AgentSessionSequence,
    command: SlashCommand,
    context: &JobContext,
) -> Result<JobCompletion, AgentSessionManagerFailure> {
    report_progress(context, 0)?;
    let sequence = user_sequence
        .next()
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
    let completed_at = timestamp()?;
    let next = successor(
        session,
        SessionSuccessor {
            title: session.title().as_str().to_owned(),
            mode: session.mode(),
            state: AgentSessionState::AwaitingUser,
            updated_at: completed_at,
            latest_sequence: Some(sequence),
            active_work_item: session.active_work_item(),
            plan_revision: session.current_plan_revision(),
            presentation_deleted: false,
        },
    )?;
    let entry = AgentSessionEntry::try_new(
        session.id(),
        sequence,
        AgentSessionEntryKind::AssistantSummary,
        AgentSessionText::try_from_string(command_clarification_question(command).to_owned())
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
        completed_at,
        None,
        None,
        None,
    )
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
    validate_agent_session_transition(session, &next)?;
    store
        .append_session_revision(project, session.revision(), &next, Some(&entry), None)
        .await?;
    report_progress(context, CONVERSATION_PROGRESS_TOTAL)?;
    Ok(JobCompletion::Succeeded)
}

const fn command_clarification_question(command: SlashCommand) -> &'static str {
    match command {
        SlashCommand::Debug => {
            "Welchen konkreten Fehler oder welches beobachtete Verhalten soll A^3 untersuchen?"
        }
        SlashCommand::Doc => {
            "Welche Dokumentation oder welcher Codebereich soll erstellt beziehungsweise aktualisiert werden?"
        }
        SlashCommand::Refactor => {
            "Welcher konkrete Codebereich soll verhaltenserhaltend überarbeitet werden?"
        }
        SlashCommand::Test => {
            "Welches Verhalten oder welcher Codebereich soll durch Tests abgedeckt werden?"
        }
        _ => "Welches konkrete Ziel soll A^3 bearbeiten?",
    }
}

fn restore_command_profile(
    mode: AgentSessionMode,
    message: &str,
    stored: &a3_application::AgentSessionCommandPresentation,
) -> Result<SlashCommandExecutionProfile, AgentSessionManagerFailure> {
    let ParsedSlashCommand::Command(invocation) = parse_slash_command(mode, message)
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?
    else {
        return Err(AgentSessionManagerFailure::InvalidOutput);
    };
    if invocation.primary() != stored.primary()
        || invocation.lenses() != stored.lenses()
        || invocation.depth() != stored.depth()
    {
        return Err(AgentSessionManagerFailure::InvalidOutput);
    }
    Ok(SlashCommandExecutionProfile::resolve(invocation))
}

#[allow(clippy::too_many_arguments)]
async fn complete_scheduled_session(
    store: Arc<dyn AgentSessionStore>,
    runtime: AgentConversationRuntime,
    project: ProjectIdentity,
    session: AgentSession,
    user_sequence: AgentSessionSequence,
    objective: String,
    transcript: Vec<(ModelMessageRole, String)>,
    materializer: Option<AgentTaskMaterializer>,
    run_manager: Option<Arc<AgentRunManager>>,
    reporter: Option<Arc<AgentSessionRunReporter>>,
    researcher: Option<AgentAskResearcher>,
    depth: AgentResearchDepth,
    command_profile: Option<SlashCommandExecutionProfile>,
    context: JobContext,
) -> JobCompletion {
    let recovery_store = Arc::clone(&store);
    let recovery_project = project.clone();
    let recovery_session_id = session.id();
    match complete_scheduled_session_inner(
        store,
        runtime,
        project,
        session,
        user_sequence,
        objective,
        transcript,
        materializer,
        run_manager,
        reporter,
        researcher,
        depth,
        command_profile,
        &context,
    )
    .await
    {
        Ok(completion) => completion,
        Err(error) => {
            let terminal = if context.cancellation_token().is_cancelled() {
                ConversationTerminal::Cancelled
            } else {
                ConversationTerminal::Failed(safe_manager_failure_message(error))
            };
            let completion = if terminal == ConversationTerminal::Cancelled {
                JobCompletion::Cancelled
            } else {
                JobCompletion::Failed
            };
            let _settled = settle_unfinished_conversation(
                &recovery_store,
                &recovery_project,
                recovery_session_id,
                terminal,
            )
            .await;
            completion
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn complete_scheduled_session_inner(
    store: Arc<dyn AgentSessionStore>,
    runtime: AgentConversationRuntime,
    project: ProjectIdentity,
    session: AgentSession,
    user_sequence: AgentSessionSequence,
    objective: String,
    transcript: Vec<(ModelMessageRole, String)>,
    materializer: Option<AgentTaskMaterializer>,
    run_manager: Option<Arc<AgentRunManager>>,
    reporter: Option<Arc<AgentSessionRunReporter>>,
    researcher: Option<AgentAskResearcher>,
    depth: AgentResearchDepth,
    command_profile: Option<SlashCommandExecutionProfile>,
    context: &JobContext,
) -> Result<JobCompletion, AgentSessionManagerFailure> {
    if context.cancellation_token().is_cancelled() {
        return Err(AgentSessionManagerFailure::Unavailable);
    }
    if let Some(profile) = command_profile.as_ref()
        && profile.invocation().subject().is_empty()
        && profile.invocation().empty_input_behavior() == SlashCommandEmptyInput::Clarify
    {
        return complete_command_clarification(
            store.as_ref(),
            &project,
            &session,
            user_sequence,
            profile.invocation().primary(),
            context,
        )
        .await;
    }
    report_progress(context, 0)?;
    let index_control = ConversationIndexControl { context };
    let research_store = researcher.as_ref().map(|value| Arc::clone(&value.trace));
    let research_result = Some(
        researcher
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?
            .research(
                &runtime,
                &project,
                session.id(),
                user_sequence,
                session.mode(),
                depth,
                &objective,
                &transcript,
                command_profile.as_ref(),
                context,
            )
            .await?,
    );
    report_progress(context, 8)?;
    let output = if context.cancellation_token().is_cancelled() {
        Err(AgentConversationFailure::Unavailable)
    } else if let Some(result) = research_result.as_ref() {
        Ok(result.markdown.clone())
    } else {
        runtime.complete(session.mode(), &transcript, context).await
    };
    report_progress(context, 9)?;
    let cancelled = context.cancellation_token().is_cancelled();
    let has_research_citations = research_result
        .as_ref()
        .is_some_and(|result| !result.citations.is_empty());
    let completed_at = timestamp()?;
    let sequence = user_sequence
        .next()
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
    let (state, kind, plan_revision, content, completion, work_item, run_start) = if cancelled {
        (
            AgentSessionState::Cancelled,
            AgentSessionEntryKind::FinalReport,
            session.current_plan_revision(),
            "Die aktuelle Verarbeitung wurde abgebrochen.".to_owned(),
            JobCompletion::Cancelled,
            None,
            None,
        )
    } else {
        match output {
            Ok(content)
                if research_result
                    .as_ref()
                    .is_some_and(|result| result.awaiting_continuation) =>
            {
                (
                    AgentSessionState::AwaitingUser,
                    AgentSessionEntryKind::AssistantSummary,
                    session.current_plan_revision(),
                    content,
                    JobCompletion::Succeeded,
                    None,
                    None,
                )
            }
            Ok(content) => match session.mode() {
                AgentSessionMode::Ask => (
                    AgentSessionState::Completed,
                    AgentSessionEntryKind::FinalReport,
                    session.current_plan_revision(),
                    content,
                    JobCompletion::Succeeded,
                    None,
                    None,
                ),
                AgentSessionMode::Plan => {
                    let (state, kind, revision, content) = plan_session_outcome(&session, &content, has_research_citations);
                    (state, kind, revision, content, JobCompletion::Succeeded, None, None)
                },
                AgentSessionMode::Agent => match classify_plan_response(&content) {
                    PlanConversationResponse::Question(question) => (
                        AgentSessionState::AwaitingUser,
                        AgentSessionEntryKind::AssistantSummary,
                        session.current_plan_revision(),
                        question,
                        JobCompletion::Succeeded,
                        None,
                        None,
                    ),
                    PlanConversationResponse::Plan(_) if !has_research_citations => (
                        AgentSessionState::AwaitingUser,
                        AgentSessionEntryKind::AssistantSummary,
                        session.current_plan_revision(),
                        "Die Vorbereitung ist noch nicht durch aktuelle Quellen belegt. Ergänze bitte den fehlenden Schwerpunkt oder setze die Recherche mit neuem Budget fort."
                            .to_owned(),
                        JobCompletion::Succeeded,
                        None,
                        None,
                    ),
                    PlanConversationResponse::Plan(plan) => {
                        let plan_revision = session
                            .current_plan_revision()
                            .unwrap_or(0)
                            .saturating_add(1);
                        let task = match (materializer, run_manager.as_ref()) {
                            (Some(materializer), Some(_)) => {
                                let (_, profile) = runtime
                                    .execution_model()
                                    .await
                                    .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
                                Some(
                                    materializer
                                        .materialize(AgentTaskMaterialization {
                                            project: &project,
                                            objective: &objective,
                                            reviewed_plan: &plan,
                                            profile,
                                            research_handoff: research_result
                                                .as_ref()
                                                .map(|result| &result.handoff),
                                            verification_profile: command_profile.as_ref().map(
                                                SlashCommandExecutionProfile::verification_profile,
                                            ),
                                            control: &index_control,
                                        })
                                        .await?,
                                )
                            }
                            _ => None,
                        };
                        match task {
                            Some(task) => {
                                let manager = run_manager
                                    .as_ref()
                                    .cloned()
                                    .ok_or(AgentSessionManagerFailure::Unavailable)?;
                                (
                                    AgentSessionState::Running,
                                    AgentSessionEntryKind::Plan,
                                    Some(plan_revision),
                                    plan,
                                    JobCompletion::Succeeded,
                                    Some(task.work_item),
                                    Some((manager, task.request)),
                                )
                            }
                            None => (
                                AgentSessionState::Failed,
                                AgentSessionEntryKind::FinalReport,
                                session.current_plan_revision(),
                                "Die deterministische Agent-Laufzeit ist derzeit nicht verfügbar."
                                    .to_owned(),
                                JobCompletion::Failed,
                                None,
                                None,
                            ),
                        }
                    }
                },
            },
            Err(error) => (
                AgentSessionState::Failed,
                AgentSessionEntryKind::FinalReport,
                session.current_plan_revision(),
                safe_failure_message(error).to_owned(),
                JobCompletion::Failed,
                None,
                None,
            ),
        }
    };
    let final_session = successor(
        &session,
        SessionSuccessor {
            title: session.title().as_str().to_owned(),
            mode: session.mode(),
            state,
            updated_at: completed_at,
            latest_sequence: Some(sequence),
            active_work_item: work_item.or(session.active_work_item()),
            plan_revision,
            presentation_deleted: false,
        },
    )?;
    let assistant_entry = AgentSessionEntry::try_new(
        session.id(),
        sequence,
        kind,
        AgentSessionText::try_from_string(content)
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
        completed_at,
        work_item.map(AgentWorkItem::id),
        work_item.map(AgentWorkItem::task_id),
        plan_revision.filter(|_| kind == AgentSessionEntryKind::Plan),
    )
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
    validate_agent_session_transition(&session, &final_session)?;
    if completion == JobCompletion::Succeeded {
        let research = research_store
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        let result = research_result
            .as_ref()
            .ok_or(AgentSessionManagerFailure::Unavailable)?;
        if let Err(error) = research
            .complete_turn(
                &project,
                session.revision(),
                &final_session,
                &assistant_entry,
                &result.terminal_event,
                &result.citations,
                &result.diagrams,
            )
            .await
        {
            if let Ok(event) = research_event(
                session.id(),
                user_sequence,
                result.terminal_event.sequence(),
                AskResearchPhase::Completed,
                AskResearchState::Failed,
                "Antwort konnte nicht atomar veröffentlicht werden; Recherche bleibt sichtbar",
                None,
                AskResearchCompleteness::Limited,
            ) {
                let _ignored = research.append_event(&project, &event).await;
            }
            return Err(error.into());
        }
    } else {
        if let (Some(research), Some(result)) = (research_store.as_ref(), research_result.as_ref())
        {
            let event = research_event(
                session.id(),
                user_sequence,
                result.terminal_event.sequence(),
                AskResearchPhase::Completed,
                if cancelled {
                    AskResearchState::Cancelled
                } else {
                    AskResearchState::Failed
                },
                if cancelled {
                    "Recherche abgebrochen; bereits gefundene Quellen bleiben sichtbar"
                } else {
                    "Antwort konnte nicht abgeschlossen werden; Recherche bleibt sichtbar"
                },
                None,
                AskResearchCompleteness::Limited,
            )?;
            research.append_event(&project, &event).await?;
        }
        store
            .append_session_revision(
                &project,
                session.revision(),
                &final_session,
                Some(&assistant_entry),
                None,
            )
            .await?;
    }
    if let (Some(work_item), Some((manager, request))) = (work_item, run_start) {
        let reporter = reporter.ok_or(AgentSessionManagerFailure::Unavailable)?;
        reporter.link(work_item.task_id(), session.id());
        manager
            .start_attempt(request)
            .map_err(|_| AgentSessionManagerFailure::Busy)?;
    }
    let _reported = report_progress(context, CONVERSATION_PROGRESS_TOTAL);
    Ok(completion)
}

fn report_progress(context: &JobContext, completed: u64) -> Result<(), AgentSessionManagerFailure> {
    let progress = Progress::determinate(completed, CONVERSATION_PROGRESS_TOTAL)
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?;
    context
        .report_progress(progress)
        .map_err(|_| AgentSessionManagerFailure::Unavailable)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationTerminal {
    Cancelled,
    Failed(&'static str),
    PlanNeedsRefresh,
    AgentStartInterrupted,
}

async fn settle_unfinished_conversation(
    store: &Arc<dyn AgentSessionStore>,
    project: &ProjectIdentity,
    session_id: AgentSessionId,
    terminal: ConversationTerminal,
) -> Result<(), AgentSessionManagerFailure> {
    for _attempt in 0..2 {
        let detail = store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await?
            .ok_or(AgentSessionManagerFailure::NotFound)?;
        if detail.session().state() != AgentSessionState::Running {
            return Ok(());
        }
        let (mode, state, kind, message) = match terminal {
            ConversationTerminal::Cancelled => (
                detail.session().mode(),
                AgentSessionState::Cancelled,
                AgentSessionEntryKind::FinalReport,
                "Die aktuelle Verarbeitung wurde abgebrochen.",
            ),
            ConversationTerminal::Failed(message) => (
                detail.session().mode(),
                AgentSessionState::Failed,
                AgentSessionEntryKind::FinalReport,
                message,
            ),
            ConversationTerminal::PlanNeedsRefresh => (
                detail.session().mode(),
                AgentSessionState::AwaitingUser,
                AgentSessionEntryKind::AssistantSummary,
                "Der Projektstand hat sich seit der Planrecherche geändert. Der bisherige Plan bleibt sichtbar, muss aber mit aktuellen Quellen neu geprüft werden.",
            ),
            ConversationTerminal::AgentStartInterrupted => (
                AgentSessionMode::Plan,
                AgentSessionState::AwaitingPlanReview,
                AgentSessionEntryKind::AssistantSummary,
                "Der Agentenstart wurde unterbrochen, bevor der Lauf sicher mit der Session verbunden war. Der geprüfte Plan bleibt erhalten und kann erneut gestartet werden.",
            ),
        };
        let sequence = next_sequence(detail.session().latest_sequence())?;
        let completed_at = timestamp()?;
        let next = successor(
            detail.session(),
            SessionSuccessor {
                title: detail.session().title().as_str().to_owned(),
                mode,
                state,
                updated_at: completed_at,
                latest_sequence: Some(sequence),
                active_work_item: detail.session().active_work_item(),
                plan_revision: detail.session().current_plan_revision(),
                presentation_deleted: false,
            },
        )?;
        let entry = AgentSessionEntry::try_new(
            session_id,
            sequence,
            kind,
            AgentSessionText::try_from_string(message.to_owned())
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?,
            completed_at,
            detail.session().active_work_item().map(AgentWorkItem::id),
            detail
                .session()
                .active_work_item()
                .map(AgentWorkItem::task_id),
            None,
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        validate_agent_session_transition(detail.session(), &next)?;
        match store
            .append_session_revision(
                project,
                detail.session().revision(),
                &next,
                Some(&entry),
                None,
            )
            .await
        {
            Ok(()) => return Ok(()),
            Err(AgentSessionStoreFailure::Conflict) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let latest = store
        .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
        .await?
        .ok_or(AgentSessionManagerFailure::NotFound)?;
    if latest.session().state() == AgentSessionState::Running {
        Err(AgentSessionManagerFailure::Conflict)
    } else {
        Ok(())
    }
}

struct SessionSuccessor {
    title: String,
    mode: AgentSessionMode,
    state: AgentSessionState,
    updated_at: AgentSessionTimestamp,
    latest_sequence: Option<AgentSessionSequence>,
    active_work_item: Option<AgentWorkItem>,
    plan_revision: Option<u32>,
    presentation_deleted: bool,
}

fn successor(
    current: &AgentSession,
    successor: SessionSuccessor,
) -> Result<AgentSession, AgentSessionManagerFailure> {
    Ok(AgentSession::from_parts(
        current.id(),
        current
            .revision()
            .next()
            .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
        AgentSessionTitle::try_from_string(successor.title)
            .map_err(|_| AgentSessionManagerFailure::InvalidInput)?,
        successor.mode,
        successor.state,
        current.created_at(),
        successor.updated_at,
        successor.latest_sequence,
        successor.active_work_item,
        successor.plan_revision,
        successor.presentation_deleted,
    ))
}

fn next_sequence(
    current: Option<AgentSessionSequence>,
) -> Result<AgentSessionSequence, AgentSessionManagerFailure> {
    current
        .map(AgentSessionSequence::next)
        .transpose()
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)
        .map(|value| value.unwrap_or(AgentSessionSequence::FIRST))
}

const fn presentation_can_be_hidden(state: AgentSessionState) -> bool {
    !matches!(
        state,
        AgentSessionState::Running
            | AgentSessionState::AwaitingApproval
            | AgentSessionState::Paused
    )
}

const fn resolve_next_message_mode(
    current_mode: Option<AgentSessionMode>,
    selected_mode: AgentSessionMode,
) -> (AgentSessionMode, bool) {
    if matches!(selected_mode, AgentSessionMode::Agent)
        && !matches!(current_mode, Some(AgentSessionMode::Agent))
    {
        (AgentSessionMode::Plan, true)
    } else {
        (selected_mode, false)
    }
}

const fn agent_run_blocks_plan_start(state: AgentRunActivityState) -> bool {
    state.owns_live_worker() || matches!(state, AgentRunActivityState::Paused)
}

const fn queue_dispatch_allows_state(
    trigger: QueueDispatchTrigger,
    state: AgentSessionState,
) -> bool {
    match trigger {
        QueueDispatchTrigger::Automatic => matches!(state, AgentSessionState::Completed),
        QueueDispatchTrigger::ExplicitResume => matches!(
            state,
            AgentSessionState::Completed | AgentSessionState::Failed | AgentSessionState::Cancelled
        ),
    }
}

fn research_handoff_matches_index(
    handoff: &ResearchHandoff,
    published: &a3_domain::PublishedIndex,
) -> bool {
    handoff.index_run_id() == published.run().id()
        && handoff.snapshot_id() == published.run().snapshot_id()
        && handoff.revisions().iter().all(|revision| {
            published
                .publication()
                .graph()
                .files()
                .iter()
                .any(|current| current == revision)
        })
}

fn title_from_message(message: &str) -> Result<AgentSessionTitle, AgentSessionManagerFailure> {
    let line = message.lines().next().unwrap_or("New conversation").trim();
    let mut title = String::new();
    for character in line.chars() {
        if title.len().saturating_add(character.len_utf8()) > 112 {
            break;
        }
        title.push(character);
    }
    if line.len() > title.len() {
        title.push('…');
    }
    AgentSessionTitle::try_from_string(title).map_err(|_| AgentSessionManagerFailure::InvalidInput)
}

fn timestamp() -> Result<AgentSessionTimestamp, AgentSessionManagerFailure> {
    AgentSessionTimestamp::from_unix_millis(now_millis()?)
        .map_err(|_| AgentSessionManagerFailure::Unavailable)
}

fn now_millis() -> Result<u64, AgentSessionManagerFailure> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AgentSessionManagerFailure::Unavailable)?
        .as_millis();
    u64::try_from(millis).map_err(|_| AgentSessionManagerFailure::Unavailable)
}

fn agent_timestamp(value: u64) -> Result<AgentRunTimestamp, AgentSessionManagerFailure> {
    AgentRunTimestamp::from_unix_millis(value).map_err(|_| AgentSessionManagerFailure::Unavailable)
}

fn verification_requirement(
    value: &str,
) -> Result<VerificationRequirement, AgentSessionManagerFailure> {
    VerificationRequirement::try_from_string(value.to_owned())
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)
}

const fn verification_command_order(
    profile: SlashCommandVerificationProfile,
) -> &'static [DiscoveredCommandKind] {
    match profile {
        SlashCommandVerificationProfile::EvidenceOnly
        | SlashCommandVerificationProfile::Review
        | SlashCommandVerificationProfile::BehaviorPreservation => &[
            DiscoveredCommandKind::Test,
            DiscoveredCommandKind::Lint,
            DiscoveredCommandKind::Build,
            DiscoveredCommandKind::Format,
        ],
        SlashCommandVerificationProfile::Repair | SlashCommandVerificationProfile::Tests => &[
            DiscoveredCommandKind::Test,
            DiscoveredCommandKind::Build,
            DiscoveredCommandKind::Lint,
            DiscoveredCommandKind::Format,
        ],
        SlashCommandVerificationProfile::Documentation => &[
            DiscoveredCommandKind::Lint,
            DiscoveredCommandKind::Build,
            DiscoveredCommandKind::Test,
            DiscoveredCommandKind::Format,
        ],
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    let value = value.trim();
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let suffix = "…";
    let mut end = max_bytes.saturating_sub(suffix.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}{suffix}", &value[..end])
}

fn utf8_prefix(value: &str, maximum_bytes: usize) -> &str {
    let mut end = value.len().min(maximum_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &value[..end]
}

fn safe_path(path: &a3_domain::RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes()).into_owned()
}

fn model_safe_path(path: &a3_domain::RepositoryPath) -> String {
    String::from_utf8_lossy(path.as_bytes())
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn research_event(
    session_id: AgentSessionId,
    user_sequence: AgentSessionSequence,
    sequence: u32,
    phase: AskResearchPhase,
    state: AskResearchState,
    action: &str,
    query: Option<&str>,
    completeness: AskResearchCompleteness,
) -> Result<AskResearchEvent, AgentSessionManagerFailure> {
    AskResearchEvent::new(
        session_id,
        user_sequence,
        sequence,
        phase,
        state,
        bounded_text(action, 512),
        visible_research_query(query),
        completeness,
        timestamp()?,
    )
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)
}

fn visible_research_query(query: Option<&str>) -> Option<String> {
    query
        .filter(|value| SecretCandidateClassifierV1::classify(value).is_none())
        .map(|value| bounded_text(value, 4 * 1024))
}

fn public_note(
    note: &a3_application::AskResearchDecisionNote,
    state: &AskResearchWorkingSet,
) -> Result<AskResearchPublicNote, AgentSessionManagerFailure> {
    let source_ids = note
        .source_ordinals
        .iter()
        .map(|ordinal| {
            state
                .sources
                .get(usize::from(ordinal.saturating_sub(1)))
                .map(AskResearchSource::id)
                .ok_or(AgentSessionManagerFailure::InvalidOutput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    AskResearchPublicNote::new(
        note.goal.clone(),
        match note.finding_kind {
            a3_application::AskResearchFindingKind::Observation => {
                AskResearchPublicFindingKind::Observation
            }
            a3_application::AskResearchFindingKind::Hypothesis => {
                AskResearchPublicFindingKind::Hypothesis
            }
            a3_application::AskResearchFindingKind::Conclusion => {
                AskResearchPublicFindingKind::Conclusion
            }
        },
        note.finding.clone(),
        source_ids,
        note.gap.clone(),
        note.next_step.clone(),
    )
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)
}

#[allow(clippy::too_many_arguments)]
async fn compile_diagram_artifacts(
    runtime: &impl ResearchModel,
    command_profile: Option<&SlashCommandExecutionProfile>,
    transcript: &mut Vec<(ModelMessageRole, String)>,
    state: &AskResearchWorkingSet,
    controller: &mut BoundedResearchController,
    started: Instant,
    control: &JobContext,
    project: &ProjectIdentity,
) -> Result<DiagramCompilationOutcome, AgentSessionManagerFailure> {
    if command_profile.is_none_or(|profile| profile.invocation().primary() != SlashCommand::Diagram)
    {
        return Ok(DiagramCompilationOutcome::complete(Vec::new(), Vec::new()));
    }
    let base_transcript = transcript.clone();
    let mut diagnostics = Vec::new();
    loop {
        if controller
            .begin_diagram_decision(elapsed_millis(started))
            .is_err()
        {
            return Ok(DiagramCompilationOutcome::incomplete(diagnostics));
        }
        diagnostics.push(format!(
            "diagram-v1/attempt; phase=diagram; model={}/2; research={}/{}",
            controller.diagram_decisions_used(),
            controller.decisions_used(),
            controller.limits().model_decisions()
        ));
        state.evidence_guard(project).validate(control).await?;
        let remaining = controller
            .limits()
            .duration_millis()
            .saturating_sub(elapsed_millis(started));
        if remaining == 0 {
            return Ok(DiagramCompilationOutcome::incomplete(diagnostics));
        }
        let raw = match tokio::time::timeout(
            Duration::from_millis(remaining),
            runtime.complete_evidence_diagrams(transcript, control),
        )
        .await
        {
            Ok(Ok(raw)) => raw,
            Ok(Err(error))
                if is_transient_conversation_failure(error)
                    || matches!(
                        error,
                        AgentConversationFailure::InvalidOutput
                            | AgentConversationFailure::OutputTruncated
                            | AgentConversationFailure::OutputTooLarge
                    ) =>
            {
                diagnostics.push(
                    if matches!(
                        error,
                        AgentConversationFailure::OutputTruncated
                            | AgentConversationFailure::OutputTooLarge
                    ) {
                        "diagram-v1/output-truncated; phase=diagram"
                    } else if is_transient_conversation_failure(error) {
                        "diagram-v1/transient; phase=diagram"
                    } else {
                        "diagram-v1/invalid-document; phase=diagram"
                    }
                    .to_owned(),
                );
                *transcript = base_transcript.clone();
                transcript.push((
                    ModelMessageRole::User,
                    "CORE DIAGRAM RETRY: Return a shorter complete typed diagram object from the unchanged evidence. This is the single formatting retry; do not reopen research."
                        .to_owned(),
                ));
                continue;
            }
            Ok(Err(_)) | Err(_) if control.cancellation_token().is_cancelled() => {
                return Err(AgentSessionManagerFailure::Unavailable);
            }
            Ok(Err(_)) | Err(_) => return Ok(DiagramCompilationOutcome::incomplete(diagnostics)),
        };
        let diagrams = DecodeEvidenceDiagrams.decode(&raw).ok().and_then(|drafts| {
            drafts
                .into_iter()
                .map(|draft| {
                    let mut source_ids = Vec::new();
                    for ordinal in draft.source_ordinals() {
                        let source_id = state
                            .sources
                            .get(usize::from(ordinal.saturating_sub(1)))
                            .map(AskResearchSource::id)?;
                        if !source_ids.contains(&source_id) {
                            source_ids.push(source_id);
                        }
                    }
                    if source_ids.is_empty() {
                        return None;
                    }
                    Some((draft, source_ids))
                })
                .collect::<Option<Vec<_>>>()
        });
        if control.cancellation_token().is_cancelled() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        if let Some(diagrams) = diagrams {
            let mut guard = state.evidence_guard(project);
            for (_, sources) in &diagrams {
                for id in sources {
                    if let Some(source) = state.sources.iter().find(|source| source.id() == *id)
                        && !guard
                            .revisions
                            .iter()
                            .any(|(revision, _)| revision == source.revision())
                    {
                        guard.revisions.push((
                            source.revision().clone(),
                            source
                                .range()
                                .map_or(1, |range| range.start_position().row().saturating_add(1)),
                        ));
                    }
                }
            }
            guard.validate(control).await?;
            let artifacts = diagrams
                .into_iter()
                .map(
                    |(draft, source_ids)| -> Result<_, AgentSessionManagerFailure> {
                        Ok(EvidenceDiagramArtifact::new(
                            AgentDiagramArtifactId::from_bytes(random_id()?),
                            &draft,
                            source_ids,
                        ))
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(DiagramCompilationOutcome::complete(artifacts, diagnostics));
        }
        diagnostics.push("diagram-v1/invalid-document; phase=diagram".to_owned());
        *transcript = base_transcript.clone();
        transcript.push((
            ModelMessageRole::User,
            "The previous diagram object violated the strict typed schema or cited an unavailable source. Return one corrected object only; do not add Mermaid or prose."
                .to_owned(),
        ));
    }
}

struct DiagramCompilationOutcome {
    artifacts: Vec<EvidenceDiagramArtifact>,
    complete: bool,
    diagnostics: Vec<String>,
}

impl DiagramCompilationOutcome {
    fn complete(artifacts: Vec<EvidenceDiagramArtifact>, diagnostics: Vec<String>) -> Self {
        Self {
            artifacts,
            complete: true,
            diagnostics,
        }
    }

    fn incomplete(diagnostics: Vec<String>) -> Self {
        Self {
            artifacts: Vec::new(),
            complete: false,
            diagnostics,
        }
    }
}

fn awaiting_continuation(
    turn: &AskResearchTurn,
    state: &AskResearchWorkingSet,
    command_profile: Option<&SlashCommandExecutionProfile>,
    reason: ResearchStopReason,
) -> Result<AskResearchResult, AgentSessionManagerFailure> {
    let terminal_event = research_event(
        turn.session_id(),
        turn.user_sequence(),
        state.event_sequence.saturating_add(1),
        AskResearchPhase::Evaluating,
        AskResearchState::AwaitingContinuation,
        reason.message(),
        None,
        AskResearchCompleteness::NotApplicable,
    )?;
    let notice = format!(
        "{} Die bisherigen Belege bleiben erhalten. Das ist keine Sicherheitsfreigabe für Dateizugriff oder Ausführung. Mit „Recherche fortsetzen“ startest du einen weiteren begrenzten Abschnitt mit revalidiertem Arbeitsstand.",
        reason.message()
    );
    let work_answer = state.work_answer();
    let partial = if state.work.is_some() {
        work_answer.as_ref()
    } else {
        state.partial_answer.as_ref()
    };
    let (mut markdown, citations) = if let Some((markdown, ordinals)) = partial {
        (
            format!("{markdown}\n\n{notice}"),
            ordinals
                .iter()
                .filter_map(|ordinal| {
                    state
                        .sources
                        .get(usize::from(ordinal.saturating_sub(1)))
                        .map(AskResearchSource::id)
                })
                .collect(),
        )
    } else {
        (
            format!(
                "{notice}{}",
                state
                    .memory_gaps
                    .last()
                    .map_or(String::new(), |gap| format!(
                        "\n\nZuletzt noch offen: {gap}"
                    ))
            ),
            Vec::new(),
        )
    };
    if let Some(work) = &state.work {
        let open = work
            .questions()
            .iter()
            .filter(|q| {
                q.definition().priority == a3_domain::ResearchQuestionPriority::Required
                    && !q.resolved()
            })
            .map(|q| format!("- {}", q.definition().outcome))
            .collect::<Vec<_>>()
            .join("\n");
        if !open.is_empty() {
            markdown.push_str(&format!("\n\nNoch nicht beantwortet:\n{open}"));
        }
    }
    Ok(AskResearchResult {
        markdown,
        citations,
        diagrams: Vec::new(),
        terminal_event,
        awaiting_continuation: true,
        handoff: research_handoff(turn, state, command_profile)?,
    })
}

fn research_handoff(
    turn: &AskResearchTurn,
    state: &AskResearchWorkingSet,
    command_profile: Option<&SlashCommandExecutionProfile>,
) -> Result<ResearchHandoff, AgentSessionManagerFailure> {
    let mut revisions = Vec::new();
    for source in &state.sources {
        if !revisions.contains(source.revision()) {
            revisions.push(source.revision().clone());
        }
    }
    let mut handoff = ResearchHandoff::new(turn.index_run_id(), turn.snapshot_id(), revisions)
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
    if let Some(work) = &state.work {
        handoff = handoff.with_work_state(work.clone());
    }
    Ok(match command_profile {
        Some(profile) => handoff.with_command(profile.invocation().clone()),
        None => handoff,
    })
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
async fn ask_decision(
    runtime: &impl ResearchModel,
    mode: AgentSessionMode,
    initial_permission: BeginResearchDecision,
    transcript: &mut Vec<(ModelMessageRole, String)>,
    control: &JobContext,
    controller: &mut BoundedResearchController,
    started: Instant,
    source_count: usize,
    command_profile: Option<&SlashCommandExecutionProfile>,
    guard: &research_model::EvidenceGuard<'_>,
    repair_already_used: bool,
) -> Result<
    (
        Result<
            (
                a3_application::AskResearchDecision,
                BeginResearchDecision,
                bool,
            ),
            ResearchStopReason,
        >,
        Vec<String>,
    ),
    AgentSessionManagerFailure,
> {
    let mut permission = initial_permission;
    let mut repaired = repair_already_used;
    let mut diagnostics = Vec::new();
    // A repair starts from this validated context, not the rejected bytes or accumulated hints.
    let base_transcript = transcript.clone();
    loop {
        guard.validate(control).await?;
        let remaining = controller
            .limits()
            .duration_millis()
            .saturating_sub(elapsed_millis(started));
        if remaining == 0 {
            return Ok((Err(ResearchStopReason::TimeLimit), diagnostics));
        }
        let result = tokio::time::timeout(
            Duration::from_millis(remaining),
            runtime.complete_research_decision(
                mode,
                permission == BeginResearchDecision::SearchAllowed,
                guard.work.as_ref().map_or(
                    a3_application::ResearchOutputPhase::Initialize,
                    research_work::WorkGuard::output_phase,
                ),
                transcript,
                command_profile.map(|profile| profile.research_constraint(mode)),
                control,
            ),
        )
        .await;
        if control.cancellation_token().is_cancelled() {
            return Err(AgentSessionManagerFailure::Unavailable);
        }
        let issue = match result {
            Ok(Ok(raw)) => {
                match research_model::validate_phase_decision(
                    &raw,
                    permission,
                    source_count,
                    guard
                        .work
                        .as_ref()
                        .map(research_work::WorkGuard::output_phase),
                )
                .and_then(|decision| {
                    guard.admit_work(decision, runtime.requires_work_contract(), mode)
                })
                .and_then(|decision| {
                    if guard.work.as_ref().is_some_and(|w| {
                        w.output_phase() != a3_application::ResearchOutputPhase::Finalize
                    }) {
                        Ok(decision)
                    } else {
                        research_model::validate_outcome_with_attribution(
                            decision,
                            mode,
                            guard.work.is_none(),
                        )
                    }
                }) {
                    Ok(decision) => return Ok((Ok((decision, permission, repaired)), diagnostics)),
                    Err(issue) => issue,
                }
            }
            Ok(Err(error)) if is_transient_conversation_failure(error) => {
                diagnostics.push(research_model::diagnostic(
                    match error {
                        AgentConversationFailure::Stream(reason) => reason.code(),
                        _ => "research-v1/transient",
                    },
                    permission,
                    controller,
                ));
                if controller.use_model_retry().is_err() {
                    return Ok((Err(ResearchStopReason::ModelRetryLimit), diagnostics));
                }
                permission = match if guard.work.is_some() {
                    controller.begin_work_decision(elapsed_millis(started))
                } else {
                    controller.begin_decision(elapsed_millis(started))
                } {
                    Ok(next) => restrict_research_permission(permission, next),
                    Err(error) => {
                        return Ok((Err(ResearchStopReason::for_controller(error)), diagnostics));
                    }
                };
                // Retrying transport is not another repair of the malformed document.
                continue;
            }
            Ok(Err(
                AgentConversationFailure::OutputTruncated
                | AgentConversationFailure::OutputTooLarge,
            )) => research_model::DecisionIssue::Truncated,
            Ok(Err(AgentConversationFailure::InvalidOutput)) => {
                research_model::DecisionIssue::Stream
            }
            Ok(Err(_)) => return Err(AgentSessionManagerFailure::Unavailable),
            Err(_) => return Ok((Err(ResearchStopReason::TimeLimit), diagnostics)),
        };
        diagnostics.push(research_model::diagnostic(
            issue.code(),
            permission,
            controller,
        ));
        if repaired {
            return Ok((Err(ResearchStopReason::InvalidDecision), diagnostics));
        }
        let next = if guard.work.is_some() {
            controller
                .use_repair()
                .ok()
                .and_then(|()| controller.begin_work_decision(elapsed_millis(started)).ok())
        } else {
            reserve_research_repair_decision(controller, elapsed_millis(started), 0)
        };
        let Some(next) = next else {
            return Ok((
                Err(repair_stop_reason(controller, elapsed_millis(started), 0)),
                diagnostics,
            ));
        };
        repaired = true;
        permission = restrict_research_permission(permission, next);
        *transcript = base_transcript.clone();
        let hint = if issue == research_model::DecisionIssue::WorkCoverage {
            guard
                .work
                .as_ref()
                .and_then(research_work::WorkGuard::coverage_repair_hint)
        } else {
            None
        }
        .unwrap_or_else(|| {
            issue.repair_hint_for_phase(
                guard
                    .work
                    .as_ref()
                    .map(research_work::WorkGuard::output_phase),
                source_count,
            )
        });
        transcript.push((ModelMessageRole::User, utf8_prefix(&hint, 768).to_owned()));
    }
}
fn reserve_research_repair_decision(
    controller: &mut BoundedResearchController,
    elapsed_millis: u64,
    reserved_decisions: u8,
) -> Option<BeginResearchDecision> {
    controller.use_repair().ok()?;
    controller
        .begin_decision_reserving(elapsed_millis, reserved_decisions)
        .ok()
}

fn repair_stop_reason(
    controller: &BoundedResearchController,
    elapsed: u64,
    reserved: u8,
) -> ResearchStopReason {
    if elapsed >= controller.limits().duration_millis() {
        ResearchStopReason::TimeLimit
    } else if controller.decisions_used().saturating_add(reserved)
        >= controller.limits().model_decisions()
    {
        ResearchStopReason::DecisionLimit
    } else {
        ResearchStopReason::InvalidDecision
    }
}

fn restrict_research_permission(
    previous: BeginResearchDecision,
    next: BeginResearchDecision,
) -> BeginResearchDecision {
    if previous == BeginResearchDecision::FinalOnly {
        previous
    } else {
        next
    }
}

const fn is_transient_conversation_failure(error: AgentConversationFailure) -> bool {
    matches!(
        error,
        AgentConversationFailure::ModelTimedOut
            | AgentConversationFailure::Unavailable
            | AgentConversationFailure::Stream(_)
    )
}

const fn answer_requires_deeper_research(
    evidence_status: AskResearchEvidenceStatus,
    named_sources_covered: bool,
) -> bool {
    matches!(evidence_status, AskResearchEvidenceStatus::Incomplete) || !named_sources_covered
}

fn research_lens_seeds(
    query: &str,
    targets: &[ResolvedQueryTarget],
) -> Result<TaskLensSeedSet, AgentSessionManagerFailure> {
    let text = TaskLensSeedText::try_from_string(bounded_text(query, 4 * 1024))
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)?;
    let paths = resolved_target_revisions(targets)
        .iter()
        .map(|revision| a3_domain::TaskLensSeed::ExplicitPath(revision.path().clone()))
        .collect();
    TaskLensSeedSet::new(text.clone(), text, paths)
        .map_err(|_| AgentSessionManagerFailure::InvalidInput)
}

fn bounded_conversation(
    transcript: &[(ModelMessageRole, String)],
) -> Vec<(ModelMessageRole, String)> {
    let start = transcript.len().saturating_sub(12);
    transcript[start..].to_vec()
}

fn original_research_question(mut message: &str) -> &str {
    // Compatibility for continuations written before the dedicated resume path existed.
    while let Some(original) = message.strip_prefix("Recherche fortsetzen. Ursprüngliche Frage:\n")
    {
        message = original;
    }
    message
}

fn source_range_covers(
    current: Option<a3_domain::SourceRange>,
    previous: Option<a3_domain::SourceRange>,
) -> bool {
    match (current, previous) {
        (Some(current), Some(previous)) => current.contains(previous),
        _ => current == previous,
    }
}

fn rebind_public_note(
    note: &AskResearchPublicNote,
    source_mapping: &[(AskResearchSourceId, AskResearchSourceId)],
) -> Result<Option<AskResearchPublicNote>, AgentSessionManagerFailure> {
    let mapped = note
        .source_ids()
        .iter()
        .filter_map(|source_id| {
            source_mapping
                .iter()
                .find(|(previous, _)| previous == source_id)
                .map(|(_, current)| *current)
        })
        .collect::<Vec<_>>();
    // Check the complete original chain BEFORE canonicalizing converging references.
    if note.finding_kind() != AskResearchPublicFindingKind::Hypothesis
        && (mapped.is_empty() || mapped.len() != note.source_ids().len())
    {
        return Ok(None);
    }
    AskResearchPublicNote::new(
        note.goal().to_owned(),
        note.finding_kind(),
        note.finding().to_owned(),
        mapped,
        note.gap().to_owned(),
        note.next_step().to_owned(),
    )
    .map(Some)
    .map_err(|_| AgentSessionManagerFailure::InvalidOutput)
}

fn revalidated_source(
    current_sources: &[AskResearchSource],
    previous: &AskResearchSource,
) -> Option<AskResearchSourceId> {
    current_sources
        .iter()
        .find(|candidate| {
            candidate.revision() == previous.revision()
                && source_range_covers(candidate.range(), previous.range())
        })
        .map(AskResearchSource::id)
}

fn prioritize_continuation_sources(
    sources: &mut [AskResearchSource],
    previous: &a3_application::AskResearchDetail,
) {
    let wanted = previous
        .events()
        .iter()
        .rev()
        .filter_map(|event| event.public_note())
        .take(3)
        .flat_map(|note| note.source_ids().iter().copied())
        .collect::<Vec<_>>();
    sources.sort_by_key(|source| {
        (
            wanted
                .iter()
                .position(|id| *id == source.id())
                .unwrap_or(usize::MAX),
            std::cmp::Reverse(source.ordinal()),
        )
    });
}

fn research_decision_context(
    conversation: &[(ModelMessageRole, String)],
    feedback: &str,
    evidence: String,
) -> Vec<(ModelMessageRole, String)> {
    let mut messages = bounded_conversation(conversation);
    if !feedback.is_empty() {
        messages.push((ModelMessageRole::User, bounded_text(feedback, 8 * 1024)));
    }
    // Rebuild from original inputs, never previous compiled Evidence packs or model transcripts.
    messages.push((ModelMessageRole::User, evidence));
    messages
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedQueryTarget {
    requested: String,
    revision: Option<a3_domain::FileRevision>,
}

fn resolve_query_targets(
    published: &a3_domain::PublishedIndex,
    query: &str,
) -> Vec<ResolvedQueryTarget> {
    query_path_candidates(query)
        .into_iter()
        .map(|requested| ResolvedQueryTarget {
            revision: resolve_index_path(published, &requested).cloned(),
            requested,
        })
        .collect()
}

fn resolved_target_revisions(targets: &[ResolvedQueryTarget]) -> Vec<a3_domain::FileRevision> {
    let mut revisions = Vec::new();
    for revision in targets.iter().filter_map(|target| target.revision.as_ref()) {
        if !revisions.contains(revision) {
            revisions.push(revision.clone());
        }
    }
    revisions
}

fn query_path_candidates(query: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for raw in query.split_whitespace() {
        let explicit = raw.starts_with('@');
        let token = raw
            // Sentence punctuation outside a quoted path is not part of the target.
            // Stop at a quote/backtick so literal trailing ?/! names remain exact.
            .trim_end_matches(['?', '!', ',', ';', ':', '.', ')', ']', '}'])
            .trim_start_matches(|character: char| {
                matches!(character, '@' | '`' | '"' | '\'' | '(' | '[' | '{')
            })
            .trim_end_matches(|character: char| {
                matches!(
                    character,
                    '`' | '"' | '\'' | ',' | ';' | ':' | '.' | ')' | ']' | '}'
                )
            })
            .replace('\\', "/");
        let file_like = token.contains('/')
            || token.rsplit_once('.').is_some_and(|(stem, extension)| {
                !stem.is_empty()
                    && (1..=12).contains(&extension.len())
                    && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
            });
        if (explicit || file_like)
            && !token.is_empty()
            && !candidates
                .iter()
                .any(|candidate: &String| candidate.eq_ignore_ascii_case(&token))
        {
            candidates.push(token);
            if candidates.len() == 8 {
                break;
            }
        }
    }
    candidates
}

fn task_lens_search_terms(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_alphanumeric() && !matches!(character, '@' | '/' | '\\' | '.' | '_')
            })
        })
        .filter(|token| token.chars().count() >= 2)
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_index_path<'a>(
    published: &'a a3_domain::PublishedIndex,
    requested: &str,
) -> Option<&'a a3_domain::FileRevision> {
    let normalized = requested
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_owned();
    let files = published.publication().graph().files();
    if let Some(exact) = files
        .iter()
        .find(|revision| safe_path(revision.path()).eq_ignore_ascii_case(&normalized))
    {
        return Some(exact);
    }
    let suffix = format!("/{normalized}").to_ascii_lowercase();
    let mut matches = files.iter().filter(|revision| {
        let path = safe_path(revision.path()).replace('\\', "/");
        index_path_matches_request(&path, &normalized, &suffix)
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn index_path_matches_request(path: &str, normalized: &str, lowercase_suffix: &str) -> bool {
    if normalized.contains('/') {
        path.eq_ignore_ascii_case(normalized)
            || path.to_ascii_lowercase().ends_with(lowercase_suffix)
    } else {
        path.rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(normalized))
    }
}

fn relation_label(relation: AskResearchRelation) -> &'static str {
    match relation {
        AskResearchRelation::Callers => "Aufrufer",
        AskResearchRelation::Callees => "aufgerufene Symbole",
        AskResearchRelation::Imports => "Imports",
        AskResearchRelation::Exports => "Exports",
        AskResearchRelation::Tests => "Tests",
    }
}

fn list_index_directory(
    published: &a3_domain::PublishedIndex,
    requested: &str,
) -> (Vec<String>, bool) {
    let directory = requested.trim().trim_matches('/').replace('\\', "/");
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let mut children = BTreeSet::new();
    let mut limited = false;
    for revision in published.publication().graph().files() {
        let path = safe_path(revision.path()).replace('\\', "/");
        let Some(remainder) = path.strip_prefix(&prefix) else {
            continue;
        };
        if remainder.is_empty() {
            continue;
        }
        let child = remainder.split('/').next().unwrap_or(remainder);
        let display = if remainder.contains('/') {
            format!("{child}/")
        } else {
            child.to_owned()
        };
        children.insert(display);
        if children.len() > 100 {
            limited = true;
            break;
        }
    }
    let mut children = children.into_iter().collect::<Vec<_>>();
    children.truncate(100);
    (children, limited)
}

fn explicit_repository_literals(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut literals = Vec::new();
    if lower.contains("todo") {
        literals.push("TODO".to_owned());
    }
    if lower.contains("fixme") {
        literals.push("FIXME".to_owned());
    }
    literals
}

fn lens_selection_reason(reason: &TaskLensEntryReason) -> AskResearchSelectionReason {
    match reason {
        TaskLensEntryReason::RepositoryAnchor => AskResearchSelectionReason::IndexedText,
        TaskLensEntryReason::Claim(_) => AskResearchSelectionReason::VerifiedModuleKnowledge,
        TaskLensEntryReason::Retrieval { explanation, .. } => {
            if explanation
                .sources()
                .iter()
                .any(|source| source.reason().source_channel() == SourceChannel::Exact)
            {
                AskResearchSelectionReason::ExactNameOrPath
            } else if explanation
                .sources()
                .iter()
                .any(|source| source.reason().source_channel() == SourceChannel::Test)
            {
                AskResearchSelectionReason::Test
            } else if explanation
                .sources()
                .iter()
                .any(|source| source.reason().source_channel() == SourceChannel::Graph)
            {
                AskResearchSelectionReason::Relationship
            } else if explanation
                .sources()
                .iter()
                .any(|source| source.reason().source_channel() == SourceChannel::Semantic)
            {
                AskResearchSelectionReason::SemanticCandidate
            } else {
                AskResearchSelectionReason::IndexedText
            }
        }
    }
}

const fn source_channel_label(channel: SourceChannel) -> &'static str {
    match channel {
        SourceChannel::Exact => "Exakte Namen/Pfade",
        SourceChannel::Lexical => "Indexierter Text",
        SourceChannel::Graph => "Beziehungen",
        SourceChannel::Test => "Tests",
        SourceChannel::Memory => "Verifiziertes Modulwissen",
        SourceChannel::Semantic => "Semantische Kandidaten",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlanConversationResponse {
    Question(String),
    Plan(String),
}

/// Shared by conversation publication and the real research/storage contract test. No plan
/// approval, task materialization or execution can be produced by this read-only transition.
fn plan_session_outcome(
    session: &AgentSession,
    content: &str,
    has_citations: bool,
) -> (
    AgentSessionState,
    AgentSessionEntryKind,
    Option<u32>,
    String,
) {
    match classify_plan_response(content) {
        PlanConversationResponse::Plan(plan) if has_citations => (
            AgentSessionState::AwaitingPlanReview, AgentSessionEntryKind::Plan,
            Some(session.current_plan_revision().unwrap_or(0).saturating_add(1)), plan,
        ),
        PlanConversationResponse::Plan(_) => (
            AgentSessionState::AwaitingUser, AgentSessionEntryKind::AssistantSummary,
            session.current_plan_revision(),
            "Der Plan ist strukturiert, aber noch nicht durch aktuelle Quellen belegt. Setze die Recherche fort oder präzisiere den gewünschten Schwerpunkt.".to_owned(),
        ),
        PlanConversationResponse::Question(question) => (
            AgentSessionState::AwaitingUser, AgentSessionEntryKind::AssistantSummary,
            session.current_plan_revision(), question,
        ),
    }
}

fn classify_plan_response(content: &str) -> PlanConversationResponse {
    let trimmed = content.trim();
    if let Some(plan) = trimmed.strip_prefix("PLAN:") {
        let plan = plan.trim();
        if !plan.is_empty() && has_required_plan_sections(plan) {
            return PlanConversationResponse::Plan(plan.to_owned());
        }
        return PlanConversationResponse::Question(
            "Der Plan konnte noch nicht vollständig strukturiert und belegt werden. Bitte starte die Recherche erneut oder ergänze den fehlenden Schwerpunkt."
                .to_owned(),
        );
    }
    if let Some(question) = trimmed.strip_prefix("QUESTION:") {
        let question = question.trim();
        if !question.is_empty() {
            return PlanConversationResponse::Question(question.to_owned());
        }
    }
    PlanConversationResponse::Question(trimmed.to_owned())
}

fn response_requires_citations(mode: AgentSessionMode, content: &str) -> bool {
    match mode {
        AgentSessionMode::Ask => true,
        AgentSessionMode::Plan | AgentSessionMode::Agent => matches!(
            classify_plan_response(content),
            PlanConversationResponse::Plan(_)
        ),
    }
}

fn has_required_plan_sections(plan: &str) -> bool {
    const REQUIRED_SECTIONS: [&str; 5] = [
        "Summary",
        "Implementation Changes",
        "Interfaces",
        "Test Plan",
        "Assumptions",
    ];
    let mut present = [false; 5];
    let mut content = [false; 5];
    let mut current = None;
    let mut fence = None;
    for line in plan.lines().map(str::trim) {
        if line.starts_with("```") || line.starts_with("~~~") {
            let marker = line.as_bytes()[0];
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            // Keep the existing work-plan compiler from seeing another set of required
            // section names inside a code example. Ordinary interface code remains content.
            if line.starts_with('#')
                && REQUIRED_SECTIONS.iter().any(|required| {
                    line.trim_start_matches('#')
                        .trim()
                        .eq_ignore_ascii_case(required)
                })
            {
                return false;
            }
            if !line.is_empty()
                && let Some(index) = current
            {
                content[index] = true;
            }
            continue;
        }
        if line.starts_with('#') {
            current = REQUIRED_SECTIONS.iter().position(|required| {
                line.trim_start_matches('#')
                    .trim()
                    .eq_ignore_ascii_case(required)
            });
            if let Some(index) = current {
                if present[index] {
                    return false;
                }
                present[index] = true;
            }
        } else if !line.is_empty()
            && let Some(index) = current
        {
            content[index] = true;
        }
    }
    present.into_iter().all(|value| value) && content.into_iter().all(|value| value)
}

fn random_id() -> Result<[u8; 32], AgentSessionManagerFailure> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AgentSessionManagerFailure::Unavailable)?;
    Ok(bytes)
}

const fn safe_failure_message(error: AgentConversationFailure) -> &'static str {
    match error {
        AgentConversationFailure::Stream(reason) => reason.code(),
        AgentConversationFailure::Cancelled => "Die Modellanfrage wurde abgebrochen.",
        AgentConversationFailure::ModelNotConfigured => {
            "Konfiguriere und verifiziere zuerst ein Coding-Modell in den Einstellungen."
        }
        AgentConversationFailure::SecretContent => {
            "Die Nachricht wurde gestoppt, weil sie ein Secret enthalten könnte. Entferne Zugangsdaten oder Tokens und versuche es erneut."
        }
        AgentConversationFailure::InvalidInput => "Die Eingabe ist ungültig.",
        AgentConversationFailure::OutputTooLarge | AgentConversationFailure::OutputTruncated => {
            "Die Modellantwort überschritt die sichere Größenbegrenzung."
        }
        AgentConversationFailure::InvalidOutput => {
            "Das Modell lieferte eine unvollständige Antwort."
        }
        AgentConversationFailure::ModelRejected => {
            "Der Modellprovider hat die begrenzte Anfrage abgelehnt. Aktualisiere die Modellliste und verifiziere die Coding-Capability erneut; wähle bei wiederholter Ablehnung ein anderes Modell."
        }
        AgentConversationFailure::ModelTimedOut => {
            "Die Modellantwort überschritt die feste Deadline. Versuche es erneut oder wähle ein kleineres beziehungsweise schnelleres Coding-Modell."
        }
        AgentConversationFailure::Unavailable => {
            "Das konfigurierte Coding-Modell ist derzeit nicht erreichbar. Prüfe Providerstatus, Verbindung und Zugangsdaten und versuche es erneut."
        }
    }
}

const fn safe_manager_failure_message(error: AgentSessionManagerFailure) -> &'static str {
    match error {
        AgentSessionManagerFailure::InvalidInput | AgentSessionManagerFailure::InvalidOutput => {
            "A^3 konnte die vorbereiteten Informationen nicht sicher verarbeiten. Prüfe den Projektindex und versuche es erneut."
        }
        AgentSessionManagerFailure::NotFound
        | AgentSessionManagerFailure::Conflict
        | AgentSessionManagerFailure::IndexChanged => {
            "Die Session hat sich während der Verarbeitung geändert. Der zuletzt sicher gespeicherte Stand bleibt erhalten."
        }
        AgentSessionManagerFailure::Busy => {
            "Ein anderer Agentenlauf belegt die lokale Laufzeit. Warte auf dessen Abschluss und versuche es erneut."
        }
        AgentSessionManagerFailure::Unavailable => {
            "A^3 konnte das aktuelle Projektwissen nicht laden. Aktualisiere den Projektindex, prüfe das Coding-Modell und versuche es erneut."
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveConversation {
    session_id: AgentSessionId,
    job_id: Option<JobId>,
}

struct SessionPermit<'a> {
    active: &'a Mutex<Option<ActiveConversation>>,
    session_id: AgentSessionId,
    activated: bool,
}

fn release_conversation_owner(
    active: &Mutex<Option<ActiveConversation>>,
    session_id: AgentSessionId,
) {
    let mut owner = lock_recovering_poison(active);
    if owner
        .as_ref()
        .is_some_and(|value| value.session_id == session_id)
    {
        *owner = None;
    }
}

impl SessionPermit<'_> {
    fn activate(&mut self, job_id: JobId) {
        let mut active = lock_recovering_poison(self.active);
        if active
            .as_ref()
            .is_some_and(|value| value.session_id == self.session_id)
        {
            *active = Some(ActiveConversation {
                session_id: self.session_id,
                job_id: Some(job_id),
            });
            self.activated = true;
        }
    }
}

impl Drop for SessionPermit<'_> {
    fn drop(&mut self) {
        if !self.activated {
            let mut active = lock_recovering_poison(self.active);
            if active
                .as_ref()
                .is_some_and(|value| value.session_id == self.session_id)
            {
                *active = None;
            }
        }
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PresentationMutation {
    Rename(String),
    Archive,
    Unarchive,
    SwitchToPlan,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSessionManagerFailure {
    InvalidInput,
    InvalidOutput,
    NotFound,
    Conflict,
    IndexChanged,
    Busy,
    Unavailable,
}

impl From<AgentSessionStoreFailure> for AgentSessionManagerFailure {
    fn from(value: AgentSessionStoreFailure) -> Self {
        match value {
            AgentSessionStoreFailure::InvalidInput
            | AgentSessionStoreFailure::InvalidStoredData => Self::InvalidInput,
            AgentSessionStoreFailure::Conflict => Self::Conflict,
            AgentSessionStoreFailure::Unavailable => Self::Unavailable,
        }
    }
}

impl From<AskResearchStoreFailure> for AgentSessionManagerFailure {
    fn from(value: AskResearchStoreFailure) -> Self {
        match value {
            AskResearchStoreFailure::InvalidInput | AskResearchStoreFailure::InvalidStoredData => {
                Self::InvalidInput
            }
            AskResearchStoreFailure::Conflict => Self::Conflict,
            AskResearchStoreFailure::Unavailable => Self::Unavailable,
        }
    }
}

impl fmt::Display for AgentSessionManagerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "Agent session input is invalid",
            Self::InvalidOutput => "Agent session output is invalid",
            Self::NotFound => "Agent session was not found",
            Self::Conflict => "Agent session changed",
            Self::IndexChanged => "Agent plan index changed",
            Self::Busy => "Agent session operation is already active",
            Self::Unavailable => "Agent session is unavailable",
        })
    }
}

impl Error for AgentSessionManagerFailure {}

impl fmt::Debug for AgentSessionManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentSessionManager")
            .finish_non_exhaustive()
    }
}

impl Drop for AgentSessionManager {
    fn drop(&mut self) {
        if let Some(job_id) = lock_recovering_poison(&self.active_session)
            .as_ref()
            .and_then(|active| active.job_id)
        {
            let _cancelled = self.submitter.cancel(job_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentConversationFailure, AskResearchWorkingSet, ConversationTaskLensControl,
        ConversationTerminal, PlanConversationResponse, QueueDispatchTrigger, ResearchStopReason,
        ResolvedQueryTarget, agent_run_blocks_plan_start, answer_requires_deeper_research,
        awaiting_continuation, classify_plan_response, command_clarification_question,
        command_message, index_path_matches_request, is_transient_conversation_failure,
        model_safe_path, parse_working_change_paths, presentation_can_be_hidden,
        query_path_candidates, queue_dispatch_allows_state, read_bounded_process_output,
        reserve_research_repair_decision, resolve_next_message_mode, response_requires_citations,
        restore_command_profile, safe_failure_message, settle_unfinished_conversation,
        verification_command_order, visible_research_query,
    };
    use a3_application::{
        AgentSessionCommandPresentation, AgentSessionDetail, AgentSessionListQuery,
        AgentSessionPage, AgentSessionStore, AgentSessionStoreFailure, AgentSessionStoreFuture,
        JobClock, JobCompletion, JobContext, JobEventKind, JobScheduler, JobSchedulerConfig,
        JobTimestamp, TaskLensControl,
    };
    use a3_application::{AskResearchEvidenceStatus, AskResearchSource, AskResearchTurn};
    use a3_domain::{
        AgentSession, AgentSessionEntry, AgentSessionEntryKind, AgentSessionId, AgentSessionMode,
        AgentSessionRevision, AgentSessionSequence, AgentSessionState, AgentSessionText,
        AgentSessionTimestamp, AgentSessionTitle, AskResearchSelectionReason, AskResearchSourceId,
        AskResearchSourceKind, AskResearchState, ContentHash, DiscoveredCommandKind, FileRevision,
        IndexRunId, JobId, JobOwner, Progress, ProjectIdentity, RepositoryId, RepositoryIdentity,
        RepositoryPath, SlashCommand, SlashCommandCatalogVersion, SlashCommandLens,
        SlashCommandVerificationProfile, SnapshotId, WorktreeAnchorId, WorktreeId,
        WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn only_live_or_paused_agent_runs_block_a_reviewed_plan_start() {
        use crate::agent_run_manager::AgentRunActivityState;

        for state in [
            AgentRunActivityState::Queued,
            AgentRunActivityState::Running,
            AgentRunActivityState::Pausing,
            AgentRunActivityState::Paused,
            AgentRunActivityState::Cancelling,
        ] {
            assert!(agent_run_blocks_plan_start(state));
        }
        for state in [
            AgentRunActivityState::Idle,
            AgentRunActivityState::Succeeded,
            AgentRunActivityState::Failed,
            AgentRunActivityState::Cancelled,
        ] {
            assert!(!agent_run_blocks_plan_start(state));
        }
    }

    #[test]
    fn agent_mode_requires_a_fresh_plan_unless_agent_continuity_is_unbroken() {
        assert_eq!(
            resolve_next_message_mode(None, AgentSessionMode::Agent),
            (AgentSessionMode::Plan, true)
        );
        assert_eq!(
            resolve_next_message_mode(Some(AgentSessionMode::Ask), AgentSessionMode::Agent),
            (AgentSessionMode::Plan, true)
        );
        assert_eq!(
            resolve_next_message_mode(Some(AgentSessionMode::Plan), AgentSessionMode::Agent),
            (AgentSessionMode::Plan, true)
        );
        assert_eq!(
            resolve_next_message_mode(Some(AgentSessionMode::Agent), AgentSessionMode::Agent),
            (AgentSessionMode::Agent, false)
        );
        assert_eq!(
            resolve_next_message_mode(Some(AgentSessionMode::Agent), AgentSessionMode::Ask),
            (AgentSessionMode::Ask, false)
        );
    }

    #[test]
    fn automatic_queue_dispatch_stops_at_every_human_halt() {
        assert!(queue_dispatch_allows_state(
            QueueDispatchTrigger::Automatic,
            AgentSessionState::Completed
        ));
        for state in [
            AgentSessionState::AwaitingUser,
            AgentSessionState::AwaitingPlanReview,
            AgentSessionState::AwaitingApproval,
            AgentSessionState::Paused,
            AgentSessionState::Failed,
            AgentSessionState::Cancelled,
        ] {
            assert!(!queue_dispatch_allows_state(
                QueueDispatchTrigger::Automatic,
                state
            ));
        }
        for state in [AgentSessionState::Failed, AgentSessionState::Cancelled] {
            assert!(queue_dispatch_allows_state(
                QueueDispatchTrigger::ExplicitResume,
                state
            ));
        }
        assert!(!queue_dispatch_allows_state(
            QueueDispatchTrigger::ExplicitResume,
            AgentSessionState::AwaitingPlanReview
        ));
    }

    #[test]
    fn plan_marker_is_removed_before_the_revision_is_persisted() {
        let plan = "## Summary\nReady\n## Implementation Changes\nChange\n## Interfaces\nNone\n## Test Plan\nTest\n## Assumptions\nCurrent index";
        assert_eq!(
            classify_plan_response(&format!("PLAN:\n{plan}")),
            PlanConversationResponse::Plan(plan.to_owned())
        );
    }

    #[test]
    fn every_reviewed_plan_materializes_atomic_change_and_test_steps()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = "## Summary\nReview\n## Implementation Changes\n- [High] Fix path validation.\n  - Keep the canonical root check.\n- [Medium] Close the race before writing.\n## Interfaces\nNone\n## Test Plan\nRun tests\n## Assumptions\nCurrent index";
        let compiled = a3_domain::AgentWorkPlan::from_reviewed_markdown(plan)?;
        assert_eq!(compiled.steps().len(), 3);
        assert_eq!(
            compiled.steps()[0].outcome(),
            "[High] Fix path validation. Keep the canonical root check."
        );
        assert_eq!(
            compiled.steps()[1].outcome(),
            "[Medium] Close the race before writing."
        );
        assert_eq!(compiled.steps()[2].outcome(), "Run tests");
        Ok(())
    }

    #[test]
    fn reviewed_plan_without_list_items_still_separates_change_and_test()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = "## Summary\nReview\n## Implementation Changes\nNo confirmed change is required.\n## Interfaces\nNone\n## Test Plan\nRun tests\n## Assumptions\nCurrent index";
        let compiled = a3_domain::AgentWorkPlan::from_reviewed_markdown(plan)?;
        assert_eq!(compiled.steps().len(), 2);
        Ok(())
    }

    #[test]
    fn missing_or_question_marker_never_unlocks_implementation() {
        assert_eq!(
            classify_plan_response("QUESTION: Welche Plattform ist relevant?"),
            PlanConversationResponse::Question("Welche Plattform ist relevant?".to_owned())
        );
        assert_eq!(
            classify_plan_response("Ich brauche noch eine Entscheidung."),
            PlanConversationResponse::Question("Ich brauche noch eine Entscheidung.".to_owned())
        );
        assert!(matches!(
            classify_plan_response("PLAN:\n## Summary\nUnvollständig"),
            PlanConversationResponse::Question(_)
        ));
    }

    #[test]
    fn answers_and_plans_require_citations_but_a_plan_clarification_does_not() {
        let plan = "PLAN:\n## Summary\nReady\n## Implementation Changes\nChange\n## Interfaces\nNone\n## Test Plan\nTest\n## Assumptions\nCurrent index";
        assert!(response_requires_citations(
            AgentSessionMode::Ask,
            "Antwort"
        ));
        assert!(response_requires_citations(AgentSessionMode::Agent, plan));
        assert!(response_requires_citations(AgentSessionMode::Plan, plan));
        assert!(!response_requires_citations(
            AgentSessionMode::Plan,
            "QUESTION: Welche Plattform ist relevant?"
        ));
        assert!(!response_requires_citations(
            AgentSessionMode::Agent,
            "QUESTION: Soll die bestehende API kompatibel bleiben?"
        ));
    }

    #[test]
    fn persisted_command_profile_is_revalidated_before_plan_materialization()
    -> Result<(), Box<dyn std::error::Error>> {
        let stored = AgentSessionCommandPresentation::restore(
            AgentSessionSequence::FIRST,
            SlashCommandCatalogVersion::V1,
            SlashCommand::Review,
            vec![SlashCommandLens::Security],
            a3_domain::AgentResearchDepth::Thorough,
        )?;
        let profile = restore_command_profile(
            AgentSessionMode::Plan,
            "/review /security authentication",
            &stored,
        )?;
        assert_eq!(
            profile.verification_profile(),
            SlashCommandVerificationProfile::Review
        );
        assert!(
            restore_command_profile(AgentSessionMode::Plan, "/review authentication", &stored)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn command_verification_profiles_choose_only_supported_manifest_kinds() {
        assert_eq!(
            verification_command_order(SlashCommandVerificationProfile::Repair),
            &[
                DiscoveredCommandKind::Test,
                DiscoveredCommandKind::Build,
                DiscoveredCommandKind::Lint,
                DiscoveredCommandKind::Format,
            ]
        );
        assert_eq!(
            verification_command_order(SlashCommandVerificationProfile::Documentation)[0],
            DiscoveredCommandKind::Lint
        );
    }

    #[test]
    fn empty_target_clarification_retains_command_and_lenses_on_follow_up()
    -> Result<(), Box<dyn std::error::Error>> {
        let stored = AgentSessionCommandPresentation::restore(
            AgentSessionSequence::FIRST,
            SlashCommandCatalogVersion::V1,
            SlashCommand::Doc,
            vec![SlashCommandLens::Architecture],
            a3_domain::AgentResearchDepth::Thorough,
        )?;
        let profile =
            restore_command_profile(AgentSessionMode::Agent, "/doc /architecture", &stored)?;
        let (text, depth, resumed) = command_message(
            AgentSessionMode::Agent,
            profile,
            "die öffentliche API in src/lib.rs",
        )?;
        let resumed = resumed.ok_or("command profile missing")?;
        assert_eq!(
            text.as_str(),
            "/doc /architecture die öffentliche API in src/lib.rs"
        );
        assert_eq!(depth, a3_domain::AgentResearchDepth::Thorough);
        assert_eq!(
            resumed.invocation().lenses(),
            &[SlashCommandLens::Architecture]
        );
        assert_eq!(
            command_clarification_question(SlashCommand::Doc),
            "Welche Dokumentation oder welcher Codebereich soll erstellt beziehungsweise aktualisiert werden?"
        );
        Ok(())
    }

    #[test]
    fn working_change_metadata_is_bounded_and_rename_origins_are_not_treated_as_targets()
    -> Result<(), std::io::Error> {
        let parsed =
            parse_working_change_paths(b" M src/lib.rs\0R  src/new.rs\0src/old.rs\0?? notes.md\0");
        assert_eq!(
            parsed.paths,
            vec![
                b"notes.md".to_vec(),
                b"src/lib.rs".to_vec(),
                b"src/new.rs".to_vec()
            ]
        );
        assert!(!parsed.limited);

        let retained = read_bounded_process_output(Cursor::new(vec![7_u8; 64]), 9)?;
        assert_eq!(retained, vec![7_u8; 9]);
        Ok(())
    }

    #[test]
    fn active_presentations_cannot_be_hidden_while_authoritative_work_continues() {
        assert!(!presentation_can_be_hidden(AgentSessionState::Running));
        assert!(!presentation_can_be_hidden(
            AgentSessionState::AwaitingApproval
        ));
        assert!(!presentation_can_be_hidden(AgentSessionState::Paused));
        assert!(presentation_can_be_hidden(AgentSessionState::Completed));
        assert!(presentation_can_be_hidden(AgentSessionState::Failed));
        assert!(presentation_can_be_hidden(AgentSessionState::Cancelled));
    }

    #[test]
    fn conversation_failures_explain_safe_recovery_without_provider_payloads() {
        let rejected = safe_failure_message(AgentConversationFailure::ModelRejected);
        assert!(rejected.contains("Coding-Capability erneut"));
        assert!(rejected.contains("anderes Modell"));
        assert!(!rejected.contains("request body"));

        let timed_out = safe_failure_message(AgentConversationFailure::ModelTimedOut);
        assert!(timed_out.contains("feste Deadline"));
        assert!(timed_out.contains("schnelleres Coding-Modell"));
    }

    #[test]
    fn ask_model_paths_cannot_inject_control_characters() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = RepositoryPath::try_from_bytes(b"src/\nmalicious.rs".to_vec())?;

        assert_eq!(model_safe_path(&path), "src/\u{fffd}malicious.rs");
        Ok(())
    }

    #[test]
    fn ask_research_events_never_persist_secret_like_search_text() {
        assert!(visible_research_query(Some("ghp_abcdefghijklmnopqrstuvwxyz1234567890")).is_none());
        assert_eq!(
            visible_research_query(Some("TODO parser")),
            Some("TODO parser".to_owned())
        );
    }

    #[test]
    fn named_repository_files_are_detected_without_at_mentions() {
        assert_eq!(
            query_path_candidates(
                "Wie schreibt taskflow/plugins.py? Prüfe (src/a.rs!) und `src/literal.py?`!"
            ),
            vec!["taskflow/plugins.py", "src/a.rs", "src/literal.py?"]
        );
        assert_eq!(
            query_path_candidates(
                "Prüfe manager.py, `plugins/base.py` und @taskflow/plugins/audit_log_plugin.py."
            ),
            vec![
                "manager.py".to_owned(),
                "plugins/base.py".to_owned(),
                "taskflow/plugins/audit_log_plugin.py".to_owned(),
            ]
        );
        assert!(query_path_candidates("Wie ist das Programm aufgebaut.").is_empty());
        assert_eq!(
            query_path_candidates(
                "Welche Methoden in manager.py, plugins/base.py und plugins/audit_log_plugin.py werden nacheinander aufgerufen?"
            ),
            vec![
                "manager.py".to_owned(),
                "plugins/base.py".to_owned(),
                "plugins/audit_log_plugin.py".to_owned(),
            ]
        );
        assert!(index_path_matches_request(
            "taskflow/plugins/base.py",
            "plugins/base.py",
            "/plugins/base.py"
        ));
        assert!(index_path_matches_request(
            "plugins/base.py",
            "plugins/base.py",
            "/plugins/base.py"
        ));
        assert!(index_path_matches_request(
            "Plugins/Base.py",
            "plugins/base.py",
            "/plugins/base.py"
        ));
        assert!(!index_path_matches_request(
            "otherplugins/base.py",
            "plugins/base.py",
            "/plugins/base.py"
        ));
        assert!(!index_path_matches_request(
            "examples/plugins/base.py.txt",
            "plugins/base.py",
            "/plugins/base.py"
        ));
    }

    #[test]
    fn prioritized_named_evidence_displaces_baseline_context_in_small_windows()
    -> Result<(), Box<dyn std::error::Error>> {
        let historical_revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"taskflow/storage/factory.py".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        let named_revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"taskflow/manager.py".to_vec())?,
            ContentHash::from_bytes([2; 32]),
        );
        let historical = AskResearchSource::new(
            a3_domain::AgentSessionId::from_bytes([3; 32]),
            a3_domain::AgentSessionSequence::FIRST,
            AskResearchSourceId::from_bytes([4; 32]),
            1,
            historical_revision,
            None,
            None,
            AskResearchSourceKind::File,
            AskResearchSelectionReason::IndexedText,
        )?;
        let named = AskResearchSource::new(
            a3_domain::AgentSessionId::from_bytes([3; 32]),
            a3_domain::AgentSessionSequence::FIRST,
            AskResearchSourceId::from_bytes([5; 32]),
            2,
            named_revision.clone(),
            None,
            None,
            AskResearchSourceKind::File,
            AskResearchSelectionReason::ExactNameOrPath,
        )?;
        let mut state = AskResearchWorkingSet::new(320);
        assert!(state.render(&historical, 1, &"old storage context ".repeat(30), false));
        state.sources.push(historical);
        assert!(state.render(
            &named,
            1,
            "class TaskFlowManager:\n    def add_task(self): pass\n",
            true,
        ));
        state.sources.push(named);
        let targets = vec![ResolvedQueryTarget {
            requested: "manager.py".to_owned(),
            revision: Some(named_revision),
        }];

        let evidence = state.model_evidence("Prüfe manager.py", &targets);

        assert!(evidence.contains("manager.py => taskflow/manager.py => S2"));
        assert!(evidence.contains("class TaskFlowManager"));
        Ok(())
    }

    #[test]
    fn incomplete_or_uncovered_answers_must_continue_research() {
        assert!(answer_requires_deeper_research(
            AskResearchEvidenceStatus::Incomplete,
            true
        ));
        assert!(answer_requires_deeper_research(
            AskResearchEvidenceStatus::Sufficient,
            false
        ));
        assert!(!answer_requires_deeper_research(
            AskResearchEvidenceStatus::Sufficient,
            true
        ));
        assert!(is_transient_conversation_failure(
            AgentConversationFailure::ModelTimedOut
        ));
        assert!(is_transient_conversation_failure(
            AgentConversationFailure::Unavailable
        ));
        assert!(!is_transient_conversation_failure(
            AgentConversationFailure::ModelRejected
        ));
    }

    #[test]
    fn invalid_research_output_exhaustion_becomes_a_safe_continuation()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut controller =
            a3_application::BoundedResearchController::new(a3_domain::AgentResearchDepth::Standard);

        assert!(reserve_research_repair_decision(&mut controller, 0, 0).is_some());
        assert!(reserve_research_repair_decision(&mut controller, 1, 0).is_some());
        assert!(reserve_research_repair_decision(&mut controller, 2, 0).is_some());
        assert!(reserve_research_repair_decision(&mut controller, 3, 0).is_none());
        let turn = AskResearchTurn::new(
            AgentSessionId::from_bytes([21; 32]),
            AgentSessionSequence::FIRST,
            IndexRunId::from_bytes([22; 32]),
            SnapshotId::from_bytes([23; 32]),
            AgentSessionTimestamp::from_unix_millis(1)?,
        );
        let result = awaiting_continuation(
            &turn,
            &AskResearchWorkingSet::new(1_024),
            None,
            ResearchStopReason::InvalidDecision,
        )?;
        assert!(result.awaiting_continuation);
        assert_eq!(
            result.terminal_event.state(),
            AskResearchState::AwaitingContinuation
        );
        assert!(result.markdown.contains("Recherche fortsetzen"));
        Ok(())
    }

    #[test]
    fn diagram_profile_does_not_reserve_research_capacity() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut controller =
            a3_application::BoundedResearchController::new(a3_domain::AgentResearchDepth::Standard);
        for elapsed in 0..11 {
            assert_eq!(
                controller.begin_decision(elapsed)?,
                a3_application::BeginResearchDecision::SearchAllowed
            );
        }
        assert_eq!(
            controller.begin_decision(11)?,
            a3_application::BeginResearchDecision::FinalOnly
        );

        assert!(controller.begin_diagram_decision(12).is_ok());
        assert!(controller.begin_diagram_decision(13).is_ok());
        assert!(controller.begin_diagram_decision(14).is_err());
        Ok(())
    }

    #[test]
    fn ask_research_progress_stays_on_the_owning_conversation_scale()
    -> Result<(), Box<dyn std::error::Error>> {
        let (scheduler, events) =
            JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixedClock))?;
        scheduler.submit(JobId::new(91), JobOwner::new(4), |context: JobContext| {
            if context
                .report_progress(Progress::determinate(0, 10).unwrap_or(Progress::Indeterminate))
                .is_err()
            {
                return JobCompletion::Failed;
            }
            for _round in 0..2 {
                let nested = ConversationTaskLensControl { context: &context };
                for completed in 0..=7 {
                    let progress =
                        Progress::determinate(completed, 7).unwrap_or(Progress::Indeterminate);
                    if nested.report_progress(progress).is_err() {
                        return JobCompletion::Failed;
                    }
                }
            }
            for completed in [8, 9, 10] {
                if context
                    .report_progress(
                        Progress::determinate(completed, 10).unwrap_or(Progress::Indeterminate),
                    )
                    .is_err()
                {
                    return JobCompletion::Failed;
                }
            }
            JobCompletion::Succeeded
        })?;

        let mut succeeded = false;
        while let Some(event) = events.next_timeout(Duration::from_secs(2))? {
            if event.kind() == JobEventKind::Succeeded {
                succeeded = true;
                break;
            }
            if matches!(event.kind(), JobEventKind::Failed | JobEventKind::Cancelled) {
                break;
            }
        }
        assert!(succeeded);
        Ok(())
    }

    #[test]
    fn unfinished_conversation_is_durably_failed_instead_of_remaining_running()
    -> Result<(), Box<dyn std::error::Error>> {
        let (session_id, concrete) = running_session_store(12)?;
        let store: Arc<dyn AgentSessionStore> = concrete.clone();
        let project = project();
        block_on(settle_unfinished_conversation(
            &store,
            &project,
            session_id,
            ConversationTerminal::Failed("Projektwissen konnte nicht geladen werden."),
        ))?;
        block_on(settle_unfinished_conversation(
            &store,
            &project,
            session_id,
            ConversationTerminal::Cancelled,
        ))?;

        let detail = concrete.detail();
        assert_eq!(detail.session().state(), AgentSessionState::Failed);
        assert_eq!(detail.session().revision().get(), 2);
        assert_eq!(detail.entries().len(), 2);
        assert_eq!(
            detail.entries()[1].text().as_str(),
            "Projektwissen konnte nicht geladen werden."
        );
        Ok(())
    }

    #[test]
    fn unfinished_conversation_can_be_cancelled_without_an_active_scheduler_job()
    -> Result<(), Box<dyn std::error::Error>> {
        let (session_id, concrete) = running_session_store(13)?;
        let store: Arc<dyn AgentSessionStore> = concrete.clone();

        block_on(settle_unfinished_conversation(
            &store,
            &project(),
            session_id,
            ConversationTerminal::Cancelled,
        ))?;

        let detail = concrete.detail();
        assert_eq!(detail.session().state(), AgentSessionState::Cancelled);
        assert_eq!(detail.session().revision().get(), 2);
        assert_eq!(detail.entries().len(), 2);
        assert_eq!(
            detail.entries()[1].text().as_str(),
            "Die aktuelle Verarbeitung wurde abgebrochen."
        );
        Ok(())
    }

    #[test]
    fn stale_plan_materialization_returns_to_a_readable_user_halt()
    -> Result<(), Box<dyn std::error::Error>> {
        let (session_id, concrete) = running_session_store(14)?;
        let store: Arc<dyn AgentSessionStore> = concrete.clone();

        block_on(settle_unfinished_conversation(
            &store,
            &project(),
            session_id,
            ConversationTerminal::PlanNeedsRefresh,
        ))?;

        let detail = concrete.detail();
        assert_eq!(detail.session().state(), AgentSessionState::AwaitingUser);
        assert_eq!(
            detail.entries()[1].kind(),
            AgentSessionEntryKind::AssistantSummary
        );
        assert!(detail.entries()[1].text().as_str().contains("Projektstand"));
        assert!(detail.entries()[1].text().as_str().contains("neu geprüft"));
        Ok(())
    }

    #[test]
    fn interrupted_agent_start_returns_to_the_exact_reviewed_plan()
    -> Result<(), Box<dyn std::error::Error>> {
        let session_id = AgentSessionId::from_bytes([15; 32]);
        let timestamp = AgentSessionTimestamp::from_unix_millis(1)?;
        let session = AgentSession::from_parts(
            session_id,
            AgentSessionRevision::INITIAL,
            AgentSessionTitle::try_from_string("Plan".to_owned())?,
            AgentSessionMode::Agent,
            AgentSessionState::Running,
            timestamp,
            timestamp,
            Some(AgentSessionSequence::FIRST),
            None,
            Some(1),
            false,
        );
        let entry = AgentSessionEntry::try_new(
            session_id,
            AgentSessionSequence::FIRST,
            AgentSessionEntryKind::Plan,
            AgentSessionText::try_from_string("Planinhalt".to_owned())?,
            timestamp,
            None,
            None,
            Some(1),
        )?;
        let concrete = Arc::new(MemorySessionStore::new(session, entry));
        let store: Arc<dyn AgentSessionStore> = concrete.clone();

        block_on(settle_unfinished_conversation(
            &store,
            &project(),
            session_id,
            ConversationTerminal::AgentStartInterrupted,
        ))?;

        let detail = concrete.detail();
        assert_eq!(detail.session().mode(), AgentSessionMode::Plan);
        assert_eq!(
            detail.session().state(),
            AgentSessionState::AwaitingPlanReview
        );
        assert_eq!(detail.session().current_plan_revision(), Some(1));
        assert_eq!(
            detail.entries()[1].kind(),
            AgentSessionEntryKind::AssistantSummary
        );
        assert_eq!(detail.entries()[1].plan_revision(), None);
        assert!(
            detail.entries()[1]
                .text()
                .as_str()
                .contains("erneut gestartet")
        );
        Ok(())
    }

    #[derive(Debug)]
    struct FixedClock;

    impl JobClock for FixedClock {
        fn now(&self) -> JobTimestamp {
            JobTimestamp::from_millis(1)
        }
    }

    #[derive(Debug)]
    struct MemorySessionStore {
        value: Mutex<(AgentSession, Vec<AgentSessionEntry>)>,
    }

    impl MemorySessionStore {
        fn new(session: AgentSession, entry: AgentSessionEntry) -> Self {
            Self {
                value: Mutex::new((session, vec![entry])),
            }
        }

        fn detail(&self) -> AgentSessionDetail {
            let value = self.value.lock().unwrap_or_else(|error| error.into_inner());
            AgentSessionDetail::new(value.0.clone(), value.1.clone(), false)
                .unwrap_or_else(|_| unreachable!())
        }
    }

    impl AgentSessionStore for MemorySessionStore {
        fn create_session<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session: &'a AgentSession,
            _first_entry: Option<&'a AgentSessionEntry>,
            _command: Option<&'a a3_domain::SlashCommandInvocation>,
        ) -> AgentSessionStoreFuture<'a, ()> {
            Box::pin(async { Err(AgentSessionStoreFailure::InvalidInput) })
        }

        fn append_session_revision<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            expected_revision: AgentSessionRevision,
            session: &'a AgentSession,
            entry: Option<&'a AgentSessionEntry>,
            _command: Option<&'a a3_domain::SlashCommandInvocation>,
        ) -> AgentSessionStoreFuture<'a, ()> {
            Box::pin(async move {
                let mut value = self.value.lock().unwrap_or_else(|error| error.into_inner());
                if value.0.revision() != expected_revision {
                    return Err(AgentSessionStoreFailure::Conflict);
                }
                value.0 = session.clone();
                if let Some(entry) = entry {
                    value.1.push(entry.clone());
                }
                Ok(())
            })
        }

        fn list_sessions<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _query: &'a AgentSessionListQuery,
        ) -> AgentSessionStoreFuture<'a, AgentSessionPage> {
            Box::pin(async { AgentSessionPage::new(Vec::new(), false) })
        }

        fn load_session<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
            _before_sequence: Option<u64>,
            _limit: u16,
        ) -> AgentSessionStoreFuture<'a, Option<AgentSessionDetail>> {
            Box::pin(async move { Ok(Some(self.detail())) })
        }

        fn load_session_commands<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
            _before_sequence: Option<u64>,
            _limit: u16,
        ) -> AgentSessionStoreFuture<'a, Vec<a3_application::AgentSessionCommandPresentation>>
        {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn enqueue_message<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _expected_session_revision: AgentSessionRevision,
            _message: &'a a3_domain::AgentQueuedMessage,
        ) -> AgentSessionStoreFuture<'a, a3_application::AgentSessionQueue> {
            Box::pin(async { Err(AgentSessionStoreFailure::InvalidInput) })
        }

        fn load_message_queue<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
        ) -> AgentSessionStoreFuture<'a, a3_application::AgentSessionQueue> {
            Box::pin(async {
                a3_application::AgentSessionQueue::new(
                    a3_domain::AgentSessionQueueRevision::EMPTY,
                    false,
                    Vec::new(),
                )
            })
        }

        fn transition_queued_message<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
            _expected_queue_revision: a3_domain::AgentSessionQueueRevision,
            _message_id: a3_domain::AgentQueuedMessageId,
            _state: a3_domain::AgentQueuedMessageState,
        ) -> AgentSessionStoreFuture<'a, a3_application::AgentSessionQueue> {
            Box::pin(async { Err(AgentSessionStoreFailure::InvalidInput) })
        }

        fn set_message_queue_paused<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
            _expected_queue_revision: a3_domain::AgentSessionQueueRevision,
            _paused: bool,
        ) -> AgentSessionStoreFuture<'a, a3_application::AgentSessionQueue> {
            Box::pin(async { Err(AgentSessionStoreFailure::InvalidInput) })
        }

        fn delete_presentation<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            _session_id: AgentSessionId,
            _expected_revision: AgentSessionRevision,
            _tombstone: &'a AgentSession,
        ) -> AgentSessionStoreFuture<'a, ()> {
            Box::pin(async { Err(AgentSessionStoreFailure::InvalidInput) })
        }
    }

    fn running_session_store(
        id_byte: u8,
    ) -> Result<(AgentSessionId, Arc<MemorySessionStore>), Box<dyn std::error::Error>> {
        let session_id = AgentSessionId::from_bytes([id_byte; 32]);
        let timestamp = AgentSessionTimestamp::from_unix_millis(1)?;
        let session = AgentSession::from_parts(
            session_id,
            AgentSessionRevision::INITIAL,
            AgentSessionTitle::try_from_string("Ask".to_owned())?,
            AgentSessionMode::Ask,
            AgentSessionState::Running,
            timestamp,
            timestamp,
            Some(AgentSessionSequence::FIRST),
            None,
            None,
            false,
        );
        let entry = AgentSessionEntry::try_new(
            session_id,
            AgentSessionSequence::FIRST,
            AgentSessionEntryKind::UserMessage,
            AgentSessionText::try_from_string("Was macht A^3?".to_owned())?,
            timestamp,
            None,
            None,
            None,
        )?;
        Ok((
            session_id,
            Arc::new(MemorySessionStore::new(session, entry)),
        ))
    }

    fn project() -> ProjectIdentity {
        let repository_id = RepositoryId::from_bytes([1; 32]);
        ProjectIdentity::new(
            RepositoryIdentity::new(
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
                )
                .unwrap_or_else(|_| unreachable!()),
                None,
            ),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                a3_domain::CanonicalDirectory::from_canonicalized(
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
                )
                .unwrap_or_else(|_| unreachable!()),
            ),
            a3_domain::GitHead::Unborn {
                reference: a3_domain::GitReferenceName::try_from_full_name("refs/heads/main")
                    .unwrap_or_else(|_| unreachable!()),
            },
        )
        .unwrap_or_else(|_| unreachable!())
    }
}

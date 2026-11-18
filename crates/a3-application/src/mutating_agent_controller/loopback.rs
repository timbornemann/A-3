//! Core-owned HTTP verification through the existing mutation lifecycle.

use super::*;
use crate::{
    AuthorizedLoopbackVerification, EvaluatedPolicyAction, GoalContractStore,
    LoopbackVerificationFailure as Failure, LoopbackVerificationOutcome,
    LoopbackVerificationRunner, MachineEffectScope, PreparedLoopbackVerification,
    RepositoryIndexControlError, RepositoryIndexObservation, TaskLedgerStore,
    model_wire::phase_elapsed_timestamp,
};
use a3_domain::{
    AgentMutationAttempt, AgentPermissionSettings, AgentToolAttemptNumber, ApprovalStatus,
    LoopbackHttpEvidence, LoopbackHttpEvidenceContext, LoopbackHttpProcess,
    LoopbackProcessSettings, PolicyAction, PolicyDecisionReason, ProcessSpec,
    TaskStepCancellationReason,
};
use std::collections::BTreeSet;

mod approval;
mod execution;
use approval::ContinuedLoopbackApproval;
pub use approval::{LoopbackApprovalAdmission, LoopbackApprovalSequence};
use execution::{HttpExecutionOutcome, PreparedHttpExecution};

/// Core-owned durable identities for one exact ordered HTTP probe approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackProbeApproval {
    /// Already-issued exact policy request for this probe.
    pub request_id: ApprovalRequestId,
    /// New decision identity for consuming the stored grant.
    pub decision_id: PolicyDecisionId,
    /// New journal identity for that decision.
    pub event_id: RunEventId,
}

/// Bounded, trusted inputs for one HTTP verification of an active candidate step.
#[derive(Debug, Clone, Copy)]
pub struct LoopbackVerificationRequest<'a> {
    /// Active step. The recipe comes from its immutable verification specification.
    pub step_id: TaskStepId,
    /// Exact Core-selected limits used in the issued process approval.
    pub settings: LoopbackProcessSettings,
    /// Already-issued requests in exact recipe order.
    pub probe_approvals: &'a [LoopbackProbeApproval],
    /// Stable identities, including the issued start approval request.
    pub ids: MutationExecutionIds,
    /// Owner-observed time at entry, advanced with monotonic elapsed time.
    pub observed_at: AgentRunTimestamp,
}

/// Core-owned coordinates for continuing a settled HTTP step after a process restart.
#[derive(Debug, Clone, Copy)]
pub struct LoopbackVerificationResume {
    /// The same active step that owned the settled effect.
    pub step_id: TaskStepId,
    /// Stable completion identities, including the original tool run.
    pub ids: MutationExecutionIds,
    /// Owner-observed time within the original run's deadline.
    pub observed_at: AgentRunTimestamp,
}

/// Existing current Goal and Ledger authorities read under the mutation lease.
#[derive(Debug, Clone, Copy)]
pub struct LoopbackVerificationAuthority<'a> {
    goal: &'a dyn GoalContractStore,
    ledger: &'a dyn TaskLedgerStore,
}

impl<'a> LoopbackVerificationAuthority<'a> {
    /// Uses the same stores as task admission and execution recovery.
    #[must_use]
    pub const fn new(goal: &'a dyn GoalContractStore, ledger: &'a dyn TaskLedgerStore) -> Self {
        Self { goal, ledger }
    }
}

impl ExecuteMutatingAgentAction<'_> {
    /// Runs one closed HTTP recipe for an already active candidate step with stored exact grants.
    ///
    /// The caller owns approval presentation and any transition out of approval wait.
    /// This entry point accepts only Execute and never asks the model for a process or probe.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_loopback_verification<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        request: LoopbackVerificationRequest<'_>,
        authority: LoopbackVerificationAuthority<'_>,
        runner: &dyn LoopbackVerificationRunner,
        workspace_policy: &WorkspacePolicy,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl,
    {
        self.execute_prepared_loopback(
            project,
            run,
            ledger,
            ledger_version,
            request,
            authority,
            runner,
            workspace_policy,
            context_seed,
            index_compiler,
            control,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_prepared_loopback<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        request: LoopbackVerificationRequest<'_>,
        authority: LoopbackVerificationAuthority<'_>,
        runner: &dyn LoopbackVerificationRunner,
        workspace_policy: &WorkspacePolicy,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
        continued: Option<ContinuedLoopbackApproval<'_>>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl,
    {
        let started = Instant::now();
        if !runner.supports_owned_cleanup() {
            return Err(admission(Failure::Unsupported));
        }
        let permission_store = self.permissions.ok_or(admission(Failure::Unavailable))?;
        let expected_state = if continued.is_some() {
            AgentControllerState::AwaitApproval
        } else {
            AgentControllerState::Execute
        };
        if run.state() != expected_state {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let definition = loopback_step(run, ledger, request.step_id)?.definition();
        let spec_id = definition.verification_spec().id();
        let VerificationTarget::LoopbackHttp(recipe) = definition.verification_spec().target()
        else {
            return Err(MutationControllerFailure::AnchorMismatch);
        };
        let start = recipe
            .process_spec(
                run.id(),
                project.worktree().id(),
                request.step_id,
                request.settings,
            )
            .map_err(|_| admission(Failure::Denied))?;
        let lease_fingerprint =
            MutationActionFingerprint::from_bytes(*start.policy_action().fingerprint().as_bytes());
        let lease =
            self.coordinator
                .try_acquire(run.id(), project.worktree().id(), lease_fingerprint)?;
        require_dispatch_room(run, request, started, control)?;
        self.require_loopback_authority(
            project,
            run,
            ledger,
            *ledger_version,
            request.step_id,
            authority,
            context_seed,
        )
        .await?;
        self.require_unreplayed_loopback(project, run, request.step_id)
            .await?;
        let snapshot = run.current_snapshot_id();
        let deadline = loopback_index_deadline(run, request.observed_at, started)?;
        let prepared = self
            .refresh_loopback_scope(
                project,
                run,
                deadline,
                start,
                recipe,
                index_compiler,
                control,
            )
            .await?;
        if !prepared.supported_by(runner) {
            return Err(admission(Failure::Unsupported));
        }
        let fingerprint = prepared.effect_fingerprint();
        let effect_resource = prepared.effect_resource();
        let script = prepared.script().clone();
        let recipe_id = recipe.id();
        let diagnostic_recipe = recipe.clone();
        let process = LoopbackHttpProcess::from_spec(prepared.start())
            .map_err(|_| admission(Failure::Denied))?;
        let settings = permission_store
            .load_agent_permissions()
            .await
            .map_err(|_| admission(Failure::Unavailable))?;
        let mut resumed_run = run.clone();
        let resumption = if let Some(continued) = &continued {
            continued.validate(
                self.approval,
                project,
                inspection_context(run, request.step_id, spec_id, snapshot),
                &prepared,
                workspace_policy,
                settings,
            )?;
            let at = require_dispatch_room(run, request, started, control)?;
            let advance = AdvanceAgentController.execute(
                &mut resumed_run,
                AgentControllerSignal::ApprovalGranted,
                continued.resume_event_id(),
                snapshot,
                at,
                false,
            )?;
            let mut next_ledger = ledger.clone();
            next_ledger.resume_step_after_approval(
                request.step_id,
                run.id(),
                ledger_timestamp(at)?,
            )?;
            Some((next_ledger, advance))
        } else {
            None
        };
        let policy = self
            .prepare_loopback_policy(
                project,
                &resumed_run,
                &prepared,
                workspace_policy,
                settings,
                request,
                started,
            )
            .await?;
        let decisions: Vec<_> = policy
            .iter()
            .map(|(_, evaluation)| evaluation.decision().clone())
            .collect();
        let decision_ids: Vec<_> = decisions.iter().map(PolicyDecision::id).collect();
        let authorized = prepared.authorize(&decisions).map_err(admission)?;
        require_dispatch_room(run, request, started, control)?;
        if let (Some((next_ledger, advance)), Some(continued)) = (resumption, &continued) {
            if permission_store
                .load_agent_permissions()
                .await
                .map_err(|_| admission(Failure::Unavailable))?
                != settings
            {
                return Err(MutationControllerFailure::PermissionsChanged);
            }
            self.require_loopback_authority(
                project,
                run,
                ledger,
                *ledger_version,
                request.step_id,
                authority,
                context_seed,
            )
            .await?;
            self.require_current_loopback_publication(project, snapshot)
                .await?;
            let now = require_dispatch_room(run, request, started, control)?;
            continued.validate_current(self.approval, project, now)?;
            *ledger_version = self
                .action_store
                .commit_ledger_action(
                    project,
                    *ledger_version,
                    run.last_event_sequence(),
                    &next_ledger,
                    &resumed_run,
                    advance.event(),
                )
                .await?;
            *ledger = next_ledger;
            *run = resumed_run;
        }
        for (next_run, evaluation) in &policy {
            PersistPolicyEvaluation::new(self.policy_store)
                .execute(project, run.last_event_sequence(), next_run, evaluation)
                .await?;
            *run = next_run.clone();
        }
        self.require_loopback_authority(
            project,
            run,
            ledger,
            *ledger_version,
            request.step_id,
            authority,
            context_seed,
        )
        .await?;
        let started_at = require_dispatch_room(run, request, started, control)?;
        let scope = MachineEffectScope::new(
            AgentMutationKind::MachineProcess,
            request.step_id,
            effect_resource,
            "python".to_owned(),
            Some(script.content_hash()),
            None,
        )
        .map_err(MutationControllerFailure::MutationStartStore)?;
        let attempt = self
            .recovery
            .begin_machine_mutation_attempt(
                project,
                run.id(),
                snapshot,
                request.ids.tool_run_id,
                fingerprint,
                &scope,
                started_at,
            )
            .await
            .map_err(MutationControllerFailure::MutationStartStore)?
            .tool_attempt()
            .attempt();
        let pre_dispatch = async {
            self.require_current_loopback_publication(project, snapshot)
                .await?;
            let current_permissions = permission_store
                .load_agent_permissions()
                .await
                .map_err(|_| admission(Failure::Unavailable))?;
            if current_permissions != settings {
                return Err(MutationControllerFailure::PermissionsChanged);
            }
            let now = require_dispatch_room(run, request, started, control)?;
            if let Some(continued) = &continued {
                continued.validate_current(self.approval, project, now)?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = pre_dispatch {
            let failure = match &error {
                MutationControllerFailure::LoopbackAdmission(failure) => classify(*failure),
                _ => MutationFailureClass::Denied,
            };
            let at = elapsed(request, started)?;
            self.settle_loopback_failure(
                project,
                run,
                request,
                attempt,
                AgentMutationDisposition::NotApplied,
                failure,
                at,
            )
            .await?;
            if failure == MutationFailureClass::Cancelled {
                return self
                    .cancel_loopback_step(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        request.step_id,
                        request.ids.resolution_transition_event_id,
                        at,
                        control,
                    )
                    .await;
            }
            return Err(error);
        }

        let observed = authorized.verify(runner, project, control).await;
        let mut http_mismatch = None;
        let (report, disposition) = match observed {
            HttpExecutionOutcome::Mismatched { cleanup, mismatch } => {
                // Both variants establish the entire owned lifecycle; Unknown never enters here.
                match cleanup {
                    crate::LoopbackStartupCleanupProof::ProcessGroupsAndEndpoints
                    | crate::LoopbackStartupCleanupProof::WindowsJobsAndEndpoints => {}
                }
                http_mismatch = Some(mismatch);
                (
                    Err(Failure::InvalidResponse),
                    AgentMutationDisposition::Applied,
                )
            }
            HttpExecutionOutcome::NotStarted(failure) => {
                (Err(failure), AgentMutationDisposition::NotApplied)
            }
            HttpExecutionOutcome::Stopped(result) => (result, AgentMutationDisposition::Applied),
            HttpExecutionOutcome::UnknownCleanup => {
                self.settle_loopback_failure(
                    project,
                    run,
                    request,
                    attempt,
                    AgentMutationDisposition::Unknown(MutationReconciliation::Required),
                    MutationFailureClass::ToolUnavailable,
                    elapsed(request, started)?,
                )
                .await?;
                return Ok(MutationControllerOutcome::ReconciliationRequired {
                    tool_run_id: request.ids.tool_run_id,
                    attempt,
                    snapshot_id: snapshot,
                });
            }
        };
        let (fresh, failure) = if disposition == AgentMutationDisposition::NotApplied {
            (None, report.as_ref().err().copied().map(classify))
        } else if AgentControllerControl::is_cancelled(control) {
            (None, Some(MutationFailureClass::Cancelled))
        } else {
            let index_control = LoopbackIndexControl {
                inner: control,
                deadline,
            };
            let batch = loopback_index_batch()?;
            match self
                .refresh
                .execute(project, &batch, index_compiler, &index_control)
                .await
            {
                Ok(refresh) if refresh.published_index().run().snapshot_id() == snapshot => (
                    Some(refresh.published_index().clone()),
                    report.as_ref().err().copied().map(classify),
                ),
                Ok(_) => (None, Some(MutationFailureClass::ContextStale)),
                Err(_) if AgentControllerControl::is_cancelled(control) => {
                    (None, Some(MutationFailureClass::Cancelled))
                }
                Err(_) if Instant::now() >= deadline => {
                    (None, Some(MutationFailureClass::TimedOut))
                }
                Err(_) => (None, Some(MutationFailureClass::IndexRefreshFailed)),
            }
        };
        let observed_at = elapsed(request, started)?;
        let time_left = run_time_remaining(run, observed_at)?;
        let failure = finished_loopback_failure(
            AgentControllerControl::is_cancelled(control),
            time_left,
            failure,
        );
        let report = match (report, fresh, failure) {
            (Ok(report), Some(index), None) if report.matches_decisions(&decision_ids) => {
                (report, index)
            }
            (_, fresh, failure) => {
                let failure = failure.unwrap_or(MutationFailureClass::VerificationFailed);
                if let (Some(mismatch), Some(_), MutationFailureClass::VerificationFailed) =
                    (http_mismatch, fresh, failure)
                {
                    let diagnostic = a3_domain::LoopbackHttpMismatchDiagnostic::new(
                        LoopbackHttpEvidenceContext {
                            verification_run_id: request.ids.verification_run_id,
                            spec_id,
                            run_id: run.id(),
                            tool_run_id: request.ids.tool_run_id,
                            step_id: request.step_id,
                            snapshot_id: snapshot,
                        },
                        attempt,
                        project.worktree().id(),
                        &diagnostic_recipe,
                        script.clone(),
                        process,
                        mismatch,
                        decision_ids.clone(),
                    )
                    .map_err(|_| MutationControllerFailure::InvalidToolResult)?;
                    let expected = run.last_event_sequence();
                    let mut next = run.clone();
                    let event = next.record(
                        request.ids.tool_event_id,
                        RunEventKind::ToolAction,
                        RunEventPayload::new(
                            RunEventCode::VerificationFailure,
                            Some(RunEventOutcome::Failed),
                            None,
                        ),
                        snapshot,
                        Some(RunEventSubject::Tool(request.ids.tool_run_id)),
                        observed_at,
                    )?;
                    self.recovery
                        .complete_loopback_mismatch(
                            project,
                            expected,
                            &next,
                            &event,
                            &diagnostic,
                            VERIFICATION_EVIDENCE_TIMEOUT.min(time_left),
                            control,
                        )
                        .await
                        .map_err(MutationControllerFailure::MutationResultStore)?;
                    *run = next;
                } else {
                    self.settle_loopback_failure(
                        project,
                        run,
                        request,
                        attempt,
                        disposition,
                        failure,
                        observed_at,
                    )
                    .await?;
                }
                let _ = lease.record_failure(failure);
                if failure == MutationFailureClass::Cancelled {
                    return self
                        .cancel_loopback_step(
                            project,
                            run,
                            ledger,
                            ledger_version,
                            request.step_id,
                            request.ids.resolution_transition_event_id,
                            observed_at,
                            control,
                        )
                        .await;
                }
                self.fail_active_step_and_transition(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    request.step_id,
                    "closed HTTP verification did not produce a current observation",
                    AgentControllerSignal::ExecutionVerificationFailed {
                        correction_allowed: false,
                    },
                    request.ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await?;
                return Ok(MutationControllerOutcome::Stopped {
                    failure,
                    snapshot_id: snapshot,
                });
            }
        };
        let (report, index) = report;
        let receipt = report
            .into_receipt(
                LoopbackHttpEvidenceContext {
                    verification_run_id: request.ids.verification_run_id,
                    spec_id,
                    run_id: run.id(),
                    tool_run_id: request.ids.tool_run_id,
                    step_id: request.step_id,
                    snapshot_id: snapshot,
                },
                recipe_id,
                script,
                process,
            )
            .map_err(|_| MutationControllerFailure::InvalidToolResult)?;
        let expected_sequence = run.last_event_sequence();
        let mut next_run = run.clone();
        let event = next_run.record(
            request.ids.tool_event_id,
            RunEventKind::ToolAction,
            RunEventPayload::new(RunEventCode::None, Some(RunEventOutcome::Succeeded), None),
            snapshot,
            Some(RunEventSubject::Tool(request.ids.tool_run_id)),
            observed_at,
        )?;
        self.recovery
            .complete_loopback_mutation(
                project,
                expected_sequence,
                &next_run,
                &event,
                attempt,
                &receipt,
                VERIFICATION_EVIDENCE_TIMEOUT.min(time_left),
                control,
            )
            .await
            .map_err(MutationControllerFailure::MutationResultStore)?;
        *run = next_run;
        let observed_at = elapsed(request, started)?;
        self.finish_settled_loopback(
            project,
            run,
            ledger,
            ledger_version,
            LoopbackVerificationResume {
                step_id: request.step_id,
                ids: request.ids,
                observed_at,
            },
            context_seed,
            control,
            &lease,
            &index,
            receipt,
        )
        .await
    }

    /// Completes the same active step from the current journal tail and its stored HTTP receipt.
    ///
    /// The exact snapshot must survive a fresh worktree observation. No process capability,
    /// policy grant or model is needed to evaluate the already observed responses.
    #[allow(clippy::too_many_arguments)]
    pub async fn resume_loopback_verification<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        request: LoopbackVerificationResume,
        authority: LoopbackVerificationAuthority<'_>,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl + RepositoryIndexControl,
    {
        let started = Instant::now();
        if run.state() != AgentControllerState::Execute {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let step = loopback_step(run, ledger, request.step_id)?;
        let spec = step.definition().verification_spec();
        let VerificationTarget::LoopbackHttp(recipe) = spec.target() else {
            return Err(MutationControllerFailure::AnchorMismatch);
        };
        if AgentControllerControl::is_cancelled(control) {
            return Err(admission(Failure::Cancelled));
        }
        if request.observed_at < run.updated_at() {
            return Err(MutationControllerFailure::InvalidTimestamp);
        }
        let deadline = started
            .checked_add(run_time_remaining(run, request.observed_at)?)
            .ok_or(MutationControllerFailure::InvalidTimestamp)?;
        if Instant::now() >= deadline {
            return Err(admission(Failure::TimedOut));
        }
        let snapshot = run.current_snapshot_id();
        let selected = self
            .find_loopback_effect(project, run, request.step_id)
            .await?
            .ok_or(MutationControllerFailure::AnchorMismatch)?;
        let (attempt, scope) = &selected;
        let tool = attempt.tool_attempt();
        if tool.status() != AgentToolAttemptStatus::Succeeded {
            return Err(MutationControllerFailure::LoopbackRecoveryPending);
        }
        if tool.tool_run_id() != request.ids.tool_run_id
            || tool.snapshot_id() != snapshot
            || step.attempts().last().is_none_or(|active| {
                tool.started_at().unix_millis() < active.started_at().unix_millis()
            })
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let lease = self.coordinator.try_acquire(
            run.id(),
            project.worktree().id(),
            attempt.fingerprint(),
        )?;
        self.require_loopback_authority(
            project,
            run,
            ledger,
            *ledger_version,
            request.step_id,
            authority,
            context_seed,
        )
        .await?;
        if self
            .find_loopback_effect(project, run, request.step_id)
            .await?
            .as_ref()
            != Some(&selected)
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let evidence_id = self
            .recovery
            .load_settled_loopback_evidence_id(
                project,
                run.id(),
                tool.tool_run_id(),
                tool.attempt(),
            )
            .await
            .map_err(MutationControllerFailure::MutationResultStore)?
            .ok_or(MutationControllerFailure::LoopbackRecoveryPending)?;

        let index_control = LoopbackIndexControl {
            inner: control,
            deadline,
        };
        let batch = RepositoryChangeBatch::full_rescan(
            Vec::new(),
            crate::RepositoryRescanReason::Explicit,
        )?;
        let fresh = self
            .refresh
            .execute(project, &batch, index_compiler, &index_control)
            .await?;
        if fresh.published_index().run().snapshot_id() != snapshot {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let now = phase_elapsed_timestamp(started, request.observed_at)
            .ok_or(MutationControllerFailure::InvalidTimestamp)?;
        let time_left = run_time_remaining(run, now)?;
        if time_left.is_zero() {
            return Err(admission(Failure::TimedOut));
        }
        let state = self
            .evidence_store
            .load_verification_state(
                project,
                run.goal_contract().task_id(),
                &[evidence_id],
                snapshot,
                VERIFICATION_EVIDENCE_TIMEOUT.min(time_left),
                control,
            )
            .await?;
        let [VerificationEvidence::LoopbackHttp(receipt)] = state.evidence() else {
            return Err(MutationControllerFailure::LoopbackRecoveryPending);
        };
        let context = receipt.context();
        let expected_start = recipe
            .process_spec(
                run.id(),
                project.worktree().id(),
                request.step_id,
                receipt.process().settings(),
            )
            .map_err(|_| MutationControllerFailure::AnchorMismatch)?;
        let expected = PreparedHttpExecution::new(
            expected_start,
            receipt.script().clone(),
            snapshot,
            recipe.clone(),
        )
        .map_err(|_| MutationControllerFailure::AnchorMismatch)?;
        if state.published_index().run().snapshot_id() != snapshot
            || receipt.id() != evidence_id
            || context.run_id != run.id()
            || context.step_id != request.step_id
            || context.spec_id != spec.id()
            || context.tool_run_id != tool.tool_run_id()
            || context.snapshot_id != snapshot
            || receipt.recipe_id() != recipe.id()
            || receipt.script().path() != recipe.script()
            || scope.expected() != Some(receipt.script().content_hash())
            || scope.resource() != expected.effect_resource()
            || attempt.fingerprint() != expected.effect_fingerprint()
            || expected.start().specification_id() != receipt.process_specification()
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let observed_at = phase_elapsed_timestamp(started, request.observed_at)
            .ok_or(MutationControllerFailure::InvalidTimestamp)?;
        self.finish_settled_loopback(
            project,
            run,
            ledger,
            ledger_version,
            LoopbackVerificationResume {
                observed_at,
                ..request
            },
            context_seed,
            control,
            &lease,
            state.published_index(),
            receipt.clone(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_settled_loopback<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        request: LoopbackVerificationResume,
        context_seed: &MutationContextSeed,
        control: &C,
        lease: &crate::WorktreeMutationLease<'_>,
        index: &PublishedIndex,
        receipt: LoopbackHttpEvidence,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl + ContextCompileControl,
    {
        let observed_at = request.observed_at;
        let snapshot = run.current_snapshot_id();
        if AgentControllerControl::is_cancelled(control) {
            return self
                .cancel_loopback_step(
                    project,
                    run,
                    ledger,
                    ledger_version,
                    request.step_id,
                    request.ids.resolution_transition_event_id,
                    observed_at,
                    control,
                )
                .await;
        }
        if run_time_remaining(run, observed_at)?.is_zero() {
            self.fail_active_step_and_transition(
                project,
                run,
                ledger,
                ledger_version,
                request.step_id,
                "closed HTTP verification exhausted the run deadline before ledger completion",
                AgentControllerSignal::ExecutionVerificationFailed {
                    correction_allowed: false,
                },
                request.ids.resolution_transition_event_id,
                observed_at,
                control,
            )
            .await?;
            return Ok(MutationControllerOutcome::Stopped {
                failure: MutationFailureClass::TimedOut,
                snapshot_id: snapshot,
            });
        }
        self.verify_evidence(
            project,
            run,
            ledger,
            ledger_version,
            request.step_id,
            request.ids,
            observed_at,
            context_seed,
            control,
            lease,
            index,
            VerificationEvidence::LoopbackHttp(receipt),
        )
        .await
    }

    async fn require_current_loopback_publication(
        self,
        project: &ProjectIdentity,
        snapshot: SnapshotId,
    ) -> Result<(), MutationControllerFailure> {
        if !self
            .refresh
            .has_current_published_snapshot(project, snapshot)
            .await
            .map_err(|_| admission(Failure::Unavailable))?
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn refresh_loopback_scope<C>(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        deadline: Instant,
        start: ProcessSpec,
        recipe: &a3_domain::LoopbackHttpRecipe,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
    ) -> Result<PreparedHttpExecution, MutationControllerFailure>
    where
        C: AgentControllerControl + RepositoryIndexControl,
    {
        let index_control = LoopbackIndexControl {
            inner: control,
            deadline,
        };
        let batch = loopback_index_batch()?;
        let current = self
            .refresh
            .execute(project, &batch, index_compiler, &index_control)
            .await
            .map_err(|_| {
                admission(if AgentControllerControl::is_cancelled(control) {
                    Failure::Cancelled
                } else if Instant::now() >= deadline {
                    Failure::TimedOut
                } else {
                    Failure::Unavailable
                })
            })?;
        let snapshot = run.current_snapshot_id();
        if current.published_index().run().snapshot_id() != snapshot {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let script = current
            .published_index()
            .publication()
            .graph()
            .files()
            .iter()
            .find(|file| file.path() == recipe.script())
            .ok_or(MutationControllerFailure::AnchorMismatch)?
            .clone();
        PreparedHttpExecution::new(start, script, snapshot, recipe.clone()).map_err(admission)
    }

    #[allow(clippy::too_many_arguments)]
    async fn require_loopback_authority(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        ledger: &TaskLedger,
        version: TaskLedgerStoreVersion,
        step_id: TaskStepId,
        authority: LoopbackVerificationAuthority<'_>,
        seed: &MutationContextSeed,
    ) -> Result<(), MutationControllerFailure> {
        let task = run.goal_contract().task_id();
        let goal = authority
            .goal
            .load_current_goal_contract(project, task)
            .await
            .map_err(|_| admission(Failure::Unavailable))?;
        let stored = authority
            .ledger
            .load_task_ledger(project, task)
            .await
            .map_err(|_| admission(Failure::Unavailable))?;
        let persisted_run = self.journal.load_agent_run(project, run.id()).await?;
        let definition = loopback_step(run, ledger, step_id)?.definition();
        if goal.as_ref() != Some(&seed.goal_contract)
            || persisted_run.as_ref() != Some(run)
            || stored
                .as_ref()
                .is_none_or(|stored| stored.version() != version || stored.ledger() != ledger)
            || seed.goal_contract.reference() != run.goal_contract()
            || seed.goal_contract.original().is_none()
            || run.model_profile() != Some(seed.model_profile.reference())
            || definition.acceptance_criteria().is_empty()
            || crate::AdmitAgentGoalExecution
                .execute_task(&seed.goal_contract, ledger)
                .is_err()
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        Ok(())
    }

    async fn require_unreplayed_loopback(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        step: TaskStepId,
    ) -> Result<(), MutationControllerFailure> {
        if self
            .find_loopback_effect(project, run, step)
            .await?
            .is_some()
        {
            return Err(MutationControllerFailure::LoopbackRecoveryPending);
        }
        Ok(())
    }

    async fn find_loopback_effect(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        step: TaskStepId,
    ) -> Result<Option<(AgentMutationAttempt, MachineEffectScope)>, MutationControllerFailure> {
        let mut selected = None;
        for attempt in self
            .recovery
            .load_agent_mutation_attempts(project, run.id())
            .await
            .map_err(MutationControllerFailure::MutationStartStore)?
        {
            if !attempt.disposition().permits_future_mutation() {
                return Err(MutationControllerFailure::MutationStartStore(
                    AgentRecoveryStoreFailure::MutationReconciliationRequired,
                ));
            }
            if attempt.kind() != AgentMutationKind::MachineProcess
                || attempt.disposition() == AgentMutationDisposition::NotApplied
            {
                continue;
            }
            let tool = attempt.tool_attempt();
            let scope = self
                .recovery
                .load_machine_effect_scope(project, tool.tool_run_id(), tool.attempt())
                .await
                .map_err(MutationControllerFailure::MutationStartStore)?
                .ok_or(MutationControllerFailure::MutationStartStore(
                    AgentRecoveryStoreFailure::InvalidStoredData,
                ))?;
            if scope.step() == step && scope.expected().is_some() {
                if attempt.disposition() != AgentMutationDisposition::Applied {
                    return Err(MutationControllerFailure::MutationStartStore(
                        AgentRecoveryStoreFailure::MutationReconciliationRequired,
                    ));
                }
                if selected.is_some() {
                    return Err(MutationControllerFailure::MutationStartStore(
                        AgentRecoveryStoreFailure::InvalidStoredData,
                    ));
                }
                selected = Some((attempt, scope));
            }
        }
        Ok(selected)
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_loopback_policy(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        prepared: &PreparedHttpExecution,
        workspace_policy: &WorkspacePolicy,
        settings: AgentPermissionSettings,
        request: LoopbackVerificationRequest<'_>,
        started: Instant,
    ) -> Result<Vec<(AgentRun, EvaluatedPolicyAction)>, MutationControllerFailure> {
        let actions = prepared.policy_actions();
        if request.probe_approvals.len() + 1 != actions.len() {
            return Err(admission(Failure::Denied));
        }
        let start_ids = LoopbackProbeApproval {
            request_id: request.ids.approval_request_id,
            decision_id: request.ids.policy_decision_id,
            event_id: request.ids.policy_event_id,
        };
        let mut request_ids = BTreeSet::new();
        let mut decision_ids = BTreeSet::new();
        let mut event_ids = BTreeSet::new();
        let mut next_run = run.clone();
        let mut evaluations = Vec::with_capacity(1 + request.probe_approvals.len());
        for (index, ids) in std::iter::once(&start_ids)
            .chain(request.probe_approvals)
            .enumerate()
        {
            if !request_ids.insert(ids.request_id)
                || !decision_ids.insert(ids.decision_id)
                || !event_ids.insert(ids.event_id)
            {
                return Err(admission(Failure::Denied));
            }
            let action = actions.get(index).ok_or(admission(Failure::Denied))?;
            let mut grant = self
                .policy_store
                .load_approval_for_request(project, ids.request_id)
                .await?
                .ok_or(admission(Failure::Denied))?;
            let now = elapsed(request, started)?;
            if grant.request_id() != ids.request_id
                || grant.status_at(now) != ApprovalStatus::Active
                || now < grant.granted_at()
            {
                return Err(admission(Failure::Denied));
            }
            let timing = PolicyEvaluationTiming::new(now, now)
                .map_err(|_| MutationControllerFailure::InvalidTimestamp)?;
            let expires = grant.expires_at();
            let evaluation = EvaluateActionPolicy::new()
                .with_permissions(settings)
                .execute(
                    &mut next_run,
                    action,
                    workspace_policy,
                    Some(&mut grant),
                    PolicyEvaluationContext::new(
                        ids.decision_id,
                        ids.request_id,
                        ids.event_id,
                        run.current_snapshot_id(),
                        timing,
                        expires,
                    ),
                )?;
            if evaluation.decision().reason() != PolicyDecisionReason::ApprovalGranted
                || evaluation.decision().outcome() != PolicyDecisionOutcome::Allowed
            {
                return Err(admission(Failure::Denied));
            }
            evaluations.push((next_run.clone(), evaluation));
        }
        Ok(evaluations)
    }

    #[allow(clippy::too_many_arguments)]
    async fn settle_loopback_failure(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        request: LoopbackVerificationRequest<'_>,
        attempt: AgentToolAttemptNumber,
        disposition: AgentMutationDisposition,
        failure: MutationFailureClass,
        at: AgentRunTimestamp,
    ) -> Result<(), MutationControllerFailure> {
        let status = match failure {
            MutationFailureClass::Cancelled => AgentToolAttemptStatus::Cancelled,
            _ => AgentToolAttemptStatus::Failed,
        };
        self.recovery
            .finish_agent_mutation_attempt(
                project,
                request.ids.tool_run_id,
                attempt,
                status,
                disposition,
                at,
            )
            .await
            .map_err(MutationControllerFailure::MutationResultStore)?;
        self.record_tool_event(
            project,
            run,
            request.ids.tool_event_id,
            request.ids.tool_run_id,
            run.current_snapshot_id(),
            false,
            None,
            at,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn cancel_loopback_step(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step: TaskStepId,
        event_id: RunEventId,
        at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure> {
        let mut next_ledger = ledger.clone();
        next_ledger.cancel_step(
            step,
            TaskStepCancellationReason::try_from_string(
                "owned HTTP verification was cancelled".to_owned(),
            )
            .map_err(|_| MutationControllerFailure::InvalidStaticText)?,
            ledger_timestamp(at)?,
        )?;
        let mut next_run = run.clone();
        let snapshot = run.current_snapshot_id();
        let advance = AdvanceAgentController.execute(
            &mut next_run,
            AgentControllerSignal::CancelRequested,
            event_id,
            snapshot,
            at,
            control.is_cancelled(),
        )?;
        *ledger_version = self
            .action_store
            .commit_ledger_action(
                project,
                *ledger_version,
                run.last_event_sequence(),
                &next_ledger,
                &next_run,
                advance.event(),
            )
            .await?;
        *ledger = next_ledger;
        *run = next_run;
        Ok(MutationControllerOutcome::Stopped {
            failure: MutationFailureClass::Cancelled,
            snapshot_id: snapshot,
        })
    }
}

fn loopback_step<'a>(
    run: &AgentRun,
    ledger: &'a TaskLedger,
    step_id: TaskStepId,
) -> Result<&'a a3_domain::TaskStep, MutationControllerFailure> {
    let step = ledger
        .step(step_id)
        .ok_or(MutationControllerFailure::AnchorMismatch)?;
    let expected_status = match run.state() {
        AgentControllerState::Execute => TaskStepStatus::InProgress,
        AgentControllerState::AwaitApproval => TaskStepStatus::AwaitingApproval,
        _ => return Err(MutationControllerFailure::AnchorMismatch),
    };
    if run.goal_contract() != ledger.goal_contract()
        || run.task_ledger_revision() != ledger.revision()
        || !step.is_active_plan_step()
        || step.status() != expected_status
        || step
            .attempts()
            .last()
            .is_none_or(|attempt| attempt.run_id() != run.id())
    {
        return Err(MutationControllerFailure::AnchorMismatch);
    }
    Ok(step)
}

fn require_dispatch_room<C>(
    run: &AgentRun,
    request: LoopbackVerificationRequest<'_>,
    started: Instant,
    control: &C,
) -> Result<AgentRunTimestamp, MutationControllerFailure>
where
    C: AgentControllerControl + ProcessRunControl,
{
    if AgentControllerControl::is_cancelled(control) || ProcessRunControl::is_cancelled(control) {
        return Err(admission(Failure::Cancelled));
    }
    let now = elapsed(request, started)?;
    if now < run.updated_at()
        || run
            .budget_exhaustion(now)
            .map_err(|_| MutationControllerFailure::InvalidTimestamp)?
            .is_some()
        || run_time_remaining(run, now)?
            < Duration::from_millis(request.settings.timeout.as_millis())
    {
        return Err(admission(Failure::TimedOut));
    }
    Ok(now)
}

fn run_time_remaining(
    run: &AgentRun,
    now: AgentRunTimestamp,
) -> Result<Duration, MutationControllerFailure> {
    let deadline = run
        .created_at()
        .unix_millis()
        .checked_add(run.budget().duration_limit().millis())
        .ok_or(MutationControllerFailure::InvalidTimestamp)?;
    Ok(Duration::from_millis(
        deadline.saturating_sub(now.unix_millis()),
    ))
}

fn loopback_index_deadline(
    run: &AgentRun,
    observed_at: AgentRunTimestamp,
    started: Instant,
) -> Result<Instant, MutationControllerFailure> {
    started
        .checked_add(run_time_remaining(run, observed_at)?)
        .ok_or(MutationControllerFailure::InvalidTimestamp)
}

fn loopback_index_batch() -> Result<RepositoryChangeBatch, RepositoryChangeBatchError> {
    RepositoryChangeBatch::full_rescan(Vec::new(), crate::RepositoryRescanReason::Explicit)
}

#[derive(Debug)]
struct LoopbackIndexControl<'a> {
    inner: &'a dyn RepositoryIndexControl,
    deadline: Instant,
}

impl RepositoryIndexControl for LoopbackIndexControl<'_> {
    fn is_cancelled(&self) -> bool {
        Instant::now() >= self.deadline || self.inner.is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryIndexControlError> {
        self.inner.report_progress(progress)
    }

    fn observe(&self, observation: RepositoryIndexObservation) {
        self.inner.observe(observation);
    }
}

fn elapsed(
    request: LoopbackVerificationRequest<'_>,
    started: Instant,
) -> Result<AgentRunTimestamp, MutationControllerFailure> {
    phase_elapsed_timestamp(started, request.observed_at)
        .ok_or(MutationControllerFailure::InvalidTimestamp)
}

const fn admission(failure: Failure) -> MutationControllerFailure {
    MutationControllerFailure::LoopbackAdmission(failure)
}

fn finished_loopback_failure(
    cancelled: bool,
    time_left: Duration,
    observed_failure: Option<MutationFailureClass>,
) -> Option<MutationFailureClass> {
    if cancelled {
        Some(MutationFailureClass::Cancelled)
    } else if time_left.is_zero() {
        Some(MutationFailureClass::TimedOut)
    } else {
        observed_failure
    }
}

const fn classify(failure: Failure) -> MutationFailureClass {
    match failure {
        Failure::Cancelled => MutationFailureClass::Cancelled,
        Failure::TimedOut => MutationFailureClass::TimedOut,
        Failure::Denied => MutationFailureClass::Denied,
        Failure::Occupied | Failure::StaleScript => MutationFailureClass::Conflict,
        Failure::InvalidResponse | Failure::ResourceLimit => {
            MutationFailureClass::VerificationFailed
        }
        Failure::Unsupported | Failure::Unavailable | Failure::Exited => {
            MutationFailureClass::ToolUnavailable
        }
    }
}

#[cfg(test)]
mod failure_priority_tests {
    use super::*;
    #[test]
    fn mismatch_never_masks_cancellation_or_the_exhausted_original_deadline() {
        let mismatch = Some(MutationFailureClass::VerificationFailed);
        assert_eq!(
            finished_loopback_failure(true, Duration::ZERO, mismatch),
            Some(MutationFailureClass::Cancelled)
        );
        assert_eq!(
            finished_loopback_failure(false, Duration::ZERO, mismatch),
            Some(MutationFailureClass::TimedOut)
        );
        assert_eq!(
            finished_loopback_failure(false, Duration::from_millis(1), mismatch),
            mismatch
        );
        assert_eq!(
            finished_loopback_failure(false, Duration::ZERO, None),
            Some(MutationFailureClass::TimedOut)
        );
        assert_eq!(
            finished_loopback_failure(false, Duration::from_millis(1), None),
            None
        );
    }
}

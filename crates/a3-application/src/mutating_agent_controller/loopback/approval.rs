//! Explicitly presents one action at a time before the existing loopback dispatch.

use super::*;
use crate::LoopbackApprovalContinuation;
use a3_domain::{ApprovalId, LoopbackHttpRecipe};

/// Core-owned identities and bounds for the complete volatile approval sequence.
#[derive(Debug, Clone, Copy)]
pub struct LoopbackApprovalAdmission<'a> {
    /// Exact settings and later grant-consumption identities.
    pub execution: LoopbackVerificationRequest<'a>,
    /// New policy-decision and journal identities for issuing Start and each ordered probe.
    pub issuance: &'a [(PolicyDecisionId, RunEventId)],
    /// Transition identity for returning to Execute after the last explicit Continue.
    pub resume_event_id: RunEventId,
    /// Exclusive request deadline, bounded by the current run deadline.
    pub expires_at: AgentRunTimestamp,
}

/// One recipe frozen by the owning candidate runtime across explicit user decisions.
///
/// Dropping it loses authorization to continue the sequence. No grant is durable proof
/// that a process started, and a new sequence cannot adopt a prior run's pending slot.
#[derive(Debug)]
pub struct LoopbackApprovalSequence {
    context: AgentInspectionContext,
    prepared: PreparedHttpExecution,
    settings: LoopbackProcessSettings,
    permissions: AgentPermissionSettings,
    workspace_policy: WorkspacePolicy,
    ids: MutationExecutionIds,
    probes: Vec<LoopbackProbeApproval>,
    issuance: Vec<(PolicyDecisionId, RunEventId)>,
    resume_event_id: RunEventId,
    expires_at: AgentRunTimestamp,
    pending: usize,
}

impl LoopbackApprovalSequence {
    /// Request currently presented by the Core, or no request after the final Continue.
    #[must_use]
    pub fn next_request_id(&self) -> Option<ApprovalRequestId> {
        self.request_id(self.pending)
    }

    fn request_id(&self, stage: usize) -> Option<ApprovalRequestId> {
        match stage.checked_sub(1) {
            None => Some(self.ids.approval_request_id),
            Some(probe) => self.probes.get(probe).map(|ids| ids.request_id),
        }
    }

    fn request(&self, observed_at: AgentRunTimestamp) -> LoopbackVerificationRequest<'_> {
        LoopbackVerificationRequest {
            step_id: self.context.step_id(),
            settings: self.settings,
            probe_approvals: &self.probes,
            ids: self.ids,
            observed_at,
        }
    }
}

pub(super) struct ContinuedLoopbackApproval<'a> {
    sequence: &'a LoopbackApprovalSequence,
    continuation: &'a LoopbackApprovalContinuation,
}

impl ContinuedLoopbackApproval<'_> {
    pub(super) const fn resume_event_id(&self) -> RunEventId {
        self.sequence.resume_event_id
    }

    pub(super) fn validate(
        &self,
        sink: &dyn AgentApprovalSink,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        prepared: &PreparedHttpExecution,
        workspace_policy: &WorkspacePolicy,
        permissions: AgentPermissionSettings,
    ) -> Result<(), MutationControllerFailure> {
        if prepared != &self.sequence.prepared
            || context != self.sequence.context
            || self.continuation.context() != self.sequence.context
            || self.sequence.next_request_id() != Some(self.continuation.request_id())
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        if workspace_policy != &self.sequence.workspace_policy
            || permissions != self.sequence.permissions
        {
            return Err(MutationControllerFailure::PermissionsChanged);
        }
        sink.validate_loopback_continuation(project, self.continuation)?;
        Ok(())
    }

    pub(super) fn validate_current(
        &self,
        sink: &dyn AgentApprovalSink,
        project: &ProjectIdentity,
        now: AgentRunTimestamp,
    ) -> Result<(), MutationControllerFailure> {
        if now >= self.sequence.expires_at {
            return Err(admission(Failure::Denied));
        }
        sink.validate_loopback_continuation(project, self.continuation)?;
        Ok(())
    }
}

impl ExecuteMutatingAgentAction<'_> {
    /// Presents the exact start after durable policy issuance, then enters the existing wait.
    #[allow(clippy::too_many_arguments)]
    pub async fn begin_loopback_approval<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        admission_request: LoopbackApprovalAdmission<'_>,
        authority: LoopbackVerificationAuthority<'_>,
        runner: &dyn LoopbackVerificationRunner,
        workspace_policy: &WorkspacePolicy,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        control: &C,
    ) -> Result<LoopbackApprovalSequence, MutationControllerFailure>
    where
        C: AgentControllerControl + ProcessRunControl + RepositoryIndexControl,
    {
        let started = Instant::now();
        let request = admission_request.execution;
        if run.state() != AgentControllerState::Execute {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        if !runner.supports_owned_cleanup() {
            return Err(admission(Failure::Unsupported));
        }
        let permission_store = self.permissions.ok_or(admission(Failure::Unavailable))?;
        let spec = loopback_step(run, ledger, request.step_id)?
            .definition()
            .verification_spec();
        let VerificationTarget::LoopbackHttp(recipe) = spec.target() else {
            return Err(MutationControllerFailure::AnchorMismatch);
        };
        validate_ids(admission_request, recipe)?;
        let deadline = run
            .created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            .ok_or(MutationControllerFailure::InvalidTimestamp)?;
        if admission_request.expires_at.unix_millis() > deadline {
            return Err(MutationControllerFailure::InvalidTimestamp);
        }
        let context =
            inspection_context(run, request.step_id, spec.id(), run.current_snapshot_id());
        let start = recipe
            .process_spec(
                run.id(),
                project.worktree().id(),
                request.step_id,
                request.settings,
            )
            .map_err(|_| admission(Failure::Denied))?;
        let fingerprint =
            MutationActionFingerprint::from_bytes(*start.policy_action().fingerprint().as_bytes());
        let _lease =
            self.coordinator
                .try_acquire(run.id(), project.worktree().id(), fingerprint)?;
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
        let prepared = self
            .refresh_loopback_scope(
                project,
                run,
                loopback_index_deadline(run, request.observed_at, started)?,
                start,
                recipe,
                index_compiler,
                control,
            )
            .await?;
        let permissions = permission_store
            .load_agent_permissions()
            .await
            .map_err(|_| admission(Failure::Unavailable))?;
        if !prepared.supported_by(runner) {
            return Err(admission(Failure::Unsupported));
        }
        let sequence = LoopbackApprovalSequence {
            context,
            prepared,
            settings: request.settings,
            permissions,
            workspace_policy: workspace_policy.clone(),
            ids: request.ids,
            probes: request.probe_approvals.to_vec(),
            issuance: admission_request.issuance.to_vec(),
            resume_event_id: admission_request.resume_event_id,
            expires_at: admission_request.expires_at,
            pending: 0,
        };
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
        let now = require_dispatch_room(run, request, started, control)?;
        let revision = self
            .issue_loopback_request(project, run, &sequence, 0, now, None)
            .await?;
        let event_id = request.ids.approval_transition_event_id;
        let Some(revision) = revision else {
            let now = require_dispatch_room(run, request, started, control)?;
            self.deny_action(
                project,
                run,
                ledger,
                ledger_version,
                request.step_id,
                event_id,
                now,
                control,
            )
            .await?;
            return Err(admission(Failure::Denied));
        };
        let waiting = async {
            self.require_current_loopback_publication(project, sequence.prepared.snapshot())
                .await?;
            let now = require_dispatch_room(run, request, started, control)?;
            if now >= sequence.expires_at {
                return Err(admission(Failure::Denied));
            }
            self.await_approval(
                project,
                run,
                ledger,
                ledger_version,
                request.step_id,
                event_id,
                now,
                false,
            )
            .await
        }
        .await;
        if waiting.is_err() {
            self.approval.discard_loopback_request(
                project,
                sequence.context,
                request.ids.approval_request_id,
                revision,
            );
        }
        waiting?;
        Ok(sequence)
    }

    /// Advances only a task-bound explicit Continue. The final one admits one owned execution.
    #[allow(clippy::too_many_arguments)]
    pub async fn continue_loopback_approval<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        sequence: &mut LoopbackApprovalSequence,
        continuation: LoopbackApprovalContinuation,
        observed_at: AgentRunTimestamp,
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
        let started = Instant::now();
        let request = sequence.request(observed_at);
        if run.state() != AgentControllerState::AwaitApproval
            || sequence.next_request_id() != Some(continuation.request_id())
            || *ledger_version != continuation.ledger_store_version()
            || ledger.revision().get() != continuation.ledger_revision()
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let continued = ContinuedLoopbackApproval {
            sequence,
            continuation: &continuation,
        };
        let next = sequence.pending + 1;
        let Some(next_request) = sequence.request_id(next) else {
            let outcome = self
                .execute_prepared_loopback(
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
                    Some(continued),
                )
                .await?;
            sequence.pending = next;
            return Ok(outcome);
        };
        let permission_store = self.permissions.ok_or(admission(Failure::Unavailable))?;
        let spec = loopback_step(run, ledger, request.step_id)?
            .definition()
            .verification_spec();
        let VerificationTarget::LoopbackHttp(recipe) = spec.target() else {
            return Err(MutationControllerFailure::AnchorMismatch);
        };
        let context =
            inspection_context(run, request.step_id, spec.id(), run.current_snapshot_id());
        if context != sequence.context {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let start = recipe
            .process_spec(
                run.id(),
                project.worktree().id(),
                request.step_id,
                request.settings,
            )
            .map_err(|_| admission(Failure::Denied))?;
        let fingerprint =
            MutationActionFingerprint::from_bytes(*start.policy_action().fingerprint().as_bytes());
        let _lease =
            self.coordinator
                .try_acquire(run.id(), project.worktree().id(), fingerprint)?;
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
        let prepared = self
            .refresh_loopback_scope(
                project,
                run,
                loopback_index_deadline(run, request.observed_at, started)?,
                start,
                recipe,
                index_compiler,
                control,
            )
            .await?;
        let permissions = permission_store
            .load_agent_permissions()
            .await
            .map_err(|_| admission(Failure::Unavailable))?;
        continued.validate(
            self.approval,
            project,
            context,
            &prepared,
            workspace_policy,
            permissions,
        )?;
        let approval = self
            .policy_store
            .load_approval_for_request(project, continuation.request_id())
            .await?
            .ok_or(admission(Failure::Denied))?;
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
        self.require_current_loopback_publication(project, prepared.snapshot())
            .await?;
        let now = require_dispatch_room(run, request, started, control)?;
        continued.validate_current(self.approval, project, now)?;
        if approval.id() != continuation.approval_id()
            || approval.status_at(now) != ApprovalStatus::Active
            || now < approval.granted_at()
        {
            return Err(admission(Failure::Denied));
        }
        let previous = (continuation.revision(), continuation.approval_id());
        if self
            .issue_loopback_request(project, run, sequence, next, now, Some(previous))
            .await?
            .is_none()
        {
            let now = require_dispatch_room(run, request, started, control)?;
            self.deny_action(
                project,
                run,
                ledger,
                ledger_version,
                request.step_id,
                sequence.resume_event_id,
                now,
                control,
            )
            .await?;
            sequence.pending = sequence.issuance.len();
            return Ok(MutationControllerOutcome::Denied);
        }
        sequence.pending = next;
        Ok(MutationControllerOutcome::AwaitingApproval(next_request))
    }

    async fn issue_loopback_request(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        sequence: &LoopbackApprovalSequence,
        stage: usize,
        observed_at: AgentRunTimestamp,
        previous: Option<(crate::AgentApprovalRevision, ApprovalId)>,
    ) -> Result<Option<crate::AgentApprovalRevision>, MutationControllerFailure> {
        let &(decision_id, event_id) = sequence
            .issuance
            .get(stage)
            .ok_or(admission(Failure::Denied))?;
        let request_id = sequence
            .request_id(stage)
            .ok_or(admission(Failure::Denied))?;
        let actions = sequence.prepared.policy_actions();
        let action = actions.get(stage).ok_or(admission(Failure::Denied))?;
        let mut next_run = run.clone();
        let timing = PolicyEvaluationTiming::new(observed_at, observed_at)
            .map_err(|_| MutationControllerFailure::InvalidTimestamp)?;
        let evaluation = EvaluateActionPolicy::new()
            .with_permissions(sequence.permissions)
            .execute(
                &mut next_run,
                action,
                &sequence.workspace_policy,
                None,
                PolicyEvaluationContext::new(
                    decision_id,
                    request_id,
                    event_id,
                    sequence.prepared.snapshot(),
                    timing,
                    sequence.expires_at,
                ),
            )?;
        PersistPolicyEvaluation::new(self.policy_store)
            .execute(project, run.last_event_sequence(), &next_run, &evaluation)
            .await?;
        *run = next_run;
        if evaluation.decision().outcome() == PolicyDecisionOutcome::Denied {
            return Ok(None);
        }
        let request = evaluation
            .approval_request()
            .ok_or(MutationControllerFailure::InvalidPolicyResult)?;
        let revision = sequence.prepared.record_approval(
            self.approval,
            project,
            sequence.context,
            request,
            evaluation.decision().reason(),
            stage,
            previous,
        )?;
        Ok(Some(revision))
    }
}

fn validate_ids(
    admission_request: LoopbackApprovalAdmission<'_>,
    recipe: &LoopbackHttpRecipe,
) -> Result<(), MutationControllerFailure> {
    let request = admission_request.execution;
    let actions = recipe.checks().len()
        + if recipe.default_start_check_index().is_some() {
            6
        } else {
            1
        };
    if request.probe_approvals.len() + 1 != actions || admission_request.issuance.len() != actions {
        return Err(admission(Failure::Denied));
    }
    let ids = request.ids;
    let mut requests = BTreeSet::from([ids.approval_request_id]);
    let mut decisions = BTreeSet::from([ids.policy_decision_id]);
    let mut events = BTreeSet::new();
    for event in [
        ids.policy_event_id,
        ids.approval_transition_event_id,
        ids.tool_event_id,
        ids.verification_transition_event_id,
        ids.resolution_transition_event_id,
        ids.context_event_id,
        admission_request.resume_event_id,
    ] {
        if !events.insert(event) {
            return Err(admission(Failure::Denied));
        }
    }
    for probe in request.probe_approvals {
        if !requests.insert(probe.request_id)
            || !decisions.insert(probe.decision_id)
            || !events.insert(probe.event_id)
        {
            return Err(admission(Failure::Denied));
        }
    }
    for &(decision, event) in admission_request.issuance {
        if !decisions.insert(decision) || !events.insert(event) {
            return Err(admission(Failure::Denied));
        }
    }
    Ok(())
}

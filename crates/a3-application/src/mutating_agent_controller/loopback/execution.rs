//! One controller lifecycle for single-start and complete startup verification.

use super::*;
use crate::{
    AuthorizedLoopbackStartupCoverage, LoopbackStartupVerificationOutcome,
    LoopbackStartupVerificationReport, PreparedLoopbackStartupCoverage,
};
use a3_domain::{
    LoopbackHttpRecipe, LoopbackStartupEvidence, PolicyResourceId, ProcessPlanBinding,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PreparedHttpExecution {
    Single(Box<PreparedLoopbackVerification>),
    Startup(Box<PreparedLoopbackStartupCoverage>),
}

impl PreparedHttpExecution {
    pub(super) fn new(
        start: ProcessSpec,
        script: FileRevision,
        snapshot: SnapshotId,
        recipe: LoopbackHttpRecipe,
    ) -> Result<Self, Failure> {
        if recipe.default_start_check_index().is_some() {
            let ProcessPlanBinding::Validated(step) = start.plan_binding() else {
                return Err(Failure::Denied);
            };
            let settings = LoopbackHttpProcess::from_spec(&start)
                .map_err(|_| Failure::Denied)?
                .settings();
            let prepared = PreparedLoopbackStartupCoverage::new(
                start.run_id(),
                start.worktree_id(),
                step,
                snapshot,
                script,
                recipe,
                settings,
            )?;
            if prepared.start() != &start {
                return Err(Failure::Denied);
            }
            Ok(Self::Startup(Box::new(prepared)))
        } else {
            PreparedLoopbackVerification::new(start, script, snapshot, recipe)
                .map(|prepared| Self::Single(Box::new(prepared)))
        }
    }

    pub(super) fn start(&self) -> &ProcessSpec {
        match self {
            Self::Single(value) => value.start(),
            Self::Startup(value) => value.start(),
        }
    }
    pub(super) fn script(&self) -> &FileRevision {
        match self {
            Self::Single(value) => value.script(),
            Self::Startup(value) => value.script(),
        }
    }
    pub(super) fn snapshot(&self) -> SnapshotId {
        match self {
            Self::Single(value) => value.snapshot(),
            Self::Startup(value) => value.snapshot(),
        }
    }

    pub(super) fn policy_actions(&self) -> Vec<PolicyAction> {
        match self {
            Self::Single(value) => std::iter::once(value.start().policy_action())
                .chain(
                    (0..value.recipe().checks().len())
                        .filter_map(|index| value.probe_policy_action(index)),
                )
                .collect(),
            Self::Startup(value) => value.policy_actions(),
        }
    }

    pub(super) fn effect_resource(&self) -> PolicyResourceId {
        match self {
            Self::Single(value) => value.start().specification_id(),
            Self::Startup(value) => value.effect_resource(),
        }
    }

    pub(super) fn effect_fingerprint(&self) -> MutationActionFingerprint {
        match self {
            Self::Single(value) => MutationActionFingerprint::from_bytes(
                *value.start().policy_action().fingerprint().as_bytes(),
            ),
            Self::Startup(value) => value.effect_fingerprint(),
        }
    }

    pub(super) fn supported_by(&self, runner: &dyn LoopbackVerificationRunner) -> bool {
        runner.supports_owned_cleanup()
            && match self {
                Self::Single(_) => true,
                Self::Startup(_) => runner.supports_startup_coverage(),
            }
    }

    pub(super) fn authorize(
        self,
        decisions: &[PolicyDecision],
    ) -> Result<AuthorizedHttpExecution, Failure> {
        match self {
            Self::Single(value) => {
                let (first, probes) = decisions.split_first().ok_or(Failure::Denied)?;
                AuthorizedLoopbackVerification::new(*value, first, probes)
                    .map(|authorized| AuthorizedHttpExecution::Single(Box::new(authorized)))
            }
            Self::Startup(value) => AuthorizedLoopbackStartupCoverage::new(*value, decisions)
                .map(|authorized| AuthorizedHttpExecution::Startup(Box::new(authorized))),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_approval(
        &self,
        sink: &dyn AgentApprovalSink,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &a3_domain::ApprovalRequest,
        reason: PolicyDecisionReason,
        stage: usize,
        previous: Option<(crate::AgentApprovalRevision, a3_domain::ApprovalId)>,
    ) -> Result<crate::AgentApprovalRevision, AgentApprovalSinkFailure> {
        match self {
            Self::Single(value) => sink.record_loopback_request(
                project,
                context,
                request,
                reason,
                value,
                stage.checked_sub(1),
                previous,
            ),
            Self::Startup(value) => sink
                .record_startup_request(project, context, request, reason, value, stage, previous),
        }
    }
}

pub(super) enum AuthorizedHttpExecution {
    Single(Box<AuthorizedLoopbackVerification>),
    Startup(Box<AuthorizedLoopbackStartupCoverage>),
}

pub(super) enum HttpExecutionOutcome {
    NotStarted(Failure),
    Stopped(Result<HttpExecutionReport, Failure>),
    Mismatched {
        cleanup: crate::LoopbackStartupCleanupProof,
        mismatch: a3_domain::LoopbackHttpMismatch,
    },
    UnknownCleanup,
}

impl AuthorizedHttpExecution {
    pub(super) async fn verify(
        self,
        runner: &dyn LoopbackVerificationRunner,
        project: &ProjectIdentity,
        control: &dyn ProcessRunControl,
    ) -> HttpExecutionOutcome {
        match self {
            Self::Single(value) => match runner.verify(project, *value, control).await {
                LoopbackVerificationOutcome::NotStarted(f) => HttpExecutionOutcome::NotStarted(f),
                LoopbackVerificationOutcome::Stopped { result, .. } => {
                    HttpExecutionOutcome::Stopped(result.map(HttpExecutionReport::Single))
                }
                LoopbackVerificationOutcome::UnknownCleanup => HttpExecutionOutcome::UnknownCleanup,
            },
            Self::Startup(value) => match runner.verify_startup(project, *value, control).await {
                LoopbackStartupVerificationOutcome::Mismatched { cleanup, mismatch } => {
                    HttpExecutionOutcome::Mismatched { cleanup, mismatch }
                }
                LoopbackStartupVerificationOutcome::NotStarted(f) => {
                    HttpExecutionOutcome::NotStarted(f)
                }
                LoopbackStartupVerificationOutcome::Stopped { result, .. } => {
                    HttpExecutionOutcome::Stopped(result.map(HttpExecutionReport::Startup))
                }
                LoopbackStartupVerificationOutcome::UnknownCleanup => {
                    HttpExecutionOutcome::UnknownCleanup
                }
            },
        }
    }
}

pub(super) enum HttpExecutionReport {
    Single(crate::LoopbackVerificationReport),
    Startup(Box<LoopbackStartupVerificationReport>),
}

impl HttpExecutionReport {
    pub(super) fn matches_decisions(&self, decisions: &[PolicyDecisionId]) -> bool {
        match self {
            Self::Single(report) => std::iter::once(report.start_decision)
                .chain(report.probes.iter().map(|probe| probe.decision()))
                .eq(decisions.iter().copied()),
            Self::Startup(report) => {
                report.decisions.ordered() == decisions
                    && report
                        .explicit_probes
                        .iter()
                        .map(|probe| probe.decision())
                        .eq(report.decisions.explicit_probes.iter().copied())
                    && report.default_probe.decision() == report.decisions.default_probe
            }
        }
    }

    pub(super) fn into_receipt(
        self,
        context: LoopbackHttpEvidenceContext,
        recipe: PolicyResourceId,
        script: FileRevision,
        process: LoopbackHttpProcess,
    ) -> Result<LoopbackHttpEvidence, a3_domain::LoopbackHttpError> {
        match self {
            Self::Single(report) => LoopbackHttpEvidence::new(
                context,
                recipe,
                script,
                process,
                report.start_decision,
                report.probes,
            ),
            Self::Startup(report) => {
                let decisions = report.decisions;
                let startup = LoopbackStartupEvidence::new(
                    decisions.reserve,
                    decisions.held_explicit,
                    decisions.default_start,
                    decisions.invalid_port,
                    report.held_exit,
                    report.invalid_exit,
                    report.default_probe,
                )?;
                LoopbackHttpEvidence::new_startup(
                    context,
                    recipe,
                    script,
                    process,
                    decisions.free_explicit,
                    report.explicit_probes,
                    startup,
                )
            }
        }
    }
}

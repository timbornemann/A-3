use crate::{
    ModuleDependencyEdgeEvidenceV1, ModuleDependencyEndpointV1, ModuleDependencyRelationV1,
    ProjectMapSearchEvidenceV1, ProjectMapSearchSymbolKindV1, ProtocolVersion,
};
use serde::{Deserialize, Serialize};

/// Strict pathless request for the bounded durable task selector.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryTaskLensTasksRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryTaskLensTasksRequestV1 {
    /// Creates a current or intentionally unsupported request for boundary tests.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Strict request selecting one Core-owned durable task by opaque identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryTaskLensTaskRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
}

impl QueryTaskLensTaskRequestV1 {
    /// Creates an untrusted task-selection request.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, task_id: String) -> Self {
        Self {
            protocol_version,
            task_id,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted opaque task identity.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

/// Strict request compiling one active durable plan step through R10.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CompileTaskLensRequestV1 {
    protocol_version: ProtocolVersion,
    task_id: String,
    step_id: String,
}

impl CompileTaskLensRequestV1 {
    /// Creates an untrusted task/step selection request without repository or path authority.
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion, task_id: String, step_id: String) -> Self {
        Self {
            protocol_version,
            task_id,
            step_id,
        }
    }

    /// Returns the requested protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the untrusted opaque task identity.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the untrusted opaque step identity.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }
}

/// Bounded list response for durable tasks available as Lens anchors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensTasksResponseV1 {
    protocol_version: ProtocolVersion,
    result: TaskLensTasksResultV1,
}

impl TaskLensTasksResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(TaskLensTasksResultV1::NoProject)
    }

    /// Creates a bounded current Goal Contract list.
    #[must_use]
    pub const fn available(tasks: Vec<TaskLensTaskSummaryV1>, truncated: bool) -> Self {
        Self::with_result(TaskLensTasksResultV1::Available { tasks, truncated })
    }

    const fn with_result(result: TaskLensTasksResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive active-project state.
    #[must_use]
    pub const fn result(&self) -> &TaskLensTasksResultV1 {
        &self.result
    }
}

/// Availability of the current worktree's durable task selector.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum TaskLensTasksResultV1 {
    /// No Core-owned project is active.
    NoProject,
    /// A bounded stable page of current Goal Contracts is available.
    Available {
        /// Tasks in stable opaque identity order.
        tasks: Vec<TaskLensTaskSummaryV1>,
        /// Additional tasks were omitted at the fixed product boundary.
        truncated: bool,
    },
}

/// Minimal current Goal Contract projection used by task selectors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensTaskSummaryV1 {
    task_id: String,
    goal_revision: u32,
    objective: String,
}

impl TaskLensTaskSummaryV1 {
    /// Creates one current durable task summary.
    #[must_use]
    pub const fn new(task_id: String, goal_revision: u32, objective: String) -> Self {
        Self {
            task_id,
            goal_revision,
            objective,
        }
    }
}

/// Current plan-step selector response for one durable task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensTaskResponseV1 {
    protocol_version: ProtocolVersion,
    result: TaskLensTaskResultV1,
}

impl TaskLensTaskResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(TaskLensTaskResultV1::NoProject)
    }

    /// Creates the response for an unknown task identity.
    #[must_use]
    pub const fn task_not_found() -> Self {
        Self::with_result(TaskLensTaskResultV1::TaskNotFound)
    }

    /// Creates the expected state before U5 has materialized a Task Ledger.
    #[must_use]
    pub const fn ledger_unavailable(task: TaskLensTaskSummaryV1) -> Self {
        Self::with_result(TaskLensTaskResultV1::LedgerUnavailable { task })
    }

    /// Creates the explicit stale-plan state after a Goal Contract revision.
    #[must_use]
    pub const fn goal_revision_mismatch(
        task_id: String,
        current_goal_revision: u32,
        ledger_goal_revision: u32,
    ) -> Self {
        Self::with_result(TaskLensTaskResultV1::GoalRevisionMismatch {
            task_id,
            current_goal_revision,
            ledger_goal_revision,
        })
    }

    /// Creates a current bounded active-plan step selector.
    #[must_use]
    pub const fn available(
        task: TaskLensTaskSummaryV1,
        ledger_revision: u32,
        ledger_store_version: String,
        steps: Vec<TaskLensStepV1>,
    ) -> Self {
        Self::with_result(TaskLensTaskResultV1::Available {
            task,
            ledger_revision,
            ledger_store_version,
            steps,
        })
    }

    const fn with_result(result: TaskLensTaskResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive task/ledger state.
    #[must_use]
    pub const fn result(&self) -> &TaskLensTaskResultV1 {
        &self.result
    }
}

/// Availability and freshness of one selected durable task anchor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum TaskLensTaskResultV1 {
    /// No Core-owned project is active.
    NoProject,
    /// No current Goal Contract exists for this identity.
    TaskNotFound,
    /// The task exists but has no materialized Task Ledger.
    LedgerUnavailable {
        /// Current Goal Contract summary.
        task: TaskLensTaskSummaryV1,
    },
    /// The Task Ledger serves an earlier Goal Contract revision.
    GoalRevisionMismatch {
        /// Stable durable task identity.
        task_id: String,
        /// Latest current Goal Contract revision.
        current_goal_revision: u32,
        /// Goal Contract revision referenced by the ledger.
        ledger_goal_revision: u32,
    },
    /// Current Goal and plan revisions agree.
    Available {
        /// Current Goal Contract summary.
        task: TaskLensTaskSummaryV1,
        /// Current plan revision.
        ledger_revision: u32,
        /// Optimistic persistence version encoded losslessly.
        ledger_store_version: String,
        /// At most 256 active-plan steps in stable identity order.
        steps: Vec<TaskLensStepV1>,
    },
}

/// One active-plan step selectable as the current Lens focus.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensStepV1 {
    step_id: String,
    intended_outcome: String,
    status: TaskLensStepStatusV1,
}

impl TaskLensStepV1 {
    /// Creates a durable active-plan step projection.
    #[must_use]
    pub const fn new(
        step_id: String,
        intended_outcome: String,
        status: TaskLensStepStatusV1,
    ) -> Self {
        Self {
            step_id,
            intended_outcome,
            status,
        }
    }
}

/// Materialized current status of an active-plan step.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensStepStatusV1 {
    /// Dependencies are not yet satisfied.
    Pending,
    /// The step may start.
    Ready,
    /// One owned run is executing the step.
    InProgress,
    /// Execution is blocked by an explicit reason.
    Blocked,
    /// Execution awaits scoped user approval.
    AwaitingApproval,
    /// The immutable verification specification is running.
    Verifying,
    /// Fresh verification evidence completed the step.
    Completed,
    /// The latest attempt failed.
    Failed,
    /// The step was explicitly cancelled.
    Cancelled,
    /// Completed evidence became stale.
    Stale,
}

/// Response from one explicit deterministic Task Lens compile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensCompileResponseV1 {
    protocol_version: ProtocolVersion,
    result: TaskLensCompileResultV1,
}

impl TaskLensCompileResponseV1 {
    /// Creates the response used before a project is active.
    #[must_use]
    pub const fn no_project() -> Self {
        Self::with_result(TaskLensCompileResultV1::NoProject)
    }

    /// Creates the response for an unknown task identity.
    #[must_use]
    pub const fn task_not_found() -> Self {
        Self::with_result(TaskLensCompileResultV1::TaskNotFound)
    }

    /// Creates the response for a task without a materialized ledger.
    #[must_use]
    pub const fn ledger_unavailable() -> Self {
        Self::with_result(TaskLensCompileResultV1::LedgerUnavailable)
    }

    /// Creates the response when the ledger serves an earlier goal revision.
    #[must_use]
    pub const fn goal_revision_mismatch(
        task_id: String,
        current_goal_revision: u32,
        ledger_goal_revision: u32,
    ) -> Self {
        Self::with_result(TaskLensCompileResultV1::GoalRevisionMismatch {
            task_id,
            current_goal_revision,
            ledger_goal_revision,
        })
    }

    /// Creates the response for an absent or retired step identity.
    #[must_use]
    pub const fn step_unavailable() -> Self {
        Self::with_result(TaskLensCompileResultV1::StepUnavailable)
    }

    /// Creates the response before the first atomic index publication.
    #[must_use]
    pub const fn no_published_index() -> Self {
        Self::with_result(TaskLensCompileResultV1::NoPublishedIndex)
    }

    /// Creates a response containing one bounded deterministic Lens.
    #[must_use]
    pub fn available(lens: TaskLensV1) -> Self {
        Self::with_result(TaskLensCompileResultV1::Available {
            lens: Box::new(lens),
        })
    }

    const fn with_result(result: TaskLensCompileResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }

    /// Returns the mutually exclusive compile state.
    #[must_use]
    pub const fn result(&self) -> &TaskLensCompileResultV1 {
        &self.result
    }
}

/// Expected missing/stale anchor states or one available Lens.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum TaskLensCompileResultV1 {
    /// No Core-owned project is active.
    NoProject,
    /// No current Goal Contract exists for the selected identity.
    TaskNotFound,
    /// The task exists but has no materialized Task Ledger.
    LedgerUnavailable,
    /// Goal and ledger revisions do not agree.
    GoalRevisionMismatch {
        /// Stable durable task identity.
        task_id: String,
        /// Latest Goal Contract revision.
        current_goal_revision: u32,
        /// Goal Contract revision referenced by the ledger.
        ledger_goal_revision: u32,
    },
    /// The requested step is absent or retired.
    StepUnavailable,
    /// No atomic index publication exists yet.
    NoPublishedIndex,
    /// A current bounded Lens is available.
    Available {
        /// Evidence-grounded temporary Lens.
        lens: Box<TaskLensV1>,
    },
}

/// Complete bounded Task Lens metadata, entries, claims, and durable anchors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensV1 {
    task_id: String,
    goal_revision: u32,
    ledger_revision: u32,
    ledger_store_version: String,
    step_id: String,
    index_run_id: String,
    snapshot_id: String,
    policy_version: u32,
    fusion_policy_version: u32,
    token_budget: u32,
    estimated_tokens: u32,
    goal_seed: String,
    step_seed: String,
    digest: String,
    excluded_stale_claims: u16,
    entries: Vec<TaskLensEntryV1>,
    claims: Vec<TaskLensClaimV1>,
    truncated: bool,
}

impl TaskLensV1 {
    /// Creates an application-validated current Lens projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        task_id: String,
        goal_revision: u32,
        ledger_revision: u32,
        ledger_store_version: String,
        step_id: String,
        index_run_id: String,
        snapshot_id: String,
        policy_version: u32,
        fusion_policy_version: u32,
        token_budget: u32,
        estimated_tokens: u32,
        goal_seed: String,
        step_seed: String,
        digest: String,
        excluded_stale_claims: u16,
        entries: Vec<TaskLensEntryV1>,
        claims: Vec<TaskLensClaimV1>,
        truncated: bool,
    ) -> Self {
        Self {
            task_id,
            goal_revision,
            ledger_revision,
            ledger_store_version,
            step_id,
            index_run_id,
            snapshot_id,
            policy_version,
            fusion_policy_version,
            token_budget,
            estimated_tokens,
            goal_seed,
            step_seed,
            digest,
            excluded_stale_claims,
            entries,
            claims,
            truncated,
        }
    }
}

/// One coarse-to-concrete selection in deterministic Lens order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensEntryV1 {
    position: u16,
    estimated_tokens: u32,
    reason: TaskLensEntryReasonV1,
    target: TaskLensEntryTargetV1,
}

impl TaskLensEntryV1 {
    /// Creates one entry at its contiguous one-based display position.
    #[must_use]
    pub const fn new(
        position: u16,
        estimated_tokens: u32,
        reason: TaskLensEntryReasonV1,
        target: TaskLensEntryTargetV1,
    ) -> Self {
        Self {
            position,
            estimated_tokens,
            reason,
            target,
        }
    }
}

/// Auditable reason a target entered the Lens.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum TaskLensEntryReasonV1 {
    /// Unconditional deterministic L0 repository anchor.
    RepositoryAnchor,
    /// Ordered R4 fusion selected the target.
    Retrieval {
        /// One-based rank in the fused result.
        rank: u16,
        /// Hard provenance band applied before score.
        priority: TaskLensPriorityV1,
        /// Weighted deterministic score inside the band.
        final_score: u32,
        /// Deduplicated channel-native source signals.
        sources: Vec<TaskLensRetrievalSourceV1>,
    },
    /// One current evidence-resolved claim expanded the target.
    Claim {
        /// Stable claim identity.
        claim_id: String,
    },
}

/// Hard R4 provenance band applied before weighted score.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensPriorityV1 {
    /// At least one exact deterministic source matched.
    Exact,
    /// At least one non-semantic evidence channel matched.
    Evidence,
    /// Only semantic similarity generated the candidate.
    Semantic,
}

/// One channel-native source retained by R4 fusion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensRetrievalSourceV1 {
    channel: TaskLensRetrievalChannelV1,
    normalized_score_basis_points: u16,
}

impl TaskLensRetrievalSourceV1 {
    /// Creates one bounded normalized channel signal.
    #[must_use]
    pub const fn new(
        channel: TaskLensRetrievalChannelV1,
        normalized_score_basis_points: u16,
    ) -> Self {
        Self {
            channel,
            normalized_score_basis_points,
        }
    }
}

/// Ordered retrieval channel; Semantic is explicitly candidate-only.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensRetrievalChannelV1 {
    /// Exact identifier, path, signature, prefix, or role.
    Exact,
    /// Weighted full-text candidate.
    Lexical,
    /// Evidence-graph relationship.
    Graph,
    /// Dedicated test relationship.
    Test,
    /// Evidence-grounded fresh memory candidate.
    Memory,
    /// Similarity-only candidate that is never proof.
    Semantic,
}

/// Safe repository-relative path display plus lossless canonical bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensPathV1 {
    path_display: String,
    path_hex: String,
}

impl TaskLensPathV1 {
    /// Creates a safe display plus lossless canonical-byte path projection.
    #[must_use]
    pub const fn new(path_display: String, path_hex: String) -> Self {
        Self {
            path_display,
            path_hex,
        }
    }
}

/// One typed L0-L3 Task Lens target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum TaskLensEntryTargetV1 {
    /// L0 deterministic repository summary.
    Repository {
        /// Module-formation policy version.
        module_policy_version: u32,
        /// Number of primary package/path modules.
        package_count: u32,
        /// Number of observed language families.
        language_count: u32,
        /// Number of bounded repository entrypoints.
        entrypoint_count: u32,
        /// Number of current indexed files.
        file_count: u32,
        /// Number of current structural symbols.
        symbol_count: u32,
    },
    /// L1 deterministic module boundary or graph community.
    Module {
        /// Stable module identity.
        module_id: String,
        /// Deterministic formation signal.
        module_kind: TaskLensModuleKindV1,
        /// Optional primary directory boundary.
        root: Option<TaskLensPathV1>,
        /// Current manifest revisions retained as direct evidence.
        manifests: Vec<ProjectMapSearchEvidenceV1>,
        /// Additional manifest revisions were omitted.
        manifests_truncated: bool,
    },
    /// L3 current file revision.
    File {
        /// Exact current revision metadata.
        evidence: ProjectMapSearchEvidenceV1,
    },
    /// L2 current structural symbol.
    Symbol {
        /// Stable structural identity.
        symbol_id: String,
        /// Language-neutral symbol category.
        symbol_kind: ProjectMapSearchSymbolKindV1,
        /// Bounded simple name.
        name: String,
        /// Optional adapter-derived signature.
        signature: Option<String>,
        /// Exact current declaration evidence.
        evidence: ProjectMapSearchEvidenceV1,
    },
    /// L3 exact declaration span selected from a symbol.
    SourceSpan {
        /// Owning structural symbol identity.
        symbol_id: String,
        /// Exact current declaration revision and range.
        evidence: ProjectMapSearchEvidenceV1,
    },
}

/// Deterministic module-formation signal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensModuleKindV1 {
    /// A package manifest established the boundary.
    ManifestBoundary,
    /// A deterministic path boundary established the module.
    PathBoundary,
    /// Evidence-graph community supplied an additional grouping.
    GraphCommunity,
}

/// Current evidence-resolved claim relevant to selected Lens targets.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaskLensClaimV1 {
    claim_id: String,
    module_id: String,
    kind: TaskLensClaimKindV1,
    polarity: TaskLensClaimPolarityV1,
    confidence_basis_points: u16,
    predicate: TaskLensClaimPredicateV1,
    evidence: Vec<TaskLensClaimEvidenceV1>,
}

impl TaskLensClaimV1 {
    /// Creates one current evidence-resolved claim projection.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        claim_id: String,
        module_id: String,
        kind: TaskLensClaimKindV1,
        polarity: TaskLensClaimPolarityV1,
        confidence_basis_points: u16,
        predicate: TaskLensClaimPredicateV1,
        evidence: Vec<TaskLensClaimEvidenceV1>,
    ) -> Self {
        Self {
            claim_id,
            module_id,
            kind,
            polarity,
            confidence_basis_points,
            predicate,
            evidence,
        }
    }
}

/// Epistemic classification independent from confidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensClaimKindV1 {
    /// Positive deterministic structural predicate with matching evidence.
    Fact,
    /// Fresh direct observation that is not a structural invariant.
    Observation,
    /// Explicitly unproven interpretation or negative absence claim.
    Hypothesis,
}

/// Whether the claim affirms or denies its predicate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskLensClaimPolarityV1 {
    /// The predicate is asserted to hold.
    Affirms,
    /// The predicate is asserted not to hold.
    Denies,
}

/// Typed proposition displayed without parsing untrusted prose for authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum TaskLensClaimPredicateV1 {
    /// A repository-relative path predicate.
    Path {
        /// Canonical path bytes and safe display.
        path: TaskLensPathV1,
    },
    /// A structural symbol-existence predicate.
    Symbol {
        /// Stable current symbol identity.
        symbol_id: String,
    },
    /// An exact deterministic graph-relation predicate.
    Relation {
        /// Exact source endpoint.
        source: ModuleDependencyEndpointV1,
        /// Exact target endpoint.
        target: ModuleDependencyEndpointV1,
        /// Language-neutral relationship.
        relation: ModuleDependencyRelationV1,
    },
    /// Fresh direct source or tool observation.
    Observed {
        /// Bounded observation statement.
        statement: String,
    },
    /// Architecture intent that deterministic evidence cannot prove.
    ArchitecturalIntent {
        /// Bounded explicitly unproven statement.
        statement: String,
    },
}

/// Exact current Evidence objects retained with one admitted claim.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "kind")]
pub enum TaskLensClaimEvidenceV1 {
    /// Exact current file revision.
    File {
        /// Stable Module Card evidence identity.
        evidence_id: String,
        /// Current revision metadata.
        revision: ProjectMapSearchEvidenceV1,
    },
    /// Exact current structural symbol.
    Symbol {
        /// Stable Module Card evidence identity.
        evidence_id: String,
        /// Stable symbol identity.
        symbol_id: String,
        /// Language-neutral symbol category.
        symbol_kind: ProjectMapSearchSymbolKindV1,
        /// Bounded simple name.
        name: String,
        /// Optional adapter-derived signature.
        signature: Option<String>,
        /// Exact current declaration revision and range.
        revision: ProjectMapSearchEvidenceV1,
    },
    /// Exact current deterministic graph edge.
    GraphEdge {
        /// Language-neutral relationship carried by the edge.
        relation: ModuleDependencyRelationV1,
        /// Complete source revision, range, endpoints, and linker basis.
        edge: ModuleDependencyEdgeEvidenceV1,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        CompileTaskLensRequestV1, QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1,
        TaskLensCompileResponseV1, TaskLensCompileResultV1, TaskLensTasksResponseV1,
        TaskLensTasksResultV1,
    };
    use crate::ProtocolVersion;

    #[test]
    fn requests_reject_paths_and_unknown_fields() {
        assert!(
            serde_json::from_str::<QueryTaskLensTasksRequestV1>(
                r#"{"protocolVersion":1,"path":"C:/secret"}"#,
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<QueryTaskLensTaskRequestV1>(
                r#"{"protocolVersion":1,"taskId":"00","root":"C:/secret"}"#,
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CompileTaskLensRequestV1>(
                r#"{"protocolVersion":1,"taskId":"00","stepId":"11","query":"fake"}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn pathless_task_request_retains_only_the_version() {
        assert_eq!(
            QueryTaskLensTasksRequestV1::new(ProtocolVersion::CURRENT).protocol_version(),
            ProtocolVersion::CURRENT
        );
    }

    #[test]
    fn expected_absence_states_are_not_command_failures() {
        assert!(matches!(
            TaskLensTasksResponseV1::no_project().result(),
            TaskLensTasksResultV1::NoProject
        ));
        assert!(matches!(
            TaskLensCompileResponseV1::no_published_index().result(),
            TaskLensCompileResultV1::NoPublishedIndex
        ));
    }
}

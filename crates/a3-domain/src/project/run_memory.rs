use super::{
    AgentRunId, GoalContract, GoalContractReference, GraphEndpoint, IndexRunId,
    ModuleClaimPolarity, ModuleClaimPredicate, PublishedIndex, RunEventSequence, SnapshotId,
    SyntaxRelationKind, TaskEvidenceId, TaskLedger, TaskLedgerRevision, TaskLensClaim,
    TaskStepAttemptNumber, TaskStepAttemptOutcome, TaskStepId, TaskStepResultSummary,
    TaskStepStatus, VerifiedClaimKind, VerifiedClaimStatus,
};
use std::error::Error;
use std::fmt;

const MAX_COMPACTED_STEP_RESULTS: usize = 1_024;
const MAX_COMPACTED_CLAIMS: usize = 128;
const MAX_OPEN_ISSUES: usize = 128;
const RUN_MEMORY_DIGEST_DOMAIN: &[u8] = b"a3.run-memory.v1";

/// Version of deterministic H8 run-memory materialization and digest semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunMemoryPolicyVersion {
    /// Initial ledger- and evidence-grounded compaction policy.
    V1,
}

impl RunMemoryPolicyVersion {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        match self {
            Self::V1 => 1,
        }
    }
}

/// Domain-separated identity of one normalized run-memory projection.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunMemoryDigest([u8; 32]);

impl RunMemoryDigest {
    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for RunMemoryDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for RunMemoryDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RunMemoryDigest({self})")
    }
}

/// Leaf identity proving where a compacted step result originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepResultSource {
    step_id: TaskStepId,
    attempt_number: TaskStepAttemptNumber,
    run_id: AgentRunId,
}

impl StepResultSource {
    /// Returns the immutable step definition owning the source attempt.
    #[must_use]
    pub const fn step_id(self) -> TaskStepId {
        self.step_id
    }

    /// Returns the immutable one-based attempt number.
    #[must_use]
    pub const fn attempt_number(self) -> TaskStepAttemptNumber {
        self.attempt_number
    }

    /// Returns the controlled run that produced the attempt.
    #[must_use]
    pub const fn run_id(self) -> AgentRunId {
        self.run_id
    }
}

/// One terminal ledger attempt retained without accepting a prior summary as its source.
#[derive(Clone, PartialEq, Eq)]
pub struct CompactedStepResult {
    source: StepResultSource,
    current_step_status: TaskStepStatus,
    outcome: TaskStepAttemptOutcome,
    summary: Option<TaskStepResultSummary>,
    evidence_ids: Vec<TaskEvidenceId>,
}

impl CompactedStepResult {
    /// Returns the original immutable attempt identity.
    #[must_use]
    pub const fn source(&self) -> StepResultSource {
        self.source
    }

    /// Returns the current materialized status, including later Stale transitions.
    #[must_use]
    pub const fn current_step_status(&self) -> TaskStepStatus {
        self.current_step_status
    }

    /// Returns the terminal attempt classification.
    #[must_use]
    pub const fn outcome(&self) -> &TaskStepAttemptOutcome {
        &self.outcome
    }

    /// Returns the bounded result text authored at the original attempt boundary.
    #[must_use]
    pub const fn summary(&self) -> Option<&TaskStepResultSummary> {
        self.summary.as_ref()
    }

    /// Returns canonical direct and verification Evidence IDs supporting the result.
    #[must_use]
    pub fn evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.evidence_ids
    }
}

impl fmt::Debug for CompactedStepResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompactedStepResult")
            .field("source", &self.source)
            .field("current_step_status", &self.current_step_status)
            .field("outcome", &self.outcome)
            .field("has_summary", &self.summary.is_some())
            .field("evidence_count", &self.evidence_ids.len())
            .finish()
    }
}

/// Open condition that must survive removal of redundant run text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OpenRunIssueKind {
    /// A deterministic verification failed and the step became ready to retry.
    VerificationFailed,
    /// Execution recorded a blocker.
    Blocked,
    /// A scoped user approval is still required.
    AwaitingApproval,
    /// Execution failed terminally.
    Failed,
    /// Execution was cancelled before completion.
    Cancelled,
    /// Previously completed verification evidence became stale.
    Stale,
}

/// One source-bound open issue derived from current Task Ledger state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpenRunIssue {
    step_id: TaskStepId,
    kind: OpenRunIssueKind,
    source: Option<StepResultSource>,
}

impl OpenRunIssue {
    /// Returns the current-plan step requiring attention.
    #[must_use]
    pub const fn step_id(self) -> TaskStepId {
        self.step_id
    }

    /// Returns the stable issue classification.
    #[must_use]
    pub const fn kind(self) -> OpenRunIssueKind {
        self.kind
    }

    /// Returns the latest attempt source when one exists.
    #[must_use]
    pub const fn source(self) -> Option<StepResultSource> {
        self.source
    }
}

/// Fresh original claim retained with its own Claim and Evidence IDs.
#[derive(Clone, PartialEq, Eq)]
pub struct CompactedRunClaim(TaskLensClaim);

impl CompactedRunClaim {
    /// Returns the original durable claim projection without changing its classification.
    #[must_use]
    pub const fn claim(&self) -> &TaskLensClaim {
        &self.0
    }
}

impl fmt::Debug for CompactedRunClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompactedRunClaim")
            .field("id", &self.0.id())
            .field("kind", &self.0.kind())
            .field("evidence_count", &self.0.evidence().len())
            .finish_non_exhaustive()
    }
}

/// Fresh deterministic memory for the next Context Pack; never a replacement for the journal.
#[derive(Clone, PartialEq, Eq)]
pub struct RunMemoryCheckpoint {
    policy_version: RunMemoryPolicyVersion,
    goal_contract: GoalContractReference,
    ledger_revision: TaskLedgerRevision,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    through_event_sequence: RunEventSequence,
    step_results: Vec<CompactedStepResult>,
    claims: Vec<CompactedRunClaim>,
    open_issues: Vec<OpenRunIssue>,
    excluded_stale_claims: u16,
    digest: RunMemoryDigest,
}

impl RunMemoryCheckpoint {
    /// Rebuilds run memory from authoritative Goal, Ledger, and original claim projections only.
    pub fn compile(
        goal: &GoalContract,
        ledger: &TaskLedger,
        published: &PublishedIndex,
        through_event_sequence: RunEventSequence,
        claims: Vec<TaskLensClaim>,
    ) -> Result<Self, RunMemoryCompileError> {
        if ledger.goal_contract() != goal.reference() {
            return Err(RunMemoryCompileError::GoalLedgerMismatch);
        }
        let step_results = compact_step_results(ledger)?;
        let open_issues = collect_open_issues(ledger)?;
        let index_run_id = published.run().id();
        let snapshot_id = published.run().snapshot_id();
        let (claims, excluded_stale_claims) = compact_claims(claims, published)?;
        let digest = run_memory_digest(
            goal,
            ledger,
            index_run_id,
            snapshot_id,
            through_event_sequence,
            &step_results,
            &claims,
            &open_issues,
            excluded_stale_claims,
        );
        Ok(Self {
            policy_version: RunMemoryPolicyVersion::V1,
            goal_contract: goal.reference(),
            ledger_revision: ledger.revision(),
            index_run_id,
            snapshot_id,
            through_event_sequence,
            step_results,
            claims,
            open_issues,
            excluded_stale_claims,
            digest,
        })
    }

    /// Returns the compaction policy governing this normalized projection.
    #[must_use]
    pub const fn policy_version(&self) -> RunMemoryPolicyVersion {
        self.policy_version
    }

    /// Returns the exact Goal Contract anchor that must be re-injected separately in full.
    #[must_use]
    pub const fn goal_contract(&self) -> GoalContractReference {
        self.goal_contract
    }

    /// Returns the authoritative ledger revision used for materialization.
    #[must_use]
    pub const fn ledger_revision(&self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the exact published index run used for retained claims.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the immutable snapshot used for retained claims.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the journal watermark observed without deleting or rewriting any event.
    #[must_use]
    pub const fn through_event_sequence(&self) -> RunEventSequence {
        self.through_event_sequence
    }

    /// Returns terminal step attempts in stable Step ID and attempt order.
    #[must_use]
    pub fn step_results(&self) -> &[CompactedStepResult] {
        &self.step_results
    }

    /// Returns only fresh original claims; no compacted summary is accepted as a source.
    #[must_use]
    pub fn claims(&self) -> &[CompactedRunClaim] {
        &self.claims
    }

    /// Returns current blockers, failed verification, cancellation, and stale conditions.
    #[must_use]
    pub fn open_issues(&self) -> &[OpenRunIssue] {
        &self.open_issues
    }

    /// Iterates active original hypotheses that must remain explicit in the next Context Pack.
    pub fn open_hypotheses(&self) -> impl Iterator<Item = &CompactedRunClaim> {
        self.claims
            .iter()
            .filter(|claim| claim.claim().kind() == VerifiedClaimKind::Hypothesis)
    }

    /// Returns claims rejected for stale status or another run/snapshot.
    #[must_use]
    pub const fn excluded_stale_claims(&self) -> u16 {
        self.excluded_stale_claims
    }

    /// Returns the normalized identity used by persistence and Context Pack reinjection.
    #[must_use]
    pub const fn digest(&self) -> RunMemoryDigest {
        self.digest
    }
}

impl fmt::Debug for RunMemoryCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunMemoryCheckpoint")
            .field("policy_version", &self.policy_version)
            .field("goal_contract", &self.goal_contract)
            .field("ledger_revision", &self.ledger_revision)
            .field("index_run_id", &self.index_run_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("through_event_sequence", &self.through_event_sequence)
            .field("step_result_count", &self.step_results.len())
            .field("claim_count", &self.claims.len())
            .field("open_issue_count", &self.open_issues.len())
            .field("excluded_stale_claims", &self.excluded_stale_claims)
            .field("digest", &self.digest)
            .finish()
    }
}

/// Authoritative input was mismatched or exceeded a fixed compaction boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMemoryCompileError {
    /// Task Ledger serves another Goal Contract revision.
    GoalLedgerMismatch,
    /// More terminal attempts exist than one checkpoint may retain.
    TooManyStepResults,
    /// More claim projections were supplied than one checkpoint may inspect.
    TooManyClaims,
    /// Claim input repeated one original Claim ID.
    DuplicateClaim,
    /// More current open conditions exist than one checkpoint may retain.
    TooManyOpenIssues,
}

impl fmt::Display for RunMemoryCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::GoalLedgerMismatch => "run memory goal and Task Ledger do not match",
            Self::TooManyStepResults => "run memory exceeds the terminal step-result limit",
            Self::TooManyClaims => "run memory exceeds the claim limit",
            Self::DuplicateClaim => "run memory repeats an original claim identity",
            Self::TooManyOpenIssues => "run memory exceeds the open-issue limit",
        })
    }
}

impl Error for RunMemoryCompileError {}

fn compact_step_results(
    ledger: &TaskLedger,
) -> Result<Vec<CompactedStepResult>, RunMemoryCompileError> {
    let mut results = Vec::new();
    for step in ledger.steps() {
        for attempt in step.attempts() {
            if matches!(attempt.outcome(), TaskStepAttemptOutcome::Active) {
                continue;
            }
            if results.len() == MAX_COMPACTED_STEP_RESULTS {
                return Err(RunMemoryCompileError::TooManyStepResults);
            }
            let mut evidence_ids = attempt.evidence_ids().to_vec();
            if let Some(verification) = attempt.verification() {
                evidence_ids.extend_from_slice(verification.evidence_ids());
            }
            evidence_ids.sort();
            evidence_ids.dedup();
            results.push(CompactedStepResult {
                source: StepResultSource {
                    step_id: step.definition().id(),
                    attempt_number: attempt.number(),
                    run_id: attempt.run_id(),
                },
                current_step_status: step.status(),
                outcome: attempt.outcome().clone(),
                summary: attempt.result_summary().cloned(),
                evidence_ids,
            });
        }
    }
    results.sort_by_key(|result| (result.source.step_id, result.source.attempt_number));
    Ok(results)
}

fn compact_claims(
    mut claims: Vec<TaskLensClaim>,
    published: &PublishedIndex,
) -> Result<(Vec<CompactedRunClaim>, u16), RunMemoryCompileError> {
    if claims.len() > MAX_COMPACTED_CLAIMS {
        return Err(RunMemoryCompileError::TooManyClaims);
    }
    claims.sort_by_key(TaskLensClaim::id);
    if claims.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
        return Err(RunMemoryCompileError::DuplicateClaim);
    }
    let mut excluded = 0_u16;
    let retained = claims
        .into_iter()
        .filter_map(|claim| {
            if claim.is_current_for(published) {
                Some(CompactedRunClaim(claim))
            } else {
                excluded = excluded.saturating_add(1);
                None
            }
        })
        .collect();
    Ok((retained, excluded))
}

fn collect_open_issues(ledger: &TaskLedger) -> Result<Vec<OpenRunIssue>, RunMemoryCompileError> {
    let mut issues = Vec::new();
    for step in ledger.steps().filter(|step| step.is_active_plan_step()) {
        let latest = step.attempts().last();
        let source = latest.map(|attempt| StepResultSource {
            step_id: step.definition().id(),
            attempt_number: attempt.number(),
            run_id: attempt.run_id(),
        });
        let kind = match step.status() {
            TaskStepStatus::Ready
                if latest.is_some_and(|attempt| {
                    matches!(
                        attempt.outcome(),
                        TaskStepAttemptOutcome::VerificationFailed
                    )
                }) =>
            {
                Some(OpenRunIssueKind::VerificationFailed)
            }
            TaskStepStatus::Blocked => Some(OpenRunIssueKind::Blocked),
            TaskStepStatus::AwaitingApproval => Some(OpenRunIssueKind::AwaitingApproval),
            TaskStepStatus::Failed => Some(OpenRunIssueKind::Failed),
            TaskStepStatus::Cancelled => Some(OpenRunIssueKind::Cancelled),
            TaskStepStatus::Stale => Some(OpenRunIssueKind::Stale),
            TaskStepStatus::Pending
            | TaskStepStatus::Ready
            | TaskStepStatus::InProgress
            | TaskStepStatus::Verifying
            | TaskStepStatus::Completed => None,
        };
        if let Some(kind) = kind {
            if issues.len() == MAX_OPEN_ISSUES {
                return Err(RunMemoryCompileError::TooManyOpenIssues);
            }
            issues.push(OpenRunIssue {
                step_id: step.definition().id(),
                kind,
                source,
            });
        }
    }
    issues.sort();
    Ok(issues)
}

#[allow(clippy::too_many_arguments)]
fn run_memory_digest(
    goal: &GoalContract,
    ledger: &TaskLedger,
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    through_event_sequence: RunEventSequence,
    results: &[CompactedStepResult],
    claims: &[CompactedRunClaim],
    issues: &[OpenRunIssue],
    excluded_stale_claims: u16,
) -> RunMemoryDigest {
    let mut hasher = blake3::Hasher::new();
    hash_bytes(&mut hasher, RUN_MEMORY_DIGEST_DOMAIN);
    hash_u32(&mut hasher, RunMemoryPolicyVersion::V1.get());
    hash_bytes(&mut hasher, goal.task_id().as_bytes());
    hash_u32(&mut hasher, goal.revision().get());
    hash_u32(&mut hasher, ledger.revision().get());
    hash_bytes(&mut hasher, index_run_id.as_bytes());
    hash_bytes(&mut hasher, snapshot_id.as_bytes());
    hash_u64(&mut hasher, through_event_sequence.get());
    hash_u64(&mut hasher, results.len() as u64);
    for result in results {
        hash_step_source(&mut hasher, result.source);
        hasher.update(&[step_status_tag(result.current_step_status)]);
        hash_attempt_outcome(&mut hasher, &result.outcome);
        hash_optional_text(
            &mut hasher,
            result.summary.as_ref().map(TaskStepResultSummary::as_str),
        );
        hash_u64(&mut hasher, result.evidence_ids.len() as u64);
        for evidence_id in &result.evidence_ids {
            hash_bytes(&mut hasher, evidence_id.as_bytes());
        }
    }
    hash_u64(&mut hasher, claims.len() as u64);
    for claim in claims {
        hash_claim(&mut hasher, claim.claim());
    }
    hash_u64(&mut hasher, issues.len() as u64);
    for issue in issues {
        hash_bytes(&mut hasher, issue.step_id.as_bytes());
        hasher.update(&[open_issue_tag(issue.kind)]);
        match issue.source {
            Some(source) => {
                hasher.update(&[1]);
                hash_step_source(&mut hasher, source);
            }
            None => {
                hasher.update(&[0]);
            }
        }
    }
    hasher.update(&excluded_stale_claims.to_le_bytes());
    RunMemoryDigest(*hasher.finalize().as_bytes())
}

fn hash_claim(hasher: &mut blake3::Hasher, claim: &TaskLensClaim) {
    hash_bytes(hasher, claim.source_index_run_id().as_bytes());
    hash_bytes(hasher, claim.snapshot_id().as_bytes());
    hash_bytes(hasher, claim.id().as_bytes());
    hash_bytes(hasher, claim.module_id().as_bytes());
    hasher.update(&[claim_polarity_tag(claim.polarity())]);
    hash_claim_predicate(hasher, claim.predicate());
    hasher.update(&[verified_claim_kind_tag(claim.kind())]);
    hasher.update(&[verified_claim_status_tag(claim.status())]);
    hasher.update(&claim.confidence().basis_points().to_le_bytes());
    hash_u64(hasher, claim.evidence().len() as u64);
    for evidence in claim.evidence() {
        hash_bytes(hasher, evidence.id().as_bytes());
    }
}

fn hash_claim_predicate(hasher: &mut blake3::Hasher, predicate: &ModuleClaimPredicate) {
    match predicate {
        ModuleClaimPredicate::Path(path) => {
            hasher.update(&[0]);
            hash_bytes(hasher, path.as_bytes());
        }
        ModuleClaimPredicate::Symbol(symbol_id) => {
            hasher.update(&[1]);
            hash_bytes(hasher, symbol_id.as_bytes());
        }
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => {
            hasher.update(&[2, relation_kind_tag(*kind)]);
            hash_graph_endpoint(hasher, source);
            hash_graph_endpoint(hasher, target);
        }
        ModuleClaimPredicate::Observed(statement) => {
            hasher.update(&[3]);
            hash_bytes(hasher, statement.as_str().as_bytes());
        }
        ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            hasher.update(&[4]);
            hash_bytes(hasher, statement.as_str().as_bytes());
        }
    }
}

fn hash_graph_endpoint(hasher: &mut blake3::Hasher, endpoint: &GraphEndpoint) {
    match endpoint {
        GraphEndpoint::File(path) => {
            hasher.update(&[0]);
            hash_bytes(hasher, path.as_bytes());
        }
        GraphEndpoint::Symbol(symbol_id) => {
            hasher.update(&[1]);
            hash_bytes(hasher, symbol_id.as_bytes());
        }
    }
}

fn hash_step_source(hasher: &mut blake3::Hasher, source: StepResultSource) {
    hash_bytes(hasher, source.step_id.as_bytes());
    hash_u32(hasher, source.attempt_number.get());
    hash_bytes(hasher, source.run_id.as_bytes());
}

fn hash_attempt_outcome(hasher: &mut blake3::Hasher, outcome: &TaskStepAttemptOutcome) {
    match outcome {
        TaskStepAttemptOutcome::Active => {
            hasher.update(&[0]);
        }
        TaskStepAttemptOutcome::Blocked { reason } => {
            hasher.update(&[1]);
            hash_bytes(hasher, reason.as_str().as_bytes());
        }
        TaskStepAttemptOutcome::VerificationFailed => {
            hasher.update(&[2]);
        }
        TaskStepAttemptOutcome::Completed => {
            hasher.update(&[3]);
        }
        TaskStepAttemptOutcome::Failed { reason } => {
            hasher.update(&[4]);
            hash_bytes(hasher, reason.as_str().as_bytes());
        }
        TaskStepAttemptOutcome::Cancelled { reason } => {
            hasher.update(&[5]);
            hash_bytes(hasher, reason.as_str().as_bytes());
        }
    }
}

fn hash_optional_text(hasher: &mut blake3::Hasher, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hash_bytes(hasher, value.as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn hash_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value);
}

fn hash_u32(hasher: &mut blake3::Hasher, value: u32) {
    hasher.update(&value.to_le_bytes());
}

fn hash_u64(hasher: &mut blake3::Hasher, value: u64) {
    hasher.update(&value.to_le_bytes());
}

const fn step_status_tag(status: TaskStepStatus) -> u8 {
    match status {
        TaskStepStatus::Pending => 0,
        TaskStepStatus::Ready => 1,
        TaskStepStatus::InProgress => 2,
        TaskStepStatus::Blocked => 3,
        TaskStepStatus::AwaitingApproval => 4,
        TaskStepStatus::Verifying => 5,
        TaskStepStatus::Completed => 6,
        TaskStepStatus::Failed => 7,
        TaskStepStatus::Cancelled => 8,
        TaskStepStatus::Stale => 9,
    }
}

const fn open_issue_tag(kind: OpenRunIssueKind) -> u8 {
    match kind {
        OpenRunIssueKind::VerificationFailed => 0,
        OpenRunIssueKind::Blocked => 1,
        OpenRunIssueKind::AwaitingApproval => 2,
        OpenRunIssueKind::Failed => 3,
        OpenRunIssueKind::Cancelled => 4,
        OpenRunIssueKind::Stale => 5,
    }
}

const fn claim_polarity_tag(polarity: ModuleClaimPolarity) -> u8 {
    match polarity {
        ModuleClaimPolarity::Affirms => 0,
        ModuleClaimPolarity::Denies => 1,
    }
}

const fn verified_claim_kind_tag(kind: VerifiedClaimKind) -> u8 {
    match kind {
        VerifiedClaimKind::Fact => 0,
        VerifiedClaimKind::Observation => 1,
        VerifiedClaimKind::Hypothesis => 2,
    }
}

const fn verified_claim_status_tag(status: VerifiedClaimStatus) -> u8 {
    match status {
        VerifiedClaimStatus::Active => 0,
        VerifiedClaimStatus::Stale => 1,
    }
}

const fn relation_kind_tag(kind: SyntaxRelationKind) -> u8 {
    match kind {
        SyntaxRelationKind::Contains => 0,
        SyntaxRelationKind::Defines => 1,
        SyntaxRelationKind::Imports => 2,
        SyntaxRelationKind::Exports => 3,
        SyntaxRelationKind::Calls => 4,
        SyntaxRelationKind::Implements => 5,
        SyntaxRelationKind::Extends => 6,
        SyntaxRelationKind::Reads => 7,
        SyntaxRelationKind::Writes => 8,
        SyntaxRelationKind::Configures => 9,
        SyntaxRelationKind::Tests => 10,
        SyntaxRelationKind::Builds => 11,
        SyntaxRelationKind::Documents => 12,
    }
}

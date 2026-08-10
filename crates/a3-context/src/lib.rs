//! Deterministic Anchor, Retrieve, Rank, Pack, Validate Context Compiler for A^3.

mod agent_read_tools;
mod digest;
mod security;

pub use agent_read_tools::DeterministicAgentReadTools;

use a3_application::{
    AgentContextCompileInput, AgentContextCompiler, AgentContextCompilerFuture,
    AgentPromptContract, CompileTaskLens, CompileTaskLensFailure, CompiledAgentContext,
    ContextCompileControl, ContextCompileFailure, ContextCompilePhase, ContextToolResult,
    ContextToolResultStatus, ModelMessage, ModelMessageRole, ModelProviderRequest, TaskLensControl,
    TaskLensControlError,
};
use a3_domain::{
    ContextBudgetPlan, ContextBudgetUsage, ContextCompilerPolicyVersion, ContextSection,
    EvidenceRef, GoalContract, GraphEndpoint, GraphSymbol, ModelProfile, ModuleCardClaimId,
    ModuleClaimPolarity, ModuleClaimPredicate, ModuleKind, ModuleRoot, OpenRunIssueKind, Progress,
    RepositoryCard, RepositoryModule, RepositoryPath, RunMemoryCheckpoint, SnapshotId, SymbolRole,
    TaskLedger, TaskLens, TaskLensClaim, TaskLensEntry, TaskLensEntryReason, TaskLensSeedSet,
    TaskLensSeedText, TaskLensTarget, TaskLensTokenBudget, TaskStep, TaskStepAttemptOutcome,
    TaskStepStatus, VerifiedClaimKind, VerifiedClaimStatus,
};
use std::collections::BTreeSet;
use std::fmt;

use digest::context_digest;
use security::reject_secret_candidate;

const MAX_TASK_LENS_SEED_BYTES: usize = 4 * 1_024;
const MIN_TASK_LENS_BUDGET: u32 = 256;
const MAX_TASK_LENS_BUDGET: u32 = 32_768;
const CONTEXT_PACK_HEADER: &str = "A3_CONTEXT_PACK_V1\n";
const CODE_AND_EVIDENCE_HEADER: &str = "[CODE_AND_EVIDENCE]\n";

/// Concrete feature implementation composing the existing ordered Task Lens retrieval use case.
#[derive(Debug, Clone, Copy)]
pub struct DeterministicAgentContextCompiler<'a> {
    task_lens: CompileTaskLens<'a>,
}

impl<'a> DeterministicAgentContextCompiler<'a> {
    /// Uses the existing exact through optional-semantic Task Lens as the sole Retrieve/Rank path.
    #[must_use]
    pub const fn new(task_lens: CompileTaskLens<'a>) -> Self {
        Self { task_lens }
    }

    async fn execute(
        self,
        input: &AgentContextCompileInput,
        control: &dyn ContextCompileControl,
    ) -> Result<CompiledAgentContext, ContextCompileFailure> {
        check_cancelled(control)?;
        report(control, ContextCompilePhase::Anchor)?;

        let profile = input.model_profile();
        let budget_plan =
            ContextBudgetPlan::for_profile(profile).map_err(ContextCompileFailure::Budget)?;
        let prompt = AgentPromptContract::current()
            .prepare(profile)
            .map_err(|_| ContextCompileFailure::PromptUnavailable)?;
        let current_step = input
            .task_ledger()
            .step(input.current_step_id())
            .filter(|step| step.is_active_plan_step())
            .ok_or(ContextCompileFailure::StaleOrMismatchedInput)?;
        let anchor = render_anchor(
            input.goal_contract(),
            input.task_ledger(),
            current_step,
            profile,
        );
        reject_secret_candidate(&anchor)?;
        let goal_tokens = count(profile, &anchor)?;
        if goal_tokens > budget_plan.allowance(ContextSection::GoalAndLedger) {
            return Err(ContextCompileFailure::AnchorTooLarge);
        }

        let goal_seed = TaskLensSeedText::try_from_string(bounded_seed(
            input.goal_contract().draft().objective().as_str(),
        ))
        .map_err(|_| ContextCompileFailure::InvalidPack)?;
        let step_seed = TaskLensSeedText::try_from_string(bounded_seed(
            current_step.definition().intended_outcome().as_str(),
        ))
        .map_err(|_| ContextCompileFailure::InvalidPack)?;
        let seeds = TaskLensSeedSet::new(goal_seed, step_seed, input.supplemental_seeds().to_vec())
            .map_err(|_| ContextCompileFailure::InvalidPack)?;
        let lens_budget = lens_budget(budget_plan)?;
        let lens_control = ContextTaskLensControl { outer: control };
        let lens = self
            .task_lens
            .execute(input.project(), seeds, lens_budget, &lens_control)
            .await
            .map_err(map_task_lens_failure)?;

        let run_memory = pack_run_memory(input.run_memory(), &lens, profile, budget_plan)?;

        check_cancelled(control)?;
        report(control, ContextCompilePhase::Pack)?;
        let (system_message, schema_grounding, structured_output) = prompt.into_parts();
        let system_tokens = count(profile, system_message.content())?
            .checked_add(match schema_grounding.as_ref() {
                Some(message) => count(profile, message.content())?,
                None => 0,
            })
            .ok_or(ContextCompileFailure::InvalidPack)?;
        if system_tokens > budget_plan.allowance(ContextSection::SystemAndTools) {
            return Err(ContextCompileFailure::Budget(
                a3_domain::ContextBudgetError::SectionExceeded {
                    section: ContextSection::SystemAndTools,
                    actual: system_tokens,
                    allowance: budget_plan.allowance(ContextSection::SystemAndTools),
                },
            ));
        }

        let packed = pack_ranked_context(
            &lens,
            &run_memory.claim_ids,
            run_memory.tokens,
            input.tool_results(),
            profile,
            budget_plan,
        )?;
        let context_message_text = format!(
            "{CONTEXT_PACK_HEADER}{anchor}{}{}{}{}{}",
            run_memory.text,
            packed.project_map,
            packed.code_and_evidence,
            packed.tool_results,
            packed.pack_state
        );
        reject_secret_candidate(&context_message_text)?;
        let context_message =
            ModelMessage::try_from_string(ModelMessageRole::User, context_message_text)
                .map_err(|_| ContextCompileFailure::InvalidPack)?;

        let budget_usage = ContextBudgetUsage::new(
            budget_plan,
            system_tokens,
            goal_tokens,
            packed.project_tokens,
            packed.code_tokens,
            packed.tool_tokens,
        )
        .map_err(ContextCompileFailure::Budget)?;

        check_cancelled(control)?;
        report(control, ContextCompilePhase::Validate)?;
        let mut messages = vec![system_message];
        if let Some(schema_grounding) = schema_grounding {
            messages.push(schema_grounding);
        }
        messages.push(context_message);
        let digest = context_digest(
            profile,
            input,
            &lens,
            budget_plan,
            budget_usage,
            &messages,
            structured_output.value().to_string().as_str(),
        );
        let request = ModelProviderRequest::new(profile.clone(), messages, Some(structured_output))
            .map_err(|_| ContextCompileFailure::InvalidPack)?;
        report(control, ContextCompilePhase::Complete)?;
        Ok(CompiledAgentContext::new(
            request,
            ContextCompilerPolicyVersion::CURRENT,
            digest,
            input.goal_contract().reference(),
            input.task_ledger().revision(),
            input.current_step_id(),
            lens.index_run_id(),
            lens.snapshot_id(),
            lens.digest(),
            input.run_memory().map(RunMemoryCheckpoint::digest),
            budget_plan,
            budget_usage,
            lens.excluded_stale_claims(),
            run_memory.truncated || packed.truncated || lens.truncated(),
        ))
    }
}

impl AgentContextCompiler for DeterministicAgentContextCompiler<'_> {
    fn compile<'a>(
        &'a self,
        input: &'a AgentContextCompileInput,
        control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        Box::pin(async move { self.execute(input, control).await })
    }
}

#[derive(Debug)]
struct ContextTaskLensControl<'a> {
    outer: &'a dyn ContextCompileControl,
}

impl TaskLensControl for ContextTaskLensControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.outer.is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), TaskLensControlError> {
        let phase = match (progress.completed(), progress.total()) {
            (Some(completed), Some(total)) if completed.saturating_mul(2) >= total => {
                ContextCompilePhase::Rank
            }
            _ => ContextCompilePhase::Retrieve,
        };
        self.outer.report_phase(phase)
    }
}

fn check_cancelled(control: &dyn ContextCompileControl) -> Result<(), ContextCompileFailure> {
    if control.is_cancelled() {
        Err(ContextCompileFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn report(
    control: &dyn ContextCompileControl,
    phase: ContextCompilePhase,
) -> Result<(), ContextCompileFailure> {
    control
        .report_phase(phase)
        .map_err(|_| ContextCompileFailure::ProgressUnavailable)
}

fn map_task_lens_failure(failure: CompileTaskLensFailure) -> ContextCompileFailure {
    match failure {
        CompileTaskLensFailure::IndexUnavailable => ContextCompileFailure::IndexUnavailable,
        CompileTaskLensFailure::Cancelled => ContextCompileFailure::Cancelled,
        CompileTaskLensFailure::TimedOut => ContextCompileFailure::TimedOut,
        CompileTaskLensFailure::ProgressUnavailable => ContextCompileFailure::ProgressUnavailable,
        CompileTaskLensFailure::Index(_)
        | CompileTaskLensFailure::Search(_)
        | CompileTaskLensFailure::Claims(_)
        | CompileTaskLensFailure::Semantic(_)
        | CompileTaskLensFailure::InvalidSeedQuery
        | CompileTaskLensFailure::InvalidChannelProjection
        | CompileTaskLensFailure::CandidateSet(_)
        | CompileTaskLensFailure::CandidateSets(_)
        | CompileTaskLensFailure::Fusion(_)
        | CompileTaskLensFailure::Compile(_)
        | CompileTaskLensFailure::ResourceLimit => ContextCompileFailure::RetrievalFailed,
    }
}

fn lens_budget(plan: ContextBudgetPlan) -> Result<TaskLensTokenBudget, ContextCompileFailure> {
    let combined = plan
        .allowance(ContextSection::ProjectMap)
        .checked_add(plan.allowance(ContextSection::CodeAndEvidence))
        .ok_or(ContextCompileFailure::InvalidPack)?;
    TaskLensTokenBudget::new(combined.clamp(MIN_TASK_LENS_BUDGET, MAX_TASK_LENS_BUDGET))
        .map_err(|_| ContextCompileFailure::InvalidPack)
}

fn bounded_seed(value: &str) -> String {
    if value.len() <= MAX_TASK_LENS_SEED_BYTES {
        return value.to_owned();
    }
    let mut boundary = MAX_TASK_LENS_SEED_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].trim_end().to_owned()
}

fn render_anchor(
    goal: &GoalContract,
    ledger: &TaskLedger,
    current_step: &TaskStep,
    profile: &ModelProfile,
) -> String {
    let mut text = String::new();
    text.push_str("[ANCHOR]\n");
    push_line(
        &mut text,
        format_args!(
            "policy={} task={} goal_rev={} ledger_rev={} profile={}:{} context={} output={}",
            ContextCompilerPolicyVersion::CURRENT.get(),
            goal.task_id(),
            goal.revision().get(),
            ledger.revision().get(),
            profile.id(),
            profile.version().get(),
            profile.settings().context_limit().get(),
            profile.settings().output_limit().get()
        ),
    );
    push_line(
        &mut text,
        format_args!("objective={}", goal.draft().objective().as_str()),
    );
    text.push_str("acceptance=\n");
    for criterion in goal.draft().acceptance_criteria() {
        push_line(
            &mut text,
            format_args!(
                "- {} requirement={} statement={}",
                criterion.id(),
                acceptance_requirement(criterion.requirement()),
                criterion.statement().as_str()
            ),
        );
    }
    for constraint in goal.draft().constraints() {
        push_line(
            &mut text,
            format_args!("constraint={}", constraint.as_str()),
        );
    }
    for non_goal in goal.draft().non_goals() {
        push_line(&mut text, format_args!("non_goal={}", non_goal.as_str()));
    }
    for decision in goal.draft().user_decisions() {
        push_line(
            &mut text,
            format_args!("user_decision={}", decision.as_str()),
        );
    }
    push_line(
        &mut text,
        format_args!(
            "success_verification={}",
            goal.draft().success_verification().as_str()
        ),
    );
    let definition = current_step.definition();
    push_line(
        &mut text,
        format_args!(
            "current_step={} status={} outcome={}",
            definition.id(),
            step_status(current_step.status()),
            definition.intended_outcome().as_str()
        ),
    );
    push_line(
        &mut text,
        format_args!(
            "verification_spec={} requirement={}",
            definition.verification_spec().id(),
            definition.verification_spec().requirement().as_str()
        ),
    );
    render_verification_target(&mut text, definition.verification_spec());
    if definition.acceptance_criteria().is_empty() {
        text.push_str("step_acceptance=legacy_unmapped\n");
    } else {
        for criterion_id in definition.acceptance_criteria() {
            push_line(&mut text, format_args!("step_acceptance={criterion_id}"));
        }
    }
    let (pending, active, completed, stale, blocked) = ledger_status_counts(ledger);
    push_line(
        &mut text,
        format_args!(
            "ledger_status pending={pending} active={active} completed={completed} stale={stale} blocked={blocked}"
        ),
    );
    text
}

fn ledger_status_counts(ledger: &TaskLedger) -> (u32, u32, u32, u32, u32) {
    let mut pending = 0_u32;
    let mut active = 0_u32;
    let mut completed = 0_u32;
    let mut stale = 0_u32;
    let mut blocked = 0_u32;
    for step in ledger.steps().filter(|step| step.is_active_plan_step()) {
        match step.status() {
            TaskStepStatus::Pending | TaskStepStatus::Ready => pending = pending.saturating_add(1),
            TaskStepStatus::InProgress
            | TaskStepStatus::AwaitingApproval
            | TaskStepStatus::Verifying => active = active.saturating_add(1),
            TaskStepStatus::Completed => completed = completed.saturating_add(1),
            TaskStepStatus::Stale => stale = stale.saturating_add(1),
            TaskStepStatus::Blocked | TaskStepStatus::Failed | TaskStepStatus::Cancelled => {
                blocked = blocked.saturating_add(1)
            }
        }
    }
    (pending, active, completed, stale, blocked)
}

struct PackedSections {
    project_map: String,
    code_and_evidence: String,
    tool_results: String,
    pack_state: String,
    project_tokens: u32,
    code_tokens: u32,
    tool_tokens: u32,
    truncated: bool,
}

struct PackedRunMemory {
    text: String,
    tokens: u32,
    claim_ids: BTreeSet<ModuleCardClaimId>,
    truncated: bool,
}

fn pack_run_memory(
    checkpoint: Option<&RunMemoryCheckpoint>,
    lens: &TaskLens,
    profile: &ModelProfile,
    budget: ContextBudgetPlan,
) -> Result<PackedRunMemory, ContextCompileFailure> {
    let Some(checkpoint) = checkpoint else {
        return Ok(PackedRunMemory {
            text: String::new(),
            tokens: 0,
            claim_ids: BTreeSet::new(),
            truncated: false,
        });
    };
    if checkpoint.index_run_id() != lens.index_run_id()
        || checkpoint.snapshot_id() != lens.snapshot_id()
    {
        return Err(ContextCompileFailure::StaleOrMismatchedInput);
    }

    let mut text = String::from("[RUN_MEMORY]\n");
    push_line(
        &mut text,
        format_args!(
            "policy={} digest={} run={} through_event={} excluded_stale_claims={}",
            checkpoint.policy_version().get(),
            checkpoint.digest(),
            checkpoint.run_id(),
            checkpoint.through_event_sequence().get(),
            checkpoint.excluded_stale_claims()
        ),
    );
    reject_secret_candidate(&text)?;
    let allowance = budget.allowance(ContextSection::CodeAndEvidence);
    let reserved_tokens = count(profile, CODE_AND_EVIDENCE_HEADER)?;
    let mut tokens = count(profile, &text)?;
    ensure_memory_fits(reserved_tokens, tokens, allowance)?;
    let mut claim_ids = BTreeSet::new();

    for issue in checkpoint.open_issues() {
        let outcome = issue.source().and_then(|source| {
            checkpoint
                .step_results()
                .iter()
                .find(|result| result.source() == source)
                .map(|result| attempt_outcome(result.outcome()))
        });
        let source = issue.source().map_or_else(
            || String::from("-"),
            |source| {
                format!(
                    "{}:{}:{}",
                    source.step_id(),
                    source.attempt_number().get(),
                    source.run_id()
                )
            },
        );
        let rendered = format!(
            "issue step={} kind={} source={} outcome={}\n",
            issue.step_id(),
            open_issue_kind(issue.kind()),
            source,
            outcome.as_deref().unwrap_or("-")
        );
        append_mandatory_memory_item(
            &mut text,
            &mut tokens,
            reserved_tokens,
            allowance,
            profile,
            &rendered,
        )?;
    }

    for compacted in checkpoint.open_hypotheses() {
        let claim = compacted.claim();
        let rendered = render_memory_claim(claim);
        append_mandatory_memory_item(
            &mut text,
            &mut tokens,
            reserved_tokens,
            allowance,
            profile,
            &rendered,
        )?;
        claim_ids.insert(claim.id());
    }

    let mut truncated = false;
    for result in checkpoint.step_results() {
        let source = result.source();
        let summary = result.summary().map_or("-", |summary| summary.as_str());
        let rendered = format!(
            "result step={} attempt={} run={} current_status={} outcome={} summary={} evidence={}\n",
            source.step_id(),
            source.attempt_number().get(),
            source.run_id(),
            step_status(result.current_step_status()),
            attempt_outcome(result.outcome()),
            summary,
            join_ids(result.evidence_ids())
        );
        if !append_optional_memory_item(
            &mut text,
            &mut tokens,
            reserved_tokens,
            allowance,
            profile,
            &rendered,
        )? {
            truncated = true;
        }
    }

    for compacted in checkpoint.claims() {
        let claim = compacted.claim();
        if claim.kind() == VerifiedClaimKind::Hypothesis {
            continue;
        }
        let rendered = render_memory_claim(claim);
        if append_optional_memory_item(
            &mut text,
            &mut tokens,
            reserved_tokens,
            allowance,
            profile,
            &rendered,
        )? {
            claim_ids.insert(claim.id());
        } else {
            truncated = true;
        }
    }

    Ok(PackedRunMemory {
        text,
        tokens,
        claim_ids,
        truncated,
    })
}

fn append_mandatory_memory_item(
    text: &mut String,
    tokens: &mut u32,
    reserved_tokens: u32,
    allowance: u32,
    profile: &ModelProfile,
    rendered: &str,
) -> Result<(), ContextCompileFailure> {
    reject_secret_candidate(rendered)?;
    let cost = count(profile, rendered)?;
    let next = tokens
        .checked_add(cost)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    ensure_memory_fits(reserved_tokens, next, allowance)?;
    text.push_str(rendered);
    *tokens = next;
    Ok(())
}

fn append_optional_memory_item(
    text: &mut String,
    tokens: &mut u32,
    reserved_tokens: u32,
    allowance: u32,
    profile: &ModelProfile,
    rendered: &str,
) -> Result<bool, ContextCompileFailure> {
    let cost = count(profile, rendered)?;
    let next = tokens
        .checked_add(cost)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    if reserved_tokens
        .checked_add(next)
        .ok_or(ContextCompileFailure::InvalidPack)?
        > allowance
    {
        return Ok(false);
    }
    reject_secret_candidate(rendered)?;
    text.push_str(rendered);
    *tokens = next;
    Ok(true)
}

fn ensure_memory_fits(
    reserved_tokens: u32,
    memory_tokens: u32,
    allowance: u32,
) -> Result<(), ContextCompileFailure> {
    if reserved_tokens
        .checked_add(memory_tokens)
        .ok_or(ContextCompileFailure::InvalidPack)?
        > allowance
    {
        Err(ContextCompileFailure::AnchorTooLarge)
    } else {
        Ok(())
    }
}

fn render_memory_claim(claim: &TaskLensClaim) -> String {
    let evidence = claim
        .evidence()
        .iter()
        .map(|item| hex(item.id().as_bytes()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "memory_claim id={} module={} kind={} polarity={} confidence={} predicate={} evidence={}\n",
        hex(claim.id().as_bytes()),
        claim.module_id(),
        claim_kind(claim.kind()),
        claim_polarity(claim.polarity()),
        claim.confidence().basis_points(),
        claim_predicate(claim.predicate()),
        evidence
    )
}

fn attempt_outcome(outcome: &TaskStepAttemptOutcome) -> String {
    match outcome {
        TaskStepAttemptOutcome::Active => String::from("active"),
        TaskStepAttemptOutcome::Blocked { reason } => format!("blocked:{}", reason.as_str()),
        TaskStepAttemptOutcome::VerificationFailed => String::from("verification_failed"),
        TaskStepAttemptOutcome::Completed => String::from("completed"),
        TaskStepAttemptOutcome::Failed { reason } => format!("failed:{}", reason.as_str()),
        TaskStepAttemptOutcome::Cancelled { reason } => {
            format!("cancelled:{}", reason.as_str())
        }
    }
}

const fn open_issue_kind(kind: OpenRunIssueKind) -> &'static str {
    match kind {
        OpenRunIssueKind::VerificationFailed => "verification_failed",
        OpenRunIssueKind::Blocked => "blocked",
        OpenRunIssueKind::AwaitingApproval => "awaiting_approval",
        OpenRunIssueKind::Failed => "failed",
        OpenRunIssueKind::Cancelled => "cancelled",
        OpenRunIssueKind::Stale => "stale",
    }
}

fn pack_ranked_context(
    lens: &TaskLens,
    run_memory_claim_ids: &BTreeSet<ModuleCardClaimId>,
    run_memory_tokens: u32,
    tool_results: &[ContextToolResult],
    profile: &ModelProfile,
    budget: ContextBudgetPlan,
) -> Result<PackedSections, ContextCompileFailure> {
    let mut project_map = String::from("[PROJECT_MAP]\n");
    let mut code_and_evidence = String::from(CODE_AND_EVIDENCE_HEADER);
    let framing_reserve = count(profile, CONTEXT_PACK_HEADER)?
        .checked_add(
            count(profile, &render_pack_state(lens, false))?
                .max(count(profile, &render_pack_state(lens, true))?),
        )
        .ok_or(ContextCompileFailure::InvalidPack)?;
    let mut project_tokens = count(profile, &project_map)?
        .checked_add(framing_reserve)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    let mut code_tokens = count(profile, &code_and_evidence)?
        .checked_add(run_memory_tokens)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    let mut target_keys = BTreeSet::new();
    let mut spans = Vec::new();
    let mut truncated = false;

    // The repository card is the untruncatable L0 project anchor. Retrieval rank can place an
    // optional L1 module before it, so reserve L0 first while preserving rank within both groups.
    let ordered_entries = lens
        .entries()
        .iter()
        .filter(|entry| matches!(entry.target(), TaskLensTarget::Repository(_)))
        .chain(
            lens.entries()
                .iter()
                .filter(|entry| !matches!(entry.target(), TaskLensTarget::Repository(_))),
        );
    for entry in ordered_entries {
        let key = target_key(entry.target());
        if !target_keys.insert(key) {
            continue;
        }
        if let TaskLensTarget::SourceSpan { evidence, .. } = entry.target() {
            let candidate = SpanKey::from_evidence(evidence);
            if spans
                .iter()
                .any(|existing: &SpanKey| existing.overlaps(&candidate))
            {
                continue;
            }
            spans.push(candidate);
        }
        let rendered = render_lens_entry(entry);
        reject_secret_candidate(&rendered)?;
        let item_tokens = count(profile, &rendered)?;
        let (section, used, allowance) = match entry.target() {
            TaskLensTarget::Repository(_) | TaskLensTarget::Module(_) => (
                &mut project_map,
                &mut project_tokens,
                budget.allowance(ContextSection::ProjectMap),
            ),
            TaskLensTarget::File(_)
            | TaskLensTarget::Symbol(_)
            | TaskLensTarget::SourceSpan { .. } => (
                &mut code_and_evidence,
                &mut code_tokens,
                budget.allowance(ContextSection::CodeAndEvidence),
            ),
        };
        let next = used
            .checked_add(item_tokens)
            .ok_or(ContextCompileFailure::InvalidPack)?;
        if next > allowance {
            if matches!(entry.target(), TaskLensTarget::Repository(_)) {
                return Err(ContextCompileFailure::InvalidPack);
            }
            truncated = true;
            continue;
        }
        section.push_str(&rendered);
        *used = next;
    }

    for claim in lens.claims() {
        if run_memory_claim_ids.contains(&claim.id()) {
            continue;
        }
        if claim.status() != VerifiedClaimStatus::Active {
            return Err(ContextCompileFailure::StaleOrMismatchedInput);
        }
        let rendered = render_claim(claim);
        reject_secret_candidate(&rendered)?;
        let item_tokens = count(profile, &rendered)?;
        let next = code_tokens
            .checked_add(item_tokens)
            .ok_or(ContextCompileFailure::InvalidPack)?;
        if next > budget.allowance(ContextSection::CodeAndEvidence) {
            truncated = true;
            continue;
        }
        code_and_evidence.push_str(&rendered);
        code_tokens = next;
    }

    let (tool_results, tool_tokens, tool_truncated) =
        pack_tool_results(tool_results, lens.snapshot_id(), profile, budget)?;
    truncated |= tool_truncated;
    let pack_state = render_pack_state(lens, truncated);
    let actual_framing = count(profile, CONTEXT_PACK_HEADER)?
        .checked_add(count(profile, &pack_state)?)
        .ok_or(ContextCompileFailure::InvalidPack)?;
    project_tokens = project_tokens
        .checked_sub(framing_reserve)
        .and_then(|tokens| tokens.checked_add(actual_framing))
        .ok_or(ContextCompileFailure::InvalidPack)?;
    Ok(PackedSections {
        project_map,
        code_and_evidence,
        tool_results,
        pack_state,
        project_tokens,
        code_tokens,
        tool_tokens,
        truncated,
    })
}

fn pack_tool_results(
    results: &[ContextToolResult],
    snapshot_id: SnapshotId,
    profile: &ModelProfile,
    budget: ContextBudgetPlan,
) -> Result<(String, u32, bool), ContextCompileFailure> {
    let header = String::from("[TOOL_RESULTS]\n");
    let header_tokens = count(profile, &header)?;
    let allowance = budget.allowance(ContextSection::ToolResults);
    if header_tokens > allowance {
        return Err(ContextCompileFailure::InvalidPack);
    }
    let mut selected = Vec::new();
    let mut tokens = header_tokens;
    let mut truncated = false;
    for result in results.iter().rev() {
        if result.snapshot_before() != snapshot_id || result.snapshot_after() != snapshot_id {
            truncated = true;
            continue;
        }
        let rendered = render_tool_result(result);
        reject_secret_candidate(&rendered)?;
        let cost = count(profile, &rendered)?;
        let next = tokens
            .checked_add(cost)
            .ok_or(ContextCompileFailure::InvalidPack)?;
        if next > allowance {
            truncated = true;
            continue;
        }
        selected.push(rendered);
        tokens = next;
    }
    selected.reverse();
    let mut text = header;
    for item in selected {
        text.push_str(&item);
    }
    Ok((text, tokens, truncated))
}

fn render_lens_entry(entry: &TaskLensEntry) -> String {
    let reason = entry_reason(entry.reason());
    match entry.target() {
        TaskLensTarget::Repository(card) => render_repository(card, reason),
        TaskLensTarget::Module(module) => render_module(module, reason),
        TaskLensTarget::File(revision) => format!(
            "L3 file path={} hash={} reason={reason}\n",
            path_text(revision.path()),
            hex(revision.content_hash().as_bytes())
        ),
        TaskLensTarget::Symbol(symbol) => render_symbol(symbol, reason),
        TaskLensTarget::SourceSpan {
            symbol_id,
            evidence,
        } => format!(
            "L3 span symbol={} path={} bytes={}..{} hash={} reason={reason}\n",
            symbol_id,
            path_text(evidence.revision().path()),
            evidence.range().start_byte(),
            evidence.range().end_byte(),
            hex(evidence.revision().content_hash().as_bytes())
        ),
    }
}

fn render_repository(card: &RepositoryCard, reason: String) -> String {
    let languages = card
        .languages()
        .iter()
        .map(|language| language.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "L0 repository snapshot={} module_policy={} files={} symbols={} package_count={} languages={} entrypoint_count={} reason={reason}\n",
        card.snapshot_id(),
        card.policy_version().get(),
        card.file_count(),
        card.symbol_count(),
        card.packages().len(),
        languages,
        card.entrypoints().symbols().len()
    )
}

fn render_module(module: &RepositoryModule, reason: String) -> String {
    let root = match module.root() {
        Some(ModuleRoot::Repository) => String::from("."),
        Some(ModuleRoot::Directory(path)) => path_text(path),
        None => String::from("-"),
    };
    let manifests = module
        .manifests()
        .iter()
        .map(|revision| path_text(revision.path()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "L1 module id={} kind={} root={} manifests={} central={} entrypoints={} tests={} reason={reason}\n",
        module.id(),
        module_kind(module.kind()),
        root,
        manifests,
        join_ids(module.central_symbols().symbols()),
        join_ids(module.entrypoints().symbols()),
        join_ids(module.tests().symbols())
    )
}

fn render_symbol(symbol: &GraphSymbol, reason: String) -> String {
    let parsed = symbol.parsed();
    let signature = parsed.signature().map_or("-", |value| value.as_str());
    let roles = match (
        parsed.roles().contains(SymbolRole::Entrypoint),
        parsed.roles().contains(SymbolRole::Test),
    ) {
        (true, true) => "entrypoint,test",
        (true, false) => "entrypoint",
        (false, true) => "test",
        (false, false) => "-",
    };
    format!(
        "L2 symbol id={} path={} kind={:?} visibility={:?} roles={} name={} signature={} reason={reason}\n",
        symbol.id(),
        path_text(symbol.revision().path()),
        parsed.kind(),
        parsed.visibility(),
        roles,
        parsed.name().as_str(),
        signature
    )
}

fn render_claim(claim: &TaskLensClaim) -> String {
    let evidence = claim
        .evidence()
        .iter()
        .map(|item| hex(item.id().as_bytes()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "claim id={} module={} kind={} polarity={} confidence={} predicate={} evidence={}\n",
        hex(claim.id().as_bytes()),
        claim.module_id(),
        claim_kind(claim.kind()),
        claim_polarity(claim.polarity()),
        claim.confidence().basis_points(),
        claim_predicate(claim.predicate()),
        evidence
    )
}

fn render_tool_result(result: &ContextToolResult) -> String {
    format!(
        "tool sequence={} id={} status={} digest={} truncated={} preview={}\n",
        result.sequence().get(),
        result.tool_run_id(),
        tool_status(result.status()),
        result.digest(),
        result.truncated(),
        result.preview().as_str()
    )
}

fn render_pack_state(lens: &TaskLens, truncated: bool) -> String {
    format!(
        "[PACK_STATE]\nindex_run={} snapshot={} lens_policy={} fusion_policy={} lens_digest={} excluded_stale_claims={} truncated={}\n",
        lens.index_run_id(),
        lens.snapshot_id(),
        lens.policy_version().get(),
        lens.fusion_policy_version().get(),
        hex(&lens.digest().as_bytes()),
        lens.excluded_stale_claims(),
        truncated || lens.truncated()
    )
}

fn target_key(target: &TaskLensTarget) -> String {
    match target {
        TaskLensTarget::Repository(_) => String::from("repository"),
        TaskLensTarget::Module(module) => format!("module:{}", module.id()),
        TaskLensTarget::File(revision) => format!("file:{}", path_text(revision.path())),
        TaskLensTarget::Symbol(symbol) => format!("symbol:{}", symbol.id()),
        TaskLensTarget::SourceSpan {
            symbol_id,
            evidence,
        } => format!(
            "span:{}:{}:{}:{}",
            symbol_id,
            path_text(evidence.revision().path()),
            evidence.range().start_byte(),
            evidence.range().end_byte()
        ),
    }
}

struct SpanKey {
    path: Vec<u8>,
    start: u32,
    end: u32,
}

impl SpanKey {
    fn from_evidence(evidence: &EvidenceRef) -> Self {
        Self {
            path: evidence.revision().path().as_bytes().to_vec(),
            start: evidence.range().start_byte(),
            end: evidence.range().end_byte(),
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.path == other.path && self.start < other.end && other.start < self.end
    }
}

fn count(profile: &ModelProfile, value: &str) -> Result<u32, ContextCompileFailure> {
    profile
        .settings()
        .token_counting()
        .count_text(value)
        .map(|count| count.get())
        .map_err(|_| ContextCompileFailure::InvalidPack)
}

fn push_line(target: &mut String, arguments: fmt::Arguments<'_>) {
    target.push_str(&arguments.to_string());
    target.push('\n');
}

fn path_text(path: &RepositoryPath) -> String {
    let mut encoded = String::with_capacity(path.as_bytes().len());
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
    }
    encoded
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        value.push(hex_digit(byte >> 4));
        value.push(hex_digit(byte & 0x0f));
    }
    value
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + value - 10) as char,
    }
}

fn join_ids<T: fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn entry_reason(reason: &TaskLensEntryReason) -> String {
    match reason {
        TaskLensEntryReason::RepositoryAnchor => String::from("anchor"),
        TaskLensEntryReason::Retrieval { rank, .. } => format!("retrieval:{rank}"),
        TaskLensEntryReason::Claim(id) => format!("claim:{}", hex(id.as_bytes())),
    }
}

const fn module_kind(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::ManifestBoundary => "manifest",
        ModuleKind::PathBoundary => "path",
        ModuleKind::GraphCommunity => "graph-community",
    }
}

const fn claim_kind(kind: VerifiedClaimKind) -> &'static str {
    match kind {
        VerifiedClaimKind::Fact => "fact",
        VerifiedClaimKind::Observation => "observation",
        VerifiedClaimKind::Hypothesis => "hypothesis",
    }
}

const fn claim_polarity(polarity: ModuleClaimPolarity) -> &'static str {
    match polarity {
        ModuleClaimPolarity::Affirms => "affirms",
        ModuleClaimPolarity::Denies => "denies",
    }
}

fn claim_predicate(predicate: &ModuleClaimPredicate) -> String {
    match predicate {
        ModuleClaimPredicate::Path(path) => format!("path:{}", path_text(path)),
        ModuleClaimPredicate::Symbol(id) => format!("symbol:{id}"),
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => format!(
            "relation:{:?}:{}:{}",
            kind,
            graph_endpoint(source),
            graph_endpoint(target)
        ),
        ModuleClaimPredicate::Observed(statement) => {
            format!("observed:{}", statement.as_str())
        }
        ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            format!("architectural_intent:{}", statement.as_str())
        }
    }
}

fn graph_endpoint(endpoint: &GraphEndpoint) -> String {
    match endpoint {
        GraphEndpoint::File(path) => format!("file:{}", path_text(path)),
        GraphEndpoint::Symbol(id) => format!("symbol:{id}"),
    }
}

const fn tool_status(status: ContextToolResultStatus) -> &'static str {
    match status {
        ContextToolResultStatus::Succeeded => "succeeded",
        ContextToolResultStatus::Failed => "failed",
        ContextToolResultStatus::Cancelled => "cancelled",
        ContextToolResultStatus::Denied => "denied",
    }
}

const fn step_status(status: TaskStepStatus) -> &'static str {
    match status {
        TaskStepStatus::Pending => "pending",
        TaskStepStatus::Ready => "ready",
        TaskStepStatus::InProgress => "in_progress",
        TaskStepStatus::Blocked => "blocked",
        TaskStepStatus::AwaitingApproval => "awaiting_approval",
        TaskStepStatus::Verifying => "verifying",
        TaskStepStatus::Completed => "completed",
        TaskStepStatus::Failed => "failed",
        TaskStepStatus::Cancelled => "cancelled",
        TaskStepStatus::Stale => "stale",
    }
}

const fn verification_method(method: a3_domain::VerificationMethod) -> &'static str {
    match method {
        a3_domain::VerificationMethod::Command => "command",
        a3_domain::VerificationMethod::Test => "test",
        a3_domain::VerificationMethod::DiffInvariant => "diff_invariant",
        a3_domain::VerificationMethod::Diagnostic => "diagnostic",
        a3_domain::VerificationMethod::UserConfirm => "user_confirm",
    }
}

const fn acceptance_requirement(
    requirement: a3_domain::AcceptanceCriterionRequirement,
) -> &'static str {
    match requirement {
        a3_domain::AcceptanceCriterionRequirement::Must => "must",
        a3_domain::AcceptanceCriterionRequirement::Should => "should",
    }
}

fn render_verification_target(text: &mut String, spec: &a3_domain::VerificationSpec) {
    match spec.target() {
        a3_domain::VerificationTarget::Legacy(method) => push_line(
            text,
            format_args!(
                "verification_target=legacy method={}",
                verification_method(*method)
            ),
        ),
        a3_domain::VerificationTarget::Command { command_id, scope } => push_line(
            text,
            format_args!(
                "verification_target=command command={} scope={}",
                command_id,
                verification_scope(*scope)
            ),
        ),
        a3_domain::VerificationTarget::Test {
            command_id,
            selector,
            minimum_cases,
            scope,
        } => match selector {
            a3_domain::TestCaseSelector::All => push_line(
                text,
                format_args!(
                    "verification_target=test command={} selector=all minimum_cases={} scope={}",
                    command_id,
                    minimum_cases.get(),
                    verification_scope(*scope)
                ),
            ),
            a3_domain::TestCaseSelector::Exact(name) => push_line(
                text,
                format_args!(
                    "verification_target=test command={} selector=exact:{} minimum_cases={} scope={}",
                    command_id,
                    name.as_str(),
                    minimum_cases.get(),
                    verification_scope(*scope)
                ),
            ),
        },
        a3_domain::VerificationTarget::DiffInvariant(invariant) => {
            push_line(
                text,
                format_args!(
                    "verification_target=diff_invariant mode={}",
                    diff_invariant_mode(invariant.mode())
                ),
            );
            for path in invariant.paths() {
                push_line(text, format_args!("verification_path={}", path_text(path)));
            }
        }
        a3_domain::VerificationTarget::Diagnostic {
            command_id,
            policy,
            scope,
        } => push_line(
            text,
            format_args!(
                "verification_target=diagnostic command={} policy={} scope={}",
                command_id,
                diagnostic_policy(*policy),
                verification_scope(*scope)
            ),
        ),
        a3_domain::VerificationTarget::UserConfirm { scope_id } => push_line(
            text,
            format_args!("verification_target=user_confirm scope={scope_id}"),
        ),
    }
}

const fn verification_scope(scope: a3_domain::VerificationScope) -> &'static str {
    match scope {
        a3_domain::VerificationScope::Targeted => "targeted",
        a3_domain::VerificationScope::Package => "package",
        a3_domain::VerificationScope::Workspace => "workspace",
    }
}

const fn diff_invariant_mode(mode: a3_domain::DiffInvariantMode) -> &'static str {
    match mode {
        a3_domain::DiffInvariantMode::NoChanges => "no_changes",
        a3_domain::DiffInvariantMode::OnlyPaths => "only_paths",
        a3_domain::DiffInvariantMode::ExactPaths => "exact_paths",
    }
}

const fn diagnostic_policy(policy: a3_domain::DiagnosticPolicy) -> &'static str {
    match policy {
        a3_domain::DiagnosticPolicy::NoErrors => "no_errors",
        a3_domain::DiagnosticPolicy::NoWarnings => "no_warnings",
    }
}

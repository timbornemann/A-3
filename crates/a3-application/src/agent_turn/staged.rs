//! Owned, bounded action generation in Application, not in a provider adapter.
use super::{after_change, source_guidance, staged_contract as contract, *};
use crate::DecodedAgentAction;
use std::time::Instant;

pub(super) type Generated = (
    DecodedAgentAction,
    ModelTokenCount,
    ModelTokenCount,
    AgentTurnRepairUsage,
    u64,
);

enum Stage {
    AfterChange(a3_domain::AgentRunAction),
    SourceWork(a3_domain::AgentRunAction),
    Choose(contract::ChoiceScope),
    Arguments(contract::Arguments),
}

impl Stage {
    fn request_contract(&self) -> (serde_json::Value, &str) {
        match self {
            Self::AfterChange(_) => (after_change::schema(), after_change::PROMPT),
            Self::SourceWork(_) => (source_guidance::schema(), source_guidance::PROMPT),
            Self::Choose(contract::ChoiceScope::All) => {
                (contract::choice_schema(), contract::CHOICE_PROMPT)
            }
            Self::Choose(scope) => (contract::choice_schema_for(*scope), scope.prompt()),
            Self::Arguments(args) => (args.schema.clone(), args.prompt.as_str()),
        }
    }
}

#[derive(Default)]
struct Usage {
    prompt: u32,
    output: u32,
    bytes: u64,
    repaired: bool,
}

impl Usage {
    fn charge(&self) -> AgentTurnCharge {
        AgentTurnCharge::new(
            ModelTokenCount::new(self.prompt),
            ModelTokenCount::new(self.output),
            None,
            self.repair(),
        )
    }
    fn repair(&self) -> AgentTurnRepairUsage {
        if self.repaired {
            AgentTurnRepairUsage::One
        } else {
            AgentTurnRepairUsage::None
        }
    }
    fn reject(
        &self,
        snapshot_id: SnapshotId,
        reason: AgentTurnRejectionReason,
    ) -> RejectedAgentTurn {
        RejectedAgentTurn {
            charge: self.charge(),
            reason,
            snapshot_id,
            observed_model_output_bytes: self.bytes,
        }
    }
    fn add(&mut self, completion: &CompletedModelRequest) -> Result<(), ExecuteAgentTurnFailure> {
        self.prompt = self
            .prompt
            .checked_add(completion.prompt_tokens.get())
            .ok_or(ExecuteAgentTurnFailure::TokenOverflow)?;
        self.output = self
            .output
            .checked_add(completion.output_tokens.get())
            .ok_or(ExecuteAgentTurnFailure::TokenOverflow)?;
        self.bytes = self
            .bytes
            .checked_add(usize_to_u64(completion.raw.len())?)
            .ok_or(ExecuteAgentTurnFailure::OutputTooLarge)?;
        Ok(())
    }
}

pub(super) async fn generate<C>(
    executor: ExecuteAgentTurn<'_>,
    run: &AgentRun,
    input: &AgentContextCompileInput,
    compiled: &crate::CompiledAgentContext,
    observed_at: AgentRunTimestamp,
    control: &C,
) -> Result<Result<(Generated, ContextDigest), RejectedAgentTurn>, ExecuteAgentTurnFailure>
where
    C: AgentControllerControl + ContextCompileControl + ModelOperationControl,
{
    let base = compiled.request();
    let digest = compiled.digest();
    let sources = compiled.original_sources();
    let started = Instant::now();
    let run_elapsed = observed_at
        .unix_millis()
        .saturating_sub(run.created_at().unix_millis());
    let deadline = executor.model_timeout.duration().min(Duration::from_millis(
        run.budget()
            .duration_limit()
            .millis()
            .saturating_sub(run_elapsed),
    ));
    let mut exchange_digest = blake3::Hasher::new();
    exchange_digest.update(b"a3-staged-agent-exchange-v1\0");
    exchange_digest.update(&digest.as_bytes());
    let snapshot = run.current_snapshot_id();
    let mut usage = Usage::default();
    let source_guided = executor.generation == AgentActionGeneration::SourceGuided;
    let verification = (source_guided
        || executor.generation == AgentActionGeneration::ReviewThenSelect)
        .then(|| after_change::planned_verification(input, run))
        .flatten();
    let mut stage = verification
        .map(Stage::AfterChange)
        .or_else(|| {
            source_guided
                .then(|| source_guidance::planned_verification(input, run, sources))
                .flatten()
                .map(Stage::SourceWork)
        })
        .unwrap_or(Stage::Choose(contract::ChoiceScope::All));
    let max_calls = if matches!(stage, Stage::AfterChange(_) | Stage::SourceWork(_)) {
        4
    } else {
        3
    };
    let mut repair: Option<String> = None;
    let step = input
        .task_ledger()
        .step(input.current_step_id())
        .ok_or(ExecuteAgentTurnFailure::InputMismatch)?;
    let anchors = crate::agent_action_codec::AgentActionTurnAnchors::new(
        run.id(),
        input.project().worktree().id(),
        snapshot,
        input.current_step_id(),
        step.definition().verification_spec().id(),
    )
    .with_verification_command(
        crate::RequestAgentFinish
            .verification_command(step)
            .map(|c| c.command_id()),
    );
    let decoder = DecodeAgentActionTurn::current().with_turn_anchors(anchors);
    let base_schema = base
        .structured_output()
        .ok_or(ExecuteAgentTurnFailure::ContextMismatch)?
        .value();
    let Some(bound_schema) = contract::bind_patch_anchors(
        base_schema,
        [
            ("run_id", run.id().to_string()),
            ("worktree_id", input.project().worktree().id().to_string()),
            ("snapshot_id", snapshot.to_string()),
            ("step_id", input.current_step_id().to_string()),
            (
                "verification_spec_id",
                step.definition().verification_spec().id().to_string(),
            ),
        ],
    ) else {
        return Ok(Err(usage.reject(
            snapshot,
            AgentTurnRejectionReason::Staged(StagedActionFailure::Contract),
        )));
    };
    // No retry of a provider failure, no recursive repair, no executable choice-stage value.
    for call in 0..max_calls {
        if AgentControllerControl::is_cancelled(control) {
            return Ok(Err(usage.reject(
                snapshot,
                AgentTurnRejectionReason::CancelledBeforeAction,
            )));
        }
        if call > 0 {
            let fresh = executor.compiler.compile(input, control).await;
            if AgentControllerControl::is_cancelled(control) {
                return Ok(Err(usage.reject(
                    snapshot,
                    AgentTurnRejectionReason::CancelledBeforeAction,
                )));
            }
            if !fresh.is_ok_and(|fresh| {
                fresh.digest() == digest
                    && fresh.snapshot_id() == snapshot
                    && fresh.original_sources() == sources
            }) {
                return Ok(Err(usage.reject(
                    snapshot,
                    AgentTurnRejectionReason::Staged(StagedActionFailure::ContextChanged),
                )));
            }
        }
        let (schema, prompt) = stage.request_contract();
        let Some(request) = contract::request(base, schema, prompt, repair.as_deref()) else {
            return Ok(Err(usage.reject(
                snapshot,
                AgentTurnRejectionReason::Staged(StagedActionFailure::Contract),
            )));
        };
        let Some(remaining) = deadline.checked_sub(started.elapsed()) else {
            return Ok(Err(usage.reject(
                snapshot,
                AgentTurnRejectionReason::Staged(StagedActionFailure::Deadline),
            )));
        };
        let Ok(timeout) =
            ModelRequestTimeout::from_millis(u64::try_from(remaining.as_millis()).unwrap_or(0))
        else {
            return Ok(Err(usage.reject(
                snapshot,
                AgentTurnRejectionReason::Staged(StagedActionFailure::Deadline),
            )));
        };
        if !fits(&request, run, &usage, repair.is_some())? {
            return Ok(Err(usage.reject(
                snapshot,
                AgentTurnRejectionReason::Staged(StagedActionFailure::BudgetExceeded),
            )));
        }
        if repair.take().is_some() {
            usage.repaired = true;
        }
        hash_request(&mut exchange_digest, &request);
        let mut completion =
            complete_request(executor.provider, &request, timeout, control).await?;
        if !completion.prompt_usage_reported {
            completion.prompt_tokens =
                add_token_counts(completion.prompt_tokens, schema_tokens(&request)?)?;
        }
        usage.add(&completion)?;
        if let Some(reason) = completion.rejection_reason() {
            return Ok(Err(usage.reject(snapshot, reason)));
        }
        let (failure, instruction) = if let Stage::AfterChange(verification)
        | Stage::SourceWork(verification) = &stage
        {
            let source_work = matches!(stage, Stage::SourceWork(_));
            let decision = if source_work {
                source_guidance::decode(&completion.raw)
            } else {
                after_change::decode(&completion.raw)
            };
            match decision {
                Some(after_change::NextWork::Verify) => {
                    let raw = after_change::verification_wire(verification);
                    let AgentActionPrimaryOutcome::Accepted(action) = decoder.decode_primary_in_snapshot(&raw, executor.patch_snapshot) else {
                        return Ok(Err(usage.reject(snapshot, AgentTurnRejectionReason::Staged(StagedActionFailure::Contract))));
                    };
                    return Ok(Ok(finish(action, &usage, exchange_digest)));
                }
                Some(after_change::NextWork::ContinueChange) => {
                    stage = Stage::Choose(contract::ChoiceScope::Changes);
                    continue;
                }
                Some(after_change::NextWork::NeedEvidence) => {
                    stage = Stage::Choose(contract::ChoiceScope::Evidence);
                    continue;
                }
                None if source_work => (StagedActionFailure::InvalidSourceWork, "Invalid SourceWork V1 decision. This is the only repair shared by all stages. Return exactly version=1 and next=change, verify or need_evidence. No action, code, IDs, success status or extra fields.".to_owned()),
                None => (StagedActionFailure::InvalidAfterChange, "Invalid AfterChange V1 decision. This is the only repair shared by all stages. Return exactly version=1 and next=verify, continue_change or need_evidence. No action, code, IDs, success status or extra fields.".to_owned()),
            }
        } else if let Stage::Arguments(args) = &stage {
            let raw = args.assemble(&completion.raw);
            match raw.as_deref().map(|raw|decoder.decode_primary_in_snapshot(raw, executor.patch_snapshot)) {
                Some(AgentActionPrimaryOutcome::Accepted(action)) if source_guided
                    && source_guidance::already_supplied(action.action(), snapshot, sources) => (
                    StagedActionFailure::SourceAlreadySupplied,
                    "This complete file range is already delivered in the current ORIGINAL_SOURCE blocks. This is the only shared repair. For the same locked inspect_file choice, request only genuinely missing lines or another needed file, not the supplied range. Do not invent a path or change action kind.".to_owned(),
                ),
                Some(AgentActionPrimaryOutcome::Accepted(action)) => {
                    return Ok(Ok(finish(action, &usage, exchange_digest)));
                }
                Some(AgentActionPrimaryOutcome::RepairRequired(rejected)) => (
                    StagedActionFailure::InvalidAction(rejected.rejection()),
                    format!("Arguments were rejected with code {}. This is the only repair. Return corrected ActionArguments V1 for the same locked choice and schema, not an AgentAction. Recheck current paths, hashes and field values. Do not change the selected operation.", rejected.repair_code()),
                ),
                None => (StagedActionFailure::InvalidArguments, "Invalid argument envelope or unexpected fields. This is the only repair. Return only version=1 and parameters for the same locked choice and schema; no action kind, fixed IDs or additional fields.".to_owned()),
            }
        } else {
            let choice = match &stage {
                Stage::Choose(contract::ChoiceScope::All) => {
                    contract::decode_choice(&completion.raw)
                }
                Stage::Choose(scope) => contract::decode_choice_for(&completion.raw, *scope),
                _ => None,
            };
            if let Some(choice) = choice {
                let Some(args) = contract::Arguments::new(&bound_schema, choice) else {
                    return Ok(Err(usage.reject(
                        snapshot,
                        AgentTurnRejectionReason::Staged(StagedActionFailure::Contract),
                    )));
                };
                stage = Stage::Arguments(args);
                continue;
            }
            (StagedActionFailure::InvalidChoice, "Invalid choice. This is the only repair shared by both stages. Return exactly version=1 and one choice from the enum; no arguments, code or additional fields.".to_owned())
        };
        if usage.repaired {
            return Ok(Err(
                usage.reject(snapshot, AgentTurnRejectionReason::Staged(failure))
            ));
        }
        repair = Some(instruction);
    }
    Ok(Err(usage.reject(
        snapshot,
        AgentTurnRejectionReason::Staged(StagedActionFailure::InvalidArguments),
    )))
}

fn finish(
    action: DecodedAgentAction,
    usage: &Usage,
    digest: blake3::Hasher,
) -> (Generated, ContextDigest) {
    (
        (
            action,
            ModelTokenCount::new(usage.prompt),
            ModelTokenCount::new(usage.output),
            usage.repair(),
            usage.bytes,
        ),
        ContextDigest::from_bytes(*digest.finalize().as_bytes()),
    )
}

fn hash_request(digest: &mut blake3::Hasher, request: &ModelProviderRequest) {
    for message in request.messages() {
        let role = format!("{:?}", message.role());
        for part in [role.as_str(), message.content()] {
            digest.update(&(part.len() as u64).to_le_bytes());
            digest.update(part.as_bytes());
        }
    }
    if let Some(schema) = request.structured_output() {
        let value = schema.value().to_string();
        digest.update(&(value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
}

fn fits(
    request: &ModelProviderRequest,
    run: &AgentRun,
    usage: &Usage,
    repair: bool,
) -> Result<bool, ExecuteAgentTurnFailure> {
    let messages = u64::from(count_request_tokens(request)?.get());
    let reserved = messages + u64::from(schema_tokens(request)?.get());
    let output = u64::from(request.profile().settings().output_limit().get());
    // Include the format-field schema, optional repeat, output reserve and protocol margin.
    Ok(
        reserved + output + 64 <= u64::from(request.profile().settings().context_limit().get())
            && run.usage().prompt_tokens() + u64::from(usage.prompt) + reserved
                <= run.budget().prompt_token_limit().get()
            && run.usage().output_tokens() + u64::from(usage.output) + output
                <= run.budget().output_token_limit().get()
            && (!repair || run.usage().repair_count() < run.budget().repair_limit().get()),
    )
}

fn schema_tokens(
    request: &ModelProviderRequest,
) -> Result<ModelTokenCount, ExecuteAgentTurnFailure> {
    let schema = request
        .structured_output()
        .map(|s| s.value().to_string())
        .unwrap_or_default();
    Ok(request
        .profile()
        .settings()
        .token_counting()
        .count_text(&schema)?)
}

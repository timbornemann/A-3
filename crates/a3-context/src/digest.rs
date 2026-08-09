//! Domain-separated identity of one fully normalized Context Pack.

use a3_application::{AgentContextCompileInput, ModelMessage, ModelMessageRole};
use a3_domain::{
    ContextBudgetPlan, ContextBudgetUsage, ContextCompilerPolicyVersion, ContextDigest,
    ContextSection, ModelProfile, TaskLens,
};

const CONTEXT_DIGEST_DOMAIN: &[u8] = b"a3.context-pack.v3";

pub(super) fn context_digest(
    profile: &ModelProfile,
    input: &AgentContextCompileInput,
    lens: &TaskLens,
    budget_plan: ContextBudgetPlan,
    budget_usage: ContextBudgetUsage,
    messages: &[ModelMessage],
    structured_schema: &str,
) -> ContextDigest {
    let mut hasher = blake3::Hasher::new();
    hash_bytes(&mut hasher, CONTEXT_DIGEST_DOMAIN);
    hash_u32(&mut hasher, ContextCompilerPolicyVersion::CURRENT.get());
    hash_bytes(&mut hasher, profile.id().as_bytes());
    hash_u32(&mut hasher, u32::from(profile.version().get()));
    hash_bytes(&mut hasher, input.goal_contract().task_id().as_bytes());
    hash_u32(&mut hasher, input.goal_contract().revision().get());
    hash_u32(&mut hasher, input.task_ledger().revision().get());
    hash_bytes(&mut hasher, input.current_step_id().as_bytes());
    hash_bytes(&mut hasher, lens.index_run_id().as_bytes());
    hash_bytes(&mut hasher, lens.snapshot_id().as_bytes());
    hash_bytes(&mut hasher, &lens.digest().as_bytes());
    match input.run_memory() {
        Some(checkpoint) => {
            hasher.update(&[1]);
            hash_bytes(&mut hasher, &checkpoint.digest().as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    hash_u32(&mut hasher, budget_plan.context_limit());
    for section in [
        ContextSection::SystemAndTools,
        ContextSection::GoalAndLedger,
        ContextSection::ProjectMap,
        ContextSection::CodeAndEvidence,
        ContextSection::ToolResults,
    ] {
        hash_u32(&mut hasher, budget_plan.allowance(section));
        hash_u32(&mut hasher, budget_usage.section(section));
    }
    hash_u32(&mut hasher, budget_plan.safety_reserve());
    hash_u32(&mut hasher, budget_plan.output_reserve());
    hash_u32(&mut hasher, budget_usage.prompt_total());
    for message in messages {
        hasher.update(&[match message.role() {
            ModelMessageRole::System => 0,
            ModelMessageRole::User => 1,
            ModelMessageRole::Assistant => 2,
        }]);
        hash_bytes(&mut hasher, message.content().as_bytes());
    }
    hash_bytes(&mut hasher, structured_schema.as_bytes());
    ContextDigest::from_bytes(*hasher.finalize().as_bytes())
}

fn hash_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn hash_u32(hasher: &mut blake3::Hasher, value: u32) {
    hasher.update(&value.to_le_bytes());
}

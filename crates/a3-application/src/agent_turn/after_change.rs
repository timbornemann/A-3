//! A work decision following a real run receipt, never a completion assessment.
use crate::{AgentContextCompileInput, ExecutedAgentMutation, RequestAgentFinish};
use a3_domain::{AgentRun, AgentRunAction};
use serde_json::{Value, json};

pub(super) const PROMPT: &str = "AfterChange V1: the Core has evidence that the latest recorded run mutation was an applied patch. This is not proof that the current step is complete; the patch may belong to earlier work. Compare the current original code with the CURRENT step and goal. Choose only the next work: verify requests its already planned test/check (which may fail); continue_change means a concrete required code change remains; need_evidence means specific source evidence is still missing. If the intended change is present, request verify rather than proposing it again. Return only version=1 and next; no code, action, IDs, claims or success status.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NextWork {
    Verify,
    ContinueChange,
    NeedEvidence,
}

pub(super) fn planned_verification(
    input: &AgentContextCompileInput,
    run: &AgentRun,
) -> Option<AgentRunAction> {
    let step = input.task_ledger().step(input.current_step_id())?;
    let checkpoint = input.execution_checkpoint()?;
    if !checkpoint.matches(input)
        || step
            .attempts()
            .last()
            .is_none_or(|attempt| attempt.run_id() != run.id())
        || checkpoint.snapshot_id() != run.current_snapshot_id()
        || checkpoint.mutation() != ExecutedAgentMutation::PatchApplied
        || input.replan_localization().is_some()
        || input.replan_research().is_some()
    {
        return None;
    }
    RequestAgentFinish.verification_command(step)
}

pub(super) fn schema() -> Value {
    json!({"title":"A^3 AfterChange V1","type":"object","additionalProperties":false,
    "required":["version","next"],"properties":{
        "version":{"const":1},
        "next":{"type":"string","enum":["verify","continue_change","need_evidence"]}
    }})
}

pub(super) fn decode(raw: &str) -> Option<NextWork> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let fields = value.as_object()?;
    if fields.len() != 2 || fields.get("version")? != &json!(1) {
        return None;
    }
    match fields.get("next")?.as_str()? {
        "verify" => Some(NextWork::Verify),
        "continue_change" => Some(NextWork::ContinueChange),
        "need_evidence" => Some(NextWork::NeedEvidence),
        _ => None,
    }
}

/// A validated work request selects known IDs; the independent action decoder still admits it.
pub(super) fn verification_wire(command: &AgentRunAction) -> String {
    json!({"schema_version":5,"action":{"kind":"run",
        "step_id":command.step_id().to_string(),"command_id":command.command_id().to_string()
    }})
    .to_string()
}

//! Source availability selects a smaller work decision, never task completion.
use crate::{AgentContextCompileInput, ContextOriginalSource, RequestAgentFinish};
use a3_domain::{AgentAction, AgentInspectTarget, AgentRun, AgentRunAction, SnapshotId};
use serde_json::{Value, json};

pub(super) const PROMPT: &str = "SourceWork V1: the Core has already delivered current original source pages in ORIGINAL_SOURCE blocks below, not just file names or search summaries. Read that code against the CURRENT step and goal now. Choose the next work only: change to implement a concrete missing requirement; verify to request the already planned check (it may fail); need_evidence only for specific source lines or files absent from those blocks. Reading a supplied file again does not implement the task. Partial pages do not prove all required evidence is present. Return exactly version=1 and next, no arguments, code, IDs, claims or success status.";

pub(super) fn planned_verification(
    input: &AgentContextCompileInput,
    run: &AgentRun,
    sources: &[ContextOriginalSource],
) -> Option<AgentRunAction> {
    let step = input.task_ledger().step(input.current_step_id())?;
    if !sources.iter().any(|source| {
        source.snapshot_id() == run.current_snapshot_id() && !source.range().is_empty()
    }) || step
        .attempts()
        .last()
        .is_none_or(|attempt| attempt.run_id() != run.id())
        || input.replan_localization().is_some()
        || input.replan_research().is_some()
    {
        return None;
    }
    RequestAgentFinish.verification_command(step)
}

pub(super) fn schema() -> Value {
    json!({"title":"A^3 SourceWork V1","type":"object","additionalProperties":false,
    "required":["version","next"],"properties":{
        "version":{"const":1},
        "next":{"type":"string","enum":["change","verify","need_evidence"]}
    }})
}

pub(super) fn decode(raw: &str) -> Option<super::after_change::NextWork> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let fields = value.as_object()?;
    if fields.len() != 2 || fields.get("version")? != &json!(1) {
        return None;
    }
    use super::after_change::NextWork;
    match fields.get("next")?.as_str()? {
        "verify" => Some(NextWork::Verify),
        "change" => Some(NextWork::ContinueChange),
        "need_evidence" => Some(NextWork::NeedEvidence),
        _ => None,
    }
}

pub(super) fn already_supplied(
    action: &AgentAction,
    snapshot: SnapshotId,
    sources: &[ContextOriginalSource],
) -> bool {
    let AgentAction::Inspect(inspect) = action else {
        return false;
    };
    let AgentInspectTarget::File(request) = inspect.target() else {
        return false;
    };
    sources
        .iter()
        .any(|source| source.covers(snapshot, request))
}

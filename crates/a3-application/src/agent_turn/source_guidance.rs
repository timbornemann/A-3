//! Source availability selects a smaller work decision, never task completion.
use crate::{AgentContextCompileInput, ContextOriginalSource, RequestAgentFinish};
use a3_domain::{AgentAction, AgentInspectTarget, AgentRun, AgentRunAction, SnapshotId};
use serde_json::{Value, json};

const PROMPT_WITH_VERIFY: &str = "SourceWork V1: the Core has already delivered current original source pages in ORIGINAL_SOURCE blocks below, not just file names or search summaries. Read that code against the CURRENT step and goal now. Choose the next work only: change to implement a concrete missing requirement; verify to request the already planned check (it may fail); need_evidence only for specific source lines or files absent from those blocks. Reading a supplied file again does not implement the task. Partial pages do not prove all required evidence is present. Return exactly version=1 and next, no arguments, code, IDs, claims or success status.";
const PROMPT_CHANGE_ONLY: &str = "SourceWork V1: the Core has already delivered current original source pages in ORIGINAL_SOURCE blocks below, not just file names or search summaries. Read that code against the CURRENT step and goal now. This step has no directly requestable command, so choose only: change to implement a concrete missing requirement; need_evidence only for specific source lines or files absent from those blocks. Reading a supplied file again does not implement the task. Partial pages do not prove all required evidence is present. Return exactly version=1 and next, no arguments, code, IDs, claims or success status.";
const PROMPT_READS_EXHAUSTED_WITH_VERIFY: &str = "SourceWork V1: the Core has already delivered current original source pages and the bounded evidence-read budget for this step is exhausted. Re-reading is not new evidence. Choose only: change to implement concrete missing work, or verify to request the already planned check. Return exactly version=1 and next, no arguments, code, IDs, claims or success status.";
const PROMPT_READS_EXHAUSTED_CHANGE_ONLY: &str = "SourceWork V1: the Core has already delivered current original source pages and the bounded evidence-read budget for this change step is exhausted. Re-reading is not new evidence and this step has no directly requestable command. Choose change to implement concrete missing work. Return exactly version=1 and next=change, no arguments, code, IDs, claims or success status.";
const PROMPT_READS_EXHAUSTED_WITHOUT_SOURCE_WITH_VERIFY: &str = "SourceWork V1: the bounded evidence-read budget for this step is exhausted. No further read may be requested in this attempt; repeated reads are not progress. Choose only: change to implement concrete missing work, or verify to request the already planned check. Return exactly version=1 and next, no arguments, code, IDs, claims or success status.";
const PROMPT_READS_EXHAUSTED_WITHOUT_SOURCE_CHANGE_ONLY: &str = "SourceWork V1: the bounded evidence-read budget for this change step is exhausted. No further read may be requested in this attempt; repeated reads are not progress, and this step has no directly requestable command. Choose change to implement concrete missing work. Return exactly version=1 and next=change, no arguments, code, IDs, claims or success status.";
pub(super) const MAX_READS_PER_STEP: usize = 4;

pub(super) const fn prompt(
    can_verify: bool,
    can_read: bool,
    source_supplied: bool,
) -> &'static str {
    match (can_verify, can_read, source_supplied) {
        (true, true, true) => PROMPT_WITH_VERIFY,
        (false, true, true) => PROMPT_CHANGE_ONLY,
        (true, false, true) => PROMPT_READS_EXHAUSTED_WITH_VERIFY,
        (false, false, true) => PROMPT_READS_EXHAUSTED_CHANGE_ONLY,
        (true, false, false) => PROMPT_READS_EXHAUSTED_WITHOUT_SOURCE_WITH_VERIFY,
        (false, false, false) => PROMPT_READS_EXHAUSTED_WITHOUT_SOURCE_CHANGE_ONLY,
        (_, true, false) => PROMPT_CHANGE_ONLY,
    }
}

pub(super) fn has_current_source(run: &AgentRun, sources: &[ContextOriginalSource]) -> bool {
    sources.iter().any(|source| {
        source.snapshot_id() == run.current_snapshot_id() && !source.range().is_empty()
    })
}

pub(super) fn available(
    input: &AgentContextCompileInput,
    run: &AgentRun,
    sources: &[ContextOriginalSource],
) -> bool {
    let Some(step) = input.task_ledger().step(input.current_step_id()) else {
        return false;
    };
    (has_current_source(run, sources) || input.tool_results().len() >= MAX_READS_PER_STEP)
        && step
            .attempts()
            .last()
            .is_some_and(|attempt| attempt.run_id() == run.id())
        && input.replan_localization().is_none()
        && input.replan_research().is_none()
}

pub(super) fn planned_verification(
    input: &AgentContextCompileInput,
    run: &AgentRun,
    sources: &[ContextOriginalSource],
) -> Option<AgentRunAction> {
    if !available(input, run, sources) {
        return None;
    }
    let step = input.task_ledger().step(input.current_step_id())?;
    RequestAgentFinish.verification_command(step)
}

pub(super) fn schema(can_verify: bool, can_read: bool) -> Value {
    let choices = match (can_verify, can_read) {
        (true, true) => json!(["change", "verify", "need_evidence"]),
        (false, true) => json!(["change", "need_evidence"]),
        (true, false) => json!(["change", "verify"]),
        (false, false) => json!(["change"]),
    };
    json!({"title":"A^3 SourceWork V1","type":"object","additionalProperties":false,
    "required":["version","next"],"properties":{
        "version":{"const":1},
        "next":{"type":"string","enum":choices}
    }})
}

pub(super) fn decode(
    raw: &str,
    can_verify: bool,
    can_read: bool,
) -> Option<super::after_change::NextWork> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let fields = value.as_object()?;
    if fields.len() != 2 || fields.get("version")? != &json!(1) {
        return None;
    }
    use super::after_change::NextWork;
    match fields.get("next")?.as_str()? {
        "verify" if can_verify => Some(NextWork::Verify),
        "change" => Some(NextWork::ContinueChange),
        "need_evidence" if can_read => Some(NextWork::NeedEvidence),
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

//! Bounded private binary representation of failed HTTP observations.
use super::*;
use a3_domain::{
    AgentToolAttemptNumber, LoopbackHttpMismatch, LoopbackHttpMismatchDiagnostic,
    LoopbackStartupPhase, WorktreeId,
};
const DOMAIN: &str = "a3 durable loopback mismatch v1";
const BOUNDS: std::ops::RangeInclusive<usize> = 400..=133_000;
pub(crate) fn encode(value: &LoopbackHttpMismatchDiagnostic) -> Vec<u8> {
    let c = value.context();
    let mut bytes = vec![1];
    for id in [
        c.verification_run_id.as_bytes(),
        c.spec_id.as_bytes(),
        c.run_id.as_bytes(),
        c.tool_run_id.as_bytes(),
        c.step_id.as_bytes(),
        c.snapshot_id.as_bytes(),
    ] {
        bytes.extend_from_slice(id);
    }
    bytes.extend_from_slice(&value.attempt().get().to_le_bytes());
    bytes.extend_from_slice(value.worktree_id().as_bytes());
    bytes.extend_from_slice(value.recipe_id().as_bytes());
    write_script(&mut bytes, value.script().path());
    bytes.extend_from_slice(value.script().content_hash().as_bytes());
    let process = value.process();
    bytes.extend_from_slice(process.specification().as_bytes());
    let s = process.settings();
    bytes.extend_from_slice(&s.timeout.as_millis().to_le_bytes());
    bytes.extend_from_slice(&s.stdout_limit.get().to_le_bytes());
    bytes.extend_from_slice(&s.stderr_limit.get().to_le_bytes());
    bytes.push(u8::from(s.system_root));
    bytes.extend_from_slice(&[
        value.mismatch().phase().tag(),
        value.mismatch().check_index(),
    ]);
    write_observation(&mut bytes, value.mismatch().observation());
    bytes.push(value.decisions().len() as u8);
    for id in value.decisions() {
        bytes.extend_from_slice(id.as_bytes());
    }
    seal(bytes, DOMAIN)
}
fn read_context(r: &mut Reader<'_>) -> Result<LoopbackHttpEvidenceContext, InvalidLoopbackHttp> {
    Ok(LoopbackHttpEvidenceContext {
        verification_run_id: VerificationRunId::from_bytes(r.take()?),
        spec_id: VerificationSpecId::from_bytes(r.take()?),
        run_id: AgentRunId::from_bytes(r.take()?),
        tool_run_id: ToolRunId::from_bytes(r.take()?),
        step_id: TaskStepId::from_bytes(r.take()?),
        snapshot_id: SnapshotId::from_bytes(r.take()?),
    })
}
pub(crate) fn context(bytes: &[u8]) -> Result<LoopbackHttpEvidenceContext, InvalidLoopbackHttp> {
    read_context(&mut open(bytes, BOUNDS, 1, DOMAIN)?)
}
pub(crate) fn decode(
    bytes: &[u8],
    recipe: &LoopbackHttpRecipe,
) -> Result<LoopbackHttpMismatchDiagnostic, InvalidLoopbackHttp> {
    let mut r = open(bytes, BOUNDS, 1, DOMAIN)?;
    let c = read_context(&mut r)?;
    let attempt = AgentToolAttemptNumber::new(r.u32()?).map_err(|_| InvalidLoopbackHttp)?;
    let worktree = WorktreeId::from_bytes(r.take()?);
    let recipe_id = PolicyResourceId::from_bytes(r.take()?);
    let script = FileRevision::new(r.script()?, ContentHash::from_bytes(r.take()?));
    let specification = PolicyResourceId::from_bytes(r.take()?);
    let timeout = ProcessTimeout::from_millis(r.u64()?).map_err(|_| InvalidLoopbackHttp)?;
    let stdout_limit = ProcessOutputLimit::new(r.u32()?).map_err(|_| InvalidLoopbackHttp)?;
    let stderr_limit = ProcessOutputLimit::new(r.u32()?).map_err(|_| InvalidLoopbackHttp)?;
    let system_root = match r.byte()? {
        0 => false,
        1 => true,
        _ => return Err(InvalidLoopbackHttp),
    };
    let phase = match r.byte()? {
        2 => LoopbackStartupPhase::FreeExplicit,
        3 => LoopbackStartupPhase::Default,
        _ => return Err(InvalidLoopbackHttp),
    };
    let index = r.byte()?;
    let actual = read_observation(&mut r)?;
    let count = usize::from(r.byte()?);
    if count != recipe.checks().len() + 6 || count > 14 || recipe_id != recipe.id() {
        return Err(InvalidLoopbackHttp);
    }
    let mut decisions = Vec::with_capacity(count);
    for _ in 0..count {
        decisions.push(PolicyDecisionId::from_bytes(r.take()?));
    }
    r.finish()?;
    let mismatch =
        LoopbackHttpMismatch::new(recipe, phase, index, actual).map_err(|_| InvalidLoopbackHttp)?;
    let process = LoopbackHttpProcess::new(
        specification,
        LoopbackProcessSettings {
            timeout,
            stdout_limit,
            stderr_limit,
            system_root,
        },
    );
    LoopbackHttpMismatchDiagnostic::new(
        c, attempt, worktree, recipe, script, process, mismatch, decisions,
    )
    .map_err(|_| InvalidLoopbackHttp)
}

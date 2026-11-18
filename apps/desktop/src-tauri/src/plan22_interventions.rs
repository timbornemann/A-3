//! Test-worker observations and supervisor barriers; never supplies a plan, patch, or ledger.
#![allow(clippy::too_many_arguments)]
use super::*;
use a3_application::{
    AgentRecoveryChoice, AgentRunExecutionFailure, AgentRunExecutionFuture,
    AgentRunExecutionRequest, AgentRunExecutor, AgentTaskControlResult,
    AgentTaskRecoveryLoadResult, AskResearchStore, TaskLensWorkspaceStore,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationKind, AgentRun, AgentRunId, AgentSessionId,
    MutationApplicationState, TaskId, TaskLedger, TaskStepStatus,
};

const ACK_LIMIT: u64 = 48 * 1024;
const CHECKPOINT_LIMIT: u64 = 512 * 1024;

#[derive(Debug, Default)]
pub(super) struct AttemptObservations {
    pub completed: u64,
    pub last_error: Option<AgentRunExecutionFailure>,
}
pub(super) type SharedAttempts = Arc<Mutex<AttemptObservations>>;
#[derive(Debug)]
pub(super) struct TrackedExecutor {
    pub inner: Arc<ProductionAgentRunExecutor>,
    pub observations: SharedAttempts,
}
impl AgentRunExecutor for TrackedExecutor {
    fn permission_change_request<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> a3_application::AgentPermissionChangeFuture<'a> {
        self.inner.permission_change_request(project)
    }
    fn execute<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: AgentRunExecutionRequest,
        control: &'a a3_application::JobContext,
    ) -> AgentRunExecutionFuture<'a> {
        Box::pin(async move {
            let result = self.inner.execute(project, request, control).await;
            let mut observed = lock_recovering_poison(&self.observations);
            observed.completed = observed.completed.saturating_add(1);
            observed.last_error = result.as_ref().err().copied();
            result
        })
    }
}

pub(super) fn is_resume() -> WorkerResult<bool> {
    match std::env::var("A3_PLAN22_STAGE") {
        Ok(value) if value == "resume" => Ok(true),
        Ok(value) if value == "start" => Ok(false),
        Err(std::env::VarError::NotPresent) => Ok(false),
        _ => Err(failure("input_worker_stage")),
    }
}
fn read_json(directory: &Path, name: &str, limit: u64) -> WorkerResult<Value> {
    let path = regular_child(directory, name, false)?;
    let mut bytes = Vec::new();
    checked(
        checked(fs::File::open(path), "stage_file_open")?
            .take(limit + 1)
            .read_to_end(&mut bytes),
        "stage_file_read",
    )?;
    if bytes.len() as u64 > limit {
        return Err(failure("stage_file_too_large"));
    }
    checked(serde_json::from_slice(&bytes), "stage_file_json")
}
fn write_json(directory: &Path, name: &str, value: &Value) -> WorkerResult<()> {
    let destination = directory.join(name);
    if destination.exists() {
        regular_child(directory, name, false)?;
    }
    let temporary = directory.join(format!("{name}.next"));
    let bytes = checked(serde_json::to_vec_pretty(value), "stage_file_json")?;
    if bytes.len() as u64 > CHECKPOINT_LIMIT {
        return Err(failure("stage_file_too_large"));
    }
    let mut file = checked(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary),
        "stage_file_create",
    )?;
    checked(file.write_all(&bytes), "stage_file_write")?;
    checked(file.sync_all(), "stage_file_sync")?;
    drop(file);
    checked(fs::rename(temporary, destination), "stage_file_commit")
}
fn request_hash(directory: &Path) -> WorkerResult<String> {
    let path = regular_child(directory, "request.json", false)?;
    let mut bytes = Vec::new();
    checked(
        checked(fs::File::open(path), "request_open")?
            .take(MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut bytes),
        "request_read",
    )?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(failure("request_too_large"));
    }
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
fn file_bytes(directory: &Path, relative: &str) -> WorkerResult<Option<Vec<u8>>> {
    let root = regular_child(directory, "worktree", true)?;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(failure("intervention_file_metadata")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4 * 1024 * 1024
    {
        return Err(failure("intervention_file_kind_or_size"));
    }
    let canonical = checked(fs::canonicalize(&path), "intervention_file_canonical")?;
    if !canonical.starts_with(&root) {
        return Err(failure("intervention_file_escape"));
    }
    let mut bytes = Vec::new();
    checked(
        checked(fs::File::open(canonical), "intervention_file_open")?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes),
        "intervention_file_read",
    )?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(failure("intervention_file_too_large"));
    }
    Ok(Some(bytes))
}
fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn parse_id(raw: &str) -> WorkerResult<[u8; 32]> {
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(failure("checkpoint_identity"));
    }
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = checked(
            u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16),
            "checkpoint_identity",
        )?;
    }
    Ok(result)
}
fn parse_payload(raw: &Value) -> WorkerResult<Vec<u8>> {
    let text = raw
        .as_str()
        .ok_or_else(|| failure("intervention_payload"))?;
    if text.len() > 32 * 1024
        || text.len() % 2 != 0
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(failure("intervention_payload_hex"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair =
                std::str::from_utf8(pair).map_err(|_| failure("intervention_payload_hex"))?;
            checked(u8::from_str_radix(pair, 16), "intervention_payload_hex")
        })
        .collect()
}
fn required_string<'a>(value: &'a Value, field: &str) -> WorkerResult<&'a str> {
    value[field]
        .as_str()
        .filter(|value| value.len() <= 256)
        .ok_or_else(|| failure("checkpoint_field"))
}
fn epoch_millis() -> WorkerResult<u64> {
    Ok(now()?.unix_millis())
}
fn checkpoint(directory: &Path, request: &Request) -> WorkerResult<Value> {
    if request.case_id != "A10" {
        return Err(failure("resume_case"));
    }
    let value = read_json(directory, "checkpoint.json", CHECKPOINT_LIMIT)?;
    if value["version"] != json!(1)
        || value["ready"] != json!(true)
        || value["request_blake3"] != json!(request_hash(directory)?)
        || value["source_pid"] == json!(std::process::id())
    {
        return Err(failure("checkpoint_not_ready_or_mismatched"));
    }
    Ok(value)
}
pub(super) fn validate_resume(directory: &Path, request: &Request) -> WorkerResult<()> {
    if is_resume()? {
        checkpoint(directory, request)?;
    }
    Ok(())
}
pub(super) fn initial_report(
    directory: &Path,
    metrics: &SharedMetrics,
) -> WorkerResult<Option<Value>> {
    if !is_resume()? {
        return Ok(None);
    }
    let mut prior = read_json(directory, "worker.json", CHECKPOINT_LIMIT)?;
    if prior["case_id"] != json!("A10")
        || prior["phase"] != json!("restart_ready")
        || prior["terminal"] != json!(false)
        || prior["worker_succeeded"] != json!(true)
        || ["conversation_stopped", "agent_stopped", "scheduler_joined"]
            .iter()
            .any(|key| prior["cleanup"][key] != json!(true))
    {
        return Err(failure("resume_without_completed_first_stage"));
    }
    let saved = &prior["metrics"];
    let number = |name: &str| -> WorkerResult<u64> {
        saved[name]
            .as_u64()
            .filter(|value| *value <= u64::MAX / 2)
            .ok_or_else(|| failure("checkpoint_metrics"))
    };
    {
        let mut data = lock_recovering_poison(metrics);
        data.calls = number("calls")?;
        data.probe_calls = number("probe_requests")?;
        data.completed = number("completed")?;
        data.errors = number("stream_errors")?;
        data.output_bytes = number("output_bytes")?;
        data.prompt_tokens = number("reported_prompt_tokens_sum")?;
        data.output_tokens = number("reported_output_tokens_sum")?;
        data.prompt_usage_missing = number("missing_prompt_usage_calls")?
            .saturating_sub(data.calls.saturating_sub(data.completed));
        data.output_usage_missing = number("missing_output_usage_calls")?
            .saturating_sub(data.calls.saturating_sub(data.completed));
        data.request_bytes = number("message_utf8_bytes")?;
        data.schema_bytes = number("format_schema_utf8_bytes")?;
        data.overflow = number("record_overflow")?;
        data.records = saved["records"]
            .as_array()
            .filter(|array| array.len() <= MAX_CALL_RECORDS)
            .ok_or_else(|| failure("checkpoint_metrics"))?
            .clone();
    }
    prior["phase"] = json!("validating_resume");
    prior["terminal"] = json!(false);
    prior["worker_stage"] = json!("resume");
    prior["worker_succeeded"] = json!(false);
    prior["resume_pid"] = json!(std::process::id());
    prior
        .as_object_mut()
        .ok_or_else(|| failure("checkpoint_report"))?
        .remove("failure_code");
    Ok(Some(prior))
}
pub(super) fn task_clock(
    directory: &Path,
    request: &Request,
    report: &mut Value,
) -> WorkerResult<Instant> {
    let current = epoch_millis()?;
    let origin = if is_resume()? {
        checkpoint(directory, request)?["task_start_unix_millis"]
            .as_u64()
            .ok_or_else(|| failure("checkpoint_clock"))?
    } else {
        current
    };
    let elapsed = current
        .checked_sub(origin)
        .ok_or_else(|| failure("checkpoint_clock_regressed"))?;
    if elapsed >= request.timeout_seconds.saturating_mul(1000) {
        return Err(failure("deadline_exceeded"));
    }
    report["task_start_unix_millis"] = json!(origin);
    report["worker_stage"] = json!(if is_resume()? { "resume" } else { "start" });
    Instant::now()
        .checked_sub(Duration::from_millis(elapsed))
        .ok_or_else(|| failure("checkpoint_clock_range"))
}
pub(super) async fn publish(
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
) -> WorkerResult<()> {
    let mut compiler = checked(
        BuiltinIncrementalIndexCompiler::new(checked(ParserPoolSize::new(1), "parser_pool")?),
        "index_compiler",
    )?;
    checked(
        RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        )
        .execute(
            project,
            &checked(
                RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                ),
                "index_batch",
            )?,
            &mut compiler,
            &IndexControl,
        )
        .await,
        "intervention_publish",
    )?;
    Ok(())
}

fn semantic_dag_digest(nodes: &[(Value, Vec<usize>)], active: &[usize]) -> String {
    let mut hashes = vec![None::<String>; nodes.len()];
    for _ in 0..nodes.len() {
        let mut progressed = false;
        for (index, (primitive, dependencies)) in nodes.iter().enumerate() {
            if hashes[index].is_some() {
                continue;
            }
            let Some(mut dependency_hashes) = dependencies
                .iter()
                .map(|dependency| hashes.get(*dependency).and_then(Clone::clone))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            dependency_hashes.sort();
            hashes[index] = Some(digest(
                json!({"node":primitive,"dependencies":dependency_hashes})
                    .to_string()
                    .as_bytes(),
            ));
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    let mut active_hashes = active
        .iter()
        .map(|index| hashes.get(*index).and_then(Clone::clone))
        .collect::<Vec<_>>();
    active_hashes.sort();
    digest(
        json!({"semantic_dag_version":1,"active_nodes":active_hashes})
            .to_string()
            .as_bytes(),
    )
}
fn plan_digest(ledger: &TaskLedger) -> String {
    let steps = ledger.steps().collect::<Vec<_>>();
    let nodes = steps
        .iter()
        .map(|step| {
            let mut expected = step
                .definition()
                .expected_evidence()
                .iter()
                .map(|item| item.as_str())
                .collect::<Vec<_>>();
            expected.sort();
            (
                json!({"outcome":step.definition().intended_outcome().as_str(),"expected":expected,
            "verification_method":format!("{:?}",step.definition().verification_spec().method())}),
                step.definition()
                    .dependencies()
                    .iter()
                    .filter_map(|dependency| {
                        steps
                            .iter()
                            .position(|other| other.definition().id() == dependency.prerequisite())
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let active = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.is_active_plan_step())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    semantic_dag_digest(&nodes, &active)
}
fn applied_patches(attempts: &[AgentMutationAttempt]) -> usize {
    attempts
        .iter()
        .filter(|attempt| {
            attempt.kind() == AgentMutationKind::Patch
                && attempt.disposition().application_state() == MutationApplicationState::Applied
        })
        .count()
}
fn terminal_attempt_digests(ledger: &TaskLedger) -> Vec<String> {
    ledger
        .steps()
        .flat_map(|step| {
            step.attempts()
                .iter()
                .filter(|attempt| attempt.finished_at().is_some())
                .map(|attempt| {
                    let outcome=match attempt.outcome(){
                        a3_domain::TaskStepAttemptOutcome::Active=>json!({"kind":"active"}),
                        a3_domain::TaskStepAttemptOutcome::Blocked{reason}=>json!({"kind":"blocked","reason_blake3":digest(reason.as_str().as_bytes())}),
                        a3_domain::TaskStepAttemptOutcome::VerificationFailed=>json!({"kind":"verification_failed"}),
                        a3_domain::TaskStepAttemptOutcome::Completed=>json!({"kind":"completed"}),
                        a3_domain::TaskStepAttemptOutcome::Failed{reason}=>json!({"kind":"failed","reason_blake3":digest(reason.as_str().as_bytes())}),
                        a3_domain::TaskStepAttemptOutcome::Cancelled{reason}=>json!({"kind":"cancelled","reason_blake3":digest(reason.as_str().as_bytes())}),
                    };
                    let verification=attempt.verification().map(|verification|json!({
                        "id":hex(verification.id().as_bytes()),"spec_id":hex(verification.spec_id().as_bytes()),
                        "run_id":hex(verification.run_id().as_bytes()),"passed":verification.passed(),
                        "verified_at":verification.verified_at().unix_millis(),
                        "evidence":verification.evidence_ids().iter().map(|id|hex(id.as_bytes())).collect::<Vec<_>>(),
                        "failure_summary_blake3":match verification.outcome(){a3_domain::StepVerificationOutcome::Passed=>None,
                            a3_domain::StepVerificationOutcome::Failed{summary}=>Some(digest(summary.as_str().as_bytes()))}
                    }));
                    digest(json!({"step_id":hex(step.definition().id().as_bytes()),"attempt":attempt.number().get(),
                        "run_id":hex(attempt.run_id().as_bytes()),"started_at":attempt.started_at().unix_millis(),
                        "finished_at":attempt.finished_at().map(|time|time.unix_millis()),"outcome":outcome,
                        "summary_blake3":attempt.result_summary().map(|summary|digest(summary.as_str().as_bytes())),
                        "evidence":attempt.evidence_ids().iter().map(|id|hex(id.as_bytes())).collect::<Vec<_>>(),
                        "verification":verification}).to_string().as_bytes())
                })
        })
        .collect()
}
fn completed_step_ids(ledger: &TaskLedger) -> Vec<String> {
    ledger
        .steps()
        .filter(|step| step.status() == TaskStepStatus::Completed)
        .map(|step| step.definition().id().to_string())
        .collect()
}
fn mutation_digests(attempts: &[AgentMutationAttempt]) -> Vec<String> {
    attempts
        .iter()
        .filter(|attempt| {
            attempt.tool_attempt().status() != a3_domain::AgentToolAttemptStatus::InFlight
        })
        .map(|attempt| {
            let tool=attempt.tool_attempt();let disposition=attempt.disposition();
            digest(json!({"tool_run_id":hex(tool.tool_run_id().as_bytes()),"run_id":hex(tool.run_id().as_bytes()),
                "snapshot_id":hex(tool.snapshot_id().as_bytes()),"attempt":tool.attempt().get(),
                "status":format!("{:?}",tool.status()),"started_at":tool.started_at().unix_millis(),"updated_at":tool.updated_at().unix_millis(),
                "fingerprint":hex(&attempt.fingerprint().as_bytes()),"kind":format!("{:?}",attempt.kind()),
                "application_state":format!("{:?}",disposition.application_state()),"requires_reconciliation":disposition.requires_reconciliation(),
                "requires_replan":disposition.requires_replan(),"permits_future_mutation":disposition.permits_future_mutation(),
                "reconciled_snapshot_id":disposition.reconciled_snapshot_id().map(|id|hex(id.as_bytes()))}).to_string().as_bytes())
        })
        .collect()
}
fn applied_mutation_keys(attempts: &[AgentMutationAttempt]) -> Vec<String> {
    attempts.iter().filter(|attempt| attempt.disposition().application_state()==MutationApplicationState::Applied)
        .map(|attempt| digest(json!({"run_id":hex(attempt.tool_attempt().run_id().as_bytes()),
            "fingerprint":hex(&attempt.fingerprint().as_bytes()),"pre_snapshot":hex(attempt.tool_attempt().snapshot_id().as_bytes())}).to_string().as_bytes()))
        .collect()
}
async fn functional_milestone(
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
    task_id: TaskId,
    ledger: &TaskLedger,
) -> WorkerResult<Option<Value>> {
    let current = checked(
        store.latest_published_index(project, &IndexControl).await,
        "milestone_index",
    )?
    .ok_or_else(|| failure("milestone_index_missing"))?;
    let catalog = checked(
        DiscoverProjectCommands.execute(project.worktree().id(), &current),
        "milestone_commands",
    )?;
    for step in ledger
        .steps()
        .filter(|step| step.status() == TaskStepStatus::Completed)
    {
        let spec = step.definition().verification_spec();
        let command_id = match spec.target() {
            a3_domain::VerificationTarget::Command { command_id, .. }
            | a3_domain::VerificationTarget::Test { command_id, .. } => *command_id,
            _ => continue,
        };
        if !catalog.commands().iter().any(|command| {
            command.id() == command_id && command.kind() == a3_domain::DiscoveredCommandKind::Test
        }) {
            continue;
        }
        let Some(verification) = step
            .attempts()
            .last()
            .and_then(a3_domain::TaskStepAttempt::verification)
            .filter(|verification| verification.passed())
        else {
            continue;
        };
        let state = checked(
            store
                .load_verification_inspection_state(
                    project,
                    task_id,
                    verification.evidence_ids(),
                    Duration::from_secs(5),
                    &crate::DesktopBoundedReadControl::new(),
                )
                .await,
            "milestone_evidence",
        )?;
        for evidence in state.evidence() {
            if evidence.run_id() == verification.run_id()
                && a3_domain::VerificationEvidenceEvaluation::evaluate(spec, evidence)
                    == a3_domain::VerificationEvidenceEvaluation::Passed
                && a3_domain::EvidenceFreshness::evaluate(evidence, state.published_index())
                    == a3_domain::EvidenceFreshness::Fresh
            {
                return Ok(Some(
                    json!({"step_id":hex(step.definition().id().as_bytes()),"verification_id":hex(verification.id().as_bytes()),
                    "evidence_id":hex(evidence.id().as_bytes()),"test_command_id":hex(command_id.as_bytes()),"passed":true,"fresh":true,
                    "method":"completed_step_current_fresh_passing_discovered_test_command"}),
                ));
            }
        }
    }
    Ok(None)
}
fn requirement_ids(text: &str) -> std::collections::BTreeSet<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| {
            word.len() == 3
                && word.starts_with('R')
                && word.as_bytes()[1..].iter().all(u8::is_ascii_digit)
        })
        .map(str::to_owned)
        .collect()
}
fn write_hashes(
    directory: &Path,
    request: &Request,
) -> WorkerResult<std::collections::BTreeMap<String, Option<String>>> {
    request
        .allowed_write_paths
        .iter()
        .map(|path| {
            let path = std::str::from_utf8(path.as_bytes())
                .map_err(|_| failure("checkpoint_path"))?
                .to_owned();
            Ok((
                path.clone(),
                file_bytes(directory, &path)?.map(|body| digest(&body)),
            ))
        })
        .collect()
}

pub(super) struct Driver {
    request_id: Option<String>,
    injected: bool,
    before_patch_count: usize,
    before_attempt_count: u64,
    recovery_attempted: bool,
    recovery_attempt_count: u64,
    stale_approval_rejected: bool,
    prior_ledger: Option<TaskLedger>,
    user_prefix: Option<Vec<u8>>,
    first_ask_done: bool,
    pause_requested: bool,
    pre_restart_paths: std::collections::BTreeMap<String, String>,
    pending_repeat: Option<(usize, u64)>,
    repeated_applied_patches: usize,
}
impl Driver {
    pub(super) fn new() -> Self {
        Self {
            request_id: None,
            injected: false,
            before_patch_count: 0,
            before_attempt_count: 0,
            recovery_attempted: false,
            recovery_attempt_count: 0,
            stale_approval_rejected: false,
            prior_ledger: None,
            user_prefix: None,
            first_ask_done: false,
            pause_requested: false,
            pre_restart_paths: std::collections::BTreeMap::new(),
            pending_repeat: None,
            repeated_applied_patches: 0,
        }
    }

    pub(super) async fn start_session(
        &mut self,
        directory: &Path,
        request: &Request,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        sessions: &AgentSessionManager,
        manager: &Arc<AgentRunManager>,
    ) -> WorkerResult<AgentSessionId> {
        report["interventions"] = json!({"implemented":true,"model_outputs_scripted":false});
        if is_resume()? {
            let saved = checkpoint(directory, request)?;
            let changed = saved["changed_write_paths"]
                .as_object()
                .ok_or_else(|| failure("checkpoint_changed_paths"))?;
            for (path, hash) in changed {
                if !request
                    .allowed_write_paths
                    .iter()
                    .any(|allowed| allowed.as_bytes() == path.as_bytes())
                {
                    return Err(failure("checkpoint_changed_path_scope"));
                }
                self.pre_restart_paths.insert(
                    path.clone(),
                    hash.as_str()
                        .filter(|value| value.len() == 64)
                        .ok_or_else(|| failure("checkpoint_changed_path_hash"))?
                        .to_owned(),
                );
            }
            let session_id =
                AgentSessionId::from_bytes(parse_id(required_string(&saved, "session_id")?)?);
            let detail = checked(
                store
                    .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                    .await,
                "resume_session",
            )?
            .ok_or_else(|| failure("resume_session_missing"))?;
            let work = detail
                .session()
                .active_work_item()
                .ok_or_else(|| failure("resume_task_missing"))?;
            if work.task_id().to_string() != required_string(&saved, "task_id")? {
                return Err(failure("resume_task_mismatch"));
            }
            let stored = checked(
                store.load_task_ledger(project, work.task_id()).await,
                "resume_ledger",
            )?
            .ok_or_else(|| failure("resume_ledger_missing"))?;
            let run_id = stored
                .ledger()
                .steps()
                .filter_map(|step| step.attempts().last())
                .map(a3_domain::TaskStepAttempt::run_id)
                .next()
                .ok_or_else(|| failure("resume_run_missing"))?;
            if run_id.to_string() != required_string(&saved, "run_id")? {
                return Err(failure("resume_run_mismatch"));
            }
            let run = checked(store.load_agent_run(project, run_id).await, "resume_run")?
                .ok_or_else(|| failure("resume_run_missing"))?;
            let inspector = InspectAgentTaskRecovery::new(
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
            );
            let inspected = checked(
                inspector
                    .execute(
                        project,
                        work.task_id(),
                        now()?,
                        &crate::DesktopBoundedReadControl::new(),
                        &IndexControl,
                    )
                    .await,
                "resume_inspection",
            )?;
            let AgentTaskRecoveryLoadResult::Available(inspected) = inspected else {
                return Err(failure("resume_not_controllable"));
            };
            if !inspected.can_resume() {
                report["product_stop_reason"] =
                    json!("actual_recovery_requires_replan_or_reconciliation");
                return Err(failure("actual_resume_not_permitted"));
            }
            if run.last_event_sequence().get()
                < saved["event_sequence"]
                    .as_u64()
                    .ok_or_else(|| failure("checkpoint_sequence"))?
            {
                return Err(failure("resume_sequence_regressed"));
            }
            let controller = ControlAgentTaskRun::new(
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
            );
            let result = checked(
                controller
                    .execute(
                        project,
                        work.task_id(),
                        inspected.ledger_revision(),
                        inspected.ledger_store_version(),
                        AgentRecoveryChoice::Resume,
                        RunEventId::from_bytes(identity()?),
                        now()?,
                        &crate::DesktopBoundedReadControl::new(),
                        &IndexControl,
                    )
                    .await,
                "resume_control",
            )?;
            let AgentTaskControlResult::Applied {
                ledger_store_version,
                ..
            } = result
            else {
                return Err(failure("actual_resume_not_applied"));
            };
            let latest = checked(
                store.load_task_ledger(project, work.task_id()).await,
                "resume_ledger",
            )?
            .ok_or_else(|| failure("resume_ledger_missing"))?;
            if let Some(reporter) = sessions.reporter.as_ref() {
                reporter.link(work.task_id(), session_id);
            }
            checked(
                manager.start_attempt(AgentRunExecutionRequest::new(
                    work.task_id(),
                    latest.ledger().revision(),
                    ledger_store_version,
                )),
                "resume_start",
            )?;
            report["observation_evidence"] = json!({"version":1,"source":"production_journal_and_scoped_intervention","case_id":"A10",
                "process_restarted":saved["source_pid"]!=json!(std::process::id()),"durable_state_reopened":true,"same_run":true,
                "source_pid":saved["source_pid"],"resume_pid":std::process::id(),"resumed_run_usage_non_decreasing":run.usage().turn_count() as u64>=saved["run_turns"].as_u64().unwrap_or(u64::MAX),
                "completed_steps_before_restart":saved["completed_step_ids"].as_array().map_or(0,Vec::len),
                "prior_mutation_attempts":saved["mutation_digests"].as_array().map_or(0,Vec::len),
                "run_memory_input_present":false,"packed_run_memory_observed":false,
                "compaction_observation_method":"current_production_uses_no_run_memory_input"});
            return Ok(session_id);
        }
        if request.case_id == "A10" {
            report["initial_write_file_blake3"] = json!(write_hashes(directory, request)?);
        }
        let objective = if request.case_id == "Q05" {
            "Welcher DEFAULT_BACKEND gilt aktuell in config.py?".to_owned()
        } else {
            request.objective.clone()
        };
        let detail = checked(
            sessions
                .submit_for_target_mode(
                    project,
                    None,
                    None,
                    request.mode,
                    Some(AgentResearchDepth::Standard),
                    false,
                    objective,
                )
                .await,
            "submit_failed",
        )?;
        Ok(detail.session().id())
    }

    async fn barrier(
        &mut self,
        directory: &Path,
        request: &Request,
        stage: &str,
        paths: &[&str],
        started: Instant,
        metrics: &SharedMetrics,
        report: &mut Value,
    ) -> WorkerResult<Vec<(String, Option<Vec<u8>>, Vec<u8>)>> {
        let request_id = hex(&identity()?);
        self.request_id = Some(request_id.clone());
        let mut before = Vec::new();
        for path in paths {
            before.push(((*path).to_owned(), file_bytes(directory, path)?));
        }
        report["phase"] = json!("awaiting_intervention");
        report["intervention_request"] = json!({"version":1,"request_id":request_id,"case_id":request.case_id,"stage":stage,
            "relative_paths":paths,"at_before_first_patch_approval":request.mode==AgentSessionMode::Agent,
            "before_blake3":before.iter().map(|(path,body)|(path.clone(),body.as_ref().map(|bytes|digest(bytes)))).collect::<std::collections::BTreeMap<_,_>>()});
        atomic_report(directory, report)?;
        if directory.join("intervention-request.json").exists() {
            return Err(failure("intervention_request_already_exists"));
        }
        write_json(
            directory,
            "intervention-request.json",
            &report["intervention_request"],
        )?;
        let mut last = Instant::now();
        let ack = loop {
            if started.elapsed() >= Duration::from_secs(request.timeout_seconds) {
                return Err(failure("intervention_deadline_exceeded"));
            }
            if directory.join("intervention.json").exists() {
                break read_json(directory, "intervention.json", ACK_LIMIT)?;
            }
            if last.elapsed() >= Duration::from_secs(1) {
                report["metrics"] = metrics_json(metrics);
                atomic_report(directory, report)?;
                last = Instant::now();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        object_fields(
            &ack,
            &[
                "version",
                "request_id",
                "case_id",
                "stage",
                "before_sha256",
                "after_sha256",
                "expected_before_hex",
                "expected_after_hex",
            ],
        )?;
        for (path, old) in &before {
            if old.is_none() != ack["before_sha256"][path].is_null() {
                return Err(failure("intervention_ack_presence"));
            }
        }
        if ack["version"] != json!(1)
            || ack["request_id"] != json!(request_id)
            || ack["case_id"] != json!(request.case_id)
            || ack["stage"] != json!(stage)
        {
            return Err(failure("intervention_ack_binding"));
        }
        for field in ["before_sha256", "after_sha256"] {
            let object = ack[field]
                .as_object()
                .ok_or_else(|| failure("intervention_ack_hashes"))?;
            if object.len() != paths.len() || paths.iter().any(|path| !object.contains_key(*path)) {
                return Err(failure("intervention_ack_paths"));
            }
            for value in object.values() {
                if value.is_null() && field == "before_sha256" {
                    continue;
                }
                if !value.as_str().is_some_and(|hash| {
                    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                }) {
                    return Err(failure("intervention_ack_hash"));
                }
            }
        }
        for field in ["expected_before_hex", "expected_after_hex"] {
            let object = ack[field]
                .as_object()
                .ok_or_else(|| failure("intervention_payload_map"))?;
            if object.len() != paths.len() || paths.iter().any(|path| !object.contains_key(*path)) {
                return Err(failure("intervention_payload_paths"));
            }
        }
        let mut payload_bytes = 0usize;
        let mut changes = Vec::new();
        for (path, old) in before {
            let after = file_bytes(directory, &path)?
                .ok_or_else(|| failure("intervention_target_absent"))?;
            let expected_before = if ack["expected_before_hex"][&path].is_null() {
                None
            } else {
                Some(parse_payload(&ack["expected_before_hex"][&path])?)
            };
            let expected_after = parse_payload(&ack["expected_after_hex"][&path])?;
            payload_bytes = payload_bytes
                .saturating_add(expected_before.as_ref().map_or(0, Vec::len))
                .saturating_add(expected_after.len());
            if payload_bytes > 16 * 1024 || old != expected_before || after != expected_after {
                return Err(failure("intervention_exact_bytes_mismatch"));
            }
            if old.as_ref() == Some(&after) {
                return Err(failure("intervention_not_observed"));
            }
            changes.push((path, old, after));
        }
        report["intervention_ack"] = json!({"version":1,"request_id":request_id,"sha256_verified_by_supervisor_not_worker":true,
            "before_sha256":ack["before_sha256"],"after_sha256":ack["after_sha256"],
            "observed_after_blake3":changes.iter().map(|(path,_,body)|(path.clone(),digest(body))).collect::<std::collections::BTreeMap<_,_>>()});
        self.injected = true;
        Ok(changes)
    }

    pub(super) async fn before_patch(
        &mut self,
        directory: &Path,
        request: &Request,
        started: Instant,
        metrics: &SharedMetrics,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        ledger: &TaskLedger,
        attempts: &SharedAttempts,
        run_id: AgentRunId,
        action: &AgentApprovalAction,
    ) -> WorkerResult<()> {
        if request.case_id == "A10" && is_resume()? {
            if let AgentApprovalAction::Patch(patch) = action {
                let touches = patch
                    .files()
                    .iter()
                    .flat_map(|file| file.source_path().into_iter().chain(file.target_path()))
                    .any(|path| {
                        std::str::from_utf8(path.as_bytes())
                            .ok()
                            .is_some_and(|path| self.pre_restart_paths.contains_key(path))
                    });
                if touches {
                    let current = checked(
                        store.load_agent_mutation_attempts(project, run_id).await,
                        "restart_mutations",
                    )?;
                    self.pending_repeat = Some((
                        applied_patches(&current),
                        lock_recovering_poison(attempts).completed.saturating_add(1),
                    ));
                }
            }
            return Ok(());
        }
        if self.injected || !matches!(request.case_id.as_str(), "A09" | "A11") {
            return Ok(());
        }
        let mutations = checked(
            store.load_agent_mutation_attempts(project, run_id).await,
            "intervention_mutation_read",
        )?;
        self.before_patch_count = applied_patches(&mutations);
        self.before_attempt_count = lock_recovering_poison(attempts).completed;
        self.prior_ledger = Some(ledger.clone());
        let (stage, paths) = if request.case_id == "A09" {
            ("before_first_patch_approval", vec!["subject.py"])
        } else {
            (
                "after_first_plan_materialization",
                vec!["reports.py", "tests/test_reports.py"],
            )
        };
        let changes = self
            .barrier(directory, request, stage, &paths, started, metrics, report)
            .await?;
        if request.case_id == "A09" {
            let body = &changes[0].2;
            let end = body
                .windows(b"\ndef render".len())
                .position(|window| window == b"\ndef render")
                .ok_or_else(|| failure("intervention_user_prefix_missing"))?;
            self.user_prefix = Some(body[..end].to_vec());
        }
        publish(store, project).await?;
        report["observation_evidence"] = json!({"version":1,"source":"production_journal_and_scoped_intervention","case_id":request.case_id,
            "intervention_request_id":self.request_id,"injection_published":true,"first_patch_waiting":true,
            "at_before_first_patch_approval":true,"plan_materialized_before_injection":true,
            "prior_plan_digest":plan_digest(ledger),"applied_patch_count_before":self.before_patch_count,
            "saved_user_prefix_blake3":self.user_prefix.as_ref().map(|bytes|digest(bytes))});
        Ok(())
    }

    pub(super) async fn ask_finished(
        &mut self,
        directory: &Path,
        request: &Request,
        started: Instant,
        metrics: &SharedMetrics,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        sessions: &AgentSessionManager,
        session_id: AgentSessionId,
        detail: &a3_application::AgentSessionDetail,
    ) -> WorkerResult<bool> {
        report["research_trace"] = research_trace(store, project, session_id).await?;
        if request.case_id != "Q05" {
            return Ok(false);
        }
        if !self.first_ask_done {
            if detail.session().state() != AgentSessionState::Completed {
                return Ok(false);
            }
            self.first_ask_done = true;
            report["ask_answer_before"] = report["ask_answer"].clone();
            report["ask_answer_before_truncated"] = report["ask_answer_truncated"].clone();
            let initial_cited = cited_config_hash(&report["research_trace"]);
            let before =
                file_bytes(directory, "config.py")?.ok_or_else(|| failure("config_missing"))?;
            checked(sessions.quiesce(), "ask_intervention_quiesce")?;
            let changes = self
                .barrier(
                    directory,
                    request,
                    "after_initial_answer",
                    &["config.py"],
                    started,
                    metrics,
                    report,
                )
                .await?;
            publish(store, project).await?;
            report["observation_evidence"] = json!({"version":1,"source":"production_journal_and_scoped_intervention","case_id":"Q05",
                "intervention_request_id":self.request_id,"initial_answer_completed":true,"initial_config_blake3":digest(&before),
                "post_edit_config_blake3":digest(&changes[0].2),"initial_cited_config_blake3":initial_cited});
            checked(
                sessions
                    .submit_for_target_mode(
                        project,
                        Some(session_id),
                        Some(detail.session().revision()),
                        AgentSessionMode::Ask,
                        Some(AgentResearchDepth::Standard),
                        false,
                        request.objective.clone(),
                    )
                    .await,
                "ask_followup_submit",
            )?;
            return Ok(true);
        }
        let final_cited = cited_config_hash(&report["research_trace"]);
        report["observation_evidence"]["final_cited_config_blake3"] = json!(final_cited);
        report["observations"]["source_edit_injected"] = json!(self.injected);
        Ok(false)
    }

    pub(super) async fn stale_approval_result(
        &mut self,
        request: &Request,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        run_id: AgentRunId,
        result: &AgentApprovalControlResult,
    ) -> WorkerResult<bool> {
        if !self.injected
            || !matches!(request.case_id.as_str(), "A09" | "A11")
            || *result != AgentApprovalControlResult::ActivityChanged
        {
            return Ok(false);
        }
        let mutations = checked(
            store.load_agent_mutation_attempts(project, run_id).await,
            "intervention_mutation_read",
        )?;
        self.stale_approval_rejected = applied_patches(&mutations) == self.before_patch_count;
        report["observation_evidence"]["stale_approval_control_result"] = json!("activity_changed");
        report["observation_evidence"]["applied_patch_count_after"] =
            json!(applied_patches(&mutations));
        report["observations"]["stale_patch_rejected"] = json!(self.stale_approval_rejected);
        Ok(self.stale_approval_rejected)
    }

    pub(super) async fn tick(
        &mut self,
        directory: &Path,
        request: &Request,
        started: Instant,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        sessions: &AgentSessionManager,
        manager: &Arc<AgentRunManager>,
        session_id: AgentSessionId,
        task_id: TaskId,
        ledger: &TaskLedger,
        run: &AgentRun,
        attempts: &SharedAttempts,
    ) -> WorkerResult<bool> {
        if let Some((before, through_attempt)) = self.pending_repeat
            && lock_recovering_poison(attempts).completed >= through_attempt
        {
            let current = checked(
                store.load_agent_mutation_attempts(project, run.id()).await,
                "restart_mutations",
            )?;
            self.repeated_applied_patches = self
                .repeated_applied_patches
                .saturating_add(applied_patches(&current).saturating_sub(before));
            self.pending_repeat = None;
        }
        if matches!(request.case_id.as_str(), "A09" | "A11")
            && self.injected
            && !manager.activity().state().owns_live_worker()
        {
            let (sequence, error) = {
                let observed = lock_recovering_poison(attempts);
                (observed.completed, observed.last_error)
            };
            if (self.stale_approval_rejected
                || (sequence > self.before_attempt_count && error.is_some()))
                && !self.recovery_attempted
            {
                let mutations = checked(
                    store.load_agent_mutation_attempts(project, run.id()).await,
                    "intervention_mutation_read",
                )?;
                report["observation_evidence"]["stale_continuation_error"] =
                    json!(error.map(|value| format!("{value:?}")));
                report["observation_evidence"]["applied_patch_count_after"] =
                    json!(applied_patches(&mutations));
                report["observations"]["stale_patch_rejected"] = json!(
                    (self.stale_approval_rejected
                        || error == Some(AgentRunExecutionFailure::AnchorsChanged))
                        && applied_patches(&mutations) == self.before_patch_count
                );
                self.recovery_attempted = true;
                self.recovery_attempt_count = sequence;
                let stored = checked(
                    store.load_task_ledger(project, task_id).await,
                    "intervention_ledger",
                )?
                .ok_or_else(|| failure("intervention_ledger_missing"))?;
                let result = ControlAgentTaskRun::new(
                    store.clone(),
                    store.clone(),
                    store.clone(),
                    store.clone(),
                    store.clone(),
                )
                .execute(
                    project,
                    task_id,
                    stored.ledger().revision().get(),
                    stored.version(),
                    AgentRecoveryChoice::Replan,
                    RunEventId::from_bytes(identity()?),
                    now()?,
                    &crate::DesktopBoundedReadControl::new(),
                    &IndexControl,
                )
                .await;
                report["observation_evidence"]["actual_replan_control"] = json!(
                    result
                        .as_ref()
                        .map(|value| format!("{value:?}"))
                        .unwrap_or_else(|_| "control_failed".to_owned())
                );
                if let Ok(AgentTaskControlResult::Applied {
                    ledger_store_version,
                    ..
                }) = result
                {
                    let latest = checked(
                        store.load_task_ledger(project, task_id).await,
                        "intervention_ledger",
                    )?
                    .ok_or_else(|| failure("intervention_ledger_missing"))?;
                    if let Err(error) = manager.start_attempt(AgentRunExecutionRequest::new(
                        task_id,
                        latest.ledger().revision(),
                        ledger_store_version,
                    )) {
                        report["product_stop_reason"] =
                            json!(format!("actual_replan_start_{error:?}"));
                        self.finish(
                            directory,
                            request,
                            report,
                            store,
                            project,
                            task_id,
                            latest.ledger(),
                            run,
                        )
                        .await?;
                        return Ok(true);
                    }
                    return Ok(false);
                }
                report["product_stop_reason"] = json!("actual_replan_control_did_not_apply");
                self.finish(
                    directory, request, report, store, project, task_id, ledger, run,
                )
                .await?;
                return Ok(true);
            }
            if self.recovery_attempted && sequence > self.recovery_attempt_count && error.is_some()
            {
                report["product_stop_reason"] = json!("actual_replan_execution_failed");
                self.finish(
                    directory, request, report, store, project, task_id, ledger, run,
                )
                .await?;
                return Ok(true);
            }
        }
        if request.case_id == "A10" && !is_resume()? {
            let completed = completed_step_ids(ledger);
            let milestone = if !self.pause_requested && !run.state().is_terminal() {
                functional_milestone(store, project, task_id, ledger).await?
            } else {
                None
            };
            if let Some(milestone) = milestone {
                report["observation_evidence"]["functional_pause_milestone"] = milestone;
                self.pause_requested = true;
                match manager.pause(task_id) {
                    Ok(()) => report["pause_requested"] = json!(true),
                    Err(error) => {
                        report["product_stop_reason"] =
                            json!(format!("pause_checkpoint_not_reached_{error:?}"));
                        self.finish(
                            directory, request, report, store, project, task_id, ledger, run,
                        )
                        .await?;
                        return Ok(true);
                    }
                }
            }
            if self.pause_requested
                && manager.activity().state()
                    == crate::agent_run_manager::AgentRunActivityState::Paused
            {
                checked(sessions.quiesce(), "restart_conversation_quiesce")?;
                let inspected = checked(
                    InspectAgentTaskRecovery::new(
                        store.clone(),
                        store.clone(),
                        store.clone(),
                        store.clone(),
                        store.clone(),
                    )
                    .execute(
                        project,
                        task_id,
                        now()?,
                        &crate::DesktopBoundedReadControl::new(),
                        &IndexControl,
                    )
                    .await,
                    "restart_inspection",
                )?;
                if !matches!(inspected,AgentTaskRecoveryLoadResult::Available(ref value) if value.can_resume())
                {
                    report["product_stop_reason"] = json!("actual_pause_not_resumable");
                    return Ok(true);
                }
                let mutations = checked(
                    store.load_agent_mutation_attempts(project, run.id()).await,
                    "restart_mutations",
                )?;
                let current_hashes = write_hashes(directory, request)?;
                let initial = report["initial_write_file_blake3"]
                    .as_object()
                    .ok_or_else(|| failure("checkpoint_initial_hashes"))?;
                let changed = current_hashes
                    .iter()
                    .filter_map(|(path, hash)| {
                        hash.as_ref()
                            .filter(|hash| initial.get(path) != Some(&json!(hash)))
                            .map(|hash| (path.clone(), hash.clone()))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let checkpoint = json!({"version":1,"ready":false,"request_blake3":request_hash(directory)?,"source_pid":std::process::id(),
                    "session_id":session_id.to_string(),"task_id":task_id.to_string(),"run_id":run.id().to_string(),
                    "event_sequence":run.last_event_sequence().get(),"goal_reference":{"task_id":hex(run.goal_contract().task_id().as_bytes()),"revision":run.goal_contract().revision().get()},
                    "run_turns":run.usage().turn_count(),"run_prompt_tokens":run.usage().prompt_tokens(),"run_output_tokens":run.usage().output_tokens(),
                    "completed_step_ids":completed,"terminal_attempt_digests":terminal_attempt_digests(ledger),"mutation_digests":mutation_digests(&mutations),
                    "changed_write_paths":changed,"applied_mutation_keys":applied_mutation_keys(&mutations),"order_state":report["order_state"].clone(),
                    "task_start_unix_millis":report["task_start_unix_millis"],"elapsed_task_millis":started.elapsed().as_millis(),
                    "remaining_task_millis":request.timeout_seconds.saturating_mul(1000).saturating_sub(started.elapsed().as_millis() as u64)});
                write_json(directory, "checkpoint.json", &checkpoint)?;
                report["restart_requested"] = json!(true);
                return Ok(true);
            }
            if self.pause_requested
                && manager.activity().state()
                    == crate::agent_run_manager::AgentRunActivityState::Failed
            {
                report["product_stop_reason"] = json!("actual_pause_failed_or_terminalized");
                self.finish(
                    directory, request, report, store, project, task_id, ledger, run,
                )
                .await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) async fn finish(
        &self,
        directory: &Path,
        request: &Request,
        report: &mut Value,
        store: &Arc<LibsqlKnowledgeStore>,
        project: &ProjectIdentity,
        task_id: TaskId,
        ledger: &TaskLedger,
        run: &AgentRun,
    ) -> WorkerResult<()> {
        report["observations"]["user_edit_injected"] =
            json!(request.case_id == "A09" && self.injected);
        report["observations"]["caller_injected_after_plan"] =
            json!(request.case_id == "A11" && self.injected);
        if let Some(prefix) = &self.user_prefix {
            let body = file_bytes(directory, "subject.py")?
                .ok_or_else(|| failure("user_edit_file_missing"))?;
            let preserved = body.starts_with(prefix);
            report["observation_evidence"]["final_user_prefix_blake3"] = json!(if preserved {
                Some(digest(prefix))
            } else {
                None
            });
            report["observations"]["user_edit_preserved"] = json!(preserved);
            report["observation_evidence"]["user_edit_preserved"] = json!(preserved);
        }
        if let Some(prior) = &self.prior_ledger {
            let retained = prior
                .steps()
                .filter(|step| {
                    ledger
                        .step(step.definition().id())
                        .is_some_and(|now| now.definition() == step.definition())
                })
                .count();
            let terminal = terminal_attempt_digests(prior);
            let current = terminal_attempt_digests(ledger);
            let kept = terminal
                .iter()
                .filter(|hash| current.contains(hash))
                .count();
            report["observation_evidence"]["prior_step_count"] = json!(prior.steps().count());
            report["observation_evidence"]["retained_prior_definitions_count"] = json!(retained);
            report["observation_evidence"]["prior_terminal_attempts_count"] = json!(terminal.len());
            report["observation_evidence"]["retained_terminal_attempts_count"] = json!(kept);
            report["observation_evidence"]["current_plan_digest"] = json!(plan_digest(ledger));
            report["observation_evidence"]["replan_count"] = json!(ledger.replans().len());
            report["observations"]["decomposition_changed"] = json!(
                ledger.replans().len() > prior.replans().len()
                    && plan_digest(prior) != plan_digest(ledger)
            );
            report["observations"]["prior_history_preserved"] =
                json!(retained == prior.steps().count() && kept == terminal.len());
            report["observation_evidence"]["prior_history_preserved"] =
                report["observations"]["prior_history_preserved"].clone();
            report["observation_evidence"]["decomposition_changed"] =
                report["observations"]["decomposition_changed"].clone();
        }
        if request.case_id == "A10" {
            let task = checked(
                store
                    .load_current_task(project, task_id, &crate::DesktopBoundedReadControl::new())
                    .await,
                "restart_goal",
            )?
            .ok_or_else(|| failure("restart_goal_missing"))?;
            let durable = task.goal_contract().draft().objective().as_str();
            report["observation_evidence"]["version"] = json!(1);
            report["observation_evidence"]["source"] =
                json!("production_journal_and_scoped_intervention");
            report["observation_evidence"]["case_id"] = json!("A10");
            report["observation_evidence"]["original_contract_matches_request"] =
                json!(durable == request.objective);
            report["observation_evidence"]["durable_original_blake3"] =
                json!(digest(durable.as_bytes()));
            report["observation_evidence"]["request_original_blake3"] =
                json!(digest(request.objective.as_bytes()));
            let mut covered = std::collections::BTreeSet::new();
            for criterion in task.goal_contract().draft().acceptance_criteria() {
                if ledger.steps().any(|step| {
                    step.status() == TaskStepStatus::Completed
                        && step
                            .definition()
                            .acceptance_criteria()
                            .contains(&criterion.id())
                        && step
                            .attempts()
                            .last()
                            .and_then(a3_domain::TaskStepAttempt::verification)
                            .is_some_and(a3_domain::StepVerification::passed)
                }) {
                    for word in criterion
                        .statement()
                        .as_str()
                        .split(|character: char| !character.is_ascii_alphanumeric())
                    {
                        if word.len() == 3
                            && word.starts_with('R')
                            && word.as_bytes()[1..].iter().all(u8::is_ascii_digit)
                        {
                            covered.insert(word.to_owned());
                        }
                    }
                }
            }
            report["observation_evidence"]["legacy_criterion_requirement_mentions"] =
                json!(covered);
            report["observation_evidence"]["expected_requirement_mentions"] =
                json!(requirement_ids(&request.objective));
            report["observation_evidence"]["covered_requirement_ids"] = json!([]);
            report["observation_evidence"]["coverage_observation_method"] =
                json!("legacy_contract_has_no_original_span_to_criterion_mapping");
            report["observation_evidence"]["completed_steps_after"] =
                json!(completed_step_ids(ledger).len());
            report["observation_evidence"]["run_memory_input_present"] = json!(false);
            report["observation_evidence"]["packed_run_memory_observed"] = json!(false);
            report["observations"]["compaction_observed"] = json!(false);
            if is_resume()? {
                let saved = checkpoint(directory, request)?;
                let mutations = checked(
                    store.load_agent_mutation_attempts(project, run.id()).await,
                    "restart_mutations",
                )?;
                let current = mutation_digests(&mutations);
                let before = saved["mutation_digests"]
                    .as_array()
                    .ok_or_else(|| failure("checkpoint_mutations"))?;
                let retained = before
                    .iter()
                    .filter(|hash| {
                        hash.as_str()
                            .is_some_and(|hash| current.iter().any(|value| value == hash))
                    })
                    .count();
                report["observation_evidence"]["current_preserved_prior_attempts"] =
                    json!(retained);
                let previous = saved["terminal_attempt_digests"]
                    .as_array()
                    .ok_or_else(|| failure("checkpoint_history"))?;
                let history = terminal_attempt_digests(ledger);
                let history_unchanged = previous.iter().all(|hash| {
                    hash.as_str()
                        .is_some_and(|hash| history.iter().any(|value| value == hash))
                });
                report["observation_evidence"]["prior_terminal_history_unchanged"] =
                    json!(history_unchanged);
                report["observations"]["process_restarted"] =
                    json!(saved["source_pid"] != json!(std::process::id()));
                report["observations"]["durable_state_reopened"] = json!(true);
                let mut changed_paths_preserved = !self.pre_restart_paths.is_empty();
                for (path, expected) in &self.pre_restart_paths {
                    changed_paths_preserved &=
                        file_bytes(directory, path)?.is_some_and(|body| digest(&body) == *expected);
                }
                report["observation_evidence"]["pre_restart_changed_path_count"] =
                    json!(self.pre_restart_paths.len());
                report["observation_evidence"]["pre_restart_changed_paths_unchanged"] =
                    json!(changed_paths_preserved);
                let repeated = self.repeated_applied_patches.saturating_add(
                    self.pending_repeat.map_or(0, |(before, _)| {
                        applied_patches(&mutations).saturating_sub(before)
                    }),
                );
                report["observation_evidence"]["applied_edits_to_pre_restart_changed_paths"] =
                    json!(repeated);
                let keys = applied_mutation_keys(&mutations);
                let previous_keys = saved["applied_mutation_keys"]
                    .as_array()
                    .ok_or_else(|| failure("checkpoint_applied_mutations"))?;
                let mut counts = std::collections::BTreeMap::<&str, usize>::new();
                for key in &keys {
                    *counts.entry(key.as_str()).or_default() += 1;
                }
                let duplicate_count = counts
                    .values()
                    .map(|count| count.saturating_sub(1))
                    .sum::<usize>();
                let repeated_prior = previous_keys
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|key| {
                        counts.get(key).copied().unwrap_or(0).saturating_sub(
                            previous_keys
                                .iter()
                                .filter(|value| value.as_str() == Some(key))
                                .count(),
                        )
                    })
                    .sum::<usize>();
                report["observation_evidence"]["duplicate_mutation_observation_method"] =
                    json!("same_run_action_fingerprint_and_pre_snapshot");
                report["observation_evidence"]["duplicate_applied_mutation_count"] =
                    json!(duplicate_count);
                report["observation_evidence"]["replayed_prior_applied_mutation_count"] =
                    json!(repeated_prior);
                report["observations"]["no_duplicate_mutation"] = json!(
                    history_unchanged
                        && retained == before.len()
                        && duplicate_count == 0
                        && repeated_prior == 0
                );
                report["observation_evidence"]["no_duplicate_mutation"] =
                    report["observations"]["no_duplicate_mutation"].clone();
            }
            report["observations"]["original_requirement_coverage"] = json!(false);
            report["observations"]["more_than_three_steps"] =
                json!(completed_step_ids(ledger).len() > 3);
        }
        Ok(())
    }
}

pub(super) fn after_cleanup(
    directory: &Path,
    metrics: &SharedMetrics,
    report: &mut Value,
) -> WorkerResult<()> {
    if report["restart_requested"] != json!(true) {
        return Ok(());
    }
    if ["conversation_stopped", "agent_stopped", "scheduler_joined"]
        .iter()
        .any(|key| report["cleanup"][key] != json!(true))
    {
        return Err(failure("restart_cleanup_not_complete"));
    }
    let mut checkpoint = read_json(directory, "checkpoint.json", CHECKPOINT_LIMIT)?;
    checkpoint["ready"] = json!(true);
    checkpoint["cleanup"] = report["cleanup"].clone();
    write_json(directory, "checkpoint.json", &checkpoint)?;
    report["metrics"] = metrics_json(metrics);
    report["phase"] = json!("restart_ready");
    report["terminal"] = json!(false);
    report["worker_succeeded"] = json!(true);
    report["harness_status"] = json!("stage_complete");
    Ok(())
}

fn range_json(range: a3_domain::SourceRange) -> Value {
    json!({"start_byte":range.start_byte(),"end_byte":range.end_byte(),
        "start_row":range.start_position().row(),"end_row":range.end_position().row()})
}
pub(super) async fn research_trace(
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
    session_id: AgentSessionId,
) -> WorkerResult<Value> {
    let turns = checked(
        store.list_turns(project, session_id, 32).await,
        "trace_turns",
    )?;
    let mut result = Vec::new();
    let mut source_remaining = 200usize;
    let mut receipt_remaining = 256usize;
    let mut event_remaining = 256usize;
    let mut result_remaining = 256usize;
    let mut truncated = false;
    for detail in turns.turns() {
        let user = detail.turn().user_sequence();
        let mut after = None;
        let mut references = Vec::new();
        loop {
            if source_remaining == 0 {
                truncated = true;
                break;
            }
            let page = checked(
                store
                    .list_sources(project, session_id, user, after, 50)
                    .await,
                "trace_sources",
            )?;
            for source in page.sources().iter().take(source_remaining) {
                references.push(json!({"source_id":source.id().to_string(),"ordinal":source.ordinal(),
                    "path":std::str::from_utf8(source.revision().path().as_bytes()).ok(),"file_blake3":hex(source.revision().content_hash().as_bytes()),
                    "range":source.range().map(range_json),"kind":format!("{:?}",source.kind()),"reason":format!("{:?}",source.reason()),
                    "cited_by_answer":detail.cited_sources().contains(&source.id()),"catalog_membership_is_not_delivery":true}));
            }
            source_remaining = source_remaining.saturating_sub(page.sources().len());
            if !page.has_more() {
                break;
            }
            after = page
                .sources()
                .last()
                .map(a3_application::AskResearchSource::ordinal);
            if after.is_none() {
                return Err(failure("trace_source_cursor"));
            }
        }
        let events=detail.events().iter().take(event_remaining).map(|event|json!({"sequence":event.sequence(),
            "phase":format!("{:?}",event.phase()),"state":format!("{:?}",event.state()),"completeness":format!("{:?}",event.completeness())})).collect::<Vec<_>>();
        truncated |= detail.events().len() > event_remaining;
        event_remaining = event_remaining.saturating_sub(events.len());
        let mut receipts = Vec::new();
        let mut results = Vec::new();
        if let Some(work) = detail.work_state() {
            for receipt in work.accesses().iter().take(receipt_remaining) {
                receipts.push(json!({"question":receipt.question.get(),
                "scope":hex(receipt.scope.as_bytes()),"key":hex(receipt.key.as_bytes()),"kind":format!("{:?}",receipt.kind),"starts":receipt.starts,
                "outcome":receipt.outcome.map(|value|format!("{value:?}"))}));
            }
            truncated |= work.accesses().len() > receipt_remaining;
            receipt_remaining = receipt_remaining.saturating_sub(receipts.len());
            for question in work.questions() {
                if let Some(answer) = question.result() {
                    for source in answer.sources() {
                        if result_remaining == 0 {
                            truncated = true;
                            break;
                        }
                        result_remaining -= 1;
                        results.push(json!({"question":question.id().get(),"kind":format!("{:?}",answer.kind()),"source_id":source.source_id.to_string(),
                    "file_blake3":hex(source.revision.content_hash().as_bytes()),"range":range_json(source.range)}));
                    }
                }
            }
        }
        result.push(json!({"user_sequence":user.get(),"snapshot":detail.turn().snapshot_id().to_string(),
            "source_catalog":references,"events":events,"closed_access_receipts":receipts,"result_sources":results}));
    }
    let mut trace = json!({"version":1,"source":"persisted_research_catalog_results_and_accesses","turns":result,
        "truncated":truncated||turns.turns().len()==32,"catalog_membership_is_not_actual_request_delivery":true,
        "projection_records_omitted":0});
    while checked(serde_json::to_vec_pretty(&trace), "trace_json")?.len() > 64 * 1024 {
        let mut removed = false;
        if let Some(turns) = trace["turns"].as_array_mut() {
            'outer: for key in [
                "source_catalog",
                "result_sources",
                "closed_access_receipts",
                "events",
            ] {
                for turn in turns.iter_mut().rev() {
                    if turn[key]
                        .as_array_mut()
                        .is_some_and(|array| array.pop().is_some())
                    {
                        removed = true;
                        break 'outer;
                    }
                }
            }
        }
        if !removed {
            return Err(failure("trace_header_too_large"));
        }
        trace["truncated"] = json!(true);
        trace["projection_records_omitted"] = json!(
            trace["projection_records_omitted"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(1)
        );
    }
    Ok(trace)
}
fn cited_config_hash(trace: &Value) -> Option<String> {
    trace["turns"].as_array()?.first()?["source_catalog"]
        .as_array()?
        .iter()
        .find(|source| {
            source["path"] == json!("config.py") && source["cited_by_answer"] == json!(true)
        })?["file_blake3"]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn intervention_id_parser_rejects_noncanonical_checkpoint_ids() -> Result<(), Box<dyn Error>> {
    assert_eq!(parse_id(&"ab".repeat(32))?, [0xab; 32]);
    assert!(parse_id(&"AB".repeat(32)).is_err());
    assert!(parse_id("short").is_err());
    Ok(())
}

#[test]
fn semantic_plan_digest_ignores_node_ids_and_storage_order() {
    let a = json!({"outcome":"change price","expected":["source"],"verification_method":"Command"});
    let b = json!({"outcome":"verify caller","expected":["tests"],"verification_method":"Test"});
    assert_eq!(
        semantic_dag_digest(&[(a.clone(), vec![]), (b.clone(), vec![0])], &[0, 1]),
        semantic_dag_digest(&[(b, vec![1]), (a, vec![])], &[1, 0])
    );
}

#[test]
fn semantic_plan_digest_detects_outcome_evidence_and_dependency_changes() {
    let a = json!({"outcome":"change price","expected":["source"],"verification_method":"Command"});
    let b = json!({"outcome":"verify caller","expected":["tests"],"verification_method":"Test"});
    let baseline = semantic_dag_digest(&[(a.clone(), vec![]), (b.clone(), vec![0])], &[0, 1]);
    let mut changed = b.clone();
    changed["outcome"] = json!("verify new report caller");
    assert_ne!(
        baseline,
        semantic_dag_digest(&[(a.clone(), vec![]), (changed, vec![0])], &[0, 1])
    );
    let mut changed = b.clone();
    changed["expected"] = json!(["new report tests"]);
    assert_ne!(
        baseline,
        semantic_dag_digest(&[(a.clone(), vec![]), (changed, vec![0])], &[0, 1])
    );
    assert_ne!(
        baseline,
        semantic_dag_digest(&[(a, vec![]), (b, vec![])], &[0, 1])
    );
}

#[test]
fn intervention_payload_parser_requires_exact_canonical_bytes() -> Result<(), Box<dyn Error>> {
    assert_eq!(parse_payload(&json!("0061ff"))?, vec![0, 97, 255]);
    assert!(parse_payload(&json!("AA")).is_err());
    assert!(parse_payload(&json!("0")).is_err());
    assert!(parse_payload(&json!(null)).is_err());
    Ok(())
}

// Append inside the existing test-only plan22_interventions module.
const LEGACY_OBSERVED_PATHS: [&str; 6] = [
    "subject.py",
    "legacy.py",
    "tests/test_legacy.py",
    "tests/test_target.py",
    "tests/__init__.py",
    "pyproject.toml",
];
const LEGACY_STREAM_LIMIT: u32 = 16 * 1024;
type LegacyFingerprints = std::collections::BTreeMap<&'static str, [u8; 32]>;

#[derive(Debug)]
struct LegacyFailureReceipt {
    process_id_hash: String,
    stream_digest: String,
}

#[derive(Debug)]
enum LegacyFailureStatus {
    Pending,
    InitialStateChanged,
    FirstMutationAuthorized,
    Recorded(LegacyFailureReceipt),
}

/// P0 observation only: never runs a process or creates product verification evidence.
pub(super) struct LegacyFailureObserver {
    initial: LegacyFingerprints,
    status: LegacyFailureStatus,
}

fn legacy_failure_header(text: &str) -> bool {
    text.lines().any(|line| {
        matches!(
            line,
            "FAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure)"
                | "FAIL: test_existing_failure (tests.test_legacy.Legacy.test_existing_failure)"
        )
    })
}

fn legacy_initial_fingerprints_match(
    initial: &LegacyFingerprints,
    live: &LegacyFingerprints,
    published: &LegacyFingerprints,
) -> bool {
    initial.len() == LEGACY_OBSERVED_PATHS.len() && initial == live && initial == published
}

fn legacy_stream_is_complete(
    summary: a3_application::AgentProcessStreamSummary,
    page: &a3_application::AgentProcessLogPage,
) -> bool {
    legacy_retention_is_complete(
        summary.observed_bytes(),
        summary.retained_bytes(),
        summary.source_truncated(),
        summary.redaction().is_some(),
    ) && !page.page_truncated()
        && !page.source_truncated()
        && page.redaction().is_none()
        && page.next_offset().is_none()
        && page.offset() == a3_application::AgentLogPageOffset::START
        && page.text().len() as u64 == summary.retained_bytes()
        && blake3::hash(page.text().as_bytes()).as_bytes() == &summary.digest().as_bytes()
}

fn legacy_retention_is_complete(
    observed: u64,
    retained: u64,
    truncated: bool,
    redacted: bool,
) -> bool {
    !truncated && !redacted && observed == retained && retained <= u64::from(LEGACY_STREAM_LIMIT)
}

impl LegacyFailureObserver {
    pub(super) fn new(directory: &Path) -> WorkerResult<Self> {
        let mut initial = LegacyFingerprints::new();
        for path in LEGACY_OBSERVED_PATHS {
            let bytes = file_bytes(directory, path)?
                .ok_or_else(|| failure("legacy_initial_file_missing"))?;
            initial.insert(path, *blake3::hash(&bytes).as_bytes());
        }
        Ok(Self {
            initial,
            status: LegacyFailureStatus::Pending,
        })
    }

    pub(super) fn before_patch_grant(&mut self) {
        if matches!(self.status, LegacyFailureStatus::Pending) {
            self.status = LegacyFailureStatus::FirstMutationAuthorized;
        }
    }

    fn publish(&self, report: &mut Value) {
        let (status, receipt) = match &self.status {
            LegacyFailureStatus::Pending => ("pending", None),
            LegacyFailureStatus::InitialStateChanged => ("initial_state_changed", None),
            LegacyFailureStatus::FirstMutationAuthorized => ("first_mutation_authorized", None),
            LegacyFailureStatus::Recorded(receipt) => ("recorded", Some(receipt)),
        };
        report["legacy_failure_evidence"] = json!({
            "version":1,"source":"production_initial_unittest_inspection","status":status,
            "failed_exit":receipt.is_some(),"initial_bytes_unchanged":receipt.is_some(),
            "publication_matches":receipt.is_some(),"before_first_mutation":receipt.is_some(),
            "legacy_failure_header":receipt.is_some(),"stream_truncated":false,
            "process_id_hash64":receipt.map(|value|value.process_id_hash.as_str()),
            "stream_digest64":receipt.map(|value|value.stream_digest.as_str())
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe(
        &mut self,
        directory: &Path,
        inspection: &a3_application::AgentInspectionBuffer,
        project: &ProjectIdentity,
        task_id: TaskId,
        run: &AgentRun,
        attempts: &[AgentMutationAttempt],
        published: &a3_domain::PublishedIndex,
        report: &mut Value,
    ) -> WorkerResult<()> {
        if !matches!(self.status, LegacyFailureStatus::Pending) {
            self.publish(report);
            return Ok(());
        }
        // Process attempts themselves are journalled as mutation boundaries; only actual
        // file-capable boundaries close this before-edit observation window.
        if attempts.iter().any(|attempt| {
            matches!(
                attempt.kind(),
                AgentMutationKind::Patch
                    | AgentMutationKind::MachineFile
                    | AgentMutationKind::UnclassifiedLegacy
            ) && attempt.disposition().application_state() != MutationApplicationState::NotApplied
        }) {
            self.status = LegacyFailureStatus::FirstMutationAuthorized;
            self.publish(report);
            return Ok(());
        }
        let mut live = LegacyFingerprints::new();
        let mut indexed = LegacyFingerprints::new();
        for path in LEGACY_OBSERVED_PATHS {
            if let Some(bytes) = file_bytes(directory, path)? {
                live.insert(path, *blake3::hash(&bytes).as_bytes());
            }
            if let Some(revision) = published
                .publication()
                .graph()
                .files()
                .iter()
                .find(|revision| revision.path().as_bytes() == path.as_bytes())
            {
                indexed.insert(path, *revision.content_hash().as_bytes());
            }
        }
        if !legacy_initial_fingerprints_match(&self.initial, &live, &indexed) {
            self.status = LegacyFailureStatus::InitialStateChanged;
            self.publish(report);
            return Ok(());
        }
        let current_snapshot = published.run().snapshot_id();
        if run.current_snapshot_id() != current_snapshot {
            self.publish(report);
            return Ok(());
        }
        let Ok(Some(overview)) = inspection.overview(project, task_id) else {
            self.publish(report);
            return Ok(());
        };
        for process in overview.processes() {
            if process.context().task_id() != task_id
                || process.context().run_id() != run.id()
                || process.context().snapshot_id() != current_snapshot
                || !matches!(
                    process.kind(),
                    a3_application::AgentProcessInspectionKind::Test
                        | a3_application::AgentProcessInspectionKind::Command
                )
                || !matches!(process.termination(), a3_domain::ProcessTermination::Exited(exit) if !exit.success())
            {
                continue;
            }
            let limit = checked(
                a3_application::AgentLogPageLimit::new(LEGACY_STREAM_LIMIT),
                "legacy_log_limit",
            )?;
            let mut captures = Vec::with_capacity(2);
            for (stream, summary) in [
                (a3_domain::ProcessStream::Stdout, process.stdout()),
                (a3_domain::ProcessStream::Stderr, process.stderr()),
            ] {
                let Ok(page) = inspection.load_process_log_page(
                    project,
                    task_id,
                    overview.revision(),
                    process.id(),
                    stream,
                    a3_application::AgentLogPageOffset::START,
                    limit,
                ) else {
                    captures.clear();
                    break;
                };
                if !legacy_stream_is_complete(summary, &page) {
                    captures.clear();
                    break;
                }
                captures.push((summary.digest(), legacy_failure_header(page.text())));
            }
            // Both independently retained streams must be available and complete.
            if captures.len() == 2
                && let Some((stream_digest, _)) = captures.iter().find(|(_, header)| *header)
            {
                self.status = LegacyFailureStatus::Recorded(LegacyFailureReceipt {
                    process_id_hash: digest(&process.id().as_bytes()),
                    stream_digest: hex(&stream_digest.as_bytes()),
                });
                break;
            }
        }
        self.publish(report);
        Ok(())
    }
}

#[test]
fn legacy_failure_header_accepts_only_complete_exact_unittest_failure_headers() {
    assert!(legacy_failure_header(
        "FF\r\nFAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure)\r\n"
    ));
    assert!(legacy_failure_header(
        "FAIL: test_existing_failure (tests.test_legacy.Legacy.test_existing_failure)\n"
    ));
    for text in [
        "test_legacy.Legacy.test_existing_failure",
        "FAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure_extra)",
        " FAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure)",
        "source says FAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure)",
        "ERROR: test_existing_failure (test_legacy.Legacy.test_existing_failure)",
        "FAIL: test_existing_failure (fake.test_legacy.Legacy.test_existing_failure)",
        "FAIL: test_existing_failure (test_legacy.Legacy.test_existing_failure) extra",
    ] {
        assert!(!legacy_failure_header(text));
    }
}

#[test]
fn legacy_initial_fingerprint_mismatch_cannot_be_a_preexisting_failure() {
    let initial = LEGACY_OBSERVED_PATHS
        .into_iter()
        .map(|path| (path, [1; 32]))
        .collect::<LegacyFingerprints>();
    assert!(legacy_initial_fingerprints_match(
        &initial, &initial, &initial
    ));
    let mut changed = initial.clone();
    changed.insert("subject.py", [2; 32]);
    assert!(!legacy_initial_fingerprints_match(
        &initial, &changed, &initial
    ));
    assert!(!legacy_initial_fingerprints_match(
        &initial, &initial, &changed
    ));
    changed = initial.clone();
    changed.remove("legacy.py");
    assert!(!legacy_initial_fingerprints_match(
        &initial, &changed, &initial
    ));
}

#[test]
fn legacy_unrecorded_status_does_not_publish_success_receipt() {
    let mut observer = LegacyFailureObserver {
        initial: LegacyFingerprints::new(),
        status: LegacyFailureStatus::Pending,
    };
    let mut report = json!({});
    observer.publish(&mut report);
    assert_eq!(
        report["legacy_failure_evidence"]["failed_exit"],
        json!(false)
    );
    assert_eq!(
        report["legacy_failure_evidence"]["process_id_hash64"],
        Value::Null
    );
    observer.before_patch_grant();
    observer.publish(&mut report);
    assert_eq!(
        report["legacy_failure_evidence"]["status"],
        json!("first_mutation_authorized")
    );
    assert_eq!(
        report["legacy_failure_evidence"]["before_first_mutation"],
        json!(false)
    );
}

#[test]
fn legacy_truncated_or_redacted_stream_cannot_prove_the_initial_failure() {
    assert!(legacy_retention_is_complete(42, 42, false, false));
    assert!(legacy_retention_is_complete(0, 0, false, false));
    assert!(!legacy_retention_is_complete(42, 42, true, false));
    assert!(!legacy_retention_is_complete(42, 42, false, true));
    assert!(!legacy_retention_is_complete(43, 42, false, false));
    assert!(!legacy_retention_is_complete(
        u64::from(LEGACY_STREAM_LIMIT) + 1,
        u64::from(LEGACY_STREAM_LIMIT) + 1,
        false,
        false
    ));
}

// Append inside the current test-only plan22_interventions module.
const ORDER_MODULES: [&str; 4] = [
    "step_one.py",
    "step_two.py",
    "step_three.py",
    "step_four.py",
];
const ORDER_PROTECTED: [&str; 7] = [
    "pipeline.py",
    "pyproject.toml",
    "tests/__init__.py",
    "tests/test_step_one.py",
    "tests/test_step_two.py",
    "tests/test_step_three.py",
    "tests/test_step_four.py",
];
const ORDER_EVENT_LIMIT: usize = 64;
type OrderHashes = std::collections::BTreeMap<String, String>;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum OrderActionKind {
    Patch,
    MachineFile,
    Check,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderPending {
    kind: OrderActionKind,
    run_id: String,
    step_id: String,
    verification_spec_id: String,
    snapshot_id: String,
    approval_request_id: String,
    action_fingerprint: String,
    requested_at: u64,
    before_blake3: OrderHashes,
    expected_after: OrderHashes,
    paths: Vec<String>,
    argv: Vec<String>,
    known_attempts: std::collections::BTreeMap<String, bool>,
    known_inspections: std::collections::BTreeSet<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderState {
    version: u8,
    task_id: Option<String>,
    run_id: Option<String>,
    initial_blake3: OrderHashes,
    last_blake3: OrderHashes,
    protected_blake3: OrderHashes,
    events: Vec<Value>,
    pending: Option<OrderPending>,
    invalid_reason: Option<String>,
    truncated: bool,
}

/// Observes the requested R25 delivery order; never supplies work or authorizes effects.
pub(super) struct A10OrderObserver {
    state: OrderState,
}

fn order_hashes(directory: &Path, paths: &[&str]) -> WorkerResult<OrderHashes> {
    paths
        .iter()
        .map(|path| {
            let bytes =
                file_bytes(directory, path)?.ok_or_else(|| failure("order_source_missing"))?;
            Ok(((*path).to_owned(), digest(&bytes)))
        })
        .collect()
}
fn order_published_hashes(published: &a3_domain::PublishedIndex, paths: &[&str]) -> OrderHashes {
    paths
        .iter()
        .filter_map(|path| {
            published
                .publication()
                .graph()
                .files()
                .iter()
                .find(|revision| revision.path().as_bytes() == path.as_bytes())
                .map(|revision| ((*path).to_owned(), hex(revision.content_hash().as_bytes())))
        })
        .collect()
}
// An unavailable/deleted result is missing evidence, not permission to stop or repair
// the measured product. Initial fixture validation above remains strict.
fn order_observed_hashes(directory: &Path, paths: &[&str]) -> OrderHashes {
    paths
        .iter()
        .filter_map(|path| {
            file_bytes(directory, path)
                .ok()
                .flatten()
                .map(|bytes| ((*path).to_owned(), digest(&bytes)))
        })
        .collect()
}
fn order_attempt_key(attempt: &AgentMutationAttempt) -> String {
    format!(
        "{}:{}",
        hex(attempt.tool_attempt().tool_run_id().as_bytes()),
        attempt.tool_attempt().attempt().get()
    )
}
fn order_check_paths(argv: &[String]) -> Option<Vec<String>> {
    let full = ["python", "-B", "-m", "unittest", "discover", "-s", "tests"];
    if argv.iter().map(String::as_str).eq(full) {
        return Some(
            ORDER_MODULES
                .iter()
                .map(|path| (*path).to_owned())
                .collect(),
        );
    }
    if argv.len() == 9 && argv[..7].iter().map(String::as_str).eq(full) && argv[7] == "-p" {
        return ORDER_MODULES
            .iter()
            .find(|path| argv[8] == format!("test_{path}"))
            .map(|path| vec![(*path).to_owned()]);
    }
    None
}
fn order_hash_shape(hashes: &OrderHashes) -> bool {
    hashes.len() == 4
        && ORDER_MODULES
            .iter()
            .all(|path| hashes.get(*path).is_some_and(|hash| parse_id(hash).is_ok()))
}
fn order_event_hashes(event: &Value, key: &str) -> Option<OrderHashes> {
    serde_json::from_value(event[key].clone()).ok()
}

// The fold is independent of Completed steps, text claims, approval counts, and TaskStep IDs.
fn order_trace_satisfies(
    events: &[Value],
    initial: &OrderHashes,
    final_hashes: &OrderHashes,
    require_complete: bool,
) -> bool {
    if !order_hash_shape(initial)
        || !order_hash_shape(final_hashes)
        || events.len() > ORDER_EVENT_LIMIT
    {
        return false;
    }
    let mut current = initial.clone();
    let mut mutated = [false; 4];
    let mut passed: [Option<String>; 4] = Default::default();
    let mut next = 0usize;
    let mut current_snapshot: Option<String> = None;
    for (ordinal, event) in events.iter().enumerate() {
        if event["seq"] != json!(ordinal + 1) || event["actual_bound"] != json!(true) {
            return false;
        }
        match event["kind"].as_str() {
            Some("mutation") => {
                let Some(pre) = event["pre_snapshot_id"]
                    .as_str()
                    .filter(|id| parse_id(id).is_ok())
                else {
                    return false;
                };
                let Some(post) = event["post_snapshot_id"]
                    .as_str()
                    .filter(|id| parse_id(id).is_ok())
                else {
                    return false;
                };
                if event["snapshot_id"].as_str() != Some(pre)
                    || current_snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot != pre)
                {
                    return false;
                }
                current_snapshot = Some(post.to_owned());
                let Some(before) = order_event_hashes(event, "before_blake3") else {
                    return false;
                };
                let Some(after) = order_event_hashes(event, "after_blake3") else {
                    return false;
                };
                if before != current
                    || !order_hash_shape(&after)
                    || order_event_hashes(event, "source_blake3") != Some(after.clone())
                {
                    return false;
                }
                let Some(paths) = event["affected_paths"].as_array() else {
                    return false;
                };
                if paths.is_empty()
                    || paths.iter().any(|path| {
                        !path
                            .as_str()
                            .is_some_and(|path| ORDER_MODULES.contains(&path))
                    })
                {
                    return false;
                }
                match event["application_state"].as_str() {
                    Some("applied") => {
                        for (index, path) in ORDER_MODULES.iter().enumerate() {
                            if before.get(*path) == after.get(*path) {
                                continue;
                            }
                            if !paths.iter().any(|value| value.as_str() == Some(*path)) {
                                return false;
                            }
                            if !mutated[index] {
                                if index != next
                                    || (index > 0
                                        && passed[index - 1].as_ref()
                                            != before.get(ORDER_MODULES[index - 1]))
                                {
                                    return false;
                                }
                                mutated[index] = true;
                                next += 1;
                            }
                            passed[index] = None;
                        }
                    }
                    Some("not_applied") if before == after => {}
                    _ => return false,
                }
                current = after;
            }
            Some("check") => {
                let Some(snapshot) = event["snapshot_id"]
                    .as_str()
                    .filter(|id| parse_id(id).is_ok())
                else {
                    return false;
                };
                if current_snapshot
                    .as_ref()
                    .is_some_and(|current| current != snapshot)
                {
                    return false;
                }
                current_snapshot = Some(snapshot.to_owned());
                if order_event_hashes(event, "source_blake3") != Some(current.clone()) {
                    return false;
                }
                let Ok(argv) = serde_json::from_value::<Vec<String>>(event["argv"].clone()) else {
                    return false;
                };
                let Some(paths) = order_check_paths(&argv) else {
                    return false;
                };
                if event["checked_paths"] != json!(paths) || event["cwd"] != json!(".") {
                    return false;
                }
                for (index, path) in ORDER_MODULES.iter().enumerate() {
                    if paths.iter().any(|value| value == path) {
                        passed[index] = None;
                    }
                }
                if event["exit_code"] == json!(0)
                    && event["streams_complete"] == json!(true)
                    && event["protected_bytes_unchanged"] == json!(true)
                {
                    for (index, path) in ORDER_MODULES.iter().enumerate() {
                        if mutated[index] && paths.iter().any(|value| value == path) {
                            passed[index] = current.get(*path).cloned();
                        }
                    }
                }
            }
            _ => return false,
        }
    }
    current == *final_hashes
        && (!require_complete
            || (mutated.iter().all(|value| *value)
                && ORDER_MODULES
                    .iter()
                    .enumerate()
                    .all(|(index, path)| passed[index].as_ref() == current.get(*path))))
}

impl A10OrderObserver {
    pub(super) fn new(directory: &Path, request: &Request) -> WorkerResult<Self> {
        let initial = order_hashes(directory, &ORDER_MODULES)?;
        let protected = order_hashes(directory, &ORDER_PROTECTED)?;
        let fallback = OrderState {
            version: 1,
            task_id: None,
            run_id: None,
            initial_blake3: initial.clone(),
            last_blake3: initial,
            protected_blake3: protected,
            events: Vec::new(),
            pending: None,
            invalid_reason: None,
            truncated: false,
        };
        if !is_resume()? {
            return Ok(Self { state: fallback });
        }
        let saved = checkpoint(directory, request)?;
        let Ok(state) = serde_json::from_value::<OrderState>(saved["order_state"].clone()) else {
            return Ok(Self {
                state: OrderState {
                    invalid_reason: Some("invalid_restored_state".to_owned()),
                    ..fallback
                },
            });
        };
        // Preserve genuine but unsuccessful history too. A wrong-order trace is a valid
        // observation to restore; the final independent fold will still reject it.
        let valid = state.version == 1
            && order_hash_shape(&state.initial_blake3)
            && order_hash_shape(&state.last_blake3)
            && state.events.len() <= ORDER_EVENT_LIMIT
            && state
                .events
                .iter()
                .enumerate()
                .all(|(index, event)| event["seq"] == json!(index + 1))
            && state
                .task_id
                .as_ref()
                .is_some_and(|id| parse_id(id).is_ok())
            && state.run_id.as_ref().is_some_and(|id| parse_id(id).is_ok());
        Ok(Self {
            state: if valid {
                state
            } else {
                OrderState {
                    invalid_reason: Some("invalid_restored_state".to_owned()),
                    ..fallback
                }
            },
        })
    }
    fn invalidate(&mut self, reason: &str) {
        if self.state.invalid_reason.is_none() {
            self.state.invalid_reason = Some(reason.to_owned());
        }
    }
    fn identity(&mut self, task_id: TaskId, run: &AgentRun) {
        let task = hex(task_id.as_bytes());
        let run = hex(run.id().as_bytes());
        if self.state.task_id.as_ref().is_some_and(|old| old != &task)
            || self.state.run_id.as_ref().is_some_and(|old| old != &run)
        {
            self.invalidate("identity_changed");
        }
        self.state.task_id = Some(task);
        self.state.run_id = Some(run);
    }
    fn publish(&self, live: &OrderHashes, report: &mut Value) -> WorkerResult<()> {
        let projected_invalid = self.state.invalid_reason.as_deref().or_else(|| {
            self.state
                .pending
                .as_ref()
                .map(|_| "unresolved_prior_action")
        });
        report["order_evidence"] = json!({"version":1,"source":"production_approval_mutation_and_process_inspection","case_id":"A10",
            "task_id":self.state.task_id,"run_id":self.state.run_id,"module_paths":ORDER_MODULES,"initial_blake3":self.state.initial_blake3,
            "final_blake3":live,"events":self.state.events,"truncated":self.state.truncated,"invalid_reason":projected_invalid});
        report["order_state"] = checked(serde_json::to_value(&self.state), "order_state_json")?;
        report["observations"]["ordered_verified_milestones"] = json!(
            self.state.invalid_reason.is_none()
                && !self.state.truncated
                && self.state.pending.is_none()
                && order_trace_satisfies(
                    &self.state.events,
                    &self.state.initial_blake3,
                    live,
                    true
                )
        );
        Ok(())
    }
    fn push(&mut self, event: Value) {
        if self.state.events.len() == ORDER_EVENT_LIMIT {
            self.state.truncated = true;
            self.invalidate("event_capacity");
        } else {
            self.state.events.push(event);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn before_approval(
        &mut self,
        directory: &Path,
        inspection: &a3_application::AgentInspectionBuffer,
        project: &ProjectIdentity,
        task_id: TaskId,
        run: &AgentRun,
        presentation: &a3_application::AgentApprovalPresentation,
        attempts: &[AgentMutationAttempt],
        published: &a3_domain::PublishedIndex,
        report: &mut Value,
    ) -> WorkerResult<()> {
        self.observe(
            directory, inspection, project, task_id, run, attempts, published, report,
        )?;
        if self.state.pending.is_some() {
            self.invalidate("unresolved_prior_action");
            self.state.pending = None;
        }
        let context = presentation.context();
        let live = order_observed_hashes(directory, &ORDER_MODULES);
        if context.task_id() != task_id
            || context.run_id() != run.id()
            || context.snapshot_id() != published.run().snapshot_id()
            || context.snapshot_id() != run.current_snapshot_id()
            || order_published_hashes(published, &ORDER_MODULES) != live
        {
            self.invalidate("approval_anchor_mismatch");
            return self.publish(&live, report);
        }
        let overview = inspection.overview(project, task_id).ok().flatten();
        let mut pending = OrderPending {
            kind: OrderActionKind::Patch,
            run_id: hex(run.id().as_bytes()),
            step_id: hex(context.step_id().as_bytes()),
            verification_spec_id: hex(context.verification_spec_id().as_bytes()),
            snapshot_id: hex(context.snapshot_id().as_bytes()),
            approval_request_id: hex(presentation.request_id().as_bytes()),
            action_fingerprint: hex(presentation.action_fingerprint().as_bytes()),
            requested_at: presentation.requested_at().unix_millis(),
            before_blake3: live.clone(),
            expected_after: live.clone(),
            paths: Vec::new(),
            argv: Vec::new(),
            known_attempts: attempts
                .iter()
                .map(|attempt| {
                    (
                        order_attempt_key(attempt),
                        attempt.tool_attempt().status().is_terminal(),
                    )
                })
                .collect(),
            known_inspections: overview
                .as_ref()
                .map(|overview| {
                    overview
                        .processes()
                        .iter()
                        .map(|process| hex(&process.id().as_bytes()))
                        .collect()
                })
                .unwrap_or_default(),
        };
        match presentation.action() {
            AgentApprovalAction::Patch(patch) => {
                let Some(preview) = overview
                    .as_ref()
                    .and_then(|overview| overview.patch())
                    .filter(|preview| preview.context() == context)
                else {
                    self.invalidate("approval_projection_unavailable");
                    return self.publish(&live, report);
                };
                for file in patch.files() {
                    for path in file.source_path().into_iter().chain(file.target_path()) {
                        let path = std::str::from_utf8(path.as_bytes())
                            .ok()
                            .filter(|path| ORDER_MODULES.contains(path));
                        let Some(path) = path else {
                            self.invalidate("approval_projection_unavailable");
                            return self.publish(&live, report);
                        };
                        if !pending.paths.iter().any(|value| value == path) {
                            pending.paths.push(path.to_owned());
                        }
                    }
                }
                for path in &pending.paths {
                    let Some(file) = preview.files().iter().find(|file| {
                        file.target_path()
                            .is_some_and(|target| target.as_bytes() == path.as_bytes())
                    }) else {
                        self.invalidate("approval_projection_unavailable");
                        return self.publish(&live, report);
                    };
                    let Some(after) = file.after() else {
                        self.invalidate("approval_projection_unavailable");
                        return self.publish(&live, report);
                    };
                    pending
                        .expected_after
                        .insert(path.clone(), hex(after.content_hash().as_bytes()));
                }
                pending.paths.sort();
            }
            AgentApprovalAction::Process(process) => {
                pending.argv = std::iter::once(process.executable().to_owned())
                    .chain(process.arguments().iter().cloned())
                    .collect();
                let Some(paths) = order_check_paths(&pending.argv) else {
                    return self.publish(&live, report);
                };
                if !matches!(
                    process.working_directory(),
                    AgentApprovalWorkingDirectory::Root
                ) {
                    return self.publish(&live, report);
                }
                pending.kind = OrderActionKind::Check;
                pending.paths = paths;
            }
            AgentApprovalAction::Machine { scope, operation }
                if scope.kind() == AgentMutationKind::MachineFile
                    && *operation != a3_domain::PathPolicyOperation::Read =>
            {
                let Some(target) = fs::canonicalize(scope.target()).ok() else {
                    self.invalidate("approval_projection_unavailable");
                    return self.publish(&live, report);
                };
                let root = regular_child(directory, "worktree", true)?;
                let Some(path) = ORDER_MODULES
                    .iter()
                    .find(|path| fs::canonicalize(root.join(path)).ok().as_ref() == Some(&target))
                else {
                    self.invalidate("approval_projection_unavailable");
                    return self.publish(&live, report);
                };
                let Some(after) = scope.proposed() else {
                    self.invalidate("approval_projection_unavailable");
                    return self.publish(&live, report);
                };
                pending.kind = OrderActionKind::MachineFile;
                pending.paths = vec![(*path).to_owned()];
                pending
                    .expected_after
                    .insert((*path).to_owned(), hex(after.as_bytes()));
            }
            _ => return self.publish(&live, report),
        }
        self.state.pending = Some(pending);
        self.publish(&live, report)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe(
        &mut self,
        directory: &Path,
        inspection: &a3_application::AgentInspectionBuffer,
        project: &ProjectIdentity,
        task_id: TaskId,
        run: &AgentRun,
        attempts: &[AgentMutationAttempt],
        published: &a3_domain::PublishedIndex,
        report: &mut Value,
    ) -> WorkerResult<()> {
        self.identity(task_id, run);
        let live = order_observed_hashes(directory, &ORDER_MODULES);
        let Some(pending) = self.state.pending.clone() else {
            if live != self.state.last_blake3 {
                self.invalidate("unobserved_file_change");
            }
            return self.publish(&live, report);
        };
        let candidates = attempts
            .iter()
            .filter(|attempt| {
                let tool = attempt.tool_attempt();
                let kind_matches = match pending.kind {
                    OrderActionKind::Patch => attempt.kind() == AgentMutationKind::Patch,
                    OrderActionKind::MachineFile => {
                        attempt.kind() == AgentMutationKind::MachineFile
                    }
                    OrderActionKind::Check => matches!(
                        attempt.kind(),
                        AgentMutationKind::Process | AgentMutationKind::MachineProcess
                    ),
                };
                kind_matches
                    && hex(tool.run_id().as_bytes()) == pending.run_id
                    && hex(tool.snapshot_id().as_bytes()) == pending.snapshot_id
                    && tool.started_at().unix_millis() >= pending.requested_at
                    && tool.status().is_terminal()
                    && pending.known_attempts.get(&order_attempt_key(attempt)) != Some(&true)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return self.publish(&live, report);
        }
        if candidates.len() != 1 {
            self.invalidate("ambiguous_actual_attempt");
            self.state.pending = None;
            return self.publish(&live, report);
        }
        let attempt = candidates[0];
        let tool = attempt.tool_attempt();
        if order_published_hashes(published, &ORDER_MODULES) != live {
            return self.publish(&live, report);
        }
        let mut event = json!({"seq":self.state.events.len()+1,"run_id":pending.run_id,"step_id":pending.step_id,"tool_run_id":hex(tool.tool_run_id().as_bytes()),"attempt":tool.attempt().get(),
            "snapshot_id":hex(tool.snapshot_id().as_bytes()),"approval_request_id":pending.approval_request_id,"action_fingerprint":pending.action_fingerprint,"source_blake3":live,"actual_bound":true});
        match pending.kind {
            OrderActionKind::Patch | OrderActionKind::MachineFile => {
                let application = match attempt.disposition().application_state() {
                    MutationApplicationState::Applied => "applied",
                    MutationApplicationState::NotApplied => "not_applied",
                    MutationApplicationState::Unknown => "unknown",
                };
                if application == "unknown" {
                    self.invalidate("unknown_file_effect");
                }
                if (application == "applied" && live != pending.expected_after)
                    || (application == "not_applied" && live != pending.before_blake3)
                {
                    self.invalidate("actual_hash_mismatch");
                    event["actual_bound"] = json!(false);
                }
                event["kind"] = json!("mutation");
                event["affected_paths"] = json!(pending.paths);
                event["before_blake3"] = json!(pending.before_blake3);
                event["after_blake3"] = json!(live);
                event["application_state"] = json!(application);
                event["pre_snapshot_id"] = json!(hex(tool.snapshot_id().as_bytes()));
                event["post_snapshot_id"] = json!(hex(published.run().snapshot_id().as_bytes()));
                self.state.last_blake3 = live.clone();
            }
            OrderActionKind::Check => {
                if live != pending.before_blake3 {
                    self.invalidate("actual_hash_mismatch");
                    return self.publish(&live, report);
                }
                let Some(overview) = inspection.overview(project, task_id).ok().flatten() else {
                    return self.publish(&live, report);
                };
                let results = overview
                    .processes()
                    .iter()
                    .filter(|process| {
                        let context = process.context();
                        !pending
                            .known_inspections
                            .contains(&hex(&process.id().as_bytes()))
                            && hex(context.run_id().as_bytes()) == pending.run_id
                            && hex(context.step_id().as_bytes()) == pending.step_id
                            && hex(context.verification_spec_id().as_bytes())
                                == pending.verification_spec_id
                            && hex(context.snapshot_id().as_bytes()) == pending.snapshot_id
                    })
                    .collect::<Vec<_>>();
                if results.is_empty() {
                    return self.publish(&live, report);
                }
                if results.len() != 1 {
                    self.invalidate("ambiguous_process_result");
                    self.state.pending = None;
                    return self.publish(&live, report);
                }
                let process = results[0];
                if tool.status() != a3_domain::AgentToolAttemptStatus::Succeeded
                    || attempt.disposition().application_state()
                        != MutationApplicationState::Applied
                {
                    event["actual_bound"] = json!(false);
                }
                let complete = [process.stdout(), process.stderr()].iter().all(|stream| {
                    !stream.source_truncated()
                        && stream.redaction().is_none()
                        && stream.observed_bytes() == stream.retained_bytes()
                });
                let protected = order_observed_hashes(directory, &ORDER_PROTECTED)
                    == self.state.protected_blake3
                    && order_published_hashes(published, &ORDER_PROTECTED)
                        == self.state.protected_blake3;
                if !protected {
                    self.invalidate("protected_bytes_changed");
                }
                event["kind"] = json!("check");
                event["checked_paths"] = json!(pending.paths);
                event["argv"] = json!(pending.argv);
                event["cwd"] = json!(".");
                event["exit_code"] = json!(match process.termination() {
                    a3_domain::ProcessTermination::Exited(exit) => exit.code(),
                    _ => None,
                });
                event["process_id_hash64"] = json!(digest(&process.id().as_bytes()));
                event["streams_complete"] = json!(complete);
                event["protected_bytes_unchanged"] = json!(protected);
            }
        }
        self.push(event);
        self.state.pending = None;
        self.publish(&live, report)
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    fn hashes() -> OrderHashes {
        ORDER_MODULES
            .iter()
            .enumerate()
            .map(|(index, path)| ((*path).to_owned(), hex(&[index as u8 + 1; 32])))
            .collect()
    }
    fn mutation(seq: usize, before: &OrderHashes, changed: &[usize], pre: u8, post: u8) -> Value {
        let mut after = before.clone();
        for index in changed {
            after.insert(ORDER_MODULES[*index].to_owned(), hex(&[seq as u8 + 10; 32]));
        }
        json!({"seq":seq,"kind":"mutation","run_id":hex(&[1;32]),"step_id":hex(&[seq as u8;32]),"tool_run_id":hex(&[seq as u8+40;32]),"attempt":1,
            "snapshot_id":hex(&[pre;32]),"approval_request_id":hex(&[seq as u8+80;32]),"action_fingerprint":hex(&[seq as u8+120;32]),"source_blake3":after,"actual_bound":true,
            "affected_paths":changed.iter().map(|index|ORDER_MODULES[*index]).collect::<Vec<_>>(),"before_blake3":before,"after_blake3":after,"application_state":"applied",
            "pre_snapshot_id":hex(&[pre;32]),"post_snapshot_id":hex(&[post;32])})
    }
    fn check(seq: usize, current: &OrderHashes, module: usize, snapshot: u8) -> Value {
        json!({"seq":seq,"kind":"check","run_id":hex(&[1;32]),"step_id":hex(&[seq as u8;32]),"tool_run_id":hex(&[seq as u8+40;32]),"attempt":1,
            "snapshot_id":hex(&[snapshot;32]),"approval_request_id":hex(&[seq as u8+80;32]),"action_fingerprint":hex(&[seq as u8+120;32]),"source_blake3":current,"actual_bound":true,
            "checked_paths":[ORDER_MODULES[module]],"argv":["python","-B","-m","unittest","discover","-s","tests","-p",format!("test_{}",ORDER_MODULES[module])],
            "cwd":".","exit_code":0,"process_id_hash64":hex(&[seq as u8+160;32]),"streams_complete":true,"protected_bytes_unchanged":true})
    }
    fn valid_trace() -> (OrderHashes, Vec<Value>, OrderHashes) {
        let initial = hashes();
        let mut current = initial.clone();
        let mut events = Vec::new();
        for index in 0..4 {
            let event = mutation(
                events.len() + 1,
                &current,
                &[index],
                index as u8 + 1,
                index as u8 + 2,
            );
            current = order_event_hashes(&event, "after_blake3").unwrap_or_default();
            events.push(event);
            events.push(check(events.len() + 1, &current, index, index as u8 + 2));
        }
        (initial, events, current)
    }
    #[test]
    fn actual_mutation_then_fresh_check_allows_the_next_module() {
        let (initial, events, current) = valid_trace();
        assert!(order_trace_satisfies(&events, &initial, &current, true));
    }
    #[test]
    fn one_big_patch_and_four_later_checks_cannot_replace_staged_delivery() {
        let initial = hashes();
        let event = mutation(1, &initial, &[0, 1, 2, 3], 1, 2);
        let current = order_event_hashes(&event, "after_blake3").unwrap_or_default();
        let mut events = vec![event];
        for index in 0..4 {
            events.push(check(events.len() + 1, &current, index, 2));
        }
        assert!(!order_trace_satisfies(&events, &initial, &current, true));
    }
    #[test]
    fn next_module_edit_before_prior_current_pass_is_rejected() {
        let initial = hashes();
        let first = mutation(1, &initial, &[0], 1, 2);
        let current = order_event_hashes(&first, "after_blake3").unwrap_or_default();
        let next = mutation(2, &current, &[1], 2, 3);
        let final_hashes = order_event_hashes(&next, "after_blake3").unwrap_or_default();
        assert!(!order_trace_satisfies(
            &[first, next],
            &initial,
            &final_hashes,
            false
        ));
        let wrong_first = mutation(1, &initial, &[1], 1, 2);
        let wrong_hashes = order_event_hashes(&wrong_first, "after_blake3").unwrap_or_default();
        assert!(!order_trace_satisfies(
            &[wrong_first],
            &initial,
            &wrong_hashes,
            false
        ));
    }
    #[test]
    fn later_source_change_invalidates_the_old_pass_until_rechecked() {
        let (initial, mut events, current) = valid_trace();
        let changed = mutation(9, &current, &[0], 5, 6);
        let changed_hashes = order_event_hashes(&changed, "after_blake3").unwrap_or_default();
        events.push(changed);
        assert!(!order_trace_satisfies(
            &events,
            &initial,
            &changed_hashes,
            true
        ));
        events.push(check(10, &changed_hashes, 0, 6));
        assert!(order_trace_satisfies(
            &events,
            &initial,
            &changed_hashes,
            true
        ));
        let mut unseen = changed_hashes.clone();
        unseen.insert(ORDER_MODULES[2].to_owned(), hex(&[99; 32]));
        assert!(!order_trace_satisfies(&events, &initial, &unseen, true));
    }
    #[test]
    fn failed_incomplete_or_wrong_snapshot_check_cannot_keep_an_old_pass() {
        let (initial, events, current) = valid_trace();
        for (field, value) in [
            ("exit_code", json!(1)),
            ("streams_complete", json!(false)),
            ("protected_bytes_unchanged", json!(false)),
            ("snapshot_id", json!(hex(&[99; 32]))),
        ] {
            let mut failed = check(9, &current, 3, 5);
            failed[field] = value;
            let mut all = events.clone();
            all.push(failed);
            assert!(!order_trace_satisfies(&all, &initial, &current, true));
        }
    }
    #[test]
    fn persisted_order_state_retains_exact_observed_prefix_across_restart()
    -> Result<(), Box<dyn Error>> {
        let (initial, events, current) = valid_trace();
        let prefix = events[..4].to_vec();
        let state = OrderState {
            version: 1,
            task_id: Some(hex(&[2; 32])),
            run_id: Some(hex(&[1; 32])),
            initial_blake3: initial.clone(),
            last_blake3: order_event_hashes(&prefix[3], "source_blake3").unwrap_or_default(),
            protected_blake3: Default::default(),
            events: prefix.clone(),
            pending: None,
            invalid_reason: None,
            truncated: false,
        };
        let mut reopened: OrderState = serde_json::from_slice(&serde_json::to_vec(&state)?)?;
        assert_eq!(reopened.events, prefix);
        reopened.events.extend_from_slice(&events[4..]);
        assert_eq!(&reopened.events[..4], prefix.as_slice());
        assert!(order_trace_satisfies(
            &reopened.events,
            &initial,
            &current,
            true
        ));
        Ok(())
    }
    #[test]
    fn exact_unittest_argv_and_failed_or_noop_actions_provide_no_false_credit() {
        let argv = vec![
            "python",
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            "tests",
            "-p",
            "test_step_one.py",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        assert_eq!(
            order_check_paths(&argv),
            Some(vec!["step_one.py".to_owned()])
        );
        let mut changed = argv;
        changed.push("-k".to_owned());
        assert_eq!(order_check_paths(&changed), None);
        let initial = hashes();
        let mut event = mutation(1, &initial, &[0], 1, 1);
        event["after_blake3"] = json!(initial);
        event["source_blake3"] = json!(initial);
        event["application_state"] = json!("not_applied");
        assert!(order_trace_satisfies(
            &[event.clone()],
            &initial,
            &initial,
            false
        ));
        assert!(!order_trace_satisfies(
            &[event.clone()],
            &initial,
            &initial,
            true
        ));
        event["application_state"] = json!("applied");
        assert!(!order_trace_satisfies(&[event], &initial, &initial, true));
    }
}

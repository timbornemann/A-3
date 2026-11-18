//! Opt-in, one-attempt P0 worker. The supervisor owns fixtures, oracles and native exit codes.
//! No scripted plan/model output, saved user settings, Full machine, or libSQL retry helper.
use super::*;
use crate::agent_run_manager::AgentRunManager;
use crate::agent_runtime_recovery::CoreAgentRuntimeRecovery;
use crate::production_agent_run_executor::{ProductionAgentRunExecutor, ProductionAgentRunPorts};
use a3_application::{
    AgentApprovalAction, AgentApprovalControlAction, AgentApprovalControlMetadata,
    AgentApprovalControlOutcome, AgentApprovalControlResult, AgentApprovalFileOperation,
    AgentApprovalLoadResult, AgentApprovalNetworkScope, AgentApprovalWorkingDirectory,
    AgentRecoveryStore, ControlAgentApproval, ControlAgentTaskRun, DiscoverProviderModels,
    GetAgentApprovalCenter, IndexPersistenceControl, IndexPersistenceControlError,
    InspectAgentTaskRecovery, JobClock, JobScheduler, JobSchedulerConfig, JobTimestamp,
    KnowledgeStore, ModelCancellationFuture, ModelCapabilityProbeRequest, ModelOperationControl,
    ModelProvider, ModelProviderFuture, ModelProviderRequest, ModelRequestTimeout,
    ProbeModelProfile, ProviderEvent, RefreshRepositoryIndex, RepositoryChangeBatch,
    RepositoryIndexControl, RepositoryIndexControlError, RepositoryRescanReason, ShutdownMode,
    VerificationEvidenceStore,
};
use a3_domain::{
    AgentControllerState, AgentRunTimestamp, ApprovalId, ModelContextLimit, ModelId,
    ModelOutputLimit, ModelParallelismLimit, ModelProfile, ModelProfileSettings,
    ModelPromptSchemaGrounding, ModelSamplingProfile, ModelStopSequences,
    ModelStructuredOutputCapability, ModelTemperature, ModelTokenCountingStrategy, ModelTopP,
    ProcessExecutionMode, RepositoryPath, RunEventId,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use futures::StreamExt;
use serde_json::{Value, json};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_REPORT_BYTES: usize = 512 * 1024;
const MAX_ANSWER_BYTES: usize = 64 * 1024;
const MAX_CALL_RECORDS: usize = 256;
const MAX_APPROVALS: u32 = 128;

#[path = "plan22_interventions.rs"]
mod interventions;

#[derive(Debug)]
struct WorkerFailure(&'static str);
impl fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}
impl Error for WorkerFailure {}
type WorkerResult<T> = Result<T, WorkerFailure>;
fn failure(code: &'static str) -> WorkerFailure {
    WorkerFailure(code)
}
fn checked<T, E>(value: Result<T, E>, code: &'static str) -> WorkerResult<T> {
    value.map_err(|_| failure(code))
}

#[derive(Debug, Clone)]
struct AllowedProcess {
    executable: String,
    arguments: Vec<String>,
}

#[derive(Debug, Clone)]
struct AllowedNetworkRequest {
    method: String,
    url: String,
}

#[derive(Debug, Clone)]
struct Request {
    case_id: String,
    variant: String,
    mode: AgentSessionMode,
    objective: String,
    model: String,
    context_limit: u32,
    output_limit: u32,
    timeout_seconds: u64,
    allowed_write_paths: Vec<RepositoryPath>,
    allowed_processes: Vec<AllowedProcess>,
    process_effect_paths: Vec<RepositoryPath>,
    allowed_network_requests: Vec<AllowedNetworkRequest>,
}

fn object_fields(value: &Value, fields: &[&str]) -> WorkerResult<()> {
    let object = value
        .as_object()
        .ok_or_else(|| failure("input_not_object"))?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(failure("input_fields"));
    }
    Ok(())
}
fn string(value: &Value, field: &str, maximum: usize) -> WorkerResult<String> {
    let value = value[field]
        .as_str()
        .ok_or_else(|| failure("input_string"))?;
    if value.trim().is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(failure("input_string_bounds"));
    }
    Ok(value.to_owned())
}
fn slug(value: String) -> WorkerResult<String> {
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        return Err(failure("input_slug"));
    }
    Ok(value)
}
fn parse_request(raw: &[u8]) -> WorkerResult<Request> {
    if raw.len() as u64 > MAX_REQUEST_BYTES {
        return Err(failure("request_too_large"));
    }
    let value: Value = checked(serde_json::from_slice(raw), "request_json")?;
    object_fields(
        &value,
        &[
            "version",
            "case_id",
            "variant",
            "mode",
            "objective",
            "model",
            "context_limit",
            "output_limit",
            "timeout_seconds",
            "allowed_write_paths",
            "allowed_processes",
            "process_effect_paths",
            "allowed_network_requests",
        ],
    )?;
    if value["version"] != json!(2) {
        return Err(failure("request_version"));
    }
    let case_id = slug(string(&value, "case_id", 32)?)?;
    let variant = slug(string(&value, "variant", 64)?)?;
    let mode = match value["mode"].as_str() {
        Some("Ask") => AgentSessionMode::Ask,
        Some("Agent") => AgentSessionMode::Agent,
        _ => return Err(failure("request_mode")),
    };
    let model = string(&value, "model", 128)?;
    if !matches!(
        model.as_str(),
        "ornith-1.5:9b"
            | "qwen38-27b-iq3s:latest"
            | "bonsai-2:27b"
            | "qwen38-4k:latest"
            | "qwen38-8k:latest"
            | "qwen38-16k:latest"
    ) {
        return Err(failure("model_not_reviewed"));
    }
    let context_limit = value["context_limit"]
        .as_u64()
        .ok_or_else(|| failure("input_context"))?;
    let output_limit = value["output_limit"]
        .as_u64()
        .ok_or_else(|| failure("input_output"))?;
    if !matches!(context_limit, 4096 | 8192 | 16384) || output_limit != context_limit / 4 {
        return Err(failure("input_profile"));
    }
    if (model == "qwen38-4k:latest" && context_limit != 4096)
        || (model == "qwen38-8k:latest" && context_limit != 8192)
        || (model == "qwen38-16k:latest" && context_limit != 16384)
    {
        return Err(failure("input_context_variant"));
    }
    let timeout_seconds = value["timeout_seconds"]
        .as_u64()
        .ok_or_else(|| failure("input_timeout"))?;
    if !(10..=3600).contains(&timeout_seconds) {
        return Err(failure("input_timeout_bounds"));
    }
    let writes = value["allowed_write_paths"]
        .as_array()
        .ok_or_else(|| failure("input_write_paths"))?;
    let processes = value["allowed_processes"]
        .as_array()
        .ok_or_else(|| failure("input_processes"))?;
    if writes.len() > 64 || processes.len() > 32 {
        return Err(failure("input_scope_bounds"));
    }
    let mut allowed_write_paths = Vec::new();
    for path in writes {
        let raw = path.as_str().ok_or_else(|| failure("input_path"))?;
        if raw.len() > 4096 || raw.contains('\\') {
            return Err(failure("input_path"));
        }
        let path = checked(
            RepositoryPath::try_from_bytes(raw.as_bytes().to_vec()),
            "input_path",
        )?;
        if allowed_write_paths.contains(&path) {
            return Err(failure("input_duplicate_path"));
        }
        allowed_write_paths.push(path);
    }
    let mut allowed_processes = Vec::new();
    for process in processes {
        object_fields(process, &["executable", "arguments"])?;
        let executable = string(process, "executable", 128)?;
        if !executable
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        {
            return Err(failure("input_executable"));
        }
        let args = process["arguments"]
            .as_array()
            .ok_or_else(|| failure("input_arguments"))?;
        if args.len() > 64 {
            return Err(failure("input_arguments_bounds"));
        }
        let arguments = args
            .iter()
            .map(|arg| {
                let arg = arg.as_str().ok_or_else(|| failure("input_argument"))?;
                if arg.len() > 4096 || arg.contains(['\0', '\r', '\n']) {
                    return Err(failure("input_argument_bounds"));
                }
                Ok(arg.to_owned())
            })
            .collect::<WorkerResult<Vec<_>>>()?;
        allowed_processes.push(AllowedProcess {
            executable,
            arguments,
        });
    }
    let effect_paths = value["process_effect_paths"]
        .as_array()
        .ok_or_else(|| failure("input_process_effect_paths"))?;
    let mut process_effect_paths = Vec::new();
    for path in effect_paths {
        let raw = path
            .as_str()
            .ok_or_else(|| failure("input_process_effect_path"))?;
        if case_id != "A05" || raw != ".a3-probe-notes.json" || !process_effect_paths.is_empty() {
            return Err(failure("input_process_effect_path_scope"));
        }
        process_effect_paths.push(checked(
            RepositoryPath::try_from_bytes(raw.as_bytes().to_vec()),
            "input_process_effect_path",
        )?);
    }
    let network = value["allowed_network_requests"]
        .as_array()
        .ok_or_else(|| failure("input_network_requests"))?;
    if network.len() > 12 {
        return Err(failure("input_network_request_bounds"));
    }
    let mut allowed_network_requests = Vec::new();
    for entry in network {
        object_fields(entry, &["method", "url"])?;
        let method = string(entry, "method", 8)?;
        let url = string(entry, "url", 256)?;
        if case_id != "A04"
            || !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH")
            || !matches!(
                url.as_str(),
                "http://127.0.0.1:18765/"
                    | "http://127.0.0.1:18765/health"
                    | "http://127.0.0.1:18765/missing"
                    | "http://127.0.0.1:8765/"
                    | "http://127.0.0.1:8765/health"
                    | "http://127.0.0.1:8765/missing"
            )
            || allowed_network_requests
                .iter()
                .any(|prior: &AllowedNetworkRequest| prior.method == method && prior.url == url)
        {
            return Err(failure("input_network_request_scope"));
        }
        allowed_network_requests.push(AllowedNetworkRequest { method, url });
    }
    if mode == AgentSessionMode::Ask
        && (!allowed_write_paths.is_empty()
            || !allowed_processes.is_empty()
            || !process_effect_paths.is_empty()
            || !allowed_network_requests.is_empty())
    {
        return Err(failure("ask_scope_must_be_empty"));
    }
    Ok(Request {
        case_id,
        variant,
        mode,
        objective: string(&value, "objective", 64 * 1024)?,
        model,
        context_limit: context_limit as u32,
        output_limit: output_limit as u32,
        timeout_seconds,
        allowed_write_paths,
        allowed_processes,
        process_effect_paths,
        allowed_network_requests,
    })
}

fn regular_child(root: &Path, name: &str, directory: bool) -> WorkerResult<PathBuf> {
    let path = root.join(name);
    let metadata = checked(fs::symlink_metadata(&path), "input_path_metadata")?;
    if metadata.file_type().is_symlink()
        || metadata.is_dir() != directory
        || (!directory && !metadata.is_file())
    {
        return Err(failure("input_path_kind"));
    }
    let canonical = checked(fs::canonicalize(path), "input_path_canonical")?;
    if canonical.parent() != Some(root) {
        return Err(failure("input_path_escape"));
    }
    Ok(canonical)
}
fn validate_run_child(approved: &Path, selected: &Path) -> WorkerResult<()> {
    if selected.parent() != Some(approved) {
        return Err(failure("run_directory_escape"));
    }
    Ok(())
}
fn run_directory() -> WorkerResult<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .ancestors()
        .nth(3)
        .ok_or_else(|| failure("repo_root"))?;
    let repo = checked(fs::canonicalize(repo), "repo_canonical")?;
    let expected = repo.join("target/plan22-runs");
    let approved = checked(fs::canonicalize(&expected), "run_root_missing")?;
    if approved != expected {
        return Err(failure("run_root_redirected"));
    }
    let selected =
        std::env::var_os("A3_PLAN22_RUN_DIR").ok_or_else(|| failure("run_opt_in_missing"))?;
    let metadata = checked(fs::symlink_metadata(&selected), "run_directory_missing")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure("run_directory_kind"));
    }
    let selected = checked(fs::canonicalize(selected), "run_directory_canonical")?;
    validate_run_child(&approved, &selected)?;
    Ok(selected)
}
fn atomic_report(directory: &Path, report: &Value) -> WorkerResult<()> {
    let bytes = bounded_report_bytes(report)?;
    let destination = directory.join("worker.json");
    if destination.exists() {
        regular_child(directory, "worker.json", false)?;
    }
    let temporary = directory.join("worker.json.next");
    let mut file = checked(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary),
        "report_create",
    )?;
    checked(file.write_all(&bytes), "report_write")?;
    checked(file.sync_all(), "report_sync")?;
    drop(file);
    checked(fs::rename(temporary, destination), "report_commit")
}

fn bounded_report_bytes(report: &Value) -> WorkerResult<Vec<u8>> {
    let bytes = checked(serde_json::to_vec_pretty(report), "report_json")?;
    if bytes.len() <= MAX_REPORT_BYTES {
        return Ok(bytes);
    }
    // Never lose the terminal outcome because optional diagnostic histories reach
    // their combined cap. Aggregate usage and verification/answer fields survive.
    let mut projection = report.clone();
    projection["report_projection_truncated"] = json!(true);
    projection["projection_records_omitted"] = json!({});
    for (group, field) in [("journal", "records"), ("metrics", "records")] {
        let mut omitted = 0_u64;
        loop {
            let Some(records) = projection
                .get_mut(group)
                .and_then(|value| value.get_mut(field))
                .and_then(Value::as_array_mut)
            else {
                break;
            };
            if records.pop().is_none() {
                break;
            }
            omitted += 1;
            projection["projection_records_omitted"][group] = json!(omitted);
            if group == "journal" {
                projection["journal"]["truncated"] = json!(true);
            }
            let bytes = checked(serde_json::to_vec_pretty(&projection), "report_json")?;
            if bytes.len() <= MAX_REPORT_BYTES {
                return Ok(bytes);
            }
        }
    }
    Err(failure("report_too_large"))
}

#[derive(Debug, Default)]
struct Metrics {
    calls: u64,
    probe_calls: u64,
    output_bytes: u64,
    completed: u64,
    errors: u64,
    prompt_tokens: u64,
    output_tokens: u64,
    prompt_usage_missing: u64,
    output_usage_missing: u64,
    request_bytes: u64,
    schema_bytes: u64,
    records: Vec<Value>,
    overflow: u64,
}
type SharedMetrics = Arc<Mutex<Metrics>>;
#[derive(Debug)]
struct ObservedProvider {
    inner: Arc<dyn ModelProvider>,
    metrics: SharedMetrics,
}

struct CallObservation {
    metrics: SharedMetrics,
    record: Value,
    index: Option<usize>,
    started: Instant,
    finished: bool,
}
impl CallObservation {
    fn new(metrics: &SharedMetrics, stage: &str, message_bytes: u64, schema_bytes: u64) -> Self {
        let record = json!({"stage":stage,"status":"in_flight","message_bytes":message_bytes,
            "schema_bytes":schema_bytes,"output_bytes":0,"prompt_tokens":null,"output_tokens":null,
            "first_output_millis":null,"elapsed_millis":null});
        let mut data = lock_recovering_poison(metrics);
        data.calls = data.calls.saturating_add(1);
        data.request_bytes = data.request_bytes.saturating_add(message_bytes);
        data.schema_bytes = data.schema_bytes.saturating_add(schema_bytes);
        let index = if data.records.len() < MAX_CALL_RECORDS {
            let index = data.records.len();
            data.records.push(record.clone());
            Some(index)
        } else {
            data.overflow = data.overflow.saturating_add(1);
            None
        };
        Self {
            metrics: Arc::clone(metrics),
            record,
            index,
            started: Instant::now(),
            finished: false,
        }
    }
    fn publish(&self, data: &mut Metrics) {
        if let Some(index) = self.index {
            data.records[index] = self.record.clone();
        }
    }
    fn bind_request(&mut self, request: &ModelProviderRequest) {
        let mut messages = blake3::Hasher::new_derive_key("a3.plan22.request-messages.v1");
        for message in request.messages() {
            let role = match message.role() {
                a3_application::ModelMessageRole::System => 0,
                a3_application::ModelMessageRole::User => 1,
                a3_application::ModelMessageRole::Assistant => 2,
            };
            messages.update(&[role]);
            messages.update(&(message.content().len() as u64).to_le_bytes());
            messages.update(message.content().as_bytes());
        }
        self.record["message_blake3"] = json!(messages.finalize().to_hex().to_string());
        self.record["schema_blake3"] = request.structured_output().map_or(Value::Null, |schema| {
            json!(
                blake3::hash(schema.value().to_string().as_bytes())
                    .to_hex()
                    .to_string()
            )
        });
        self.publish(&mut lock_recovering_poison(&self.metrics));
    }
    fn output(&mut self, bytes: usize) {
        if self.finished {
            return;
        }
        if self.record["first_output_millis"].is_null() {
            self.record["first_output_millis"] = json!(self.started.elapsed().as_millis());
        }
        self.record["output_bytes"] = json!(
            self.record["output_bytes"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(bytes as u64)
        );
        let mut data = lock_recovering_poison(&self.metrics);
        data.output_bytes = data.output_bytes.saturating_add(bytes as u64);
        self.publish(&mut data);
    }
    fn complete(&mut self, completion: a3_application::ModelProviderCompletion) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.record["status"] = json!("completed");
        self.record["finish"] = json!(format!("{:?}", completion.reason()));
        self.record["elapsed_millis"] = json!(self.started.elapsed().as_millis());
        let usage = completion.usage();
        self.record["prompt_tokens"] = json!(usage.prompt_tokens());
        self.record["output_tokens"] = json!(usage.output_tokens());
        let mut data = lock_recovering_poison(&self.metrics);
        data.completed = data.completed.saturating_add(1);
        match usage.prompt_tokens() {
            Some(tokens) => data.prompt_tokens = data.prompt_tokens.saturating_add(tokens),
            None => data.prompt_usage_missing = data.prompt_usage_missing.saturating_add(1),
        }
        match usage.output_tokens() {
            Some(tokens) => data.output_tokens = data.output_tokens.saturating_add(tokens),
            None => data.output_usage_missing = data.output_usage_missing.saturating_add(1),
        }
        self.publish(&mut data);
    }
    fn failed(&mut self, status: &str, reason: &str) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.record["status"] = json!(status);
        self.record["failure"] = json!(reason);
        self.record["elapsed_millis"] = json!(self.started.elapsed().as_millis());
        let mut data = lock_recovering_poison(&self.metrics);
        data.errors = data.errors.saturating_add(1);
        self.publish(&mut data);
    }
}
impl Drop for CallObservation {
    fn drop(&mut self) {
        if !self.finished {
            self.failed("incomplete", "stream_dropped_without_completion");
        }
    }
}
impl ModelProvider for ObservedProvider {
    fn provider_id(&self) -> &a3_domain::ModelProviderId {
        self.inner.provider_id()
    }
    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let message_bytes = request
                .messages()
                .iter()
                .map(|message| message.content().len() as u64)
                .sum::<u64>();
            let schema_bytes = request
                .structured_output()
                .map_or(0, |schema| schema.value().to_string().len() as u64);
            let stage = request
                .structured_output()
                .and_then(|schema| schema.value().get("title"))
                .and_then(Value::as_str)
                .filter(|title| title.starts_with("A^3 ") && title.len() < 80)
                .unwrap_or("research")
                .to_owned();
            let mut observed =
                CallObservation::new(&self.metrics, &stage, message_bytes, schema_bytes);
            observed.bind_request(request);
            let stream = match self.inner.stream(request, timeout, control).await {
                Ok(stream) => stream,
                Err(error) => {
                    observed.failed("admission_failed", &format!("{error:?}"));
                    return Err(error);
                }
            };
            Ok(Box::pin(stream.inspect(move |event| match event {
                Ok(ProviderEvent::OutputText(chunk)) => observed.output(chunk.as_str().len()),
                Ok(ProviderEvent::Completed(completion)) => observed.complete(*completion),
                Err(error) => observed.failed("stream_failed", &format!("{error:?}")),
            })) as a3_application::ProviderEventStream<'a>)
        })
    }
}
fn metrics_json(metrics: &SharedMetrics) -> Value {
    let data = lock_recovering_poison(metrics);
    let incomplete = data.calls.saturating_sub(data.completed);
    json!({"calls":data.calls,"probe_requests":data.probe_calls,"probe_metric_kind":"capability_probe_invocations_excluding_catalog",
        "completed":data.completed,"stream_errors":data.errors,"output_bytes":data.output_bytes,
        "provider_prompt_tokens":(incomplete==0 && data.prompt_usage_missing==0).then_some(data.prompt_tokens),
        "provider_output_tokens":(incomplete==0 && data.output_usage_missing==0).then_some(data.output_tokens),
        "reported_prompt_tokens_sum":data.prompt_tokens,"reported_output_tokens_sum":data.output_tokens,
        "missing_prompt_usage_calls":data.prompt_usage_missing+incomplete,"missing_output_usage_calls":data.output_usage_missing+incomplete,
        "message_utf8_bytes":data.request_bytes,"format_schema_utf8_bytes":data.schema_bytes,
        "counter":"ConservativeUtf8BytesV1","schema_grounding":"FormatFieldOnly","thinking":false,
        "records":data.records,"record_overflow":data.overflow})
}

#[derive(Debug)]
struct ProbeControl {
    cancelled: AtomicBool,
}
impl ModelOperationControl for ProbeControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}
async fn probe(
    request: &Request,
    metrics: &SharedMetrics,
) -> WorkerResult<(Arc<dyn ModelProvider>, ModelProfile)> {
    let provider = Arc::new(checked(
        OllamaModelProvider::new(
            checked(OllamaEndpoint::parse("http://127.0.0.1:11434"), "endpoint")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        ),
        "provider",
    )?);
    let control = ProbeControl {
        cancelled: AtomicBool::new(false),
    };
    let timeout = checked(ModelRequestTimeout::from_millis(120_000), "probe_timeout")?;
    let catalog = checked(
        DiscoverProviderModels::new(provider.as_ref())
            .execute(timeout, &control)
            .await,
        "catalog_unavailable",
    )?;
    if !catalog
        .model_ids()
        .iter()
        .any(|model| model.as_str() == request.model)
    {
        return Err(failure("model_not_installed"));
    }
    let settings = checked(
        ModelProfileSettings::new(
            checked(
                ModelContextLimit::new(request.context_limit),
                "profile_context",
            )?,
            checked(
                ModelOutputLimit::new(request.output_limit),
                "profile_output",
            )?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            checked(ModelParallelismLimit::new(1), "profile_parallelism")?,
            ModelSamplingProfile::new(
                checked(ModelTemperature::from_milli(0), "profile_temperature")?,
                checked(ModelTopP::from_milli(1000), "profile_top_p")?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        ),
        "profile_settings",
    )?;
    {
        let mut observed = lock_recovering_poison(metrics);
        observed.probe_calls = observed.probe_calls.saturating_add(1);
    }
    let profile = checked(
        ProbeModelProfile::new(provider.as_ref())
            .execute(
                &ModelCapabilityProbeRequest::new(
                    checked(ModelId::try_from_string(request.model.clone()), "model_id")?,
                    settings,
                ),
                timeout,
                &control,
            )
            .await,
        "capability_probe_failed",
    )?;
    if profile.capabilities().structured_output() != ModelStructuredOutputCapability::Verified {
        return Err(failure("capability_not_verified"));
    }
    Ok((
        Arc::new(ObservedProvider {
            inner: provider,
            metrics: Arc::clone(metrics),
        }),
        profile,
    ))
}

struct Clock(Instant);
impl JobClock for Clock {
    fn now(&self) -> JobTimestamp {
        JobTimestamp::from_millis(self.0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
    }
}
#[derive(Debug)]
struct IndexControl;
impl IndexPersistenceControl for IndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}
impl RepositoryIndexControl for IndexControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}
fn now() -> WorkerResult<AgentRunTimestamp> {
    let millis = checked(SystemTime::now().duration_since(UNIX_EPOCH), "clock")?.as_millis();
    checked(
        AgentRunTimestamp::from_unix_millis(checked(u64::try_from(millis), "clock_range")?),
        "clock_range",
    )
}
fn identity() -> WorkerResult<[u8; 32]> {
    let mut bytes = [0; 32];
    checked(getrandom::fill(&mut bytes), "identity")?;
    Ok(bytes)
}
fn scope_allows(request: &Request, action: &AgentApprovalAction) -> bool {
    match action {
        AgentApprovalAction::Machine { scope, operation } => {
            scope.kind() == a3_domain::AgentMutationKind::MachineNetwork
                && *operation == a3_domain::PathPolicyOperation::Read
                && request
                    .allowed_network_requests
                    .iter()
                    .any(|allowed| allowed.method == "GET" && allowed.url == scope.target())
        }
        AgentApprovalAction::Patch(patch) => {
            !patch.files().is_empty()
                && patch.files().iter().all(|file| {
                    matches!(
                        file.operation(),
                        AgentApprovalFileOperation::Add | AgentApprovalFileOperation::Update
                    ) && file
                        .source_path()
                        .into_iter()
                        .chain(file.target_path())
                        .all(|path| request.allowed_write_paths.contains(path))
                })
        }
        AgentApprovalAction::Process(process) => process_scope_allows(
            request,
            process.executable(),
            process.arguments(),
            process.working_directory(),
            process.execution_mode(),
            process.network(),
        ),
        AgentApprovalAction::LoopbackStart { .. }
        | AgentApprovalAction::LoopbackStartup { .. }
        | AgentApprovalAction::LoopbackProbe { .. } => false,
    }
}

fn process_scope_allows(
    request: &Request,
    executable: &str,
    arguments: &[String],
    directory: &AgentApprovalWorkingDirectory,
    mode: ProcessExecutionMode,
    network: AgentApprovalNetworkScope,
) -> bool {
    // Open argv still receives an exact one-time grant through the real policy boundary.
    // It is not a Full-machine classification or permission for other commands/scripts.
    directory == &AgentApprovalWorkingDirectory::Root
        && matches!(
            mode,
            ProcessExecutionMode::KnownSafe | ProcessExecutionMode::Open
        )
        && network == AgentApprovalNetworkScope::Denied
        && request
            .allowed_processes
            .iter()
            .any(|allowed| allowed.executable == executable && allowed.arguments == arguments)
}

fn scope_summary(request: &Request, action: &AgentApprovalAction) -> Value {
    match action {
        AgentApprovalAction::Process(process) => json!({"kind":"process",
            "execution_mode":format!("{:?}",process.execution_mode()),
            "root_cwd":process.working_directory()==&AgentApprovalWorkingDirectory::Root,
            "network_denied":process.network()==AgentApprovalNetworkScope::Denied,
            "executable_matches":request.allowed_processes.iter().any(|allowed|allowed.executable==process.executable()),
            "argv_matches":request.allowed_processes.iter().any(|allowed|allowed.executable==process.executable() && allowed.arguments==process.arguments())}),
        AgentApprovalAction::Patch(patch) => {
            json!({"kind":"patch","file_count":patch.files().len(),
            "paths_match":patch.files().iter().all(|file| file.source_path().into_iter().chain(file.target_path()).all(|path|request.allowed_write_paths.contains(path)))})
        }
        AgentApprovalAction::Machine { scope, operation } => json!({"kind":"machine",
            "http_get":scope.kind()==a3_domain::AgentMutationKind::MachineNetwork && *operation==a3_domain::PathPolicyOperation::Read,
            "target_matches":request.allowed_network_requests.iter().any(|allowed|allowed.method=="GET"&&allowed.url==scope.target())}),
        AgentApprovalAction::LoopbackStart { .. }
        | AgentApprovalAction::LoopbackStartup { .. }
        | AgentApprovalAction::LoopbackProbe { .. } => {
            json!({"kind":"unsupported"})
        }
    }
}

// Observes the existing product's check-before-edit behavior. It never runs a check,
// changes the plan, imports the outer oracle, or makes verification evidence itself.
struct RecoveryCheck {
    initial_hash: [u8; 32],
    patch_authorized: bool,
    failed: std::collections::BTreeMap<a3_application::AgentInspectionId, a3_domain::SnapshotId>,
    successful: std::collections::BTreeMap<
        a3_application::AgentInspectionId,
        (a3_domain::SnapshotId, [u8; 32]),
    >,
    seen: std::collections::BTreeSet<a3_application::AgentInspectionId>,
}

impl RecoveryCheck {
    fn new(directory: &Path) -> WorkerResult<Self> {
        Ok(Self {
            initial_hash: recovery_source_hash(directory)?,
            patch_authorized: false,
            failed: Default::default(),
            successful: Default::default(),
            seen: Default::default(),
        })
    }

    fn before_patch_grant(&mut self) {
        self.patch_authorized = true;
    }

    fn record_process(
        &mut self,
        id: a3_application::AgentInspectionId,
        snapshot: a3_domain::SnapshotId,
        termination: a3_domain::ProcessTermination,
        source_hash: [u8; 32],
        applied_patches: usize,
    ) -> WorkerResult<()> {
        if !self.seen.insert(id) {
            return Ok(());
        }
        if self.seen.len() > 128 {
            return Err(failure("recovery_check_capacity"));
        }
        let source_unchanged = source_hash == self.initial_hash;
        if let a3_domain::ProcessTermination::Exited(exit) = termination {
            if !exit.success() && !self.patch_authorized && applied_patches == 0 && source_unchanged
            {
                self.failed.insert(id, snapshot);
            } else if exit.success()
                && !source_unchanged
                && applied_patches > 0
                && self.failed.values().any(|before| *before != snapshot)
            {
                self.successful.insert(id, (snapshot, source_hash));
            }
        }
        Ok(())
    }

    fn current_success(
        &self,
        snapshot: a3_domain::SnapshotId,
        source_hash: [u8; 32],
        published_hash: Option<[u8; 32]>,
    ) -> bool {
        published_hash == Some(source_hash)
            && self
                .successful
                .values()
                .any(|evidence| *evidence == (snapshot, source_hash))
    }

    fn observe(
        &mut self,
        directory: &Path,
        overview: Option<&a3_application::AgentInspectionOverview>,
        attempts: &[a3_domain::AgentMutationAttempt],
        published: &a3_domain::PublishedIndex,
        report: &mut Value,
    ) -> WorkerResult<()> {
        let source_hash = recovery_source_hash(directory)?;
        let source_unchanged = source_hash == self.initial_hash;
        let current_snapshot = published.run().snapshot_id();
        let published_hash = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| revision.path().as_bytes() == b"average.py")
            .map(|revision| *revision.content_hash().as_bytes());
        let live_matches_publication = published_hash == Some(source_hash);
        let applied_patches = attempts
            .iter()
            .filter(|attempt| {
                attempt.kind() == a3_domain::AgentMutationKind::Patch
                    && attempt.disposition().application_state()
                        == a3_domain::MutationApplicationState::Applied
            })
            .count();
        if let Some(overview) = overview.filter(|_| live_matches_publication) {
            for process in overview.processes() {
                // A08 admits only the fixture's exact unittest argv. Machine-process
                // observations are classified Command by the existing controller.
                if matches!(
                    process.kind(),
                    a3_application::AgentProcessInspectionKind::Test
                        | a3_application::AgentProcessInspectionKind::Command
                ) && process.context().snapshot_id() == current_snapshot
                {
                    self.record_process(
                        process.id(),
                        process.context().snapshot_id(),
                        process.termination(),
                        source_hash,
                        applied_patches,
                    )?;
                }
            }
        }
        report["observations"] = json!({"observed_failed_check":!self.failed.is_empty(),
            "corrected_after_failure":self.current_success(current_snapshot, source_hash, published_hash)});
        report["observation_evidence"] = json!({"version":1,
            "source":"production_process_inspection_and_mutation_journal",
            "failed_checks_before_first_patch":self.failed.len(),
            "successful_checks_after_correction":self.successful.len(),
            "applied_patch_count":applied_patches,"check_count":self.seen.len(),
            "initial_file_unchanged":source_unchanged,
            "current_successful_check":self.current_success(current_snapshot, source_hash, published_hash)});
        Ok(())
    }
}

fn recovery_source_hash(directory: &Path) -> WorkerResult<[u8; 32]> {
    let worktree = regular_child(directory, "worktree", true)?;
    let source = regular_child(&worktree, "average.py", false)?;
    let mut bytes = Vec::new();
    checked(
        checked(fs::File::open(source), "recovery_source_open")?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes),
        "recovery_source_read",
    )?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(failure("recovery_source_too_large"));
    }
    Ok(*blake3::hash(&bytes).as_bytes())
}

async fn capture_journal(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    run_id: a3_domain::AgentRunId,
) -> WorkerResult<Value> {
    let mut records = Vec::new();
    let mut after = None;
    let mut truncated = false;
    'pages: for _ in 0..4 {
        let page = checked(
            store
                .load_run_events(
                    project,
                    run_id,
                    after,
                    checked(
                        a3_application::RunEventPageLimit::new(256),
                        "event_page_limit",
                    )?,
                )
                .await,
            "journal_read",
        )?;
        for event in page.events() {
            let (kind, from, to) = match event.kind() {
                a3_domain::RunEventKind::RunStarted => ("run_started", None, None),
                a3_domain::RunEventKind::StateTransition { from, to } => (
                    "state_transition",
                    Some(format!("{from:?}")),
                    Some(format!("{to:?}")),
                ),
                a3_domain::RunEventKind::ContextCompiled => ("context_compiled", None, None),
                a3_domain::RunEventKind::ModelInteraction => ("model_interaction", None, None),
                a3_domain::RunEventKind::ToolAction => ("tool_action", None, None),
                a3_domain::RunEventKind::LedgerUpdated { .. } => ("ledger_updated", None, None),
                a3_domain::RunEventKind::VerificationRecorded => {
                    ("verification_recorded", None, None)
                }
                a3_domain::RunEventKind::ApprovalRecorded => ("approval_recorded", None, None),
                a3_domain::RunEventKind::Diagnostic => ("diagnostic", None, None),
            };
            records.push(json!({"sequence":event.sequence().get(),"kind":kind,"from":from,"to":to,
                "code":format!("{:?}",event.payload().code()),
                "outcome":event.payload().outcome().map(|outcome|format!("{outcome:?}")),
                "action":event.turn_charge().and_then(|charge|charge.action()).map(|action|format!("{action:?}")),
                "repair":event.turn_charge().map(|charge|format!("{:?}",charge.repair()))}));
            if checked(serde_json::to_vec_pretty(&records), "journal_report_json")?.len()
                > 96 * 1024
            {
                records.pop();
                truncated = true;
                break 'pages;
            }
        }
        truncated = page.has_more();
        after = page.events().last().map(a3_domain::RunEvent::sequence);
        if !truncated {
            break;
        }
    }
    Ok(json!({"truncated":truncated,"records":records}))
}

async fn capture_agent_report(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    session_id: a3_domain::AgentSessionId,
    task_id: a3_domain::TaskId,
    report: &mut Value,
) -> WorkerResult<()> {
    let detail = checked(
        store
            .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
            .await,
        "public_report_read",
    )?
    .ok_or_else(|| failure("session_missing"))?;
    let entry = public_agent_report(detail.entries(), detail.session().state(), task_id);
    if let Some(entry) = entry {
        let answer = entry.text().as_str();
        let mut end = answer.len().min(MAX_ANSWER_BYTES);
        while !answer.is_char_boundary(end) {
            end -= 1;
        }
        report["agent_answer"] = json!(&answer[..end]);
        report["agent_answer_truncated"] = json!(end != answer.len());
        report["agent_answer_metadata"] = json!({"entry_kind":format!("{:?}",entry.kind()),
            "entry_sequence":entry.sequence().get(),"task_matches":true});
    } else {
        report["agent_answer"] = Value::Null;
        report["agent_answer_truncated"] = json!(false);
        report["agent_answer_metadata"] = json!({"task_matches":false});
    }
    Ok(())
}

fn public_agent_report(
    entries: &[a3_domain::AgentSessionEntry],
    state: AgentSessionState,
    task_id: a3_domain::TaskId,
) -> Option<&a3_domain::AgentSessionEntry> {
    entries.iter().rev().find(|entry| {
        entry.task_id() == Some(task_id)
            && (entry.kind() == AgentSessionEntryKind::FinalReport
                || (state == AgentSessionState::AwaitingUser
                    && entry.kind() == AgentSessionEntryKind::AssistantSummary))
    })
}

fn stopped_executor_failure(
    state: crate::agent_run_manager::AgentRunActivityState,
    observed: &interventions::AttemptObservations,
) -> Option<a3_application::AgentRunExecutionFailure> {
    (state == crate::agent_run_manager::AgentRunActivityState::Failed && observed.completed > 0)
        .then_some(observed.last_error)
        .flatten()
}

fn settled_product_stop(
    state: crate::agent_run_manager::AgentRunActivityState,
    session: AgentSessionState,
) -> bool {
    !state.owns_live_worker()
        && (state == crate::agent_run_manager::AgentRunActivityState::Failed
            || matches!(
                session,
                AgentSessionState::Failed
                    | AgentSessionState::AwaitingUser
                    | AgentSessionState::Cancelled
            ))
}

async fn evaluate(
    directory: &Path,
    request: &Request,
    metrics: &SharedMetrics,
    report: &mut Value,
) -> WorkerResult<()> {
    interventions::validate_resume(directory, request)?;
    let worktree = regular_child(directory, "worktree", true)?;
    let appdata = regular_child(directory, "appdata", true)?;
    if !interventions::is_resume()?
        && checked(fs::read_dir(&appdata), "appdata_read")?
            .next()
            .is_some()
    {
        return Err(failure("appdata_not_empty"));
    }
    let project = checked(
        RepositoryInspector::new().inspect(&worktree),
        "repository_inspection",
    )?;
    let store = Arc::new(checked(
        LibsqlKnowledgeStore::open(&checked(
            StorageLayout::prepare(&appdata),
            "storage_layout",
        )?)
        .await,
        "storage_open",
    )?);
    checked(
        store.record_opened_project(&project).await,
        "record_project",
    )?;
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
            &project,
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
        "index_failed",
    )?;
    report["phase"] = json!("probing");
    atomic_report(directory, report)?;
    let probe_started = Instant::now();
    let probed = tokio::time::timeout(Duration::from_secs(120), probe(request, metrics)).await;
    report["probe_elapsed_millis"] = json!(probe_started.elapsed().as_millis());
    let (provider, profile) = checked(probed, "probe_deadline_exceeded")??;
    let task_started = interventions::task_clock(directory, request, report)?;
    let runtime = AgentConversationRuntime::new(
        store.clone(),
        Arc::new(a3_credentials::NativeProviderCredentialStore::new()),
    )
    .with_execution_override(provider, profile);
    let inspection = Arc::new(a3_application::AgentInspectionBuffer::new());
    inspection.activate_project(&project);
    let approvals = Arc::new(a3_application::AgentApprovalBuffer::new());
    approvals.activate_project(&project);
    let reporter = Arc::new(AgentSessionRunReporter::new(store.clone()));
    let executor = Arc::new(checked(
        ProductionAgentRunExecutor::new(
            ProductionAgentRunPorts {
                permissions: Some(store.clone()),
                workspace: store.clone(),
                ledgers: store.clone(),
                journal: store.clone(),
                actions: store.clone(),
                recovery: store.clone(),
                policy: store.clone(),
                evidence: store.clone(),
                index: store.clone(),
                lens_index: store.clone(),
                search: store.clone(),
                claims: store.clone(),
                allowlist: store.clone(),
                research: Some(store.clone()),
            },
            runtime.clone(),
            inspection.clone(),
            approvals.clone(),
            Some(reporter.clone()),
        ),
        "executor",
    )?);
    let observed_attempts = Arc::new(Mutex::new(interventions::AttemptObservations::default()));
    let executor: Arc<dyn a3_application::AgentRunExecutor> =
        Arc::new(interventions::TrackedExecutor {
            inner: executor,
            observations: observed_attempts.clone(),
        });
    let (scheduler, events) = checked(
        JobScheduler::new(
            checked(JobSchedulerConfig::new(1, 4, 256), "scheduler_config")?,
            Arc::new(Clock(Instant::now())),
        ),
        "scheduler",
    )?;
    let ids = Arc::new(crate::job_ids::DesktopJobIds::new());
    let recovery = Arc::new(CoreAgentRuntimeRecovery::new(
        InspectAgentTaskRecovery::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        ),
        ControlAgentTaskRun::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        ),
    ));
    let manager = Arc::new(checked(
        AgentRunManager::start(
            checked(scheduler.submitter(), "submitter")?,
            events,
            executor,
            recovery,
            ids.clone(),
        ),
        "run_manager",
    )?);
    checked(
        manager.activate_project(project.clone()),
        "activate_project",
    )?;
    let sessions = AgentSessionManager::new(AgentSessionManagerDependencies {
        store: store.clone(),
        runtime,
        submitter: checked(scheduler.submitter(), "submitter")?,
        job_ids: ids,
        materializer: Some(AgentTaskMaterializer::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        )),
        run_manager: Some(manager.clone()),
        reporter: Some(reporter),
        researcher: Some(AgentAskResearcher::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        )),
        research_store: Some(store.clone()),
    });
    let result = observe_case(
        directory,
        request,
        metrics,
        report,
        &store,
        &project,
        &sessions,
        &manager,
        &approvals,
        &inspection,
        &observed_attempts,
        task_started,
    )
    .await;
    report["task_elapsed_millis"] = json!(task_started.elapsed().as_millis());
    // All paths after scheduler creation stop and join their owned workers before returning.
    let conversation_stopped = sessions.quiesce().is_ok();
    let agent_stopped = manager.deactivate_project().is_ok();
    drop(sessions);
    drop(manager);
    let shutdown = scheduler.shutdown(ShutdownMode::CancelAndWait);
    report["cleanup"] = json!({"conversation_stopped":conversation_stopped,"agent_stopped":agent_stopped,"scheduler_joined":shutdown.is_ok()});
    if !conversation_stopped || !agent_stopped || shutdown.is_err() {
        return Err(failure("cleanup_failed"));
    }
    interventions::after_cleanup(directory, metrics, report)?;
    result
}

#[allow(clippy::too_many_arguments)]
async fn observe_case(
    directory: &Path,
    request: &Request,
    metrics: &SharedMetrics,
    report: &mut Value,
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
    sessions: &AgentSessionManager,
    manager: &Arc<AgentRunManager>,
    approvals: &Arc<a3_application::AgentApprovalBuffer>,
    inspection: &Arc<a3_application::AgentInspectionBuffer>,
    observed_attempts: &interventions::SharedAttempts,
    started: Instant,
) -> WorkerResult<()> {
    let mut driver = interventions::Driver::new();
    let mut order_check = if request.case_id == "A10" {
        Some(interventions::A10OrderObserver::new(directory, request)?)
    } else {
        None
    };
    let session_id = driver
        .start_session(
            directory, request, report, store, project, sessions, manager,
        )
        .await?;
    let query = GetAgentApprovalCenter::new(
        store.clone(),
        store.clone(),
        store.clone(),
        approvals.clone(),
    );
    let approve = ControlAgentApproval::new(
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
        approvals.clone(),
    );
    let mut grants = report["exact_grants"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let mut recovery_check = if request.case_id == "A08" {
        Some(RecoveryCheck::new(directory)?)
    } else {
        None
    };
    let mut legacy_check = if request.case_id == "A12" {
        Some(interventions::LegacyFailureObserver::new(directory)?)
    } else {
        None
    };
    let mut last_report = Instant::now() - Duration::from_secs(1);
    loop {
        if started.elapsed() >= Duration::from_secs(request.timeout_seconds) {
            return Err(failure("deadline_exceeded"));
        }
        let detail = checked(
            store
                .load_session(project, session_id, None, SESSION_PAGE_LIMIT)
                .await,
            "session_read",
        )?
        .ok_or_else(|| failure("session_missing"))?;
        report["session_state"] = json!(format!("{:?}", detail.session().state()));
        report["phase"] = json!("running");
        if request.mode == AgentSessionMode::Ask {
            if matches!(
                detail.session().state(),
                AgentSessionState::Completed
                    | AgentSessionState::AwaitingUser
                    | AgentSessionState::Failed
                    | AgentSessionState::Cancelled
            ) {
                let answer = detail
                    .entries()
                    .iter()
                    .rev()
                    .find(|entry| entry.kind() != AgentSessionEntryKind::UserMessage)
                    .map(|entry| entry.text().as_str())
                    .unwrap_or("");
                let mut end = answer.len().min(MAX_ANSWER_BYTES);
                while !answer.is_char_boundary(end) {
                    end -= 1;
                }
                report["ask_answer"] = json!(&answer[..end]);
                report["ask_answer_truncated"] = json!(end != answer.len());
                report["formal_completed"] =
                    json!(detail.session().state() == AgentSessionState::Completed);
                if driver
                    .ask_finished(
                        directory, request, started, metrics, report, store, project, sessions,
                        session_id, &detail,
                    )
                    .await?
                {
                    continue;
                }
                report["phase"] = json!("terminal");
                return Ok(());
            }
        } else if let Some(work_item) = detail.session().active_work_item() {
            let stored = checked(
                store.load_task_ledger(project, work_item.task_id()).await,
                "ledger_read",
            )?
            .ok_or_else(|| failure("ledger_missing"))?;
            let run_id = stored
                .ledger()
                .steps()
                .filter_map(|step| step.attempts().last())
                .map(a3_domain::TaskStepAttempt::run_id)
                .next()
                .ok_or_else(|| failure("run_missing"))?;
            let run = checked(store.load_agent_run(project, run_id).await, "run_read")?
                .ok_or_else(|| failure("run_missing"))?;
            report["run_state"] = json!(format!("{:?}", run.state()));
            if run.state().is_terminal() {
                report["formal_completed"] = json!(run.state() == AgentControllerState::Done);
            }
            // Run state is committed before the product's public report. Observe
            // its natural owner completion instead of cancelling that last write.
            if run.state().is_terminal() && manager.activity().state().owns_live_worker() {
                if last_report.elapsed() >= Duration::from_secs(1) {
                    report["phase"] = json!("awaiting_public_report");
                    report["metrics"] = metrics_json(metrics);
                    report["elapsed_millis"] = json!(started.elapsed().as_millis());
                    atomic_report(directory, report)?;
                    last_report = Instant::now();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
            report["ledger"] = json!({"revision":stored.ledger().revision().get(),"replans":stored.ledger().replans().len(),
                "steps":stored.ledger().steps().filter(|step|step.is_active_plan_step()).map(|step|format!("{:?}",step.status())).collect::<Vec<_>>(),
                "verification":stored.ledger().steps().filter(|step|step.is_active_plan_step()).map(|step| {
                    let verification = step.attempts().last().and_then(a3_domain::TaskStepAttempt::verification);
                    json!({"method":format!("{:?}",step.definition().verification_spec().target().method()),
                        "acceptance_criteria_count":step.definition().acceptance_criteria().len(),
                        "passed":verification.map(a3_domain::StepVerification::passed),
                        "evidence_count":verification.map_or(0, |result|result.evidence_ids().len())})
                }).collect::<Vec<_>>()});
            report["run_usage"] = json!({"turns":run.usage().turn_count(),"prompt_tokens":run.usage().prompt_tokens(),
                "output_tokens":run.usage().output_tokens(),"actions":run.usage().action_count(),"repairs":run.usage().repair_count()});
            if run.state().is_terminal()
                || (run.state() == AgentControllerState::AwaitApproval
                    && !manager.activity().state().owns_live_worker())
            {
                report["journal"] = tokio::time::timeout(
                    Duration::from_secs(10),
                    capture_journal(store, project, run_id),
                )
                .await
                .map_err(|_| failure("journal_deadline_exceeded"))??;
            }
            if let Some(check) = recovery_check.as_mut() {
                let attempts = checked(
                    store.load_agent_mutation_attempts(project, run_id).await,
                    "mutation_read",
                )?;
                let overview = checked(
                    inspection.overview(project, work_item.task_id()),
                    "process_inspection",
                )?;
                let published = checked(
                    store.latest_published_index(project, &IndexControl).await,
                    "published_index_read",
                )?
                .ok_or_else(|| failure("published_index_missing"))?;
                check.observe(directory, overview.as_ref(), &attempts, &published, report)?;
            }
            if let Some(check) = legacy_check.as_mut() {
                let attempts = checked(
                    store.load_agent_mutation_attempts(project, run_id).await,
                    "mutation_read",
                )?;
                let published = checked(
                    store.latest_published_index(project, &IndexControl).await,
                    "published_index_read",
                )?
                .ok_or_else(|| failure("published_index_missing"))?;
                check.observe(
                    directory,
                    inspection,
                    project,
                    work_item.task_id(),
                    &run,
                    &attempts,
                    &published,
                    report,
                )?;
            }
            if let Some(check) = order_check.as_mut() {
                let attempts = checked(
                    store.load_agent_mutation_attempts(project, run_id).await,
                    "mutation_read",
                )?;
                let published = checked(
                    store.latest_published_index(project, &IndexControl).await,
                    "published_index_read",
                )?
                .ok_or_else(|| failure("published_index_missing"))?;
                check.observe(
                    directory,
                    inspection,
                    project,
                    work_item.task_id(),
                    &run,
                    &attempts,
                    &published,
                    report,
                )?;
            }
            if driver
                .tick(
                    directory,
                    request,
                    started,
                    report,
                    store,
                    project,
                    sessions,
                    manager,
                    session_id,
                    work_item.task_id(),
                    stored.ledger(),
                    &run,
                    observed_attempts,
                )
                .await?
            {
                report["research_trace"] =
                    interventions::research_trace(store, project, session_id).await?;
                capture_agent_report(store, project, session_id, work_item.task_id(), report)
                    .await?;
                report["formal_completed"] = json!(run.state() == AgentControllerState::Done);
                report["journal"] = tokio::time::timeout(
                    Duration::from_secs(10),
                    capture_journal(store, project, run_id),
                )
                .await
                .map_err(|_| failure("journal_deadline_exceeded"))??;
                return Ok(());
            }
            let execution_failure = {
                let observed = lock_recovering_poison(observed_attempts);
                stopped_executor_failure(manager.activity().state(), &observed)
            };
            let activity = manager.activity();
            if activity.task_id() == Some(work_item.task_id())
                && settled_product_stop(activity.state(), detail.session().state())
            {
                let failed = activity.state()
                    == crate::agent_run_manager::AgentRunActivityState::Failed
                    || detail.session().state() == AgentSessionState::Failed;
                report["product_failed"] = json!(failed);
                report["product_halted"] = json!(!failed);
                report["executor_failure"] =
                    json!(execution_failure.map(|error| format!("{error:?}")));
                report["product_stop_reason"] = json!("settled_product_owner");
                report["research_trace"] =
                    interventions::research_trace(store, project, session_id).await?;
                capture_agent_report(store, project, session_id, work_item.task_id(), report)
                    .await?;
                driver
                    .finish(
                        directory,
                        request,
                        report,
                        store,
                        project,
                        work_item.task_id(),
                        stored.ledger(),
                        &run,
                    )
                    .await?;
                report["journal"] = tokio::time::timeout(
                    Duration::from_secs(10),
                    capture_journal(store, project, run_id),
                )
                .await
                .map_err(|_| failure("journal_deadline_exceeded"))??;
                return Ok(());
            }
            if run.state().is_terminal() {
                capture_agent_report(store, project, session_id, work_item.task_id(), report)
                    .await?;
                report["research_trace"] =
                    interventions::research_trace(store, project, session_id).await?;
                driver
                    .finish(
                        directory,
                        request,
                        report,
                        store,
                        project,
                        work_item.task_id(),
                        stored.ledger(),
                        &run,
                    )
                    .await?;
                report["formal_completed"] = json!(run.state() == AgentControllerState::Done);
                let evidence_ids = stored
                    .ledger()
                    .steps()
                    .filter(|step| step.is_active_plan_step())
                    .filter_map(|step| {
                        step.attempts()
                            .last()
                            .and_then(a3_domain::TaskStepAttempt::verification)
                    })
                    .flat_map(|verification| verification.evidence_ids().iter().copied())
                    .collect::<std::collections::BTreeSet<_>>();
                let selected = evidence_ids.iter().copied().take(128).collect::<Vec<_>>();
                if !selected.is_empty() {
                    let state = checked(
                        store
                            .load_verification_inspection_state(
                                project,
                                work_item.task_id(),
                                &selected,
                                Duration::from_secs(2),
                                &crate::DesktopBoundedReadControl::new(),
                            )
                            .await,
                        "verification_inspection",
                    )?;
                    report["product_verification"] = json!({"evidence_count":evidence_ids.len(),
                        "truncated":evidence_ids.len()>selected.len(),
                        "evidence":state.evidence().iter().map(|evidence|json!({
                            "kind":match evidence {
                                a3_domain::VerificationEvidence::Command(_)=>"command",
                                a3_domain::VerificationEvidence::Test(_)=>"test",
                                a3_domain::VerificationEvidence::Diff(_)=>"diff",
                                a3_domain::VerificationEvidence::Diagnostic(_)=>"diagnostic",
                                a3_domain::VerificationEvidence::UserConfirmation(_)=>"user_confirmation",
                                a3_domain::VerificationEvidence::LoopbackHttp(_)=>"loopback_http",
                            },
                            "fresh":a3_domain::EvidenceFreshness::evaluate(evidence,state.published_index())==a3_domain::EvidenceFreshness::Fresh
                        })).collect::<Vec<_>>(),
                        "external_oracle_evidence_imported":false});
                } else {
                    report["product_verification"] = json!({"evidence_count":0,"truncated":false,
                        "evidence":[],"external_oracle_evidence_imported":false});
                }
                let attempts = checked(
                    store.load_agent_mutation_attempts(project, run_id).await,
                    "mutation_read",
                )?;
                report["mutation_attempts"] = json!(attempts.iter().map(|attempt| json!({
                    "kind":format!("{:?}",attempt.kind()),
                    "status":format!("{:?}",attempt.tool_attempt().status()),
                    "application_state":format!("{:?}",attempt.disposition().application_state()),
                    "requires_reconciliation":attempt.disposition().requires_reconciliation(),
                    "requires_replan":attempt.disposition().requires_replan()
                })).collect::<Vec<_>>());
                report["formal_completed"] = json!(run.state() == AgentControllerState::Done);
                report["phase"] = json!("terminal");
                return Ok(());
            }
            if run.state() == AgentControllerState::AwaitApproval
                && !manager.activity().state().owns_live_worker()
            {
                let AgentApprovalLoadResult::Available(center) = checked(
                    query
                        .execute(
                            project,
                            work_item.task_id(),
                            now()?,
                            &crate::DesktopBoundedReadControl::new(),
                        )
                        .await,
                    "approval_read",
                )?
                else {
                    return Err(failure("approval_missing"));
                };
                if !scope_allows(request, center.presentation().action()) {
                    report["denied_scope"] = scope_summary(request, center.presentation().action());
                    return Err(failure("approval_scope_denied"));
                }
                if matches!(
                    center.presentation().action(),
                    AgentApprovalAction::Patch(_)
                ) {
                    driver
                        .before_patch(
                            directory,
                            request,
                            started,
                            metrics,
                            report,
                            store,
                            project,
                            stored.ledger(),
                            observed_attempts,
                            run_id,
                            center.presentation().action(),
                        )
                        .await?;
                    if let Some(check) = recovery_check.as_mut() {
                        check.before_patch_grant();
                    }
                    if let Some(check) = legacy_check.as_mut() {
                        check.before_patch_grant();
                    }
                }
                if grants >= MAX_APPROVALS || !center.can_allow_once() {
                    return Err(failure("approval_limit_or_unavailable"));
                }
                if let Some(check) = order_check.as_mut() {
                    let attempts = checked(
                        store.load_agent_mutation_attempts(project, run_id).await,
                        "mutation_read",
                    )?;
                    let published = checked(
                        store.latest_published_index(project, &IndexControl).await,
                        "published_index_read",
                    )?
                    .ok_or_else(|| failure("published_index_missing"))?;
                    check.before_approval(
                        directory,
                        inspection,
                        project,
                        work_item.task_id(),
                        &run,
                        center.presentation(),
                        &attempts,
                        &published,
                        report,
                    )?;
                }
                let grant_id = ApprovalId::from_bytes(identity()?);
                let granted = checked(
                    approve
                        .execute(
                            project,
                            work_item.task_id(),
                            center.presentation().revision(),
                            center.ledger_revision(),
                            center.ledger_store_version(),
                            AgentApprovalControlAction::AllowOnce,
                            AgentApprovalControlMetadata::new(
                                grant_id,
                                RunEventId::from_bytes(identity()?),
                                now()?,
                            ),
                            &crate::DesktopBoundedReadControl::new(),
                        )
                        .await,
                    "approval_grant",
                )?;
                if !matches!(
                    granted,
                    AgentApprovalControlResult::Applied(
                        AgentApprovalControlOutcome::GrantStored { .. }
                    )
                ) {
                    if driver
                        .stale_approval_result(request, report, store, project, run_id, &granted)
                        .await?
                    {
                        continue;
                    }
                    return Err(failure("approval_not_granted"));
                }
                let current = checked(
                    store.load_task_ledger(project, work_item.task_id()).await,
                    "ledger_read",
                )?
                .ok_or_else(|| failure("ledger_missing"))?;
                checked(
                    manager.start_attempt(
                        a3_application::AgentRunExecutionRequest::after_approval(
                            work_item.task_id(),
                            current.ledger().revision(),
                            current.version(),
                            grant_id,
                        ),
                    ),
                    "approval_continue",
                )?;
                grants += 1;
                report["exact_grants"] = json!(grants);
            }
        } else if matches!(
            detail.session().state(),
            AgentSessionState::AwaitingUser
                | AgentSessionState::Failed
                | AgentSessionState::Cancelled
                | AgentSessionState::Completed
        ) {
            report["research_trace"] =
                interventions::research_trace(store, project, session_id).await?;
            report["formal_completed"] = json!(false);
            report["phase"] = json!("terminal");
            return Ok(());
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            report["metrics"] = metrics_json(metrics);
            report["elapsed_millis"] = json!(started.elapsed().as_millis());
            atomic_report(directory, report)?;
            last_report = Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[test]
#[ignore = "explicit one-attempt local-model P0 worker; requires prepared A3_PLAN22_RUN_DIR; never CI"]
fn current_production_baseline() -> Result<(), Box<dyn Error>> {
    let directory = run_directory()?;
    // An already-started directory is never silently reused after a native failure.
    if directory.join("worker.json").exists() && !interventions::is_resume()? {
        return Err(Box::new(failure("run_already_started")));
    }
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let started = Instant::now();
    let mut report = interventions::initial_report(&directory, &metrics)?.unwrap_or_else(||json!({"version":1,"worker_attempt":1,"phase":"validating","terminal":false,
        "formal_completed":false,"oracle_evaluated":false,"engine":"current_production_source_guided",
        "interventions":{"implemented":false},"exact_grants":0}));
    atomic_report(&directory, &report)?;
    let result = (|| -> WorkerResult<()> {
        let path = regular_child(&directory, "request.json", false)?;
        if checked(fs::metadata(&path), "request_metadata")?.len() > MAX_REQUEST_BYTES {
            return Err(failure("request_too_large"));
        }
        let mut raw = Vec::new();
        checked(
            checked(fs::File::open(path), "request_open")?
                .take(MAX_REQUEST_BYTES + 1)
                .read_to_end(&mut raw),
            "request_read",
        )?;
        let request = parse_request(&raw)?;
        report["case_id"] = json!(request.case_id);
        report["variant"] = json!(request.variant);
        report["mode"] = json!(format!("{:?}", request.mode));
        report["model"] = json!(request.model);
        report["effect_scope"] = json!({"patch_path_count":request.allowed_write_paths.len(),
            "exact_process_count":request.allowed_processes.len(),
            "process_effect_path_count":request.process_effect_paths.len(),
            "exact_network_request_count":request.allowed_network_requests.len()});
        report["profile"] = json!({"context_limit":request.context_limit,"output_limit":request.output_limit,
            "temperature_milli":0,"top_p_milli":1000,"parallelism":1,"schema_grounding":"FormatFieldOnly","thinking":false,
            "derived_effective_num_ctx":request.context_limit,"num_ctx_derivation":"ollama_operational_floor_16384_for_profiles_at_most_16384_not_wire_observation"});
        let runtime = checked(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build(),
            "worker_runtime",
        )?;
        runtime.block_on(evaluate(&directory, &request, &metrics, &mut report))
    })();
    if report["phase"] != json!("restart_ready") || result.is_err() {
        report["terminal"] = json!(true);
        report["phase"] = json!("terminal");
    }
    report["worker_succeeded"] = json!(result.is_ok());
    report["elapsed_millis"] = json!(started.elapsed().as_millis());
    report["metrics"] = metrics_json(&metrics);
    if let Err(error) = &result {
        report["failure_code"] = json!(error.0);
        report["harness_status"] = json!(if error.0 == "intervention_not_implemented" {
            "harness_incomplete"
        } else {
            "worker_failure"
        });
    } else {
        report["harness_status"] = json!("completed");
    }
    atomic_report(&directory, &report)?;
    result.map_err(|error| Box::new(error) as Box<dyn Error>)
}

#[test]
fn stopped_executor_error_ends_measurement_even_when_durable_run_is_not_terminal() {
    use crate::agent_run_manager::AgentRunActivityState;
    let mut observed = interventions::AttemptObservations {
        completed: 1,
        last_error: Some(a3_application::AgentRunExecutionFailure::AnchorsChanged),
    };
    assert_eq!(
        stopped_executor_failure(AgentRunActivityState::Failed, &observed),
        observed.last_error
    );
    for state in [
        AgentRunActivityState::Running,
        AgentRunActivityState::Queued,
        AgentRunActivityState::Paused,
        AgentRunActivityState::Succeeded,
        AgentRunActivityState::Idle,
    ] {
        assert!(stopped_executor_failure(state, &observed).is_none());
    }
    observed.completed = 0;
    assert!(stopped_executor_failure(AgentRunActivityState::Failed, &observed).is_none());
    observed.completed = 1;
    observed.last_error = None;
    assert!(stopped_executor_failure(AgentRunActivityState::Failed, &observed).is_none());
    assert!(settled_product_stop(
        AgentRunActivityState::Failed,
        AgentSessionState::Running
    ));
    assert!(settled_product_stop(
        AgentRunActivityState::Succeeded,
        AgentSessionState::AwaitingUser
    ));
    assert!(settled_product_stop(
        AgentRunActivityState::Succeeded,
        AgentSessionState::Failed
    ));
    assert!(!settled_product_stop(
        AgentRunActivityState::Running,
        AgentSessionState::Failed
    ));
    assert!(!settled_product_stop(
        AgentRunActivityState::Succeeded,
        AgentSessionState::Completed
    ));
}

#[test]
fn public_report_selection_excludes_progress_and_other_task_reports() -> Result<(), Box<dyn Error>>
{
    let task = a3_domain::TaskId::from_bytes([1; 32]);
    let foreign = a3_domain::TaskId::from_bytes([2; 32]);
    let entry = |sequence, kind, task| -> Result<a3_domain::AgentSessionEntry, Box<dyn Error>> {
        Ok(a3_domain::AgentSessionEntry::try_new(
            a3_domain::AgentSessionId::from_bytes([3; 32]),
            a3_domain::AgentSessionSequence::new(sequence)?,
            kind,
            a3_domain::AgentSessionText::try_from_string("public fixture text".to_owned())?,
            a3_domain::AgentSessionTimestamp::from_unix_millis(1)?,
            Some(a3_domain::AgentWorkItemId::from_bytes([4; 32])),
            Some(task),
            None,
        )?)
    };
    let entries = vec![
        entry(1, AgentSessionEntryKind::FinalReport, task)?,
        entry(2, AgentSessionEntryKind::Activity, task)?,
        entry(3, AgentSessionEntryKind::FinalReport, foreign)?,
        entry(4, AgentSessionEntryKind::AssistantSummary, task)?,
    ];
    assert_eq!(
        public_agent_report(&entries, AgentSessionState::Completed, task)
            .map(|entry| entry.sequence().get()),
        Some(1)
    );
    assert_eq!(
        public_agent_report(&entries, AgentSessionState::AwaitingUser, task)
            .map(|entry| entry.sequence().get()),
        Some(4)
    );
    assert!(public_agent_report(&entries[1..3], AgentSessionState::Completed, task).is_none());
    Ok(())
}

#[test]
fn report_bound_preserves_terminal_result_and_marks_optional_history_loss()
-> Result<(), Box<dyn Error>> {
    let report = json!({"terminal":true,"formal_completed":false,"session_state":"Failed",
        "metrics":{"calls":256,"provider_prompt_tokens":1234,
            "records":(0..256).map(|_|json!({"bounded_fixture":"x".repeat(4096)})).collect::<Vec<_>>()},
        "journal":{"truncated":false,"records":[{"kind":"state_transition"}]}});
    let bytes = bounded_report_bytes(&report)?;
    assert!(bytes.len() <= MAX_REPORT_BYTES);
    let projection: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(projection["terminal"], json!(true));
    assert_eq!(projection["session_state"], json!("Failed"));
    assert_eq!(projection["metrics"]["calls"], json!(256));
    assert_eq!(projection["metrics"]["provider_prompt_tokens"], json!(1234));
    assert_eq!(projection["report_projection_truncated"], json!(true));
    assert!(
        projection["projection_records_omitted"]["metrics"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    assert_eq!(
        report["metrics"]["records"].as_array().map(Vec::len),
        Some(256)
    );
    assert!(bounded_report_bytes(&json!({"unbounded":"x".repeat(MAX_REPORT_BYTES+1)})).is_err());
    Ok(())
}

#[test]
fn failed_check_observation_requires_real_exit_before_edit_and_new_snapshot_after_edit()
-> Result<(), Box<dyn Error>> {
    use a3_application::AgentInspectionId;
    use a3_domain::{ProcessExit, ProcessTermination, SnapshotId};
    let mut check = RecoveryCheck {
        initial_hash: [0; 32],
        patch_authorized: false,
        failed: Default::default(),
        successful: Default::default(),
        seen: Default::default(),
    };
    let before = SnapshotId::from_bytes([1; 32]);
    let after = SnapshotId::from_bytes([2; 32]);
    let failed = ProcessTermination::Exited(ProcessExit::new(Some(1), false)?);
    let passed = ProcessTermination::Exited(ProcessExit::new(Some(0), true)?);
    check.record_process(
        AgentInspectionId::from_bytes([1; 32]),
        before,
        ProcessTermination::TimedOut,
        [0; 32],
        0,
    )?;
    assert!(check.failed.is_empty());
    check.record_process(
        AgentInspectionId::from_bytes([2; 32]),
        before,
        failed,
        [0; 32],
        0,
    )?;
    assert_eq!(check.failed.len(), 1);
    check.before_patch_grant();
    check.record_process(
        AgentInspectionId::from_bytes([3; 32]),
        before,
        failed,
        [0; 32],
        0,
    )?;
    assert_eq!(check.failed.len(), 1);
    check.record_process(
        AgentInspectionId::from_bytes([4; 32]),
        before,
        passed,
        [9; 32],
        1,
    )?;
    check.record_process(
        AgentInspectionId::from_bytes([5; 32]),
        after,
        passed,
        [9; 32],
        0,
    )?;
    check.record_process(
        AgentInspectionId::from_bytes([6; 32]),
        after,
        passed,
        [0; 32],
        1,
    )?;
    assert!(check.successful.is_empty());
    check.record_process(
        AgentInspectionId::from_bytes([7; 32]),
        after,
        passed,
        [9; 32],
        1,
    )?;
    assert_eq!(check.successful.len(), 1);
    assert!(check.current_success(after, [9; 32], Some([9; 32])));
    assert!(!check.current_success(after, [9; 32], Some([8; 32])));
    assert!(!check.current_success(after, [9; 32], None));
    // A later edit invalidates the previous green check, even if an outer oracle
    // could independently verify the new file. Both anchors must still match.
    assert!(!check.current_success(SnapshotId::from_bytes([3; 32]), [9; 32], Some([9; 32])));
    assert!(!check.current_success(after, [8; 32], Some([8; 32])));
    // A retained overview must not count the same process again.
    check.record_process(
        AgentInspectionId::from_bytes([7; 32]),
        after,
        passed,
        [9; 32],
        1,
    )?;
    assert_eq!(check.successful.len(), 1);
    Ok(())
}

#[test]
fn baseline_request_rejects_unknown_fields_profiles_paths_and_ask_writes()
-> Result<(), Box<dyn Error>> {
    let valid = json!({"version":2,"case_id":"A01","variant":"base","mode":"Agent","objective":"Fix increment.",
        "model":"ornith-1.5:9b","context_limit":8192,"output_limit":2048,"timeout_seconds":120,
        "allowed_write_paths":["increment.py"],"allowed_processes":[{"executable":"python","arguments":["-m","unittest"]}],
        "process_effect_paths":[],"allowed_network_requests":[]});
    let parse = |value: &Value| {
        parse_request(&serde_json::to_vec(value)?)
            .map_err(|error| Box::new(error) as Box<dyn Error>)
    };
    assert_eq!(parse(&valid)?.context_limit, 8192);
    for (field, bad) in [
        ("extra", json!(1)),
        ("version", json!(1)),
        ("mode", json!("Plan")),
        ("model", json!("cloud-model")),
        ("context_limit", json!(65536)),
        ("output_limit", json!(8192)),
        ("allowed_write_paths", json!(["../escape.py"])),
        ("allowed_write_paths", json!(["C:\\outside.py"])),
        (
            "allowed_write_paths",
            json!(["increment.py", "increment.py"]),
        ),
        (
            "allowed_processes",
            json!([{"executable":"cmd /c","arguments":[]}]),
        ),
    ] {
        let mut bad_request = valid.clone();
        bad_request[field] = bad;
        assert!(parse(&bad_request).is_err(), "{field}");
    }
    let mut ask = valid;
    ask["mode"] = json!("Ask");
    assert!(parse(&ask).is_err());
    ask["allowed_write_paths"] = json!([]);
    ask["allowed_processes"] = json!([]);
    assert!(parse(&ask).is_ok());
    let mut network = ask.clone();
    network["case_id"] = json!("A04");
    network["mode"] = json!("Agent");
    network["allowed_network_requests"] = json!([{"method":"GET","url":"http://127.0.0.1:18765/"}]);
    assert!(parse(&network).is_ok());
    network["allowed_network_requests"]
        .as_array_mut()
        .ok_or("network fixture")?
        .push(json!({"method":"GET","url":"http://127.0.0.1:8765/"}));
    assert!(parse(&network).is_ok());
    for url in [
        "http://localhost:18765/",
        "http://127.0.0.1:18766/",
        "http://127.0.0.1:18765/?q=x",
        "https://example.com/",
    ] {
        network["allowed_network_requests"][0]["url"] = json!(url);
        assert!(parse(&network).is_err());
    }
    let mut runtime = ask.clone();
    runtime["case_id"] = json!("A05");
    runtime["mode"] = json!("Agent");
    runtime["process_effect_paths"] = json!([".a3-probe-notes.json"]);
    assert!(parse(&runtime).is_ok());
    runtime["process_effect_paths"] = json!(["other.json"]);
    assert!(parse(&runtime).is_err());
    ask["objective"] = json!("ü".repeat(32 * 1024));
    assert_eq!(parse(&ask)?.objective.len(), 64 * 1024);
    ask["objective"] = json!("a".repeat(64 * 1024 + 1));
    assert!(parse(&ask).is_err());
    Ok(())
}

#[test]
fn baseline_directory_requires_exact_direct_child() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/plan22-runs");
    validate_run_child(&root, &root.join("one_attempt"))?;
    assert!(validate_run_child(&root, &root).is_err());
    assert!(validate_run_child(&root, &root.join("nested/attempt")).is_err());
    assert!(validate_run_child(&root, &root.with_file_name("elsewhere").join("attempt")).is_err());
    Ok(())
}

#[test]
fn exact_preapproved_argv_keeps_open_process_approval_and_denies_shell_or_scope_changes()
-> Result<(), Box<dyn Error>> {
    let request = parse_request(&serde_json::to_vec(&json!({
        "version":2,"case_id":"A01","variant":"development","mode":"Agent",
        "objective":"Fix the public fixture.","model":"ornith-1.5:9b",
        "context_limit":8192,"output_limit":2048,"timeout_seconds":360,
        "allowed_write_paths":["subject.py"],
        "allowed_processes":[{"executable":"python","arguments":["-B","-m","unittest","discover","-s","tests"]}],
        "process_effect_paths":[],"allowed_network_requests":[]
    }))?)?;
    let arguments = request.allowed_processes[0].arguments.clone();
    for mode in [ProcessExecutionMode::KnownSafe, ProcessExecutionMode::Open] {
        assert!(process_scope_allows(
            &request,
            "python",
            &arguments,
            &AgentApprovalWorkingDirectory::Root,
            mode,
            AgentApprovalNetworkScope::Denied
        ));
    }
    assert!(!process_scope_allows(
        &request,
        "python",
        &arguments,
        &AgentApprovalWorkingDirectory::Root,
        ProcessExecutionMode::Shell,
        AgentApprovalNetworkScope::Denied
    ));
    assert!(!process_scope_allows(
        &request,
        "other-python",
        &arguments,
        &AgentApprovalWorkingDirectory::Root,
        ProcessExecutionMode::Open,
        AgentApprovalNetworkScope::Denied
    ));
    let mut different = arguments.clone();
    different.push("unreviewed-file.py".to_owned());
    assert!(!process_scope_allows(
        &request,
        "python",
        &different,
        &AgentApprovalWorkingDirectory::Root,
        ProcessExecutionMode::Open,
        AgentApprovalNetworkScope::Denied
    ));
    let mut http_request = request.clone();
    http_request.allowed_network_requests = vec![AllowedNetworkRequest {
        method: "GET".to_owned(),
        url: "http://127.0.0.1:18765/".to_owned(),
    }];
    let machine = |kind, operation, target: &str| -> Result<AgentApprovalAction, Box<dyn Error>> {
        Ok(AgentApprovalAction::Machine {
            scope: a3_application::MachineEffectScope::new(
                kind,
                a3_domain::TaskStepId::from_bytes([1; 32]),
                a3_domain::PolicyResourceId::from_bytes([2; 32]),
                target.to_owned(),
                None,
                None,
            )?,
            operation,
        })
    };
    assert!(scope_allows(
        &http_request,
        &machine(
            a3_domain::AgentMutationKind::MachineNetwork,
            a3_domain::PathPolicyOperation::Read,
            "http://127.0.0.1:18765/"
        )?
    ));
    assert!(!scope_allows(
        &http_request,
        &machine(
            a3_domain::AgentMutationKind::MachineFile,
            a3_domain::PathPolicyOperation::Read,
            "http://127.0.0.1:18765/"
        )?
    ));
    assert!(!scope_allows(
        &http_request,
        &machine(
            a3_domain::AgentMutationKind::MachineNetwork,
            a3_domain::PathPolicyOperation::Read,
            "http://127.0.0.1:18766/"
        )?
    ));
    http_request.allowed_network_requests[0].method = "POST".to_owned();
    assert!(!scope_allows(
        &http_request,
        &machine(
            a3_domain::AgentMutationKind::MachineNetwork,
            a3_domain::PathPolicyOperation::Read,
            "http://127.0.0.1:18765/"
        )?
    ));
    Ok(())
}

#[test]
fn baseline_metrics_retain_failures_unknown_usage_and_dropped_calls() {
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    {
        let mut call = CallObservation::new(&metrics, "research", 100, 10);
        call.failed("admission_failed", "TimedOut");
    }
    {
        let mut call = CallObservation::new(&metrics, "edit", 200, 20);
        call.output(7);
        call.complete(a3_application::ModelProviderCompletion::new(
            a3_application::ModelFinishReason::Stop,
            a3_application::ModelProviderUsage::new(Some(40), None),
        ));
    }
    {
        let _dropped = CallObservation::new(&metrics, "read", 300, 30);
    }
    let report = metrics_json(&metrics);
    assert_eq!(report["calls"], 3);
    assert_eq!(report["completed"], 1);
    assert_eq!(report["stream_errors"], 2);
    assert_eq!(report["records"][0]["failure"], "TimedOut");
    assert_eq!(report["records"][2]["status"], "incomplete");
    assert!(report["provider_prompt_tokens"].is_null());
    assert!(report["provider_output_tokens"].is_null());
    assert_eq!(report["reported_prompt_tokens_sum"], 40);
    assert_eq!(report["missing_prompt_usage_calls"], 2);
    assert_eq!(report["missing_output_usage_calls"], 3);
}

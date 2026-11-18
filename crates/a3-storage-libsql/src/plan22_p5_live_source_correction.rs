//! Explicit Qwen 4k measurement of one actual correction after a controlled red check.

use super::super::super::middle_edit_tests::p4_live_edit::probe_projected_profile;
use super::*;
use a3_application::{
    CompiledEditPhaseRequest, EditPhaseReservationStore, ExecuteEditPhase, LanguageParseControl,
    LanguageParseControlError, ModelProvider, ModelRequestTimeout, ModelWireThinking,
    PhaseWireBudget,
};
use a3_domain::{
    AgentActionLimit, AgentRepairLimit, AgentRunBudget, AgentRunDurationLimit, AgentTokenLimit,
    AgentTurnLimit, FileRevision,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use serde_json::Value;
use std::io::Write;

const TAG: &str = "qwen38-4k:latest";
const ENV: &str = "A3_PLAN22_P5_CORRECTION_RUN_DIR";
const SCHEMA: &str = "a3.plan22.p5.controlled-correction.v1";
const TASK_MILLIS: u64 = 360_000;
const CONTEXT: u32 = 4_096;
const OUTPUT: u32 = 1_024;

#[test]
#[ignore = "explicit isolated Qwen 4k correction measurement with a supervisor-owned directory"]
fn live_ollama_4k_corrects_the_actual_guarded_red_after_reopen() -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = launch_root()?;
        write_once(
            &root,
            "native-start.json",
            json!({"schema": SCHEMA, "tag": TAG}),
        )?;
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_live(&root));
        write_once(
            &root,
            "native-final.json",
            json!({
                "schema": SCHEMA,
                "source_done_after_cold_reopen": result.is_ok(),
                "controlled_initial_red": true,
                "full_a01_claim": false,
            }),
        )?;
        result
    })
}

fn launch_root() -> TestResult<PathBuf> {
    let path = PathBuf::from(std::env::var_os(ENV).ok_or("correction launch directory required")?);
    if !path.is_absolute() || !fs::symlink_metadata(&path)?.file_type().is_dir() {
        return Err("correction launch directory must be an owned absolute directory".into());
    }
    let root = fs::canonicalize(&path)?;
    let attempt = root
        .parent()
        .ok_or("correction attempt directory missing")?;
    let base = fs::canonicalize(std::env::current_dir()?.join("target/plan22-p5-correction-runs"))?;
    if root.file_name().and_then(|name| name.to_str()) != Some("native")
        || attempt.parent() != Some(base.as_path())
        || fs::read_dir(&root)?.next().is_some()
    {
        return Err(
            "correction needs a fresh native directory under the dedicated attempt root".into(),
        );
    }
    let launch_path = attempt.join("launch.json");
    let info = fs::symlink_metadata(&launch_path)?;
    if !info.is_file() || info.len() > 8_192 {
        return Err("correction launch receipt missing or invalid".into());
    }
    let launch: Value = serde_json::from_slice(&fs::read(launch_path)?)?;
    if launch["schema"] != SCHEMA
        || launch["tag"] != TAG
        || launch["num_ctx"] != CONTEXT
        || launch["num_predict"] != OUTPUT
        || launch["task_model_calls"] != 1
        || launch["task_timeout_millis"] != TASK_MILLIS
    {
        return Err("correction launch does not match the pinned profile and scope".into());
    }
    Ok(root)
}

async fn run_live(root: &Path) -> TestResult {
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let profile = probe_projected_profile(root, &provider, TAG, CONTEXT, OUTPUT).await?;
    let live = LiveCorrection {
        provider,
        clock: Instant::now(),
        root: root.to_path_buf(),
    };
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    fs::create_dir(worktree.join("tests"))?;
    fs::write(worktree.join("pyproject.toml"), MANIFEST)?;
    fs::write(worktree.join("tests/__init__.py"), b"")?;
    fs::write(worktree.join("tests/test_behavior.py"), TEST_BODY)?;
    fs::write(worktree.join(SUBJECT_PATH), INITIAL)?;
    let fixture = Fixture {
        root: root.to_path_buf(),
        layout: StorageLayout::prepare(root.join("appdata"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let mut protected = inventory(&worktree)?;
    let budget = AgentRunBudget::new(
        AgentTurnLimit::new(12)?,
        AgentTokenLimit::new(45_000)?,
        AgentTokenLimit::new(24_000)?,
        AgentActionLimit::new(8)?,
        AgentRunDurationLimit::from_millis(TASK_MILLIS)?,
        AgentRepairLimit::new(1)?,
    );
    let run = exercise_guarded_red(&fixture, profile.clone(), budget).await?;
    write_once(
        root,
        "red-reopened.json",
        json!({"schema": SCHEMA, "run_id": hex(run.as_bytes()), "verification_failed": true}),
    )?;
    exercise_correction(&fixture, run, profile, Some(&live)).await?;
    let (_, subject) = protected
        .iter_mut()
        .find(|(path, _)| path == Path::new(SUBJECT_PATH))
        .ok_or("correction subject missing")?;
    let corrected = fs::read(worktree.join(SUBJECT_PATH))?;
    assert_ne!(corrected, INCORRECT);
    assert_ne!(corrected, INITIAL);
    *subject = corrected;
    assert_eq!(inventory(&worktree)?, protected);
    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let durable_run = reopened
        .load_agent_run(&fixture.project, run)
        .await?
        .ok_or("corrected Run missing on the final cold read")?;
    if durable_run.state() != AgentControllerState::Done || durable_run.budget() != budget {
        return Err("corrected Run or original allowance changed on final reopen".into());
    }
    let usage = reopened
        .load_edit_phase_usage(&fixture.project, run)
        .await?;
    if (
        usage.calls,
        usage.repairs,
        usage.unknown_calls,
        usage.unobserved_calls,
    ) != (1, 0, 0, 0)
    {
        return Err("corrected Run lost the observed Edit settlement on reopen".into());
    }
    write_once(
        root,
        "cold-usage.json",
        json!({
            "schema": SCHEMA, "run_id": hex(run.as_bytes()), "calls": usage.calls, "repairs": usage.repairs,
            "unknown_calls": usage.unknown_calls, "unobserved_calls": usage.unobserved_calls,
            "prompt_charged": usage.prompt_tokens, "output_charged": usage.output_tokens,
            "deadline_millis": durable_run.created_at().unix_millis() + durable_run.budget().duration_limit().millis(),
        }),
    )?;
    Ok(())
}

#[derive(Debug)]
pub(super) struct LiveCorrection {
    provider: OllamaModelProvider,
    clock: Instant,
    root: PathBuf,
}

impl LiveCorrection {
    pub(super) fn observed_at(&self, minimum: u64) -> TestResult<AgentRunTimestamp> {
        at(3_u64
            .checked_add(u64::try_from(self.clock.elapsed().as_millis())?)
            .ok_or("correction clock overflow")?
            .max(minimum))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn edit(
        &self,
        store: &LibsqlKnowledgeStore,
        project: &ProjectIdentity,
        patch: &WorkspacePatchAdapter,
        phase: PreparedEditPhase,
        state: EditPhaseState<'_>,
        compiled: &mut CompiledEditPhaseRequest,
        now: AgentRunTimestamp,
    ) -> TestResult<AgentAction> {
        let phase = Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
            phase,
            compiled,
            state,
            now,
            patch,
            &ActiveControl,
        ))
        .await?;
        let run = state.anchor().run();
        let before = store.load_edit_phase_usage(project, run).await?;
        if before.calls != 0 {
            return Err("controlled correction already consumed a model call".into());
        }
        let projection = self.provider.project_wire_request(compiled.request())?;
        let budget = PhaseWireBudget::fit(projection, compiled.request().profile())?;
        if projection.context_tokens() != CONTEXT
            || projection.output_tokens() != OUTPUT
            || projection.thinking() != ModelWireThinking::Disabled
        {
            return Err("correction wire violates the frozen profile".into());
        }
        write_once(
            &self.root,
            "edit-intent.json",
            json!({
                "schema": SCHEMA,
                "run_id": hex(run.as_bytes()),
                "wire_blake3": hex(projection.digest().as_bytes()),
                "job_blake3": hex(phase.job_digest().as_bytes()),
                "request_bytes": projection.request_bytes(),
                "num_ctx": CONTEXT, "num_predict": OUTPUT, "think": false,
                "prompt_reserved": budget.prompt_tokens(), "output_reserved": budget.output_tokens(),
                "observed_at_millis": now.unix_millis(),
            }),
        )?;
        let result = ExecuteEditPhase::new(&self.provider, store, patch)
            .execute(
                phase,
                state,
                compiled,
                now,
                ModelRequestTimeout::from_millis(300_000)?,
                &ActiveControl,
            )
            .await;
        let durable = store.load_edit_phase_usage(project, run).await?;
        let finished = result.as_ref().ok();
        write_once(
            &self.root,
            "edit-result.json",
            json!({
                "schema": SCHEMA,
                "wire_blake3": finished.map(|report| hex(report.projection.digest().as_bytes())),
                "finish": finished.map(|_| "stop"),
                "prompt_tokens": finished.and_then(|report| report.usage.prompt_tokens()),
                "output_tokens": finished.and_then(|report| report.usage.output_tokens()),
                "failure": result.as_ref().err().map(|failure| format!("{failure:?}")),
                "calls": durable.calls, "repairs": durable.repairs, "unknown_calls": durable.unknown_calls,
                "unobserved_calls": durable.unobserved_calls, "prompt_charged": durable.prompt_tokens,
                "output_charged": durable.output_tokens,
            }),
        )?;
        let result = result?;
        if result.projection != projection
            || durable.calls != 1
            || durable.repairs != 0
            || durable.unknown_calls != 0
            || durable.unobserved_calls != 0
            || durable.prompt_tokens != u64::from(budget.prompt_tokens())
            || durable.output_tokens != u64::from(budget.output_tokens())
            || matches!(result.usage.prompt_tokens(), None | Some(0))
            || matches!(result.usage.output_tokens(), None | Some(0))
        {
            return Err("correction did not settle exactly one observed call".into());
        }
        let EditPhaseOutcome::Proposal(action) = result.outcome else {
            return Err("actual model did not propose a bounded correction".into());
        };
        Ok(action)
    }

    pub(super) fn require_safe_source(
        &self,
        required: &PythonIntegerTranslation,
        hash: ContentHash,
        source: &[u8],
        deadline: AgentRunTimestamp,
    ) -> TestResult {
        let verifier = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
        let revision = FileRevision::new(required.path().clone(), hash);
        if verifier
            .prove_python_integer_translation(
                required,
                &revision,
                source,
                &ApprovalControl(self, deadline),
            )?
            .is_none()
        {
            return Err("fixture approval refused unverified Python source".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ApprovalControl<'a>(&'a LiveCorrection, AgentRunTimestamp);

impl LanguageParseControl for ApprovalControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.observed_at(0).map_or(true, |now| now >= self.1)
    }

    fn report_progress(&self, _: Progress) -> Result<(), LanguageParseControlError> {
        if self.is_cancelled() {
            return Err(LanguageParseControlError::Unavailable);
        }
        Ok(())
    }
}

fn hex(digest: &[u8; 32]) -> String {
    blake3::Hash::from_bytes(*digest).to_hex().to_string()
}

fn write_once(root: &Path, name: &str, value: Value) -> TestResult {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(name))?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

use super::*;
use crate::agent_conversation_runtime::AgentConversationRuntime;
use crate::index_test_support as support;
use a3_application::{
    AdvanceAgentController, AgentApprovalAction, AgentApprovalBuffer, AgentApprovalControlAction,
    AgentApprovalControlMetadata, AgentApprovalControlResult, AgentApprovalLoadResult,
    AgentControllerControl, AgentControllerSignal, AgentInspectionBuffer, AgentRunExecutionRequest,
    AppendRunEvent, ContextCompileControl, ContextCompilePhase, ControlAgentApproval,
    CreateAgentRun, CreateGoalContract, CreateTaskLedger, DesktopSettingsStore,
    GetAgentApprovalCenter, JobClock, JobCompletion, JobContext, JobEventKind, JobScheduler,
    JobSchedulerConfig, JobTimestamp, KnowledgeStore, ModelFinishReason, ModelOperationControl,
    ModelOutputChunk, ModelProvider, ModelProviderCompletion, ModelProviderFuture,
    ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout, ProviderEvent,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryRescanReason, TaskLedgerStore, TaskLensWorkspaceControl,
};
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentControllerState,
    AgentRun, AgentRunId, AgentRunTimestamp, DeferredCommandVerification, DiffInvariantMode,
    DiffInvariantVerification, DiscoveredCommandKind, ExpectedTaskEvidence, GoalContract,
    GoalContractDraft, GoalContractTimestamp, GoalObjective, JobId, JobOwner, ModelCapabilities,
    ModelContextLimit, ModelId, ModelOutputLimit, ModelParallelismLimit, ModelProfile,
    ModelProfileSettings, ModelPromptSchemaGrounding, ModelProviderId, ModelSamplingProfile,
    ModelStopSequences, ModelStructuredOutputCapability, ModelTemperature,
    ModelTokenCountingStrategy, ModelToolCallMode, ModelTopP, Progress, RunEventId,
    SuccessVerification, TaskId, TaskLedger, TaskLedgerTimestamp, TaskStepDefinition, TaskStepId,
    TaskStepOutcome, TaskStepRationale, TaskStepStatus, VerificationRequirement, VerificationScope,
    VerificationSpec, VerificationSpecId,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SERVER: &str = r#"from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def response_for(method: str, path: str) -> tuple[int, bytes, dict[str, str]]:
    if method != "GET":
        return 405, b"Method Not Allowed\n", {"Allow": "GET"}
    if path != "/":
        return 404, b"Not Found\n", {}
    return 200, b"Hello, world!\n", {"Content-Type": "text/plain; charset=utf-8"}


class Handler(BaseHTTPRequestHandler):
    def _respond(self) -> None:
        status, body, headers = response_for(self.command, self.path)
        self.send_response(status)
        for name, value in headers.items():
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = _respond
    do_POST = _respond

    def log_message(self, format: str, *args: object) -> None:
        return


def main() -> None:
    ThreadingHTTPServer(("127.0.0.1", 8000), Handler).serve_forever()


if __name__ == "__main__":
    main()
"#;

const TESTS: &str = r#"import unittest

from server import response_for


class ServerTests(unittest.TestCase):
    def test_root_returns_hello_world(self):
        status, body, headers = response_for("GET", "/")
        self.assertEqual(status, 200)
        self.assertEqual(body, b"Hello, world!\n")
        self.assertEqual(headers["Content-Type"], "text/plain; charset=utf-8")

    def test_unknown_path_returns_404(self):
        self.assertEqual(response_for("GET", "/missing")[0], 404)

    def test_post_returns_405_and_allow_header(self):
        status, _, headers = response_for("POST", "/")
        self.assertEqual(status, 405)
        self.assertEqual(headers["Allow"], "GET")


if __name__ == "__main__":
    unittest.main()
"#;

#[derive(Debug)]
struct ScriptedGreenfieldProvider {
    provider_id: ModelProviderId,
    responses: Mutex<VecDeque<String>>,
    calls: AtomicUsize,
}

impl ModelProvider for ScriptedGreenfieldProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let raw = self
                .responses
                .lock()
                .map_err(|_| a3_application::ModelProviderFailure::Unavailable)?
                .pop_front()
                .ok_or(a3_application::ModelProviderFailure::InvalidResponse)?;
            let events = vec![
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(raw)
                        .map_err(|_| a3_application::ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(100), Some(20)),
                ))),
            ];
            Ok(Box::pin(futures::stream::iter(events)) as a3_application::ProviderEventStream<'a>)
        })
    }
}

#[derive(Debug)]
struct FixedClock;

impl JobClock for FixedClock {
    fn now(&self) -> JobTimestamp {
        JobTimestamp::from_millis(1)
    }
}

#[test]
fn empty_project_creates_files_binds_tests_and_reaches_done_with_real_unittest()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let data = support::TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?).await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let indexed = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?,
                &TestControl,
            )
            .await?;
        assert!(
            indexed
                .published_index()
                .publication()
                .graph()
                .files()
                .is_empty()
        );

        let profile = model_profile()?;
        let criterion = AcceptanceCriterionId::from_bytes([1; 32]);
        let task_id = TaskId::from_bytes([2; 32]);
        let change_id = TaskStepId::from_bytes([3; 32]);
        let test_id = TaskStepId::from_bytes([4; 32]);
        let run_id = AgentRunId::from_bytes([5; 32]);
        let goal = GoalContract::initial(
            task_id,
            GoalContractDraft::new(
                GoalObjective::try_from_string(
                    "create a tested Hello World Python server".to_owned(),
                )?,
                vec![AcceptanceCriterion::new(
                    criterion,
                    AcceptanceCriterionStatement::try_from_string(
                        "the server files exist and all local tests pass".to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    "run the discovered unittest suite".to_owned(),
                )?,
            )?,
            GoalContractTimestamp::from_unix_millis(now()?.unix_millis())?,
        );
        let change = TaskStepDefinition::new(
            change_id,
            None,
            TaskStepOutcome::try_from_string("create the server and its tests".to_owned())?,
            TaskStepRationale::try_from_string("the project starts empty".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "new source files".to_owned(),
            )?],
            VerificationSpec::diff_invariant(
                VerificationSpecId::from_bytes([6; 32]),
                VerificationRequirement::try_from_string(
                    "the patch creates project files".to_owned(),
                )?,
                DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
            ),
        )?
        .with_acceptance_criteria(vec![criterion])?;
        let test = TaskStepDefinition::new(
            test_id,
            None,
            TaskStepOutcome::try_from_string("run the created project tests".to_owned())?,
            TaskStepRationale::try_from_string(
                "behavior requires executable verification".to_owned(),
            )?,
            vec![a3_domain::StepDependency::new(change_id)],
            vec![ExpectedTaskEvidence::try_from_string(
                "structured unittest cases".to_owned(),
            )?],
            VerificationSpec::deferred_command(
                VerificationSpecId::from_bytes([7; 32]),
                VerificationRequirement::try_from_string("all tests pass".to_owned())?,
                DeferredCommandVerification::new(
                    vec![DiscoveredCommandKind::Test],
                    VerificationScope::Workspace,
                )?,
            ),
        )?
        .with_acceptance_criteria(vec![criterion])?;
        let mut ledger = TaskLedger::new(
            goal.reference(),
            vec![change, test],
            TaskLedgerTimestamp::from_unix_millis(now()?.unix_millis())?,
        )?;
        ledger.start_step(
            change_id,
            run_id,
            TaskLedgerTimestamp::from_unix_millis(now()?.unix_millis())?,
        )?;
        CreateGoalContract::new(store.as_ref())
            .execute(&project, &goal)
            .await?;
        let version = CreateTaskLedger::new(store.as_ref())
            .execute(&project, &ledger)
            .await?
            .version();
        let (mut run, start) = AgentRun::start(
            run_id,
            goal.reference(),
            ledger.revision(),
            profile.reference(),
            indexed.published_index().run().snapshot_id(),
            RunEventId::from_bytes([8; 32]),
            now()?,
        )?;
        CreateAgentRun::new(store.as_ref())
            .execute(&project, &run, &start)
            .await?;
        for (offset, signal) in [
            AgentControllerSignal::AnchorsAccepted,
            AgentControllerSignal::LocalizationComplete,
            AgentControllerSignal::PlanReady,
        ]
        .into_iter()
        .enumerate()
        {
            let expected = run.last_event_sequence();
            let snapshot = run.current_snapshot_id();
            let advance = AdvanceAgentController.execute(
                &mut run,
                signal,
                RunEventId::from_bytes([9 + u8::try_from(offset)?; 32]),
                snapshot,
                now()?,
                false,
            )?;
            AppendRunEvent::new(store.as_ref())
                .execute(&project, expected, &run, advance.event())
                .await?;
        }

        let patch = serde_json::json!({
            "schema_version": 5,
            "action": {
                "kind": "apply_patch",
                "run_id": hex(run_id.as_bytes()),
                "worktree_id": hex(project.worktree().id().as_bytes()),
                "snapshot_id": hex(indexed.published_index().run().snapshot_id().as_bytes()),
                "step_id": hex(change_id.as_bytes()),
                "verification_spec_id": hex(VerificationSpecId::from_bytes([6; 32]).as_bytes()),
                "rationale": "create the requested server and executable regression tests",
                "operations": [
                    {"kind":"add", "path":"server.py", "content":SERVER},
                    {"kind":"add", "path":"tests/__init__.py", "content":"# test package\n"},
                    {"kind":"add", "path":"tests/test_server.py", "content":TESTS}
                ]
            }
        })
        .to_string();
        let provider = Arc::new(ScriptedGreenfieldProvider {
            provider_id: profile.provider_id().clone(),
            responses: Mutex::new(VecDeque::from([
                patch,
                serde_json::json!({"schema_version":5,"action":{"kind":"finish"}}).to_string(),
            ])),
            calls: AtomicUsize::new(0),
        });
        let runtime = AgentConversationRuntime::new(
            store.clone() as Arc<dyn DesktopSettingsStore>,
            Arc::new(a3_credentials::NativeProviderCredentialStore::new()),
        )
        .with_execution_override(provider.clone(), profile);
        let inspection = Arc::new(AgentInspectionBuffer::new());
        inspection.activate_project(&project);
        let approvals = Arc::new(AgentApprovalBuffer::new());
        approvals.activate_project(&project);
        let executor = Arc::new(ProductionAgentRunExecutor::new(
            ProductionAgentRunPorts {
                workspace: store.clone(),
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
                research: None,
            },
            runtime,
            inspection,
            approvals.clone(),
            None,
        )?);
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
            approvals,
        );
        let mut request = AgentRunExecutionRequest::new(task_id, ledger.revision(), version);
        for attempt in 0..3u8 {
            run_attempt(executor.clone(), project.clone(), request)?;
            let observed_run = store.load_agent_run(&project, run_id).await?.ok_or("run")?;
            if observed_run.state() == AgentControllerState::Done {
                break;
            }
            assert_eq!(observed_run.state(), AgentControllerState::AwaitApproval);
            let AgentApprovalLoadResult::Available(center) = query
                .execute(&project, task_id, now()?, &TestControl)
                .await?
            else {
                return Err("missing exact approval".into());
            };
            if attempt == 0 {
                assert!(matches!(
                    center.presentation().action(),
                    AgentApprovalAction::Patch(_)
                ));
            } else {
                let AgentApprovalAction::Process(process) = center.presentation().action() else {
                    return Err("expected process approval".into());
                };
                assert_eq!(process.executable(), "python");
                assert_eq!(process.arguments(), ["-B", "-m", "unittest", "discover"]);
            }
            let approval_id = a3_domain::ApprovalId::from_bytes([20 + attempt; 32]);
            let result = approve
                .execute(
                    &project,
                    task_id,
                    center.presentation().revision(),
                    center.ledger_revision(),
                    center.ledger_store_version(),
                    AgentApprovalControlAction::AllowOnce,
                    AgentApprovalControlMetadata::new(
                        approval_id,
                        RunEventId::from_bytes([30 + attempt; 32]),
                        now()?,
                    ),
                    &TestControl,
                )
                .await?;
            assert!(matches!(result, AgentApprovalControlResult::Applied(_)));
            let stored = store
                .load_task_ledger(&project, task_id)
                .await?
                .ok_or("ledger")?;
            request = AgentRunExecutionRequest::after_approval(
                task_id,
                stored.ledger().revision(),
                stored.version(),
                approval_id,
            );
        }

        let final_run = store
            .load_agent_run(&project, run_id)
            .await?
            .ok_or("final run")?;
        let final_ledger = store
            .load_task_ledger(&project, task_id)
            .await?
            .ok_or("final ledger")?;
        assert_eq!(final_run.state(), AgentControllerState::Done);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        assert!(
            final_ledger
                .ledger()
                .steps()
                .filter(|step| step.is_active_plan_step())
                .all(|step| step.status() == TaskStepStatus::Completed)
        );
        assert_eq!(
            std::fs::read_to_string(repository.path().join("server.py"))?,
            SERVER
        );
        assert_eq!(
            std::fs::read_to_string(repository.path().join("tests/test_server.py"))?,
            TESTS
        );
        Ok(())
    })
}

fn run_attempt(
    executor: Arc<ProductionAgentRunExecutor>,
    project: a3_domain::ProjectIdentity,
    request: AgentRunExecutionRequest,
) -> Result<(), Box<dyn Error>> {
    let (scheduler, events) =
        JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixedClock))?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    scheduler.submit(
        JobId::new(1),
        JobOwner::new(1),
        move |control: JobContext| {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| a3_application::AgentRunExecutionFailure::Unavailable)
                .and_then(|runtime| {
                    runtime.block_on(executor.execute(&project, request, &control))
                });
            let succeeded = result.is_ok();
            let _sent = send.send(result);
            if succeeded {
                JobCompletion::Succeeded
            } else {
                JobCompletion::Failed
            }
        },
    )?;
    loop {
        let event = events
            .next_timeout(Duration::from_secs(30))?
            .ok_or("agent attempt timed out")?;
        if matches!(
            event.kind(),
            JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
        ) {
            break;
        }
    }
    receive.recv_timeout(Duration::from_secs(1))??;
    Ok(())
}

fn model_profile() -> Result<ModelProfile, Box<dyn Error>> {
    Ok(ModelProfile::from_probe(
        ModelProviderId::try_from_string("greenfield-fixture".to_owned())?,
        ModelId::try_from_string("deterministic".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(16_384)?,
            ModelOutputLimit::new(4_096)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1_000)?,
            ),
            ModelStopSequences::empty(),
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    ))
}

fn now() -> Result<AgentRunTimestamp, Box<dyn Error>> {
    Ok(AgentRunTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?,
    )?)
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
struct TestControl;

impl AgentControllerControl for TestControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl ContextCompileControl for TestControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_phase(
        &self,
        _: ContextCompilePhase,
    ) -> Result<(), a3_application::TaskLensControlError> {
        Ok(())
    }
}

impl RepositoryIndexControl for TestControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl TaskLensWorkspaceControl for TestControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

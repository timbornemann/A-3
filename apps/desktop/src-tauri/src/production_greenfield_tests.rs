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
    greenfield_fixture(true, false, false, false, None)
}

#[test]
fn small_hello_server_ask_permissions_has_no_test_scaffolding() -> Result<(), Box<dyn Error>> {
    greenfield_fixture(false, false, false, false, None)
}

#[test]
fn small_hello_server_full_machine_applies_without_a_grant() -> Result<(), Box<dyn Error>> {
    greenfield_fixture(false, true, false, false, None)
}

#[test]
fn switching_to_full_machine_resumes_the_exact_pending_patch() -> Result<(), Box<dyn Error>> {
    greenfield_fixture(false, false, true, false, None)
}

#[test]
fn switching_back_to_ask_before_admission_keeps_the_pending_approval() -> Result<(), Box<dyn Error>>
{
    greenfield_fixture(false, false, true, true, None)
}

#[test]
fn full_machine_still_asks_before_unconfirmed_project_tests() -> Result<(), Box<dyn Error>> {
    greenfield_fixture(true, true, false, false, None)
}

#[test]
fn machine_recovery_observes_the_exact_file_in_both_modes_without_replaying_the_write()
-> Result<(), Box<dyn Error>> {
    for full in [false, true] {
        greenfield_fixture(false, full, false, false, Some(0))?;
    }
    Ok(())
}

#[test]
fn machine_recovery_restarts_at_each_durable_reconciliation_and_replan_boundary()
-> Result<(), Box<dyn Error>> {
    for phase in 1..=5 {
        greenfield_fixture(false, true, false, false, Some(phase))?;
    }
    Ok(())
}

const SMALL_SERVER: &str = r#"from http.server import BaseHTTPRequestHandler, HTTPServer
import sys

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"Hello World")

HTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 8000), Handler).serve_forever()
"#;

fn greenfield_fixture(
    with_tests: bool,
    full: bool,
    switch: bool,
    revoke: bool,
    recovery_phase: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let data = support::TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?).await?,
        );
        store.record_opened_project(&project).await?;
        use a3_application::AgentPermissionStore;
        if full {
            store
                .update_agent_permissions(
                    a3_domain::AgentPermissionRevision::INITIAL,
                    a3_domain::AgentPermissionMode::FullMachine,
                )
                .await?;
        }
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
        // Dependency order deliberately differs from lexical ID order at the UI boundary.
        let change_id = TaskStepId::from_bytes([6; 32]);
        let test_id = TaskStepId::from_bytes([4; 32]);
        let run_id = AgentRunId::from_bytes([5; 32]);
        let goal = GoalContract::initial(
            task_id,
            GoalContractDraft::new(
                GoalObjective::try_from_string(
                    (if with_tests { "create a tested Hello World Python server" } else { "create a small Hello World Python server; keine Tests" }).to_owned(),
                )?,
                vec![AcceptanceCriterion::new(
                    criterion,
                    AcceptanceCriterionStatement::try_from_string(
                        (if with_tests { "the server files exist and all local tests pass" } else { "the requested source file exists; runtime behavior is checked independently" }).to_owned(),
                    )?,
                )],
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SuccessVerification::try_from_string(
                    (if with_tests { "run the discovered unittest suite" } else { "verify the applied source change; do not invent runtime evidence" }).to_owned(),
                )?,
            )?,
            GoalContractTimestamp::from_unix_millis(now()?.unix_millis())?,
        );
        let change = TaskStepDefinition::new(
            change_id,
            None,
            TaskStepOutcome::try_from_string(
                (if with_tests {
                    "create the server and its tests"
                } else {
                    "create only the requested small server"
                })
                .to_owned(),
            )?,
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
            if with_tests {
                vec![change, test]
            } else {
                vec![change]
            },
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

        let source = if with_tests { SERVER } else { SMALL_SERVER };
        let mut operations = vec![serde_json::json!({"path":"server.py", "content":source})];
        if with_tests {
            operations.push(
                serde_json::json!({"path":"tests/__init__.py", "content":"# test package\n"}),
            );
            operations.push(serde_json::json!({"path":"tests/test_server.py", "content":TESTS}));
        }
        let patch_arguments = serde_json::json!({
            "version": 1,
            "parameters": { "rationale": if with_tests { "create requested server and explicitly requested tests" } else { "create only the requested small server" }, "operations": operations }
        }).to_string();
        let provider = Arc::new(ScriptedGreenfieldProvider {
            provider_id: profile.provider_id().clone(),
            responses: Mutex::new(VecDeque::from([
                serde_json::json!({"version":1,"choice":"patch_add"}).to_string(),
                patch_arguments,
                serde_json::json!({"version":1,"next":"verify"}).to_string(),
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
                ledgers: store.clone(),
                permissions: Some(store.clone()),
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
            inspection.clone(),
            approvals.clone(),
            None,
        )?);
        if let Some(phase) = recovery_phase {
            return machine_recovery_fixture(
                &project, store, executor, run, ledger, version, phase,
            )
            .await;
        }
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
        let mut ui_projections = Vec::new();
        for attempt in 0..3u8 {
            run_attempt(executor.clone(), project.clone(), request)?;
            let work_plan = a3_application::GetTaskLensTask::new(store.clone())
                .execute(&project, task_id, &crate::DesktopBoundedReadControl::new())
                .await?;
            let a3_application::TaskLensTaskLoadResult::Available(work_plan) = work_plan else {
                return Err("execution must expose a coherent work plan".into());
            };
            ui_projections.push(serde_json::json!({
                "kind": "workPlan",
                "response": crate::map_task_lens_task_to_v1(&work_plan)
                    .map_err(|_| "invalid work-plan projection")?,
            }));
            let activity = a3_application::GetAgentActivity::new(store.clone(), store.clone())
                .execute(&project, task_id, &crate::DesktopBoundedReadControl::new())
                .await?;
            let a3_application::AgentActivityLoadResult::Available(activity) = activity else {
                return Err("execution must expose a coherent activity projection".into());
            };
            ui_projections.push(serde_json::json!({
                "kind": "activity",
                "response": a3_protocol::AgentActivityResponseV1::available(
                    crate::map_agent_activity_to_v1(&activity).ok_or("invalid activity projection")?,
                ),
            }));
            let verification =
                a3_application::GetTaskVerificationInspection::new(store.clone(), store.clone())
                    .execute(&project, task_id, &crate::DesktopBoundedReadControl::new())
                    .await?;
            let a3_application::TaskVerificationInspectionLoadResult::Available(verification) =
                verification
            else {
                return Err("execution must expose a coherent verification projection".into());
            };
            let overview = inspection.overview(&project, task_id)?;
            let overview = overview
                .as_ref()
                .filter(|value| crate::inspection_contexts_are_current(value, &verification));
            ui_projections.push(serde_json::json!({
                "kind": "inspection",
                "response": a3_protocol::AgentInspectionResponseV1::available(
                    crate::agent_inspection_mapping::map_agent_inspection_to_v1(overview, &verification),
                ),
            }));
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
            ui_projections.push(serde_json::json!({
                "kind": "approval",
                "response": a3_protocol::AgentApprovalResponseV1::new(
                    a3_protocol::AgentApprovalResultV1::Available {
                        approval: Box::new(crate::agent_approval_mapping::map_agent_approval_to_v1(&center).ok_or("legacy approval mapping")?),
                    },
                ),
            }));
            if attempt == 0 && !full {
                assert!(matches!(
                    center.presentation().action(),
                    AgentApprovalAction::Patch(_)
                ));
            } else {
                let AgentApprovalAction::Process(process) = center.presentation().action() else {
                    return Err("expected process approval".into());
                };
                assert_eq!(process.executable(), "python");
                assert_eq!(
                    process.arguments(),
                    ["-B", "-m", "unittest", "discover", "-s", "tests"]
                );
            }
            if switch && attempt == 0 {
                let settings = store.load_agent_permissions().await?;
                store
                    .update_agent_permissions(
                        settings.revision(),
                        a3_domain::AgentPermissionMode::FullMachine,
                    )
                    .await?;
                request = executor
                    .permission_change_request(&project)
                    .await?
                    .ok_or("missing permission wakeup")?;
                if revoke {
                    let settings = store.load_agent_permissions().await?;
                    store
                        .update_agent_permissions(
                            settings.revision(),
                            a3_domain::AgentPermissionMode::AskPermissions,
                        )
                        .await?;
                    run_attempt(executor.clone(), project.clone(), request)?;
                    assert!(!repository.path().join("server.py").exists());
                    assert_eq!(
                        store
                            .load_agent_run(&project, run_id)
                            .await?
                            .ok_or("run")?
                            .state(),
                        AgentControllerState::AwaitApproval
                    );
                    assert!(
                        executor
                            .permission_change_request(&project)
                            .await?
                            .is_none()
                    );
                    assert!(matches!(
                        query
                            .execute(&project, task_id, now()?, &TestControl)
                            .await?,
                        AgentApprovalLoadResult::Available(_)
                    ));
                    return Ok(());
                }
                continue;
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
        // Explicit opt-in exports only this deterministic, isolated fixture's public IPC
        // projections for the frontend decoder contract, never user repository state.
        if let Some(output) = std::env::var_os("A3_AGENT_UI_CONTRACT_OUTPUT") {
            std::fs::write(output, serde_json::to_vec_pretty(&ui_projections)?)?;
        }
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            if with_tests { 3 } else { 2 }
        );
        assert!(
            final_ledger
                .ledger()
                .steps()
                .filter(|step| step.is_active_plan_step())
                .all(|step| step.status() == TaskStepStatus::Completed)
        );
        assert_eq!(
            std::fs::read_to_string(repository.path().join("server.py"))?,
            source
        );
        if with_tests {
            assert_eq!(
                std::fs::read_to_string(repository.path().join("tests/test_server.py"))?,
                TESTS
            );
        } else {
            let entries = std::fs::read_dir(repository.path())?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(entries.len(), 2, "only .git and server.py may exist");
            assert!(!repository.path().join("tests").exists());
            assert!(!repository.path().join("pyproject.toml").exists());
            check_http_server(repository.path())?;
        }
        Ok(())
    })
}

async fn machine_recovery_fixture(
    project: &a3_domain::ProjectIdentity,
    store: Arc<LibsqlKnowledgeStore>,
    executor: Arc<ProductionAgentRunExecutor>,
    mut run: AgentRun,
    mut ledger: TaskLedger,
    mut version: a3_application::TaskLedgerStoreVersion,
    phase: u8,
) -> Result<(), Box<dyn Error>> {
    use a3_application::{
        AgentRecoveryChoice, AgentRecoveryStore, MachineEffectScope, MachineFileTool,
        ReconcileUnknownMutation, RecoverAgentRun,
    };
    use a3_domain::{
        AgentMutationDisposition, AgentMutationKind, MutationActionFingerprint,
        MutationReconciliation, ToolRunId,
    };
    let external = support::TempDirectory::new()?;
    let target = external.path().join("external.txt");
    let step = active_step_id(&ledger)?;
    let fingerprint = MutationActionFingerprint::from_bytes([51; 32]);
    let tool = ToolRunId::from_bytes([52; 32]);
    let body = b"observed external result";
    let scope = if phase == 0 {
        let files = a3_workspace::WorkspaceMachineFileTool::new(store.clone());
        let action = a3_domain::MachineFileAction::new(
            step,
            a3_domain::MachineFilePath::new(target.to_str().ok_or("path")?.to_owned())?,
            a3_domain::MachineFileOperation::Write {
                expected: None,
                content: a3_domain::PatchFileContent::try_from_bytes(body.to_vec())?,
            },
        )?;
        let prepared = files.prepare(project, &action, &ActiveMachineFiles).await?;
        let a3_domain::PolicyAction::MachineFile { resource_id, .. } = prepared.policy_action()
        else {
            return Err("scope".into());
        };
        let canonical = prepared.root().as_path().join("external.txt");
        MachineEffectScope::new(
            AgentMutationKind::MachineFile,
            step,
            *resource_id,
            canonical.to_str().ok_or("path")?.to_owned(),
            None,
            Some(a3_domain::ContentHash::from_bytes(
                *blake3::hash(body).as_bytes(),
            )),
        )?
    } else {
        MachineEffectScope::new(
            AgentMutationKind::MachineProcess,
            step,
            a3_domain::PolicyResourceId::from_bytes([53; 32]),
            "unknown-script.exe".to_owned(),
            None,
            None,
        )?
    };
    let attempt = store
        .begin_machine_mutation_attempt(
            project,
            run.id(),
            run.current_snapshot_id(),
            tool,
            fingerprint,
            &scope,
            now()?,
        )
        .await?;
    std::fs::write(&target, body)?; // Simulates the effect after durable begin but before its journal commit.
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let denied = ReconcileUnknownMutation::new(&executor.coordinator, store.as_ref(), &refresh)
        .execute(
            project,
            &mut run,
            tool,
            attempt.tool_attempt().attempt(),
            RunEventId::from_bytes([54; 32]),
            now()?,
            &mut compiler,
            &TestControl,
        )
        .await;
    assert!(
        matches!(
            denied,
            Err(a3_application::MutationReconciliationError::AttemptState)
        ),
        "a repository scan cannot reconcile a machine action"
    );
    if phase > 0 {
        store
            .acknowledge_machine_recovery(
                project,
                tool,
                attempt.tool_attempt().attempt(),
                fingerprint,
                scope.resource(),
                None,
                None,
                now()?,
            )
            .await?;
        ReconcileUnknownMutation::new(&executor.coordinator, store.as_ref(), &refresh)
            .execute(
                project,
                &mut run,
                tool,
                attempt.tool_attempt().attempt(),
                RunEventId::from_bytes([55; 32]),
                now()?,
                &mut compiler,
                &TestControl,
            )
            .await?;
    }
    if phase >= 2 {
        let recovered = RecoverAgentRun::new(
            store.as_ref(),
            store.as_ref(),
            store.as_ref(),
            store.as_ref(),
        )
        .execute(
            project,
            run.id(),
            AgentRecoveryChoice::Replan,
            RunEventId::from_bytes([56; 32]),
            now()?,
            &crate::DesktopBoundedReadControl::new(),
        )
        .await?;
        run = recovered.run().clone();
        (ledger, version) = recovered.ledger().clone().into_parts();
    }
    if phase >= 3 {
        let sequence = run.last_event_sequence();
        let snapshot = run.current_snapshot_id();
        let event = AdvanceAgentController.execute(
            &mut run,
            AgentControllerSignal::ExecutionNeedsReplan,
            RunEventId::from_bytes([57; 32]),
            snapshot,
            now()?,
            false,
        )?;
        AppendRunEvent::new(store.as_ref())
            .execute(project, sequence, &run, event.event())
            .await?;
        let reason = TaskReplanReason::try_from_string(
            "Explicit machine recovery requires a fresh plan".to_owned(),
        )?;
        let (retire, additions) = automatic_replan_steps(&ledger, &reason)?;
        version = ApplyAgentPlanRevision::new(store.as_ref())
            .execute(
                project,
                version,
                &mut run,
                &mut ledger,
                retire,
                additions,
                reason,
                RunEventId::from_bytes([58; 32]),
                now()?,
                &TestControl,
            )
            .await?;
    }
    for (phase_required, signal, id) in [
        (4, AgentControllerSignal::ReplanApplied, 59),
        (5, AgentControllerSignal::LocalizationComplete, 60),
    ] {
        if phase >= phase_required {
            let sequence = run.last_event_sequence();
            let snapshot = run.current_snapshot_id();
            let event = AdvanceAgentController.execute(
                &mut run,
                signal,
                RunEventId::from_bytes([id; 32]),
                snapshot,
                now()?,
                false,
            )?;
            AppendRunEvent::new(store.as_ref())
                .execute(project, sequence, &run, event.event())
                .await?;
        }
    }
    let shown = executor
        .machine_recovery_scope(project, ledger.goal_contract().task_id())
        .await?
        .ok_or("recovery after restart")?;
    assert_eq!(shown.scope, "33".repeat(32));
    let request = AgentRunExecutionRequest::after_machine_recovery(
        ledger.goal_contract().task_id(),
        ledger.revision(),
        version,
        fingerprint,
    );
    let run_id = run.id();
    // The query and job use durable state after dropping the abandoned attempt's in-memory anchors.
    let _discarded_run = run;
    drop(ledger);
    let (scheduler, events) =
        JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixedClock))?;
    let project_for_job = project.clone();
    let store_for_job = store.clone();
    let executor_for_job = executor.clone();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    scheduler.submit(
        JobId::new(101),
        JobOwner::new(1),
        move |context: JobContext| {
            let result = futures::executor::block_on(async {
                let ledger = store_for_job
                    .load_task_ledger(&project_for_job, request.task_id())
                    .await
                    .map_err(|_| a3_application::AgentRunExecutionFailure::Unavailable)?
                    .ok_or(a3_application::AgentRunExecutionFailure::InvalidState)?;
                executor_for_job
                    .machine_recovery_anchor(&project_for_job, ledger.ledger(), fingerprint)
                    .await?;
                let mut run = store_for_job
                    .load_agent_run(&project_for_job, run_id)
                    .await
                    .map_err(|_| a3_application::AgentRunExecutionFailure::Unavailable)?
                    .ok_or(a3_application::AgentRunExecutionFailure::InvalidState)?;
                executor_for_job
                    .recover_machine(
                        &project_for_job,
                        request,
                        &mut run,
                        fingerprint,
                        &AgentAttemptControl { context: &context },
                    )
                    .await
            });
            let success = result.is_ok();
            let _sent = send.send(result);
            if success {
                JobCompletion::Succeeded
            } else {
                JobCompletion::Failed
            }
        },
    )?;
    loop {
        let event = events
            .next_timeout(Duration::from_secs(30))?
            .ok_or("recovery job timeout")?;
        if matches!(
            event.kind(),
            JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
        ) {
            break;
        }
    }
    let (ledger, _, next) = receive.recv_timeout(Duration::from_secs(1))??;
    assert_eq!(
        ledger.step(next).ok_or("next")?.status(),
        TaskStepStatus::InProgress
    );
    assert_eq!(ledger.replans().len(), 1);
    assert_eq!(std::fs::read(&target)?, body);
    let attempts = store.load_agent_mutation_attempts(project, run_id).await?;
    assert_eq!(
        attempts.len(),
        1,
        "recovery must not retry the original action"
    );
    assert!(matches!(
        attempts[0].disposition(),
        AgentMutationDisposition::Unknown(MutationReconciliation::Replanned { .. })
    ));
    assert!(
        store
            .machine_recovery_acknowledged(project, tool, attempt.tool_attempt().attempt())
            .await?
    );
    assert!(
        executor
            .machine_recovery_scope(project, request.task_id())
            .await?
            .is_none()
    );
    Ok(())
}

#[derive(Debug)]
struct ActiveMachineFiles;
impl a3_application::MachineFileControl for ActiveMachineFiles {
    fn is_cancelled(&self) -> bool {
        false
    }
}

fn check_http_server(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::process::{Command, Stdio};
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let mut child = Command::new("python")
        .args(["-I", "-B", "server.py", &address.port().to_string()])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
                Ok(socket) => break socket,
                Err(_) if std::time::Instant::now() < deadline && child.try_wait()?.is_none() => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error.into()),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        socket.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
        let mut response = String::new();
        socket.take(4096).read_to_string(&mut response)?;
        if !response.starts_with("HTTP/1.0 200") || !response.ends_with("Hello World") {
            return Err("independent HTTP oracle rejected response".into());
        }
        Ok(())
    })();
    let _stop = child.kill();
    child.wait()?;
    result
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

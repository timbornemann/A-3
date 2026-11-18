//! Empty Greenfield inventory through the candidate Plan and its durable Ledger.

#[path = "plan22_p7_live_a04.rs"]
mod live_a04;

use super::super::super::p4_live_edit::probe_projected_profile;
use super::*;
use a3_application::{
    AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader, AgentSourceReaderFuture,
    ExecuteInitialPlan, InitialPlanExecutionFailure, InitialPlanExecutionOutcome,
    ModelFinishReason, ModelOperationControl, ModelOutputChunk, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture, ModelProviderRequest,
    ModelProviderUsage, ModelRequestTimeout, ModelWireProjection, ModelWireThinking,
    PlanPhaseFailure, ProviderEvent, ProviderEventStream,
};
use a3_domain::{
    AgentFileInspection, CriterionEvidenceRequirement, FileRevision, LOOPBACK_DEFAULT_PORT,
    LoopbackExpectedBody, LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe,
    LoopbackMethod, LoopbackProbe, LoopbackStart, ProjectIdentity, VerificationRequirement,
    VerificationSpec,
};
use a3_provider::{LocalOnlyOllamaEndpointPolicy, OllamaEndpoint, OllamaModelProvider};
use futures::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

const ORIGINAL: &[u8] = b"Create only README.md with a short project heading.\n";
const OUTCOME: &str = "Create README.md with a short project heading";
const INVENTORY_GUIDANCE: &str = "The current published index lists no files.";
const HTTP_ORIGINAL: &[u8] = b"Create only server.py with Python's standard library. Start it on 127.0.0.1:18765. GET /status returns 200, text/plain; charset=utf-8, and exactly 'ready' followed by LF. GET /missing returns 404. Verify both responses and stop the server.\n";
const A04_ORIGINAL_ENV: &str = "A3_PLAN22_P7_A04_ORIGINAL_PATH";
const A04_ORIGINAL_SHA256: &str =
    "656d4b30909ec4c40bfbd8750638a4e4d3b2bd32dd66fd6b3ef560d4166fa84c";
const A04_ORIGINAL_BYTES: usize = 1207;
const A04_SOURCE_EDGES: [usize; 5] = [0, 402, 475, 639, A04_ORIGINAL_BYTES];

fn greenfield_data() -> TestResult<IntakeFixture> {
    IntakeFixture::for_creation("README.md", ORIGINAL, None)
}

#[test]
fn empty_inventory_reopens_ready_plan() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture =
                Promoted::from_intake(greenfield_data()?, AgentRunBudget::DEFAULT).await?;
            let project = &fixture.data.fixture.project;
            let worktree = fixture.data.fixture.root.join("worktree");
            let task = fixture.data.goal.task_id();
            let step = fixture.data.step.id();
            assert!(inventory(&worktree)?.is_empty());
            assert!(fixture.published.publication().graph().files().is_empty());
            let phase = fixture.phase(step)?;
            let input = plan_view(phase.request())?;
            assert_eq!(input["inventory"], "emptyPublishedIndex");
            assert_eq!(input.get("source"), Some(&Value::Null));
            assert!(
                phase.request().messages()[0]
                    .content()
                    .contains(INVENTORY_GUIDANCE)
            );
            let provider = FixedPlanProvider {
                project,
                store: fixture.store.as_ref(),
                task,
                id: fixture.model.provider_id().clone(),
            };
            let report = ExecuteInitialPlan::new(
                &provider,
                fixture.store.as_ref(),
                fixture.store.as_ref(),
                &NoSourceReads,
                fixture.store.as_ref(),
            )
            .execute(
                project,
                &phase,
                INITIAL,
                at(6)?,
                ModelRequestTimeout::from_millis(5_000)?,
                &ActiveControl,
            )
            .await?;
            let InitialPlanExecutionOutcome::Materialized(ready) = report.outcome else {
                return Err("empty inventory Plan did not commit a Ready Ledger".into());
            };
            assert_eq!(ready.version(), TaskLedgerStoreVersion::INITIAL);
            assert_eq!(
                ready.ledger().goal_contract(),
                fixture.data.goal.reference()
            );
            assert_eq!(ready.ledger().steps().len(), 1);
            let first = ready.ledger().step(step).ok_or("ready step missing")?;
            assert_eq!(first.status(), TaskStepStatus::Ready);
            assert_eq!(first.definition().intended_outcome().as_str(), OUTCOME);
            assert!(first.attempts().is_empty());
            assert!(inventory(&worktree)?.is_empty());

            let Promoted { data, store, .. } = fixture;
            drop(store);
            let store = data.open().await?;
            let project = &data.fixture.project;
            let reopened = store.load_task_ledger(project, task).await?;
            assert_eq!(reopened, Some(ready));
            let usage = store.load_initial_plan_usage(project, task).await?;
            assert_eq!(
                (usage.calls, usage.repairs, usage.unobserved_calls),
                (1, 0, 0)
            );
            drop(store);
            data.fixture.cleanup()
        })
    })
}

#[test]
fn nonempty_publication_invalidates_old_plan() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture =
                Promoted::from_intake(greenfield_data()?, AgentRunBudget::DEFAULT).await?;
            let project = &fixture.data.fixture.project;
            let task = fixture.data.goal.task_id();
            let phase = fixture.phase(fixture.data.step.id())?;
            assert_eq!(
                plan_view(phase.request())?["inventory"],
                "emptyPublishedIndex"
            );
            let permit = fixture
                .store
                .reserve_initial_plan(project, &phase, INITIAL, wire(2)?, at(6)?)
                .await?;
            let PlanPhaseOutcome::Steps(plan) =
                phase.complete(fixture.state()?, fixed_reply(phase.request())?.as_bytes())?
            else {
                return Err("expected the original empty-inventory plan".into());
            };
            fs::write(
                fixture.data.fixture.root.join("worktree/notes.txt"),
                OTHER_FILE,
            )?;
            let (_, _, _, published) = index_fixture(&fixture.data.fixture).await?;
            assert_eq!(published.publication().graph().files().len(), 1);
            assert_ne!(published.run().snapshot_id(), phase.anchor().snapshot());
            let Promoted {
                data, store, model, ..
            } = fixture;
            let fixture = Promoted {
                data,
                store,
                published,
                model,
            };
            let project = &fixture.data.fixture.project;
            let current = fixture.phase(fixture.data.step.id())?;
            let input = plan_view(current.request())?;
            assert_eq!(input.get("source"), Some(&Value::Null));
            assert!(input.get("inventory").is_none());
            assert!(
                !current.request().messages()[0]
                    .content()
                    .contains(INVENTORY_GUIDANCE)
            );
            assert_eq!(
                fixture
                    .store
                    .finish_initial_plan(project, permit, accepted(plan), at(7)?)
                    .await,
                Err(PlanFailure::StalePlan)
            );
            assert!(
                fixture
                    .store
                    .load_task_ledger(project, task)
                    .await?
                    .is_none()
            );
            let usage = fixture.store.load_initial_plan_usage(project, task).await?;
            assert_eq!((usage.calls, usage.unobserved_calls), (1, 0));
            fixture.cleanup()
        })
    })
}

#[test]
#[ignore = "explicit local 4k Plan measurement; requires unused A3_PLAN22_P7_INVENTORY_RUN_DIR and exact A3_PLAN22_P7_INVENTORY_MODEL"]
fn live_ollama_empty_inventory_commits_one_ready_plan() -> TestResult {
    live_inventory_plan(None, None)
}

#[test]
#[ignore = "explicit local 4k HTTP Plan measurement; requires unused A3_PLAN22_P7_INVENTORY_RUN_DIR and exact A3_PLAN22_P7_INVENTORY_MODEL"]
fn live_ollama_empty_inventory_http_offer_reopens_ready() -> TestResult {
    let recipe = LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        std::num::NonZeroU16::new(18_765).ok_or("invalid fixture port")?,
        vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/status".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: ContentHash::from_bytes(*blake3::hash(b"ready\n").as_bytes()),
                    bytes: 6,
                },
                LoopbackHeaderChecks {
                    plain_utf8: true,
                    allow_get: false,
                },
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/missing".to_owned())?,
                404,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?,
        ],
    )?;
    live_inventory_plan(Some(recipe), None)
}

#[test]
#[ignore = "explicit local 4k frozen A04 Plan measurement; requires unused A3_PLAN22_P7_INVENTORY_RUN_DIR, A3_PLAN22_P7_INVENTORY_MODEL and A3_PLAN22_P7_A04_ORIGINAL_PATH"]
fn live_ollama_frozen_a04_development_aggregate_plan_reopens_ready() -> TestResult {
    live_inventory_plan(Some(frozen_a04_recipe()?), Some(A04_ORIGINAL_ENV))
}

fn frozen_a04_recipe() -> TestResult<LoopbackHttpRecipe> {
    let body = b"Hello, world!\n";
    let mut checks = vec![
        LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/".to_owned())?,
            200,
            LoopbackExpectedBody::Exact {
                hash: ContentHash::from_bytes(*blake3::hash(body).as_bytes()),
                bytes: u32::try_from(body.len())?,
            },
            LoopbackHeaderChecks {
                plain_utf8: true,
                allow_get: false,
            },
        )?,
        LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/missing".to_owned())?,
            404,
            LoopbackExpectedBody::AnyBounded,
            LoopbackHeaderChecks::default(),
        )?,
    ];
    for method in [
        LoopbackMethod::Post,
        LoopbackMethod::Put,
        LoopbackMethod::Delete,
        LoopbackMethod::Patch,
    ] {
        checks.push(LoopbackHttpCheck::new(
            LoopbackProbe::new(method, "/".to_owned())?,
            405,
            LoopbackExpectedBody::AnyBounded,
            LoopbackHeaderChecks {
                plain_utf8: false,
                allow_get: true,
            },
        )?);
    }
    let recipe = LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        std::num::NonZeroU16::new(18_765).ok_or("invalid fixture port")?,
        checks,
    )?
    .with_startup_coverage(0)?;
    Ok(recipe)
}

fn read_frozen_a04_original(env: &str) -> TestResult<Vec<u8>> {
    let path = std::path::PathBuf::from(
        std::env::var_os(env).ok_or("explicit frozen A04 original path required")?,
    );
    if !path.is_absolute() || !fs::metadata(&path)?.is_file() {
        return Err("frozen A04 original must be an absolute regular file".into());
    }
    let mut bytes = Vec::with_capacity(A04_ORIGINAL_BYTES + 1);
    fs::File::open(path)?
        .take((A04_ORIGINAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() != A04_ORIGINAL_BYTES
        || format!("{:x}", Sha256::digest(&bytes)) != A04_ORIGINAL_SHA256
    {
        return Err("frozen A04 original does not match its pinned development bytes".into());
    }
    Ok(bytes)
}

fn live_inventory_plan(
    http_recipe: Option<LoopbackHttpRecipe>,
    frozen_a04_env: Option<&str>,
) -> TestResult {
    crate::run_native_libsql_live_test(async {
        let root = std::path::PathBuf::from(
            std::env::var_os("A3_PLAN22_P7_INVENTORY_RUN_DIR")
                .ok_or("explicit inventory run directory required")?,
        );
        let tag = std::env::var("A3_PLAN22_P7_INVENTORY_MODEL")?;
        let frozen_a04 = frozen_a04_env.map(read_frozen_a04_original).transpose()?;
        if !root.is_absolute() || root.file_name().is_none() {
            return Err("inventory run directory must be absolute and unused".into());
        }
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_live_inventory(
                &root,
                &tag,
                http_recipe,
                frozen_a04.as_deref(),
            ))
    })
}

async fn run_live_inventory(
    root: &Path,
    tag: &str,
    http_recipe: Option<LoopbackHttpRecipe>,
    frozen_a04: Option<&[u8]>,
) -> TestResult {
    let worktree = root.join("worktree");
    fs::create_dir(&worktree)?;
    initialize_git(&worktree)?;
    let fixture = Fixture {
        root: root.to_owned(),
        layout: StorageLayout::prepare(root.join("app-data"))?,
        project: RepositoryInspector::new().inspect(&worktree)?,
    };
    let provider = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let model = probe_projected_profile(root, &provider, tag, 4_096, 1_024).await?;
    let fixture = promote_live_inventory(fixture, model, http_recipe.as_ref(), frozen_a04).await?;
    let project = &fixture.data.fixture.project;
    let task = fixture.data.goal.task_id();
    let step = fixture.data.step.id();
    assert!(inventory(&worktree)?.is_empty());
    assert!(fixture.published.publication().graph().files().is_empty());
    let criterion = fixture
        .data
        .goal
        .draft()
        .acceptance_criteria()
        .first()
        .ok_or("inventory criterion missing")?;
    let expected_check = if http_recipe.is_some() {
        "http"
    } else {
        "diff"
    };
    let expected_spec = if let Some(recipe) = http_recipe.as_ref() {
        assert!(matches!(
            criterion.origin().map(|origin| origin.evidence_requirement()),
            Some(CriterionEvidenceRequirement::LoopbackHttp(required)) if required == recipe
        ));
        VerificationSpec::loopback_http(
            fixture.data.step.verification_spec().id(),
            VerificationRequirement::try_from_string(
                if frozen_a04.is_some() {
                    "Verify explicit and default starts, required HTTP responses and both rejected starts"
                } else {
                    "Verify both owned loopback responses"
                }
                .to_owned(),
            )?,
            recipe.clone(),
        )
    } else {
        fixture.data.step.verification_spec().clone()
    };
    let phase = PreparedPlanPhase::new(
        fixture.state()?,
        vec![PlanStepOffer::new(
            step,
            Vec::new(),
            vec![criterion.id()],
            expected_spec.clone(),
        )],
        None,
    )?;
    let input = plan_view(phase.request())?;
    assert_eq!(input["inventory"], "emptyPublishedIndex");
    assert_eq!(input["offers"][0]["check"], expected_check);
    assert_eq!(input.get("source"), Some(&Value::Null));
    if frozen_a04.is_some() {
        let wire = provider.project_wire_request(phase.request())?;
        eprintln!(
            "Frozen A04 Plan wire: bytes={} messages={} schema={} context={} output={}",
            wire.request_bytes(),
            wire.message_bytes(),
            wire.format_bytes(),
            wire.context_tokens(),
            wire.output_tokens(),
        );
    }
    let observer = ObservedInventoryProvider(&provider);
    let executor = || {
        ExecuteInitialPlan::new(
            &observer,
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &NoSourceReads,
            fixture.store.as_ref(),
        )
    };
    let timeout = ModelRequestTimeout::from_millis(180_000)?;
    let first = executor()
        .execute(
            project,
            &phase,
            INITIAL,
            live_timestamp()?,
            timeout,
            &ActiveControl,
        )
        .await;
    let (report, repairs) = match first {
        Err(error) if http_recipe.is_some() && repairable_plan_reply(&error) => {
            eprintln!("first Plan disposition: {error:?}");
            let report = executor()
                .execute(
                    project,
                    &phase,
                    a3_application::EditPhaseCallKind::StructuralRepair,
                    live_timestamp()?,
                    timeout,
                    &ActiveControl,
                )
                .await
                .map_err(|error| format!("repaired Plan disposition: {error:?}"))?;
            (report, 1)
        }
        result => (
            result.map_err(|error| format!("first Plan disposition: {error:?}"))?,
            0,
        ),
    };
    let ready = match report.outcome {
        InitialPlanExecutionOutcome::Materialized(ready) => ready,
        InitialPlanExecutionOutcome::NeedEvidence { kind, .. } => {
            return Err(format!("live Greenfield Plan requested {kind:?} evidence").into());
        }
        InitialPlanExecutionOutcome::TooLarge => {
            return Err("live Greenfield Plan rejected the scope as too large".into());
        }
        InitialPlanExecutionOutcome::Halted(reason) => {
            return Err(format!("live Greenfield Plan halted: {reason:?}").into());
        }
    };
    assert_eq!(ready.version(), TaskLedgerStoreVersion::INITIAL);
    assert_eq!(ready.ledger().steps().len(), 1);
    let first = ready.ledger().step(step).ok_or("ready step missing")?;
    assert_eq!(first.status(), TaskStepStatus::Ready);
    assert_eq!(first.definition().verification_spec(), &expected_spec);
    assert_eq!(first.definition().acceptance_criteria(), &[criterion.id()]);
    assert!(first.attempts().is_empty());
    assert_eq!(report.projection.context_tokens(), 4_096);
    assert_eq!(report.projection.output_tokens(), 1_024);
    assert_eq!(report.projection.thinking(), ModelWireThinking::Disabled);

    let Promoted {
        data, store, model, ..
    } = fixture;
    drop(store);
    let store = data.open().await?;
    let project = &data.fixture.project;
    assert_eq!(store.load_task_ledger(project, task).await?, Some(ready));
    let goal = store
        .load_current_goal_contract(project, task)
        .await?
        .ok_or("goal missing after Plan reopen")?;
    assert_eq!(goal, data.goal);
    let frozen_scope = if let Some(bytes) = frozen_a04 {
        let recipe = http_recipe.as_ref().ok_or("frozen A04 recipe missing")?;
        let original = goal.original().ok_or("frozen A04 original missing")?;
        assert_eq!(original.as_bytes(), bytes);
        let [criterion] = goal.draft().acceptance_criteria() else {
            return Err("frozen A04 needs exactly one outcome".into());
        };
        assert_eq!(
            criterion.requirement(),
            a3_domain::AcceptanceCriterionRequirement::Must
        );
        let origin = criterion
            .origin()
            .ok_or("frozen A04 criterion origin missing")?;
        assert_eq!(
            origin.evidence_requirement(),
            &CriterionEvidenceRequirement::LoopbackHttp(recipe.clone())
        );
        let mut ranges = vec![origin.original_range()];
        for constraint in goal.draft().constraints() {
            ranges.push(
                constraint
                    .origin()
                    .ok_or("frozen A04 constraint origin missing")?,
            );
        }
        let edges = ranges
            .iter()
            .map(|range| [range.start_byte() as usize, range.end_byte() as usize])
            .collect::<Vec<_>>();
        assert_eq!(
            edges,
            A04_SOURCE_EDGES
                .windows(2)
                .map(|bounds| [bounds[0], bounds[1]])
                .collect::<Vec<_>>()
        );
        let schema = store
            .open_project_knowledge_for_run_journal(project)
            .await?
            .schema_version()
            .get();
        assert!(schema >= 57);
        Some(json!({
            "case_id":"A04", "variant":"development", "proof_scope":"initial_plan_only",
            "original_sha256":A04_ORIGINAL_SHA256, "original_bytes":bytes.len(),
            "original_ranges":edges, "candidate_schema":schema,
            "startup_scope": {
                "host":"127.0.0.1", "explicit_port":recipe.port().get(),
                "explicit_http_checks":recipe.checks().len(), "default_port":LOOPBACK_DEFAULT_PORT,
                "default_get_index":recipe.default_start_check_index(),
                "invalid_port_exit_required":true, "occupied_explicit_port_exit_required":true
            }
        }))
    } else {
        None
    };
    let usage = store.load_initial_plan_usage(project, task).await?;
    assert_eq!(
        (
            usage.calls,
            usage.repairs,
            usage.unobserved_calls,
            usage.unknown_calls
        ),
        (1 + repairs, repairs, 0, 0)
    );
    assert!(inventory(&worktree)?.is_empty());
    let receipt = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("inventory-plan-reopen.json"))?;
    let mut body = json!({
            "version":1, "phase":"initial_plan", "model":tag,
            "profile_id":blake3::Hash::from_bytes(*model.reference().id().as_bytes()).to_hex().as_str(),
            "wire_digest":blake3::Hash::from_bytes(*report.projection.digest().as_bytes()).to_hex().as_str(),
            "wire_bytes":report.projection.request_bytes(), "context_tokens":4_096, "output_limit":1_024,
            "reported_prompt_tokens":report.usage.prompt_tokens(), "reported_output_tokens":report.usage.output_tokens(),
            "calls":usage.calls, "repairs":usage.repairs, "ready_steps":1, "cold_reopen":true,
            "verification_check":expected_check, "exact_spec_reopened":true,
            "published_files":0, "server_starts":0, "mutations":0, "done":false,
    });
    if let Some(scope) = frozen_scope {
        body["frozen_a04"] = scope;
    }
    serde_json::to_writer_pretty(&receipt, &body)?;
    receipt.sync_all()?;
    Ok(())
}

fn repairable_plan_reply(error: &InitialPlanExecutionFailure) -> bool {
    matches!(
        error,
        InitialPlanExecutionFailure::Reply(
            PlanPhaseFailure::InvalidReply
                | PlanPhaseFailure::UnknownOffer
                | PlanPhaseFailure::DuplicateOffer
                | PlanPhaseFailure::InvalidDependency
                | PlanPhaseFailure::Capacity
        )
    )
}

#[derive(Debug)]
struct ObservedInventoryProvider<'a>(&'a OllamaModelProvider);

impl ModelProvider for ObservedInventoryProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.0.provider_id()
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.0.project_wire_request(request)
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let expected_job = request
                .structured_output()
                .and_then(|schema| {
                    schema.value()["oneOf"][0]["properties"]["job"]["const"].as_str()
                })
                .ok_or(ModelProviderFailure::Rejected)?;
            let mut reply = String::new();
            let mut overflow = false;
            let stream = self
                .0
                .stream_projected(request, projection, timeout, control)
                .await?
                .inspect(move |event| match event {
                    Ok(ProviderEvent::OutputText(chunk)) if !overflow => {
                        if reply.len().saturating_add(chunk.as_str().len()) <= 8 * 1024 {
                            reply.push_str(chunk.as_str());
                        } else {
                            overflow = true;
                        }
                    }
                    Ok(ProviderEvent::Completed(_)) if !overflow => {
                        report_reply_shape(&reply, expected_job);
                    }
                    _ => {}
                });
            Ok(Box::pin(stream) as ProviderEventStream<'a>)
        })
    }
}

fn report_reply_shape(reply: &str, expected_job: &str) {
    let Ok(Value::Object(body)) = serde_json::from_str(reply) else {
        eprintln!("Plan reply shape: no JSON object, {} bytes", reply.len());
        return;
    };
    let steps = body.get("steps").and_then(Value::as_array);
    let step_shapes = steps.map(|steps| {
        steps
            .iter()
            .take(3)
            .map(|step| {
                let outcome = step.get("outcome").and_then(Value::as_str);
                json!({
                    "fields": step.as_object().map(serde_json::Map::len),
                    "p1": step.get("offer").and_then(Value::as_str) == Some("P1"),
                    "outcome_bytes": outcome.map(str::len),
                    "outcome_controls": outcome.map(|text| text.chars().any(char::is_control)),
                    "after_items": step.get("after").and_then(Value::as_array).map(Vec::len),
                    "after_present": step.get("after").is_some(),
                })
            })
            .collect::<Vec<_>>()
    });
    eprintln!(
        "Plan reply shape: {}",
        json!({
            "fields": body.len(),
            "version_matches": body.get("version").and_then(Value::as_u64) == Some(1),
            "job_matches": body.get("job").and_then(Value::as_str) == Some(expected_job),
            "steps_present": body.contains_key("steps"),
            "step_count": steps.map(Vec::len),
            "step_shapes": step_shapes,
            "need_present": body.contains_key("need"),
            "too_large_present": body.contains_key("tooLarge"),
        })
    );
}

async fn promote_live_inventory(
    fixture: Fixture,
    model: ModelProfile,
    http_recipe: Option<&LoopbackHttpRecipe>,
    frozen_a04: Option<&[u8]>,
) -> TestResult<Promoted> {
    promote_live_inventory_with_budget(
        fixture,
        model,
        http_recipe,
        frozen_a04,
        AgentRunBudget::DEFAULT,
    )
    .await
}

async fn promote_live_inventory_with_budget(
    fixture: Fixture,
    model: ModelProfile,
    http_recipe: Option<&LoopbackHttpRecipe>,
    frozen_a04: Option<&[u8]>,
    budget: AgentRunBudget,
) -> TestResult<Promoted> {
    let (path, objective) = match (http_recipe, frozen_a04) {
        (Some(_), Some(original)) => ("server.py", original),
        (Some(_), None) => ("server.py", HTTP_ORIGINAL),
        (None, None) => ("README.md", ORIGINAL),
        (None, Some(_)) => return Err("frozen A04 requires its exact HTTP recipe".into()),
    };
    let mut data = IntakeFixture::with_goal(fixture, path, objective)?;
    let at = live_timestamp()?;
    data.pending = PendingGoalIntake::new(
        data.pending.original().clone(),
        GoalContractTimestamp::from_unix_millis(at.unix_millis())?,
    )?;
    let store = data.capture().await?;
    let project = &data.fixture.project;
    let pending = data.pending(&store).await?;
    let original = pending.pending().original();
    let window = original.range(0, original.as_bytes().len())?;
    let anchor = IntakePhaseAnchor::new(project, &pending, None, window, model.reference())?;
    let kind = if let Some(recipe) = http_recipe {
        GoalIntakeFindingKind::Outcome {
            requirement: a3_domain::AcceptanceCriterionRequirement::Must,
            evidence_requirement: CriterionEvidenceRequirement::LoopbackHttp(recipe.clone()),
        }
    } else {
        data.review()?
            .findings()
            .next()
            .ok_or("inventory criterion missing")?
            .kind()
            .clone()
    };
    let findings = if frozen_a04.is_some() {
        A04_SOURCE_EDGES
            .windows(2)
            .enumerate()
            .map(|(index, bounds)| {
                let range = original.range(bounds[0], bounds[1])?;
                let kind = if index == 0 {
                    kind.clone()
                } else {
                    GoalIntakeFindingKind::Constraint
                };
                Ok(GoalIntakeFinding::new(
                    kind,
                    range,
                    original.resolve(range)?.to_owned(),
                )?)
            })
            .collect::<TestResult<Vec<_>>>()?
    } else {
        vec![GoalIntakeFinding::new(
            kind,
            window,
            original.resolve(window)?.to_owned(),
        )?]
    };
    let proposal = GoalIntakeReviewBatchProposal::new(window, findings, Vec::new())?;
    let review = GoalIntakeReview::begin(&data.pending, proposal)?;
    let accounting = store
        .commit_bound_intake(project, anchor, &review, budget, at)
        .await?;
    assert_eq!(accounting.usage(), EditPhaseReservedUsage::default());
    data.goal = a3_application::prepare_goal_from_intake_review(&data.pending, &review)?;
    data.promote(&store).await?;
    drop(store);
    let (store, _, _, published) = index_fixture(&data.fixture).await?;
    Ok(Promoted {
        data,
        store,
        published,
        model,
    })
}

fn live_timestamp() -> TestResult<AgentRunTimestamp> {
    let millis = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    at(millis)
}

fn plan_view(request: &ModelProviderRequest) -> TestResult<Value> {
    let message = request.messages().last().ok_or("missing Plan input")?;
    Ok(serde_json::from_str(message.content())?)
}

fn fixed_reply(request: &ModelProviderRequest) -> Result<String, ModelProviderFailure> {
    let schema = request
        .structured_output()
        .ok_or(ModelProviderFailure::Rejected)?
        .value();
    let alias = schema["oneOf"][0]["properties"]["job"]["const"]
        .as_str()
        .ok_or(ModelProviderFailure::Rejected)?;
    Ok(json!({"version":1,"job":alias,"steps":[{"offer":"P1","outcome":OUTCOME}]}).to_string())
}

#[derive(Debug)]
struct NoSourceReads;

impl AgentSourceReader for NoSourceReads {
    fn read_page<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: &'a FileRevision,
        _: &'a AgentFileInspection,
        _: &'a dyn AgentSourceReadControl,
    ) -> AgentSourceReaderFuture<'a> {
        Box::pin(std::future::ready(Err(AgentSourceReadFailure::Denied)))
    }
}

#[derive(Debug)]
struct FixedPlanProvider<'a> {
    project: &'a ProjectIdentity,
    store: &'a LibsqlKnowledgeStore,
    task: TaskId,
    id: ModelProviderId,
}

impl ModelProvider for FixedPlanProvider<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        let schema = request
            .structured_output()
            .ok_or(ModelProviderFailure::Rejected)?
            .value();
        let messages = request
            .messages()
            .iter()
            .map(|message| message.content())
            .collect::<Vec<_>>();
        let format = serde_json::to_vec(schema).map_err(|_| ModelProviderFailure::Rejected)?;
        let body =
            serde_json::to_vec(&(&messages, schema)).map_err(|_| ModelProviderFailure::Rejected)?;
        ModelWireProjection::from_serialized(
            &body,
            messages.iter().map(|text| text.len()).sum(),
            format.len(),
            request.profile().settings().context_limit().get(),
            request.profile().settings().output_limit().get(),
            ModelWireThinking::Disabled,
        )
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            let usage = self
                .store
                .load_initial_plan_usage(self.project, self.task)
                .await
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if self.project_wire_request(request)? != projection
                || (usage.calls, usage.repairs, usage.unobserved_calls) != (1, 0, 1)
            {
                return Err(ModelProviderFailure::Rejected);
            }
            let output = ModelOutputChunk::try_from_string(fixed_reply(request)?)
                .map_err(|_| ModelProviderFailure::InvalidResponse)?;
            let events = futures::stream::iter([
                Ok(ProviderEvent::OutputText(output)),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(128), Some(32)),
                ))),
            ]);
            Ok(Box::pin(events) as ProviderEventStream<'a>)
        })
    }
}

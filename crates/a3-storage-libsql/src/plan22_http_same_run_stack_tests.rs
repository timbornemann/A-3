//! Controlled regression for the actual worker's patch and review-ready boundaries.
//! This invokes no model/probe/HTTP provider; real fixture processes/DB/patch remain owned.
use super::*;

#[test]
fn http_same_run_worker_controlled_context_patch_reaches_review_ready_without_model_inference()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(Box::pin(controlled_patch_to_ready()))
    })
}

async fn controlled_patch_to_ready() -> TestResult {
    let mut f =
        Box::pin(super::super::context_application::applied_context_for_closed_no_change(false))
            .await?;
    let before = inventory(&f.http.root)?;
    let original_source = fs::read(f.http.root.join("server.py"))?;
    let original = f.http.goal.original().ok_or("original goal")?.clone();
    let run_id = f.run.id();
    let budget = f.run.budget();
    let created = f.run.created_at();
    let goal = f.http.goal.clone();
    let (phase, mut compiled, page, _) = Box::pin(edit::acquire(&mut f)).await?;
    let range = page.range();
    let current = f
        .http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or("current")?;
    let usage_before = f
        .http
        .store
        .load_edit_phase_usage(&f.http.fixture.project, run_id)
        .await?;
    let corrected = page
        .text()
        .replace("b\"Hello, world!\"", "b\"Hello, world!\\n\"");
    assert_ne!(corrected, page.text());
    let provider = edit::controlled_native_edit_provider(
        f.http.store.as_ref(),
        &f.http.fixture.project,
        run_id,
        json!({"code":corrected}).to_string(),
    )?;
    let report = ExecuteEditPhase::new(
        &provider,
        f.http.store.as_ref(),
        &WorkspacePatchAdapter::with_change_window_preview(),
    )
    .execute(
        Box::pin(crate::plan22_goal_contract_tests::prepare_indexed_edit(
            &f.http.compiler,
            phase,
            &mut compiled,
            edit::state(&f, &current)?,
            edit::after_phases(&f).await?,
            &WorkspacePatchAdapter::with_change_window_preview(),
            &ActiveControl,
        ))
        .await?,
        edit::state(&f, &current)?,
        &compiled,
        edit::after_phases(&f).await?,
        ModelRequestTimeout::DEFAULT,
        &ActiveControl,
    )
    .await?;
    assert!(report.source_delivery.is_some());
    let EditPhaseOutcome::Proposal(action) = report.outcome else {
        return Err("controlled actual Edit did not propose a patch".into());
    };
    let usage = f
        .http
        .store
        .load_edit_phase_usage(&f.http.fixture.project, run_id)
        .await?;
    assert_eq!(usage.calls, usage_before.calls + 1);
    assert_eq!(usage.repairs, usage_before.repairs);
    assert_eq!((usage.unobserved_calls, usage.unknown_calls), (0, 0));
    let floor = edit::after_phases(&f).await?;
    drop((provider, current, compiled, page));
    let patch_future = apply_live_patch(f, action, floor);
    eprintln!(
        "http_same_run_controlled_patch_phase_future_bytes={}",
        std::mem::size_of_val(&patch_future)
    );
    let fixture = Box::pin(patch_future).await?;
    let source = fs::read(
        fixture
            .project
            .worktree()
            .root()
            .as_path()
            .join("server.py"),
    )?;
    let expected = FROZEN_SOURCE.replacen("b\"Hello, world!\"", "b\"Hello, world!\\n\"", 1);
    assert_eq!(source, expected.as_bytes());
    assert_eq!(
        &source[..range.start_byte() as usize],
        &original_source[..range.start_byte() as usize]
    );
    assert!(source.ends_with(&original_source[range.end_byte() as usize..]));
    let after = inventory(fixture.project.worktree().root().as_path())?;
    assert_eq!(before.len(), after.len());
    assert!(
        before
            .iter()
            .zip(&after)
            .all(|((path, bytes), (other, now))| path == other
                && (path == Path::new("server.py") || bytes == now))
    );
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let run = store
        .load_agent_run(&fixture.project, run_id)
        .await?
        .ok_or("cold patch run")?;
    let stored = store
        .load_task_ledger(&fixture.project, goal.task_id())
        .await?
        .ok_or("cold patch ledger")?;
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.budget(), budget);
    assert_eq!(run.created_at(), created);
    assert_eq!(
        store
            .load_edit_phase_usage(&fixture.project, run_id)
            .await?,
        usage
    );
    assert_eq!(
        store
            .load_current_goal_contract(&fixture.project, goal.task_id())
            .await?
            .ok_or("cold original")?
            .original(),
        Some(&original)
    );
    let proofs = stored
        .ledger()
        .steps()
        .filter(|step| {
            step.is_active_plan_step()
                && matches!(
                    step.definition().verification_spec().target(),
                    VerificationTarget::LoopbackHttp(_)
                )
        })
        .collect::<Vec<_>>();
    let [proof] = proofs.as_slice() else {
        return Err("one full pending HTTP proof".into());
    };
    assert!(proof.attempts().is_empty());
    assert_eq!(proof.status(), TaskStepStatus::Ready);
    let binding = json!({"owner_nonce":"declared-native-control-no-model-inference","controlled_provider":true});
    let patch_state = json!({"run_bytes":run_id.as_bytes(),"snapshot":format!("{:?}",run.current_snapshot_id()),
        "ledger":stored.ledger().revision().get(),"store_version":stored.version().get(),"source_sha256":sha(&source),
        "phase_calls":usage.calls,"repairs":usage.repairs,"outside_window_preserved":true});
    let ready_hash = publish_review_ready(&fixture.root, &binding, &source, &patch_state)?;
    let ready_bytes = fs::read(fixture.root.join("review-ready.json"))?;
    assert_eq!(sha(&ready_bytes), ready_hash);
    let ready: serde_json::Value = serde_json::from_slice(&ready_bytes)?;
    assert_eq!(ready["source_sha256"], sha(&source));
    assert_eq!(ready["source_bytes"], source.len());
    assert_eq!(ready["owner"], binding);
    assert_eq!(ready["state"], patch_state);
    assert_eq!(ready["same_owner_required"], true);
    // Ready is still no source review, operational HTTP proof, approval or Done.
    assert!(!fixture.root.join("review-accepted.json").exists());
    assert!(!fixture.root.join("http-functional-final.json").exists());
    assert_eq!(
        store.load_agent_run(&fixture.project, run_id).await?,
        Some(run)
    );
    assert_eq!(
        store
            .load_edit_phase_usage(&fixture.project, run_id)
            .await?,
        usage
    );
    drop(proofs);
    drop((stored, store));
    fixture.cleanup()
}

// Two bounded declared replies behind the same provider-port representation used by
// the actual worker. Each existing native provider independently checks its own
// pre-I/O durable debit and projects the real Ollama wire; neither invokes Ollama.
struct DeclaredWindowAndNeed<'a> {
    window: &'a dyn ModelProvider,
    edit: &'a dyn ModelProvider,
    calls: AtomicUsize,
}
impl std::fmt::Debug for DeclaredWindowAndNeed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclaredWindowAndNeed")
            .finish_non_exhaustive()
    }
}
impl ModelProvider for DeclaredWindowAndNeed<'_> {
    fn provider_id(&self) -> &ModelProviderId {
        self.window.provider_id()
    }
    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async { Err(ModelProviderFailure::Rejected) })
    }
    fn project_wire_request(
        &self,
        r: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.window.project_wire_request(r)
    }
    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        timeout: ModelRequestTimeout,
        control: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let field = match call {
            0 => "window",
            1 => "code",
            _ => return Box::pin(async { Err(ModelProviderFailure::Rejected) }),
        };
        let expected = request
            .structured_output()
            .and_then(|schema| schema.value()["oneOf"].as_array())
            .is_some_and(|branches| {
                branches
                    .iter()
                    .any(|branch| branch["properties"].get(field).is_some())
            });
        if !expected {
            return Box::pin(async { Err(ModelProviderFailure::Rejected) });
        }
        let provider = if call == 0 { self.window } else { self.edit };
        provider.stream_projected(request, projection, timeout, control)
    }
}

#[test]
fn http_same_run_worker_controlled_v6_window_reaches_own_read_and_cold_need_without_model_inference()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(Box::pin(controlled_window_to_cold_need()))
    })
}

async fn controlled_window_to_cold_need() -> TestResult {
    // The owner starts before setup and is retained through material/read/edit/cold halt.
    let started = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?;
    let mut clock = LiveOwnerClock::begin(started)?;
    let (mut f, phase, mut compiled, page) =
        Box::pin(super::super::super::edit::no_change::boundaries::prepared_16k()).await?;
    clock.bind(&f.run)?;
    let root = f.http.fixture.root.clone();
    let layout = f.http.fixture.layout.clone();
    let project = f.http.fixture.project.clone();
    let store = f.http.store.clone();
    let run_id = f.run.id();
    let original = f.http.goal.original().ok_or("original")?.clone();
    let created = f.run.created_at();
    let budget = f.run.budget();
    let before = inventory(&f.http.root)?;
    let before_source = fs::read(f.http.root.join("server.py"))?;
    let current = store
        .latest_published_index(&project, &ActiveControl)
        .await?
        .ok_or("index")?;
    let unchanged = edit::controlled_native_edit_provider(
        store.as_ref(),
        &project,
        run_id,
        json!({"code":page.text()}).to_string(),
    )?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let at = now(&f.http, &f.run, &mut clock).await?;
    let result = Box::pin(
        ExecuteEditPhase::new(&unchanged, store.as_ref(), &patch).execute(
            Box::pin(crate::plan22_goal_contract_tests::prepare_native_edit(
                phase,
                &mut compiled,
                edit::state(&f, &current)?,
                at,
                &patch,
                &ActiveControl,
            ))
            .await?,
            edit::state(&f, &current)?,
            &compiled,
            at,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        ),
    )
    .await;
    assert!(matches!(
        result,
        Err(EditPhaseExecutionFailure::Reply(EditPhaseFailure::NoChange))
    ));
    let (mut ledger, version) = f.stored.clone().into_parts();
    let at = now(&f.http, &f.run, &mut clock).await?;
    let version = Box::pin(
        CloseEditNoChange::new(store.as_ref(), store.as_ref(), &patch).execute(
            &project,
            &f.http.goal,
            &mut f.run,
            &mut ledger,
            version,
            &current,
            &f.http.model,
            &page,
            at,
            &ActiveControl,
        ),
    )
    .await?;
    f.stored = StoredTaskLedger::new(ledger, version);
    let old_range = page.range();
    drop((unchanged, current, compiled, page));
    f = Box::pin(edit::reopen(f)).await?;
    let usage = store.load_edit_phase_usage(&project, run_id).await?;
    let original_no_change_count =
        count(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?;
    let mutations = count(&f, "SELECT count(*) FROM mutation_attempts").await?;
    let window=edit::controlled_native_edit_provider(store.as_ref(),&project,run_id,
        json!({"window":"W1","intent":"Implement the GET / handler to return 200 with body 'Hello, world!' and correct headers, and 404"}).to_string())?;
    let need = edit::controlled_native_edit_provider(
        store.as_ref(),
        &project,
        run_id,
        json!({"need":"test"}).to_string(),
    )?;
    let provider = DeclaredWindowAndNeed {
        window: &window,
        edit: &need,
        calls: AtomicUsize::new(0),
    };
    let mut stage = "declared-native-control";
    // Identical concrete continuation function and ownership wrapper as the real worker.
    // This metric is NOT peak stack; a seam-only baseline must be run separately before
    // claiming that this controlled case reproduces the actual Source008 stack overflow.
    let (future_bytes, future) = owned_window_continuation(
        f,
        &provider,
        &root,
        &mut clock,
        &mut stage,
        LiveWindowInput::DeclaredNativeControl,
    );
    eprintln!(
        "http_same_run_controlled_window_future_bytes={future_bytes} owned_pointer_bytes={}",
        std::mem::size_of_val(&future)
    );
    let outcome = future
        .await
        .map_err(|error| format!("controlled window continuation at {stage}: {error}"))?;
    let LiveWindowResult::Halt(final_report) = outcome else {
        return Err("declared Need became ready/code".into());
    };
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(stage, "followup-edit");
    assert_eq!(final_report["terminal"], "regular_halt");
    assert_eq!(final_report["halt_stage"], "followup-edit");
    assert_eq!(final_report["halt_kind"], "needEvidence");
    assert_eq!(final_report["functional_component_passed"], false);
    assert_eq!(final_report["durable_edit_halt"]["kind"], "test");
    assert_eq!(final_report["durable_edit_halt"]["cold_reopened"], true);
    assert_eq!(final_report["durable_edit_halt"]["source_delivery"], false);
    assert_eq!(
        final_report["durable_edit_halt"]["automatic_resolution"],
        false
    );
    let material: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("no-change-window-material.json"))?)?;
    let read: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("followup-read.json"))?)?;
    assert_eq!(material["actual_material"], true);
    assert_eq!(material["proper_subset"], true);
    assert_eq!(material["old_window"], range_report(old_range));
    assert_eq!(read["actual_new_step_read"], true);
    assert_eq!(read["old_read_reused"], false);
    assert_eq!(read["new_window"], material["new_window"]);
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("no-change-plan-result.json"))?)?;
    assert_eq!(plan["controlled_model"], true);
    assert_eq!(plan["actual_stored_structural_repair"], false);
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&layout).await?;
    let run = cold
        .load_agent_run(&project, run_id)
        .await?
        .ok_or("cold Run")?;
    let loaded = cold
        .load_current_goal_contract(&project, run.goal_contract().task_id())
        .await?
        .ok_or("cold Goal")?;
    assert_eq!(loaded.original(), Some(&original));
    assert_eq!(run.created_at(), created);
    assert_eq!(run.budget(), budget);
    assert_eq!(run.state(), AgentControllerState::Execute);
    let charged = cold.load_edit_phase_usage(&project, run_id).await?;
    assert_eq!(charged.calls, usage.calls + 2);
    assert_eq!(charged.repairs, usage.repairs);
    assert_eq!((charged.unknown_calls, charged.unobserved_calls), (0, 0));
    assert!(
        charged.prompt_tokens > usage.prompt_tokens && charged.output_tokens > usage.output_tokens
    );
    assert_eq!(inventory(project.worktree().root().as_path())?, before);
    assert_eq!(
        fs::read(project.worktree().root().as_path().join("server.py"))?,
        before_source
    );
    for name in [
        "patch-state.json",
        "review-ready.json",
        "http-functional-final.json",
        "same-run-final.json",
    ] {
        assert!(!root.join(name).exists())
    }
    let db = cold
        .open_project_knowledge_for_run_journal(&project)
        .await?;
    let connection = db.connection_for_operation().await?;
    let mut rows=connection.query("SELECT (SELECT count(*) FROM mutation_attempts),(SELECT count(*) FROM edit_phase_no_change_receipts),(SELECT count(*) FROM edit_phase_jobs WHERE halt_kind='test')",()).await?;
    let row = rows.next().await?.ok_or("cold counts")?;
    assert_eq!(row.get::<i64>(0)?, mutations);
    assert_eq!(row.get::<i64>(1)?, original_no_change_count);
    assert_eq!(row.get::<i64>(2)?, 1);
    // The final Row owns a Statement/Connection independently of Rows.
    drop(row);
    drop((rows, connection, db, cold));
    drop((window, need));
    drop(store);
    Fixture {
        root,
        layout,
        project,
    }
    .cleanup()
    .map_err(|error| format!("controlled window fixture cleanup: {error}").into())
}

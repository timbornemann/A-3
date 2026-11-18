//! Full unchanged HTTP proof after the closed preseeded correction; no greenfield claim.
use super::*;
use a3_application::{
    AuthorizedLoopbackStartupCoverage, LoopbackStartupVerificationFuture,
    LoopbackStartupVerificationOutcome, PreparedLoopbackStartupCoverage,
};
use std::time::Instant;

fn next_time(started: Instant, base: u64, run: &AgentRun) -> TestResult<AgentRunTimestamp> {
    let elapsed = u64::try_from(started.elapsed().as_millis())?;
    let now = at(base
        .checked_add(elapsed)
        .ok_or("clock overflow")?
        .max(run.updated_at().unix_millis())
        .checked_add(1)
        .ok_or("clock overflow")?)?;
    if run.budget_exhaustion(now)?.is_some() {
        return Err("original Run budget exhausted".into());
    }
    Ok(now)
}

fn require_reviewed_source(project: &ProjectIdentity, reviewed: ContentHash) -> TestResult {
    let source = fs::canonicalize(project.worktree().root().as_path().join("server.py"))?;
    if source.parent() != Some(project.worktree().root().as_path())
        || fs::metadata(&source)?.len() > 64 * 1024
    {
        return Err("reviewed source escaped the bounded current fixture".into());
    }
    if blake3::hash(&fs::read(source)?).as_bytes() != reviewed.as_bytes() {
        return Err("current bytes differ from independently reviewed source".into());
    }
    Ok(())
}

/// Reopens after the actual Patch, executes the existing full HTTP proof, and reopens Done.
/// The returned report describes this closed preseeded component, never greenfield A04/P7.
pub(in super::super) async fn finish_http(
    fixture: Fixture,
    run_id: AgentRunId,
    profile: ModelProfile,
    settings: LoopbackProcessSettings,
    not_before: AgentRunTimestamp,
    reviewed_source: ContentHash,
) -> TestResult<(Fixture, serde_json::Value)> {
    finish_http_scope(
        fixture,
        run_id,
        profile,
        settings,
        not_before,
        reviewed_source,
        HttpFixtureFinish::ClosedGoal,
    )
    .await
}

// The second original criterion exists before Run creation; this entry does not
// change the HTTP recipe, its evidence, or the ordinary closed-goal Done checks.
pub(in super::super) async fn finish_http_with_remaining_original_criterion(
    fixture: Fixture,
    run_id: AgentRunId,
    profile: ModelProfile,
    settings: LoopbackProcessSettings,
    not_before: AgentRunTimestamp,
    reviewed_source: ContentHash,
) -> TestResult<(Fixture, serde_json::Value)> {
    finish_http_scope(
        fixture,
        run_id,
        profile,
        settings,
        not_before,
        reviewed_source,
        HttpFixtureFinish::RemainingOriginalCriterion,
    )
    .await
}

#[derive(Clone, Copy)]
enum HttpFixtureFinish {
    ClosedGoal,
    RemainingOriginalCriterion,
}

async fn finish_http_scope(
    fixture: Fixture,
    run_id: AgentRunId,
    profile: ModelProfile,
    settings: LoopbackProcessSettings,
    not_before: AgentRunTimestamp,
    reviewed_source: ContentHash,
    finish: HttpFixtureFinish,
) -> TestResult<(Fixture, serde_json::Value)> {
    let started = Instant::now();
    let project = &fixture.project;
    let store = Arc::new(LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?);
    let mut run = store
        .load_agent_run(project, run_id)
        .await?
        .ok_or("corrected Run missing")?;
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.model_profile(), Some(profile.reference()));
    let goal = store
        .load_current_goal_contract(project, run.goal_contract().task_id())
        .await?
        .ok_or("original Goal missing")?;
    let original = goal.original().ok_or("original request missing")?.clone();
    match finish {
        HttpFixtureFinish::ClosedGoal => assert_eq!(original.as_bytes().len(), 1207),
        HttpFixtureFinish::RemainingOriginalCriterion => {
            let standard = a04_goal()?;
            assert!(
                original
                    .as_bytes()
                    .starts_with(standard.original().ok_or("standard original")?.as_bytes())
            );
            assert!(original.as_bytes().len() > 1207);
            assert_eq!(goal.draft().acceptance_criteria().len(), 2);
        }
    }
    let stored = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("corrected Ledger missing")?;
    let (mut ledger, mut version) = stored.into_parts();
    let old_budget = run.budget();
    let old_created = run.created_at();
    let old_phase_usage = store.load_edit_phase_usage(project, run_id).await?;
    assert_eq!(
        (
            old_phase_usage.unobserved_calls,
            old_phase_usage.unknown_calls
        ),
        (0, 0)
    );
    let old_attempts = store.load_agent_mutation_attempts(project, run_id).await?;
    if old_attempts.iter().any(|attempt| {
        !attempt.tool_attempt().status().is_terminal()
            || matches!(attempt.disposition(), AgentMutationDisposition::Unknown(_))
    }) {
        return Err("prior mutation is InFlight or Unknown".into());
    }
    if ledger
        .steps()
        .filter(|step| step.is_active_plan_step())
        .count()
        != 2
    {
        return Err("functional component requires exactly correction and HTTP proof".into());
    }
    let retired = ledger
        .steps()
        .filter(|step| !step.is_active_plan_step())
        .cloned()
        .collect::<Vec<_>>();
    let before = inventory(project.worktree().root().as_path())?;
    let proofs = ledger
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
        return Err("unique pending HTTP proof missing".into());
    };
    assert_eq!(proof.status(), TaskStepStatus::Ready);
    assert!(proof.attempts().is_empty());
    AdmitSourceTaskStep.execute(&goal, proof.definition())?;
    let proof_id = proof.definition().id();
    let spec = proof.definition().verification_spec().clone();
    let VerificationTarget::LoopbackHttp(recipe) = spec.target() else {
        return Err("HTTP recipe missing".into());
    };
    let recipe = recipe.clone();
    if matches!(finish, HttpFixtureFinish::ClosedGoal) {
        assert_eq!(goal.draft().acceptance_criteria().len(), 1);
    }
    let [criterion_id] = proof.definition().acceptance_criteria() else {
        return Err(
            "the unchanged HTTP proof must still cover exactly its original criterion".into(),
        );
    };
    let criterion = goal
        .draft()
        .acceptance_criteria()
        .iter()
        .find(|c| c.id() == *criterion_id)
        .ok_or("original HTTP criterion missing")?;
    assert!(
        criterion.origin().is_some_and(|origin|matches!(origin.evidence_requirement(),CriterionEvidenceRequirement::LoopbackHttp(original) if original==&recipe))
    );
    let [dependency] = proof.definition().dependencies() else {
        return Err("correction dependency missing".into());
    };
    let correction = ledger
        .step(dependency.prerequisite())
        .ok_or("correction missing")?;
    assert!(
        correction.is_active_plan_step()
            && correction.status() == TaskStepStatus::Completed
            && correction.definition().acceptance_criteria().is_empty()
    );
    let diff_ids = correction
        .attempts()
        .last()
        .ok_or("correction attempt missing")?
        .evidence_ids()
        .to_vec();
    let snapshot = run.current_snapshot_id();
    let current = store
        .load_verification_state(
            project,
            goal.task_id(),
            &diff_ids,
            snapshot,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    assert!(
        matches!(current.evidence(),[VerificationEvidence::Diff(diff)] if diff.complete() && diff.snapshot_id()==snapshot)
    );
    assert_eq!(current.published_index().run().snapshot_id(), snapshot);
    let script = current
        .published_index()
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == recipe.script())
        .ok_or("current corrected script missing")?
        .clone();
    assert_eq!(script.path().as_bytes(), b"server.py");
    if script.content_hash() != reviewed_source {
        return Err("published source differs from independently reviewed hash".into());
    }
    let owner = {
        let mut hash = blake3::Hasher::new_derive_key("a3.plan22.corrected-http-proof-fixture.v1");
        for bytes in [
            run_id.as_bytes(),
            snapshot.as_bytes(),
            proof_id.as_bytes(),
            spec.id().as_bytes(),
        ] {
            hash.update(bytes);
        }
        *hash.finalize().as_bytes()
    };
    let id = |role: u8, slot: u8| {
        let mut hash = blake3::Hasher::new_derive_key("a3.plan22.corrected-http-proof-role.v1");
        hash.update(&owner);
        hash.update(&[role, slot]);
        *hash.finalize().as_bytes()
    };
    let base = {
        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        let tx = connection
            .transaction_with_behavior(libsql::TransactionBehavior::Deferred)
            .await?;
        let (_, latest) = crate::edit_phase_reservation_repository::read_run_usage(
            &tx,
            project.worktree().id(),
            &run,
        )
        .await?;
        let base = run
            .updated_at()
            .unix_millis()
            .max(latest.map_or(0, |time| time.unix_millis()))
            .max(not_before.unix_millis());
        tx.commit().await?;
        base
    };
    require_reviewed_source(project, reviewed_source)?;
    let now = next_time(started, base, &run)?;
    let continuation = ContinueSourceAgentPlan::new(store.as_ref())
        .execute(
            project,
            version,
            &goal,
            &mut run,
            &mut ledger,
            snapshot,
            RunEventId::from_bytes(id(1, 0)),
            now,
            &ActiveControl,
        )
        .await?;
    let SourcePlanContinuationOutcome::Continued(ContinueVerifiedAgentPlanOutcome::StepStarted {
        step_id,
        ledger_version,
    }) = continuation
    else {
        return Err("Core did not start pending unchanged HTTP proof".into());
    };
    assert_eq!(step_id, proof_id);
    version = ledger_version;
    let prepared = PreparedLoopbackStartupCoverage::new(
        run_id,
        project.worktree().id(),
        proof_id,
        snapshot,
        script.clone(),
        recipe.clone(),
        settings,
    )?;
    let actions = prepared.policy_actions();
    assert_eq!(actions.len(), recipe.checks().len() + 6);
    let requests = actions
        .iter()
        .enumerate()
        .map(|(index, _)| Ok(ApprovalRequestId::from_bytes(id(30, u8::try_from(index)?))))
        .collect::<TestResult<Vec<_>>>()?;
    let decisions = actions
        .iter()
        .enumerate()
        .map(|(index, _)| Ok(PolicyDecisionId::from_bytes(id(50, u8::try_from(index)?))))
        .collect::<TestResult<Vec<_>>>()?;
    let grants = requests
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, request)| {
            Ok(LoopbackProbeApproval {
                request_id: *request,
                decision_id: decisions[index],
                event_id: RunEventId::from_bytes(id(51, u8::try_from(index)?)),
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    let policy = WorkspacePolicy::unrestricted();
    let deadline = at(old_created
        .unix_millis()
        .checked_add(old_budget.duration_limit().millis())
        .ok_or("deadline overflow")?)?;
    require_reviewed_source(project, reviewed_source)?;
    for (index, (action, request)) in actions.iter().zip(&requests).enumerate() {
        assert!(
            store
                .load_approval_for_request(project, *request)
                .await?
                .is_none()
        );
        let slot = u8::try_from(index)?;
        let now = next_time(started, base, &run)?;
        let expected = run.last_event_sequence();
        let waiting = EvaluateActionPolicy::new()
            .with_permissions(store.load_agent_permissions().await?)
            .execute(
                &mut run,
                action,
                &policy,
                None,
                PolicyEvaluationContext::new(
                    PolicyDecisionId::from_bytes(id(31, slot)),
                    *request,
                    RunEventId::from_bytes(id(32, slot)),
                    snapshot,
                    PolicyEvaluationTiming::new(now, now)?,
                    deadline,
                ),
            )?;
        assert_eq!(
            waiting.decision().outcome(),
            PolicyDecisionOutcome::ApprovalRequired
        );
        PersistPolicyEvaluation::new(store.as_ref())
            .execute(project, expected, &run, &waiting)
            .await?;
        let now = next_time(started, base, &run)?;
        let granted = GrantPolicyApproval::new(store.as_ref())
            .execute(
                project,
                &mut run,
                *request,
                ApprovalId::from_bytes(id(33, slot)),
                RunEventId::from_bytes(id(34, slot)),
                snapshot,
                now,
            )
            .await?;
        assert_eq!(granted.state(), ApprovalGrantState::Active);
    }
    let ids = MutationExecutionIds::new(
        decisions[0],
        requests[0],
        RunEventId::from_bytes(id(51, 0)),
        RunEventId::from_bytes(id(52, 0)),
        ToolRunId::from_bytes(id(3, 0)),
        RunEventId::from_bytes(id(53, 0)),
        RunEventId::from_bytes(id(54, 0)),
        RunEventId::from_bytes(id(55, 0)),
        RunEventId::from_bytes(id(56, 0)),
        VerificationRunId::from_bytes(id(57, 0)),
        StepVerificationId::from_bytes(id(58, 0)),
    );
    let tool = ToolRunId::from_bytes(id(3, 0));
    assert!(
        old_attempts
            .iter()
            .all(|attempt| attempt.tool_attempt().tool_run_id() != tool)
    );
    let native = native_http_runner()?;
    let observed = FreshStartupRunner {
        store: store.as_ref(),
        runner: &native,
        requests: &requests,
        decisions: &decisions,
        tool,
        fingerprint: prepared.effect_fingerprint(),
        reviewed_source,
        calls: AtomicUsize::new(0),
    };
    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(project);
    let approval = AgentApprovalBuffer::new();
    approval.activate_project(project);
    let patch = WorkspacePatchAdapter::new();
    let process = WorkspaceProcessRunner::default();
    let context = UnexpectedContext::default();
    let refresh = RefreshRepositoryIndex::new(
        Arc::new(Blake3RepositorySnapshotBuilder::new()),
        store.clone(),
        Arc::new(Blake3IndexRunIdFactory),
    );
    let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approval,
        &patch,
        &process,
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let now = next_time(started, base, &run)?;
    let checked = controller
        .execute_loopback_verification(
            project,
            &mut run,
            &mut ledger,
            &mut version,
            LoopbackVerificationRequest {
                step_id: proof_id,
                settings,
                probe_approvals: &grants,
                ids,
                observed_at: now,
            },
            LoopbackVerificationAuthority::new(store.as_ref(), store.as_ref()),
            &observed,
            &policy,
            &seed,
            &mut compiler,
            &ActiveControl,
        )
        .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = checked
    else {
        return Err("actual corrected HTTP recipe did not verify".into());
    };
    assert_eq!(snapshot_id, snapshot);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(observed.calls.load(Ordering::SeqCst), 1);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.created_at(), old_created);
    assert_eq!(run.budget(), old_budget);
    assert_eq!(
        store.load_edit_phase_usage(project, run_id).await?,
        old_phase_usage
    );
    assert_eq!(inventory(project.worktree().root().as_path())?, before);
    let attempts = store.load_agent_mutation_attempts(project, run_id).await?;
    assert_eq!(attempts.len(), old_attempts.len() + 1);
    for old in &old_attempts {
        assert!(attempts.contains(old));
    }
    for old in &retired {
        assert_eq!(ledger.step(old.definition().id()), Some(old));
    }
    let final_run = run.clone();
    let final_ledger = StoredTaskLedger::new(ledger.clone(), version);
    drop((compiler, refresh));
    drop(store);
    let (receipt, held_exit, invalid_exit) = {
        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let state = cold
            .load_verification_state(
                project,
                goal.task_id(),
                &[evidence_id],
                snapshot,
                Duration::from_secs(10),
                &ActiveControl,
            )
            .await?;
        let [VerificationEvidence::LoopbackHttp(receipt)] = state.evidence() else {
            return Err("current cold HTTP product evidence missing".into());
        };
        assert!(receipt.matches(&recipe));
        assert_eq!(receipt.script(), &script);
        let c = receipt.context();
        assert_eq!(c.run_id, run_id);
        assert_eq!(c.step_id, proof_id);
        assert_eq!(c.spec_id, spec.id());
        assert_eq!(c.tool_run_id, tool);
        assert_eq!(c.snapshot_id, snapshot);
        let startup = receipt.startup().ok_or("full startup evidence missing")?;
        assert_eq!(cold.load_agent_run(project, run_id).await?, Some(final_run));
        assert_eq!(
            cold.load_task_ledger(project, goal.task_id()).await?,
            Some(final_ledger)
        );
        assert_eq!(
            cold.load_current_goal_contract(project, goal.task_id())
                .await?,
            Some(goal.clone())
        );
        (
            receipt.id(),
            startup.held_exit().get(),
            startup.invalid_exit().get(),
        )
    };
    // Pure acceptance consumes no new model/action allowance; the original time limit remains.
    let acceptance_at = base
        .checked_add(u64::try_from(started.elapsed().as_millis())?)
        .ok_or("clock overflow")?
        .max(run.updated_at().unix_millis())
        .checked_add(1)
        .ok_or("clock overflow")?;
    if matches!(finish, HttpFixtureFinish::RemainingOriginalCriterion) {
        assert!(
            ledger
                .steps()
                .filter(|step| step.is_active_plan_step())
                .all(|step| step.status() == TaskStepStatus::Completed)
        );
        let extra = goal
            .draft()
            .acceptance_criteria()
            .iter()
            .find(|c| c.id() != criterion.id())
            .ok_or("remaining original criterion missing")?;
        assert!(
            !ledger
                .steps()
                .filter(|step| step.is_active_plan_step())
                .any(|step| step
                    .definition()
                    .acceptance_criteria()
                    .contains(&extra.id()))
        );
        let report = json!({"scope":"Controlled complete B2 HTTP proof with an additional original unplanned criterion; no Done or model claim",
            "same_run":true,"fresh_exact_grants":actions.len(),"owned_starts":4,
            "total_http_observations":recipe.checks().len()+1,"cold_http_verified":true,
            "run_state":"Verify","cold_done":false,"goal_done":false,
            "generated_code_executed":true,"http_reexecuted":true,"original_unchanged":true,
            "old_attempts_preserved":true,"provider_usage_unchanged":true,
            "a04_completed":false,"p7_completed":false});
        return Ok((fixture, report));
    }
    finish_reopened_source_for_step(
        &fixture,
        goal.clone(),
        evidence_id,
        snapshot,
        acceptance_at,
        run_id,
        proof_id,
        RunEventId::from_bytes(id(2, 0)),
    )
    .await?;
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let done = cold
        .load_agent_run(project, run_id)
        .await?
        .ok_or("cold Done missing")?;
    assert_eq!(done.state(), AgentControllerState::Done);
    assert_eq!(done.budget(), old_budget);
    assert_eq!(done.created_at(), old_created);
    assert_eq!(
        cold.load_current_goal_contract(project, goal.task_id())
            .await?
            .ok_or("cold original missing")?
            .original(),
        Some(&original)
    );
    drop(cold);
    let report = json!({"scope":"Closed preseeded corrected HTTP component with current product verification; no greenfield A04 claim",
        "same_run":true,"fresh_exact_grants":actions.len(),"owned_full_startup_coverage":true,"owned_startup_invocations":1,
        "explicit_http_observations":recipe.checks().len(),"default_http_observations":1,
        "total_http_observations":recipe.checks().len()+1,"owned_starts":4,"exact_original_typed_recipe":true,
        "held_failure_exit":held_exit,"invalid_port_failure_exit":invalid_exit,
        "current_source_hash":format!("{:?}",script.content_hash()),"current_snapshot":format!("{:?}",snapshot),
        "proof_step":format!("{:?}",proof_id),"proof_spec":format!("{:?}",spec.id()),"new_tool":format!("{:?}",tool),
        "evidence":format!("{:?}",receipt),"original_unchanged":true,"old_attempts_preserved":true,"provider_usage_unchanged":true,
        "run_state":"Done","cold_done":true,"goal_done":true,"generated_code_executed":true,"http_reexecuted":true,
        "greenfield_creation_history":false,"automatic_mismatch_admission":false,"a04_completed":false,"p7_completed":false});
    Ok((fixture, report))
}

struct FreshStartupRunner<'a> {
    store: &'a LibsqlKnowledgeStore,
    runner: &'a dyn LoopbackVerificationRunner,
    requests: &'a [ApprovalRequestId],
    decisions: &'a [PolicyDecisionId],
    tool: ToolRunId,
    fingerprint: MutationActionFingerprint,
    reviewed_source: ContentHash,
    calls: AtomicUsize,
}
impl std::fmt::Debug for FreshStartupRunner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FreshStartupRunner").finish_non_exhaustive()
    }
}
impl LoopbackVerificationRunner for FreshStartupRunner<'_> {
    fn supports_owned_cleanup(&self) -> bool {
        self.runner.supports_owned_cleanup()
    }
    fn supports_startup_coverage(&self) -> bool {
        self.runner.supports_startup_coverage()
    }
    fn verify_startup<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackStartupCoverage,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackStartupVerificationFuture<'a> {
        Box::pin(async move {
            assert_eq!(self.calls.fetch_add(1, Ordering::SeqCst), 0);
            assert_eq!(
                authorized.prepared().script().content_hash(),
                self.reviewed_source
            );
            let attempts = self
                .store
                .load_agent_mutation_attempts(project, authorized.prepared().start().run_id())
                .await
                .map_err(|_| LoopbackVerificationFailure::Unavailable);
            let attempts = match attempts {
                Ok(value) => value,
                Err(error) => return LoopbackStartupVerificationOutcome::NotStarted(error),
            };
            let current = attempts
                .iter()
                .filter(|attempt| attempt.tool_attempt().tool_run_id() == self.tool)
                .collect::<Vec<_>>();
            assert!(
                matches!(current.as_slice(),[attempt] if attempt.fingerprint()==self.fingerprint
                && attempt.tool_attempt().status()==AgentToolAttemptStatus::InFlight && attempt.disposition().requires_reconciliation())
            );
            for (&request, &decision) in self.requests.iter().zip(self.decisions) {
                let grant = self.store.load_approval_for_request(project, request).await;
                assert!(
                    matches!(grant,Ok(Some(grant)) if matches!(grant.state(),ApprovalGrantState::Consumed{decision_id,..} if decision_id==decision))
                );
            }
            self.runner
                .verify_startup(project, authorized, control)
                .await
        })
    }
    fn verify<'a>(
        &'a self,
        _: &'a ProjectIdentity,
        _: AuthorizedLoopbackVerification,
        _: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a> {
        Box::pin(async {
            LoopbackVerificationOutcome::NotStarted(LoopbackVerificationFailure::Denied)
        })
    }
}

#[test]
#[ignore = "explicit independent source review, exact reviewed hash and original wall-clock owner required"]
#[cfg(any(windows, target_os = "linux"))]
fn http_correction_edit_verify_reviewed_current_fixture() -> TestResult {
    crate::run_native_libsql_live_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let tag = std::env::var("A3_HTTP_CORRECTION_MODEL")?;
                if !["qwen38-4k:latest", "ornith-1.5:9b"].contains(&tag.as_str()) {
                    return Err("unsupported reviewed model tag".into());
                }
                let input = PathBuf::from(
                    std::env::var_os("A3_HTTP_CORRECTION_OUTPUT")
                        .ok_or("existing retained output required")?,
                );
                if !input.is_absolute() {
                    return Err("absolute retained fixture required".into());
                }
                let allowed = fs::canonicalize(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plan22-p7-runs"),
                )?;
                let root = fs::canonicalize(input)?;
                if root.parent() != Some(allowed.as_path()) {
                    return Err(
                        "reviewed fixture must be a current direct child of retained target".into(),
                    );
                }
                let slug = root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("fixture slug missing")?;
                let read_json = |path: &Path| -> TestResult<(serde_json::Value, Vec<u8>)> {
                    let path = fs::canonicalize(path)?;
                    if !path.starts_with(&allowed) || fs::metadata(&path)?.len() > 64 * 1024 {
                        return Err("bounded fixture metadata escaped".into());
                    }
                    let bytes = fs::read(path)?;
                    Ok((serde_json::from_slice(&bytes)?, bytes))
                };
                let (context, context_bytes) = read_json(&root.join("coding-context.json"))?;
                let bytes = context["run_bytes"]
                    .as_array()
                    .ok_or("trusted coding context run bytes missing")?;
                if bytes.len() != 32 {
                    return Err("exact run bytes required".into());
                }
                let mut run_bytes = [0_u8; 32];
                for (target, value) in run_bytes.iter_mut().zip(bytes) {
                    *target = u8::try_from(value.as_u64().ok_or("invalid run byte")?)?;
                }
                let run_id = AgentRunId::from_bytes(run_bytes);
                let reviewed = std::env::var("A3_HTTP_REVIEWED_SOURCE_HASH")?;
                if reviewed.len() != 64
                    || !reviewed
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err("exact lowercase reviewed BLAKE3 digest required".into());
                }
                let (owner, _) = read_json(&allowed.join(format!("{slug}-owner.json")))?;
                let (launch, _) = read_json(&allowed.join(format!("{slug}-launch.json")))?;
                let (result, _) = read_json(&allowed.join(format!("{slug}-result.json")))?;
                use sha2::Digest;
                let context_sha256 = format!("{:x}", sha2::Sha256::digest(&context_bytes));
                let started = reviewed_code_owner_start(
                    &owner,
                    &launch,
                    &result,
                    slug,
                    &tag,
                    &context,
                    &context_sha256,
                )?;
                let app_data = fs::canonicalize(root.join("app-data"))?;
                let worktree = fs::canonicalize(root.join("worktree"))?;
                if app_data.parent() != Some(root.as_path())
                    || worktree.parent() != Some(root.as_path())
                {
                    return Err("retained app data or worktree escaped its exact fixture".into());
                }
                let source = fs::canonicalize(worktree.join("server.py"))?;
                if source.parent() != Some(worktree.as_path())
                    || fs::metadata(&source)?.len() > 64 * 1024
                {
                    return Err("reviewed script escaped the bounded fixture".into());
                }
                let actual = blake3::hash(&fs::read(source)?);
                if actual.to_hex().as_str() != reviewed {
                    return Err("current script differs from independently reviewed bytes".into());
                }
                let fixture = Fixture {
                    layout: StorageLayout::prepare(root.join("app-data"))?,
                    project: RepositoryInspector::new().inspect(&worktree)?,
                    root: root.clone(),
                };
                let (created, active_profile) = {
                    let store =
                        LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                    let run = store
                        .load_agent_run(&fixture.project, run_id)
                        .await?
                        .ok_or("reviewed Run missing")?;
                    (
                        run.created_at().unix_millis(),
                        run.model_profile().ok_or("reviewed Run profile missing")?,
                    )
                };
                let settings = LoopbackProcessSettings {
                    timeout: ProcessTimeout::from_millis(8000)?,
                    stdout_limit: ProcessOutputLimit::new(1024)?,
                    stderr_limit: ProcessOutputLimit::new(1024)?,
                    system_root: cfg!(windows),
                };
                let profile = reviewed_fixture_profile(&tag, active_profile)?;
                let now = u64::try_from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_millis(),
                )?;
                let elapsed = now
                    .checked_sub(started)
                    .ok_or("original owner timestamp is in future")?
                    .checked_add(1)
                    .ok_or("owner clock overflow")?;
                let not_before = at(created
                    .checked_add(elapsed)
                    .ok_or("original owner elapsed overflow")?)?;
                let (fixture, mut report) = finish_http(
                    fixture,
                    run_id,
                    profile,
                    settings,
                    not_before,
                    ContentHash::from_bytes(*actual.as_bytes()),
                )
                .await?;
                report["independent_review_hash"] = json!(reviewed);
                report["original_owner_started_utc_unix_ms"] = json!(started);
                report["wall_elapsed_upper_bound_ms"] = json!(elapsed);
                report["not_before_logical_ms"] = json!(not_before.unix_millis());
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(root.join("http-functional-final.json"))?;
                use std::io::Write;
                serde_json::to_writer_pretty(&mut output, &report)?;
                output.write_all(b"\n")?;
                output.sync_all()?;
                drop(fixture);
                Ok(())
            })
    })
}

fn reviewed_code_owner_start(
    owner: &serde_json::Value,
    launch: &serde_json::Value,
    result: &serde_json::Value,
    slug: &str,
    model: &str,
    context: &serde_json::Value,
    context_sha256: &str,
) -> TestResult<u64> {
    if owner.get("exit_code").and_then(serde_json::Value::as_i64) != Some(0)
        || ["timed_out", "interrupted", "output_limit"]
            .iter()
            .any(|key| owner.get(*key).and_then(serde_json::Value::as_bool) != Some(false))
        || !owner
            .get("supervisor_error")
            .is_some_and(serde_json::Value::is_null)
        || owner["attempt"].as_str() != Some(slug)
        || owner["model"].as_str() != Some(model)
        || owner["run_bytes"] != context["run_bytes"]
        || owner["coding_context_sha256"].as_str() != Some(context_sha256)
        || result["technical_component_passed"].as_bool() != Some(true)
        || result["model"].as_str() != Some(model)
        || result["worker_sha256"] != owner["worker_sha256"]
    {
        return Err("original code owner did not complete this exact accepted component".into());
    }
    for key in ["attempt", "model", "worker_sha256", "prereg_sha256", "test"] {
        if owner
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_none_or(|value| value.is_empty())
            || owner[key] != launch[key]
        {
            return Err("original launch and owner binding disagree".into());
        }
    }
    owner["started_utc_unix_ms"]
        .as_u64()
        .ok_or("original code owner start UTC milliseconds missing".into())
}

#[test]
fn http_correction_edit_reviewed_owner_rejects_incomplete_and_cross_bound_code_trials() -> TestResult
{
    let context = json!({"run_bytes":vec![1_u8;32]});
    let owner = json!({"exit_code":0,"timed_out":false,"interrupted":false,"output_limit":false,"supervisor_error":null,
        "attempt":"reviewed-owner","model":"qwen38-4k:latest","worker_sha256":"worker","prereg_sha256":"prereg","test":"code-test",
        "run_bytes":context["run_bytes"],"coding_context_sha256":"actual-context-sha256","started_utc_unix_ms":1000});
    let launch = owner.clone();
    let result = json!({"technical_component_passed":true,"model":"qwen38-4k:latest","worker_sha256":"worker"});
    assert_eq!(
        reviewed_code_owner_start(
            &owner,
            &launch,
            &result,
            "reviewed-owner",
            "qwen38-4k:latest",
            &context,
            "actual-context-sha256"
        )?,
        1000
    );
    for case in [
        "exit",
        "timeout",
        "interrupted",
        "supervisor",
        "output",
        "missing-terminal",
        "attempt",
        "model",
        "worker",
        "prereg",
        "test",
        "run",
        "context",
        "missing-start",
        "launch",
        "result",
        "result-worker",
    ] {
        let mut o = owner.clone();
        let mut l = launch.clone();
        let mut r = result.clone();
        match case {
            "exit" => o["exit_code"] = json!(1),
            "timeout" => o["timed_out"] = json!(true),
            "interrupted" => o["interrupted"] = json!(true),
            "supervisor" => o["supervisor_error"] = json!("owner-failed"),
            "output" => o["output_limit"] = json!(true),
            "missing-terminal" => {
                o.as_object_mut()
                    .ok_or("object missing")?
                    .remove("timed_out");
            }
            "attempt" => o["attempt"] = json!("other-attempt"),
            "model" => o["model"] = json!("ornith-1.5:9b"),
            "worker" => o["worker_sha256"] = json!("other-worker"),
            "prereg" => o["prereg_sha256"] = json!("other-prereg"),
            "test" => o["test"] = json!("other-test"),
            "run" => o["run_bytes"] = json!(vec![2_u8; 32]),
            "context" => o["coding_context_sha256"] = json!("other-context-sha256"),
            "missing-start" => {
                o.as_object_mut()
                    .ok_or("object missing")?
                    .remove("started_utc_unix_ms");
            }
            "launch" => l["worker_sha256"] = json!("other-worker"),
            "result" => r["technical_component_passed"] = json!(false),
            "result-worker" => r["worker_sha256"] = json!("other-worker"),
            _ => return Err("unexpected owner case".into()),
        }
        assert!(
            reviewed_code_owner_start(
                &o,
                &l,
                &r,
                "reviewed-owner",
                "qwen38-4k:latest",
                &context,
                "actual-context-sha256"
            )
            .is_err(),
            "{case}"
        );
    }
    Ok(())
}

// This test fixture has two explicit context settings and two provider-reported V1
// tool modes. Select only a complete content-derived identity matching the current
// durable Run; never alter a profile ID or re-probe after its coding owner stopped.
fn reviewed_fixture_profile(tag: &str, active: ModelProfileReference) -> TestResult<ModelProfile> {
    if !["qwen38-4k:latest", "ornith-1.5:9b"].contains(&tag) {
        return Err("unsupported reviewed fixture model".into());
    }
    let mut matched = None;
    for context in [HttpFixtureContext::FourK, HttpFixtureContext::SixteenK] {
        let template = ollama_profile_with_context(tag, context)?;
        for mode in [
            ModelToolCallMode::Disabled,
            ModelToolCallMode::NativeProviderReported,
        ] {
            let profile = ModelProfile::from_probe(
                template.provider_id().clone(),
                template.model_id().clone(),
                template.settings().clone(),
                ModelCapabilities::new(ModelStructuredOutputCapability::Verified, mode),
            );
            if profile.reference() == active && matched.replace(profile).is_some() {
                return Err("ambiguous reviewed fixture profile".into());
            }
        }
    }
    matched.ok_or_else(|| "durable profile does not match this exact verified fixture".into())
}

#[derive(Debug)]
struct FixtureProfileProbe {
    provider: ModelProviderId,
    mode: ModelToolCallMode,
}
impl ModelCapabilityProbe for FixtureProfileProbe {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider
    }
    fn probe<'a>(
        &'a self,
        request: &'a ModelCapabilityProbeRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelCapabilityProbeFuture<'a> {
        Box::pin(async move {
            Ok(ModelCapabilityObservation::new(
                Some(
                    ReportedModelContextLimit::new(request.settings().context_limit().get())
                        .map_err(|_| ModelProviderFailure::Rejected)?,
                ),
                ModelCapabilities::new(ModelStructuredOutputCapability::Verified, self.mode),
            ))
        })
    }
}

#[test]
fn http_correction_edit_reviewed_profile_retains_actual_probed_capability_and_durable_reference()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
                    for mode in [
                        ModelToolCallMode::Disabled,
                        ModelToolCallMode::NativeProviderReported,
                    ] {
                        let template = ollama_profile(tag)?;
                        let probe = FixtureProfileProbe {
                            provider: template.provider_id().clone(),
                            mode,
                        };
                        let actual = ProbeModelProfile::new(&probe)
                            .execute(
                                &ModelCapabilityProbeRequest::new(
                                    template.model_id().clone(),
                                    template.settings().clone(),
                                ),
                                ModelRequestTimeout::DEFAULT,
                                &ActiveControl,
                            )
                            .await?;
                        assert_eq!(actual.capabilities().tool_call_mode(), mode);
                        if mode == ModelToolCallMode::NativeProviderReported {
                            assert_ne!(actual.reference(), template.reference());
                        }
                        let CurrentPlanFixture { http, run, .. } =
                            full_fixture(actual.clone(), false).await?;
                        let run_id = run.id();
                        let PreparedHttpFixture {
                            fixture,
                            store,
                            refresh,
                            compiler,
                            ..
                        } = http;
                        drop((store, refresh, compiler));
                        let cold =
                            LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                        let durable = cold
                            .load_agent_run(&fixture.project, run_id)
                            .await?
                            .ok_or("cold profiled Run missing")?;
                        let active = durable
                            .model_profile()
                            .ok_or("cold active profile missing")?;
                        assert_eq!(active, actual.reference());
                        assert_eq!(reviewed_fixture_profile(tag, active)?, actual);
                        let other = if tag == "qwen38-4k:latest" {
                            "ornith-1.5:9b"
                        } else {
                            "qwen38-4k:latest"
                        };
                        assert!(reviewed_fixture_profile(other, active).is_err());
                        assert_eq!(
                            cold.load_agent_run(&fixture.project, run_id).await?,
                            Some(durable)
                        );
                        drop(cold);
                        fixture.cleanup()?;
                    }
                }
                let template = ollama_profile("qwen38-4k:latest")?;
                let unavailable = ModelProfile::from_probe(
                    template.provider_id().clone(),
                    template.model_id().clone(),
                    template.settings().clone(),
                    ModelCapabilities::new(
                        ModelStructuredOutputCapability::Unavailable,
                        ModelToolCallMode::Disabled,
                    ),
                );
                assert!(
                    reviewed_fixture_profile("qwen38-4k:latest", unavailable.reference()).is_err()
                );
                let wider = ModelProfile::from_probe(
                    template.provider_id().clone(),
                    template.model_id().clone(),
                    ModelProfileSettings::new(
                        ModelContextLimit::new(8192)?,
                        ModelOutputLimit::new(1024)?,
                        ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
                        ModelParallelismLimit::new(1)?,
                        template.settings().sampling(),
                        ModelStopSequences::empty(),
                        ModelPromptSchemaGrounding::FormatFieldOnly,
                    )?,
                    template.capabilities(),
                );
                assert!(reviewed_fixture_profile("qwen38-4k:latest", wider.reference()).is_err());
                assert!(reviewed_fixture_profile("unsupported", template.reference()).is_err());
                Ok(())
            })
    })
}

#[test]
fn http_correction_explicit_sixteen_k_profile_keeps_actual_wire_reserve_and_cold_identity()
-> TestResult {
    crate::run_native_libsql_test(async {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let provider = OllamaModelProvider::for_projected_wire(
                    OllamaEndpoint::parse("http://127.0.0.1:11434")?,
                    Arc::new(LocalOnlyOllamaEndpointPolicy),
                )?;
                for tag in ["qwen38-4k:latest", "ornith-1.5:9b"] {
                    for mode in [
                        ModelToolCallMode::Disabled,
                        ModelToolCallMode::NativeProviderReported,
                    ] {
                        let template =
                            ollama_profile_with_context(tag, HttpFixtureContext::SixteenK)?;
                        let probe = FixtureProfileProbe {
                            provider: template.provider_id().clone(),
                            mode,
                        };
                        let actual = ProbeModelProfile::new(&probe)
                            .execute(
                                &ModelCapabilityProbeRequest::new(
                                    template.model_id().clone(),
                                    template.settings().clone(),
                                ),
                                ModelRequestTimeout::DEFAULT,
                                &ActiveControl,
                            )
                            .await?;
                        assert_ne!(actual.reference(), ollama_profile(tag)?.reference());
                        assert_eq!(actual.settings().context_limit().get(), 16384);
                        let f = full_fixture(actual.clone(), false).await?;
                        assert_eq!(
                            f.http
                                .goal
                                .original()
                                .ok_or("original missing")?
                                .as_bytes()
                                .len(),
                            1207
                        );
                        for kind in [
                            EditPhaseCallKind::Initial,
                            EditPhaseCallKind::StructuralRepair,
                        ] {
                            let request = f.phase.request_for(kind)?;
                            for constraint in f.http.goal.draft().constraints() {
                                assert!(
                                    request.messages()[1]
                                        .content()
                                        .contains(constraint.as_str())
                                );
                            }
                            let wire = provider.project_wire_request(&request)?;
                            let budget = PhaseWireBudget::fit(wire, request.profile())?;
                            assert_eq!(
                                (wire.context_tokens(), wire.output_tokens()),
                                (16384, 1024)
                            );
                            assert_eq!(budget.output_tokens(), 3605);
                            assert!(budget.prompt_tokens() + budget.output_tokens() <= 16384);
                        }
                        let run_id = f.run.id();
                        let goal = f.http.goal.clone();
                        let PreparedHttpFixture {
                            fixture,
                            store,
                            refresh,
                            compiler,
                            ..
                        } = f.http;
                        drop((store, refresh, compiler));
                        let cold =
                            LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
                        let durable = cold
                            .load_agent_run(&fixture.project, run_id)
                            .await?
                            .ok_or("cold Run missing")?;
                        let active = durable.model_profile().ok_or("cold profile missing")?;
                        assert_eq!(active, actual.reference());
                        assert_eq!(reviewed_fixture_profile(tag, active)?, actual);
                        assert_eq!(
                            cold.load_current_goal_contract(&fixture.project, goal.task_id())
                                .await?,
                            Some(goal)
                        );
                        drop(cold);
                        fixture.cleanup()?;
                    }
                }
                Ok(())
            })
    })
}

#[test]
fn http_correction_fixture_context_selection_is_explicit_and_closed() -> TestResult {
    assert_eq!(
        HttpFixtureContext::from_setting(None)?,
        HttpFixtureContext::FourK
    );
    assert_eq!(
        HttpFixtureContext::from_setting(Some("4096"))?,
        HttpFixtureContext::FourK
    );
    assert_eq!(
        HttpFixtureContext::from_setting(Some("16384"))?,
        HttpFixtureContext::SixteenK
    );
    for value in [
        "", "4k", "16k", "8192", "65536", " 16384", "16384 ", "016384",
    ] {
        assert!(HttpFixtureContext::from_setting(Some(value)).is_err());
    }
    Ok(())
}

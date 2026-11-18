//! Pure documentation delivery with the actual patch, evidence and acceptance ports.
use super::*;
use a3_domain::{
    DiffEvidenceSource, GoalContractReference, PatchAction, PatchActionSchemaVersion,
    PatchFileContent, PatchOperation, PatchRationale, PatchUpdate,
};

const BEFORE: &[u8] = b"# Project draft\n";
const AFTER: &[u8] = b"# Project notes\n\nDocumentation for the project.\n";
const UNCHANGED: &[u8] = b"# Existing notes\n";

#[test]
fn approved_readme_only_change_reaches_source_done_after_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            fs::write(worktree.join("README.md"), BEFORE)?;
            fs::write(worktree.join("NOTES.md"), UNCHANGED)?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;

            let (goal, evidence, snapshot) = apply_documentation_patch(&fixture).await?;
            let expected = vec![
                (PathBuf::from("NOTES.md"), UNCHANGED.to_vec()),
                (PathBuf::from("README.md"), AFTER.to_vec()),
            ];
            assert_eq!(inventory(&worktree)?, expected);
            finish_reopened_source(&fixture, goal, evidence, snapshot, 30).await?;
            assert_eq!(inventory(&worktree)?, expected);
            fixture.cleanup()?;
            Ok(())
        })
    })
}

async fn apply_documentation_patch(
    fixture: &Fixture,
) -> TestResult<(GoalContract, TaskEvidenceId, SnapshotId)> {
    let path = RepositoryPath::try_from_bytes(b"README.md".to_vec())?;
    let invariant =
        DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?;
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"Update README.md only.\n".to_vec(),
    )?;
    let criterion =
        criterion(2, "README.md is the sole changed file")?.with_origin(CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            CriterionEvidenceRequirement::ChangeOnly(invariant.clone()),
        ));
    let criterion_id = criterion.id();
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Update README.md only".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Verify the exact changed path".to_owned())?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    let step_id = TaskStepId::from_bytes([STEP; 32]);
    let spec_id = VerificationSpecId::from_bytes([213; 32]);
    let step = TaskStepDefinition::new(
        step_id,
        None,
        TaskStepOutcome::try_from_string("Update the README documentation".to_owned())?,
        TaskStepRationale::try_from_string("Apply the requested README-only edit".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "An actual diff containing exactly README.md".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            spec_id,
            VerificationRequirement::try_from_string(
                "README.md is the only changed path".to_owned(),
            )?,
            invariant,
        ),
    )?
    .with_acceptance_criteria(vec![criterion_id])?;

    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let profile = profile()?;
    let (mut run, stored, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, base, vec![step]).await?;
    assert!(unplanned.is_empty());
    let (mut ledger, mut version) = stored.into_parts();
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path() == &path)
        .ok_or("README missing from the initial publication")?
        .clone();
    assert_eq!(
        revision.content_hash().as_bytes(),
        blake3::hash(BEFORE).as_bytes()
    );
    let action = PatchAction::new(
        PatchActionSchemaVersion::V1,
        run.id(),
        fixture.project.worktree().id(),
        base,
        step_id,
        spec_id,
        PatchRationale::try_from_string("Update the existing README documentation".to_owned())?,
        vec![PatchOperation::Update(PatchUpdate::new(
            revision,
            PatchFileContent::try_from_bytes(AFTER.to_vec())?,
        )?)],
    )?;

    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = WorkspacePatchAdapter::new();
    let runner = native_runner()?;
    let factory = BuiltinProcessVerificationEvidenceFactory;
    let context = UnexpectedContext::default();
    let events = BoundedEvents::default();
    let controller = ExecuteMutatingAgentAction::new(
        &coordinator,
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &inspection,
        &approvals,
        &patch,
        &runner,
        &factory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let outcome = execute_approved(
        controller,
        fixture,
        &store,
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        AgentAction::ApplyPatch(Box::new(action)),
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!(
            "the actual README patch did not verify its path relation: {outcome:?}"
        )
        .into());
    };
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.current_snapshot_id(), snapshot_id);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    assert!(
        events
            .values
            .lock()
            .map_err(|_| "event capture poisoned")?
            .is_empty()
    );
    let attempts = store
        .load_agent_mutation_attempts(&fixture.project, run.id())
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("expected one actual mutation attempt".into());
    };
    assert_eq!(attempt.kind(), AgentMutationKind::Patch);
    assert_eq!(
        attempt.tool_attempt().status(),
        AgentToolAttemptStatus::Succeeded
    );

    let state = store
        .load_verification_state(
            &fixture.project,
            goal.task_id(),
            &[evidence_id],
            snapshot_id,
            Duration::from_secs(10),
            &ActiveControl,
        )
        .await?;
    let [VerificationEvidence::Diff(diff)] = state.evidence() else {
        return Err("expected one persisted DiffEvidence".into());
    };
    assert!(matches!(diff.source(), DiffEvidenceSource::Patch { .. }));
    assert!(diff.complete());
    assert_eq!(diff.base_snapshot_id(), base);
    assert_eq!(diff.snapshot_id(), snapshot_id);
    assert_eq!(diff.spec_id(), spec_id);
    assert_eq!(diff.changed_paths(), std::slice::from_ref(&path));
    Ok((goal, evidence_id, snapshot_id))
}

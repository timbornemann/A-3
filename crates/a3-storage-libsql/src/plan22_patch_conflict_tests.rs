//! Native conflict observation through the existing approval, patch and index ports.
use super::*;
use a3_application::{
    AuthorizedPatchAction, PatchApplyFuture, PatchPreviewFuture, RunEventPageLimit,
    WorkspacePatchTool,
};
use a3_domain::{
    AgentMutationDisposition, ContentHash, GoalContractReference, PatchAction,
    PatchActionSchemaVersion, PatchFileContent, PatchOperation, PatchRationale, PatchUpdate,
    RunEventKind, RunEventOutcome,
};

const FIRST_BEFORE: &[u8] = b"first before\n";
const FIRST_AFTER: &[u8] = b"first agent\n";
const SECOND_BEFORE: &[u8; 14] = b"second before\n";
const FOREIGN: &[u8; 14] = b"second editor\n";

#[test]
fn conflict_refresh_publishes_the_foreign_hash_before_retry_context() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for trigger in [0, 1] {
                let mut fixture = Fixture::new()?;
                let root = fixture.root.join("worktree");
                initialize_git(&root)?;
                fs::write(root.join("a-first.txt"), FIRST_BEFORE)?;
                fs::write(root.join("z-second.txt"), SECOND_BEFORE)?;
                fixture.project = RepositoryInspector::new().inspect(&root)?;
                observe_conflict(&fixture, trigger).await?;
                assert_eq!(fs::read(root.join("z-second.txt"))?, FOREIGN);
                let expected_first = if trigger == 0 {
                    FIRST_BEFORE
                } else {
                    FIRST_AFTER
                };
                assert_eq!(fs::read(root.join("a-first.txt"))?, expected_first);
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

#[derive(Debug)]
struct ForeignEditPatch {
    adapter: WorkspacePatchAdapter,
    path: PathBuf,
    trigger: u64,
}
impl WorkspacePatchTool for ForeignEditPatch {
    fn preview<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        action: &'a PatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchPreviewFuture<'a> {
        self.adapter.preview(project, published, action, control)
    }

    fn apply<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        authorized: AuthorizedPatchAction,
        control: &'a dyn WorkspacePatchControl,
    ) -> PatchApplyFuture<'a> {
        Box::pin(async move {
            let observer = ForeignEditProgress {
                control,
                path: &self.path,
                trigger: self.trigger,
            };
            self.adapter
                .apply(project, published, authorized, &observer)
                .await
        })
    }
}

#[derive(Debug)]
struct ForeignEditProgress<'a> {
    control: &'a dyn WorkspacePatchControl,
    path: &'a Path,
    trigger: u64,
}
impl WorkspacePatchControl for ForeignEditProgress<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), WorkspacePatchProgressError> {
        self.control.report_progress(progress)?;
        if progress.completed() == Some(self.trigger) {
            fs::write(self.path, FOREIGN).map_err(|_| WorkspacePatchProgressError)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ObserveRetryContext<'a> {
    store: &'a LibsqlKnowledgeStore,
    observed: Mutex<Option<(SnapshotId, ContentHash)>>,
}
impl AgentContextCompiler for ObserveRetryContext<'_> {
    fn compile<'a>(
        &'a self,
        input: &'a AgentContextCompileInput,
        _control: &'a dyn ContextCompileControl,
    ) -> AgentContextCompilerFuture<'a> {
        Box::pin(async move {
            let current = self
                .store
                .latest_published_index(input.project(), &ActiveControl)
                .await
                .map_err(|_| ContextCompileFailure::InvalidPack)?
                .ok_or(ContextCompileFailure::InvalidPack)?;
            let second = current
                .publication()
                .graph()
                .files()
                .iter()
                .find(|revision| revision.path().as_bytes() == b"z-second.txt")
                .ok_or(ContextCompileFailure::InvalidPack)?;
            *self
                .observed
                .lock()
                .map_err(|_| ContextCompileFailure::InvalidPack)? =
                Some((current.run().snapshot_id(), second.content_hash()));
            // End the test at the context boundary. The observation above used the real index.
            Err(ContextCompileFailure::InvalidPack)
        })
    }
}

async fn observe_conflict(fixture: &Fixture, trigger: u64) -> TestResult {
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let (goal, step) = conflict_task()?;
    let step_id = step.id();
    let spec_id = step.verification_spec().id();
    let profile = profile()?;
    let (mut run, stored, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, base, vec![step]).await?;
    assert!(unplanned.is_empty());
    let (mut ledger, mut version) = stored.into_parts();
    let operations = [
        (b"a-first.txt".as_slice(), FIRST_AFTER),
        (b"z-second.txt".as_slice(), b"second agent\n".as_slice()),
    ]
    .into_iter()
    .map(|(path, bytes)| {
        let expected = published
            .publication()
            .graph()
            .files()
            .iter()
            .find(|revision| revision.path().as_bytes() == path)
            .ok_or("initial published revision missing")?
            .clone();
        Ok(PatchOperation::Update(PatchUpdate::new(
            expected,
            PatchFileContent::try_from_bytes(bytes.to_vec())?,
        )?))
    })
    .collect::<TestResult<Vec<_>>>()?;
    let action = PatchAction::new(
        PatchActionSchemaVersion::V1,
        run.id(),
        fixture.project.worktree().id(),
        base,
        step_id,
        spec_id,
        PatchRationale::try_from_string("Change the two bound files".to_owned())?,
        operations,
    )?;

    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
    let patch = ForeignEditPatch {
        adapter: WorkspacePatchAdapter::new(),
        path: fixture.root.join("worktree/z-second.txt"),
        trigger,
    };
    let runner = native_runner()?;
    let context = ObserveRetryContext {
        store: &store,
        observed: Mutex::new(None),
    };
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
        &BuiltinProcessVerificationEvidenceFactory,
        &context,
        &refresh,
    )
    .with_permissions(store.as_ref());
    let seed = MutationContextSeed::new(goal.clone(), profile, Vec::new(), Vec::new());
    let result = execute_approved(
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
    .await;
    let error = result
        .err()
        .ok_or("conflict unexpectedly produced a successful outcome")?;
    assert!(matches!(
        error.downcast_ref::<MutationControllerFailure>(),
        Some(MutationControllerFailure::Context(
            ContextCompileFailure::InvalidPack
        ))
    ));
    let current = run.current_snapshot_id();
    assert_ne!(current, base);
    assert_eq!(
        *context
            .observed
            .lock()
            .map_err(|_| "context observation poisoned")?,
        Some((
            current,
            ContentHash::from_bytes(*blake3::hash(FOREIGN).as_bytes())
        )),
    );
    let attempts = store
        .load_agent_mutation_attempts(&fixture.project, run.id())
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("expected exactly one patch attempt".into());
    };
    assert_eq!(attempt.kind(), AgentMutationKind::Patch);
    assert_eq!(
        attempt.tool_attempt().status(),
        AgentToolAttemptStatus::Failed
    );
    assert_eq!(
        attempt.disposition(),
        if trigger == 0 {
            AgentMutationDisposition::NotApplied
        } else {
            AgentMutationDisposition::Applied
        }
    );
    let page = store
        .load_run_events(
            &fixture.project,
            run.id(),
            None,
            RunEventPageLimit::new(32)?,
        )
        .await?;
    assert!(!page.has_more());
    let mut tool_events = page
        .events()
        .iter()
        .filter(|event| event.kind() == RunEventKind::ToolAction);
    let failure = tool_events.next().ok_or("tool failure event missing")?;
    assert_eq!(failure.payload().outcome(), Some(RunEventOutcome::Failed));
    assert_eq!(failure.snapshot_id(), current);
    assert!(tool_events.next().is_none());
    let step = ledger.step(step_id).ok_or("step missing")?;
    assert_eq!(step.status(), TaskStepStatus::InProgress);
    let attempt = step.attempts().last().ok_or("step attempt missing")?;
    assert!(attempt.verification().is_none());
    assert!(attempt.evidence_ids().is_empty());
    Ok(())
}

fn conflict_task() -> TestResult<(GoalContract, TaskStepDefinition)> {
    let invariant = DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![
            RepositoryPath::try_from_bytes(b"a-first.txt".to_vec())?,
            RepositoryPath::try_from_bytes(b"z-second.txt".to_vec())?,
        ],
    )?;
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        b"Change a-first.txt and z-second.txt only.\n".to_vec(),
    )?;
    let criterion =
        criterion(2, "Exactly the two requested files change")?.with_origin(CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            CriterionEvidenceRequirement::ChangeOnly(invariant.clone()),
        ));
    let id = criterion.id();
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Update the two named files".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Verify both changed paths".to_owned())?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    let step = TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string("Update only the selected files".to_owned())?,
        TaskStepRationale::try_from_string("The requested changes are independent".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Both exact changed paths".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string("The approved two paths changed".to_owned())?,
            invariant,
        ),
    )?
    .with_acceptance_criteria(vec![id])?;
    Ok((goal, step))
}

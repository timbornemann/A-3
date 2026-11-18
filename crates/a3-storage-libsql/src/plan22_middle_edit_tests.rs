#[path = "plan22_bound_source_reader_tests.rs"]
mod bound_source_reader_tests;
#[path = "plan22_p4_live_edit.rs"]
pub(super) mod p4_live_edit;
#[path = "plan22_prepolicy_conflict_tests.rs"]
mod prepolicy_conflict_tests;
#[path = "plan22_short_edit_tests.rs"]
mod short_edit_tests;
#[path = "plan22_target_read_edit_binding_tests.rs"]
mod target_read_binding_tests;

use super::*;
use a3_application::{
    AgentApprovalAction, AgentApprovalFileOperation, AgentApprovalPresentationState,
    AgentDiffLineEnding, AgentDiffRow, AgentSourcePage, AgentSourceReadControl, AgentSourceReader,
    EditPhaseFailure, EditPhaseOutcome, EditPhaseState, PatchConflictKind, PatchPreviewFailure,
    PreparedEditPhase,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, AgentMutationDisposition,
    ContentHash, DiffEvidenceSource, GoalContractReference, MutationActionFingerprint,
    PatchLineEndings, PatchOperation, PatchTextEncoding,
};
use a3_workspace::WorkspaceAgentSourceReader;
use serde_json::{Value, json};

#[path = "plan22_edit_unknown_tests.rs"]
mod unknown_tests;

#[path = "plan22_edit_phase_reservation_tests.rs"]
mod reservation_tests;

#[path = "plan22_intake_phase_reservation_tests.rs"]
mod intake_reservation_tests;

const SUBJECT_PATH: &str = "subject.py";
const MIDDLE_PATH: &str = "subject.md";
const OLD_LINE: &str = "label = \"Grüße λ🙂\"";
const NEW_LINE: &str = "label = \"Änderung Ω🦀\"";
const OTHER_FILE: &[u8] = b"unrelated file remains byte-identical\r\n";

impl AgentSourceReadControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[test]
fn actual_middle_edit_shows_approved_absolute_hunk_and_reopens_verified() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;

            let guard = "# unberührter Wächter α\r\n".repeat(4_500);
            let prefix = format!("{OLD_LINE}\r\n{guard}# target follows\r\n");
            let suffix = format!("# after target\r\n{guard}");
            assert!(prefix.len() > 100 * 1_024 && suffix.len() > 100 * 1_024);
            let before = format!("{prefix}{OLD_LINE}\r\n{suffix}");
            let expected = format!("{prefix}{NEW_LINE}\r\n{suffix}");
            assert!(before.len() > 200 * 1_024);
            assert_eq!(before.matches(OLD_LINE).count(), 2);
            fs::write(worktree.join(MIDDLE_PATH), before.as_bytes())?;
            fs::write(worktree.join("notes.txt"), OTHER_FILE)?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;

            let (goal, evidence, snapshot) =
                apply_middle_window(&fixture, &prefix, &before, &expected).await?;
            let expected_inventory = vec![
                (PathBuf::from("notes.txt"), OTHER_FILE.to_vec()),
                (PathBuf::from(MIDDLE_PATH), expected.into_bytes()),
            ];
            assert!(
                inventory(&worktree)? == expected_inventory,
                "only the exact second occurrence may change"
            );
            finish_reopened_source(&fixture, goal, evidence, snapshot, 30).await?;
            assert!(
                inventory(&worktree)? == expected_inventory,
                "reopen and acceptance may not alter any worktree bytes"
            );
            fixture.cleanup()?;
            Ok(())
        })
    })
}

async fn apply_middle_window(
    fixture: &Fixture,
    prefix: &str,
    before: &str,
    expected: &str,
) -> TestResult<(GoalContract, TaskEvidenceId, SnapshotId)> {
    let path = RepositoryPath::try_from_bytes(MIDDLE_PATH.as_bytes().to_vec())?;
    let (goal, step) = path_edit_goal(
        &path,
        b"Replace the second matching assignment in subject.md and preserve all surrounding bytes.\n",
    )?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let profile = profile()?;
    let (mut run, stored, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, base, vec![step]).await?;
    assert!(unplanned.is_empty());
    let run_id = run.id();
    assert_eq!(
        store.load_agent_permissions().await?.mode(),
        AgentPermissionMode::AskPermissions
    );
    let before_hash = ContentHash::from_bytes(*blake3::hash(before.as_bytes()).as_bytes());
    let after_hash = ContentHash::from_bytes(*blake3::hash(expected.as_bytes()).as_bytes());
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("published subject missing")?;
    assert_eq!(revision.content_hash(), before_hash);

    let line_number = u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count())? + 1;
    let old_text = format!("{OLD_LINE}\r\n");
    let page = WorkspaceAgentSourceReader
        .read_page(
            &fixture.project,
            revision,
            &AgentFileInspection::new(
                path.clone(),
                AgentFileStartLine::new(line_number)?,
                AgentFileLineCount::new(1)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert_eq!(page.revision(), revision);
    assert_eq!(page.range().start_byte(), u32::try_from(prefix.len())?);
    assert_eq!(
        page.range().end_byte(),
        u32::try_from(prefix.len() + old_text.len())?
    );
    assert_eq!(page.text(), old_text);
    assert!(page.next_start_line().is_some());

    let state = || EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published);
    let phase = PreparedEditPhase::update(state()?, &page)?;
    let forged_page = AgentSourcePage::new(
        revision.clone(),
        page.range(),
        page.start_line(),
        old_text.replacen("label", "table", 1),
        page.next_start_line(),
        page.truncated(),
    )?;
    let forged_phase = PreparedEditPhase::update(state()?, &forged_page)?;
    assert_ne!(forged_phase.job_digest(), phase.job_digest());
    assert!(phase.input_json().len() < 1_024);
    let input: Value = serde_json::from_str(phase.input_json())?;
    assert_eq!(input.get("version").and_then(Value::as_u64), Some(3));
    assert_eq!(
        input.get("operation").and_then(Value::as_str),
        Some("replace")
    );
    assert_eq!(
        input.get("current").and_then(Value::as_str),
        Some(old_text.as_str())
    );
    assert!(input.get("window").is_none());
    // Reply routing belongs to the Core; the model supplies only canonical LF code.
    let response = serde_json::to_vec(&json!({
        "code": format!("{NEW_LINE}\n"),
    }))?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let first_page = WorkspaceAgentSourceReader
        .read_page(
            &fixture.project,
            revision,
            &AgentFileInspection::new(
                path.clone(),
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(1)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert_eq!(first_page.text(), page.text());
    let other_window = PreparedEditPhase::update(state()?, &first_page)?;
    let other_reply = phase.bind_reply(&response)?;
    assert!(matches!(
        other_window
            .complete(other_reply, state()?, &patch, &ActiveControl)
            .await,
        Err(EditPhaseFailure::WrongReplyBinding)
    ));

    // Change an outside byte without changing the selected source or the file length.
    let subject = fixture.root.join("worktree").join(MIDDLE_PATH);
    let foreign = before.replacen("# target follows", "# target followS", 1);
    assert_eq!(foreign.len(), before.len());
    assert_ne!(foreign, before);
    fs::write(&subject, foreign.as_bytes())?;
    let stale = PreparedEditPhase::update(state()?, &page)?;
    let stale_reply = stale.bind_reply(&response)?;
    let stale_result = stale
        .complete(stale_reply, state()?, &patch, &ActiveControl)
        .await;
    assert_eq!(fs::read(&subject)?, foreign.as_bytes());
    fs::write(&subject, before.as_bytes())?;
    assert!(matches!(
        stale_result,
        Err(EditPhaseFailure::ReadBase(PatchPreviewFailure::Conflict(
            PatchConflictKind::SourceChangedOnDisk,
        )))
    ));

    let reply = phase.bind_reply(&response)?;
    let outcome = phase
        .complete(reply, state()?, &patch, &ActiveControl)
        .await?;
    let EditPhaseOutcome::Proposal(action) = outcome else {
        return Err("the bound replacement did not produce a proposal".into());
    };
    let AgentAction::ApplyPatch(compiled) = &action else {
        return Err("the edit phase did not compile to the existing patch action".into());
    };
    assert_eq!(compiled.run_id(), run_id);
    assert_eq!(compiled.worktree_id(), fixture.project.worktree().id());
    assert_eq!(compiled.snapshot_id(), base);
    assert_eq!(compiled.task_step_id(), TaskStepId::from_bytes([STEP; 32]));
    assert_eq!(
        compiled.verification_spec_id(),
        VerificationSpecId::from_bytes([213; 32])
    );
    let [PatchOperation::Update(update)] = compiled.operations() else {
        return Err("expected exactly one compiled update".into());
    };
    assert_eq!(update.expected(), revision);
    assert!(
        update.content().as_bytes() == expected.as_bytes(),
        "the compiled update must preserve all bytes outside the delivered window"
    );
    assert_eq!(update.content().content_hash(), after_hash);
    let policy_action = compiled.policy_action();
    let mutation_fingerprint = MutationActionFingerprint::from_action(&action)?;

    let coordinator = WorktreeMutationCoordinator::new();
    let inspection = AgentInspectionBuffer::new();
    inspection.activate_project(&fixture.project);
    let approvals = AgentApprovalBuffer::new();
    approvals.activate_project(&fixture.project);
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
    let (mut ledger, mut version) = stored.into_parts();
    let outcome = execute_approved_observing(
        controller,
        fixture,
        &store,
        &mut run,
        &mut ledger,
        &mut version,
        &published,
        action,
        None,
        60,
        10,
        &seed,
        &mut compiler,
        &events,
        async |request| {
            let pending = approvals
                .presentation(&fixture.project, goal.task_id())?
                .ok_or("pending approval presentation missing")?;
            assert_eq!(pending.request_id(), request);
            assert_eq!(pending.state(), AgentApprovalPresentationState::Pending);
            assert_eq!(pending.action_fingerprint(), policy_action.fingerprint());
            assert_eq!(pending.scope_digest(), policy_action.scope_digest());
            let AgentApprovalAction::Patch(approval_patch) = pending.action() else {
                return Err("expected a pending patch approval".into());
            };
            let [approval_file] = approval_patch.files() else {
                return Err("expected one pending patch path".into());
            };
            assert_eq!(
                approval_file.operation(),
                AgentApprovalFileOperation::Update
            );
            assert_eq!(approval_file.source_path(), Some(&path));
            assert_eq!(approval_file.target_path(), Some(&path));

            let overview = inspection
                .overview(&fixture.project, goal.task_id())?
                .ok_or("task inspection missing before approval")?;
            let preview = overview
                .patch()
                .ok_or("patch preview missing before approval")?;
            assert_eq!(preview.context(), pending.context());
            assert_eq!(preview.context().snapshot_id(), base);
            assert_eq!(preview.context().run_id(), run_id);
            assert_eq!(
                preview.context().step_id(),
                TaskStepId::from_bytes([STEP; 32])
            );
            assert!(preview.retained_bytes() <= 64 * 1_024);
            let [file] = preview.files() else {
                return Err("expected one file in the approval preview".into());
            };
            assert_eq!(file.source_path(), Some(&path));
            assert_eq!(file.target_path(), Some(&path));
            assert!(file.content_truncated());
            for (side, bytes, digest) in [
                (file.before(), before, before_hash),
                (file.after(), expected, after_hash),
            ] {
                let side = side.ok_or("full-content preview metadata missing")?;
                assert_eq!(side.content_hash(), digest);
                assert_eq!(side.total_bytes(), u64::try_from(bytes.len())?);
                assert_eq!(side.encoding(), PatchTextEncoding::Utf8);
                assert_eq!(side.line_endings(), PatchLineEndings::Crlf);
                assert!(side.text().len() <= 16 * 1_024);
            }
            let [hunk] = file.hunks() else {
                return Err("the actual middle edit must be visible before approval".into());
            };
            assert_eq!(file.removed_lines(), 1);
            assert_eq!(file.added_lines(), 1);
            assert!(hunk.rows().iter().any(|row| matches!(
                row,
                AgentDiffRow::Removed { before_line, line }
                    if *before_line == line_number
                        && line.text() == OLD_LINE
                        && line.ending() == AgentDiffLineEnding::Crlf
            )));
            assert!(hunk.rows().iter().any(|row| matches!(
                row,
                AgentDiffRow::Added { after_line, line }
                    if *after_line == line_number
                        && line.text() == NEW_LINE
                        && line.ending() == AgentDiffLineEnding::Crlf
            )));

            let durable = store
                .load_agent_run(&fixture.project, run_id)
                .await?
                .ok_or("run missing before approval")?;
            assert_eq!(durable.state(), AgentControllerState::AwaitApproval);
            assert!(
                store
                    .load_agent_mutation_attempts(&fixture.project, run_id)
                    .await?
                    .is_empty()
            );
            let live = fs::read(fixture.root.join("worktree").join(MIDDLE_PATH))?;
            assert!(
                live == before.as_bytes(),
                "the approval preview may not write"
            );
            Ok(())
        },
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("approved middle edit did not verify: {outcome:?}").into());
    };
    assert_ne!(snapshot_id, base);
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
        .load_agent_mutation_attempts(&fixture.project, run_id)
        .await?;
    let [attempt] = attempts.as_slice() else {
        return Err("expected exactly one durable patch attempt".into());
    };
    assert_eq!(attempt.kind(), AgentMutationKind::Patch);
    assert_eq!(attempt.fingerprint(), mutation_fingerprint);
    assert_eq!(attempt.disposition(), AgentMutationDisposition::Applied);
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
        return Err("expected one actual patch diff artifact".into());
    };
    assert!(matches!(diff.source(), DiffEvidenceSource::Patch { .. }));
    assert!(diff.complete());
    assert_eq!(diff.base_snapshot_id(), base);
    assert_eq!(diff.snapshot_id(), snapshot_id);
    assert_eq!(diff.changed_paths(), std::slice::from_ref(&path));
    let published_subject = state
        .published_index()
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path)
        .ok_or("changed file missing from the published index")?;
    assert_eq!(published_subject.content_hash(), after_hash);
    Ok((goal, evidence_id, snapshot_id))
}

fn path_edit_goal(
    path: &RepositoryPath,
    request: &[u8],
) -> TestResult<(GoalContract, TaskStepDefinition)> {
    let invariant =
        DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![path.clone()])?;
    let task = TaskId::from_bytes([1; 32]);
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(task),
        request.to_vec(),
    )?;
    let criterion =
        criterion(2, "Change only the requested file")?.with_origin(CriterionOrigin::new(
            original.range(0, original.as_bytes().len())?,
            CriterionEvidenceRequirement::ChangeOnly(invariant.clone()),
        ));
    let criterion_id = criterion.id();
    let draft = GoalContractDraft::new(
        GoalObjective::try_from_string("Apply the requested source edit".to_owned())?,
        vec![criterion],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("Verify the exact changed path".to_owned())?,
    )?;
    let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(1)?)
        .with_original(original)?;
    let step = TaskStepDefinition::new(
        TaskStepId::from_bytes([STEP; 32]),
        None,
        TaskStepOutcome::try_from_string("Apply the selected source edit".to_owned())?,
        TaskStepRationale::try_from_string(
            "Use the bounded edit selected for this step".to_owned(),
        )?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "An actual diff containing exactly the requested path".to_owned(),
        )?],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes([213; 32]),
            VerificationRequirement::try_from_string(
                "The requested file is the only changed path".to_owned(),
            )?,
            invariant,
        ),
    )?
    .with_acceptance_criteria(vec![criterion_id])?;
    Ok((goal, step))
}

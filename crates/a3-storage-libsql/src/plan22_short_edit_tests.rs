use super::*;

#[derive(Clone, Copy)]
enum ShortEdit {
    FirstLine,
    Append,
    Add,
}

struct ShortCase {
    edit: ShortEdit,
    file: &'static str,
    request: &'static [u8],
    before: Option<&'static str>,
    current: &'static str,
    replacement: &'static str,
    expected: &'static str,
}

const CASES: [ShortCase; 3] = [
    ShortCase {
        edit: ShortEdit::FirstLine,
        file: "subject.md",
        request:
            b"Update the first line of subject.md and retain its encoding and all later bytes.\n",
        before: Some("\u{feff}title = 'alt'\r\nkeep = 'Wächter 🙂'\r\n"),
        current: "title = 'alt'\r\n",
        replacement: "title = 'neu λ'\n",
        expected: "\u{feff}title = 'neu λ'\r\nkeep = 'Wächter 🙂'\r\n",
    },
    ShortCase {
        edit: ShortEdit::Append,
        file: SUBJECT_PATH,
        request: b"Append a line at the end of subject.py and preserve all existing bytes.\n",
        before: Some("first = 'Grüße'\r\nlast = '🙂'\r\n"),
        current: "last = '🙂'\r\n",
        replacement: "added = 'Ω'\n",
        expected: "first = 'Grüße'\r\nlast = '🙂'\r\nadded = 'Ω'\r\n",
    },
    ShortCase {
        edit: ShortEdit::Add,
        file: SUBJECT_PATH,
        request: b"Create the new file subject.py.\n",
        before: None,
        current: "",
        replacement: "created = 'Grüße'\n",
        expected: "created = 'Grüße'\n",
    },
];

#[test]
fn actual_short_window_edits_keep_bom_bind_eof_and_create_absent_file() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for case in &CASES {
                let mut fixture = Fixture::new()?;
                let worktree = fixture.root.join("worktree");
                initialize_git(&worktree)?;
                if let Some(before) = case.before {
                    fs::write(worktree.join(case.file), before.as_bytes())?;
                }
                fs::write(worktree.join("notes.txt"), OTHER_FILE)?;
                fixture.project = RepositoryInspector::new().inspect(&worktree)?;

                let (goal, evidence, snapshot) = apply_short_edit(&fixture, case).await?;
                let expected = vec![
                    (PathBuf::from("notes.txt"), OTHER_FILE.to_vec()),
                    (PathBuf::from(case.file), case.expected.as_bytes().to_vec()),
                ];
                assert_eq!(inventory(&worktree)?, expected);
                finish_reopened_source(&fixture, goal, evidence, snapshot, 30).await?;
                assert_eq!(inventory(&worktree)?, expected);
                fixture.cleanup()?;
            }
            Ok(())
        })
    })
}

async fn apply_short_edit(
    fixture: &Fixture,
    case: &ShortCase,
) -> TestResult<(GoalContract, TaskEvidenceId, SnapshotId)> {
    let path = RepositoryPath::try_from_bytes(case.file.as_bytes().to_vec())?;
    let (goal, step) = path_edit_goal(&path, case.request)?;
    let (store, refresh, mut compiler, published) = index_fixture(fixture).await?;
    let base = published.run().snapshot_id();
    let profile = profile()?;
    let (mut run, stored, unplanned) =
        start_fixture_step(fixture, &store, &goal, &profile, base, vec![step]).await?;
    assert!(unplanned.is_empty());

    let state = || EditPhaseState::new(&fixture.project, &goal, &stored, &run, &published);
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|file| file.path() == &path);
    let (phase, operation) = match case.edit {
        ShortEdit::Add => {
            assert!(revision.is_none());
            (PreparedEditPhase::add(state()?, path.clone())?, "add")
        }
        edit => {
            let revision = revision.ok_or("published edit source missing")?;
            let append = matches!(edit, ShortEdit::Append);
            let line = if append { 2 } else { 1 };
            let page = WorkspaceAgentSourceReader
                .read_page(
                    &fixture.project,
                    revision,
                    &AgentFileInspection::new(
                        path.clone(),
                        AgentFileStartLine::new(line)?,
                        AgentFileLineCount::new(1)?,
                    ),
                    &ActiveControl,
                )
                .await?;
            if append {
                let before = case.before.ok_or("EOF fixture requires existing bytes")?;
                assert!(!page.truncated());
                assert_eq!(page.range().end_byte(), u32::try_from(before.len())?);
                (PreparedEditPhase::append(state()?, &page)?, "append")
            } else {
                assert_eq!(page.range().start_byte(), 0);
                assert!(page.text().starts_with('\u{feff}'));
                (PreparedEditPhase::update(state()?, &page)?, "replace")
            }
        }
    };
    let input: Value = serde_json::from_str(phase.input_json())?;
    assert_eq!(input.get("version").and_then(Value::as_u64), Some(3));
    assert_eq!(
        input.get("operation").and_then(Value::as_str),
        Some(operation)
    );
    assert_eq!(
        input.get("current").and_then(Value::as_str),
        Some(case.current)
    );
    assert!(input.get("window").is_none());
    let reply = serde_json::to_vec(&json!({
        "code": case.replacement,
    }))?;
    let patch = WorkspacePatchAdapter::with_change_window_preview();
    let reply = phase.bind_reply(&reply)?;
    let outcome = phase
        .complete(reply, state()?, &patch, &ActiveControl)
        .await?;
    let EditPhaseOutcome::Proposal(action) = outcome else {
        return Err("the bound replacement did not produce a proposal".into());
    };
    let AgentAction::ApplyPatch(compiled) = &action else {
        return Err("expected a compiled short patch".into());
    };
    match (case.edit, compiled.operations()) {
        (ShortEdit::FirstLine | ShortEdit::Append, [PatchOperation::Update(update)]) => {
            assert_eq!(Some(update.expected()), revision);
            assert_eq!(update.content().as_bytes(), case.expected.as_bytes());
        }
        (ShortEdit::Add, [PatchOperation::Add(add)]) => {
            assert_eq!(add.path(), &path);
            assert_eq!(add.content().as_bytes(), case.expected.as_bytes());
        }
        _ => return Err("short edit compiled to the wrong patch operation".into()),
    }

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
                .ok_or("short edit must await an actual grant")?;
            assert_eq!(pending.request_id(), request);
            assert_eq!(pending.state(), AgentApprovalPresentationState::Pending);
            let overview = inspection
                .overview(&fixture.project, goal.task_id())?
                .ok_or("short edit inspection missing")?;
            let preview = overview.patch().ok_or("short edit preview missing")?;
            assert_eq!(preview.context(), pending.context());
            let [file] = preview.files() else {
                return Err("short edit must preview exactly one file".into());
            };
            assert_eq!(file.target_path(), Some(&path));
            assert!(!file.hunks().is_empty());
            let disk = fixture.root.join("worktree").join(case.file);
            if let Some(before) = case.before {
                assert_eq!(fs::read(disk)?, before.as_bytes());
            } else {
                assert!(!disk.try_exists()?);
            }
            Ok(())
        },
    )
    .await?;
    let MutationControllerOutcome::StepVerified {
        evidence_id,
        snapshot_id,
    } = outcome
    else {
        return Err(format!("approved short edit did not verify: {outcome:?}").into());
    };
    assert_ne!(snapshot_id, base);
    assert_eq!(run.state(), AgentControllerState::Verify);
    assert_eq!(run.current_snapshot_id(), snapshot_id);
    assert_eq!(context.calls.load(Ordering::SeqCst), 0);
    Ok((goal, evidence_id, snapshot_id))
}

//! Actual Task Lens and safe original read under the candidate run journal.
use super::*;
#[path = "plan22_target_read_definition_tests.rs"]
mod target_definition_tests;
use a3_application::{
    AgentReadAction, AgentReadTimeout, AgentReadToolFailure, AgentReadTools, AppendAgentRead,
    BoundEditSource, BoundEditSourceFailure, CompileTaskLens, EditEvidenceNeed,
    EditPhasePrerequisites, EditPhaseRequestFailure, EditPhaseReservationStore, RunEventPageLimit,
    build_edit_phase_request,
};
use a3_context::DeterministicAgentReadTools;
use a3_domain::{
    AgentInspectAction, AgentInspectTarget, AgentRunUsage, GraphEndpoint, RunEventKind,
    RunEventSubject, SymbolId, SyntaxRelationKind, TaskLensSeed, TaskLensSeedSet, TaskLensSeedText,
    TaskLensTokenBudget,
};

const TARGET: &[u8] = b"def target(value):\n    return value + 1\n";
const CALLER_IMPORT: &str = "from subject import target\n\n";
const CALLER_BODY: &str =
    "def invoke(value):\n    \"\"\"Gr\u{fc}\u{df}e \u{3bb}.\"\"\"\n    return target(value)\n";

#[test]
fn actual_task_lens_caller_read_and_evidence_survive_candidate_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            let caller = format!("{CALLER_IMPORT}{CALLER_BODY}");
            fs::write(worktree.join("subject.py"), TARGET)?;
            fs::write(worktree.join("caller.py"), caller.as_bytes())?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;
            let before = inventory(&worktree)?;

            let (run_id, observed) = read_current_caller(&fixture).await?;
            assert_eq!(inventory(&worktree)?, before);

            let foreign = caller.replace('\u{3bb}', "\u{3a9}");
            assert_eq!(caller.len(), foreign.len());
            assert_ne!(caller.as_bytes(), foreign.as_bytes());
            fs::write(worktree.join("caller.py"), foreign.as_bytes())?;
            reject_changed_live_caller(&fixture, run_id).await?;
            assert_eq!(fs::read(worktree.join("caller.py"))?, foreign.as_bytes());
            assert_eq!(fs::read(worktree.join("subject.py"))?, TARGET);
            assert_eq!(inventory(&worktree)?.len(), before.len());

            let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
            let reopened = store
                .load_agent_run(&fixture.project, run_id)
                .await?
                .ok_or("candidate run lost after reopen")?;
            assert_eq!(reopened.state(), AgentControllerState::Execute);
            assert_eq!(reopened.usage(), AgentRunUsage::ZERO);
            assert_eq!(reopened.last_event_sequence(), observed.sequence);
            let events = store
                .load_run_events(&fixture.project, run_id, None, RunEventPageLimit::new(16)?)
                .await?;
            let tools = events
                .events()
                .iter()
                .filter(|event| event.kind() == RunEventKind::ToolAction)
                .map(|event| event.subject())
                .collect::<Vec<_>>();
            assert_eq!(
                tools,
                [
                    Some(RunEventSubject::Tool(ToolRunId::from_bytes([181; 32]))),
                    Some(RunEventSubject::Tool(ToolRunId::from_bytes([182; 32]))),
                ]
            );
            assert!(events.events().iter().all(|event| {
                event.kind() != RunEventKind::ModelInteraction && event.turn_charge().is_none()
            }));
            let retained = store
                .load_agent_tool_evidence(&fixture.project, run_id, &[observed.source.id()])
                .await?;
            assert_eq!(retained, vec![observed.source]);
            assert!(
                store
                    .load_agent_mutation_attempts(&fixture.project, run_id)
                    .await?
                    .is_empty()
            );
            let reserved = store
                .load_edit_phase_usage(&fixture.project, run_id)
                .await?;
            assert_eq!(reserved.calls, 0);
            assert_eq!(reserved.repairs, 0);
            assert_eq!(
                store
                    .interrupt_agent_tool_attempts(&fixture.project, run_id, at(21)?)
                    .await?,
                0
            );
            drop(store);
            fixture.cleanup()
        })
    })
}

struct ObservedRead {
    source: a3_domain::AgentToolEvidence,
    sequence: a3_domain::RunEventSequence,
}

async fn read_current_caller(fixture: &Fixture) -> TestResult<(AgentRunId, ObservedRead)> {
    use a3_application::SourceReadStore;
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let snapshot = published.run().snapshot_id();
    let callee = symbol_id(&published, b"subject.py", "target")?;
    let caller = symbol_id(&published, b"caller.py", "invoke")?;
    assert!(published.publication().graph().edges().iter().any(|edge| {
        edge.snapshot_id() == snapshot
            && edge.kind() == SyntaxRelationKind::Calls
            && edge.source() == &GraphEndpoint::Symbol(caller)
            && edge.target() == &GraphEndpoint::Symbol(callee)
    }));
    let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
    let (goal, step) = path_edit_goal(
        &path,
        b"Inspect the actual caller before changing subject.py.\n",
    )?;
    let profile = profile()?;
    let (mut run, ledger, _) =
        start_fixture_step(fixture, &store, &goal, &profile, snapshot, vec![step]).await?;
    assert_eq!(run.state(), AgentControllerState::Execute);

    let lens = compile_lens(&store, project, path.clone(), callee, caller).await?;
    let definition = BoundEditSource::for_symbol(project, &published, &lens, callee)?;
    let explicit =
        BoundEditSource::for_file_span(project, &published, &lens, definition.required())?;
    assert_eq!(
        definition.read_action(project, &published)?,
        explicit.read_action(project, &published)?
    );
    let bound = BoundEditSource::for_direct_caller(project, &published, &lens, callee, caller)?;
    assert!(matches!(
        BoundEditSource::for_direct_caller(project, &published, &lens, caller, callee),
        Err(BoundEditSourceFailure::MissingDirectCaller)
    ));
    assert_eq!(bound.required().revision().path().as_bytes(), b"caller.py");
    let target = WorkspaceAgentSourceReader
        .read_page(
            project,
            definition.required().revision(),
            &AgentFileInspection::new(
                path,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(2)?,
            ),
            &ActiveControl,
        )
        .await?;
    assert_eq!(target.text().as_bytes(), TARGET);
    let previous_job = {
        let state = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
        let phase = PreparedEditPhase::update(state, &target)?;
        assert!(matches!(
            build_edit_phase_request(
                &goal,
                &ledger,
                &phase,
                state,
                profile.clone(),
                None,
                EditPhasePrerequisites::none().with_minimum_replacement_bytes(8193),
            ),
            Err(EditPhaseRequestFailure::TooLarge {
                minimum_replacement_bytes: 8193,
                maximum_replacement_bytes: 8192,
            })
        ));
        let compile = |page| {
            build_edit_phase_request(
                &goal,
                &ledger,
                &phase,
                state,
                profile.clone(),
                None,
                EditPhasePrerequisites::none().with_required_source(
                    EditEvidenceNeed::Caller,
                    &bound,
                    page,
                ),
            )
        };
        assert!(matches!(
            compile(None),
            Err(EditPhaseRequestFailure::NeedEvidence(
                EditEvidenceNeed::Caller
            ))
        ));
        phase.job_digest()
    };

    let reader = DeterministicAgentReadTools::new(
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let metadata =
        AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::Symbol(caller)));
    let original = bound.read_action(project, &published)?;
    let mut observed = None;
    let mut acquired = None;
    for (id, time, action) in [(181_u8, 14_u64, metadata), (182, 16, original)] {
        let tool = ToolRunId::from_bytes([id; 32]);
        let attempt = if id == 181 {
            store
                .begin_agent_tool_attempt(project, run.id(), snapshot, tool, at(time)?)
                .await?
        } else {
            let anchor = EditPhaseState::new(project, &goal, &ledger, &run, &published)?.anchor();
            store
                .begin_source_read(project, anchor, &bound, tool, at(time)?)
                .await?
                .attempt()
        };
        assert_eq!(attempt.status(), AgentToolAttemptStatus::InFlight);
        let mut read = reader
            .execute(
                project,
                snapshot,
                tool,
                &action,
                AgentReadTimeout::DEFAULT,
                &ActiveControl,
            )
            .await?;
        let result = bound.take_original(project, &published, tool, &mut read);
        if id == 181 {
            assert_eq!(result, Err(BoundEditSourceFailure::MissingOriginal));
        } else {
            let page = result?;
            assert_eq!(page.text().as_bytes(), CALLER_BODY.as_bytes());
            assert!(!page.text().as_bytes().contains(&b'\r'));
            assert_eq!(page.revision(), bound.required().revision());
            assert_eq!(
                page.range().start_byte(),
                u32::try_from(CALLER_IMPORT.len())?
            );
            assert_eq!(
                page.range().end_byte(),
                u32::try_from(CALLER_IMPORT.len() + CALLER_BODY.len())?
            );
            assert!(page.range().contains(bound.required().range()));
            observed = Some(page.evidence());
            acquired = Some(page);
        }
        let previous = run.last_event_sequence();
        let recorded = read.record(&mut run, RunEventId::from_bytes([id; 32]), at(time + 1)?)?;
        assert_eq!(
            recorded.original_evidence(),
            observed.as_ref().map(a3_domain::AgentToolEvidence::id)
        );
        AppendAgentRead::new(store.as_ref())
            .execute(project, previous, &run, &recorded)
            .await?;
    }
    let current = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
    let next = PreparedEditPhase::update(current, &target)?;
    assert_ne!(next.job_digest(), previous_job);
    let acquired = acquired.ok_or("bound caller original missing")?;
    let durable = store
        .load_source_read_state(project, current.anchor())
        .await?;
    assert!(durable.progress().receipts().is_empty());
    let stamp = durable
        .acquired()
        .ok_or("durable original caller missing")?;
    let compile = |page| {
        build_edit_phase_request(
            &goal,
            &ledger,
            &next,
            current,
            profile.clone(),
            None,
            EditPhasePrerequisites::none().with_required_source(
                EditEvidenceNeed::Caller,
                &bound,
                Some((page, stamp)),
            ),
        )
    };
    assert!(matches!(
        compile(&target),
        Err(EditPhaseRequestFailure::Stale)
    ));
    let arbitrary = BoundEditSource::for_file_span(project, &published, &lens, bound.required())?;
    let foreign_target = PreparedEditPhase::update(current, &acquired)?;
    for (phase, source, need) in [
        (&next, &arbitrary, EditEvidenceNeed::Caller),
        (&foreign_target, &bound, EditEvidenceNeed::Caller),
        (&next, &bound, EditEvidenceNeed::Test),
    ] {
        let result = build_edit_phase_request(
            &goal,
            &ledger,
            phase,
            current,
            profile.clone(),
            None,
            EditPhasePrerequisites::none().with_required_source(
                need,
                source,
                Some((&acquired, stamp)),
            ),
        );
        match need {
            EditEvidenceNeed::Caller => {
                assert!(matches!(
                    result,
                    Err(EditPhaseRequestFailure::InvalidCallerTarget)
                ));
            }
            _ => {
                assert!(matches!(
                    result,
                    Err(EditPhaseRequestFailure::UnsupportedSourceNeed(
                        EditEvidenceNeed::Test
                    ))
                ));
            }
        }
    }
    let compiled = compile(&acquired)?;
    let messages = compiled.request().messages();
    let [_, _, supplemental, target] = messages else {
        return Err("edit did not include the required caller before the target window".into());
    };
    let supplemental: Value = serde_json::from_str(supplemental.content())?;
    assert_eq!(
        supplemental["readOnly"],
        json!({"kind":"caller","path":"caller.py","text":CALLER_BODY})
    );
    assert_eq!(target.content(), next.input_json());
    assert_eq!(run.usage(), AgentRunUsage::ZERO);
    Ok((
        run.id(),
        ObservedRead {
            source: observed.ok_or("original caller read missing")?,
            sequence: run.last_event_sequence(),
        },
    ))
}

async fn reject_changed_live_caller(fixture: &Fixture, run_id: AgentRunId) -> TestResult {
    let project = &fixture.project;
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let published = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("candidate publication missing")?;
    let run = store
        .load_agent_run(project, run_id)
        .await?
        .ok_or("candidate run missing")?;
    let snapshot = published.run().snapshot_id();
    assert_eq!(run.current_snapshot_id(), snapshot);
    let callee = symbol_id(&published, b"subject.py", "target")?;
    let caller = symbol_id(&published, b"caller.py", "invoke")?;
    let lens = compile_lens(
        &store,
        project,
        RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
        callee,
        caller,
    )
    .await?;
    let bound = BoundEditSource::for_direct_caller(project, &published, &lens, callee, caller)?;
    let action = bound.read_action(project, &published)?;
    let tool = ToolRunId::from_bytes([183; 32]);
    let attempt = store
        .begin_agent_tool_attempt(project, run.id(), snapshot, tool, at(18)?)
        .await?;
    let reader =
        DeterministicAgentReadTools::new(&store, &store, &store, &WorkspaceAgentSourceReader);
    assert!(matches!(
        reader
            .execute(
                project,
                snapshot,
                tool,
                &action,
                AgentReadTimeout::DEFAULT,
                &ActiveControl
            )
            .await,
        Err(AgentReadToolFailure::Unavailable)
    ));
    let failed = store
        .finish_agent_tool_attempt(
            project,
            tool,
            attempt.attempt(),
            AgentToolAttemptStatus::Failed,
            at(19)?,
        )
        .await?;
    assert_eq!(failed.status(), AgentToolAttemptStatus::Failed);
    Ok(())
}

async fn compile_lens(
    store: &LibsqlKnowledgeStore,
    project: &ProjectIdentity,
    path: RepositoryPath,
    callee: SymbolId,
    caller: SymbolId,
) -> TestResult<a3_domain::TaskLens> {
    let seed = TaskLensSeedText::try_from_string("Read the actual caller of target".to_owned())?;
    Ok(CompileTaskLens::new(store, store, store)
        .execute(
            project,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![
                    TaskLensSeed::ExplicitPath(path),
                    TaskLensSeed::ExplicitSymbol(callee),
                    TaskLensSeed::ExplicitSymbol(caller),
                ],
            )?,
            TaskLensTokenBudget::new(4_096)?,
            &ActiveControl,
        )
        .await?)
}

fn symbol_id(
    published: &a3_domain::PublishedIndex,
    path: &[u8],
    name: &str,
) -> TestResult<SymbolId> {
    Ok(published
        .publication()
        .graph()
        .symbols()
        .iter()
        .find(|symbol| {
            symbol.revision().path().as_bytes() == path && symbol.parsed().name().as_str() == name
        })
        .ok_or("expected real indexed Python symbol missing")?
        .id())
}

#[derive(Clone, Copy)]
enum WireControl {
    Accepted,
    Unknown,
    Stale,
    MetadataOnly,
}

#[test]
fn actual_source_read_only_credits_exact_accepted_wire_across_cold_replay() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for control in [
                WireControl::Accepted,
                WireControl::Unknown,
                WireControl::Stale,
                WireControl::MetadataOnly,
            ] {
                source_replay_control(control).await?;
            }
            Ok(())
        })
    })
}

async fn source_replay_control(control: WireControl) -> TestResult {
    use a3_application::{
        AcquiredSourceRead, EditPhaseCallKind, EditPhaseObservation, EditPhaseObservationOutcome,
        EditPhaseReservationFailure, EditPhaseWireCost, SourceDeliveryFailure,
        SourceDeliveryOutcome, SourceReadFailure, SourceReadStore,
    };
    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    let caller_text = format!("{CALLER_IMPORT}{CALLER_BODY}");
    fs::write(worktree.join("subject.py"), TARGET)?;
    fs::write(worktree.join("caller.py"), caller_text.as_bytes())?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(&fixture).await?;
    let snapshot = published.run().snapshot_id();
    let new_path = RepositoryPath::try_from_bytes(b"new.py".to_vec())?;
    let (goal, step) = path_edit_goal(&new_path, b"Create new.py using the selected caller.")?;
    let (mut run, ledger, _) =
        start_fixture_step(&fixture, &store, &goal, &profile()?, snapshot, vec![step]).await?;
    let callee = symbol_id(&published, b"subject.py", "target")?;
    let caller = symbol_id(&published, b"caller.py", "invoke")?;
    let lens = compile_lens(
        &store,
        project,
        RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
        callee,
        caller,
    )
    .await?;
    let bound = BoundEditSource::for_direct_caller(project, &published, &lens, callee, caller)?;
    let anchor = EditPhaseState::new(project, &goal, &ledger, &run, &published)?.anchor();
    let tool = ToolRunId::from_bytes([188; 32]);
    let admission = store
        .begin_source_read(project, anchor, &bound, tool, at(20)?)
        .await?;
    assert!(matches!(
        store
            .begin_source_read(
                project,
                anchor,
                &bound,
                ToolRunId::from_bytes([189; 32]),
                at(20)?,
            )
            .await,
        Err(SourceReadFailure::Progress(
            SourceDeliveryFailure::AcquisitionPending
        ))
    ));
    let in_flight = store.load_source_read_state(project, anchor).await?;
    assert!(in_flight.progress().receipts().is_empty() && in_flight.acquired().is_none());
    assert_eq!(
        store
            .abandon_source_read(project, anchor, admission.key())
            .await,
        Err(SourceReadFailure::NotPending)
    );
    let action = if matches!(control, WireControl::MetadataOnly) {
        AgentReadAction::Inspect(AgentInspectAction::new(AgentInspectTarget::Symbol(caller)))
    } else {
        bound.read_action(project, &published)?
    };
    let reader = DeterministicAgentReadTools::new(
        store.as_ref(),
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
    );
    let mut read = reader
        .execute(
            project,
            snapshot,
            tool,
            &action,
            AgentReadTimeout::DEFAULT,
            &ActiveControl,
        )
        .await?;
    let page = match bound.take_original(project, &published, tool, &mut read) {
        Ok(page) => Some(page),
        Err(BoundEditSourceFailure::MissingOriginal)
            if matches!(control, WireControl::MetadataOnly) =>
        {
            None
        }
        Err(error) => return Err(error.into()),
    };
    let before = run.last_event_sequence();
    let record = read.record(&mut run, RunEventId::from_bytes([188; 32]), at(21)?)?;
    AppendAgentRead::new(store.as_ref())
        .execute(project, before, &run, &record)
        .await?;
    let state = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
    let anchor = state.anchor();
    let pending = store.load_source_read_state(project, anchor).await?;
    assert!(pending.progress().receipts().is_empty());
    assert!(pending.progress().pending().is_some());

    let wire = EditPhaseWireCost::new(ContentHash::from_bytes([195; 32]), 600, 512, 256)?;
    let mut expected_delivery = None;
    if let Some(page) = page {
        let phase = PreparedEditPhase::add(state, new_path)?;
        let acquired = pending
            .acquired()
            .ok_or("original must remain bound to its ToolAction")?;
        assert_eq!(acquired.key(), admission.key());
        assert_eq!(acquired.original().range(), page.range());
        assert!(matches!(
            store
                .reserve_edit_phase(project, &phase, EditPhaseCallKind::Initial, wire, at(22)?)
                .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        let forged = AcquiredSourceRead::from_recorded(
            ContentHash::from_bytes([196; 32]),
            acquired.scope(),
            acquired.required().clone(),
            acquired.original().clone(),
        )?;
        assert!(matches!(
            store
                .reserve_edit_phase_with_source(
                    project,
                    &phase,
                    EditPhaseCallKind::Initial,
                    wire,
                    at(22)?,
                    &forged,
                )
                .await,
            Err(EditPhaseReservationFailure::StaleAnchor)
        ));
        assert_eq!(
            store.load_edit_phase_usage(project, run.id()).await?.calls,
            0
        );
        let permit = store
            .reserve_edit_phase_with_source(
                project,
                &phase,
                EditPhaseCallKind::Initial,
                wire,
                at(22)?,
                acquired,
            )
            .await?;
        let sent = store.load_source_read_state(project, anchor).await?;
        assert!(sent.progress().receipts().is_empty() && sent.acquired().is_none());
        assert_eq!(
            store
                .abandon_source_read(project, anchor, admission.key())
                .await,
            Err(SourceReadFailure::NotPending)
        );

        // A controlled provider observation tests the storage boundary; no generation is performed.
        let outcome = match control {
            WireControl::Unknown => EditPhaseObservationOutcome::Unknown,
            _ => EditPhaseObservationOutcome::AcceptedReply,
        };
        if matches!(control, WireControl::Stale) {
            fs::write(
                worktree.join("subject.py"),
                b"def target(value):\n    return value - 1\n",
            )?;
            let (fresh_store, _, _, latest) = index_fixture(&fixture).await?;
            assert_ne!(latest.run().snapshot_id(), snapshot);
            drop(fresh_store);
        }
        let finished = store
            .finish_edit_phase(
                project,
                permit,
                EditPhaseObservation::new(outcome, Some(128), Some(32), at(23)?),
            )
            .await;
        if matches!(control, WireControl::Stale) {
            assert_eq!(finished, Err(EditPhaseReservationFailure::StaleAnchor));
        } else {
            finished?;
        }
        if matches!(control, WireControl::Accepted) {
            expected_delivery = Some((page.evidence(), page.range().len()));
        }
    } else {
        assert!(pending.acquired().is_none());
        store
            .abandon_source_read(project, anchor, admission.key())
            .await?;
    }
    let run_id = run.id();
    drop(store);
    let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let durable_run = reopened
        .load_agent_run(project, run_id)
        .await?
        .ok_or("run lost")?;
    assert_eq!(durable_run, run);
    let durable_ledger = reopened
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("ledger lost")?;
    let anchor =
        EditPhaseState::new(project, &goal, &durable_ledger, &durable_run, &published)?.anchor();
    let durable = reopened.load_source_read_state(project, anchor).await;
    match control {
        WireControl::Accepted => {
            let durable = durable?;
            let [receipt] = durable.progress().receipts() else {
                return Err("expected exactly one durable accepted wire".into());
            };
            let (original, len) = expected_delivery.ok_or("controlled wire missing")?;
            let delivery = receipt.delivery().ok_or("wire credit lost")?;
            assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NewBytes(len));
            assert_eq!(delivery.wire_digest(), wire.digest());
            assert_eq!(delivery.source().snapshot_id(), snapshot);
            assert_eq!(delivery.source().revision(), original.location().revision());
            assert_eq!(Some(delivery.source().range()), original.location().range());
            assert!(durable.progress().pending().is_none() && durable.acquired().is_none());
        }
        WireControl::Unknown => {
            let durable = durable?;
            assert!(
                durable.progress().receipts().is_empty() && durable.progress().pending().is_some()
            );
            assert!(durable.acquired().is_none());
            assert_eq!(
                reopened
                    .abandon_source_read(project, anchor, admission.key())
                    .await,
                Err(SourceReadFailure::NotPending)
            );
            assert_eq!(
                reopened
                    .load_edit_phase_usage(project, run_id)
                    .await?
                    .unknown_calls,
                1
            );
        }
        WireControl::Stale => {
            assert!(matches!(
                durable,
                Err(SourceReadFailure::Reservation(
                    EditPhaseReservationFailure::StaleAnchor
                ))
            ));
        }
        WireControl::MetadataOnly => {
            let durable = durable?;
            let [receipt] = durable.progress().receipts() else {
                return Err("metadata charge lost".into());
            };
            assert_eq!(receipt.outcome(), SourceDeliveryOutcome::NotDelivered);
            assert!(receipt.delivery().is_none() && durable.acquired().is_none());
            assert_eq!(
                reopened.load_edit_phase_usage(project, run_id).await?.calls,
                0
            );
        }
    }
    assert_eq!(
        fs::read(worktree.join("caller.py"))?,
        caller_text.as_bytes()
    );
    assert!(!worktree.join("new.py").exists());
    assert!(
        reopened
            .load_agent_mutation_attempts(project, run_id)
            .await?
            .is_empty()
    );
    drop(reopened);
    fixture.cleanup()
}

#[test]
fn bounded_caller_lookup_is_charged_once_after_no_match_or_interrupted_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            for complete_lookup in [true, false] {
                caller_gap_after_reopen(complete_lookup).await?;
            }
            Ok(())
        })
    })
}

async fn caller_gap_after_reopen(complete_lookup: bool) -> TestResult {
    use a3_application::{
        CallerRelocalization, CallerRelocalizationOutcome, SourceAcquisition,
        SourceDeliveryFailure, SourceDeliveryOutcome, SourceReadFailure, SourceReadStore,
    };

    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    fs::write(worktree.join("subject.py"), TARGET)?;
    fs::write(
        worktree.join("other.py"),
        b"def other(value):\n    return value\n\ndef invoke(value):\n    return other(value)\n",
    )?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    let before = inventory(&worktree)?;
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(&fixture).await?;
    let snapshot = published.run().snapshot_id();
    let callee = symbol_id(&published, b"subject.py", "target")?;
    let unrelated = symbol_id(&published, b"other.py", "other")?;
    let caller = symbol_id(&published, b"other.py", "invoke")?;
    assert!(published.publication().graph().edges().iter().any(|edge| {
        edge.snapshot_id() == snapshot
            && edge.kind() == SyntaxRelationKind::Calls
            && edge.source() == &GraphEndpoint::Symbol(caller)
            && edge.target() == &GraphEndpoint::Symbol(unrelated)
    }));
    assert!(!published.publication().graph().edges().iter().any(|edge| {
        edge.kind() == SyntaxRelationKind::Calls && edge.target() == &GraphEndpoint::Symbol(callee)
    }));

    let path = RepositoryPath::try_from_bytes(b"subject.py".to_vec())?;
    let (goal, step) = path_edit_goal(
        &path,
        b"Inspect the actual caller before changing subject.py.",
    )?;
    let (mut run, ledger, _) =
        start_fixture_step(&fixture, &store, &goal, &profile()?, snapshot, vec![step]).await?;
    let lens = compile_lens(&store, project, path.clone(), callee, caller).await?;
    let revision = published
        .publication()
        .graph()
        .files()
        .iter()
        .find(|item| item.path() == &path)
        .ok_or("active file missing")?;
    let target = WorkspaceAgentSourceReader
        .read_page(
            project,
            revision,
            &AgentFileInspection::new(
                path,
                AgentFileStartLine::new(1)?,
                AgentFileLineCount::new(2)?,
            ),
            &ActiveControl,
        )
        .await?;
    let current = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
    let phase = PreparedEditPhase::update(current, &target)?;
    assert!(matches!(
        CallerRelocalization::new(current, &phase, &lens, caller),
        Err(AgentReadToolFailure::InvalidResult)
    ));
    let job = CallerRelocalization::new(current, &phase, &lens, callee)?;
    let tool = ToolRunId::from_bytes([190; 32]);
    let admission = store
        .begin_caller_relocalization(project, &job, tool, at(20)?)
        .await?;
    let before_lookup = run.last_event_sequence();
    if complete_lookup {
        let tools = DeterministicAgentReadTools::new(
            store.as_ref(),
            store.as_ref(),
            store.as_ref(),
            &WorkspaceAgentSourceReader,
        );
        let result = job
            .observe(
                project,
                admission,
                &tools,
                AgentReadTimeout::DEFAULT,
                &ActiveControl,
            )
            .await?;
        assert_eq!(
            result.receipt().outcome(),
            CallerRelocalizationOutcome::NoMatch
        );
        let read = result.record(&mut run, RunEventId::from_bytes([190; 32]), at(21)?)?;
        assert!(read.original_evidence().is_none() && read.evidence().evidence().is_empty());
        AppendAgentRead::new(store.as_ref())
            .execute(project, before_lookup, &run, &read)
            .await?;
    }
    let run_id = run.id();
    drop(store);

    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let run = store
        .load_agent_run(project, run_id)
        .await?
        .ok_or("Run lost")?;
    let ledger = store
        .load_task_ledger(project, goal.task_id())
        .await?
        .ok_or("Ledger lost")?;
    let published = store
        .latest_published_index(project, &ActiveControl)
        .await?
        .ok_or("publication lost")?;
    let current = EditPhaseState::new(project, &goal, &ledger, &run, &published)?;
    if !complete_lookup {
        let pending = store
            .load_source_read_state(project, current.anchor())
            .await?;
        assert!(pending.progress().pending().is_some() && pending.acquired().is_none());
        assert!(pending.caller_relocalizations().is_empty());
        assert_eq!(
            store
                .interrupt_agent_tool_attempts(project, run_id, at(22)?)
                .await?,
            1
        );
    }
    let durable = store
        .load_source_read_state(project, current.anchor())
        .await?;
    let [receipt] = durable.caller_relocalizations() else {
        return Err("caller gap observation lost".into());
    };
    assert_eq!(receipt.callee(), callee);
    assert_eq!(
        receipt.outcome(),
        if complete_lookup {
            CallerRelocalizationOutcome::NoMatch
        } else {
            CallerRelocalizationOutcome::Interrupted
        }
    );
    let [settled] = durable.progress().receipts() else {
        return Err("caller acquisition charge lost".into());
    };
    assert_eq!(settled.outcome(), SourceDeliveryOutcome::NotDelivered);
    assert_eq!(
        settled.admission().acquisition(),
        &SourceAcquisition::Relocalize {
            need: a3_application::EditEvidenceNeed::Caller,
            target: GraphEndpoint::Symbol(callee)
        }
    );
    assert!(settled.delivery().is_none() && durable.acquired().is_none());
    let phase = PreparedEditPhase::update(current, &target)?;
    let retry = CallerRelocalization::new(current, &phase, &lens, callee)?;
    assert!(matches!(
        store
            .begin_caller_relocalization(project, &retry, ToolRunId::from_bytes([191; 32]), at(23)?)
            .await,
        Err(SourceReadFailure::Progress(
            SourceDeliveryFailure::GapAlreadyRelocalized
        ))
    ));
    let events = store
        .load_run_events(
            project,
            run_id,
            Some(before_lookup),
            RunEventPageLimit::new(8)?,
        )
        .await?;
    if complete_lookup {
        let [event] = events.events() else {
            return Err("expected exactly one graph ToolAction".into());
        };
        assert_eq!(event.kind(), RunEventKind::ToolAction);
        assert_eq!(event.subject(), Some(RunEventSubject::Tool(tool)));
        assert!(event.turn_charge().is_none());
    } else {
        assert!(events.events().is_empty());
    }
    let database = fixture.candidate_database().await?;
    let connection = database.connection_for_operation().await?;
    assert_eq!(
        count(&connection, "SELECT count(*) FROM tool_run_attempts").await?,
        1
    );
    assert_eq!(store.load_edit_phase_usage(project, run_id).await?.calls, 0);
    assert!(
        store
            .load_agent_mutation_attempts(project, run_id)
            .await?
            .is_empty()
    );
    assert_eq!(inventory(&worktree)?, before);
    drop(connection);
    drop(database);
    drop(store);
    fixture.cleanup()
}

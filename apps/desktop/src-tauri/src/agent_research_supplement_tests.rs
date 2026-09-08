//! Actual packet admission of cached supplemental originals, without model-side memory.
use super::*;
use a3_application::ResearchOutputPhase;

const QUERY: &str = "Verfolge add_task in taskflow/manager.py bis zum tatsächlichen Audit-Schreibvorgang in taskflow/plugins.py. Nenne alle Methoden der Kette, Reihenfolge, Logdatei, Pfadauflösung relativ zum Arbeitsverzeichnis und Schreibmodus.";
const FILES: [(&str, &str); 5] = [
    (
        "main.py",
        include_str!("../../../../fixtures/research-eval-v1/main.py"),
    ),
    (
        "taskflow/manager.py",
        include_str!("../../../../fixtures/research-eval-v1/taskflow/manager.py"),
    ),
    (
        "taskflow/storage.py",
        include_str!("../../../../fixtures/research-eval-v1/taskflow/storage.py"),
    ),
    (
        "taskflow/plugins.py",
        include_str!("../../../../fixtures/research-eval-v1/taskflow/plugins.py"),
    ),
    (
        "taskflow/api.py",
        include_str!("../../../../fixtures/research-eval-v1/taskflow/api.py"),
    ),
];

struct SupplementalModel {
    delivered: std::sync::Mutex<Vec<[bool; 3]>>,
    budget: usize,
    review: ReviewBehavior,
    review_calls: AtomicUsize,
    root: std::path::PathBuf,
    canceller: std::sync::Mutex<Option<JobSubmitter>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewBehavior {
    Disabled,
    Valid,
    Repair,
    QuoteRepair,
    Invalid,
    Transient,
    RetryExhausted,
    Edit,
    Cancel,
}

impl ResearchModel for SupplementalModel {
    fn analysis_method(&self) -> a3_application::ResearchAnalysisMethod {
        if self.review == ReviewBehavior::Disabled {
            a3_application::ResearchAnalysisMethod::Joint
        } else {
            a3_application::ResearchAnalysisMethod::SourceLocal
        }
    }
    async fn complete_source_review(
        &self,
        transcript: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.review_calls.fetch_add(1, Ordering::SeqCst);
        let packet = &transcript[0].1;
        assert!(
            packet.contains(QUERY),
            "unchanged user objective, not a substitute task"
        );
        assert!(packet.len() <= self.budget);
        let files = FILES
            .iter()
            .filter(|(_, body)| packet.contains(*body))
            .collect::<Vec<_>>();
        assert_eq!(
            files.len(),
            1,
            "exactly one complete original, no cross-source memory"
        );
        assert!(!packet.contains("UNVERIFIED SOURCE INTERPRETATIONS"));
        assert!(packet.contains("CORE CURRENT STEP Q"));
        assert!(packet.contains("SOURCE-LOCAL SUBTASK:"));
        assert!(!packet.contains("Required original file coverage"));
        assert!(!packet.contains("CORE RESEARCH CONTRACT"));
        let operations = packet
            .split_once("SOURCE OPERATIONS")
            .map(|(_, tail)| tail)
            .ok_or(AgentConversationFailure::InvalidInput)?;
        assert!(operations.contains("unknown effects are not absent"));
        if files[0].0 == "taskflow/plugins.py" {
            assert!(
                operations.contains("output.write"),
                "the actual writer must survive the optional inventory budget: {operations}"
            );
        }
        if files[0].0 == "taskflow/storage.py" {
            assert!(operations.contains("save_tasks") && operations.contains("Return"));
            assert!(operations.contains("Dynamic"));
        }
        assert!(
            !transcript
                .iter()
                .any(|(_, s)| s.contains("rejected sentinel"))
        );
        if self.review == ReviewBehavior::Invalid
            || (self.review == ReviewBehavior::Repair && call == 0)
        {
            return Ok("{rejected sentinel".to_owned());
        }
        if self.review == ReviewBehavior::RetryExhausted
            || (self.review == ReviewBehavior::Transient && call == 0)
        {
            return Err(AgentConversationFailure::ModelTimedOut);
        }
        if self.review == ReviewBehavior::Repair && call == 1 {
            assert_eq!(transcript.len(), 2);
            assert!(transcript[1].1.starts_with("REPAIR source-review/"));
        }
        if self.review == ReviewBehavior::QuoteRepair {
            if call == 0 {
                assert!(files[0].1.len() > 512);
                return Ok(serde_json::json!({"schema_version":2,"interpretation":"Short valid interpretation.","quotes":[files[0].1]}).to_string());
            }
            if call == 1 {
                assert_eq!(transcript.len(), 2);
                assert!(transcript[1].1.contains("quote 1"));
                assert!(transcript[1].1.contains(&files[0].1.len().to_string()));
                assert!(
                    transcript[1]
                        .1
                        .contains("Shortening interpretation alone does not fix")
                );
                assert!(transcript[1].1.len() <= 768);
            }
        }
        if self.review == ReviewBehavior::Edit {
            std::fs::write(
                self.root.join(files[0].0),
                "# source edited during review\n",
            )
            .map_err(|_| AgentConversationFailure::Unavailable)?;
        }
        if self.review == ReviewBehavior::Cancel {
            self.canceller
                .lock()
                .map_err(|_| AgentConversationFailure::Unavailable)?
                .as_ref()
                .ok_or(AgentConversationFailure::Unavailable)?
                .cancel(JobId::new(1))
                .map_err(|_| AgentConversationFailure::Unavailable)?;
        }
        let quote = files[0]
            .1
            .lines()
            .find(|line| !line.trim().is_empty())
            .ok_or(AgentConversationFailure::InvalidInput)?;
        Ok(serde_json::json!({"schema_version":2,"interpretation":"Fixture-local hint; recheck the original before answering.","quotes":[quote]}).to_string())
    }
    fn requires_work_contract(&self) -> bool {
        true
    }

    async fn research_evidence_budget(
        &self,
        _: AgentSessionMode,
        _: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        Ok(self.budget)
    }

    async fn complete_research_decision(
        &self,
        _: AgentSessionMode,
        _: bool,
        phase: ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let packet = &transcript
            .iter()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1;
        assert!(packet.len() <= self.budget);
        assert!(!packet.contains("unpublished sentinel"));
        let response = match phase {
            ResearchOutputPhase::Initialize => serde_json::json!({"kind":"questions","questions":[{
                "kind":"repository","outcome":"Explain the audit call chain from current originals.","priority":"required","dependencies":[]
            }]}),
            ResearchOutputPhase::Analyze(id) | ResearchOutputPhase::SummarizeOriginals(id) => {
                assert_eq!(
                    packet.contains("UNVERIFIED SOURCE INTERPRETATIONS"),
                    self.review != ReviewBehavior::Disabled
                );
                self.delivered
                    .lock()
                    .map_err(|_| AgentConversationFailure::Unavailable)?
                    .push([1, 3, 2].map(|file| packet.contains(FILES[file].1)));
                let evidence = ["self.storage.save_tasks(self.tasks)", "output.write("]
                    .iter()
                    .map(|needle| coherent_contract::quote(packet, needle))
                    .collect::<Option<Vec<_>>>()
                    .ok_or(AgentConversationFailure::InvalidInput)?;
                serde_json::json!({"kind":"interpretation","result":{"question_id":id.get(),
                    "text":"add_task calls save_tasks and trigger_task_created; on_task_created calls _log, which appends via output.write to the constructor's os.path.abspath(log_filepath). A call to save_tasks alone does not prove persistence.","evidence":evidence}})
            }
            ResearchOutputPhase::Design(id) | ResearchOutputPhase::DesignTests(id) => {
                serde_json::json!({
                    "kind":"designDecision","result":{"question_id":id.get(),"text":"Document the audit chain. Test dispatch order, callback, constructor destination and UTF-8 append separately without assuming storage effects.","evidence":[]}
                })
            }
            ResearchOutputPhase::Finalize => return Err(AgentConversationFailure::InvalidInput),
        };
        Ok(serde_json::json!({"schema_version":7,"response":response}).to_string())
    }

    async fn complete_evidence_diagrams(
        &self,
        _: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        Err(AgentConversationFailure::InvalidInput)
    }
}

#[test]
fn research_supplemental_storage_originals_share_the_actual_audit_packet()
-> Result<(), Box<dyn Error>> {
    supplement_fixture(8192, StorageRevision::Current, ReviewBehavior::Disabled)?;
    supplement_fixture(4096, StorageRevision::Current, ReviewBehavior::Disabled)
}

#[test]
fn research_supplemental_stale_callee_is_unavailable_not_original_evidence()
-> Result<(), Box<dyn Error>> {
    supplement_fixture(8192, StorageRevision::Changed, ReviewBehavior::Disabled)
}

#[test]
fn source_review_actual_researcher_keeps_all_originals_in_ask_plan_and_agent()
-> Result<(), Box<dyn Error>> {
    // 3409 is the actual current FormatFieldOnly 8k/2k profile packet allowance.
    for budget in [3409, 4096, 8192] {
        supplement_fixture(budget, StorageRevision::Current, ReviewBehavior::Valid)?;
    }
    Ok(())
}

#[test]
fn source_review_actual_researcher_repairs_once_and_charges_transient_retries()
-> Result<(), Box<dyn Error>> {
    for review in [
        ReviewBehavior::Repair,
        ReviewBehavior::Invalid,
        ReviewBehavior::Transient,
        ReviewBehavior::RetryExhausted,
    ] {
        supplement_fixture(8192, StorageRevision::Current, review)?;
    }
    Ok(())
}

#[test]
fn source_review_actual_researcher_rejects_an_edit_during_model_call() -> Result<(), Box<dyn Error>>
{
    supplement_fixture(8192, StorageRevision::Current, ReviewBehavior::Edit)?;
    supplement_fixture(8192, StorageRevision::Current, ReviewBehavior::Cancel)
}

#[test]
fn source_review_field_specific_quote_repair_preserves_all_modes_and_originals()
-> Result<(), Box<dyn Error>> {
    supplement_fixture(8192, StorageRevision::Current, ReviewBehavior::QuoteRepair)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageRevision {
    Current,
    Changed,
}

fn supplement_fixture(
    budget: usize,
    storage_revision: StorageRevision,
    review: ReviewBehavior,
) -> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, body) in FILES {
            repository.write(path, body)?;
        }
        repository.git(["add", "."])?;
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
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &FixtureControl,
            )
            .await?;
        let published = store
            .latest_published_index(&project, &FixtureControl)
            .await?
            .ok_or("index")?;
        assert_source_bound_candidates(&published)?;
        if storage_revision == StorageRevision::Changed {
            repository.write(
                FILES[2].0,
                "def create_storage():\n    return 'unpublished sentinel'\n",
            )?;
        }
        for (index, mode) in [
            (1, AgentSessionMode::Ask),
            (2, AgentSessionMode::Plan),
            (3, AgentSessionMode::Agent),
        ] {
            // The Core Plan inventory uses a smaller work projection here, so it
            // also fits the optional callee at 3409 bytes; the two Ask duties do not.
            let has_supplement = budget != 3409 || mode != AgentSessionMode::Ask;
            let id = AgentSessionId::from_bytes([index; 32]);
            let time = timestamp()?;
            let session = AgentSession::from_parts(
                id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("Supplemental originals".to_owned())?,
                mode,
                AgentSessionState::Running,
                time,
                time,
                Some(AgentSessionSequence::FIRST),
                None,
                None,
                false,
            );
            let entry = AgentSessionEntry::try_new(
                id,
                AgentSessionSequence::FIRST,
                AgentSessionEntryKind::UserMessage,
                AgentSessionText::try_from_string(QUERY.to_owned())?,
                time,
                None,
                None,
                None,
            )?;
            store
                .create_session(&project, &session, Some(&entry), None)
                .await?;
            let model = Arc::new(SupplementalModel {
                delivered: std::sync::Mutex::new(Vec::new()),
                budget,
                review,
                review_calls: AtomicUsize::new(0),
                root: repository.path().to_owned(),
                canceller: std::sync::Mutex::new(None),
            });
            let worker_model = model.clone();
            let worker_project = project.clone();
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone())
                    .with_function_flows(Some(a3_application::ExploreFunctionFlows::new(
                        store.clone(),
                    )));
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned(move |control, submitter| {
                *worker_model.canceller.lock().map_err(|_| "poisoned")? = Some(submitter);
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                send.send(runtime.block_on(researcher.research(
                    worker_model.as_ref(),
                    &worker_project,
                    id,
                    AgentSessionSequence::FIRST,
                    mode,
                    AgentResearchDepth::Standard,
                    QUERY,
                    &[(ModelMessageRole::User, QUERY.to_owned())],
                    None,
                    &control,
                )))?;
                Ok(())
            })?;
            let result = receive.recv_timeout(Duration::from_secs(1))?;
            if review == ReviewBehavior::Cancel {
                assert!(matches!(
                    result,
                    Err(AgentSessionManagerFailure::Unavailable)
                ));
                assert_eq!(model.review_calls.load(Ordering::SeqCst), 1);
                assert!(model.delivered.lock().map_err(|_| "poisoned")?.is_empty());
                continue;
            }
            if review == ReviewBehavior::Edit {
                assert!(matches!(
                    result,
                    Err(AgentSessionManagerFailure::IndexChanged)
                ));
                assert_eq!(model.review_calls.load(Ordering::SeqCst), 1);
                assert!(model.delivered.lock().map_err(|_| "poisoned")?.is_empty());
                // Only this throwaway fixture is reset for the next independently pinned mode.
                for (path, body) in FILES {
                    repository.write(path, body)?;
                }
                continue;
            }
            let result = result?;
            let detail = store
                .load_detail(&project, id, AgentSessionSequence::FIRST)
                .await?
                .ok_or("trace")?;
            let sources = store
                .list_sources(&project, id, AgentSessionSequence::FIRST, None, 50)
                .await?;
            println!(
                "supplement {mode:?}: selected={:?}, delivered={:?}",
                sources
                    .sources()
                    .iter()
                    .filter_map(|source| FILES.iter().position(|(path, _)| source
                        .revision()
                        .path()
                        .as_bytes()
                        == path.as_bytes()))
                    .collect::<Vec<_>>(),
                model.delivered.lock().map_err(|_| "poisoned")?
            );
            let calls = model.review_calls.load(Ordering::SeqCst);
            assert_eq!(
                detail
                    .events()
                    .iter()
                    .filter(|e| e
                        .action()
                        .starts_with("Core prüft ein aktuelles Original einzeln"))
                    .count(),
                calls,
                "every actual provider start has a Deciding receipt"
            );
            if matches!(
                review,
                ReviewBehavior::Invalid | ReviewBehavior::RetryExhausted
            ) {
                assert!(result.awaiting_continuation);
                assert!(!detail.work_state().ok_or("work")?.ready_to_finish());
                assert_eq!(
                    calls,
                    if review == ReviewBehavior::Invalid {
                        2
                    } else {
                        3
                    }
                );
                assert!(model.delivered.lock().map_err(|_| "poisoned")?.is_empty());
                continue;
            }
            assert_eq!(
                calls,
                match review {
                    ReviewBehavior::Disabled => 0,
                    ReviewBehavior::Valid =>
                        if has_supplement {
                            3
                        } else {
                            2
                        },
                    _ => 4,
                }
            );
            assert!(!result.awaiting_continuation);
            assert!(detail.work_state().ok_or("work")?.ready_to_finish());
            assert_eq!(
                detail.work_state().ok_or("work")?.accesses().len(),
                usize::from(has_supplement),
                "one Core-owned supplemental read, no model evidence request"
            );
            if let Some(access) = detail.work_state().ok_or("work")?.accesses().first() {
                assert_eq!(
                    access.outcome,
                    Some(if storage_revision == StorageRevision::Current {
                        a3_domain::ResearchAccessOutcome::Completed
                    } else {
                        a3_domain::ResearchAccessOutcome::Unavailable
                    })
                );
            }
            let delivered = model.delivered.lock().map_err(|_| "poisoned")?;
            assert!(!delivered.is_empty());
            assert!(
                delivered.iter().all(|coverage| *coverage
                    == [
                        true,
                        true,
                        storage_revision == StorageRevision::Current && has_supplement
                    ]),
                "caller, writer and storage originals must coexist without model memory"
            );
            for (path, body) in FILES {
                if storage_revision == StorageRevision::Changed && path == FILES[2].0 {
                    assert_eq!(
                        std::fs::read_to_string(repository.path().join(path))?,
                        "def create_storage():\n    return 'unpublished sentinel'\n"
                    );
                    continue;
                }
                assert_eq!(std::fs::read_to_string(repository.path().join(path))?, body);
            }
        }
        Ok(())
    })
}

#[test]
fn research_supplemental_candidates_are_bounded_and_never_traverse_their_callees()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let mut entry = String::new();
        for i in 0..8 {
            entry.push_str(&format!("from helper{i} import run{i}\n"));
            repository.write(
                format!("helper{i}.py"),
                format!("from leaf import finish\ndef run{i}():\n    return finish()\n"),
            )?;
        }
        entry.push_str("def entry():\n");
        for i in 0..8 {
            entry.push_str(&format!("    run{i}()\n    run{i}()\n"));
        }
        repository.write("entry.py", &entry)?;
        repository.write("leaf.py", "def finish():\n    return 7\n")?;
        repository.git(["add", "."])?;
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
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &FixtureControl,
            )
            .await?;
        let published = store
            .latest_published_index(&project, &FixtureControl)
            .await?
            .ok_or("published")?;
        let graph = published.publication().graph();
        let root = graph
            .files()
            .iter()
            .find(|file| file.path().as_bytes() == b"entry.py")
            .ok_or("root")?;
        let mut state = AskResearchWorkingSet::new(8192);
        state.work_required_revisions.push(root.clone());
        // All graph callsites are covered, but only the original named root can seed reads.
        for file in graph.files() {
            state.current_delivery.push(research_context::CoveredRange {
                revision: file.clone(),
                start: a3_domain::SourcePosition::new(0, 0),
                end: a3_domain::SourcePosition::new(100, 0),
            });
        }
        let actions = research_supplement::candidates(&published, &state);
        assert_eq!(actions.len(), 4);
        assert_eq!(
            actions.iter().collect::<BTreeSet<_>>().len(),
            4,
            "repeated callsites do not duplicate reads"
        );
        assert!(actions.iter().all(|action| matches!(action, AskResearchAction::InspectPath {path, start_line: 1} if path.starts_with("helper") && path.ends_with(".py"))));
        assert_eq!(research_supplement::candidates(&published, &state), actions);
        Ok(())
    })
}

fn assert_source_bound_candidates(
    published: &a3_domain::PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    let graph = published.publication().graph();
    let edge = graph
        .edges()
        .iter()
        .find(|edge| {
            edge.kind() == a3_domain::SyntaxRelationKind::Calls
                && edge.evidence().revision().path().as_bytes() == FILES[1].0.as_bytes()
        })
        .ok_or("resolved factory call")?;
    let mut state = AskResearchWorkingSet::new(8192);
    state
        .work_required_revisions
        .push(edge.evidence().revision().clone());
    let window = research_context::CoveredRange {
        revision: edge.evidence().revision().clone(),
        start: edge.evidence().range().start_position(),
        end: edge.evidence().range().end_position(),
    };
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "index metadata alone is not a delivered callsite"
    );
    state.current_delivery.push(window.clone());
    let expected = vec![AskResearchAction::InspectPath {
        path: FILES[2].0.to_owned(),
        start_line: 1,
    }];
    assert_eq!(research_supplement::candidates(published, &state), expected);
    assert_eq!(research_supplement::candidates(published, &state), expected);
    state.current_delivery[0].end = window.start;
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "partial callsite cannot seed a read"
    );
    state.current_delivery[0] = window.clone();
    state.current_delivery[0].revision = a3_domain::FileRevision::new(
        window.revision.path().clone(),
        a3_domain::ContentHash::from_bytes([99; 32]),
    );
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "stale callsite cannot seed a read"
    );
    state.current_delivery[0] = window;
    state.work_required_revisions.clear();
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "supplements cannot become new traversal roots"
    );
    state
        .work_required_revisions
        .push(edge.evidence().revision().clone());
    let storage = graph
        .files()
        .iter()
        .find(|file| file.path().as_bytes() == FILES[2].0.as_bytes())
        .ok_or("storage revision")?;
    state.complete_files.push(storage.clone());
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "do not repeat a complete read"
    );
    state.complete_files.clear();
    let unresolved = graph.unresolved().iter().find(|edge| {
        matches!(edge.target(), a3_domain::UnresolvedGraphTarget::Reference(name) if name.as_str() == "self.storage.save_tasks")
    }).ok_or("dynamic storage call")?;
    state.current_delivery[0].start = unresolved.evidence().range().start_position();
    state.current_delivery[0].end = unresolved.evidence().range().end_position();
    assert!(
        research_supplement::candidates(published, &state).is_empty(),
        "never promote a dynamic callee by name"
    );
    Ok(())
}

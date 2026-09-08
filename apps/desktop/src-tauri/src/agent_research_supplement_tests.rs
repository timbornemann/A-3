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
}

impl ResearchModel for SupplementalModel {
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
    supplement_fixture(8192, StorageRevision::Current)?;
    supplement_fixture(4096, StorageRevision::Current)
}

#[test]
fn research_supplemental_stale_callee_is_unavailable_not_original_evidence()
-> Result<(), Box<dyn Error>> {
    supplement_fixture(8192, StorageRevision::Changed)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageRevision {
    Current,
    Changed,
}

fn supplement_fixture(
    budget: usize,
    storage_revision: StorageRevision,
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
            });
            let worker_model = model.clone();
            let worker_project = project.clone();
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned(move |control, _| {
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
            let result = receive.recv_timeout(Duration::from_secs(1))??;
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
            assert!(!result.awaiting_continuation);
            assert!(detail.work_state().ok_or("work")?.ready_to_finish());
            assert_eq!(
                detail.work_state().ok_or("work")?.accesses().len(),
                1,
                "one Core-owned supplemental read, no model evidence request"
            );
            let access = &detail.work_state().ok_or("work")?.accesses()[0];
            assert_eq!(
                access.outcome,
                Some(if storage_revision == StorageRevision::Current {
                    a3_domain::ResearchAccessOutcome::Completed
                } else {
                    a3_domain::ResearchAccessOutcome::Unavailable
                })
            );
            let delivered = model.delivered.lock().map_err(|_| "poisoned")?;
            assert!(!delivered.is_empty());
            assert!(
                delivered.iter().all(|coverage| *coverage
                    == [true, true, storage_revision == StorageRevision::Current]),
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

//! Real original reads and durable restart: negative results are not unavailable reads.
use super::*;
use a3_domain::{
    ResearchAccessOutcome, ResearchQuestionDraft, ResearchQuestionKind, ResearchQuestionPriority,
    ResearchWorkState,
};

#[test]
fn research_access_restart_keeps_negative_receipts_but_rehydrates_original_pages()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("taskflow/config.py", "storage = 'sqlite'\n")?;
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
        let published = Arc::new(
            store
                .latest_published_index(&project, &FixtureControl)
                .await?
                .ok_or("index")?,
        );
        let id = AgentSessionId::from_bytes([92; 32]);
        let time = timestamp()?;
        let session = AgentSession::from_parts(
            id,
            AgentSessionRevision::new(1)?,
            AgentSessionTitle::try_from_string("Access receipts".to_owned())?,
            AgentSessionMode::Ask,
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
            AgentSessionText::try_from_string("storage".to_owned())?,
            time,
            None,
            None,
            None,
        )?;
        store
            .create_session(&project, &session, Some(&entry), None)
            .await?;
        let turn = AskResearchTurn::new_for_mode(
            id,
            AgentSessionSequence::FIRST,
            published.run().id(),
            published.run().snapshot_id(),
            time,
            AgentSessionMode::Ask,
            AgentResearchDepth::Standard,
        );
        let mut state = AskResearchWorkingSet::new(4096);
        state.work = Some(ResearchWorkState::new(
            "storage".to_owned(),
            vec![ResearchQuestionDraft {
                request_fragment: "storage".to_owned(),
                outcome: "Explain storage".to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Repository,
                dependencies: vec![],
            }],
        )?);
        store
            .begin_turn(
                &project,
                &turn,
                &research_event(
                    id,
                    AgentSessionSequence::FIRST,
                    1,
                    AskResearchPhase::Reading,
                    AskResearchState::Running,
                    "Access fixture",
                    None,
                    AskResearchCompleteness::NotApplicable,
                )?
                .with_work_state(state.work.clone().ok_or("work")?),
            )
            .await?;
        let researcher =
            AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
        let worker_store = store.clone();
        let worker_project = project.clone();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        recovery_contract::owned_with_timeout(Duration::from_secs(20), move |control, _| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let actions = vec![
                    AskResearchAction::SearchSourceText(vec![
                        "missing_fixture_marker_2387".to_owned(),
                    ]),
                    AskResearchAction::InspectPath {
                        path: "missing.py".to_owned(),
                        start_line: 1,
                    },
                    AskResearchAction::InspectPath {
                        path: "taskflow/config.py".to_owned(),
                        start_line: 1,
                    },
                ];
                researcher
                    .execute_actions(
                        &worker_project,
                        &published,
                        &turn,
                        &mut state,
                        actions.clone(),
                        &control,
                    )
                    .await?;
                let saved = worker_store
                    .load_detail(&worker_project, id, AgentSessionSequence::FIRST)
                    .await?
                    .ok_or("trace")?;
                let work = saved.work_state().ok_or("work")?;
                assert_eq!(work.accesses().len(), 3);
                assert_eq!(
                    work.accesses()[0].outcome,
                    Some(ResearchAccessOutcome::NoMatch)
                );
                assert_eq!(
                    work.accesses()[1].outcome,
                    Some(ResearchAccessOutcome::Unresolved)
                );
                assert_eq!(
                    work.accesses()[2].outcome,
                    Some(ResearchAccessOutcome::Completed)
                );
                assert!(work.accesses().iter().all(|a| a.starts == 1));
                assert!(!work.ready_to_finish());
                let mut reopened = AskResearchWorkingSet::new(4096);
                reopened.restore_work(work, &[])?;
                // Continuations have a new trace/source namespace; only the durable access
                // identities survive. Do not manufacture a second S1 inside the old turn.
                let reopened_id = AgentSessionId::from_bytes([93; 32]);
                let reopened_session = AgentSession::from_parts(
                    reopened_id,
                    AgentSessionRevision::new(1)?,
                    AgentSessionTitle::try_from_string("Restored receipts".to_owned())?,
                    AgentSessionMode::Ask,
                    AgentSessionState::Running,
                    time,
                    time,
                    Some(AgentSessionSequence::FIRST),
                    None,
                    None,
                    false,
                );
                let reopened_entry = AgentSessionEntry::try_new(
                    reopened_id,
                    AgentSessionSequence::FIRST,
                    AgentSessionEntryKind::UserMessage,
                    AgentSessionText::try_from_string("storage".to_owned())?,
                    time,
                    None,
                    None,
                    None,
                )?;
                worker_store
                    .create_session(
                        &worker_project,
                        &reopened_session,
                        Some(&reopened_entry),
                        None,
                    )
                    .await?;
                let reopened_turn = AskResearchTurn::new_for_mode(
                    reopened_id,
                    AgentSessionSequence::FIRST,
                    published.run().id(),
                    published.run().snapshot_id(),
                    time,
                    AgentSessionMode::Ask,
                    AgentResearchDepth::Standard,
                );
                worker_store
                    .begin_turn(
                        &worker_project,
                        &reopened_turn,
                        &research_event(
                            reopened_id,
                            AgentSessionSequence::FIRST,
                            1,
                            AskResearchPhase::Reading,
                            AskResearchState::Running,
                            "Restored access fixture",
                            None,
                            AskResearchCompleteness::NotApplicable,
                        )?
                        .with_work_state(reopened.work.clone().ok_or("work")?),
                    )
                    .await?;
                assert_eq!(
                    reopened.novel_work_accesses(&published, actions.clone()),
                    vec![actions[2].clone()]
                );
                researcher
                    .execute_actions(
                        &worker_project,
                        &published,
                        &reopened_turn,
                        &mut reopened,
                        actions,
                        &control,
                    )
                    .await?;
                assert_eq!(reopened.sources.len(), 1);
                let receipts = reopened.work.as_ref().ok_or("work")?.accesses();
                assert_eq!(
                    receipts.iter().map(|a| a.starts).collect::<Vec<_>>(),
                    vec![1, 1, 2]
                );
                assert_eq!(
                    research_access::identity(
                        &published,
                        &reopened,
                        &AskResearchAction::InspectSource(1)
                    ),
                    research_access::identity(
                        &published,
                        &reopened,
                        &AskResearchAction::InspectPath {
                            path: ".\\config.py".to_owned(),
                            start_line: 1
                        }
                    )
                );
                assert_eq!(
                    research_access::identity(
                        &published,
                        &reopened,
                        &AskResearchAction::SearchSourceText(vec!["x".to_owned(), "y".to_owned()])
                    ),
                    research_access::identity(
                        &published,
                        &reopened,
                        &AskResearchAction::SearchSourceText(vec![
                            "y".to_owned(),
                            "x".to_owned(),
                            "x".to_owned()
                        ])
                    )
                );
                let saved = worker_store
                    .load_detail(&worker_project, reopened_id, AgentSessionSequence::FIRST)
                    .await?
                    .ok_or("trace")?;
                assert_eq!(saved.work_state(), reopened.work.as_ref());
                let _packet = reopened.model_evidence("storage", &[]);
                let packet = reopened.work_packet_key();
                reopened
                    .work
                    .as_mut()
                    .ok_or("work")?
                    .begin_analysis(a3_domain::ResearchQuestionId::FIRST, packet)?;
                assert!(reopened.close_investigated_boundary(research_access::scope(&published))?);
                researcher
                    .append_work_checkpoint(&worker_project, &reopened_turn, &mut reopened)
                    .await?;
                let limited = worker_store
                    .load_detail(&worker_project, reopened_id, AgentSessionSequence::FIRST)
                    .await?
                    .ok_or("limited trace")?;
                let work = limited.work_state().ok_or("limited work")?;
                assert!(work.ready_to_finish());
                assert_eq!(
                    work.questions()[0].status(),
                    a3_domain::ResearchQuestionStatus::Limited
                );
                assert!(
                    !work.questions()[0]
                        .result()
                        .ok_or("result")?
                        .sources()
                        .is_empty()
                );
                let mut unmapped = AskResearchWorkingSet::new(4096);
                unmapped.restore_work(work, &[])?;
                let question = &unmapped.work.as_ref().ok_or("unmapped work")?.questions()[0];
                assert_eq!(question.status(), a3_domain::ResearchQuestionStatus::Stale);
                assert!(question.exclusions().is_empty());
                missing_target_research_completes(
                    &researcher,
                    &worker_store,
                    &worker_project,
                    &control,
                )
                .await?;
                send.send(())?;
                Ok::<_, Box<dyn Error>>(())
            })
        })?;
        receive.recv_timeout(Duration::from_secs(1))?;
        assert_eq!(
            std::fs::read_to_string(repository.path().join("taskflow/config.py"))?,
            "storage = 'sqlite'\n"
        );
        Ok(())
    })
}

struct MissingTargetModel {
    calls: AtomicUsize,
}
impl ResearchModel for MissingTargetModel {
    fn requires_work_contract(&self) -> bool {
        true
    }
    async fn research_evidence_budget(
        &self,
        _: AgentSessionMode,
        _: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        Ok(4096)
    }
    async fn complete_research_decision(
        &self,
        _: AgentSessionMode,
        _: bool,
        phase: a3_application::ResearchOutputPhase,
        _: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        assert!(
            self.calls.fetch_add(1, Ordering::SeqCst) < 2,
            "negative boundary spent a repeated model analysis"
        );
        let questions = if phase == a3_application::ResearchOutputPhase::Initialize {
            serde_json::json!([{"outcome":"Implementierung in missing_plugin.py erklären", "priority":"required", "kind":"repository", "dependencies":[]}])
        } else {
            serde_json::json!([])
        };
        Ok(serde_json::json!({"schema_version":5,"decision":{"kind":"progress","note":{"goal":"Implementierung lokalisieren","finding_kind":"hypothesis","finding":"Noch nicht lokalisiert","finding_source_refs":[],"gap":"missing_plugin.py","next_step":"Pfad und Referenzen prüfen"}},"work":{"questions":questions,"results":[]}}).to_string())
    }
    async fn complete_evidence_diagrams(
        &self,
        _: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        Err(AgentConversationFailure::InvalidInput)
    }
}

async fn missing_target_research_completes(
    researcher: &AgentAskResearcher,
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
    control: &JobContext,
) -> Result<(), Box<dyn Error>> {
    let id = AgentSessionId::from_bytes([94; 32]);
    let time = timestamp()?;
    let query = "Erkläre die Implementierung in missing_plugin.py.";
    let session = AgentSession::from_parts(
        id,
        AgentSessionRevision::new(1)?,
        AgentSessionTitle::try_from_string("Missing target".to_owned())?,
        AgentSessionMode::Ask,
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
        AgentSessionText::try_from_string(query.to_owned())?,
        time,
        None,
        None,
        None,
    )?;
    store
        .create_session(project, &session, Some(&entry), None)
        .await?;
    let model = MissingTargetModel {
        calls: AtomicUsize::new(0),
    };
    let result = researcher
        .research(
            &model,
            project,
            id,
            AgentSessionSequence::FIRST,
            AgentSessionMode::Ask,
            AgentResearchDepth::Standard,
            query,
            &[],
            None,
            control,
        )
        .await?;
    assert!(!result.awaiting_continuation, "{}", result.markdown);
    assert_eq!(
        result.terminal_event.completeness(),
        AskResearchCompleteness::Limited
    );
    assert!(result.markdown.contains("Begrenzte Erkenntnis"));
    assert!(
        result
            .markdown
            .contains("weder die allgemeine Nichtexistenz")
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    let detail = store
        .load_detail(project, id, AgentSessionSequence::FIRST)
        .await?
        .ok_or("missing-target trace")?;
    let work = detail.work_state().ok_or("missing-target work")?;
    assert!(work.ready_to_finish());
    assert_eq!(
        work.questions()[0].status(),
        a3_domain::ResearchQuestionStatus::Limited
    );
    assert!(work.accesses().iter().all(|a| a.starts == 1));
    assert!(
        work.accesses()
            .iter()
            .any(|a| a.outcome == Some(ResearchAccessOutcome::NoMatch))
    );
    assert!(
        work.accesses()
            .iter()
            .any(|a| a.outcome == Some(ResearchAccessOutcome::Unresolved))
    );
    Ok(())
}

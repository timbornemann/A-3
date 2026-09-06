//! Plan convergence with real index/readers/storage and a model that has NO evidence memory.
use super::*;

const PLAN_QUERY: &str = "Erstelle einen CLI-Befehl python main.py import-csv <filepath.csv>, der Aufgaben validiert und über den TaskFlowManager speichert. Erhalte alle bestehenden Aufgaben.";
const INTERFACES: [(&str, &str); 4] = [
    ("main.py", "cli_main()"),
    ("taskflow/cli.py", "def cli_main():"),
    ("taskflow/manager.py", "def add_task(title, description):"),
    ("taskflow/models.py", "class Task:"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Plan,
    RepairMissingCitation,
    Incomplete,
    Question,
    ContextLimit,
}
struct PlanModel {
    budget: usize,
    calls: AtomicUsize,
    behavior: Behavior,
}
impl ResearchModel for PlanModel {
    async fn research_evidence_budget(
        &self,
        _: AgentSessionMode,
        _: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        Ok(self.budget)
    }
    async fn complete_research_decision(
        &self,
        mode: AgentSessionMode,
        search: bool,
        _: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        assert_eq!(mode, AgentSessionMode::Plan);
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call < 12,
            "must converge or report actual stagnation before wasting the profile"
        );
        let packet = transcript
            .iter()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1
            .as_str();
        assert!(packet.contains(PLAN_QUERY));
        assert!(packet.len() <= self.budget);
        let note = serde_json::json!({"goal":"CSV-Import planen","finding_kind":"hypothesis","finding":"Vorhandene Schnittstellen prüfen, CSV-Vertrag neu vorschlagen","finding_source_refs":[],"gap":"Schnittstellen gemeinsam prüfen","next_step":"Aktuelle APIs vergleichen"});
        if self.behavior == Behavior::Question {
            return Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"sufficient","markdown":"QUESTION: Welche Zeitzone gilt für mehrdeutige lokale Fälligkeiten?","source_refs":[],"note":note}}).to_string());
        }
        if self.behavior == Behavior::Incomplete {
            return Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"incomplete","markdown":"Eine unbelegte Frage bleibt offen.","source_refs":[],"note":note}}).to_string());
        }
        // No `seen` map or scripted evidence from earlier calls: ALL interfaces must actually
        // coexist in the current request. Repeating the whole batch exposes focus overwrites.
        if INTERFACES.iter().any(|(_, api)| !packet.contains(api)) {
            assert!(search, "interfaces must fit before FinalOnly");
            let actions = INTERFACES.iter().map(|(path, _)| serde_json::json!({"kind":"inspectPath","path":path,"start_line":61})).collect::<Vec<_>>();
            return Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"research","evidence_status":"incomplete","actions":actions,"note":note}}).to_string());
        }
        let mut refs = INTERFACES
            .iter()
            .map(|(path, _)| source_ref(packet, path))
            .collect::<Result<Vec<_>, _>>()?;
        if self.behavior == Behavior::RepairMissingCitation {
            refs.remove(0);
        }
        let markers = refs
            .iter()
            .map(|source| format!("【{source}】"))
            .collect::<String>();
        // First otherwise valid answer has a broken PLAN shape; the same-document correction
        // must be automatic and must keep this identical complete packet.
        let repaired = transcript
            .iter()
            .any(|(_, text)| text.starts_with("REPAIR:") && text.contains("planning answer"));
        let markdown = if repaired {
            fixture_plan(&format!(
                "Bestehende Schnittstellen tragen den CSV-Import. {markers}"
            ))
        } else {
            format!("PLAN:\n## Summary\nSchnittstellen geprüft. {markers}")
        };
        Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"sufficient","markdown":markdown,"source_refs":refs,"note":note}}).to_string())
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
fn plan_research_converges_from_current_packet_repairs_and_publishes_review_revision()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, api) in INTERFACES {
            repository.write(
                path,
                format!(
                    "{}{api}\n{}",
                    "# unrelated earlier code\n".repeat(60),
                    if path == "main.py" { "" } else { "    pass\n" }
                ),
            )?;
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
        for (index, budget, depth, behavior) in [
            (1, 2048, AgentResearchDepth::Standard, Behavior::Plan),
            (2, 4096, AgentResearchDepth::Thorough, Behavior::Plan),
            (3, 4096, AgentResearchDepth::Standard, Behavior::Incomplete),
            (4, 4096, AgentResearchDepth::Standard, Behavior::Question),
            (5, 128, AgentResearchDepth::Standard, Behavior::ContextLimit),
            (
                6,
                4096,
                AgentResearchDepth::Standard,
                Behavior::RepairMissingCitation,
            ),
        ] {
            let id = AgentSessionId::from_bytes([index; 32]);
            let time = timestamp()?;
            let session = AgentSession::from_parts(
                id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("CSV plan regression".to_owned())?,
                AgentSessionMode::Plan,
                AgentSessionState::Running,
                time,
                time,
                Some(AgentSessionSequence::FIRST),
                None,
                None,
                false,
            );
            let user = AgentSessionEntry::try_new(
                id,
                AgentSessionSequence::FIRST,
                AgentSessionEntryKind::UserMessage,
                AgentSessionText::try_from_string(PLAN_QUERY.to_owned())?,
                time,
                None,
                None,
                None,
            )?;
            store
                .create_session(&project, &session, Some(&user), None)
                .await?;
            let model = Arc::new(PlanModel {
                budget,
                calls: AtomicUsize::new(0),
                behavior,
            });
            let worker_model = model.clone();
            let worker_project = project.clone();
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned(move |control, _| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()?;
                let result = runtime.block_on(researcher.research(
                    worker_model.as_ref(),
                    &worker_project,
                    id,
                    AgentSessionSequence::FIRST,
                    AgentSessionMode::Plan,
                    depth,
                    PLAN_QUERY,
                    &[(ModelMessageRole::User, PLAN_QUERY.to_owned())],
                    None,
                    &control,
                ));
                send.send(result)?;
                Ok(())
            })?;
            let result = receive.recv_timeout(Duration::from_secs(1))??;
            if behavior == Behavior::ContextLimit {
                assert!(result.awaiting_continuation);
                assert!(
                    result
                        .markdown
                        .contains(ResearchStopReason::ContextLimit.message())
                );
                assert_eq!(model.calls.load(Ordering::SeqCst), 0);
                continue;
            }
            if behavior == Behavior::Incomplete {
                assert!(result.awaiting_continuation);
                assert!(
                    result
                        .markdown
                        .contains(ResearchStopReason::Stagnation.message())
                );
                assert!(model.calls.load(Ordering::SeqCst) < 12);
                continue;
            }
            if behavior == Behavior::RepairMissingCitation {
                assert!(result.awaiting_continuation);
                assert!(
                    result
                        .markdown
                        .contains(ResearchStopReason::CitationRepair.message())
                );
                assert_eq!(model.calls.load(Ordering::SeqCst), 3);
                continue;
            }
            assert!(!result.awaiting_continuation, "{}", result.markdown);
            let (state, kind, plan_revision, content) =
                plan_session_outcome(&session, &result.markdown, !result.citations.is_empty());
            let question = behavior == Behavior::Question;
            assert_eq!(result.citations.len(), if question { 0 } else { 4 });
            assert_eq!(
                a3_domain::AgentWorkPlan::from_reviewed_markdown(&result.markdown).is_ok(),
                !question
            );
            assert_eq!(
                state,
                if question {
                    AgentSessionState::AwaitingUser
                } else {
                    AgentSessionState::AwaitingPlanReview
                }
            );
            assert_eq!(
                kind,
                if question {
                    AgentSessionEntryKind::AssistantSummary
                } else {
                    AgentSessionEntryKind::Plan
                }
            );
            assert_eq!(plan_revision, if question { None } else { Some(1) });
            let next = successor(
                &session,
                SessionSuccessor {
                    title: session.title().as_str().to_owned(),
                    mode: session.mode(),
                    state,
                    updated_at: timestamp()?,
                    latest_sequence: Some(AgentSessionSequence::FIRST.next()?),
                    active_work_item: None,
                    plan_revision,
                    presentation_deleted: false,
                },
            )?;
            validate_agent_session_transition(&session, &next)?;
            let entry = AgentSessionEntry::try_new(
                id,
                AgentSessionSequence::FIRST.next()?,
                kind,
                AgentSessionText::try_from_string(content)?,
                next.updated_at(),
                None,
                None,
                plan_revision,
            )?;
            store
                .complete_turn(
                    &project,
                    session.revision(),
                    &next,
                    &entry,
                    &result.terminal_event,
                    &result.citations,
                    &result.diagrams,
                )
                .await?;
            let stored = store
                .load_session(&project, id, None, 50)
                .await?
                .ok_or("persisted plan")?;
            assert_eq!(stored.session().state(), state);
            assert_eq!(stored.session().current_plan_revision(), plan_revision);
            assert!(stored.session().active_work_item().is_none());
            assert_eq!(stored.entries().last().ok_or("plan entry")?.kind(), kind);
            if !question {
                let trace = store
                    .load_detail(&project, id, AgentSessionSequence::FIRST)
                    .await?
                    .ok_or("research trace")?;
                assert_eq!(
                    trace
                        .events()
                        .iter()
                        .filter(|event| event.action().contains("research-v1/plan-shape"))
                        .count(),
                    1
                );
                println!(
                    "stateless plan fixture: {budget} bytes, {depth:?}, {} calls, four interfaces, one repair, persisted review revision 1",
                    model.calls.load(Ordering::SeqCst)
                );
            }
        }
        Ok(())
    })
}

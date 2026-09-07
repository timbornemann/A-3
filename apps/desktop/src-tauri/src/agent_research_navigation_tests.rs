//! V6 follows names discovered in real originals, never model-invented paths or actions.
use super::*;
use a3_application::ResearchOutputPhase;

const QUERY: &str = "Erkläre die in entry.py referenzierten Helfer und deren Rückgabewert.";
const FILES: [(&str, &str); 3] = [
    (
        "entry.py",
        "def entry(registry):\n    return registry['resolve_target']()\n",
    ),
    (
        "zeta.py",
        "def resolve_target():\n    return registry['resolve_destination']()\n",
    ),
    (
        "kappa.py",
        "def resolve_destination():\n    return 'archive.log'\n",
    ),
];

struct NavigationModel {
    calls: AtomicUsize,
    needs: std::sync::Mutex<Vec<String>>,
    invalid: bool,
    disjoint: bool,
}

fn anchor(packet: &str, needle: &str) -> Option<serde_json::Value> {
    let mut current = None;
    for line in packet.lines() {
        if line.starts_with("[S") {
            current = line
                .rsplit_once(" [E")
                .and_then(|(_, label)| label.strip_suffix(']'))
                .map(|label| format!("E{label}"));
        } else if line.contains(needle) {
            return current.map(|label| serde_json::json!({"anchor_ref":label}));
        }
    }
    None
}

impl ResearchModel for NavigationModel {
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
        phase: ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(call < 12, "unchanged outer navigation budget");
        let packet = &transcript
            .iter()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1;
        assert!(packet.len() <= 4096);
        println!(
            "navigation packet {phase:?} call={call}: {:?}; target={} destination={}",
            packet
                .lines()
                .filter(|line| line.starts_with("[S"))
                .collect::<Vec<_>>(),
            anchor(packet, "registry['resolve_destination']()").is_some(),
            anchor(packet, "return 'archive.log'").is_some()
        );
        let mut document = serde_json::json!({"schema_version":6,"work":{"questions":[],"results":[]},"decision":{"kind":"progress"}});
        match phase {
            ResearchOutputPhase::Initialize => {
                document["work"]["questions"] = serde_json::json!([{
                    "kind":"repository","outcome":"Explain the referenced helper candidates and return value without claiming a proven runtime binding.",
                    "priority":"required","dependencies":[]
                }]);
            }
            ResearchOutputPhase::Analyze(id) | ResearchOutputPhase::SummarizeOriginals(id) => {
                let target = if self.invalid {
                    Some("invented_private_helper")
                } else if anchor(packet, "registry['resolve_destination']()").is_none() {
                    Some("resolve_target")
                } else if anchor(packet, "return 'archive.log'").is_none() {
                    Some("resolve_destination")
                } else {
                    None
                };
                if let Some(target) = target {
                    if !self.invalid {
                        assert!(
                            anchor(packet, &format!("registry['{target}']()")).is_some(),
                            "need must come from a current original, not earlier model memory"
                        );
                    }
                    self.needs
                        .lock()
                        .map_err(|_| AgentConversationFailure::Unavailable)?
                        .push(target.to_owned());
                    document["decision"] = serde_json::json!({"kind":"evidenceNeed","question_id":id.get(),"targets":[target]});
                } else {
                    let evidence = [
                        "registry['resolve_target']()",
                        "registry['resolve_destination']()",
                        "return 'archive.log'",
                    ]
                    .iter()
                    .map(|needle| anchor(packet, needle))
                    .collect::<Option<Vec<_>>>()
                    .ok_or(AgentConversationFailure::InvalidInput)?;
                    document["work"]["results"] = serde_json::json!([{"question_id":id.get(),"kind":"interpretation",
                        "text":"entry ruft den Registry-Eintrag resolve_target auf. Der gleichnamige Kandidat in zeta.py ruft den Eintrag resolve_destination auf; dessen Kandidat in kappa.py gibt archive.log zurück. Die Namensgleichheit allein beweist keine Laufzeitverdrahtung.","evidence":evidence}]);
                }
            }
            ResearchOutputPhase::Design(id) | ResearchOutputPhase::DesignTests(id) => {
                document["work"]["results"] = serde_json::json!([{"question_id":id.get(),"kind":"designDecision",
                    "text":"Die Kandidatenkette dokumentieren; mit injizierten Helfern Eingabe, Aufruf und Rückgabewert archive.log separat prüfen. Keine bestehende Laufzeitverdrahtung unterstellen.","evidence":[]}]);
            }
            ResearchOutputPhase::Finalize => return Err(AgentConversationFailure::InvalidInput),
        }
        if self.disjoint {
            let response = if phase == ResearchOutputPhase::Initialize {
                serde_json::json!({"kind":"questions","questions":document["work"]["questions"]})
            } else if document["decision"]["kind"] == "evidenceNeed" {
                document["decision"].clone()
            } else {
                let mut result = document["work"]["results"][0].clone();
                let kind = result
                    .as_object_mut()
                    .ok_or(AgentConversationFailure::InvalidOutput)?
                    .remove("kind")
                    .ok_or(AgentConversationFailure::InvalidOutput)?;
                serde_json::json!({"kind":kind,"result":result})
            };
            document = serde_json::json!({"schema_version":7,"response":response});
        }
        Ok(document.to_string())
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
fn research_v6_original_bound_needs_follow_two_helpers_and_persist_as_navigation()
-> Result<(), Box<dyn Error>> {
    navigation_fixture(false, false)
}

#[test]
fn research_v6_invented_need_gets_one_repair_without_adaptive_reads() -> Result<(), Box<dyn Error>>
{
    navigation_fixture(true, false)
}

#[test]
fn research_v7_disjoint_navigation_preserves_origins_resume_and_invalid_need_boundary()
-> Result<(), Box<dyn Error>> {
    navigation_fixture(false, true)?;
    navigation_fixture(true, true)
}

fn navigation_fixture(invalid: bool, disjoint: bool) -> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let files = FILES.map(|(path, body)| {
            let body = if path == "entry.py" {
                body.to_owned()
            } else {
                body.replacen(
                    '\n',
                    &format!(
                        "\n{}",
                        "    # existing unrelated internal bookkeeping\n".repeat(190)
                    ),
                    1,
                )
            };
            (path, body)
        });
        for (path, body) in &files {
            repository.write(path, body)?;
        }
        for number in 0..32 {
            repository.write(
                format!("noise/utility{number:02}.py"),
                format!("def utility{number:02}(value):\n    return value\n"),
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
                AgentSessionTitle::try_from_string("V6 navigation".to_owned())?,
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
            let model = Arc::new(NavigationModel {
                calls: AtomicUsize::new(0),
                needs: std::sync::Mutex::new(Vec::new()),
                invalid,
                disjoint,
            });
            let worker_model = model.clone();
            let worker_project = project.clone();
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned_with_timeout(Duration::from_secs(20), move |control, _| {
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
            let work = detail.work_state().ok_or("work")?;
            let needs = model.needs.lock().map_err(|_| "poisoned fixture")?.clone();
            println!(
                "navigation {mode:?}: needs={needs:?} calls={} ready={} accesses={} result={}",
                model.calls.load(Ordering::SeqCst),
                work.ready_to_finish(),
                work.accesses().len(),
                result.markdown
            );
            assert_eq!(result.awaiting_continuation, invalid);
            assert_eq!(work.ready_to_finish(), !invalid);
            assert!(
                detail
                    .events()
                    .iter()
                    .all(|event| event.public_note().is_none())
            );
            if invalid {
                assert_eq!(needs.len(), 2, "exactly one repair");
                assert!(work.accesses().is_empty());
                assert!(
                    detail
                        .events()
                        .iter()
                        .all(|event| event.query() != Some("invented_private_helper"))
                );
            } else {
                let mut stages = needs.clone();
                stages.dedup();
                assert_eq!(
                    stages,
                    ["resolve_target", "resolve_destination"],
                    "no return to an already found helper"
                );
                assert!(
                    !work.accesses().is_empty(),
                    "real additional reads, not model memory"
                );
                for target in &*needs {
                    assert!(
                        detail
                            .events()
                            .iter()
                            .any(|event| event.query() == Some(target)),
                        "navigation hint must survive database reload"
                    );
                }
                assert!(result.markdown.contains("archive.log"));
                assert_restored_navigation(&store, &project, id, mode).await?;
            }
            for (path, body) in &files {
                assert_eq!(
                    std::fs::read_to_string(repository.path().join(path))?,
                    *body
                );
            }
        }
        Ok(())
    })
}

async fn assert_restored_navigation(
    store: &Arc<LibsqlKnowledgeStore>,
    project: &ProjectIdentity,
    id: AgentSessionId,
    mode: AgentSessionMode,
) -> Result<(), Box<dyn Error>> {
    let sequence = AgentSessionSequence::new(2)?;
    let time = timestamp()?;
    let session = AgentSession::from_parts(
        id,
        AgentSessionRevision::new(2)?,
        AgentSessionTitle::try_from_string("V6 navigation".to_owned())?,
        mode,
        AgentSessionState::Running,
        time,
        time,
        Some(sequence),
        None,
        None,
        false,
    );
    let entry = AgentSessionEntry::try_new(
        id,
        sequence,
        AgentSessionEntryKind::UserMessage,
        AgentSessionText::try_from_string(QUERY.to_owned())?,
        time,
        None,
        None,
        None,
    )?;
    store
        .append_session_revision(
            project,
            AgentSessionRevision::new(1)?,
            &session,
            Some(&entry),
            None,
        )
        .await?;
    let published = store
        .latest_published_index(project, &FixtureControl)
        .await?
        .ok_or("index")?;
    let turn = AskResearchTurn::new_for_mode(
        id,
        sequence,
        published.run().id(),
        published.run().snapshot_id(),
        time,
        mode,
        AgentResearchDepth::Standard,
    );
    let first = research_event(
        id,
        sequence,
        1,
        AskResearchPhase::Preparing,
        AskResearchState::Running,
        "Revalidate stored navigation",
        None,
        AskResearchCompleteness::NotApplicable,
    )?;
    store.begin_turn(project, &turn, &first).await?;
    let worker_project = project.clone();
    let mut researcher =
        AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
    researcher.continuation_from = Some(AgentSessionSequence::FIRST);
    recovery_contract::owned_with_timeout(Duration::from_secs(20), move |control, _| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let mut state = AskResearchWorkingSet::new(4096);
        runtime.block_on(researcher.reuse_previous_sources(
            &worker_project,
            &published,
            &turn,
            QUERY,
            &mut state,
            &control,
        ))?;
        assert!(state.memory_findings.is_empty());
        assert!(state.memory_gaps.is_empty());
        assert!(state.memory.is_none());
        assert!(state.continuation_feedback.contains("resolve_destination"));
        assert!(
            !state.sources.is_empty(),
            "originals must be revalidated, not merely notes loaded"
        );
        assert!(state.work.is_some());
        Ok(())
    })?;
    Ok(())
}

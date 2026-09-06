//! No model-side evidence history: the entire call chain must coexist in one real packet.
use super::*;

const QUERY: &str = "Verfolge die Aufgabenerstellung in taskflow/manager.py, taskflow/plugins/base.py und taskflow/plugins/audit_log_plugin.py. Welche Methoden werden aufgerufen und wohin wird das Audit-Log geschrieben?";
const ADD: &str = "    def add_task(self, project_id, title):\n        if project_id not in self.projects:\n            raise ValueError('unknown project')\n        task = Task(title)\n        self.tasks.append(task)\n        self.storage.save_tasks(self.tasks)\n        self.plugin_manager.trigger_task_created(task.to_dict())\n        return task\n";
const DISPATCH: &str = "    def trigger_task_created(self, task_data):\n        for plugin in self.plugins:\n            plugin.on_task_created(task_data)\n";
const INIT: &str = "    def __init__(self, log_filepath='audit_log.txt'):\n        self.log_filepath = os.path.abspath(log_filepath)\n";
const LOG: &str = "    def _log(self, event, task_data):\n        with open(self.log_filepath, 'a', encoding='utf-8') as log:\n            log.write(f'{event}: {task_data}\\n')\n";
const CALLBACK: &str =
    "    def on_task_created(self, task_data):\n        self._log('TASK_CREATED', task_data)\n";
const PATHS: [&str; 3] = [
    "taskflow/manager.py",
    "taskflow/plugins/base.py",
    "taskflow/plugins/audit_log_plugin.py",
];

struct CoherentModel {
    live: Option<live_fixture::LiveResearchModel>,
    fault: WorkFault,
    work_contract: bool,
    budget: usize,
    calls: AtomicUsize,
    diagrams: AtomicUsize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkFault {
    None,
    NoResults,
    InvalidInitialization,
    InvalidAnalysis,
    EmptyDesignOnce,
    EmptyDesignAlways,
    MissingOriginalOnce,
    RepeatedOriginalAnchors,
}

fn quote(packet: &str, needle: &str) -> Option<serde_json::Value> {
    let mut source = None;
    let mut anchor = None;
    for line in packet.lines() {
        if let Some(header) = line.strip_prefix("[S") {
            let (ordinal, rest) = header.split_once("] ")?;
            rest.split_once(" ab Zeile ")?;
            source = Some(format!("S{ordinal}"));
            anchor = line
                .rsplit_once(" [E")
                .and_then(|(_, tail)| tail.strip_suffix(']'))
                .map(|number| format!("E{number}"));
        } else if line.contains(needle)
            && let Some(source) = &source
        {
            return Some(anchor.as_ref().map_or_else(
                || serde_json::json!({"source_ref":source,"quote":needle}),
                |anchor| serde_json::json!({"anchor_ref":anchor}),
            ));
        }
    }
    None
}

fn v5_decision(
    packet: &str,
    mode: AgentSessionMode,
    call: usize,
) -> Result<String, AgentConversationFailure> {
    assert!(call < 6, "V5 did not converge");
    if mode != AgentSessionMode::Ask {
        let id = if packet.contains("ACTIVE Q1:") {
            1
        } else if packet.contains("ACTIVE Q2:") {
            2
        } else {
            3
        };
        let evidence = [
            "self.plugin_manager.trigger_task_created(task.to_dict())",
            "plugin.on_task_created(task_data)",
            "self.log_filepath = os.path.abspath(log_filepath)",
            "log.write(",
            "self._log('TASK_CREATED', task_data)",
        ]
        .into_iter()
        .map(|needle| quote(packet, needle))
        .collect::<Option<Vec<_>>>();
        let results = match (id, evidence) {
            (1, Some(mut anchors)) => {
                anchors.dedup();
                serde_json::json!([{"question_id":1,"kind":"interpretation","text":"add_task calls trigger_task_created, on_task_created and _log, which appends to os.path.abspath('audit_log.txt') relative to the working directory.","evidence":anchors}])
            }
            (1, None) => serde_json::json!([]),
            _ => {
                serde_json::json!([{"question_id":id,"kind":"designDecision","text":if id==2 {"Document the requested method chain and destination without changing behavior."} else {"Verify all named methods, ordering, audit_log.txt, CWD resolution and append behavior against the original code."},"evidence":[]}])
            }
        };
        return Ok(serde_json::json!({"schema_version":5,"work":{"questions":[],"results":results},"decision":{"kind":"progress","note":{"goal":"Requested plan","finding_kind":"hypothesis","finding":"Bounded result","finding_source_refs":[],"gap":"Current original method bodies","next_step":"Resolve the selected obligation"}}}).to_string());
    }
    let initial = !packet.contains("CORE RESEARCH CONTRACT");
    let questions = if initial {
        serde_json::json!([
            {"request_fragment":"Welche Methoden werden aufgerufen", "outcome":"Die vollständige Aufrufkette der Aufgabenerstellung erklären", "priority":"required", "kind":"repository", "dependencies":[]},
            {"request_fragment":"wohin wird das Audit-Log geschrieben", "outcome":"konkretes Schreibziel und Standardwert des Audit-Logs erklären", "priority":"required", "kind":"repository", "dependencies":[]},
            {"request_fragment":"Aufgabenerstellung", "outcome":"optionale Plugin-Registrierung", "priority":"optional", "kind":"repository", "dependencies":[]}
        ])
    } else {
        serde_json::json!([])
    };
    let mut results = Vec::new();
    if !packet.contains("Q1 result:")
        && let Some(mut evidence) = [
            "self.plugin_manager.trigger_task_created(task.to_dict())",
            "plugin.on_task_created(task_data)",
            "self._log('TASK_CREATED', task_data)",
            "log.write(",
        ]
        .into_iter()
        .map(|needle| quote(packet, needle))
        .collect::<Option<Vec<_>>>()
    {
        evidence.dedup();
        results.push(serde_json::json!({"question_id":1,"kind":"interpretation","text":"add_task ruft trigger_task_created auf, das on_task_created und darüber _log erreicht.","evidence":evidence}));
    }
    if packet.contains("ACTIVE Q2:")
        && let Some(mut evidence) = [
            "log_filepath='audit_log.txt'",
            "self.log_filepath = os.path.abspath(log_filepath)",
            "with open(self.log_filepath, 'a', encoding='utf-8') as log:",
        ]
        .into_iter()
        .map(|needle| quote(packet, needle))
        .collect::<Option<Vec<_>>>()
    {
        evidence.dedup();
        results.push(serde_json::json!({"question_id":2,"kind":"interpretation","text":"Geschrieben wird append in os.path.abspath('audit_log.txt'); ein übergebener log_filepath ersetzt den Standard.","evidence":evidence}));
    }
    let note = serde_json::json!({"goal":"Aufrufkette", "finding_kind":"hypothesis", "finding":"Zwischenstand", "finding_source_refs":[], "gap":"optionale Plugin-Registrierung", "next_step":"Plugin-Registrierung nochmal suchen"});
    // Intentionally omit the requested path from the final prose. The Core must retain Q2.
    let refs = PATHS
        .iter()
        .filter_map(|path| source_ref(packet, path).ok())
        .collect::<Vec<_>>();
    let summary = format!(
        "Aufrufkette geklärt. {}",
        refs.iter().map(|r| format!("【{r}】")).collect::<String>()
    );
    let markdown = if mode == AgentSessionMode::Ask {
        summary
    } else {
        fixture_plan(&summary)
    };
    let decision = if results.is_empty() && !packet.contains("ALL REQUIRED QUESTIONS RESOLVED") {
        serde_json::json!({"kind":"research","evidence_status":"incomplete","note":note,"actions":PATHS.map(|path| serde_json::json!({"kind":"inspectPath","path":path,"start_line":1}))})
    } else {
        serde_json::json!({"kind":"answer","evidence_status":"sufficient","note":note,"markdown":markdown,"source_refs":refs})
    };
    Ok(serde_json::json!({"schema_version":5,"decision":decision,"work":{"questions":questions,"results":results}}).to_string())
}
impl ResearchModel for CoherentModel {
    fn requires_work_contract(&self) -> bool {
        self.work_contract
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
        mode: AgentSessionMode,
        search: bool,
        phase: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let packet = &transcript
            .iter()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1;
        assert!(packet.len() <= self.budget);
        if self.budget < 3000 {
            println!(
                "small-packet: {phase:?}; bytes={}/{}; bodies={:?}; headers={:?}",
                packet.len(),
                self.budget,
                [ADD, DISPATCH, INIT, LOG, CALLBACK].map(|body| packet.contains(body)),
                packet
                    .lines()
                    .filter(|line| line.starts_with("[S"))
                    .collect::<Vec<_>>()
            );
        }
        if self.fault == WorkFault::InvalidInitialization {
            return Ok("{invalid initialization".to_owned());
        }
        if self.fault == WorkFault::InvalidAnalysis
            && matches!(phase, a3_application::ResearchOutputPhase::Analyze(_))
        {
            return Ok("{invalid analysis".to_owned());
        }
        if let Some(live) = &self.live {
            let output = live
                .complete(mode, search, phase, transcript, control)
                .await?;
            println!(
                "local-packet: phase={phase:?}; bytes={}; bodies={:?}",
                packet.len(),
                [ADD, DISPATCH, INIT, LOG, CALLBACK].map(|body| packet.contains(body))
            );
            if phase == a3_application::ResearchOutputPhase::Finalize
                && let Ok(document) = serde_json::from_str::<serde_json::Value>(&output)
            {
                println!(
                    "local-plan-shape: changes={}, tests={}",
                    document["decision"]["changes"]
                        .as_array()
                        .map_or(0, Vec::len),
                    document["decision"]["tests"].as_array().map_or(0, Vec::len)
                );
            }
            return Ok(output);
        }
        if self.fault == WorkFault::MissingOriginalOnce && call == 1 {
            let hint = &transcript
                .last()
                .ok_or(AgentConversationFailure::InvalidInput)?
                .1;
            assert!(hint.starts_with("Original coverage repair for Q1"));
            assert!(hint.contains("E1"));
            assert!(hint.len() <= 768);
        }
        if self.work_contract {
            let mut document: serde_json::Value =
                serde_json::from_str(&v5_decision(packet, mode, call)?)
                    .map_err(|_| AgentConversationFailure::InvalidOutput)?;
            if self.fault == WorkFault::NoResults {
                document["work"]["results"] = serde_json::json!([]);
                // Repeated confidence and the same optional gap must not become completion.
                document["decision"]["evidence_status"] = serde_json::json!("incomplete");
                document["decision"]["note"]["gap"] =
                    serde_json::json!("optionale Plugin-Registrierung");
            }
            if phase == a3_application::ResearchOutputPhase::Initialize {
                document["work"]["results"] = serde_json::json!([]);
                if let Some(questions) = document["work"]["questions"].as_array_mut() {
                    for question in questions {
                        if let Some(question) = question.as_object_mut() {
                            question.remove("request_fragment");
                        }
                    }
                }
            }
            if let a3_application::ResearchOutputPhase::Analyze(question)
            | a3_application::ResearchOutputPhase::SummarizeOriginals(question)
            | a3_application::ResearchOutputPhase::Design(question) = phase
                && let Some(results) = document["work"]["results"].as_array_mut()
            {
                results.retain(|r| r["question_id"] == question.get());
                if matches!(phase, a3_application::ResearchOutputPhase::Design(_)) {
                    for result in results {
                        result["evidence"] = serde_json::json!([]);
                    }
                }
            }
            if phase != a3_application::ResearchOutputPhase::Finalize {
                let note = document["decision"]["note"].clone();
                document["decision"] = serde_json::json!({"kind":"progress", "note":note});
            } else {
                let note = document["decision"]["note"].clone();
                document["decision"] = serde_json::json!({"kind":"plan", "note":note, "summary":"Aufrufkette geklärt.", "changes":["Die gewünschte Dokumentation der Aufrufkette ergänzen."], "interfaces":"Keine API-Änderung.", "tests":["Dokumentation gegen Originalbelege prüfen."], "assumptions":"Bestehendes Verhalten erhalten."});
                document["work"]["results"] = serde_json::json!([]);
            }
            if matches!(phase, a3_application::ResearchOutputPhase::Design(id) if id.get() == 2)
                && (self.fault == WorkFault::EmptyDesignAlways
                    || (self.fault == WorkFault::EmptyDesignOnce && call == 1))
            {
                document["work"]["results"] = serde_json::json!([]);
            }
            if self.fault == WorkFault::MissingOriginalOnce && call == 0 {
                document["work"]["results"][0]["evidence"] =
                    serde_json::json!([{"anchor_ref":"E1"}]);
            }
            if self.fault == WorkFault::RepeatedOriginalAnchors
                && let Some(results) = document["work"]["results"].as_array_mut()
            {
                for result in results {
                    if let Some(evidence) = result["evidence"].as_array_mut() {
                        evidence.extend(evidence.clone());
                    }
                }
            }
            return Ok(document.to_string());
        }
        let complete = [ADD, DISPATCH, INIT, LOG, CALLBACK]
            .iter()
            .all(|body| packet.contains(body))
            && [
                "class Manager:",
                "class BasePlugin:",
                "class PluginManager:",
                "class AuditLogPlugin:",
            ]
            .iter()
            .all(|scope| packet.contains(scope));
        let note = serde_json::json!({"goal":"Aufrufkette und Logziel prüfen","finding_kind":"hypothesis","finding":"Methoden gemeinsam prüfen","finding_source_refs":[],"gap":if call == 0 {"add_task trigger_task_created on_task_created _log"} else {"__init__ log_filepath"},"next_step":"Die zusammengehörigen Methodenkörper vergleichen"});
        if !complete {
            assert!(
                search && call < 4,
                "coherent research did not converge: budget={}, call={}, bodies={:?}",
                self.budget,
                call,
                [ADD, DISPATCH, INIT, LOG, CALLBACK].map(|body| packet.contains(body))
            );
            let actions = PATHS
                .iter()
                .map(|path| serde_json::json!({"kind":"inspectPath","path":path,"start_line":1}))
                .collect::<Vec<_>>();
            return Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"research","evidence_status":"incomplete","actions":actions,"note":note}}).to_string());
        }
        let refs = PATHS
            .iter()
            .map(|path| {
                packet
                    .lines()
                    .find_map(|line| {
                        let (label, rest) = line.strip_prefix('[')?.split_once("] ")?;
                        rest.starts_with(&format!("{path} ab Zeile "))
                            .then(|| label.to_owned())
                    })
                    .ok_or(AgentConversationFailure::InvalidInput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let summary = format!(
            "add_task → save_tasks → trigger_task_created → on_task_created → _log. Append in os.path.abspath('audit_log.txt'). {}",
            refs.iter().map(|r| format!("【{r}】")).collect::<String>()
        );
        let markdown = if mode == AgentSessionMode::Ask {
            summary
        } else {
            fixture_plan(&summary)
        };
        Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"sufficient","markdown":markdown,"source_refs":refs,"note":note}}).to_string())
    }
    async fn complete_evidence_diagrams(
        &self,
        transcript: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.diagrams.fetch_add(1, Ordering::SeqCst);
        let packet = transcript
            .iter()
            .find(|(_, text)| text.contains(ADD))
            .ok_or(AgentConversationFailure::InvalidInput)?;
        assert!(
            [DISPATCH, INIT, LOG, CALLBACK]
                .iter()
                .all(|body| packet.1.contains(body))
        );
        let manager = source_ref(&packet.1, PATHS[0])?;
        let audit = source_ref(&packet.1, PATHS[2])?;
        Ok(serde_json::json!({"schema_version":1,"diagrams":[{"type":"sequence","title":"Aufgabenerstellung","description":"Aktuelle Quellen","elements":[{"id":"manager","label":"Manager","category":"function","source_refs":[manager]}, {"id":"audit","label":"Audit","category":"function","source_refs":[audit]}],"relationships":[{"from":"manager","to":"audit","label":"benachrichtigt über Plugins","source_refs":[manager,audit]}]}]}).to_string())
    }
}

#[test]
fn research_keeps_complete_call_chain_and_log_initialization_in_one_packet()
-> Result<(), Box<dyn Error>> {
    coherent_fixture(false)
}

#[test]
fn research_v5_keeps_required_log_when_model_drops_it_and_persists_real_evidence()
-> Result<(), Box<dyn Error>> {
    coherent_fixture(true)
}

fn coherent_fixture(work_contract: bool) -> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(work_contract, false, WorkFault::None)
}

#[path = "agent_research_live_fixture.rs"]
mod live_fixture;

#[path = "agent_research_matrix.rs"]
mod matrix;

#[test]
#[ignore = "Requires explicit local-model approval and A3_LOCAL_RESEARCH_MODEL; never runs in CI"]
fn research_v5_local_model_coherent_smoke() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("A3_CONFIGURED_RESEARCH_CATALOG").is_some() {
        return Err("use the explicitly configured-model test".into());
    }
    coherent_fixture_selected(true, true, WorkFault::None)
}

#[test]
#[ignore = "Requires explicit approval for the actual configured provider and A3_CONFIGURED_RESEARCH_CATALOG"]
fn research_configured_model_coherent_smoke() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("A3_CONFIGURED_RESEARCH_CATALOG").is_none() {
        return Err("configured catalog opt-in missing".into());
    }
    coherent_fixture_selected(true, true, WorkFault::None)
}

#[test]
fn research_v5_unresolved_repeated_reads_end_honestly_without_legacy_recovery_or_false_success()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::NoResults)
}

#[test]
fn research_v5_invalid_initialization_gets_one_repair_and_never_a_legacy_recovery()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::InvalidInitialization)
}

#[test]
fn research_v5_invalid_analysis_does_not_poison_the_same_packet_on_resume()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::InvalidAnalysis)
}

#[test]
fn research_v5_empty_design_repairs_once_without_repository_reads() -> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::EmptyDesignOnce)
}

#[test]
fn research_v5_empty_design_after_repair_stops_without_repository_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::EmptyDesignAlways)
}

fn coherent_fixture_selected(
    work_contract: bool,
    live: bool,
    fault: WorkFault,
) -> Result<(), Box<dyn Error>> {
    coherent_fixture_with_query(work_contract, live, fault, QUERY)
}

#[test]
fn research_v5_missing_original_receives_exact_current_groups_in_its_single_repair()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::MissingOriginalOnce)
}

#[test]
fn research_v5_repeated_original_anchors_do_not_consume_repair_or_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::RepeatedOriginalAnchors)
}

#[test]
fn research_sentence_punctuation_does_not_invent_a_missing_original_file()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_with_query(
        true,
        false,
        WorkFault::None,
        "Wie verläuft die Aufgabenerstellung in taskflow/manager.py, taskflow/plugins/base.py und taskflow/plugins/audit_log_plugin.py? Welche Methoden werden aufgerufen und wohin wird das Audit-Log geschrieben?",
    )
}

fn coherent_fixture_with_query(
    work_contract: bool,
    live: bool,
    fault: WorkFault,
    fixture_query: &str,
) -> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(work_contract, live, fault, fixture_query, None)
}

#[test]
fn research_eight_k_profile_keeps_real_originals_and_work_contract_without_larger_limits()
-> Result<(), Box<dyn Error>> {
    use a3_domain::*;
    let profile = ModelProfile::from_probe(
        ModelProviderId::try_from_string("ollama".to_owned())?,
        ModelId::try_from_string("offline-8k-fixture".to_owned())?,
        ModelProfileSettings::new(
            ModelContextLimit::new(8192)?,
            ModelOutputLimit::new(2048)?,
            ModelTokenCountingStrategy::ConservativeUtf8BytesV1,
            ModelParallelismLimit::new(1)?,
            ModelSamplingProfile::new(
                ModelTemperature::from_milli(0)?,
                ModelTopP::from_milli(1000)?,
            ),
            ModelStopSequences::new(vec![])?,
            ModelPromptSchemaGrounding::FormatFieldOnly,
        )?,
        ModelCapabilities::new(
            ModelStructuredOutputCapability::Verified,
            ModelToolCallMode::NativeProviderReported,
        ),
    );
    coherent_fixture_with_profile(true, false, WorkFault::None, QUERY, Some(&profile))
}

fn coherent_fixture_with_profile(
    work_contract: bool,
    live: bool,
    fault: WorkFault,
    fixture_query: &str,
    profile: Option<&a3_domain::ModelProfile>,
) -> Result<(), Box<dyn Error>> {
    support::run_libsql_test_selected(
        async {
            let repository = support::TempDirectory::new()?;
            repository.git(["init", "--initial-branch=main"])?;
            let noise = format!(
                "    def unrelated(self):\n{}        return None\n\n",
                "        # unrelated project maintenance and validation details\n".repeat(100)
            );
            let files = [
                format!("class Manager:\n{noise}{ADD}"),
                format!(
                    "class BasePlugin:\n    def on_task_created(self, task_data):\n        raise NotImplementedError\n\nclass PluginManager:\n{noise}{DISPATCH}"
                ),
                format!("import os\nclass AuditLogPlugin:\n{INIT}\n{noise}{LOG}\n{CALLBACK}"),
            ];
            for (path, body) in PATHS.iter().zip(&files) {
                repository.write(path, body)?;
            }
            repository.git(["add", "."])?;
            let project = RepositoryInspector::new().inspect(repository.path())?;
            let data = support::TempDirectory::new()?;
            let store = Arc::new(
                LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?)
                    .await?,
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
            let local_model = if live {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                Some(runtime.block_on(live_fixture::LiveResearchModel::probe())?)
            } else {
                None
            };
            for (index, mode, budget) in [
                (1, AgentSessionMode::Ask, 4096),
                (2, AgentSessionMode::Plan, 4096),
                (3, AgentSessionMode::Agent, 4096),
                (4, AgentSessionMode::Ask, 2048),
                (5, AgentSessionMode::Ask, 8192),
                (6, AgentSessionMode::Ask, 4096),
            ] {
                if matches!(
                    fault,
                    WorkFault::EmptyDesignOnce
                        | WorkFault::EmptyDesignAlways
                        | WorkFault::MissingOriginalOnce
                ) && mode == AgentSessionMode::Ask
                {
                    continue;
                }
                if work_contract && (index == 4 || index == 6) {
                    continue;
                }
                if live && index > 3 {
                    continue;
                }
                let query = if index == 6 {
                    format!("/diagram {fixture_query}")
                } else {
                    fixture_query.to_owned()
                };
                let command_profile = match parse_slash_command(mode, &query)? {
                    ParsedSlashCommand::Command(invocation) => {
                        Some(SlashCommandExecutionProfile::resolve(invocation))
                    }
                    ParsedSlashCommand::Plain(_) => None,
                };
                let id = AgentSessionId::from_bytes([index; 32]);
                let time = timestamp()?;
                let session = AgentSession::from_parts(
                    id,
                    AgentSessionRevision::new(1)?,
                    AgentSessionTitle::try_from_string("Coherent research".to_owned())?,
                    mode,
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
                    AgentSessionText::try_from_string(query.clone())?,
                    time,
                    None,
                    None,
                    None,
                )?;
                store
                    .create_session(&project, &session, Some(&user), None)
                    .await?;
                let budget = profile.map_or(Ok(budget), |profile| {
                    crate::agent_conversation_runtime::research_evidence_budget_for_profile(
                        profile, mode, None,
                    )
                })?;
                let budget = local_model
                    .as_ref()
                    .map_or(Ok(budget), |model| model.evidence_budget(mode))?;
                let model = Arc::new(CoherentModel {
                    live: local_model.clone(),
                    fault,
                    work_contract,
                    budget,
                    calls: AtomicUsize::new(0),
                    diagrams: AtomicUsize::new(0),
                });
                let worker_model = model.clone();
                let worker_project = project.clone();
                let researcher = AgentAskResearcher::new(
                    store.clone(),
                    store.clone(),
                    store.clone(),
                    store.clone(),
                );
                let (send, receive) = std::sync::mpsc::sync_channel(1);
                let started = Instant::now();
                recovery_contract::owned_with_timeout(
                    if live {
                        Duration::from_secs(420)
                    } else {
                        Duration::from_secs(20)
                    },
                    move |control, _| {
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
                            &query,
                            &[(ModelMessageRole::User, query.clone())],
                            command_profile.as_ref(),
                            &control,
                        )))?;
                        Ok(())
                    },
                )?;
                let received = receive.recv_timeout(Duration::from_secs(1))?;
                if live && received.is_err() {
                    println!(
                        "local-research-category: {:?}; calls={}; elapsed_ms={}",
                        received.as_ref().err(),
                        model.calls.load(Ordering::SeqCst),
                        started.elapsed().as_millis()
                    );
                    if let Some(detail) = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                    {
                        for event in detail.events() {
                            println!("local-event: {}", event.action());
                        }
                    }
                }
                let result = received?;
                if fault == WorkFault::RepeatedOriginalAnchors {
                    assert!(
                        !result.awaiting_continuation,
                        "identical valid anchors cannot block research"
                    );
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        3,
                        "no duplicate-anchor repair"
                    );
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(work.ready_to_finish());
                    assert!(work.accesses().is_empty());
                }
                if matches!(
                    fault,
                    WorkFault::EmptyDesignOnce
                        | WorkFault::EmptyDesignAlways
                        | WorkFault::MissingOriginalOnce
                ) {
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(
                        work.accesses().is_empty(),
                        "repairing current output must not start repository navigation"
                    );
                    let failed = fault == WorkFault::EmptyDesignAlways;
                    assert_eq!(result.awaiting_continuation, failed);
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        if failed { 3 } else { 4 }
                    );
                    assert_eq!(work.ready_to_finish(), !failed);
                    assert_eq!(work.resolved_count(), if failed { 1 } else { 3 });
                    if failed {
                        assert!(
                            work.questions()[1].attempts().is_empty(),
                            "invalid designs must not poison packet receipts"
                        );
                    }
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if fault == WorkFault::InvalidInitialization {
                    assert!(result.awaiting_continuation);
                    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    if mode == AgentSessionMode::Ask {
                        assert!(detail.work_state().is_none());
                    } else {
                        assert_eq!(
                            detail
                                .work_state()
                                .ok_or("Core plan work")?
                                .resolved_count(),
                            0
                        );
                    }
                    assert!(
                        detail
                            .events()
                            .iter()
                            .all(|e| !e.action().contains("Recovery"))
                    );
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if fault == WorkFault::NoResults {
                    assert!(result.awaiting_continuation);
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(!work.ready_to_finish());
                    assert_eq!(work.resolved_count(), 0);
                    assert!(!work.accesses().is_empty());
                    assert!(
                        work.accesses()
                            .iter()
                            .all(|access| access.outcome.is_some())
                    );
                    assert!(model.calls.load(Ordering::SeqCst) <= 12);
                    assert!(
                        detail
                            .events()
                            .iter()
                            .all(|e| !e.action().contains("Recovery"))
                    );
                    if mode == AgentSessionMode::Ask {
                        assert!(
                            work.questions()
                                .iter()
                                .all(|q| result.markdown.contains(&q.definition().outcome))
                        );
                    }
                    continue;
                }
                if fault == WorkFault::InvalidAnalysis {
                    assert!(result.awaiting_continuation);
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        if mode == AgentSessionMode::Ask { 3 } else { 2 }
                    ); // Core plan initialization consumes no model call
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(!work.ready_to_finish());
                    assert!(
                        work.questions().iter().all(|q| q.attempts().is_empty()),
                        "invalid outputs are not analyzed-packet receipts"
                    );
                    let mut restored = AskResearchWorkingSet::new(budget);
                    restored.restore_work(work, &[])?;
                    assert_eq!(
                        restored.work.as_ref().and_then(|w| w.next_question()),
                        Some(a3_domain::ResearchQuestionId::FIRST)
                    );
                    assert!(
                        restored.work.as_ref().ok_or("work")?.questions()[0]
                            .attempts()
                            .is_empty()
                    );
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if live {
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    println!(
                        "local-smoke: calls={} elapsed_ms={} continuation={} resolved={} questions={}",
                        model.calls.load(Ordering::SeqCst),
                        started.elapsed().as_millis(),
                        result.awaiting_continuation,
                        detail.work_state().map_or(0, |w| w.resolved_count()),
                        detail.work_state().map_or(0, |w| w.questions().len())
                    );
                    for event in detail.events() {
                        println!("local-event: {}", event.action());
                    }
                    if let Some(work) = detail.work_state() {
                        for question in work.questions() {
                            println!(
                                "local-public-question: {} -> {}",
                                question.definition().request_fragment,
                                question.definition().outcome
                            );
                            if let Some(result) = question.result() {
                                println!("local-public-result: {}", result.text());
                            }
                        }
                    }
                    assert!(
                        !result.awaiting_continuation,
                        "live model did not complete research"
                    );
                    assert!(
                        result.markdown.contains("audit_log.txt"),
                        "requested log destination missing"
                    );
                    assert!(
                        result
                            .handoff
                            .work_state()
                            .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
                    );
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                assert!(!result.awaiting_continuation, "{}", result.markdown);
                if work_contract {
                    assert!(
                        result.markdown.contains("audit_log.txt"),
                        "required answer omitted"
                    );
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("persisted trace")?;
                    let work = detail.work_state().ok_or("persisted work")?;
                    assert!(work.ready_to_finish());
                    if mode == AgentSessionMode::Ask {
                        assert_eq!(
                            work.questions().len(),
                            2,
                            "model-invented optional registration is not a user obligation"
                        );
                        assert_eq!(
                            work.questions()[1].result().ok_or("log result")?.sources()[0]
                                .revision
                                .path()
                                .as_bytes(),
                            PATHS[2].as_bytes()
                        );
                    } else {
                        assert_eq!(work.questions().len(), 3);
                        assert_eq!(
                            work.questions()[2]
                                .result()
                                .ok_or("verification design")?
                                .kind(),
                            a3_domain::ResearchResultKind::DesignDecision
                        );
                    }
                    assert!(
                        result
                            .handoff
                            .work_state()
                            .is_some_and(a3_domain::ResearchWorkState::ready_to_finish)
                    );
                }
                assert_eq!(result.citations.len(), 3);
                assert_eq!(result.diagrams.len(), usize::from(index == 6));
                assert_eq!(
                    model.diagrams.load(Ordering::SeqCst),
                    usize::from(index == 6)
                );
                assert!(model.calls.load(Ordering::SeqCst) <= 4);
                for (path, expected) in PATHS.iter().zip(&files) {
                    assert_eq!(
                        std::fs::read_to_string(repository.path().join(path))?,
                        *expected
                    );
                }
                println!(
                    "coherent fixture: {mode:?}, {budget} bytes, {} calls, 5/5 complete method bodies simultaneously",
                    model.calls.load(Ordering::SeqCst)
                );
            }
            Ok(())
        },
        live,
    )
}

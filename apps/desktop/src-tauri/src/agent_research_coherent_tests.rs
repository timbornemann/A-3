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
    truncated_packet: std::sync::Mutex<Option<String>>,
    command_packet: std::sync::Mutex<Option<String>>,
    oversized_transcript: std::sync::Mutex<Option<Vec<(ModelMessageRole, String)>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkFault {
    None,
    NoResults,
    InvalidInitialization,
    InvalidAnalysis,
    TruncatedAnalysisOnce,
    TruncatedAnalysisAlways,
    RenamedCommandDesignOnce,
    RenamedCommandDesignAlways,
    RenamedCommandTestsOnce,
    RenamedCommandTestsAlways,
    EmptyDesignOnce,
    EmptyDesignAlways,
    MissingOriginalOnce,
    RepeatedOriginalAnchors,
    LongDesign,
    LongInterpretation,
    OriginalDesignBasis,
    OriginalDesignLimit,
    OversizedAnalysisOnce,
    OversizedAnalysisAlways,
    OversizedDesignOnce,
    OversizedDesignAlways,
    OversizedTestsOnce,
    OversizedTestsAlways,
    EmptyNavigationStatus,
    RepeatedNoteSources,
    EchoTestObligationOnce,
    EchoTestObligationAlways,
    TestConfirmationOnce,
    TestConfirmationAlways,
    CoreStatus,
    CoreStatusRepairOnce,
    DisjointResponse,
    DisjointResponseRepairOnce,
}

impl WorkFault {
    fn result_oversize(self) -> Option<(u8, bool)> {
        match self {
            Self::OversizedAnalysisOnce => Some((1, false)),
            Self::OversizedAnalysisAlways => Some((1, true)),
            Self::OversizedDesignOnce => Some((2, false)),
            Self::OversizedDesignAlways => Some((2, true)),
            Self::OversizedTestsOnce => Some((3, false)),
            Self::OversizedTestsAlways => Some((3, true)),
            _ => None,
        }
    }
    fn command_rename(self) -> Option<(u8, bool)> {
        match self {
            Self::RenamedCommandDesignOnce => Some((2, false)),
            Self::RenamedCommandDesignAlways => Some((2, true)),
            Self::RenamedCommandTestsOnce => Some((3, false)),
            Self::RenamedCommandTestsAlways => Some((3, true)),
            _ => None,
        }
    }
}

fn retained_long_design() -> String {
    format!(
        "{} Stop on first error. Preserve earlier writes; no rollback, no skipping. Keep UTF-8: Größe 🦀.",
        "Preserve the existing audit call order and relative destination resolution. ".repeat(26)
    )
}

fn retained_long_interpretation() -> String {
    format!(
        "{} The audit destination is os.path.abspath('audit_log.txt'), resolved against the working directory at construction. It appends UTF-8: Größe 🦀.",
        "Manager.add_task saves tasks before calling the plugin callbacks. ".repeat(12)
    )
}

pub(super) fn quote(packet: &str, needle: &str) -> Option<serde_json::Value> {
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
    fn design_basis(&self) -> a3_application::ResearchDesignBasis {
        if matches!(
            self.fault,
            WorkFault::OriginalDesignBasis | WorkFault::OriginalDesignLimit
        ) {
            a3_application::ResearchDesignBasis::Originals
        } else {
            a3_application::ResearchDesignBasis::Interpretations
        }
    }
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
        if matches!(
            self.fault,
            WorkFault::TruncatedAnalysisOnce | WorkFault::TruncatedAnalysisAlways
        ) && matches!(phase, a3_application::ResearchOutputPhase::Analyze(id) if id == a3_domain::ResearchQuestionId::FIRST)
        {
            let mut original = self
                .truncated_packet
                .lock()
                .map_err(|_| AgentConversationFailure::Unavailable)?;
            if let Some(original) = original.as_ref() {
                assert_eq!(
                    original, packet,
                    "repair must retain the same entire current packet"
                );
                let hint = &transcript
                    .last()
                    .ok_or(AgentConversationFailure::InvalidInput)?
                    .1;
                assert!(hint.contains("SHORTER"));
                assert!(hint.len() <= 768);
                if self.fault == WorkFault::TruncatedAnalysisAlways {
                    return Err(AgentConversationFailure::OutputTruncated);
                }
            } else {
                *original = Some(packet.clone());
                return Err(AgentConversationFailure::OutputTruncated);
            }
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
            | a3_application::ResearchOutputPhase::Design(question)
            | a3_application::ResearchOutputPhase::DesignTests(question) = phase
                && let Some(results) = document["work"]["results"].as_array_mut()
            {
                results.retain(|r| r["question_id"] == question.get());
                if phase.is_design() {
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
            if let Some((target, repeated)) = self.fault.result_oversize()
                && matches!(phase, a3_application::ResearchOutputPhase::Analyze(id)
                    | a3_application::ResearchOutputPhase::SummarizeOriginals(id)
                    | a3_application::ResearchOutputPhase::Design(id)
                    | a3_application::ResearchOutputPhase::DesignTests(id) if id.get() == u16::from(target))
            {
                let text = "UNTRUSTED_OVERSIZE Größe 🦀. ".repeat(180);
                let mut original = self
                    .oversized_transcript
                    .lock()
                    .map_err(|_| AgentConversationFailure::Unavailable)?;
                let oversized = if let Some(original) = original.as_ref() {
                    let (hint, base) = transcript
                        .split_last()
                        .ok_or(AgentConversationFailure::InvalidInput)?;
                    assert_eq!(
                        base,
                        original.as_slice(),
                        "repair keeps the entire role-bound transcript"
                    );
                    assert!(
                        hint.1.len() <= 768
                            && hint.1.contains("SHORTER")
                            && hint.1.contains("4096")
                    );
                    assert!(
                        hint.1.contains("result-text-too-large")
                            && hint.1.contains(&text.trim().len().to_string())
                    );
                    assert!(
                        transcript
                            .iter()
                            .all(|(_, body)| !body.contains("UNTRUSTED_OVERSIZE"))
                    );
                    repeated
                } else {
                    *original = Some(transcript.to_vec());
                    true
                };
                if oversized {
                    document["work"]["results"][0]["text"] = serde_json::json!(text);
                }
            }
            if self.fault == WorkFault::EmptyNavigationStatus {
                document["decision"]["note"]["gap"] = serde_json::json!("");
                document["decision"]["note"]["next_step"] = serde_json::json!("");
            }
            if self.fault == WorkFault::RepeatedNoteSources {
                let source = source_ref(packet, PATHS[0])?;
                document["decision"]["note"]["finding_source_refs"] =
                    serde_json::json!([source, source]);
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
            if let Some((failed_question, repeated)) = self.fault.command_rename()
                && phase.is_design()
            {
                let mut renamed = false;
                let targeted = matches!(phase,
                    a3_application::ResearchOutputPhase::Design(id)
                    | a3_application::ResearchOutputPhase::DesignTests(id) if id.get() == u16::from(failed_question));
                if targeted {
                    let mut original = self
                        .command_packet
                        .lock()
                        .map_err(|_| AgentConversationFailure::Unavailable)?;
                    if let Some(original) = original.as_ref() {
                        assert_eq!(
                            original, packet,
                            "repair keeps the same original and prerequisites"
                        );
                        let hint = &transcript
                            .last()
                            .ok_or(AgentConversationFailure::InvalidInput)?
                            .1;
                        assert!(hint.starts_with("Request name repair for Q"));
                        assert!(hint.contains("export-events"));
                        assert!(hint.len() <= 768);
                        renamed = repeated;
                    } else {
                        *original = Some(packet.clone());
                        renamed = true;
                    }
                }
                document["work"]["results"][0]["text"] = serde_json::json!(if renamed {
                    "Implement or test export with a destination argument; preserve prior behavior, return 0 on success and 1 on failure."
                } else {
                    "Implement or test export-events with a destination argument; assert return 0 for a writable output and 1 for a denied output, preserving prior behavior."
                });
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
            if matches!(
                self.fault,
                WorkFault::LongDesign | WorkFault::OriginalDesignLimit
            ) {
                if matches!(phase, a3_application::ResearchOutputPhase::Design(id) if id.get() == 2)
                {
                    document["work"]["results"][0]["text"] =
                        serde_json::json!(retained_long_design());
                }
                if matches!(phase, a3_application::ResearchOutputPhase::DesignTests(id) if id.get() == 3)
                {
                    assert!(
                        packet.contains(&retained_long_design()),
                        "every late design byte must reach its test-design consumer"
                    );
                }
            }
            if matches!(
                self.fault,
                WorkFault::LongInterpretation
                    | WorkFault::OriginalDesignBasis
                    | WorkFault::OriginalDesignLimit
            ) {
                if matches!(phase, a3_application::ResearchOutputPhase::Analyze(id) if id.get() == 1)
                {
                    document["work"]["results"][0]["text"] =
                        serde_json::json!(retained_long_interpretation());
                }
                if phase.is_design() {
                    if matches!(
                        self.fault,
                        WorkFault::OriginalDesignBasis | WorkFault::OriginalDesignLimit
                    ) {
                        assert!(!packet.contains(&retained_long_interpretation()));
                        assert!(packet.contains("Interpretation prose omitted"));
                        for original in [ADD, DISPATCH, INIT, LOG, CALLBACK] {
                            assert!(
                                packet.contains(original),
                                "no original body may disappear with its summary"
                            );
                        }
                    } else {
                        assert!(
                            packet.contains(&retained_long_interpretation()),
                            "a fitting prerequisite must not lose its late destination to a fixed preview"
                        );
                    }
                }
            }
            if matches!(phase, a3_application::ResearchOutputPhase::DesignTests(id) if id.get() == 3)
                && (self.fault == WorkFault::EchoTestObligationAlways
                    || (self.fault == WorkFault::EchoTestObligationOnce && call == 2))
            {
                let outcome = packet
                    .lines()
                    .find_map(|line| line.strip_prefix("ACTIVE Q3: "))
                    .ok_or(AgentConversationFailure::InvalidInput)?;
                document["work"]["results"][0]["text"] = serde_json::json!(outcome);
            }
            if matches!(phase, a3_application::ResearchOutputPhase::DesignTests(id) if id.get() == 3)
                && (self.fault == WorkFault::TestConfirmationAlways
                    || (self.fault == WorkFault::TestConfirmationOnce && call == 2))
            {
                let note = document["decision"]["note"].clone();
                document["decision"] = serde_json::json!({"kind":"question","note":note,
                    "message":"Please confirm the test scenarios before I define the requested tests."});
                document["work"]["results"] = serde_json::json!([]);
            }
            if matches!(
                self.fault,
                WorkFault::CoreStatus
                    | WorkFault::CoreStatusRepairOnce
                    | WorkFault::DisjointResponse
                    | WorkFault::DisjointResponseRepairOnce
                    | WorkFault::TruncatedAnalysisOnce
                    | WorkFault::TruncatedAnalysisAlways
            ) || self.fault.command_rename().is_some()
                || self.fault.result_oversize().is_some()
            {
                document["schema_version"] = serde_json::json!(6);
                document["decision"]
                    .as_object_mut()
                    .ok_or(AgentConversationFailure::InvalidOutput)?
                    .remove("note");
                if self.fault == WorkFault::CoreStatusRepairOnce && call == 0 {
                    document["decision"]["note"] =
                        serde_json::json!({"finding":"injected presentation"});
                }
                if matches!(
                    self.fault,
                    WorkFault::DisjointResponse
                        | WorkFault::DisjointResponseRepairOnce
                        | WorkFault::TruncatedAnalysisOnce
                        | WorkFault::TruncatedAnalysisAlways
                ) || self.fault.command_rename().is_some()
                    || self.fault.result_oversize().is_some()
                {
                    let response = if phase == a3_application::ResearchOutputPhase::Initialize {
                        serde_json::json!({"kind":"questions","questions":document["work"]["questions"]})
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
                    if self.fault == WorkFault::DisjointResponseRepairOnce && call == 0 {
                        document["response"]["note"] =
                            serde_json::json!({"finding":"injected presentation"});
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

#[test]
fn research_oversized_results_repair_once_with_the_same_context_in_all_modes()
-> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedAnalysisOnce)
}

#[test]
fn research_oversized_results_twice_remain_open_after_reopening_without_more_reads()
-> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedAnalysisAlways)
}

// Each fault owns a named test: Windows isolation exits after its first fixture.
// A loop around that boundary would replay the first fault in every child process.
fn oversized_fixture(fault: WorkFault) -> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(true, false, fault, QUERY, Some(&eight_k_profile()?))
}

#[test]
fn research_oversized_results_design_repairs_once() -> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedDesignOnce)
}

#[test]
fn research_oversized_results_design_twice_cannot_complete() -> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedDesignAlways)
}

#[test]
fn research_oversized_results_tests_repair_once() -> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedTestsOnce)
}

#[test]
fn research_oversized_results_tests_twice_cannot_complete() -> Result<(), Box<dyn Error>> {
    oversized_fixture(WorkFault::OversizedTestsAlways)
}

#[test]
fn research_original_design_basis_stops_before_underdelivered_tests_and_preserves_checkpoint()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::OriginalDesignLimit,
        QUERY,
        Some(&eight_k_profile()?),
    )
}

#[test]
fn research_original_design_basis_keeps_real_eight_k_originals_and_full_decisions()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::OriginalDesignBasis,
        QUERY,
        Some(&eight_k_profile()?),
    )
}

#[test]
fn research_command_names_repair_without_extra_reads_in_plan_and_agent()
-> Result<(), Box<dyn Error>> {
    command_name_fixture(WorkFault::RenamedCommandDesignOnce)
}

#[test]
fn research_command_names_twice_invalid_preserve_unresolved_work_on_reopen()
-> Result<(), Box<dyn Error>> {
    command_name_fixture(WorkFault::RenamedCommandDesignAlways)
}

fn command_name_fixture(fault: WorkFault) -> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(
        true,
        false,
        fault,
        &format!("{QUERY} Plane python taskflow/manager.py export-events <destination>."),
        Some(&eight_k_profile()?),
    )
}

#[test]
fn research_command_names_in_tests_repair_without_extra_reads() -> Result<(), Box<dyn Error>> {
    command_name_fixture(WorkFault::RenamedCommandTestsOnce)
}

#[test]
fn research_command_names_in_tests_twice_invalid_preserve_unresolved_work()
-> Result<(), Box<dyn Error>> {
    command_name_fixture(WorkFault::RenamedCommandTestsAlways)
}

#[test]
fn research_truncated_analysis_repairs_once_with_unchanged_originals_in_all_modes()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::TruncatedAnalysisOnce)
}

#[test]
fn research_truncated_analysis_twice_cannot_complete_or_retry_again() -> Result<(), Box<dyn Error>>
{
    coherent_fixture_selected(true, false, WorkFault::TruncatedAnalysisAlways)
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
#[ignore = "Requires explicit approval for the configured provider and A3_CONFIGURED_RESEARCH_CATALOG"]
fn research_configured_model_empty_project_agent_smoke() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("A3_CONFIGURED_RESEARCH_CATALOG").is_none() {
        return Err("configured catalog opt-in missing".into());
    }
    support::run_libsql_test_selected(
        async {
            let repository = support::TempDirectory::new()?;
            repository.git(["init", "--initial-branch=main"])?;
            let project = RepositoryInspector::new().inspect(repository.path())?;
            let data = support::TempDirectory::new()?;
            let store = Arc::new(
                LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?)
                    .await?,
            );
            store.record_opened_project(&project).await?;
            RefreshRepositoryIndex::new(
                Arc::new(Blake3RepositorySnapshotBuilder::new()),
                store.clone(),
                Arc::new(Blake3IndexRunIdFactory),
            )
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?,
                &FixtureControl,
            )
            .await?;
            let live = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(live_fixture::LiveResearchModel::probe())?;
            let mode = AgentSessionMode::Agent;
            let budget = live.evidence_budget(mode)?;
            let model = Arc::new(CoherentModel {
                live: Some(live),
                fault: WorkFault::None,
                work_contract: true,
                budget,
                calls: AtomicUsize::new(0),
                diagrams: AtomicUsize::new(0),
                truncated_packet: std::sync::Mutex::new(None),
                command_packet: std::sync::Mutex::new(None),
                oversized_transcript: std::sync::Mutex::new(None),
            });
            let id = AgentSessionId::from_bytes([90; 32]);
            let time = timestamp()?;
            let objective = "erstelle einen kleinen python server mit einer hello world webseite";
            let session = AgentSession::from_parts(
                id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("Empty Luna project".to_owned())?,
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
                AgentSessionText::try_from_string(objective.to_owned())?,
                time,
                None,
                None,
                None,
            )?;
            store
                .create_session(&project, &session, Some(&user), None)
                .await?;
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let worker_model = model.clone();
            let worker_project = project.clone();
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned_with_timeout(Duration::from_secs(420), move |control, _| {
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
                    objective,
                    &[(ModelMessageRole::User, objective.to_owned())],
                    None,
                    &control,
                )))?;
                Ok(())
            })?;
            let result = receive.recv_timeout(Duration::from_secs(1))??;
            println!(
                "configured-empty-project: calls={} continuation={} plan={}",
                model.calls.load(Ordering::SeqCst),
                result.awaiting_continuation,
                result.markdown
            );
            assert!(!result.awaiting_continuation);
            assert!(result.has_plan_grounding());
            assert!(result.empty_publication_inventory);
            assert!(result.citations.is_empty());
            assert!(result.markdown.contains("server.py"));
            assert!(result.markdown.to_lowercase().contains("hello"));
            let plan = a3_domain::AgentWorkPlan::from_reviewed_markdown(&result.markdown)?;
            assert!(plan.steps().len() >= 2);
            assert!(plan.steps().iter().any(|step| {
                step.verification_intent() == a3_domain::AgentWorkPlanVerificationIntent::Change
            }));
            assert!(plan.steps().iter().any(|step| {
                step.verification_intent() == a3_domain::AgentWorkPlanVerificationIntent::Test
            }));
            Ok(())
        },
        true,
    )
}

fn run_combined_agent_attempt(
    executor: Arc<crate::ProductionAgentRunExecutor>,
    project: a3_domain::ProjectIdentity,
    request: a3_application::AgentRunExecutionRequest,
) -> Result<a3_application::AgentRunExecutionOutcome, Box<dyn Error>> {
    let (scheduler, events) = a3_application::JobScheduler::new(
        a3_application::JobSchedulerConfig::new(1, 2, 32)?,
        Arc::new(FixtureClock),
    )?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let job_id = a3_domain::JobId::new(1);
    scheduler.submit(
        job_id,
        a3_domain::JobOwner::new(1),
        move |control: a3_application::JobContext| {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| a3_application::AgentRunExecutionFailure::Unavailable)
                .and_then(|runtime| {
                    runtime.block_on(a3_application::AgentRunExecutor::execute(
                        executor.as_ref(),
                        &project,
                        request,
                        &control,
                    ))
                });
            let succeeded = result.is_ok();
            let _sent = send.send(result);
            if succeeded {
                a3_application::JobCompletion::Succeeded
            } else {
                a3_application::JobCompletion::Failed
            }
        },
    )?;
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() >= Duration::from_secs(180) {
            scheduler.cancel(job_id)?;
            return Err("combined Agent attempt timed out".into());
        }
        if let Some(event) = events.next_timeout(Duration::from_millis(100))?
            && matches!(
                event.kind(),
                a3_application::JobEventKind::Succeeded
                    | a3_application::JobEventKind::Failed
                    | a3_application::JobEventKind::Cancelled
            )
        {
            break;
        }
    }
    Ok(receive.recv_timeout(Duration::from_secs(1))??)
}

#[derive(Debug)]
struct CombinedWorkspaceControl;

impl a3_application::TaskLensWorkspaceControl for CombinedWorkspaceControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

fn validate_combined_approval(
    action: &a3_application::AgentApprovalAction,
) -> Result<(), Box<dyn Error>> {
    match action {
        a3_application::AgentApprovalAction::Patch(patch) => {
            const ALLOWED: [&str; 5] = [
                "server.py",
                "README.md",
                "test_server.py",
                "tests/__init__.py",
                "tests/test_server.py",
            ];
            if patch.files().is_empty() || patch.files().len() > ALLOWED.len() {
                return Err("combined fixture rejected an empty or oversized patch".into());
            }
            for file in patch.files() {
                let source = file
                    .source_path()
                    .and_then(|path| std::str::from_utf8(path.as_bytes()).ok());
                let target = file
                    .target_path()
                    .and_then(|path| std::str::from_utf8(path.as_bytes()).ok());
                let allowed = match file.operation() {
                    a3_application::AgentApprovalFileOperation::Add => {
                        source.is_none() && target.is_some_and(|path| ALLOWED.contains(&path))
                    }
                    a3_application::AgentApprovalFileOperation::Update => {
                        source == target && target.is_some_and(|path| ALLOWED.contains(&path))
                    }
                    a3_application::AgentApprovalFileOperation::Move
                    | a3_application::AgentApprovalFileOperation::Delete => false,
                };
                if !allowed {
                    return Err(
                        "combined fixture rejected a patch outside the reviewed paths".into(),
                    );
                }
            }
            Ok(())
        }
        a3_application::AgentApprovalAction::Process(process) => {
            let arguments = process
                .arguments()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let allowed_arguments = arguments == ["-B", "-m", "unittest", "discover"]
                || arguments == ["-B", "-m", "unittest", "discover", "-s", "tests"];
            if process.executable() == "python"
                && allowed_arguments
                && process.working_directory()
                    == &a3_application::AgentApprovalWorkingDirectory::Root
                && process.execution_mode() == a3_domain::ProcessExecutionMode::KnownSafe
                && process.network() == a3_application::AgentApprovalNetworkScope::Denied
            {
                Ok(())
            } else {
                Err("combined fixture rejected a process outside the closed unittest scope".into())
            }
        }
    }
}

fn run_combined_locked_tests(
    root: &std::path::Path,
    test_root: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let mut command = std::process::Command::new("python");
    command.args(["-B", "-m", "unittest", "discover"]);
    if let Some(test_root) = test_root {
        command.args(["-s", test_root]);
    }
    command.current_dir(root);
    run_combined_check(&mut command)
}

const COMBINED_HTTP_ORACLE: &str = r#"
import http.client
import http.server
import importlib.util
import pathlib
import sys
import threading

root = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("a3_greenfield_server", root / "server.py")
if spec is None or spec.loader is None:
    raise SystemExit(2)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
handlers = [
    value for value in vars(module).values()
    if isinstance(value, type)
    and value is not http.server.BaseHTTPRequestHandler
    and issubclass(value, http.server.BaseHTTPRequestHandler)
]
if not handlers:
    raise SystemExit(3)
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handlers[0])
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    port = server.server_address[1]
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    connection.request("GET", "/")
    response = connection.getresponse()
    body = response.read().decode("utf-8")
    assert response.status == 200
    assert "text/html" in response.getheader("Content-Type", "").lower()
    assert "hello" in body.lower() and "world" in body.lower()
    connection.close()

    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    connection.request("GET", "/a3-missing")
    response = connection.getresponse()
    response.read()
    assert response.status == 404
    connection.close()

    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    connection.request("POST", "/")
    response = connection.getresponse()
    response.read()
    assert response.status == 405
    assert response.getheader("Allow") == "GET"
    connection.close()
finally:
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)
    assert not thread.is_alive()
"#;

fn run_combined_http_oracle(root: &std::path::Path) -> Result<bool, Box<dyn Error>> {
    run_combined_check(
        std::process::Command::new("python")
            .args(["-I", "-B", "-c", COMBINED_HTTP_ORACLE])
            .arg(root)
            .current_dir(root),
    )
}

fn run_combined_check(command: &mut std::process::Command) -> Result<bool, Box<dyn Error>> {
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {}
            Err(error) => {
                let _killed = child.kill();
                let _joined = child.wait();
                return Err(error.into());
            }
        }
        if started.elapsed() >= Duration::from_secs(30) {
            let _killed = child.kill();
            let _joined = child.wait();
            return Err("combined physical check timed out".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "Requires explicit approval for the configured provider and A3_CONFIGURED_RESEARCH_CATALOG"]
fn configured_model_empty_project_research_handoff_completes_verified_project()
-> Result<(), Box<dyn Error>> {
    if std::env::var_os("A3_CONFIGURED_RESEARCH_CATALOG").is_none() {
        return Err("configured catalog opt-in missing".into());
    }
    support::run_libsql_test_selected(
        async {
            let repository = support::TempDirectory::new()?;
            repository.git(["init", "--initial-branch=main"])?;
            let project = RepositoryInspector::new().inspect(repository.path())?;
            let data = support::TempDirectory::new()?;
            let store = Arc::new(
                LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("data"))?)
                    .await?,
            );
            store.record_opened_project(&project).await?;
            RefreshRepositoryIndex::new(
                Arc::new(Blake3RepositorySnapshotBuilder::new()),
                store.clone(),
                Arc::new(Blake3IndexRunIdFactory),
            )
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?,
                &FixtureControl,
            )
            .await?;
            let live = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(live_fixture::LiveResearchModel::probe())?;
            let (provider, profile) = live.execution_parts();
            let mode = AgentSessionMode::Agent;
            let budget = live.evidence_budget(mode)?;
            let model = Arc::new(CoherentModel {
                live: Some(live),
                fault: WorkFault::None,
                work_contract: true,
                budget,
                calls: AtomicUsize::new(0),
                diagrams: AtomicUsize::new(0),
                truncated_packet: std::sync::Mutex::new(None),
                command_packet: std::sync::Mutex::new(None),
                oversized_transcript: std::sync::Mutex::new(None),
            });
            let session_id = AgentSessionId::from_bytes([91; 32]);
            let time = timestamp()?;
            let objective = "erstelle einen kleinen python server mit einer hello world webseite";
            let session = AgentSession::from_parts(
                session_id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("Full greenfield handoff".to_owned())?,
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
                session_id,
                AgentSessionSequence::FIRST,
                AgentSessionEntryKind::UserMessage,
                AgentSessionText::try_from_string(objective.to_owned())?,
                time,
                None,
                None,
                None,
            )?;
            store
                .create_session(&project, &session, Some(&user), None)
                .await?;
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let worker_model = Arc::clone(&model);
            let worker_project = project.clone();
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            recovery_contract::owned_with_timeout(Duration::from_secs(420), move |control, _| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                send.send(runtime.block_on(researcher.research(
                    worker_model.as_ref(),
                    &worker_project,
                    session_id,
                    AgentSessionSequence::FIRST,
                    mode,
                    AgentResearchDepth::Standard,
                    objective,
                    &[(ModelMessageRole::User, objective.to_owned())],
                    None,
                    &control,
                )))?;
                Ok(())
            })?;
            let result = receive.recv_timeout(Duration::from_secs(1))??;
            assert!(!result.awaiting_continuation);
            assert!(result.has_plan_grounding());

            let materializer = AgentTaskMaterializer::new(
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
            );
            let task = materializer
                .materialize(AgentTaskMaterialization {
                    project: &project,
                    objective,
                    reviewed_plan: &result.markdown,
                    profile: profile.clone(),
                    research_handoff: Some(&result.handoff),
                    verification_profile: None,
                    control: &FixtureControl,
                })
                .await?;
            let task_id = task.work_item.task_id();
            let plan_sequence = AgentSessionSequence::FIRST.next()?;
            let completed_at = timestamp()?;
            let published_session = successor(
                &session,
                SessionSuccessor {
                    title: session.title().as_str().to_owned(),
                    mode,
                    state: cited_plan_halt(mode),
                    updated_at: completed_at,
                    latest_sequence: Some(plan_sequence),
                    active_work_item: Some(task.work_item),
                    plan_revision: Some(1),
                    presentation_deleted: false,
                },
            )?;
            let plan_entry = AgentSessionEntry::try_new(
                session_id,
                plan_sequence,
                AgentSessionEntryKind::Plan,
                AgentSessionText::try_from_string(result.markdown.clone())?,
                completed_at,
                Some(task.work_item.id()),
                Some(task_id),
                Some(1),
            )?;
            store
                .complete_turn(
                    &project,
                    session.revision(),
                    &published_session,
                    &plan_entry,
                    &result.terminal_event,
                    &result.citations,
                    &result.diagrams,
                )
                .await?;
            let runtime = AgentConversationRuntime::new(
                store.clone(),
                Arc::new(a3_credentials::NativeProviderCredentialStore::new()),
            )
            .with_execution_override(provider, profile);
            let inspection = Arc::new(a3_application::AgentInspectionBuffer::new());
            inspection.activate_project(&project);
            let approval = Arc::new(a3_application::AgentApprovalBuffer::new());
            approval.activate_project(&project);
            let executor = Arc::new(crate::ProductionAgentRunExecutor::new(
                crate::ProductionAgentRunPorts {
                    workspace: store.clone(),
                    journal: store.clone(),
                    actions: store.clone(),
                    recovery: store.clone(),
                    policy: store.clone(),
                    evidence: store.clone(),
                    index: store.clone(),
                    lens_index: store.clone(),
                    search: store.clone(),
                    claims: store.clone(),
                    allowlist: store.clone(),
                    research: Some(store.clone()),
                },
                runtime,
                inspection,
                approval.clone(),
                None,
            )?);
            let query = a3_application::GetAgentApprovalCenter::new(
                store.clone(),
                store.clone(),
                store.clone(),
                approval.clone(),
            );
            let approve = a3_application::ControlAgentApproval::new(
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
                approval,
            );
            let mut request = task.request;
            let mut run_id = None;
            for attempt in 0..12_u8 {
                run_combined_agent_attempt(Arc::clone(&executor), project.clone(), request)?;
                let stored = store
                    .load_task_ledger(&project, task_id)
                    .await?
                    .ok_or("task ledger")?;
                if run_id.is_none() {
                    run_id = stored
                        .ledger()
                        .steps()
                        .filter_map(|step| step.attempts().last())
                        .map(a3_domain::TaskStepAttempt::run_id)
                        .next();
                }
                let current_run_id = run_id.ok_or("agent run id")?;
                let run = store
                    .load_agent_run(&project, current_run_id)
                    .await?
                    .ok_or("agent run")?;
                if run.state() == a3_domain::AgentControllerState::Done {
                    break;
                }
                if run.state() == a3_domain::AgentControllerState::Failed {
                    return Err("combined research-to-agent run reached Failed".into());
                }
                assert_eq!(run.state(), a3_domain::AgentControllerState::AwaitApproval);
                let observed_at = agent_timestamp(now_millis()?)?;
                let a3_application::AgentApprovalLoadResult::Available(center) = query
                    .execute(&project, task_id, observed_at, &CombinedWorkspaceControl)
                    .await?
                else {
                    return Err("combined run stopped without exact approval".into());
                };
                validate_combined_approval(center.presentation().action())?;
                assert!(center.can_allow_once());
                let approval_id = a3_domain::ApprovalId::from_bytes([100 + attempt; 32]);
                let result = approve
                    .execute(
                        &project,
                        task_id,
                        center.presentation().revision(),
                        center.ledger_revision(),
                        center.ledger_store_version(),
                        a3_application::AgentApprovalControlAction::AllowOnce,
                        a3_application::AgentApprovalControlMetadata::new(
                            approval_id,
                            a3_domain::RunEventId::from_bytes([120 + attempt; 32]),
                            observed_at,
                        ),
                        &CombinedWorkspaceControl,
                    )
                    .await?;
                assert!(matches!(
                    result,
                    a3_application::AgentApprovalControlResult::Applied(
                        a3_application::AgentApprovalControlOutcome::GrantStored { .. }
                    )
                ));
                let updated = store
                    .load_task_ledger(&project, task_id)
                    .await?
                    .ok_or("updated task ledger")?;
                request = a3_application::AgentRunExecutionRequest::after_approval(
                    task_id,
                    updated.ledger().revision(),
                    updated.version(),
                    approval_id,
                );
            }
            let current_run_id = run_id.ok_or("final agent run id")?;
            let run = store
                .load_agent_run(&project, current_run_id)
                .await?
                .ok_or("final agent run")?;
            let stored = store
                .load_task_ledger(&project, task_id)
                .await?
                .ok_or("final task ledger")?;
            assert_eq!(run.state(), a3_domain::AgentControllerState::Done);
            assert!(
                stored
                    .ledger()
                    .steps()
                    .filter(|step| step.is_active_plan_step())
                    .all(|step| {
                        step.status() == a3_domain::TaskStepStatus::Completed
                            && step
                                .attempts()
                                .last()
                                .and_then(a3_domain::TaskStepAttempt::verification)
                                .is_some_and(|verification| {
                                    verification.passed() && !verification.evidence_ids().is_empty()
                                })
                    })
            );
            assert!(repository.path().join("server.py").is_file());
            let test_root = if repository.path().join("tests/test_server.py").is_file() {
                Some("tests")
            } else if repository.path().join("test_server.py").is_file() {
                None
            } else {
                return Err("combined run did not create a discoverable test_server.py".into());
            };
            assert!(run_combined_locked_tests(repository.path(), test_root)?);
            assert!(run_combined_http_oracle(repository.path())?);
            Ok(())
        },
        true,
    )
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
fn research_core_test_design_repairs_unnecessary_confirmation_without_user_halt()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::TestConfirmationOnce)
}

#[test]
fn research_core_test_design_repeated_confirmation_cannot_complete_or_start_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::TestConfirmationAlways)
}

#[test]
fn research_v6_core_status_completes_all_modes_without_model_presentation()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::CoreStatus)
}

#[test]
fn research_v6_injected_status_gets_one_repair_without_extra_reads() -> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::CoreStatusRepairOnce)
}

#[test]
fn research_v7_disjoint_response_closes_all_modes_without_status() -> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::DisjointResponse)
}

#[test]
fn research_v7_disjoint_response_repairs_injected_status_once() -> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::DisjointResponseRepairOnce)
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
fn research_empty_navigation_status_preserves_all_modes_without_repair_or_extra_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::EmptyNavigationStatus)
}

#[test]
fn research_repeated_v5_status_sources_preserve_all_modes_without_repair_or_extra_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::RepeatedNoteSources)
}

#[test]
fn research_echoed_test_obligation_gets_one_repair_without_research_reads()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::EchoTestObligationOnce)
}

#[test]
fn research_repeated_test_obligation_cannot_complete_or_poison_the_checkpoint()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_selected(true, false, WorkFault::EchoTestObligationAlways)
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
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::None,
        QUERY,
        Some(&eight_k_profile()?),
    )
}

#[test]
fn research_eight_k_profile_preserves_long_design_before_tests_without_history_reservation()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::LongDesign,
        QUERY,
        Some(&eight_k_profile()?),
    )
}

fn eight_k_profile() -> Result<a3_domain::ModelProfile, Box<dyn Error>> {
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
    Ok(profile)
}

#[test]
fn research_eight_k_profile_preserves_fitting_interpretation_across_design_steps()
-> Result<(), Box<dyn Error>> {
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::LongInterpretation,
        QUERY,
        Some(&eight_k_profile()?),
    )
}

#[test]
fn research_literal_list_does_not_create_an_isolated_extra_question() -> Result<(), Box<dyn Error>>
{
    coherent_fixture_with_profile(
        true,
        false,
        WorkFault::None,
        &QUERY.replace("werden aufgerufen und", "werden aufgerufen, und"),
        Some(&eight_k_profile()?),
    )
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
                if (matches!(
                    fault,
                    WorkFault::EmptyDesignOnce
                        | WorkFault::EmptyDesignAlways
                        | WorkFault::MissingOriginalOnce
                        | WorkFault::LongDesign
                        | WorkFault::LongInterpretation
                        | WorkFault::OriginalDesignBasis
                        | WorkFault::OriginalDesignLimit
                        | WorkFault::EchoTestObligationOnce
                        | WorkFault::EchoTestObligationAlways
                        | WorkFault::TestConfirmationOnce
                        | WorkFault::TestConfirmationAlways
                ) || fault.command_rename().is_some())
                    && mode == AgentSessionMode::Ask
                {
                    continue;
                }
                if fault
                    .result_oversize()
                    .is_some_and(|(question, _)| question > 1)
                    && mode == AgentSessionMode::Ask
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
                    truncated_packet: std::sync::Mutex::new(None),
                    command_packet: std::sync::Mutex::new(None),
                    oversized_transcript: std::sync::Mutex::new(None),
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
                if let Some((failed_question, repeated)) = fault.result_oversize() {
                    assert_eq!(result.awaiting_continuation, repeated);
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        if repeated {
                            usize::from(failed_question)
                                + 1
                                + usize::from(mode == AgentSessionMode::Ask)
                        } else {
                            4
                        }
                    );
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert_eq!(work.ready_to_finish(), !repeated);
                    assert!(
                        work.accesses().is_empty(),
                        "no extra reads can repair an overlong result"
                    );
                    let reopened = LibsqlKnowledgeStore::open(&StorageLayout::prepare(
                        data.path().join("data"),
                    )?)
                    .await?;
                    assert_eq!(
                        reopened
                            .load_detail(&project, id, AgentSessionSequence::FIRST)
                            .await?
                            .ok_or("reopened")?
                            .work_state(),
                        Some(work)
                    );
                    if repeated {
                        let question = work
                            .question(a3_domain::ResearchQuestionId::new(u16::from(
                                failed_question,
                            ))?)
                            .ok_or("question")?;
                        assert!(question.result().is_none() && question.attempts().is_empty());
                    }
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if fault == WorkFault::OriginalDesignLimit {
                    assert!(result.awaiting_continuation);
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        2,
                        "no Q3 inference or repair without all original ranges"
                    );
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(!work.ready_to_finish());
                    assert!(work.accesses().is_empty());
                    let q3 = work
                        .question(a3_domain::ResearchQuestionId::new(3)?)
                        .ok_or("Q3")?;
                    assert!(q3.result().is_none() && q3.attempts().is_empty());
                    assert_eq!(
                        work.question(a3_domain::ResearchQuestionId::new(2)?)
                            .ok_or("Q2")?
                            .result()
                            .ok_or("design")?
                            .text(),
                        retained_long_design()
                    );
                    let reopened = LibsqlKnowledgeStore::open(&StorageLayout::prepare(
                        data.path().join("data"),
                    )?)
                    .await?;
                    assert_eq!(
                        reopened
                            .load_detail(&project, id, AgentSessionSequence::FIRST)
                            .await?
                            .ok_or("reopened")?
                            .work_state(),
                        Some(work)
                    );
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if let Some((failed_question, repeated)) = fault.command_rename() {
                    assert_eq!(result.awaiting_continuation, repeated);
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        if repeated {
                            usize::from(failed_question) + 1
                        } else {
                            4
                        }
                    );
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    let reopened = LibsqlKnowledgeStore::open(&StorageLayout::prepare(
                        data.path().join("data"),
                    )?)
                    .await?;
                    let reopened_detail = reopened
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("reopened trace")?;
                    assert_eq!(
                        reopened_detail.work_state(),
                        Some(work),
                        "new storage adapter must load the same durable contract/results/receipts"
                    );
                    assert_eq!(work.ready_to_finish(), !repeated);
                    assert!(work.accesses().is_empty());
                    assert_eq!(work.objective(), fixture_query);
                    if repeated {
                        let failed = &work.questions()[usize::from(failed_question) - 1];
                        assert!(failed.result().is_none());
                        assert!(failed.attempts().is_empty());
                        let mapping = work
                            .questions()
                            .iter()
                            .filter_map(|q| q.result())
                            .flat_map(|r| r.sources())
                            .map(|s| (s.source_id, s.source_id))
                            .collect::<Vec<_>>();
                        let mut restored = AskResearchWorkingSet::new(budget);
                        restored.restore_work(work, &mapping)?;
                        assert_eq!(
                            restored.work.as_ref().and_then(|w| w.next_question()),
                            Some(a3_domain::ResearchQuestionId::new(u16::from(
                                failed_question
                            ))?)
                        );
                    } else {
                        for question in &work.questions()[1..] {
                            assert!(
                                question
                                    .result()
                                    .ok_or("design")?
                                    .text()
                                    .contains("export-events")
                            );
                        }
                        assert_eq!(result.markdown.matches("export-events").count(), 2);
                    }
                    for (path, expected) in PATHS.iter().zip(&files) {
                        assert_eq!(
                            std::fs::read_to_string(repository.path().join(path))?,
                            *expected
                        );
                    }
                    continue;
                }
                if matches!(
                    fault,
                    WorkFault::CoreStatus
                        | WorkFault::CoreStatusRepairOnce
                        | WorkFault::DisjointResponse
                        | WorkFault::DisjointResponseRepairOnce
                        | WorkFault::TruncatedAnalysisOnce
                ) {
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(work.ready_to_finish());
                    assert!(work.accesses().is_empty());
                    assert!(
                        detail
                            .events()
                            .iter()
                            .all(|event| event.public_note().is_none()),
                        "Core status must survive persistence as audit, not a reusable model finding"
                    );
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        if matches!(
                            fault,
                            WorkFault::CoreStatusRepairOnce
                                | WorkFault::DisjointResponseRepairOnce
                                | WorkFault::TruncatedAnalysisOnce
                        ) {
                            4
                        } else {
                            3
                        }
                    );
                }
                if matches!(
                    fault,
                    WorkFault::EchoTestObligationOnce
                        | WorkFault::EchoTestObligationAlways
                        | WorkFault::TestConfirmationOnce
                        | WorkFault::TestConfirmationAlways
                ) {
                    let failed = matches!(
                        fault,
                        WorkFault::EchoTestObligationAlways | WorkFault::TestConfirmationAlways
                    );
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        4,
                        "exactly one repair of Q3"
                    );
                    assert_eq!(result.awaiting_continuation, failed);
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert_eq!(work.ready_to_finish(), !failed);
                    assert_eq!(work.resolved_count(), if failed { 2 } else { 3 });
                    assert!(work.accesses().is_empty());
                    if failed {
                        assert!(work.questions()[2].attempts().is_empty());
                        assert!(work.questions()[2].result().is_none());
                        let mut restored = AskResearchWorkingSet::new(budget);
                        restored.restore_work(work, &[])?;
                        assert_eq!(
                            restored.work.as_ref().and_then(|w| w.next_question()),
                            Some(a3_domain::ResearchQuestionId::FIRST),
                            "without rehydrated originals, source-dependent prerequisites reopen"
                        );
                        let mapping = work
                            .questions()
                            .iter()
                            .filter_map(|q| q.result())
                            .flat_map(|r| r.sources())
                            .map(|s| (s.source_id, s.source_id))
                            .collect::<Vec<_>>();
                        restored.restore_work(work, &mapping)?;
                        assert_eq!(
                            restored.work.as_ref().and_then(|w| w.next_question()),
                            Some(a3_domain::ResearchQuestionId::new(3)?)
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
                if fault == WorkFault::LongDesign {
                    assert!(
                        !result.awaiting_continuation,
                        "mandatory design fits the actual model window; optional dialogue cannot block Q3"
                    );
                    assert_eq!(model.calls.load(Ordering::SeqCst), 3);
                    let detail = store
                        .load_detail(&project, id, AgentSessionSequence::FIRST)
                        .await?
                        .ok_or("trace")?;
                    let work = detail.work_state().ok_or("work")?;
                    assert!(work.ready_to_finish());
                    assert!(work.accesses().is_empty());
                    let rendered_design = retained_long_design()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert!(result.markdown.contains(&rendered_design));
                }
                if matches!(
                    fault,
                    WorkFault::RepeatedOriginalAnchors
                        | WorkFault::LongInterpretation
                        | WorkFault::OriginalDesignBasis
                        | WorkFault::EmptyNavigationStatus
                        | WorkFault::RepeatedNoteSources
                ) {
                    assert!(
                        !result.awaiting_continuation,
                        "valid anchors and neutral status hints cannot block research"
                    );
                    assert_eq!(
                        model.calls.load(Ordering::SeqCst),
                        3,
                        "no presentation-only repair"
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
                if matches!(
                    fault,
                    WorkFault::InvalidAnalysis | WorkFault::TruncatedAnalysisAlways
                ) {
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

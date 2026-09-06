//! Explicit live evaluation; the actual researcher, storage, index and reader are unchanged.
use super::*;
use a3_application::AskResearchDecision;

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
const QUESTIONS: [[&str; 3]; 4] = [
    [
        "Erkläre die Umschaltung zwischen JSON und SQLite in main.py, taskflow/manager.py und taskflow/storage.py: Priorität von CLI und Umgebung, Default, Dateinamen und ungültige Werte.",
        "Wie entscheidet dieses Projekt zwischen SQLite und JSON? Verfolge main.py über taskflow/manager.py bis taskflow/storage.py. Nenne Auswahlpriorität, Umgebungsvariable, Standarddateien und Fehlerfall.",
        "Untersuche main.py, taskflow/storage.py und taskflow/manager.py: Welcher Storage wird wann genommen, welche Einstellung gewinnt, wie heißen die Dateien und was passiert bei unbekanntem Backend?",
    ],
    [
        "Verfolge add_task in taskflow/manager.py bis zum tatsächlichen Audit-Schreibvorgang in taskflow/plugins.py. Nenne alle Methoden der Kette, Reihenfolge, Logdatei, Pfadauflösung relativ zum Arbeitsverzeichnis und Schreibmodus.",
        "Wie kommt eine neue Aufgabe aus taskflow/manager.py in das Audit-Log aus taskflow/plugins.py? Erkläre Aufrufer, Dispatcher, Callback, Writer, Default-Dateiname, Arbeitsverzeichnisbezug und Append-Verhalten.",
        "Erkläre die vollständige Audit-Aufrufkette beim Erstellen einer Aufgabe anhand taskflow/manager.py und taskflow/plugins.py, einschließlich Writer, Dateiname, absoluter Pfadbildung aus dem Arbeitsverzeichnis und Anhängen.",
    ],
    [
        "Verfolge GET /tasks/99 in taskflow/api.py und taskflow/manager.py vom Router über den Handler zum Manager. Wo entsteht KeyError, wie wird daraus 404, und was passiert beim gefundenen Task oder unbekannter Route?",
        "Erkläre mit taskflow/api.py und taskflow/manager.py den REST-Pfad: dispatch, get_task_response und get_task. Vergleiche vorhandenen Task (200), fehlenden Task (KeyError/404) und unbekannte Route (404).",
        "Wie werden in taskflow/api.py und taskflow/manager.py eine Aufgabenabfrage, eine fehlende Aufgabe und eine fehlende Route behandelt? Zeige die Methodenfolge, die KeyError-Umwandlung und Status 200/404.",
    ],
    [
        "Plane python main.py import-csv <filepath> für dieses Projekt (main.py, taskflow/manager.py). UTF-8-CSV mit project_id,title über csv.DictReader; jede gültige Zeile soll Manager.add_task nutzen. Definiere Fehlerbehandlung und konkrete Tests. Implementiere noch nichts.",
        "Erstelle einen umsetzbaren Plan für den neuen CLI-Befehl import-csv in main.py mit taskflow/manager.py: UTF-8, csv.DictReader, project_id und title, vorhandenes add_task, verständliche Fehler und Regressionstests. Keine Umsetzung.",
        "Untersuche main.py und taskflow/manager.py und plane einen CSV-Import per python main.py import-csv <filepath>. Nutze UTF-8, DictReader, project_id/title und add_task; lege Validierung, Fehlerverhalten und Tests fest.",
    ],
];

fn missing_concepts(family: usize, answer: &str) -> Vec<&'static str> {
    let lower = answer.to_lowercase();
    let groups: &[&[&str]] = match family {
        0 => &[
            &["json"],
            &["sqlite"],
            &["tasks.db"],
            &["tasks.json"],
            &["taskflow_storage"],
            &["backend"],
            &["valueerror", "ungültig", "unbekannt", "unsupported"],
        ],
        1 => &[
            &["add_task"],
            &["trigger_task_created"],
            &["on_task_created"],
            &["_log"],
            &["write"],
            &["audit_log.txt"],
            &["abspath"],
            &["arbeitsverzeichnis", "working directory", "cwd"],
            &["append", "anhäng", "anhang", "'a'"],
        ],
        2 => &[
            &["dispatch"],
            &["get_task_response"],
            &["get_task"],
            &["keyerror"],
            &["404"],
            &["200"],
        ],
        _ => &[
            &["plan:"],
            &["import-csv"],
            &["dictreader"],
            &["project_id"],
            &["title"],
            &["add_task"],
            &["utf-8", "utf8"],
            &["test"],
            &["fehler", "invalid", "ungültig"],
        ],
    };
    groups
        .iter()
        .filter(|alternatives| !alternatives.iter().any(|s| lower.contains(s)))
        .map(|alternatives| alternatives[0])
        .collect()
}

struct MatrixModel {
    live: live_fixture::LiveResearchModel,
    budget: usize,
    calls: AtomicUsize,
    bytes: AtomicUsize,
    empty_analysis_notes: std::sync::Mutex<Vec<serde_json::Value>>,
    decisions: std::sync::Mutex<Vec<serde_json::Value>>,
}

// Only identities of this public fixture and numeric shape data, never original text,
// free-form model fields, arbitrary paths or provider payloads. Not a fact validator.
fn decision_diagnostic(
    phase: a3_application::ResearchOutputPhase,
    transcript: &[(ModelMessageRole, String)],
    output: &str,
) -> serde_json::Value {
    let mut fingerprint = blake3::Hasher::new();
    for (role, body) in transcript {
        fingerprint.update(format!("{role:?}:{}:", body.len()).as_bytes());
        fingerprint.update(body.as_bytes());
    }
    let packet = transcript
        .iter()
        .rev()
        .find_map(|(_, body)| body.starts_with("CURRENT QUESTION:\n").then_some(body));
    let delivered = packet
        .into_iter()
        .flat_map(|body| body.lines())
        .filter_map(|line| {
            let (_, rest) = line.strip_prefix("[S")?.split_once("] ")?;
            let (path, _) = rest.split_once(" ab Zeile ")?;
            let file = FILES.iter().position(|(known, _)| *known == path)?;
            let (_, label) = line.rsplit_once(" [E")?;
            let label = label.strip_suffix(']')?;
            let anchor = label.parse::<u16>().ok()?;
            ((1..=8).contains(&anchor) && label == anchor.to_string())
                .then(|| serde_json::json!({"file":file,"anchor":anchor}))
        })
        .take(8)
        .collect::<Vec<_>>();
    let decoded = a3_application::DecodeAskResearchDecision.decode_phase(output, phase);
    let results = decoded
        .as_ref()
        .ok()
        .and_then(|decision| {
            let note = match decision {
                AskResearchDecision::Answer { note, .. }
                | AskResearchDecision::Research { note, .. } => note,
            };
            note.work.as_ref()
        })
        .map(|work| {
            work.results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "question":r.question_id.get(),
                        "anchors":r.anchors.iter().map(|a| a.get()).collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>()
        });
    let shape = serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .map(|document| invalid_shape_summary(&document));
    serde_json::json!({
        "phase":format!("{phase:?}"),
        "transcript_blake3":fingerprint.finalize().to_hex().to_string(),
        "delivered":delivered,
        "decoded":decoded.is_ok(),
        "results":results,
        "shape":shape
    })
}

#[test]
fn research_matrix_diagnostics_bind_numeric_anchors_without_source_or_model_text() {
    let phase = a3_application::ResearchOutputPhase::Analyze(a3_domain::ResearchQuestionId::FIRST);
    let packet = "CURRENT QUESTION:\nprivate-looking sentinel\n[S1] main.py ab Zeile 1 (Spalte 0) [E1]\nsource sentinel\n[S2] taskflow/manager.py ab Zeile 1 (Spalte 0) [E2]\n[S3] private-looking/path.py ab Zeile 1 (Spalte 0) [E3]";
    let transcript = vec![(ModelMessageRole::User, packet.to_owned())];
    let output = serde_json::json!({"schema_version":5,"decision":{"kind":"progress","note":{"goal":"private-looking sentinel","finding_kind":"hypothesis","finding":"output sentinel","finding_source_refs":[],"gap":"","next_step":""}},"work":{"questions":[],"results":[{"question_id":1,"kind":"interpretation","text":"model sentinel","evidence":[{"anchor_ref":"E2"}]}]}}).to_string();
    let diagnostic = decision_diagnostic(phase, &transcript, &output);
    assert_eq!(
        diagnostic["delivered"],
        serde_json::json!([{"file":0,"anchor":1},{"file":1,"anchor":2}])
    );
    assert_eq!(
        diagnostic["results"],
        serde_json::json!([{"question":1,"anchors":[2]}])
    );
    assert_eq!(diagnostic["decoded"], true);
    assert!(!diagnostic.to_string().contains("sentinel"));
    assert!(!diagnostic.to_string().contains("path.py"));
    assert_eq!(diagnostic, decision_diagnostic(phase, &transcript, &output));
    let changed = vec![(ModelMessageRole::User, format!("{packet}\nnew bytes"))];
    assert_ne!(
        diagnostic["transcript_blake3"],
        decision_diagnostic(phase, &changed, &output)["transcript_blake3"]
    );
    let invalid = decision_diagnostic(phase, &transcript, "invalid sentinel");
    assert_eq!(invalid["decoded"], false);
    assert!(invalid["results"].is_null());
    assert!(!invalid.to_string().contains("sentinel"));
}

// Fixed-shape diagnostics for the public fixture, never raw model/provider content.
fn invalid_shape_summary(document: &serde_json::Value) -> serde_json::Value {
    let note = &document["decision"]["note"];
    let refs = note["finding_source_refs"].as_array();
    let canonical_ref = |value: &serde_json::Value, prefix: char, max: u16| {
        value.as_str().is_some_and(|text| {
            text.strip_prefix(prefix)
                .and_then(|n| n.parse::<u16>().ok())
                .is_some_and(|n| n > 0 && n <= max && text == format!("{prefix}{n}"))
        })
    };
    let results = document["work"]["results"]
        .as_array()
        .map(|items| {
            items.iter().take(2).map(|r| {
                let anchors = r["evidence"].as_array();
                serde_json::json!({
                    "question_id":r["question_id"].as_u64(),
                    "interpretation":r["kind"] == "interpretation",
                    "design":r["kind"] == "designDecision",
                    "text_bytes":r["text"].as_str().map(str::len),
                    "anchors_canonical":anchors.is_some_and(|a| a.iter().all(|e| canonical_ref(&e["anchor_ref"], 'E', 8))),
                    "anchors_duplicate":anchors.is_some_and(|a| a.iter().enumerate().any(|(i,e)| a[..i].contains(e)))
                })
            }).collect::<Vec<_>>()
        });
    serde_json::json!({
        "note_bytes":(["goal","finding","gap","next_step"].map(|key| note[key].as_str().map(|s| s.trim().len()))),
        "note_kind_valid":matches!(note["finding_kind"].as_str(), Some("observation" | "hypothesis" | "conclusion")),
        "note_refs_canonical":refs.is_some_and(|r| r.iter().all(|v| canonical_ref(v, 'S', 200))),
        "note_refs_duplicate":refs.is_some_and(|r| r.iter().enumerate().any(|(i,v)| r[..i].contains(v))),
        "results":results
    })
}

#[test]
fn research_matrix_shape_diagnostics_distinguish_repeated_anchors_without_raw_text() {
    let document = serde_json::json!({"decision":{"note":{"goal":"fixture sentinel","finding_kind":"hypothesis","finding":"visible finding","gap":" ","next_step":"next","finding_source_refs":["E1"]}},"work":{"results":[{"question_id":1,"kind":"interpretation","text":"private-looking sentinel","evidence":[{"anchor_ref":"E1"},{"anchor_ref":"E1"}]}]}});
    let summary = invalid_shape_summary(&document);
    assert_eq!(summary["note_bytes"][2], 0);
    assert_eq!(summary["note_refs_canonical"], false);
    assert_eq!(summary["results"][0]["anchors_duplicate"], true);
    assert!(!summary.to_string().contains("sentinel"));
}
impl ResearchModel for MatrixModel {
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
        mode: AgentSessionMode,
        search: bool,
        phase: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        control: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.bytes.fetch_add(
            transcript.iter().map(|(_, s)| s.len()).sum(),
            Ordering::SeqCst,
        );
        let output = self
            .live
            .complete(mode, search, phase, transcript, control)
            .await?;
        {
            let mut decisions = self
                .decisions
                .lock()
                .map_err(|_| AgentConversationFailure::Unavailable)?;
            if decisions.len() < 24 {
                decisions.push(decision_diagnostic(phase, transcript, &output));
            }
        }
        if matches!(
            phase,
            a3_application::ResearchOutputPhase::Analyze(_)
                | a3_application::ResearchOutputPhase::SummarizeOriginals(_)
                | a3_application::ResearchOutputPhase::Design(_)
        ) && let Ok(document) = serde_json::from_str::<serde_json::Value>(&output)
            && document["work"]["results"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            let mut notes = self
                .empty_analysis_notes
                .lock()
                .map_err(|_| AgentConversationFailure::Unavailable)?;
            if notes.len() < 24 {
                // Only bounded public status fields of this synthetic fixture. Never
                // capture hidden reasoning, raw provider payloads or user-project text.
                let note = &document["decision"]["note"];
                notes.push(serde_json::json!({"phase":format!("{phase:?}"),
                    "finding":super::super::utf8_prefix(note["finding"].as_str().unwrap_or_default(),512),
                    "gap":super::super::utf8_prefix(note["gap"].as_str().unwrap_or_default(),512),
                    "next_step":super::super::utf8_prefix(note["next_step"].as_str().unwrap_or_default(),512)}));
            }
        }
        if let Err(issue) = a3_application::DecodeAskResearchDecision.decode_phase(&output, phase) {
            let shape = serde_json::from_str::<serde_json::Value>(&output).ok();
            if let Some(document) = &shape {
                println!("A3_EVAL_INVALID_SHAPE {}", invalid_shape_summary(document));
            }
            println!(
                "A3_EVAL_DECODE phase={phase:?} issue={issue:?} bytes={} decision={} questions={}",
                output.len(),
                shape.as_ref().map_or_else(
                    || "invalid-json".to_owned(),
                    |s| s["decision"]["kind"].to_string()
                ),
                shape
                    .as_ref()
                    .and_then(|s| s["work"]["questions"].as_array())
                    .map_or(0, Vec::len)
            );
        }
        Ok(output)
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
fn research_matrix_rubric_rejects_missing_destinations_callbacks_and_plan_requirements() {
    assert!(
        !missing_concepts(1, "add_task calls trigger_task_created and on_task_created").is_empty()
    );
    assert!(!missing_concepts(0, "sqlite or json").is_empty());
    assert!(!missing_concepts(3, "PLAN: add a CSV command").is_empty());
    assert!(
        missing_concepts(
            2,
            "dispatch calls get_task_response and get_task; KeyError becomes 404, success 200"
        )
        .is_empty()
    );
}

#[test]
#[ignore = "Explicit approved-model evaluation only; never CI or an automatic provider call"]
fn research_approved_model_matrix() -> Result<(), Box<dyn Error>> {
    let repeats = std::env::var("A3_RESEARCH_EVAL_REPETITIONS")
        .unwrap_or_else(|_| "1".to_owned())
        .parse::<usize>()?;
    if !(1..=5).contains(&repeats) {
        return Err("repetition count must be 1..5".into());
    }
    support::run_libsql_test_selected(
        async {
            let repository = support::TempDirectory::new()?;
            repository.git(["init", "--initial-branch=main"])?;
            for (path, body) in FILES {
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
            let live = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(live_fixture::LiveResearchModel::probe())?;
            let mut failed = 0;
            use std::io::Write;
            let reports = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/research-eval");
            std::fs::create_dir_all(&reports)?;
            let report_path = reports.join(format!(
                "eval-{}.jsonl",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
            ));
            let mut report = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&report_path)?;
            println!("A3_EVAL_REPORT {}", report_path.canonicalize()?.display());
            let selected = std::env::var("A3_RESEARCH_EVAL_CASE").ok();
            let mut total = 0;
            for (family, questions) in QUESTIONS.iter().enumerate() {
                for (variant, question) in questions.iter().enumerate() {
                    if selected
                        .as_ref()
                        .is_some_and(|s| s != &format!("{family}:{variant}"))
                    {
                        continue;
                    }
                    for repeat in 0..repeats {
                        let mode = if family == 3 {
                            AgentSessionMode::Plan
                        } else {
                            AgentSessionMode::Ask
                        };
                        let id = AgentSessionId::from_bytes(
                            [u8::try_from(family * 15 + variant * 5 + repeat + 1)?; 32],
                        );
                        let time = timestamp()?;
                        let session = AgentSession::from_parts(
                            id,
                            AgentSessionRevision::new(1)?,
                            AgentSessionTitle::try_from_string("Research evaluation".to_owned())?,
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
                            AgentSessionText::try_from_string((*question).to_owned())?,
                            time,
                            None,
                            None,
                            None,
                        )?;
                        store
                            .create_session(&project, &session, Some(&user), None)
                            .await?;
                        let model = Arc::new(MatrixModel {
                            budget: live.evidence_budget(mode)?,
                            live: live.clone(),
                            calls: AtomicUsize::new(0),
                            bytes: AtomicUsize::new(0),
                            empty_analysis_notes: std::sync::Mutex::new(Vec::new()),
                            decisions: std::sync::Mutex::new(Vec::new()),
                        });
                        let researcher = AgentAskResearcher::new(
                            store.clone(),
                            store.clone(),
                            store.clone(),
                            store.clone(),
                        );
                        let worker_model = model.clone();
                        let worker_project = project.clone();
                        let query = (*question).to_owned();
                        let (send, receive) = std::sync::mpsc::sync_channel(1);
                        let started = Instant::now();
                        let job = recovery_contract::owned_with_timeout(
                            Duration::from_secs(420),
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
                                    None,
                                    &control,
                                )))?;
                                Ok(())
                            },
                        );
                        let result = if job.is_ok() {
                            receive.recv_timeout(Duration::from_secs(1)).ok()
                        } else {
                            None
                        };
                        let (completed, answer, error) = match result {
                            Some(Ok(result)) => {
                                (!result.awaiting_continuation, result.markdown, None)
                            }
                            Some(Err(e)) => (false, String::new(), Some(format!("{e:?}"))),
                            None => (false, String::new(), Some("owned_job_failed".to_owned())),
                        };
                        let missing = missing_concepts(family, &answer);
                        let passed = completed && missing.is_empty();
                        let detail = store
                            .load_detail(&project, id, AgentSessionSequence::FIRST)
                            .await?;
                        let adaptive_reads =
                            detail.as_ref().and_then(|d| d.work_state()).map(|w| {
                                w.accesses()
                                    .iter()
                                    .map(|a| u64::from(a.starts))
                                    .sum::<u64>()
                            });
                        let repeated_adaptive_reads =
                            detail.as_ref().and_then(|d| d.work_state()).map(|w| {
                                w.accesses()
                                    .iter()
                                    .map(|a| u64::from(a.starts.saturating_sub(1)))
                                    .sum::<u64>()
                            });
                        let user_halt = !completed || answer.trim_start().starts_with("QUESTION:");
                        let work_summary = detail.as_ref().and_then(|d| d.work_state()).map(|work| {
                            serde_json::json!({
                                "questions":work.questions().iter().map(|q| serde_json::json!({
                                    "id":q.id().get(), "kind":format!("{:?}",q.definition().kind),
                                    "outcome":q.definition().outcome, "status":format!("{:?}",q.status()),
                                    "packets":q.attempts().len()
                                })).collect::<Vec<_>>(),
                                "accesses":work.accesses().iter().map(|a| serde_json::json!({
                                    "question":a.question.get(),"kind":format!("{:?}",a.kind),
                                    "outcome":format!("{:?}",a.outcome),"starts":a.starts
                                })).collect::<Vec<_>>()
                            })
                        });
                        total += 1;
                        if !passed {
                            failed += 1;
                            if let Some(detail) = store
                                .load_detail(&project, id, AgentSessionSequence::FIRST)
                                .await?
                            {
                                for event in detail.events() {
                                    if event.action().starts_with("research-v") {
                                        println!("A3_EVAL_DIAGNOSTIC {}", event.action());
                                    }
                                    if let Some(query) =
                                        event.query().filter(|q| q.starts_with("research-v"))
                                    {
                                        println!("A3_EVAL_DIAGNOSTIC {query}");
                                    }
                                }
                                if let Some(work) = detail.work_state() {
                                    for question in work.questions() {
                                        println!(
                                            "A3_EVAL_OBLIGATION Q{} {:?} {:?}: {}",
                                            question.id().get(),
                                            question.definition().kind,
                                            question.status(),
                                            question.definition().outcome
                                        );
                                    }
                                }
                            }
                        }
                        let empty_notes = model
                            .empty_analysis_notes
                            .lock()
                            .map_err(|_| "fixture notes poisoned")?
                            .clone();
                        let decisions = model
                            .decisions
                            .lock()
                            .map_err(|_| "fixture diagnostics poisoned")?
                            .clone();
                        let record = serde_json::json!({"fixture":"research-eval-v1","family":family,"variant":variant,"repeat":repeat,"completed":completed,"passed":passed,"missing":missing,"error":error,"calls":model.calls.load(Ordering::SeqCst),"adaptive_reads":adaptive_reads,"repeated_adaptive_reads":repeated_adaptive_reads,"user_halt":user_halt,"context_utf8_bytes":model.bytes.load(Ordering::SeqCst),"elapsed_ms":started.elapsed().as_millis(),"answer":answer,"work_summary":work_summary,"empty_analysis_notes":empty_notes,"decision_diagnostics":decisions});
                        writeln!(report, "{record}")?;
                        report.flush()?;
                        let mut summary = record;
                        if let Some(object) = summary.as_object_mut() {
                            object.remove("answer");
                            object.remove("work_summary");
                            object.remove("empty_analysis_notes");
                            object.remove("decision_diagnostics");
                        }
                        println!("A3_EVAL {summary}");
                        for (path, body) in FILES {
                            assert_eq!(
                                std::fs::read_to_string(repository.path().join(path))?,
                                body
                            );
                        }
                    }
                }
            }
            if total == 0 {
                return Err("no evaluation case selected".into());
            }
            println!("A3_EVAL_SUMMARY total={total} failed={failed}");
            if failed > 0 {
                return Err(format!("{failed} evaluation cases require investigation").into());
            }
            Ok(())
        },
        true,
    )
}

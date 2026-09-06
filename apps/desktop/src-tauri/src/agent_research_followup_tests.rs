//! Real index, libSQL, filesystem readers and owned research loop; only model output is scripted.
use super::*;
use a3_application::{
    JobClock, JobEventKind, JobScheduler, JobSchedulerConfig, JobTimestamp, KnowledgeStore,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryRescanReason,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::index_test_support as support;

#[path = "agent_research_recovery_tests.rs"]
mod recovery_contract;

#[path = "agent_research_plan_tests.rs"]
mod plan_contract;

#[path = "agent_research_access_tests.rs"]
mod access_contract;
#[path = "agent_research_coherent_tests.rs"]
mod coherent_contract;

fn fixture_plan(summary: &str) -> String {
    format!(
        "PLAN:\n## Summary\n{summary}\n## Implementation Changes\n1. Die gewünschte Erweiterung über die bestehende Manager-API integrieren.\n## Interfaces\nNeue CSV-Spalten als vorgeschlagenen Vertrag dokumentieren.\n## Test Plan\n1. Gültige und fehlerhafte CSV-Zeilen sowie unveränderte Bestandsaufgaben prüfen.\n## Assumptions\nNeue CSV-Spalten werden entworfen, nicht als vorhandene Schnittstelle behauptet."
    )
}

const QUERY: &str =
    "Wie schalten taskflow/storage/factory.py und config.ini zwischen den Storage-Treibern um?";
const MANAGER: &str = "from .config import TaskFlowConfig\nfrom .storage.factory import get_storage\nclass TaskFlowManager:\n    def __init__(self):\n        self.config = TaskFlowConfig()\n        self.storage = get_storage(self.config.storage_type, self.config)\n";

const FLOW_QUERY: &str = "Erkläre die Aufgabenerstellung in taskflow/manager.py, taskflow/plugins/base.py und taskflow/plugins/audit_log_plugin.py.";
const FLOW_MANAGER: &str =
    include_str!("../../../../fixtures/research-progressive-v1/taskflow/manager.py");
const FLOW_BASE: &str =
    include_str!("../../../../fixtures/research-progressive-v1/taskflow/plugins/base.py");
const FLOW_AUDIT: &str = include_str!(
    "../../../../fixtures/research-progressive-v1/taskflow/plugins/audit_log_plugin.py"
);

struct ProgressiveModel {
    budget: usize,
    calls: AtomicUsize,
    diagrams: AtomicUsize,
    seen: std::sync::Mutex<BTreeMap<String, String>>,
    fault: ProgressiveFault,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProgressiveFault {
    IndependentRepairs,
    FailedRepair,
    NullRounds,
}
impl ResearchModel for ProgressiveModel {
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
        command: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(call < 12);
        if let Some(command) = command {
            assert!(command.contains("later separate phase"));
        }
        let packet = transcript
            .iter()
            .rev()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1
            .as_str();
        assert!(packet.len() <= self.budget);
        // Two unrelated malformed decisions: the first successful repair must not suppress
        // the second. Invalid actions are never salvaged from either document.
        if (self.fault == ProgressiveFault::IndependentRepairs && (call == 0 || call == 2))
            || (self.fault == ProgressiveFault::FailedRepair && call < 2)
        {
            return Ok("{invalid document".to_owned());
        }
        if self.fault == ProgressiveFault::NullRounds && call < 2 {
            return Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"research","evidence_status":"incomplete",
                "note":{"goal":"Aufgabenfluss","finding_kind":"hypothesis","finding":"Offen","finding_source_refs":[],"gap":"Unbekannt","next_step":"Suchen"},
                "actions":[{"kind":"searchIndex","query":"absent_fixture_identifier_undefined"}]}}).to_string());
        }
        let mut seen = self
            .seen
            .lock()
            .map_err(|_| AgentConversationFailure::Unavailable)?;
        for (path, body) in [
            ("taskflow/manager.py", "self.plugins.dispatch"),
            ("taskflow/plugins/base.py", "plugin.on_task_created"),
            (
                "taskflow/plugins/audit_log_plugin.py",
                "self.entries.append",
            ),
        ] {
            if packet.contains(body) {
                seen.insert(path.to_owned(), source_ref(packet, path)?);
            }
        }
        let missing = [
            "taskflow/manager.py",
            "taskflow/plugins/base.py",
            "taskflow/plugins/audit_log_plugin.py",
        ]
        .into_iter()
        .find(|path| !seen.contains_key(*path));
        let refs = seen.values().cloned().collect::<Vec<_>>();
        let note = serde_json::json!({"goal":"Aufgabenerstellung verstehen", "finding_kind":"hypothesis", "finding":"Quellen schrittweise prüfen", "finding_source_refs":[],
            "gap":if missing == Some("taskflow/manager.py") {"add_task"} else {"Nächste Datei"}, "next_step":"Aktuellen Abschnitt prüfen"});
        if let Some(path) = missing {
            assert!(
                search,
                "evidence chain must complete inside the normal research budget; call={call}, missing={path}"
            );
            Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"research","evidence_status":"incomplete","note":note,
                "actions":[{"kind":"inspectPath","path":path,"start_line":if path == "taskflow/manager.py" {6} else {1}}]}}).to_string())
        } else {
            let markers = refs
                .iter()
                .map(|source| format!("【{source}】"))
                .collect::<String>();
            let finding = format!(
                "add_task speichert und verteilt task_created; das Audit-Plugin erfasst die Aufgabe. {markers}"
            );
            let markdown = if mode == AgentSessionMode::Ask {
                finding
            } else {
                format!(
                    "PLAN:\n## Summary\n{finding}\n## Implementation Changes\n1. Den belegten Aufgabenfluss dokumentieren.\n## Interfaces\nManager und Plugin-Hooks bleiben unverändert.\n## Test Plan\n1. Die Dokumentation mit den aktuellen Aufrufstellen abgleichen.\n## Assumptions\nKeine Codeänderung erforderlich."
                )
            };
            Ok(serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"sufficient","note":note,
                "markdown":markdown,"source_refs":refs}}).to_string())
        }
    }
    async fn complete_evidence_diagrams(
        &self,
        transcript: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.diagrams.fetch_add(1, Ordering::SeqCst);
        assert!(call < 2);
        assert!(
            !transcript
                .iter()
                .any(|(_, text)| text.starts_with("REPAIR:"))
        );
        if call == 0 {
            return Err(AgentConversationFailure::OutputTruncated);
        }
        let seen = self
            .seen
            .lock()
            .map_err(|_| AgentConversationFailure::Unavailable)?;
        let manager = seen
            .get("taskflow/manager.py")
            .ok_or(AgentConversationFailure::InvalidInput)?;
        let audit = seen
            .get("taskflow/plugins/audit_log_plugin.py")
            .ok_or(AgentConversationFailure::InvalidInput)?;
        Ok(serde_json::json!({"schema_version":1,"diagrams":[{"type":"sequence","title":"Aufgabenerstellung","description":"Aktuelle Quellen",
            "elements":[{"id":"manager","label":"Manager","category":"function","source_refs":[manager]}, {"id":"audit","label":"Audit","category":"function","source_refs":[audit]}],
            "relationships":[{"from":"manager","to":"audit","label":"benachrichtigt über Plugins","source_refs":[manager,audit]}]}]}).to_string())
    }
}

#[test]
fn late_taskflow_flow_completes_in_all_modes_and_small_windows_with_independent_repairs()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, body) in [
            ("taskflow/manager.py", FLOW_MANAGER),
            ("taskflow/plugins/base.py", FLOW_BASE),
            ("taskflow/plugins/audit_log_plugin.py", FLOW_AUDIT),
            (
                "taskflow/storage/factory.py",
                include_str!(
                    "../../../../fixtures/research-storage-v1/taskflow/storage/factory.py"
                ),
            ),
            (
                "taskflow/config.py",
                include_str!("../../../../fixtures/research-storage-v1/taskflow/config.py"),
            ),
            (
                "config.ini",
                include_str!("../../../../fixtures/research-storage-v1/config.ini"),
            ),
        ] {
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
        for (budget_index, budget) in [1024, 2048, 4096, 8192].into_iter().enumerate() {
            for (mode_index, mode, diagram, fault) in [
                (
                    0,
                    AgentSessionMode::Ask,
                    false,
                    ProgressiveFault::IndependentRepairs,
                ),
                (
                    1,
                    AgentSessionMode::Plan,
                    false,
                    ProgressiveFault::IndependentRepairs,
                ),
                (
                    2,
                    AgentSessionMode::Agent,
                    false,
                    ProgressiveFault::IndependentRepairs,
                ),
                (
                    3,
                    AgentSessionMode::Ask,
                    true,
                    ProgressiveFault::IndependentRepairs,
                ),
                (
                    4,
                    AgentSessionMode::Ask,
                    false,
                    ProgressiveFault::FailedRepair,
                ),
                (
                    5,
                    AgentSessionMode::Ask,
                    false,
                    ProgressiveFault::NullRounds,
                ),
            ] {
                let index = u8::try_from(1 + budget_index * 6 + mode_index)?;
                let id = AgentSessionId::from_bytes([index; 32]);
                let time = timestamp()?;
                let query = if diagram {
                    format!("/diagram {FLOW_QUERY}")
                } else {
                    FLOW_QUERY.to_owned()
                };
                let session = AgentSession::from_parts(
                    id,
                    AgentSessionRevision::new(1)?,
                    AgentSessionTitle::try_from_string("Progressive flow regression".to_owned())?,
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
                    AgentSessionText::try_from_string(query.clone())?,
                    time,
                    None,
                    None,
                    None,
                )?;
                let command_profile = match parse_slash_command(mode, &query)? {
                    ParsedSlashCommand::Command(invocation) => {
                        Some(SlashCommandExecutionProfile::resolve(invocation))
                    }
                    ParsedSlashCommand::Plain(_) => None,
                };
                store
                    .create_session(
                        &project,
                        &session,
                        Some(&entry),
                        command_profile
                            .as_ref()
                            .map(SlashCommandExecutionProfile::invocation),
                    )
                    .await?;
                let (scheduler, events) =
                    JobScheduler::new(JobSchedulerConfig::new(1, 2, 128)?, Arc::new(FixtureClock))?;
                let model = Arc::new(ProgressiveModel {
                    budget,
                    calls: AtomicUsize::new(0),
                    diagrams: AtomicUsize::new(0),
                    seen: std::sync::Mutex::new(BTreeMap::new()),
                    fault,
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
                scheduler.submit(
                    JobId::new(u64::from(index)),
                    JobOwner::new(4),
                    move |control: JobContext| {
                        let result = tokio::runtime::Builder::new_current_thread()
                            .enable_time()
                            .build()
                            .map_err(|_| AgentSessionManagerFailure::Unavailable)
                            .and_then(|runtime| {
                                runtime.block_on(researcher.research(
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
                                ))
                            });
                        let success = result.is_ok();
                        let _sent = send.send(result);
                        if success {
                            JobCompletion::Succeeded
                        } else {
                            JobCompletion::Failed
                        }
                    },
                )?;
                loop {
                    let event = events
                        .next_timeout(Duration::from_secs(30))?
                        .ok_or("progressive research timeout")?;
                    if matches!(
                        event.kind(),
                        JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
                    ) {
                        break;
                    }
                }
                let result = receive.recv_timeout(Duration::from_secs(1))??;
                assert!(
                    !result.awaiting_continuation,
                    "budget={budget}, mode={mode:?}, diagram={diagram}: {}",
                    result.markdown
                );
                assert_eq!(result.citations.len(), 3);
                assert_eq!(
                    model.diagrams.load(Ordering::SeqCst),
                    if diagram { 2 } else { 0 }
                );
                assert_eq!(result.diagrams.len(), usize::from(diagram));
                let trace = store
                    .load_detail(&project, id, AgentSessionSequence::FIRST)
                    .await?
                    .ok_or("trace")?;
                assert_eq!(
                    trace
                        .events()
                        .iter()
                        .filter(|event| event.action().contains("research-v1/json"))
                        .count(),
                    if fault == ProgressiveFault::NullRounds {
                        0
                    } else {
                        2
                    }
                );
                assert_eq!(
                    trace
                        .events()
                        .iter()
                        .filter(|event| event.action().contains("research-v1/recovery-progress"))
                        .count(),
                    usize::from(fault != ProgressiveFault::IndependentRepairs)
                );
                let sources = store
                    .list_sources(&project, id, AgentSessionSequence::FIRST, None, 50)
                    .await?;
                assert!(!sources.has_more());
                assert!(!sources.sources().iter().any(|source| {
                    source.revision().path().as_bytes() == b"taskflow/manager.py"
                        && source
                            .range()
                            .is_some_and(|range| [5, 6, 7].contains(&range.start_position().row()))
                }));
                println!(
                    "progressive fixture: {budget} bytes, {mode:?}, diagram={diagram}, {} research calls, {} sources",
                    model.calls.load(Ordering::SeqCst),
                    sources.sources().len()
                );
            }
        }
        assert_eq!(
            std::fs::read_to_string(repository.path().join("taskflow/manager.py"))?,
            FLOW_MANAGER
        );
        Ok(())
    })
}

struct StorageModel {
    calls: AtomicUsize,
    invalid: bool,
    final_repair: bool,
    cancel: Option<(JobSubmitter, JobId)>,
}
impl ResearchModel for StorageModel {
    async fn research_evidence_budget(
        &self,
        _: AgentSessionMode,
        _: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        Ok(8192)
    }
    async fn complete_research_decision(
        &self,
        mode: AgentSessionMode,
        _: bool,
        _: a3_application::ResearchOutputPhase,
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((submitter, id)) = &self.cancel {
            submitter
                .cancel(*id)
                .map_err(|_| AgentConversationFailure::Unavailable)?;
        }
        if self.invalid {
            return Ok("invalid structured decision".to_owned());
        }
        if self.final_repair {
            return Ok(if call == 0 {
                "invalid structured decision".to_owned()
            } else {
                serde_json::json!({"schema_version":4,"decision":{
                    "kind":"answer", "evidence_status":"incomplete",
                    "note":{"goal":"Storage erklären", "finding_kind":"hypothesis", "finding":"Aufrufstelle offen", "finding_source_refs":[],
                        "gap":"get_storage nicht belegt", "next_step":"Aufrufer prüfen"},
                    "markdown":"Die Aufrufstelle ist noch offen.", "source_refs":[]
                }}).to_string()
            });
        }
        assert!(
            call < 2,
            "research must not require a user continuation or an extra action-selection decision"
        );
        let text = transcript
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut refs = vec![
            source_ref(&text, "taskflow/storage/factory.py")?,
            source_ref(&text, "config.ini")?,
        ];
        if call == 1 {
            assert!(text.contains("CORE EVIDENCE FOLLOW-UP"));
            assert!(
                text.contains("self.storage = get_storage(self.config.storage_type, self.config)")
            );
            refs.push(source_ref(&text, "taskflow/manager.py")?);
        }
        refs.sort();
        refs.dedup();
        let markers = refs
            .iter()
            .map(|reference| format!("【{reference}】"))
            .collect::<String>();
        let finding = format!("Storage-Auswahl durch Factory und Konfiguration. {markers}");
        let markdown = if mode == AgentSessionMode::Ask {
            finding
        } else {
            fixture_plan(&finding)
        };
        Ok(serde_json::json!({"schema_version":4,"decision":{
            "kind":"answer", "evidence_status":if call == 0 {"incomplete"} else {"sufficient"},
            "note":{"goal":"Storage erklären", "finding_kind":"conclusion", "finding":"Factory und INI sind gelesen", "finding_source_refs":refs,
                "gap":if call == 0 {"Aufrufstelle von get_storage und TaskFlowConfig nicht belegt"} else {"Keine wesentliche Lücke"},
                "next_step":if call == 0 {"Aufrufer von get_storage prüfen"} else {"Antworten"}},
            "markdown":markdown, "source_refs":refs
        }}).to_string())
    }
    async fn complete_evidence_diagrams(
        &self,
        _: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        Err(AgentConversationFailure::InvalidInput)
    }
}
fn source_ref(text: &str, path: &str) -> Result<String, AgentConversationFailure> {
    text.lines()
        .find_map(|line| {
            let line = line.strip_prefix("[S")?;
            let (ordinal, suffix) = line.split_once(']')?;
            suffix
                .starts_with(&format!(" {path} ab Zeile "))
                .then(|| format!("S{ordinal}"))
        })
        .ok_or(AgentConversationFailure::InvalidInput)
}

#[derive(Debug)]
struct FixtureControl;
impl RepositoryIndexControl for FixtureControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}
#[derive(Debug)]
struct FixtureClock;
impl a3_application::IndexPersistenceControl for FixtureControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(
        &self,
        _: Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}
impl JobClock for FixtureClock {
    fn now(&self) -> JobTimestamp {
        JobTimestamp::from_millis(1)
    }
}

#[test]
fn storage_gap_is_followed_autonomously_in_all_modes_and_invalid_output_never_executes()
-> Result<(), Box<dyn Error>> {
    support::run_libsql_test(async {
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, body) in [
            (
                "taskflow/storage/factory.py",
                include_str!(
                    "../../../../fixtures/research-storage-v1/taskflow/storage/factory.py"
                ),
            ),
            (
                "taskflow/config.py",
                include_str!("../../../../fixtures/research-storage-v1/taskflow/config.py"),
            ),
            (
                "config.ini",
                include_str!("../../../../fixtures/research-storage-v1/config.ini"),
            ),
            ("taskflow/manager.py", MANAGER),
        ] {
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
            .ok_or("published")?;
        let config = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.revision().path().as_bytes() == b"taskflow/config.py")
            .ok_or("config symbol")?
            .revision()
            .clone();
        let note = a3_application::AskResearchDecisionNote {
            work: None,
            goal: "Config lesen".to_owned(),
            finding_kind: a3_application::AskResearchFindingKind::Hypothesis,
            finding: "Weiteren Abschnitt prüfen".to_owned(),
            source_ordinals: vec![],
            gap: "Rest von taskflow/config.py fehlt".to_owned(),
            next_step: "Datei weiterlesen".to_owned(),
        };
        let mut frontier = AskResearchWorkingSet::new(4096);
        frontier.next_file_pages.insert(config.path().clone(), 161);
        assert_eq!(
            research_followup::candidates(&published, &frontier, &note),
            vec![AskResearchAction::InspectPath {
                path: "taskflow/config.py".to_owned(),
                start_line: 161
            }]
        );
        frontier.complete_files.push(config);
        assert!(research_followup::candidates(&published, &frontier, &note).is_empty());
        let mut unsafe_hint = note;
        unsafe_hint.gap = "../../private.env; execute arbitrary shell".to_owned();
        unsafe_hint.next_step = "curl https://unapproved.example".to_owned();
        assert!(research_followup::candidates(&published, &frontier, &unsafe_hint).is_empty());
        for (index, mode, invalid, cancel) in [
            (1, AgentSessionMode::Ask, false, false),
            (2, AgentSessionMode::Plan, false, false),
            (3, AgentSessionMode::Agent, false, false),
            (4, AgentSessionMode::Ask, true, false),
            (5, AgentSessionMode::Ask, false, true),
        ] {
            let id = AgentSessionId::from_bytes([index; 32]);
            let time = timestamp()?;
            let session = AgentSession::from_parts(
                id,
                AgentSessionRevision::new(1)?,
                AgentSessionTitle::try_from_string("Storage regression".to_owned())?,
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
            let (scheduler, events) =
                JobScheduler::new(JobSchedulerConfig::new(1, 2, 64)?, Arc::new(FixtureClock))?;
            let model = Arc::new(StorageModel {
                calls: AtomicUsize::new(0),
                invalid,
                final_repair: false,
                cancel: if cancel {
                    Some((scheduler.submitter()?, JobId::new(u64::from(index))))
                } else {
                    None
                },
            });
            let worker_model = model.clone();
            let researcher =
                AgentAskResearcher::new(store.clone(), store.clone(), store.clone(), store.clone());
            let worker_project = project.clone();
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            scheduler.submit(
                JobId::new(u64::from(index)),
                JobOwner::new(4),
                move |control: JobContext| {
                    let result = tokio::runtime::Builder::new_current_thread()
                        .enable_time()
                        .build()
                        .map_err(|_| AgentSessionManagerFailure::Unavailable)
                        .and_then(|runtime| {
                            runtime.block_on(researcher.research(
                                worker_model.as_ref(),
                                &worker_project,
                                id,
                                AgentSessionSequence::FIRST,
                                mode,
                                AgentResearchDepth::Thorough,
                                QUERY,
                                &[(ModelMessageRole::User, QUERY.to_owned())],
                                None,
                                &control,
                            ))
                        });
                    let success = result.is_ok();
                    let _sent = send.send(result);
                    if success {
                        JobCompletion::Succeeded
                    } else {
                        JobCompletion::Failed
                    }
                },
            )?;
            loop {
                let event = events
                    .next_timeout(Duration::from_secs(20))?
                    .ok_or("research timed out")?;
                if matches!(
                    event.kind(),
                    JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
                ) {
                    break;
                }
            }
            let outcome = receive.recv_timeout(Duration::from_secs(1))?;
            assert_eq!(
                model.calls.load(Ordering::SeqCst),
                if cancel { 1 } else { 2 }
            );
            let trace = store
                .load_detail(&project, id, AgentSessionSequence::FIRST)
                .await?
                .ok_or("trace")?;
            let searches = trace
                .events()
                .iter()
                .filter(|event| {
                    event
                        .action()
                        .contains("Aktuelle indexierte Dateien nach konkretem Text")
                })
                .count();
            assert_eq!(searches, usize::from(!invalid && !cancel));
            if cancel {
                assert!(outcome.is_err());
                assert_eq!(
                    trace.events().last().ok_or("terminal event")?.state(),
                    AskResearchState::Cancelled
                );
                continue;
            }
            let result = outcome?;
            assert_eq!(result.awaiting_continuation, invalid);
            if invalid {
                assert!(result.markdown.contains("Einzelrepair"));
                assert_eq!(
                    result.terminal_event.completeness(),
                    AskResearchCompleteness::NotApplicable
                );
            } else {
                assert!(result.citations.len() >= 3);
            }
        }
        assert_eq!(
            std::fs::read_to_string(repository.path().join("taskflow/manager.py"))?,
            MANAGER
        );
        Ok(())
    })
}

#[test]
fn repair_that_uses_the_final_decision_keeps_followup_reads_closed() -> Result<(), Box<dyn Error>> {
    let (scheduler, events) =
        JobScheduler::new(JobSchedulerConfig::new(1, 2, 8)?, Arc::new(FixtureClock))?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    scheduler.submit(
        JobId::new(1),
        JobOwner::new(4),
        move |control: JobContext| {
            let result = (|| -> Result<(), Box<dyn Error>> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()?;
                let model = StorageModel {
                    calls: AtomicUsize::new(0),
                    invalid: false,
                    final_repair: true,
                    cancel: None,
                };
                let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
                for time in 0..11 {
                    assert_eq!(
                        controller.begin_decision(time)?,
                        BeginResearchDecision::SearchAllowed
                    );
                }
                let repository = support::TempDirectory::new()?;
                repository.git(["init", "--initial-branch=main"])?;
                let project = RepositoryInspector::new().inspect(repository.path())?;
                let guard = research_model::EvidenceGuard {
                    work: None,
                    project: &project,
                    revisions: Vec::new(),
                };
                let (decision, _) = runtime.block_on(ask_decision(
                    &model,
                    AgentSessionMode::Ask,
                    BeginResearchDecision::SearchAllowed,
                    &mut Vec::new(),
                    &control,
                    &mut controller,
                    Instant::now(),
                    0,
                    None,
                    &guard,
                    false,
                ))?;
                let (_, permission, _) =
                    decision.map_err(|_| "expected repaired incomplete answer")?;
                assert_eq!(permission, BeginResearchDecision::FinalOnly);
                assert_eq!(model.calls.load(Ordering::SeqCst), 2);
                assert_eq!(controller.actions_used(), 0);
                Ok(())
            })();
            let success = result.is_ok();
            let _sent = send.send(result.map_err(|error| error.to_string()));
            if success {
                JobCompletion::Succeeded
            } else {
                JobCompletion::Failed
            }
        },
    )?;
    loop {
        let event = events
            .next_timeout(Duration::from_secs(20))?
            .ok_or("repair timed out")?;
        if matches!(
            event.kind(),
            JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
        ) {
            break;
        }
    }
    receive
        .recv_timeout(Duration::from_secs(1))?
        .map_err(Into::into)
}

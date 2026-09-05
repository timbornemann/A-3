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
    budget: usize,
    calls: AtomicUsize,
    diagrams: AtomicUsize,
}
impl ResearchModel for CoherentModel {
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
        transcript: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let packet = &transcript
            .iter()
            .find(|(_, text)| text.starts_with("CURRENT QUESTION:\n"))
            .ok_or(AgentConversationFailure::InvalidInput)?
            .1;
        assert!(packet.len() <= self.budget);
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
    support::run_libsql_test(async {
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
        for (index, mode, budget) in [
            (1, AgentSessionMode::Ask, 4096),
            (2, AgentSessionMode::Plan, 4096),
            (3, AgentSessionMode::Agent, 4096),
            (4, AgentSessionMode::Ask, 2048),
            (5, AgentSessionMode::Ask, 8192),
            (6, AgentSessionMode::Ask, 4096),
        ] {
            let query = if index == 6 {
                format!("/diagram {QUERY}")
            } else {
                QUERY.to_owned()
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
            let model = Arc::new(CoherentModel {
                budget,
                calls: AtomicUsize::new(0),
                diagrams: AtomicUsize::new(0),
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
            })?;
            let result = receive.recv_timeout(Duration::from_secs(1))??;
            assert!(!result.awaiting_continuation, "{}", result.markdown);
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
    })
}

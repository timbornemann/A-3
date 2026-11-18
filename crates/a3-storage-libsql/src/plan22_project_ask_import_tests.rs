//! Native Q01 source delivery with a fixed projected reply used only for provenance.

use super::*;
use a3_application::ProjectAskPhaseReceipt;

const Q01: &str = r#"Verfolge Manager.add_task vollständig bis zum tatsächlichen Writer und Return. Welche Reihenfolge, welche tatsächlichen Dateioperationen, welcher Pfadbezug und Schreibmodus? Unterscheide Speicherung von bloßen Rückgaben."#;
const MANAGER: &str = r#"from storage import Storage
from plugins import Dispatcher

class Manager:
    def __init__(self):
        self.storage = Storage()
        self.plugins = Dispatcher()
        self.tasks = []

    def add_task(self, title):
        task = {"title": title}
        self.tasks.append(task)
        self.storage.save_tasks(self.tasks)
        self.plugins.created(task)
        return task
"#;
const STORAGE: &str = r#"class Storage:
    def save_tasks(self, tasks):
        return ('memory-only', tasks)
"#;
const WRITING_STORAGE: &str = r#"import json

class Storage:
    def save_tasks(self, tasks):
        with open('tasks.json', 'w', encoding='utf-8') as output:
            json.dump(tasks, output, ensure_ascii=False)
        return len(tasks)
"#;
const PLUGINS: &str = r#"from pathlib import Path

class Audit:
    def on_created(self, task):
        return self._log(task)

    def _log(self, task):
        path = Path("audit.log").resolve()
        with path.open("a", encoding="utf-8") as output:
            output.write(str(task) + "\n")
        return path

class Dispatcher:
    def __init__(self):
        self.plugins = [Audit()]

    def created(self, task):
        for plugin in self.plugins:
            plugin.on_created(task)
"#;
const FILES: [(&str, &str); 3] = [
    ("manager.py", MANAGER),
    ("plugins.py", PLUGINS),
    ("storage.py", STORAGE),
];
const PLUGIN_QUESTION: &str = "Verfolge in Quelle 2 den Einstieg, den `add_task` aus Quelle 1 aufruft. Gib die sichtbare Aufrufkette in Reihenfolge mit exakten Aufrufnamen an. Zitiere sichtbare Rückgaben. Bei einem Schreibzugriff nenne den exakten Schreibaufruf, den Pfad und seinen Bezug zum aktuellen Arbeitsverzeichnis, den Modus und das Encoding.";
const STORAGE_WRITER_QUESTION: &str = "Was zeigt die importierte Deklaration `save_tasks` in Quelle 3? Zitiere den exakten Rückgabeausdruck. Beschreibe nur den gezeigten Deklarationskörper. Bei einem sichtbaren Schreibzugriff nenne den Schreibaufruf, den Pfad und seinen Bezug zum aktuellen Arbeitsverzeichnis, den Modus und das Encoding. Bei einer reinen Rückgabe nenne den Ausdruck und dass dort kein Schreibzugriff sichtbar ist.";
const STORAGE_RETURN_QUESTION: &str = "Gib im Feld text nur die vollständige Codezeile mit der Rückgabe von `save_tasks` wieder. Ohne Einrückung und ohne Markdown.";
const STORAGE_RETURN_REPLY: &str = "return ('memory-only', tasks)";
const STORAGE_RETURN_OBSERVATION: &str = "Der gezeigte Körper von `save_tasks` gibt `('memory-only', tasks)` zurück. In diesem Körper ist kein Schreibaufruf sichtbar.";
const PHASE_TEXTS: [&str; 3] = [
    "Belegteil zum Manager.",
    "Belegteil zum Plugin.",
    "Belegteil zur Storage-Deklaration.",
];

#[test]
fn project_ask_q01_delivers_complete_small_imports_with_exact_citations() -> TestResult {
    run_q01(STORAGE)
}

#[test]
fn project_ask_q01_focused_import_still_supplies_the_actual_writer() -> TestResult {
    run_q01(WRITING_STORAGE)
}

fn run_q01(storage_source: &'static str) -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            for (path, source) in FILES {
                let source = if path == "storage.py" {
                    storage_source
                } else {
                    source
                };
                fs::write(worktree.join(path), source)?;
            }
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;
            let before = inventory(&worktree)?;
            exercise_q01(&fixture, storage_source).await?;
            assert_eq!(inventory(&worktree)?, before);
            fixture.cleanup()
        })
    })
}

async fn exercise_q01(fixture: &Fixture, storage_source: &str) -> TestResult {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let profile = profile()?;
    let (storage_question, storage_reply, storage_answer) = if storage_source == STORAGE {
        (
            STORAGE_RETURN_QUESTION,
            STORAGE_RETURN_REPLY,
            STORAGE_RETURN_OBSERVATION,
        )
    } else {
        (STORAGE_WRITER_QUESTION, PHASE_TEXTS[2], PHASE_TEXTS[2])
    };
    let provider = ImportProvider {
        id: profile.provider_id().clone(),
        storage_reply,
        dispatches: Mutex::new(Vec::new()),
    };
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    let ProjectAskOutcome::Answer(answer) = ask
        .execute(
            Some(project),
            ask_request(Q01, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("Q01 did not deliver the small project import originals".into());
    };
    assert_eq!(answer.snapshot_id(), published.run().snapshot_id());
    assert_eq!(answer.worktree_id(), project.worktree().id());
    let [manager, plugin, storage] = answer.phases() else {
        return Err("expected one owner job and two import jobs".into());
    };
    assert_eq!(answer.parts().len(), PHASE_TEXTS.len());
    assert!(manager.calls().is_empty());
    assert_eq!(manager.part_count(), 1);
    let owner = &answer.parts()[0];
    let declaration = MANAGER
        .find("def add_task(")
        .ok_or("owner declaration missing")?;
    assert_eq!(owner.text().trim(), MANAGER[declaration..].trim());
    assert_eq!(owner.sources(), manager.delivered());
    {
        let dispatches = provider
            .dispatches
            .lock()
            .map_err(|_| "provider lock poisoned")?;
        assert_eq!(dispatches.len(), PHASE_TEXTS.len() - 1);
        let focused_storage = [("storage.py", storage_source)];
        for (import, ((phase, part), dispatch)) in answer
            .phases()
            .iter()
            .zip(answer.parts())
            .skip(1)
            .zip(dispatches.iter())
            .enumerate()
        {
            let number = import + 1;
            let (expected_files, question, aliases, expected_text) = if number == 1 {
                (
                    &FILES[..2],
                    PLUGIN_QUESTION,
                    [1, 2].as_slice(),
                    PHASE_TEXTS[1],
                )
            } else {
                (
                    focused_storage.as_slice(),
                    storage_question,
                    [3].as_slice(),
                    storage_answer,
                )
            };
            assert_phase(dispatch, phase, expected_files, question, aliases)?;
            assert_eq!(part.text(), expected_text);
            assert_eq!(part.sources(), phase.delivered());
            assert!(dispatch.messages.iter().all(|message| {
                PHASE_TEXTS[..number]
                    .iter()
                    .all(|earlier| !message.contains(earlier))
            }));
        }
        if storage_source == STORAGE {
            assert_ne!(answer.parts()[2].text(), provider.storage_reply);
        }
        assert_eq!(plugin.delivered()[0], manager.delivered()[0]);
        assert!(
            dispatches[1]
                .messages
                .iter()
                .all(|message| !message.contains("add_task") && !message.contains("manager.py"))
        );
        let aggregate = [
            manager.delivered()[0].clone(),
            plugin.delivered()[1].clone(),
            storage.delivered()[0].clone(),
        ];
        assert_eq!(answer.delivered(), aggregate.as_slice());
        assert_eq!(answer.wire(), Some(dispatches[1].wire));
    }

    let return_only = "Gib nur den Rückgabewert von Manager.add_task an.";
    let ProjectAskOutcome::Answer(local) = ask
        .execute(
            Some(project),
            ask_request(return_only, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("the requested return-only declaration did not produce an answer".into());
    };
    let [phase] = local.phases() else {
        return Err("the local return question dispatched multiple jobs".into());
    };
    assert_eq!(phase.calls().len(), 1);
    assert_eq!(phase.delivered().len(), 1);
    assert_eq!(
        phase.delivered()[0].revision().path().as_bytes(),
        b"manager.py"
    );
    assert!(phase.delivered()[0].range().start_byte() > 0);
    let dispatches = provider
        .dispatches
        .lock()
        .map_err(|_| "provider lock poisoned")?;
    let local_dispatch = dispatches.last().ok_or("local declaration job missing")?;
    let user = local_dispatch
        .messages
        .last()
        .ok_or("local question missing")?;
    let view: serde_json::Value = serde_json::from_str(user)?;
    assert_eq!(view["question"], return_only);
    assert_eq!(view["originals"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["originals"][0]["source"].as_u64(), Some(1));
    Ok(())
}

fn assert_phase(
    dispatch: &AskDispatch,
    phase: &ProjectAskPhaseReceipt,
    expected_files: &[(&str, &str)],
    expected_question: &str,
    expected_aliases: &[u64],
) -> TestResult {
    let [call] = phase.calls() else {
        return Err("expected one projected call for the source job".into());
    };
    assert_eq!(call.wire(), dispatch.wire);
    assert_eq!(call.usage(), Some(USAGE));
    assert_eq!(call.finish(), Some(ModelFinishReason::Stop));
    assert!(!call.is_repair());
    let message = dispatch
        .messages
        .last()
        .ok_or("Q01 user projection missing")?;
    let view: serde_json::Value = serde_json::from_str(message)?;
    assert_eq!(view["question"], expected_question);
    let originals = view["originals"]
        .as_array()
        .ok_or("source projection missing")?;
    assert_eq!(originals.len(), expected_files.len());
    assert_eq!(phase.delivered().len(), expected_files.len());
    assert_eq!(expected_aliases.len(), expected_files.len());
    for (index, &(path, expected)) in expected_files.iter().enumerate() {
        let original = &originals[index];
        let citation = &phase.delivered()[index];
        assert_eq!(original["source"].as_u64(), Some(expected_aliases[index]));
        assert_eq!(original["path"].as_str(), Some(path));
        assert_eq!(original["original"].as_str(), Some(expected));
        assert_eq!(citation.revision().path().as_bytes(), path.as_bytes());
        assert_eq!(
            citation.revision().content_hash().as_bytes(),
            blake3::hash(expected.as_bytes()).as_bytes()
        );
        assert_eq!(citation.range().start_byte(), 0);
        assert_eq!(
            usize::try_from(citation.range().end_byte())?,
            expected.len()
        );
    }
    Ok(())
}

#[derive(Debug)]
struct ImportProvider {
    id: ModelProviderId,
    storage_reply: &'static str,
    dispatches: Mutex<Vec<AskDispatch>>,
}

impl ModelProvider for ImportProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        project_test_wire(request)
    }

    fn stream_projected<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        projection: ModelWireProjection,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            if project_test_wire(request)? != projection {
                return Err(ModelProviderFailure::Rejected);
            }
            let mut captured = self
                .dispatches
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            let number = captured.len() + 1;
            let (text, sources): (&str, &[u8]) = match number {
                1 => (PHASE_TEXTS[1], &[1, 2]),
                2 => (self.storage_reply, &[3]),
                3 => (PHASE_TEXTS[0], &[1]),
                _ => return Err(ModelProviderFailure::Rejected),
            };
            captured.push(AskDispatch {
                messages: request
                    .messages()
                    .iter()
                    .map(|message| message.content().to_owned())
                    .collect(),
                wire: projection,
            });
            let reply = json!({
                "v": 1,
                "parts": [{"kind": "observation", "text": text, "sources": sources}],
            })
            .to_string();
            let events = futures::stream::iter([
                Ok(output(&reply)?),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    USAGE,
                ))),
            ]);
            Ok(Box::pin(events) as ProviderEventStream<'a>)
        })
    }
}

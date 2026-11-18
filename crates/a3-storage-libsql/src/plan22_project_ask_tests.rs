//! Q05 source freshness through the real candidate index and safe workspace reader.

#[path = "plan22_project_ask_caller_tests.rs"]
mod caller_tests;
#[path = "plan22_project_ask_effect_tests.rs"]
mod effect_tests;
#[path = "plan22_project_ask_import_tests.rs"]
mod import_tests;
#[path = "plan22_p6_live_ask_matrix.rs"]
mod live_matrix;
#[path = "plan22_p6_live_q05.rs"]
mod live_q05;
#[path = "plan22_project_ask_scope_tests.rs"]
mod scope_tests;
#[path = "plan22_project_ask_symbol_tests.rs"]
mod symbol_tests;

use super::*;
use a3_application::{
    AnswerProjectQuestion, ModelFinishReason, ModelOperationControl, ModelOutputChunk,
    ModelProvider, ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture,
    ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout, ModelWireProjection,
    ModelWireThinking, ProjectAskAnswer, ProjectAskFailure, ProjectAskOutcome, ProjectAskPartKind,
    ProjectAskRequest, ProviderEvent, ProviderEventStream,
};
use a3_workspace::WorkspaceAgentSourceReader;
use futures::StreamExt;
use serde_json::json;
use std::sync::atomic::AtomicUsize;

const QUESTION: &str = "Welcher DEFAULT_BACKEND gilt aktuell in config.py?";
const OLD_SOURCE: &str = "DEFAULT_BACKEND = 'json'\n";
const NEW_SOURCE: &str = "DEFAULT_BACKEND = 'sqlite'\n";
const INLINE_SOURCE: &str = "DEFAULT_BACKEND = 'memory'\n";
const OLD_ANSWER: &str = "DEFAULT_BACKEND ist json.";
const NEW_ANSWER: &str = "DEFAULT_BACKEND ist sqlite.";
const USAGE: ModelProviderUsage = ModelProviderUsage::new(Some(128), Some(28));

#[test]
fn project_ask_q05_reads_the_new_revision_and_rejects_an_edit_during_stream() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            let file = worktree.join("config.py");
            fs::write(&file, OLD_SOURCE)?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;

            exercise_q05(&fixture, &worktree, file).await?;
            fixture.cleanup()
        })
    })
}

async fn exercise_q05(fixture: &Fixture, worktree: &Path, file: PathBuf) -> TestResult {
    let project = &fixture.project;
    let (store, refresh, mut compiler, initial) = index_fixture(fixture).await?;
    let profile = profile()?;
    let provider = AskProvider {
        id: profile.provider_id().clone(),
        file: file.clone(),
        calls: AtomicUsize::new(0),
        dispatches: Mutex::new(Vec::new()),
    };
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );

    assert!(matches!(
        ask.execute(None, ask_request(QUESTION, &profile, None), &ActiveControl)
            .await?,
        ProjectAskOutcome::NoProject
    ));
    assert!(matches!(
        ask.execute(
            Some(project),
            ask_request(
                "Wie ist das Quasarmagnetfeld CelestialFlux aufgebaut?",
                &profile,
                None,
            ),
            &ActiveControl,
        )
        .await?,
        ProjectAskOutcome::NoProjectEvidence
    ));
    assert!(matches!(
        ask.execute(
            Some(project),
            ask_request("Lies ../config.py und C:\\config.py.", &profile, None),
            &ActiveControl,
        )
        .await?,
        ProjectAskOutcome::NoProjectEvidence
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert_only_source(worktree, OLD_SOURCE)?;

    let ProjectAskOutcome::Answer(first) = ask
        .execute(
            Some(project),
            ask_request(QUESTION, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("expected the current initial project answer".into());
    };
    let old_snapshot = initial.run().snapshot_id();
    assert_answer(&first, project, old_snapshot, None, OLD_ANSWER, OLD_SOURCE)?;
    assert!(first.historical_parts().is_empty());
    provider.assert_dispatch(0, &first, OLD_SOURCE, None)?;
    assert_only_source(worktree, OLD_SOURCE)?;

    fs::write(&file, NEW_SOURCE)?;
    let path = RepositoryPath::try_from_bytes(b"config.py".to_vec())?;
    let refresh = refresh
        .execute(
            project,
            &RepositoryChangeBatch::incremental(vec![path])?,
            &mut compiler,
            &ActiveControl,
        )
        .await?;
    let new_snapshot = refresh.published_index().run().snapshot_id();
    assert_ne!(old_snapshot, new_snapshot);

    let ProjectAskOutcome::Answer(second) = ask
        .execute(
            Some(project),
            ask_request(QUESTION, &profile, Some(&first)),
            &ActiveControl,
        )
        .await?
    else {
        return Err("expected a newly grounded project answer".into());
    };
    assert_answer(
        &second,
        project,
        new_snapshot,
        Some(old_snapshot),
        NEW_ANSWER,
        NEW_SOURCE,
    )?;
    let [historical] = second.historical_parts() else {
        return Err("expected one earlier answer part with its historical evidence".into());
    };
    assert_eq!(historical.text(), OLD_ANSWER);
    assert_eq!(historical.sources(), first.delivered());
    provider.assert_dispatch(1, &second, NEW_SOURCE, Some(OLD_SOURCE))?;
    assert_only_source(worktree, NEW_SOURCE)?;

    assert_eq!(NEW_SOURCE.len(), INLINE_SOURCE.len());
    assert!(matches!(
        ask.execute(
            Some(project),
            ask_request(QUESTION, &profile, Some(&second)),
            &ActiveControl,
        )
        .await,
        Err(ProjectAskFailure::StaleSource)
    ));
    {
        let dispatches = provider
            .dispatches
            .lock()
            .map_err(|_| "provider lock poisoned")?;
        assert_eq!(dispatches.len(), 3);
        dispatches[2].assert_original(NEW_SOURCE)?;
    }
    assert_only_source(worktree, INLINE_SOURCE)?;
    assert_eq!(
        store
            .latest_published_index(project, &ActiveControl)
            .await?
            .ok_or("published index lost")?
            .run()
            .snapshot_id(),
        new_snapshot,
    );
    Ok(())
}

fn ask_request<'a>(
    question: &'a str,
    profile: &'a ModelProfile,
    previous: Option<&'a ProjectAskAnswer>,
) -> ProjectAskRequest<'a> {
    ProjectAskRequest {
        question,
        profile,
        previous,
        selected_sources: &[],
        timeout: ModelRequestTimeout::DEFAULT,
    }
}

fn assert_answer(
    answer: &ProjectAskAnswer,
    project: &ProjectIdentity,
    snapshot: SnapshotId,
    previous: Option<SnapshotId>,
    expected: &str,
    source: &str,
) -> TestResult {
    assert_eq!(answer.worktree_id(), project.worktree().id());
    assert_eq!(answer.snapshot_id(), snapshot);
    assert_eq!(answer.previous_snapshot_id(), previous);
    assert_eq!(answer.usage(), Some(USAGE));
    let [delivered] = answer.delivered() else {
        return Err("expected one exact delivered original".into());
    };
    assert_eq!(
        delivered.revision().path(),
        &RepositoryPath::try_from_bytes(b"config.py".to_vec())?
    );
    assert_eq!(
        delivered.revision().content_hash().as_bytes(),
        blake3::hash(source.as_bytes()).as_bytes(),
    );
    assert_eq!(delivered.range().start_byte(), 0);
    assert_eq!(usize::try_from(delivered.range().end_byte())?, source.len());
    let [part] = answer.parts() else {
        return Err("expected one source-bound observation".into());
    };
    assert_eq!(part.text(), expected);
    assert_eq!(part.kind(), ProjectAskPartKind::Observation);
    assert_eq!(part.sources(), answer.delivered());
    Ok(())
}

fn assert_only_source(worktree: &Path, expected: &str) -> TestResult {
    assert_eq!(
        inventory(worktree)?,
        [(PathBuf::from("config.py"), expected.as_bytes().to_vec())]
    );
    Ok(())
}

#[derive(Debug)]
struct AskProvider {
    id: ModelProviderId,
    file: PathBuf,
    calls: AtomicUsize,
    dispatches: Mutex<Vec<AskDispatch>>,
}

#[derive(Debug)]
struct AskDispatch {
    messages: Vec<String>,
    wire: ModelWireProjection,
}

impl AskDispatch {
    fn assert_original(&self, source: &str) -> TestResult {
        let view = self
            .messages
            .iter()
            .filter_map(|message| serde_json::from_str::<serde_json::Value>(message).ok())
            .find(|value| value.get("originals").is_some())
            .ok_or("provider did not receive the original source projection")?;
        assert_eq!(view["question"], QUESTION);
        assert_eq!(
            view["originals"],
            json!([{
                "source": 1, "path": "config.py", "original": source
            }])
        );
        Ok(())
    }
}

impl AskProvider {
    fn assert_dispatch(
        &self,
        index: usize,
        answer: &ProjectAskAnswer,
        source: &str,
        stale: Option<&str>,
    ) -> TestResult {
        let dispatches = self
            .dispatches
            .lock()
            .map_err(|_| "provider lock poisoned")?;
        assert_eq!(dispatches.len(), index + 1);
        let dispatch = &dispatches[index];
        assert_eq!(
            Some(dispatch.wire.digest()),
            answer.wire().map(ModelWireProjection::digest)
        );
        dispatch.assert_original(source)?;
        if let Some(stale) = stale {
            assert!(!dispatch.messages.iter().any(|message| {
                message.contains(stale.trim_end()) || message.contains(OLD_ANSWER)
            }));
        }
        Ok(())
    }
}

impl ModelProvider for AskProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.id
    }

    fn stream<'a>(
        &'a self,
        _: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Err(ModelProviderFailure::Rejected)))
    }

    fn project_wire_request(
        &self,
        request: &ModelProviderRequest,
    ) -> Result<ModelWireProjection, ModelProviderFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            if project_test_wire(request)? != projection {
                return Err(ModelProviderFailure::Rejected);
            }
            let number = {
                let mut dispatches = self
                    .dispatches
                    .lock()
                    .map_err(|_| ModelProviderFailure::Unavailable)?;
                let number = dispatches.len();
                dispatches.push(AskDispatch {
                    messages: request
                        .messages()
                        .iter()
                        .map(|message| message.content().to_owned())
                        .collect(),
                    wire: projection,
                });
                number
            };
            let answer = match number {
                0 => OLD_ANSWER,
                1 | 2 => NEW_ANSWER,
                _ => return Err(ModelProviderFailure::Rejected),
            };
            let reply = json!({
                "v": 1,
                "parts": [{"kind": "observation", "text": answer, "sources": [1]}],
            })
            .to_string();
            let (first, last) = reply.split_at(8);
            let first = output(first)?;
            let last = output(last)?;
            let first = futures::stream::iter([Ok(first)]);
            let last = futures::stream::once(async move {
                if number == 2 {
                    // The first output fragment has been read, but the answer has not ended.
                    fs::write(&self.file, INLINE_SOURCE)
                        .map_err(|_| ModelProviderFailure::Unavailable)?;
                }
                Ok(last)
            });
            let completed = futures::stream::iter([Ok(ProviderEvent::Completed(
                ModelProviderCompletion::new(ModelFinishReason::Stop, USAGE),
            ))]);
            Ok(Box::pin(first.chain(last).chain(completed)) as ProviderEventStream<'a>)
        })
    }
}

fn output(text: &str) -> Result<ProviderEvent, ModelProviderFailure> {
    ModelOutputChunk::try_from_string(text.to_owned())
        .map(ProviderEvent::OutputText)
        .map_err(|_| ModelProviderFailure::InvalidResponse)
}

fn project_test_wire(
    request: &ModelProviderRequest,
) -> Result<ModelWireProjection, ModelProviderFailure> {
    let schema = request
        .structured_output()
        .ok_or(ModelProviderFailure::Rejected)?
        .value();
    let format = serde_json::to_vec(schema).map_err(|_| ModelProviderFailure::Rejected)?;
    let messages = request
        .messages()
        .iter()
        .map(|message| json!({"role": format!("{:?}", message.role()), "text": message.content()}))
        .collect::<Vec<_>>();
    let context = request.profile().settings().context_limit().get();
    let output = request.profile().settings().output_limit().get();
    let body = serde_json::to_vec(&json!({
        "messages": messages,
        "format": schema,
        "context": context,
        "output": output,
        "think": false,
    }))
    .map_err(|_| ModelProviderFailure::Rejected)?;
    ModelWireProjection::from_serialized(
        &body,
        request
            .messages()
            .iter()
            .map(|message| message.content().len())
            .sum(),
        format.len(),
        context,
        output,
        ModelWireThinking::Disabled,
    )
}

//! Native Q04 source and repair contracts with an explicitly scripted provider.

use super::*;

const Q04: &str = "Was kann anhand dieses Projekts über die Dateiwirkungen von dispatcher.dispatch sicher gesagt werden, und welche Implementierung oder Laufzeitevidence fehlt gegebenenfalls?";
const CALLBACK: &str = "def dispatch(callback, payload):\n    return callback(payload)\n";
const REGISTRY: &str = "from writer import write\n\nREGISTRY = {'audit': write}\n\ndef dispatch(name, payload):\n    return REGISTRY[name](payload)\n";
const WRITER: &str = "def write(payload):\n    return ('not-persisted', payload)\n";
const UNSUPPORTED: &str = "Es werden keine Dateioperationen ausgeführt.";
const QUOTE: &str = "return callback(payload)";
const CLOSED_EFFECT: &str = "Der Eintrag 'audit' bindet write. Diese Funktion gibt das Tupel ('not-persisted', payload) zurück.";

#[test]
fn project_ask_q04_repairs_unsupported_callback_claim_into_bound_observation() -> TestResult {
    let files = [("dispatcher.py", CALLBACK)];
    let replies = [(UNSUPPORTED, [1].as_slice()), (QUOTE, [1].as_slice())];
    run_effect_case(&files, &replies, |answer, dispatches| {
        let [phase] = answer.phases() else {
            return Err("expected one effect job".into());
        };
        let [initial, repair] = phase.calls() else {
            return Err("expected one bounded structural repair".into());
        };
        assert!(!initial.is_repair());
        assert!(repair.is_repair());
        assert_eq!(initial.finish(), Some(ModelFinishReason::Stop));
        assert_eq!(repair.finish(), Some(ModelFinishReason::Stop));
        assert_eq!(initial.wire(), dispatches[0].wire);
        assert_eq!(repair.wire(), dispatches[1].wire);
        assert_ne!(initial.wire(), repair.wire());
        for dispatch in dispatches {
            assert_originals(dispatch, &files)?;
            assert!(
                dispatch
                    .messages
                    .iter()
                    .all(|message| !message.contains(UNSUPPORTED))
            );
        }
        let [limit] = answer.parts() else {
            return Err("expected one Core-rendered limit for the callback".into());
        };
        assert_eq!(limit.kind(), ProjectAskPartKind::Limit);
        assert_eq!(limit.sources(), phase.delivered());
        assert!(limit.text().contains(QUOTE));
        assert!(limit.text().contains("konkrete Implementierung"));
        assert!(limit.text().contains("mögliche Dateiwirkungen"));
        assert!(limit.text().contains("offen"));
        assert!(!limit.text().contains(UNSUPPORTED));
        Ok(())
    })
}

#[test]
fn project_ask_q04_closed_registry_preserves_the_imported_writer() -> TestResult {
    let files = [("dispatcher.py", REGISTRY), ("writer.py", WRITER)];
    let replies = [(CLOSED_EFFECT, [1, 2].as_slice())];
    run_effect_case(&files, &replies, |answer, dispatches| {
        let [phase] = answer.phases() else {
            return Err("expected one dispatch and writer job".into());
        };
        let [part] = answer.parts() else {
            return Err("expected the original writer observation".into());
        };
        let [call] = phase.calls() else {
            return Err("closed registry must not enter callback repair".into());
        };
        assert!(!call.is_repair());
        assert_eq!(part.kind(), ProjectAskPartKind::Observation);
        assert_eq!(part.text(), CLOSED_EFFECT);
        assert_eq!(part.sources(), phase.delivered());
        assert_originals(&dispatches[0], &files)
    })
}

fn run_effect_case(
    files: &[(&str, &str)],
    replies: &[(&str, &[u8])],
    assert_answer: impl FnOnce(&ProjectAskAnswer, &[AskDispatch]) -> TestResult,
) -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            for (path, source) in files {
                fs::write(worktree.join(path), source)?;
            }
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;
            let before = inventory(&worktree)?;
            let (store, _, _, published) = index_fixture(&fixture).await?;
            let profile = profile()?;
            let provider = EffectProvider {
                id: profile.provider_id().clone(),
                replies,
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
                    Some(&fixture.project),
                    ask_request(Q04, &profile, None),
                    &ActiveControl,
                )
                .await?
            else {
                return Err("expected a Q04 answer with current evidence".into());
            };
            assert_eq!(answer.snapshot_id(), published.run().snapshot_id());
            assert_eq!(answer.index_run_id(), published.run().id());
            assert_eq!(answer.delivered().len(), files.len());
            for (citation, &(path, original)) in answer.delivered().iter().zip(files) {
                assert_eq!(citation.revision().path().as_bytes(), path.as_bytes());
                assert_eq!(
                    citation.revision().content_hash().as_bytes(),
                    blake3::hash(original.as_bytes()).as_bytes()
                );
                assert_eq!(citation.range().start_byte(), 0);
                assert_eq!(
                    usize::try_from(citation.range().end_byte())?,
                    original.len()
                );
            }
            assert_eq!(inventory(&worktree)?, before);
            let dispatches = provider
                .dispatches
                .into_inner()
                .map_err(|_| "provider lock poisoned")?;
            assert_eq!(dispatches.len(), replies.len());
            assert_answer(&answer, &dispatches)?;
            drop(store);
            fixture.cleanup()
        })
    })
}

fn assert_originals(dispatch: &AskDispatch, files: &[(&str, &str)]) -> TestResult {
    let user = dispatch
        .messages
        .last()
        .ok_or("Q04 user projection missing")?;
    let view: serde_json::Value = serde_json::from_str(user)?;
    let originals = view["originals"]
        .as_array()
        .ok_or("Q04 originals missing")?;
    assert_eq!(originals.len(), files.len());
    for (index, (original, &(path, text))) in originals.iter().zip(files).enumerate() {
        assert_eq!(original["source"].as_u64(), Some(u64::try_from(index + 1)?));
        assert_eq!(original["path"].as_str(), Some(path));
        assert_eq!(original["original"].as_str(), Some(text));
    }
    Ok(())
}

#[derive(Debug)]
struct EffectProvider<'a> {
    id: ModelProviderId,
    replies: &'a [(&'a str, &'a [u8])],
    dispatches: Mutex<Vec<AskDispatch>>,
}

impl ModelProvider for EffectProvider<'_> {
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
            let mut dispatches = self
                .dispatches
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            let &(text, sources) = self
                .replies
                .get(dispatches.len())
                .ok_or(ModelProviderFailure::Rejected)?;
            dispatches.push(AskDispatch {
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

//! Q03 qualified-symbol delivery through the actual candidate index and safe reader.

use super::*;
use a3_application::KnowledgeSearchStore;
use a3_domain::{ExactSearchPageSize, ExactSearchQuery, ExactSearchTarget, ExactSearchTerm};

const Q03: &str = "Storage.save_tasks committed die SQLite-Transaktion. Wo erfolgt der Commit?";
const CLASS_HEADER: &str = "class Storage:\n";
const STORAGE: &str =
    "class Storage:\n    def save_tasks(self, tasks):\n        return ('memory-only', tasks)\n";
const SHADOW: &str =
    "class Archive:\n    def save_tasks(self, tasks):\n        return ('other-storage', tasks)\n";
const DUPLICATE: &str =
    "class Storage:\n    def save_tasks(self, tasks):\n        return ('second-storage', tasks)\n";
const RESPONSE: &str = r#"{"v":1,"parts":[{"kind":"observation","text":"Die Methode gibt ein Tupel zurueck.","sources":[1]}]}"#;

#[test]
fn project_ask_q03_delivers_only_the_qualified_method_and_stops_on_ambiguity() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let mut fixture = Fixture::new()?;
            let worktree = fixture.root.join("worktree");
            initialize_git(&worktree)?;
            fs::write(worktree.join("storage.py"), STORAGE)?;
            fs::write(worktree.join("archive.py"), SHADOW)?;
            fixture.project = RepositoryInspector::new().inspect(&worktree)?;
            exercise_symbol_lookup(&fixture, &worktree).await?;
            fixture.cleanup()
        })
    })
}

async fn exercise_symbol_lookup(fixture: &Fixture, worktree: &Path) -> TestResult {
    let project = &fixture.project;
    let (store, refresh, mut compiler, initial) = index_fixture(fixture).await?;
    let profile = profile()?;
    let provider = SymbolProvider {
        id: profile.provider_id().clone(),
        calls: AtomicUsize::new(0),
        captured: Mutex::new(None),
    };
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    let original_inventory = inventory(worktree)?;
    let ProjectAskOutcome::Answer(answer) = ask
        .execute(
            Some(project),
            ask_request(Q03, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("the unique qualified method did not produce an answer".into());
    };
    assert_eq!(answer.snapshot_id(), initial.run().snapshot_id());
    assert_eq!(answer.worktree_id(), project.worktree().id());
    let [part] = answer.parts() else {
        return Err("expected one source-bound response part".into());
    };
    let [source] = answer.delivered() else {
        return Err("expected only the selected declaration".into());
    };
    assert_eq!(part.kind(), ProjectAskPartKind::Observation);
    assert_eq!(part.sources(), answer.delivered());
    assert_eq!(source.revision().path().as_bytes(), b"storage.py");
    assert_eq!(
        source.revision().content_hash().as_bytes(),
        blake3::hash(STORAGE.as_bytes()).as_bytes()
    );
    // The safe reader returns the declaration's exact complete lines.
    assert_eq!(
        usize::try_from(source.range().start_byte())?,
        CLASS_HEADER.len()
    );
    assert_eq!(usize::try_from(source.range().end_byte())?, STORAGE.len());
    {
        let mut captured = provider
            .captured
            .lock()
            .map_err(|_| "provider lock poisoned")?;
        let dispatched = captured.take().ok_or("projected request missing")?;
        assert_eq!(Some(dispatched.wire), answer.wire());
        let user = dispatched
            .messages
            .last()
            .ok_or("user source projection missing")?;
        let user: serde_json::Value = serde_json::from_str(user)?;
        assert_eq!(user["question"], Q03);
        assert_eq!(
            user["originals"],
            json!([{"source": 1, "path": "storage.py", "original": &STORAGE[CLASS_HEADER.len()..]}])
        );
        assert!(dispatched.messages.iter().all(|message| {
            !message.contains("class Storage:") && !message.contains("other-storage")
        }));
    }
    assert_eq!(inventory(worktree)?, original_inventory);

    let whole_file_question =
        "Gib den kompletten Inhalt von storage.py und erkläre Storage.save_tasks.";
    let ProjectAskOutcome::Answer(whole_file) = ask
        .execute(
            Some(project),
            ask_request(whole_file_question, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("explicit full-file selection was lost".into());
    };
    let [source] = whole_file.delivered() else {
        return Err("expected one coalesced complete file".into());
    };
    assert_eq!(source.range().start_byte(), 0);
    assert_eq!(usize::try_from(source.range().end_byte())?, STORAGE.len());
    {
        let captured = provider
            .captured
            .lock()
            .map_err(|_| "provider lock poisoned")?;
        let user = captured
            .as_ref()
            .and_then(|dispatch| dispatch.messages.last())
            .ok_or("complete file projection missing")?;
        let user: serde_json::Value = serde_json::from_str(user)?;
        assert_eq!(user["question"], whole_file_question);
        assert_eq!(
            user["originals"],
            json!([{"source": 1, "path": "storage.py", "original": STORAGE}])
        );
    }

    fs::write(worktree.join("second.py"), DUPLICATE)?;
    let path = RepositoryPath::try_from_bytes(b"second.py".to_vec())?;
    let published = refresh
        .execute(
            project,
            &RepositoryChangeBatch::incremental(vec![path])?,
            &mut compiler,
            &ActiveControl,
        )
        .await?;
    let hits = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("save_tasks".to_owned())?),
            ExactSearchPageSize::DEFAULT,
            None,
            &ActiveControl,
        )
        .await?;
    assert_eq!(hits.index_run_id(), published.published_index().run().id());
    assert!(hits.next_cursor().is_none());
    let mut matching_paths = hits
        .hits()
        .iter()
        .filter_map(|hit| {
            let ExactSearchTarget::Symbol(symbol) = hit.target() else {
                return None;
            };
            let name = symbol.qualified_name().as_str();
            (name == "Storage::save_tasks" || name.ends_with("::Storage::save_tasks"))
                .then_some(symbol.symbol().revision().path().as_bytes())
        })
        .collect::<Vec<_>>();
    matching_paths.sort_unstable();
    assert_eq!(
        matching_paths,
        [b"second.py".as_slice(), b"storage.py".as_slice()]
    );

    let calls = provider.calls.load(Ordering::SeqCst);
    let ambiguous_inventory = inventory(worktree)?;
    assert!(matches!(
        ask.execute(
            Some(project),
            ask_request(Q03, &profile, None),
            &ActiveControl
        )
        .await?,
        ProjectAskOutcome::NoProjectEvidence
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), calls);
    assert_eq!(inventory(worktree)?, ambiguous_inventory);
    Ok(())
}

#[derive(Debug)]
struct SymbolProvider {
    id: ModelProviderId,
    calls: AtomicUsize,
    captured: Mutex<Option<AskDispatch>>,
}

impl ModelProvider for SymbolProvider {
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
            let mut captured = self
                .captured
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            if captured.is_some() {
                return Err(ModelProviderFailure::Rejected);
            }
            *captured = Some(AskDispatch {
                messages: request
                    .messages()
                    .iter()
                    .map(|message| message.content().to_owned())
                    .collect(),
                wire: projection,
            });
            // Fixed output exercises citation binding, without measuring model quality.
            let events = futures::stream::iter([
                Ok(output(RESPONSE)?),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    USAGE,
                ))),
            ]);
            Ok(Box::pin(events) as ProviderEventStream<'a>)
        })
    }
}

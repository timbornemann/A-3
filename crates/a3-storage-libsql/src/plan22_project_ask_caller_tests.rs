//! Q02 caller evidence from the actual published candidate graph.

use super::*;
use a3_application::{AgentSourceReader, KnowledgeSearchStore, ProjectAskPhaseReceipt};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, ExactSearchPageSize,
    ExactSearchQuery, ExactSearchTerm, GraphEdge, GraphEndpoint, GraphSymbol, GraphTraversalHit,
    GraphTraversalTarget, SourceChannel, SyntaxRelationKind, TraversalDepth, TraversalDirection,
    TraversalQuery, TraversalResultLimit,
};

const DOMAIN: &str = r#"def create_task(title):
    return {'title': title, 'id': 'local-' + title}
"#;
const SERVICE: &str = r#"from domain import create_task as persist_task

def schedule(title):
    return persist_task(title)
"#;
const API: &str = r#"from service import schedule

def post(title):
    return schedule(title)
"#;
const BACKFILL: &str = r#"from service import schedule

def restore(title):
    return schedule(title)
"#;
const REPORTING: &str = r#"def create_task_report(count):
    return str(count)
"#;
const FILES: [(&str, &str); 5] = [
    ("domain.py", DOMAIN),
    ("service.py", SERVICE),
    ("api.py", API),
    ("backfill.py", BACKFILL),
    ("reporting.py", REPORTING),
];

const Q02: &str = r#"Welche direkten und indirekten Aufrufer führen zu domain.create_task, was bedeutet der Alias persist_task und gehört create_task_report dazu? Belege die aktuelle Kette."#;
const REPORT_QUOTE: &str = "return str(count)";
const REPORT_TEXT: &str = "Der gezeigte Körper ruft `str(count)` auf und gibt das Ergebnis dieses Aufrufs zurück. Ein direkter Aufruf mit dem Namen `create_task` ist in diesem Körper nicht sichtbar.";

#[test]
fn project_ask_q02_current_graph_proves_direct_and_indirect_callers() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = q02_fixture()?;
            let worktree = fixture.root.join("worktree");
            let before = inventory(&worktree)?;
            exercise_q02(&fixture).await?;
            assert_eq!(inventory(&worktree)?, before);
            fixture.cleanup()
        })
    })
}

#[test]
fn project_ask_q02_composes_call_chain_and_report_declaration() -> TestResult {
    run_q02_composition(Q02, false)
}

#[test]
fn project_ask_q02_explicit_file_keeps_the_qualified_caller_seed() -> TestResult {
    run_q02_composition(&format!("{Q02} Die Deklaration steht in domain.py."), false)
}

#[test]
fn project_ask_q02_keeps_a_separate_declaration_in_a_caller_file() -> TestResult {
    run_q02_composition(Q02, true)
}

fn run_q02_composition(question: &str, shared_file: bool) -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let (fixture, report) = if shared_file {
                let service = format!("{SERVICE}\n{REPORTING}");
                (
                    q02_fixture_with_report(Some(&service))?,
                    ("service.py", service, SERVICE.len() + 1),
                )
            } else {
                (q02_fixture()?, ("reporting.py", REPORTING.to_owned(), 0))
            };
            let worktree = fixture.root.join("worktree");
            let before = inventory(&worktree)?;
            exercise_q02_composition(&fixture, question, (report.0, &report.1, report.2)).await?;
            assert_eq!(inventory(&worktree)?, before);
            fixture.cleanup()
        })
    })
}

#[test]
fn project_ask_q02_ambiguous_caller_seed_does_not_dispatch_a_report_job() -> TestResult {
    crate::run_native_libsql_test(async {
        with_context_runtime(async {
            let fixture = q02_fixture()?;
            let worktree = fixture.root.join("worktree");
            let alternate = worktree.join("alternate");
            fs::create_dir(&alternate)?;
            fs::write(alternate.join("domain.py"), DOMAIN)?;
            let before = inventory(&worktree)?;
            assert_ambiguous_caller_stops(&fixture).await?;
            assert_eq!(inventory(&worktree)?, before);
            fixture.cleanup()
        })
    })
}

async fn assert_ambiguous_caller_stops(fixture: &Fixture) -> TestResult {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let candidates = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("create_task".to_owned())?),
            ExactSearchPageSize::DEFAULT,
            None,
            &ActiveControl,
        )
        .await?;
    assert_eq!(candidates.index_run_id(), published.run().id());
    assert!(candidates.next_cursor().is_none());
    let mut domains = candidates
        .hits()
        .iter()
        .filter_map(|hit| {
            let GraphTraversalTarget::Symbol(symbol) = hit.target() else {
                return None;
            };
            let name = symbol.qualified_name().as_str();
            (name == "domain::create_task" || name.ends_with("::domain::create_task"))
                .then_some(symbol.symbol().revision().path().as_bytes())
        })
        .collect::<Vec<_>>();
    domains.sort_unstable();
    assert_eq!(
        domains,
        [b"alternate/domain.py".as_slice(), b"domain.py".as_slice()]
    );
    let report = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(
                "create_task_report".to_owned(),
            )?),
            ExactSearchPageSize::DEFAULT,
            None,
            &ActiveControl,
        )
        .await?;
    let [report] = report.hits() else {
        return Err("expected the separate report declaration to be uniquely visible".into());
    };
    assert!(
        matches!(report.target(), GraphTraversalTarget::Symbol(symbol)
        if symbol.symbol().revision().path().as_bytes() == b"reporting.py")
    );

    let profile = profile()?;
    let provider = ComposedAskProvider::new(&profile);
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    assert!(matches!(
        ask.execute(
            Some(project),
            ask_request(Q02, &profile, None),
            &ActiveControl
        )
        .await?,
        ProjectAskOutcome::NoProjectEvidence
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

fn q02_fixture() -> TestResult<Fixture> {
    q02_fixture_with_report(None)
}

fn q02_fixture_with_report(service_with_report: Option<&str>) -> TestResult<Fixture> {
    let mut fixture = Fixture::new()?;
    let worktree = fixture.root.join("worktree");
    initialize_git(&worktree)?;
    for (path, source) in FILES {
        let source = match (service_with_report, path) {
            (Some(source), "service.py") => source,
            (Some(_), "reporting.py") => continue,
            _ => source,
        };
        fs::write(worktree.join(path), source)?;
    }
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    Ok(fixture)
}

async fn exercise_q02_composition(
    fixture: &Fixture,
    question: &str,
    expected_report: (&str, &str, usize),
) -> TestResult {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let profile = profile()?;
    let provider = ComposedAskProvider::new(&profile);
    let ask = AnswerProjectQuestion::new(
        store.as_ref(),
        store.as_ref(),
        &WorkspaceAgentSourceReader,
        &provider,
    );
    let ProjectAskOutcome::Answer(answer) = ask
        .execute(
            Some(project),
            ask_request(question, &profile, None),
            &ActiveControl,
        )
        .await?
    else {
        return Err("expected the composed source-bound Q02 answer".into());
    };
    assert_eq!(answer.snapshot_id(), published.run().snapshot_id());
    assert_eq!(answer.worktree_id(), project.worktree().id());
    let [chain, report] = answer.phases() else {
        return Err("expected a caller-chain job and a separate declaration job".into());
    };
    let [routes, report_part] = answer.parts() else {
        return Err("expected bound static routes and a separate declaration".into());
    };
    assert_eq!(routes.kind(), ProjectAskPartKind::Observation);
    for route in [
        "`service.py::schedule` → `domain.py::create_task`",
        "`api.py::post` → `service.py::schedule` → `domain.py::create_task`",
        "`backfill.py::restore` → `service.py::schedule` → `domain.py::create_task`",
    ] {
        assert!(routes.text().contains(route));
    }
    assert!(routes.text().contains("`persist_task`"));
    assert!(
        routes
            .text()
            .contains("Importquelle `from domain import create_task as persist_task`")
    );
    assert!(
        routes
            .text()
            .contains("return {'title': title, 'id': 'local-' + title}")
    );
    assert_eq!(routes.sources().len(), 4);
    assert!(
        routes
            .sources()
            .iter()
            .all(|source| chain.delivered().contains(source))
    );
    assert_eq!(chain.part_count(), 1);
    assert!(chain.calls().is_empty());
    assert_eq!(report.part_count(), 1);
    assert_eq!(report_part.text(), REPORT_TEXT);
    assert_eq!(report_part.sources(), report.delivered());
    assert!(
        answer
            .delivered()
            .iter()
            .eq(chain.delivered().iter().chain(report.delivered()))
    );
    let dispatches = provider
        .dispatches
        .lock()
        .map_err(|_| "provider lock poisoned")?;
    let [report_wire] = dispatches.as_slice() else {
        return Err("expected only the separate declaration provider call".into());
    };
    assert_phase(
        report_wire,
        report,
        expected_report,
        "Gib im Feld text nur die vollständige Codezeile mit der Rückgabe von `create_task_report` wieder. Ohne Einrückung und ohne Markdown.",
    )?;
    assert!(
        report_wire
            .messages
            .iter()
            .all(|message| !message.contains("Statisch sichtbare Aufrufpfade"))
    );
    Ok(())
}

fn assert_phase(
    dispatch: &AskDispatch,
    phase: &ProjectAskPhaseReceipt,
    expected_report: (&str, &str, usize),
    expected_question: &str,
) -> TestResult {
    let [call] = phase.calls() else {
        return Err("expected one actual initial call for this source job".into());
    };
    assert_eq!(call.wire(), dispatch.wire);
    assert_eq!(call.usage(), Some(USAGE));
    assert_eq!(call.finish(), Some(ModelFinishReason::Stop));
    assert!(!call.is_repair());
    let user = dispatch
        .messages
        .last()
        .ok_or("Q02 user projection missing")?;
    let user: serde_json::Value = serde_json::from_str(user)?;
    assert_eq!(user["question"], expected_question);
    let originals = user["originals"]
        .as_array()
        .ok_or("Q02 originals missing")?;
    let ([original], [evidence]) = (originals.as_slice(), phase.delivered()) else {
        return Err("expected one original for the related declaration".into());
    };
    let (path, source, start) = expected_report;
    assert_eq!(original["path"].as_str(), Some(path));
    assert_eq!(original["source"].as_u64(), Some(5));
    assert_eq!(original["original"].as_str(), source.get(start..));
    assert_eq!(evidence.revision().path().as_bytes(), path.as_bytes());
    assert_eq!(
        evidence.revision().content_hash().as_bytes(),
        blake3::hash(source.as_bytes()).as_bytes()
    );
    assert_eq!(usize::try_from(evidence.range().start_byte())?, start);
    assert_eq!(usize::try_from(evidence.range().end_byte())?, source.len());
    Ok(())
}

#[derive(Debug)]
struct ComposedAskProvider {
    id: ModelProviderId,
    calls: AtomicUsize,
    dispatches: Mutex<Vec<AskDispatch>>,
}

impl ComposedAskProvider {
    fn new(profile: &ModelProfile) -> Self {
        Self {
            id: profile.provider_id().clone(),
            calls: AtomicUsize::new(0),
            dispatches: Mutex::new(Vec::new()),
        }
    }
}

impl ModelProvider for ComposedAskProvider {
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
            let mut dispatches = self
                .dispatches
                .lock()
                .map_err(|_| ModelProviderFailure::Unavailable)?;
            let (text, sources): (&str, &[u8]) = match dispatches.len() {
                0 => (REPORT_QUOTE, &[5]),
                _ => return Err(ModelProviderFailure::Rejected),
            };
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

async fn exercise_q02(fixture: &Fixture) -> TestResult {
    let project = &fixture.project;
    let (store, _, _, published) = index_fixture(fixture).await?;
    let graph = published.publication().graph();
    let create = symbol(graph.symbols(), "domain.py", "create_task")?;
    let schedule = symbol(graph.symbols(), "service.py", "schedule")?;
    let post = symbol(graph.symbols(), "api.py", "post")?;
    let restore = symbol(graph.symbols(), "backfill.py", "restore")?;
    let report = symbol(graph.symbols(), "reporting.py", "create_task_report")?;
    assert_eq!(
        create.revision().content_hash().as_bytes(),
        blake3::hash(DOMAIN.as_bytes()).as_bytes()
    );
    let query = TraversalQuery::new(
        GraphEndpoint::Symbol(create.id()),
        TraversalDirection::Incoming,
        SyntaxRelationKind::Calls,
        TraversalDepth::INTERACTIVE_MAX,
        TraversalResultLimit::DEFAULT,
    );
    let result = store
        .traverse_graph(project, &query, &ActiveControl)
        .await?;
    assert_eq!(result.index_run_id(), published.run().id());
    let snapshot = published.run().snapshot_id();
    assert_eq!(result.snapshot_id(), snapshot);
    assert_eq!(result.query(), &query);
    assert!(!result.truncated());
    assert_eq!(result.hits().len(), 3);
    assert!(
        result
            .hits()
            .iter()
            .all(|hit| hit.source_channel() == SourceChannel::Graph)
    );
    assert!(!result.hits().iter().any(|hit| {
        matches!(hit.target(), GraphTraversalTarget::Symbol(target) if target.symbol() == report)
    }));

    let [service_edge] = caller(result.hits(), schedule)?.path() else {
        return Err("schedule was not the direct caller".into());
    };
    let [via_service_from_api, api_edge] = caller(result.hits(), post)?.path() else {
        return Err("post was not a two-hop caller".into());
    };
    let [via_service_from_backfill, backfill_edge] = caller(result.hits(), restore)?.path() else {
        return Err("restore was not a two-hop caller".into());
    };
    assert_eq!(via_service_from_api, service_edge);
    assert_eq!(via_service_from_backfill, service_edge);
    assert_edge(
        project,
        service_edge,
        schedule,
        create,
        SERVICE,
        "persist_task",
        snapshot,
    )
    .await?;
    assert_edge(project, api_edge, post, schedule, API, "schedule", snapshot).await?;
    assert_edge(
        project,
        backfill_edge,
        restore,
        schedule,
        BACKFILL,
        "schedule",
        snapshot,
    )
    .await?;
    Ok(())
}

fn symbol<'a>(symbols: &'a [GraphSymbol], path: &str, name: &str) -> TestResult<&'a GraphSymbol> {
    let mut matches = symbols.iter().filter(|symbol| {
        symbol.revision().path().as_bytes() == path.as_bytes()
            && symbol.parsed().name().as_str() == name
    });
    let found = matches
        .next()
        .ok_or("fixture declaration missing from current index")?;
    assert!(matches.next().is_none());
    Ok(found)
}

fn caller<'a>(
    hits: &'a [GraphTraversalHit],
    symbol: &GraphSymbol,
) -> TestResult<&'a GraphTraversalHit> {
    hits.iter().find(|hit| {
        matches!(hit.target(), GraphTraversalTarget::Symbol(target) if target.symbol() == symbol)
    }).ok_or_else(|| "expected current caller missing".into())
}

async fn assert_edge(
    project: &ProjectIdentity,
    edge: &GraphEdge,
    from: &GraphSymbol,
    to: &GraphSymbol,
    expected_file: &str,
    call_name: &str,
    snapshot: SnapshotId,
) -> TestResult {
    assert_eq!(edge.snapshot_id(), snapshot);
    assert_eq!(edge.kind(), SyntaxRelationKind::Calls);
    assert_eq!(edge.source(), &GraphEndpoint::Symbol(from.id()));
    assert_eq!(edge.target(), &GraphEndpoint::Symbol(to.id()));
    let evidence = edge.evidence();
    assert_eq!(evidence.revision(), from.revision());
    assert_eq!(
        evidence.revision().content_hash().as_bytes(),
        blake3::hash(expected_file.as_bytes()).as_bytes()
    );
    let span = evidence.range();
    let first_line = span
        .start_position()
        .row()
        .checked_add(1)
        .ok_or("invalid call line")?;
    let request = AgentFileInspection::new(
        evidence.revision().path().clone(),
        AgentFileStartLine::new(first_line)?,
        AgentFileLineCount::new(1)?,
    );
    let page = WorkspaceAgentSourceReader
        .read_page(project, evidence.revision(), &request, &ActiveControl)
        .await?;
    assert_eq!(page.revision(), evidence.revision());
    assert!(page.range().contains(span));
    assert!(!page.truncated());
    let base = page.range().start_byte();
    let start = usize::try_from(span.start_byte() - base)?;
    let end = usize::try_from(span.end_byte() - base)?;
    assert_eq!(page.text().get(start..end), Some(call_name));
    assert_eq!(
        page.text(),
        expected_file
            .split_inclusive('\n')
            .nth(usize::try_from(first_line - 1)?)
            .ok_or("expected call line missing in frozen fixture")?
    );
    Ok(())
}

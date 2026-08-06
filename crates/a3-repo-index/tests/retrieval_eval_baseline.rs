//! Versioned, offline end-to-end retrieval quality baseline for Gate M4/M5.

mod support;

use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore,
    KnowledgeSearchControl, KnowledgeSearchStore, KnowledgeStore, RefreshRepositoryIndex,
    RepositoryChangeBatch, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryRescanReason,
};
use a3_domain::{
    ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, ExactSearchRole, ExactSearchTarget,
    ExactSearchTerm, GraphEndpoint, GraphTraversalResult, LexicalSearchPage, LexicalSearchPageSize,
    LexicalSearchQuery, LexicalSearchTerm, Progress, PublishedIndex, SourceChannel, TraversalQuery,
    TraversalResultLimit,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::fmt::Write;
use std::sync::Arc;
use support::{TempDirectory, run_libsql_test};

const BASELINE_SCHEMA_VERSION: u16 = 1;
const TOP_K: u16 = 5;
const EXPECTED_BASELINE: &str = include_str!("fixtures/retrieval_eval_v1.golden");

const FIXTURE_FILES: &[(&str, &[u8])] = &[
    (
        "Cargo.toml",
        include_bytes!("../../../fixtures/graph-linker/Cargo.toml"),
    ),
    (
        "src/lib.rs",
        include_bytes!("../../../fixtures/graph-linker/src/lib.rs"),
    ),
    (
        "src/main.rs",
        include_bytes!("../../../fixtures/graph-linker/src/main.rs"),
    ),
    (
        "src/service.rs",
        include_bytes!("../../../fixtures/graph-linker/src/service.rs"),
    ),
    (
        "web/main.ts",
        include_bytes!("../../../fixtures/graph-linker/web/main.ts"),
    ),
    (
        "web/helper.ts",
        include_bytes!("../../../fixtures/graph-linker/web/helper.ts"),
    ),
    (
        "python/pyproject.toml",
        include_bytes!("../../../fixtures/graph-linker/python/pyproject.toml"),
    ),
    (
        "python/sample/__init__.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/__init__.py"),
    ),
    (
        "python/sample/base.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/base.py"),
    ),
    (
        "python/sample/service.py",
        include_bytes!("../../../fixtures/graph-linker/python/sample/service.py"),
    ),
    (
        "python/tests/test_service.py",
        include_bytes!("../../../fixtures/graph-linker/python/tests/test_service.py"),
    ),
];

#[derive(Debug)]
struct EvaluationControl;

impl RepositoryIndexControl for EvaluationControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl KnowledgeSearchControl for EvaluationControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl IndexPersistenceControl for EvaluationControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedHit {
    target: String,
    channel: SourceChannel,
    explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaseResult {
    id: &'static str,
    query: &'static str,
    channel: SourceChannel,
    expected: Vec<&'static str>,
    ranks: Vec<usize>,
    explanations: Vec<String>,
}

impl CaseResult {
    fn evaluate(
        id: &'static str,
        query: &'static str,
        channel: SourceChannel,
        expected: Vec<&'static str>,
        observed: &[ObservedHit],
    ) -> Result<Self, Box<dyn Error>> {
        let mut ranks = Vec::with_capacity(expected.len());
        let mut explanations = Vec::with_capacity(expected.len());
        for target in &expected {
            let (index, hit) = observed
                .iter()
                .enumerate()
                .find(|(_, hit)| hit.target == *target)
                .ok_or_else(|| format!("case {id} did not retrieve expected target {target}"))?;
            let rank = index + 1;
            if rank > usize::from(TOP_K) {
                return Err(format!(
                    "case {id} ranked expected target {target} at {rank}, outside top {TOP_K}"
                )
                .into());
            }
            if hit.channel != channel {
                return Err(format!(
                    "case {id} returned {target} through {:?}, expected {channel:?}",
                    hit.channel
                )
                .into());
            }
            ranks.push(rank);
            explanations.push(hit.explanation.clone());
        }
        Ok(Self {
            id,
            query,
            channel,
            expected,
            ranks,
            explanations,
        })
    }
}

#[test]
fn retrieval_eval_v1_matches_the_reviewed_offline_baseline() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        for (path, source) in FIXTURE_FILES {
            repository.write(path, source)?;
        }
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;

        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let index_store: Arc<dyn KnowledgeIndexStore> = store.clone();
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            index_store,
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(2)?)?;
        let indexed = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &EvaluationControl,
            )
            .await?;
        if !indexed.published() {
            return Err("retrieval eval fixture was not published".into());
        }
        let published = store
            .latest_published_index(&project, &EvaluationControl)
            .await?
            .ok_or("published retrieval eval index is missing")?;
        if published.run().snapshot_id() != indexed.snapshot().id() {
            return Err("retrieval eval read a different snapshot than it published".into());
        }

        let first = evaluate_v1(store.as_ref(), &project, &published).await?;
        let second = evaluate_v1(store.as_ref(), &project, &published).await?;
        if first != second {
            return Err(
                "retrieval eval produced different normalized results on repetition".into(),
            );
        }
        if first != EXPECTED_BASELINE {
            return Err(format!(
                "retrieval eval baseline differs\n--- expected ---\n{EXPECTED_BASELINE}--- actual ---\n{first}"
            )
            .into());
        }
        Ok(())
    })
}

async fn evaluate_v1(
    store: &LibsqlKnowledgeStore,
    project: &a3_domain::ProjectIdentity,
    published: &PublishedIndex,
) -> Result<String, Box<dyn Error>> {
    let exact_launch = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("launch".to_owned())?),
            ExactSearchPageSize::new(TOP_K)?,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_exact_page(&exact_launch, published)?;

    let lexical_launch = store
        .search_lexical(
            project,
            &LexicalSearchQuery::new(LexicalSearchTerm::try_from_string("luanch".to_owned())?),
            LexicalSearchPageSize::new(TOP_K)?,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_lexical_page(&lexical_launch, published)?;

    let python_base_service = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("BaseService".to_owned())?),
            ExactSearchPageSize::new(TOP_K)?,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_exact_page(&python_base_service, published)?;

    let manifests = store
        .search_exact(
            project,
            &ExactSearchQuery::Role(ExactSearchRole::Manifest),
            ExactSearchPageSize::new(TOP_K)?,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_exact_page(&manifests, published)?;

    let rust_module = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("lib".to_owned())?),
            ExactSearchPageSize::DEFAULT,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_exact_page(&rust_module, published)?;
    let rust_module_id = symbol_id(&rust_module, "symbol:src/lib.rs::lib")?;

    let rust_imports = store
        .traverse_graph(
            project,
            &TraversalQuery::imports(
                GraphEndpoint::Symbol(rust_module_id),
                TraversalResultLimit::new(TOP_K)?,
            ),
            &EvaluationControl,
        )
        .await?;
    validate_graph_result(&rust_imports, published)?;

    let typescript_module = store
        .search_exact(
            project,
            &ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("main".to_owned())?),
            ExactSearchPageSize::DEFAULT,
            None,
            &EvaluationControl,
        )
        .await?;
    validate_exact_page(&typescript_module, published)?;
    let typescript_module_id = symbol_id(&typescript_module, "symbol:web/main.ts::main")?;

    let typescript_imports = store
        .traverse_graph(
            project,
            &TraversalQuery::imports(
                GraphEndpoint::Symbol(typescript_module_id),
                TraversalResultLimit::new(TOP_K)?,
            ),
            &EvaluationControl,
        )
        .await?;
    validate_graph_result(&typescript_imports, published)?;

    let cases = vec![
        CaseResult::evaluate(
            "symbol_exact",
            "symbol:launch",
            SourceChannel::Exact,
            vec!["symbol:src/lib.rs::launch"],
            &observe_exact(&exact_launch)?,
        )?,
        CaseResult::evaluate(
            "symbol_typo",
            "lexical:luanch",
            SourceChannel::Lexical,
            vec!["symbol:src/lib.rs::launch"],
            &observe_lexical(&lexical_launch)?,
        )?,
        CaseResult::evaluate(
            "python_symbol_exact",
            "symbol:BaseService",
            SourceChannel::Exact,
            vec!["symbol:python/sample/base.py::BaseService"],
            &observe_exact(&python_base_service)?,
        )?,
        CaseResult::evaluate(
            "architecture_manifests",
            "role:manifest",
            SourceChannel::Exact,
            vec!["file:Cargo.toml", "file:python/pyproject.toml"],
            &observe_exact(&manifests)?,
        )?,
        CaseResult::evaluate(
            "rust_dependency",
            "imports:src/lib.rs",
            SourceChannel::Graph,
            vec!["file:src/service.rs"],
            &observe_graph(&rust_imports)?,
        )?,
        CaseResult::evaluate(
            "typescript_dependency",
            "imports:web/main.ts",
            SourceChannel::Graph,
            vec!["file:web/helper.ts"],
            &observe_graph(&typescript_imports)?,
        )?,
    ];
    normalize(&cases, published)
}

fn validate_exact_page(
    page: &ExactSearchPage,
    published: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    validate_publication_binding(page.index_run_id(), page.snapshot_id(), published)?;
    for hit in page.hits() {
        validate_current_target(hit.target(), published)?;
    }
    Ok(())
}

fn validate_lexical_page(
    page: &LexicalSearchPage,
    published: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    validate_publication_binding(page.index_run_id(), page.snapshot_id(), published)?;
    for hit in page.hits() {
        validate_current_target(hit.target(), published)?;
    }
    Ok(())
}

fn validate_graph_result(
    result: &GraphTraversalResult,
    published: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    validate_publication_binding(result.index_run_id(), result.snapshot_id(), published)?;
    for hit in result.hits() {
        validate_current_target(hit.target(), published)?;
        for edge in hit.path() {
            if edge.snapshot_id() != published.run().snapshot_id()
                || !published
                    .publication()
                    .graph()
                    .files()
                    .contains(edge.evidence().revision())
            {
                return Err("graph retrieval returned stale path evidence".into());
            }
        }
    }
    Ok(())
}

fn validate_publication_binding(
    run_id: a3_domain::IndexRunId,
    snapshot_id: a3_domain::SnapshotId,
    published: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    if run_id != published.run().id() || snapshot_id != published.run().snapshot_id() {
        return Err("retrieval result is not bound to the current publication".into());
    }
    Ok(())
}

fn validate_current_target(
    target: &ExactSearchTarget,
    published: &PublishedIndex,
) -> Result<(), Box<dyn Error>> {
    if !published
        .publication()
        .graph()
        .files()
        .contains(target.revision())
    {
        return Err("retrieval result contains a stale file revision".into());
    }
    Ok(())
}

fn observe_exact(page: &ExactSearchPage) -> Result<Vec<ObservedHit>, Box<dyn Error>> {
    page.hits()
        .iter()
        .map(|hit| {
            Ok(ObservedHit {
                target: target_label(hit.target())?,
                channel: hit.source_channel(),
                explanation: format!("{:?}", hit.explanation()),
            })
        })
        .collect()
}

fn observe_lexical(page: &LexicalSearchPage) -> Result<Vec<ObservedHit>, Box<dyn Error>> {
    page.hits()
        .iter()
        .map(|hit| {
            Ok(ObservedHit {
                target: target_label(hit.target())?,
                channel: hit.source_channel(),
                explanation: format!("{:?}", hit.explanation()),
            })
        })
        .collect()
}

fn observe_graph(result: &GraphTraversalResult) -> Result<Vec<ObservedHit>, Box<dyn Error>> {
    result
        .hits()
        .iter()
        .map(|hit| {
            Ok(ObservedHit {
                target: target_label(hit.target())?,
                channel: hit.source_channel(),
                explanation: format!("{:?}/{}hop", result.query().relation(), hit.path().len()),
            })
        })
        .collect()
}

fn symbol_id(
    page: &ExactSearchPage,
    expected_label: &str,
) -> Result<a3_domain::SymbolId, Box<dyn Error>> {
    page.hits()
        .iter()
        .find_map(|hit| match hit.target() {
            ExactSearchTarget::Symbol(symbol)
                if target_label(hit.target()).ok().as_deref() == Some(expected_label) =>
            {
                Some(symbol.symbol().id())
            }
            ExactSearchTarget::File(_) | ExactSearchTarget::Symbol(_) => None,
        })
        .ok_or_else(|| format!("exact retrieval did not find graph seed {expected_label}").into())
}

fn target_label(target: &ExactSearchTarget) -> Result<String, Box<dyn Error>> {
    let path = std::str::from_utf8(target.revision().path().as_bytes())?;
    Ok(match target {
        ExactSearchTarget::File(_) => format!("file:{path}"),
        ExactSearchTarget::Symbol(symbol) => {
            format!(
                "symbol:{path}::{}",
                symbol.symbol().parsed().name().as_str()
            )
        }
    })
}

fn normalize(cases: &[CaseResult], published: &PublishedIndex) -> Result<String, Box<dyn Error>> {
    let expectation_count = cases.iter().map(|case| case.expected.len()).sum::<usize>();
    let mut reciprocal_rank_sum = 0_u64;
    for rank in cases.iter().flat_map(|case| case.ranks.iter()) {
        reciprocal_rank_sum +=
            10_000_u64 / u64::try_from(*rank).map_err(|_| "rank conversion overflow")?;
    }
    let mean_reciprocal_rank = reciprocal_rank_sum
        / u64::try_from(expectation_count).map_err(|_| "expectation count overflow")?;
    let mut output = String::new();
    writeln!(output, "retrieval_eval_schema={BASELINE_SCHEMA_VERSION}")?;
    writeln!(output, "fixture=graph-linker-v1")?;
    writeln!(output, "fixture_digest={}", fixture_digest())?;
    writeln!(
        output,
        "publication files={} symbols={}",
        published.publication().graph().files().len(),
        published.publication().graph().symbols().len()
    )?;
    writeln!(output, "top_k={TOP_K}")?;
    for case in cases {
        let ranks = case
            .ranks
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            output,
            "case id={} query={} channel={:?} expected={} ranks={} explanations={}",
            case.id,
            case.query,
            case.channel,
            case.expected.join(","),
            ranks,
            case.explanations.join(",")
        )?;
    }
    writeln!(
        output,
        "metrics cases={} expectations={} recall_at_{}_bp=10000 mrr_bp={mean_reciprocal_rank}",
        cases.len(),
        expectation_count,
        TOP_K
    )?;
    Ok(output)
}

fn fixture_digest() -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    for (path, source) in FIXTURE_FILES {
        hasher.update(path.as_bytes());
        hasher.update(&[0]);
        hasher.update(source);
        hasher.update(&[u8::MAX]);
    }
    hasher.finalize()
}

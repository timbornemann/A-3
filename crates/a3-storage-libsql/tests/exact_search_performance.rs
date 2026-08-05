//! Reproducible manual R1/R2 fast-retrieval baseline; excluded from default tests.

mod support;

use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore,
    KnowledgeSearchControl, KnowledgeSearchStore,
};
use a3_domain::{
    CanonicalDirectory, Centrality, ContentHash, ExactSearchPageSize, ExactSearchQuery,
    ExactSearchTerm, FileRevision, GitHead, GitReferenceName, GraphSymbol, IndexPublication,
    IndexRunId, IndexRunStart, IndexSchemaVersion, LanguageAdapterRevision, LanguageAdapterVersion,
    LexicalSearchPageSize, LexicalSearchQuery, LexicalSearchTarget, LexicalSearchTerm, LinkedGraph,
    LocalSymbolId, ParsedSymbol, ProjectIdentity, RankProjection, RankScore, RankingPolicyVersion,
    RepositoryId, RepositoryIdentity, RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind,
    SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank,
    SymbolRankSignals, WorktreeAnchorId, WorktreeGeneration, WorktreeId, WorktreeIdentity,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use futures::executor::block_on;
use std::error::Error;
use std::fs;
use std::time::{Duration, Instant};
use support::TempDirectory;

const STRUCTURAL_LINES: usize = 100_000;
const SYMBOL_COUNT: usize = STRUCTURAL_LINES / 2;
const BASELINE_SAMPLES: usize = 5;
const EXACT_SAMPLES: usize = 30;
const LEXICAL_SAMPLES: usize = 30;
const EXACT_P95_TARGET: Duration = Duration::from_millis(100);
const LEXICAL_P95_TARGET: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct SilentControl;

impl KnowledgeSearchControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl IndexPersistenceControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

#[test]
#[ignore = "manual 100,000-structural-line exact/FTS-search P95 baseline"]
fn exact_symbol_search_meets_the_100_millisecond_p95_target() -> Result<(), Box<dyn Error>> {
    block_on(async {
        let temporary = TempDirectory::new()?;
        let app_data = temporary.path().join("app-data");
        let worktree = temporary.path().join("worktree");
        let common = temporary.path().join("common-git");
        fs::create_dir(&worktree)?;
        fs::create_dir(&common)?;
        let layout = StorageLayout::prepare(app_data)?;
        let project = project(&worktree, &common)?;
        let store = LibsqlKnowledgeStore::open(&layout).await?;
        let (snapshot, publication) = fixture(project.worktree().id())?;
        store.append_snapshot(&project, &snapshot).await?;
        let run = store
            .start_index_run(
                &project,
                IndexRunStart::new(
                    IndexRunId::from_bytes([9; 32]),
                    snapshot.id(),
                    RankingPolicyVersion::v1(),
                ),
            )
            .await?;
        store
            .publish_index(&project, run.id(), &publication, &SilentControl)
            .await?;

        let target = format!("function_{:05}", SYMBOL_COUNT - 1);
        let query = ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string(target.clone())?);
        let lexical_query = LexicalSearchQuery::new(LexicalSearchTerm::try_from_string(format!(
            "function_{:04}x",
            (SYMBOL_COUNT - 1) / 10
        ))?);
        let _warm_exact = store
            .search_exact(
                &project,
                &query,
                ExactSearchPageSize::DEFAULT,
                None,
                &SilentControl,
            )
            .await?;
        let _warm_lexical = store
            .search_lexical(
                &project,
                &lexical_query,
                LexicalSearchPageSize::DEFAULT,
                None,
                &SilentControl,
            )
            .await?;
        let _warm_baseline = store
            .latest_published_index(&project, &SilentControl)
            .await?
            .ok_or("published benchmark index is missing")?;

        let mut baseline_samples = Vec::with_capacity(BASELINE_SAMPLES);
        for _ in 0..BASELINE_SAMPLES {
            let started = Instant::now();
            let published = store
                .latest_published_index(&project, &SilentControl)
                .await?
                .ok_or("published benchmark index is missing")?;
            let found = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .any(|symbol| symbol.parsed().name().as_str() == target);
            baseline_samples.push(started.elapsed());
            assert!(found);
        }

        let mut exact_samples = Vec::with_capacity(EXACT_SAMPLES);
        for _ in 0..EXACT_SAMPLES {
            let started = Instant::now();
            let page = store
                .search_exact(
                    &project,
                    &query,
                    ExactSearchPageSize::DEFAULT,
                    None,
                    &SilentControl,
                )
                .await?;
            exact_samples.push(started.elapsed());
            assert_eq!(page.hits().len(), 1);
        }
        let mut lexical_samples = Vec::with_capacity(LEXICAL_SAMPLES);
        for _ in 0..LEXICAL_SAMPLES {
            let started = Instant::now();
            let page = store
                .search_lexical(
                    &project,
                    &lexical_query,
                    LexicalSearchPageSize::DEFAULT,
                    None,
                    &SilentControl,
                )
                .await?;
            lexical_samples.push(started.elapsed());
            assert!(page.hits().iter().any(|hit| matches!(
                hit.target(),
                LexicalSearchTarget::Symbol(symbol)
                    if symbol.symbol().parsed().name().as_str() == target
            )));
        }
        baseline_samples.sort_unstable();
        exact_samples.sort_unstable();
        lexical_samples.sort_unstable();
        let baseline_p50 = baseline_samples[BASELINE_SAMPLES / 2];
        let baseline_p95 = baseline_samples[percentile_index(BASELINE_SAMPLES)];
        let exact_p50 = exact_samples[EXACT_SAMPLES / 2];
        let exact_p95 = exact_samples[percentile_index(EXACT_SAMPLES)];
        let lexical_p50 = lexical_samples[LEXICAL_SAMPLES / 2];
        let lexical_p95 = lexical_samples[percentile_index(LEXICAL_SAMPLES)];
        println!(
            "A^3 fast-search baseline: {STRUCTURAL_LINES} structural lines, {SYMBOL_COUNT} symbols; pre-retrieval full-index-load scan {BASELINE_SAMPLES} samples P50={baseline_p50:?}, P95={baseline_p95:?}; indexed exact retrieval {EXACT_SAMPLES} samples P50={exact_p50:?}, P95={exact_p95:?}; typo-tolerant FTS retrieval {LEXICAL_SAMPLES} samples P50={lexical_p50:?}, P95={lexical_p95:?}"
        );
        assert!(
            lexical_p95 <= LEXICAL_P95_TARGET,
            "lexical-search P95 {lexical_p95:?} exceeded {LEXICAL_P95_TARGET:?}"
        );
        assert!(
            exact_p95 <= EXACT_P95_TARGET,
            "exact-search P95 {exact_p95:?} exceeded {EXACT_P95_TARGET:?}"
        );
        Ok::<(), Box<dyn Error>>(())
    })
}

fn percentile_index(sample_count: usize) -> usize {
    sample_count
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1)
}

fn project(
    root: &std::path::Path,
    common: &std::path::Path,
) -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(root.canonicalize()?)?;
    let common = CanonicalDirectory::from_canonicalized(common.canonicalize()?)?;
    let repository_id = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository_id, common, None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository_id,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn fixture(worktree_id: WorktreeId) -> Result<(Snapshot, IndexPublication), Box<dyn Error>> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/benchmark.rs".to_vec())?,
        ContentHash::from_bytes([4; 32]),
    );
    let snapshot_id = SnapshotId::from_bytes([5; 32]);
    let snapshot = Snapshot::new(
        snapshot_id,
        worktree_id,
        None,
        WorktreeGeneration::new(1)?,
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
        IndexSchemaVersion::v3(),
        vec![LanguageAdapterRevision::new(
            a3_domain::IndexLanguage::Rust,
            LanguageAdapterVersion::try_from_string("performance-rust-1".to_owned())?,
        )],
        vec![SnapshotChange::new(
            revision.path().clone(),
            revision.content_hash(),
            SnapshotChangeKind::Upsert,
        )],
    )?;
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let mut symbols = Vec::with_capacity(SYMBOL_COUNT);
    let mut ranks = Vec::with_capacity(SYMBOL_COUNT);
    for index in 0..SYMBOL_COUNT {
        let symbol_id = symbol_id(index)?;
        symbols.push(GraphSymbol::new(
            symbol_id,
            revision.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(u32::try_from(index)?.saturating_add(1))?,
                SymbolKind::Function,
                SymbolName::try_from_string(format!("function_{index:05}"))?,
                range,
                range,
            )?,
        ));
        ranks.push(SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(0)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(0)?,
                degree_contribution: 0,
                centrality_contribution: 0,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        ));
    }
    let graph = LinkedGraph::new(snapshot_id, vec![revision], symbols, Vec::new(), Vec::new())?;
    let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
    Ok((snapshot, IndexPublication::new(graph, ranking)?))
}

fn symbol_id(index: usize) -> Result<SymbolId, Box<dyn Error>> {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&u64::try_from(index)?.to_be_bytes());
    Ok(SymbolId::from_bytes(bytes))
}

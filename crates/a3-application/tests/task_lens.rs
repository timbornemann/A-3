//! R10 ordered read-only Task Lens orchestration contracts.

use a3_application::{
    CompileTaskLens, CompileTaskLensFailure, IndexPersistenceControl, KnowledgeIndexFailure,
    KnowledgeIndexFuture, KnowledgeIndexStore, KnowledgeSearchControl, KnowledgeSearchFailure,
    KnowledgeSearchFuture, KnowledgeSearchStore, TaskLensClaimLimit, TaskLensClaimReadFuture,
    TaskLensClaimResult, TaskLensClaimStore, TaskLensClaimStoreFailure, TaskLensClaimStoreFuture,
    TaskLensControl, TaskLensControlError, TaskLensIndexStore, TaskLensIndexStoreFuture,
    TaskLensSemanticHit, TaskLensSemanticLimit, TaskLensSemanticResult, TaskLensSemanticSearch,
    TaskLensSemanticSearchFailure, TaskLensSemanticSearchFuture, TaskLensTimeout,
};
use a3_domain::{
    CanonicalDirectory, Centrality, Confidence, ContentHash, EvidenceRef, ExactSearchCursor,
    ExactSearchExplanation, ExactSearchHit, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery,
    ExactSearchSymbol, ExactSearchTarget, FileRevision, GitHead, GitReferenceName, GraphEdge,
    GraphEndpoint, GraphSymbol, GraphTraversalHit, GraphTraversalResult, IndexLanguage,
    IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStart, IndexRunStatus,
    IndexRunTerminalOutcome, LexicalScore, LexicalSearchCursor, LexicalSearchExplanation,
    LexicalSearchHit, LexicalSearchPage, LexicalSearchPageSize, LexicalSearchQuery, LinkResolution,
    LinkedGraph, LocalSymbolId, ModuleCardClaimId, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleClaimStatement, ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
    ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, NormalizedRetrievalSignal,
    ParsedSymbol, Progress, ProjectIdentity, PublishedIndex, QualifiedSymbolName, RankProjection,
    RankScore, RankingPolicyVersion, RepositoryCard, RepositoryFileState, RepositoryId,
    RepositoryIdentity, RepositoryModule, RepositoryPath, Snapshot, SnapshotId, SourceChannel,
    SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals,
    SymbolRole, SyntaxProvider, SyntaxRelationKind, TaskLensClaim, TaskLensSeed, TaskLensSeedSet,
    TaskLensSeedText, TaskLensTarget, TaskLensTokenBudget, TraversalQuery, VerifiedClaimKind,
    VerifiedClaimStatus, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use futures::executor::block_on;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};

#[test]
fn channels_run_in_order_and_claims_are_packed_before_optional_semantic_candidates()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published.clone(),
        production: fixture.production,
        test: fixture.test,
        irrelevant: fixture.irrelevant,
        calls: &calls,
    };
    let control = RecordingControl::default();
    let lens = block_on(
        CompileTaskLens::new(&store, &store, &store)
            .with_semantic(&store)
            .execute(
                &project()?,
                TaskLensSeedSet::new(
                    seed("fix broken parser")?,
                    seed("change production and regression test")?,
                    vec![
                        TaskLensSeed::ExplicitIdentifier(seed("broken")?),
                        TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
                    ],
                )?,
                TaskLensTokenBudget::new(900)?,
                &control,
            ),
    )?;

    let calls = calls
        .lock()
        .map_err(|_| TestError("call log lock was poisoned"))?;
    assert_eq!(calls.first(), Some(&"index"));
    let lexical = position(&calls, "lexical")?;
    let claims = position(&calls, "claims")?;
    let semantic = position(&calls, "semantic")?;
    assert!(calls[1..lexical].iter().all(|call| *call == "exact"));
    assert!(
        calls[lexical + 1..claims]
            .iter()
            .all(|call| matches!(*call, "graph" | "test"))
    );
    assert!(lexical < claims && claims < semantic);
    drop(calls);

    let progress = control
        .progress
        .lock()
        .map_err(|_| TestError("progress lock was poisoned"))?;
    assert_eq!(progress.len(), 8);
    assert_eq!(
        progress.first().and_then(|value| value.completed()),
        Some(0)
    );
    assert_eq!(progress.last().map(|value| value.is_complete()), Some(true));
    drop(progress);

    assert!(lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Symbol(symbol) if symbol.id() == fixture.production
    )));
    assert!(lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Symbol(symbol) if symbol.id() == fixture.test
    )));
    assert!(!lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Symbol(symbol) if symbol.id() == fixture.irrelevant
    )));
    assert_eq!(lens.claims().len(), 1);
    assert_eq!(lens.claims()[0].kind(), VerifiedClaimKind::Hypothesis);
    assert!(lens.truncated());
    Ok(())
}

#[test]
fn cancellation_stops_before_any_read() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published,
        production: fixture.production,
        test: fixture.test,
        irrelevant: fixture.irrelevant,
        calls: &calls,
    };
    let control = RecordingControl {
        cancelled: true,
        progress: Mutex::new(Vec::new()),
    };
    let result = block_on(CompileTaskLens::new(&store, &store, &store).execute(
        &project()?,
        TaskLensSeedSet::new(seed("goal")?, seed("step")?, Vec::new())?,
        TaskLensTokenBudget::DEFAULT,
        &control,
    ));
    assert!(matches!(
        result,
        Err(a3_application::CompileTaskLensFailure::Cancelled)
    ));
    assert!(
        calls
            .lock()
            .map_err(|_| TestError("call log lock was poisoned"))?
            .is_empty()
    );
    Ok(())
}

#[test]
fn whole_operation_deadline_is_propagated_into_the_index_port() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let calls = Mutex::new(Vec::new());
    let store = StubStore {
        published: fixture.published,
        production: fixture.production,
        test: fixture.test,
        irrelevant: fixture.irrelevant,
        calls: &calls,
    };
    let result = block_on(
        CompileTaskLens::new(&DeadlineIndex, &store, &store)
            .with_timeout(TaskLensTimeout::from_millis(1)?)
            .execute(
                &project()?,
                TaskLensSeedSet::new(seed("goal")?, seed("step")?, Vec::new())?,
                TaskLensTokenBudget::DEFAULT,
                &RecordingControl::default(),
            ),
    );

    assert!(matches!(result, Err(CompileTaskLensFailure::TimedOut)));
    assert!(
        calls
            .lock()
            .map_err(|_| TestError("call log lock was poisoned"))?
            .is_empty()
    );
    Ok(())
}

#[derive(Debug, Default)]
struct RecordingControl {
    cancelled: bool,
    progress: Mutex<Vec<Progress>>,
}

impl TaskLensControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn report_progress(&self, progress: Progress) -> Result<(), TaskLensControlError> {
        self.progress
            .lock()
            .map_err(|_| TaskLensControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

#[derive(Debug)]
struct StubStore<'a> {
    published: PublishedIndex,
    production: SymbolId,
    test: SymbolId,
    irrelevant: SymbolId,
    calls: &'a Mutex<Vec<&'static str>>,
}

impl StubStore<'_> {
    fn record(&self, call: &'static str) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
    }

    fn exact_target(
        &self,
        symbol_id: SymbolId,
    ) -> Result<ExactSearchTarget, KnowledgeSearchFailure> {
        let symbol = self
            .published
            .publication()
            .graph()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == symbol_id)
            .cloned()
            .ok_or(KnowledgeSearchFailure::InvalidStoredProjection)?;
        let name = QualifiedSymbolName::try_from_string(symbol.parsed().name().as_str().to_owned())
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
        Ok(ExactSearchTarget::Symbol(ExactSearchSymbol::new(
            symbol, name,
        )))
    }
}

impl KnowledgeIndexStore for StubStore<'_> {
    fn append_snapshot<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _snapshot: &'a Snapshot,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotConflict) })
    }

    fn latest_snapshot<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
    }

    fn current_file_state<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, RepositoryFileState> {
        Box::pin(async { Err(KnowledgeIndexFailure::SnapshotNotFound) })
    }

    fn start_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _request: IndexRunStart,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn finish_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _run_id: IndexRunId,
        _outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn publish_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _run_id: IndexRunId,
        _publication: &'a IndexPublication,
        _control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async { Err(KnowledgeIndexFailure::InvalidIndexRunTransition) })
    }

    fn latest_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn latest_published_index_run<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async { Err(KnowledgeIndexFailure::IndexRunNotFound) })
    }

    fn latest_published_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>> {
        self.record("index");
        let published = self.published.clone();
        Box::pin(async move { Ok(Some(published)) })
    }

    fn rebuild_regenerable_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async { Err(KnowledgeIndexFailure::InvalidIndexRunTransition) })
    }
}

impl TaskLensIndexStore for StubStore<'_> {
    fn load_current_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a> {
        self.record("index");
        let published = Arc::new(self.published.clone());
        Box::pin(async move { Ok(Some(published)) })
    }
}

#[derive(Debug)]
struct DeadlineIndex;

impl TaskLensIndexStore for DeadlineIndex {
    fn load_current_index<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a> {
        Box::pin(async move {
            while !control.is_cancelled() {
                std::hint::spin_loop();
            }
            Err(KnowledgeIndexFailure::Cancelled)
        })
    }
}

impl KnowledgeSearchStore for StubStore<'_> {
    fn search_exact<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _query: &'a ExactSearchQuery,
        page_size: ExactSearchPageSize,
        _cursor: Option<&'a ExactSearchCursor>,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
        self.record("exact");
        let result = self.exact_target(self.production).and_then(|target| {
            let ExactSearchTarget::Symbol(symbol) = target else {
                return Err(KnowledgeSearchFailure::InvalidStoredProjection);
            };
            let hit = ExactSearchHit::symbol(symbol, ExactSearchExplanation::SymbolNameExact)
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
            ExactSearchPage::new(
                self.published.run().id(),
                self.published.run().snapshot_id(),
                vec![hit],
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
        });
        Box::pin(async move { result })
    }

    fn search_lexical<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _query: &'a LexicalSearchQuery,
        page_size: LexicalSearchPageSize,
        _cursor: Option<&'a LexicalSearchCursor>,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
        self.record("lexical");
        let result = self.exact_target(self.production).and_then(|target| {
            let ExactSearchTarget::Symbol(symbol) = target else {
                return Err(KnowledgeSearchFailure::InvalidStoredProjection);
            };
            LexicalSearchPage::new(
                self.published.run().id(),
                self.published.run().snapshot_id(),
                vec![LexicalSearchHit::symbol(
                    symbol,
                    LexicalSearchExplanation::SymbolName,
                    LexicalScore::new(90_000)
                        .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?,
                )],
                None,
                page_size,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
        });
        Box::pin(async move { result })
    }

    fn traverse_graph<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        query: &'a TraversalQuery,
        _control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
        let is_test = query.source_channel() == SourceChannel::Test;
        self.record(if is_test { "test" } else { "graph" });
        let result = if is_test {
            self.exact_target(self.test).and_then(|target| {
                let edge = self
                    .published
                    .publication()
                    .graph()
                    .edges()
                    .iter()
                    .find(|edge| edge.kind() == SyntaxRelationKind::Tests)
                    .cloned()
                    .ok_or(KnowledgeSearchFailure::InvalidStoredProjection)?;
                let hit = GraphTraversalHit::new(
                    target,
                    vec![edge],
                    query,
                    self.published.run().snapshot_id(),
                )
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
                GraphTraversalResult::new(
                    self.published.run().id(),
                    self.published.run().snapshot_id(),
                    query.clone(),
                    vec![hit],
                    false,
                )
                .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
            })
        } else {
            GraphTraversalResult::new(
                self.published.run().id(),
                self.published.run().snapshot_id(),
                query.clone(),
                Vec::new(),
                false,
            )
            .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)
        };
        Box::pin(async move { result })
    }
}

impl TaskLensClaimStore for StubStore<'_> {
    fn load_claims<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        _limit: TaskLensClaimLimit,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimStoreFuture<'a> {
        self.record("claims");
        let module_id = published.publication().modules().modules()[0].id();
        let claim = (|| {
            let statement = ModuleClaimStatement::try_from_string(
                "production and its regression test evolve together".to_owned(),
            )
            .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)?;
            let confidence = Confidence::from_basis_points(5_000)
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)?;
            TaskLensClaim::new(
                published.run().id(),
                published.run().snapshot_id(),
                ModuleCardClaimId::from_bytes([91; 32]),
                module_id,
                ModuleClaimPolarity::Affirms,
                ModuleClaimPredicate::ArchitecturalIntent(statement),
                VerifiedClaimKind::Hypothesis,
                VerifiedClaimStatus::Active,
                confidence,
                Vec::new(),
            )
            .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)
        })();
        Box::pin(async move {
            let claim = claim?;
            TaskLensClaimResult::new(vec![claim], false)
                .map_err(|_| TaskLensClaimStoreFailure::InvalidStoredProjection)
        })
    }

    fn load_claim<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _published: &'a PublishedIndex,
        _claim_id: ModuleCardClaimId,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimReadFuture<'a> {
        Box::pin(async { Ok(None) })
    }
}

impl TaskLensSemanticSearch for StubStore<'_> {
    fn search<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        _seeds: &'a TaskLensSeedSet,
        _limit: TaskLensSemanticLimit,
        _control: &'a dyn TaskLensControl,
    ) -> TaskLensSemanticSearchFuture<'a> {
        self.record("semantic");
        let result = self
            .exact_target(self.irrelevant)
            .map_err(|_| TaskLensSemanticSearchFailure::InvalidResult)
            .and_then(|target| {
                TaskLensSemanticResult::new(
                    published.run().id(),
                    published.run().snapshot_id(),
                    vec![TaskLensSemanticHit::new(
                        target,
                        NormalizedRetrievalSignal::FULL,
                    )],
                    false,
                )
                .map_err(|_| TaskLensSemanticSearchFailure::InvalidResult)
            });
        Box::pin(async move { result })
    }
}

struct Fixture {
    published: PublishedIndex,
    production: SymbolId,
    test: SymbolId,
    irrelevant: SymbolId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let snapshot_id = SnapshotId::from_bytes([1; 32]);
        let production_revision = revision("src/bug.rs", 10)?;
        let test_revision = revision("tests/bug_test.rs", 20)?;
        let irrelevant_revision = revision("vendor/huge.rs", 30)?;
        let production = SymbolId::from_bytes([11; 32]);
        let test = SymbolId::from_bytes([12; 32]);
        let irrelevant = SymbolId::from_bytes([13; 32]);
        let production_symbol = symbol(production, production_revision.clone(), "broken", false)?;
        let test_symbol = symbol(test, test_revision.clone(), "regression", true)?;
        let irrelevant_symbol = symbol(
            irrelevant,
            irrelevant_revision.clone(),
            "generated_vendor_blob",
            false,
        )?;
        let range = production_symbol.parsed().declaration_range();
        let test_edge = GraphEdge::new(
            GraphEndpoint::Symbol(test),
            GraphEndpoint::Symbol(production),
            SyntaxRelationKind::Tests,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterLocalSymbol,
            snapshot_id,
            EvidenceRef::new(test_revision.clone(), range),
        );
        let graph = LinkedGraph::new(
            snapshot_id,
            vec![
                production_revision.clone(),
                test_revision.clone(),
                irrelevant_revision.clone(),
            ],
            vec![production_symbol, test_symbol, irrelevant_symbol],
            vec![test_edge],
            Vec::new(),
        )?;
        let ranking = RankProjection::new(
            snapshot_id,
            RankingPolicyVersion::v1(),
            vec![
                rank(production, 3_000)?,
                rank(test, 2_000)?,
                rank(irrelevant, 1_000)?,
            ],
        )?;
        let relevant_module = ModuleId::from_bytes([31; 32]);
        let irrelevant_module = ModuleId::from_bytes([32; 32]);
        let modules = ModuleProjection::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![
                RepositoryModule::new(
                    relevant_module,
                    ModuleKind::PathBoundary,
                    Some(ModuleRoot::Repository),
                    Vec::new(),
                    ModuleSymbolSet::new(vec![production], false)?,
                    ModuleSymbolSet::empty(),
                    ModuleSymbolSet::new(vec![test], false)?,
                )?,
                RepositoryModule::new(
                    irrelevant_module,
                    ModuleKind::PathBoundary,
                    Some(ModuleRoot::Directory(path("vendor")?)),
                    Vec::new(),
                    ModuleSymbolSet::new(vec![irrelevant], false)?,
                    ModuleSymbolSet::empty(),
                    ModuleSymbolSet::empty(),
                )?,
            ],
            vec![
                ModuleMembership::new(
                    relevant_module,
                    production,
                    ModuleMembershipEvidence::path(production_revision),
                ),
                ModuleMembership::new(
                    relevant_module,
                    test,
                    ModuleMembershipEvidence::path(test_revision),
                ),
                ModuleMembership::new(
                    irrelevant_module,
                    irrelevant,
                    ModuleMembershipEvidence::path(irrelevant_revision),
                ),
            ],
            RepositoryCard::new(
                snapshot_id,
                ModulePolicyVersion::v1(),
                vec![relevant_module, irrelevant_module],
                vec![IndexLanguage::Rust],
                ModuleSymbolSet::empty(),
                3,
                3,
            )?,
        )?;
        let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([100; 32]),
            snapshot_id,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(Self {
            published: PublishedIndex::new(run, publication)?,
            production,
            test,
            irrelevant,
        })
    }
}

fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = std::env::current_dir()?.canonicalize()?;
    let repository_id = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        ),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}

fn symbol(
    id: SymbolId,
    revision: FileRevision,
    name: &str,
    is_test: bool,
) -> Result<GraphSymbol, Box<dyn Error>> {
    let range = SourceRange::new(0, 32, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?;
    let parsed = ParsedSymbol::new(
        LocalSymbolId::new(u32::from(id.as_bytes()[0]))?,
        SymbolKind::Function,
        SymbolName::try_from_string(name.to_owned())?,
        range,
        range,
    )?;
    Ok(GraphSymbol::new(
        id,
        revision,
        if is_test {
            parsed.with_role(SymbolRole::Test)
        } else {
            parsed
        },
    ))
}

fn rank(id: SymbolId, score: u32) -> Result<SymbolRank, Box<dyn Error>> {
    Ok(SymbolRank::new(
        id,
        RankScore::try_from_sum(u64::from(score))?,
        SymbolRankSignals {
            in_degree: 0,
            out_degree: 0,
            centrality: Centrality::from_basis_points(u16::try_from(score)?)?,
            degree_contribution: 0,
            centrality_contribution: score,
            entrypoint_contribution: 0,
            public_export_contribution: 0,
            manifest_contribution: 0,
            test_contribution: 0,
        },
    ))
}

fn revision(value: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        path(value)?,
        ContentHash::from_bytes([hash; 32]),
    ))
}

fn path(value: &str) -> Result<RepositoryPath, Box<dyn Error>> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

fn seed(value: &str) -> Result<TaskLensSeedText, Box<dyn Error>> {
    Ok(TaskLensSeedText::try_from_string(value.to_owned())?)
}

fn position(calls: &[&'static str], target: &'static str) -> Result<usize, Box<dyn Error>> {
    calls
        .iter()
        .position(|call| *call == target)
        .ok_or_else(|| Box::<dyn Error>::from(TestError("expected call is missing")))
}

#[derive(Debug)]
struct TestError(&'static str);

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for TestError {}

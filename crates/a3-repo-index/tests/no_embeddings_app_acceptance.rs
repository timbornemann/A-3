//! Offline acceptance contract for the current Gate M4/M5 application core.

mod support;

use a3_application::{
    CompileTaskLens, EmbeddingExecutionMode, EmbeddingOperationControl, EmbeddingProgressError,
    GenerateSemanticEmbeddings, GenerateSemanticEmbeddingsOutcome, IndexPersistenceControl,
    IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeSearchControl, KnowledgeStore,
    PlanDeepMap, ProjectMapSearchQuery, RefreshRepositoryIndex, RepositoryChangeBatch,
    RepositoryIndexControl, RepositoryIndexControlError, RepositoryRescanReason, SearchProjectMap,
    SemanticEmbeddingJobControl, TaskLensControl, TaskLensControlError,
};
use a3_domain::{
    EmbeddingBatchSize, EmbeddingDimension, EmbeddingModelId, EmbeddingModelProfile,
    EmbeddingProviderId, ExploreBudget, ExplorePlanStopReason, ModuleCardSchemaVersion,
    ModuleCoverageSnapshot, NormalizedSemanticCard, Progress, SemanticCardBatch, SemanticCardId,
    SourceChannel, TaskLensEntryReason, TaskLensSeed, TaskLensSeedSet, TaskLensSeedText,
    TaskLensTarget, TaskLensTokenBudget,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;
use support::{TempDirectory, run_libsql_test};

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
struct OfflineControl;

impl RepositoryIndexControl for OfflineControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl IndexPersistenceControl for OfflineControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

impl TaskLensControl for OfflineControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), TaskLensControlError> {
        Ok(())
    }
}

impl KnowledgeSearchControl for OfflineControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl EmbeddingOperationControl for OfflineControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl SemanticEmbeddingJobControl for OfflineControl {
    fn report_progress(&self, _progress: Progress) -> Result<(), EmbeddingProgressError> {
        Ok(())
    }
}

#[test]
fn current_app_core_runs_end_to_end_without_embeddings() -> Result<(), Box<dyn Error>> {
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
                &OfflineControl,
            )
            .await?;
        if !indexed.published() {
            return Err("no-embeddings acceptance index was not published".into());
        }
        let published = store
            .latest_published_index(&project, &OfflineControl)
            .await?
            .ok_or("no-embeddings acceptance publication is missing")?;
        if published.run().snapshot_id() != indexed.snapshot().id() {
            return Err("no-embeddings acceptance read a different snapshot".into());
        }

        let search = SearchProjectMap::new(store.clone())
            .execute(
                &project,
                &ProjectMapSearchQuery::try_from_string("launch".to_owned())?,
                &OfflineControl,
            )
            .await?;
        if search.index_run_id() != published.run().id()
            || search.snapshot_id() != published.run().snapshot_id()
            || search.hits().is_empty()
        {
            return Err("Project Map search did not retain its current publication".into());
        }
        let launch = search
            .hits()
            .iter()
            .find(|hit| {
                matches!(
                    hit.target(),
                    a3_domain::ExactSearchTarget::Symbol(symbol)
                        if symbol.symbol().parsed().name().as_str() == "launch"
                )
            })
            .ok_or("Project Map search did not find the launch symbol")?;
        let search_channels = launch
            .explanation()
            .sources()
            .iter()
            .map(|source| source.reason().source_channel())
            .collect::<BTreeSet<_>>();
        if !search_channels.contains(&SourceChannel::Exact)
            || !search_channels.contains(&SourceChannel::Lexical)
            || search_channels.contains(&SourceChannel::Semantic)
        {
            return Err(
                "Project Map search did not preserve deterministic source provenance".into(),
            );
        }

        let coverage = ModuleCoverageSnapshot::empty(
            published.run().snapshot_id(),
            ModuleCardSchemaVersion::V1,
        );
        let first_plan =
            PlanDeepMap::version_one().execute(&published, &coverage, ExploreBudget::DEFAULT)?;
        let second_plan =
            PlanDeepMap::version_one().execute(&published, &coverage, ExploreBudget::DEFAULT)?;
        if first_plan != second_plan
            || first_plan.index_run_id() != published.run().id()
            || first_plan.snapshot_id() != published.run().snapshot_id()
            || first_plan.steps().is_empty()
            || first_plan.stop_reason() != ExplorePlanStopReason::CoveragePlanned
        {
            return Err("Deep Map did not complete deterministically without embeddings".into());
        }

        let seeds = TaskLensSeedSet::new(
            TaskLensSeedText::try_from_string("understand the launch path".to_owned())?,
            TaskLensSeedText::try_from_string("inspect the launch service dependency".to_owned())?,
            vec![TaskLensSeed::ExplicitIdentifier(
                TaskLensSeedText::try_from_string("launch".to_owned())?,
            )],
        )?;
        let task_lens = CompileTaskLens::new(store.as_ref(), store.as_ref(), store.as_ref());
        let first_lens = task_lens
            .execute(
                &project,
                seeds.clone(),
                TaskLensTokenBudget::DEFAULT,
                &OfflineControl,
            )
            .await?;
        let second_lens = task_lens
            .execute(
                &project,
                seeds,
                TaskLensTokenBudget::DEFAULT,
                &OfflineControl,
            )
            .await?;
        if first_lens != second_lens || !first_lens.is_current_for(&published) {
            return Err("Task Lens was not deterministic and current without embeddings".into());
        }
        if first_lens.estimated_tokens() > first_lens.token_budget().get() {
            return Err("Task Lens exceeded its token budget".into());
        }
        if !first_lens.entries().iter().any(|entry| {
            matches!(
                entry.target(),
                TaskLensTarget::Symbol(symbol) if symbol.parsed().name().as_str() == "launch"
            )
        }) {
            return Err("Task Lens did not retain the exact launch symbol".into());
        }

        let mut retrieval_channels = BTreeSet::new();
        for entry in first_lens.entries() {
            let TaskLensEntryReason::Retrieval { explanation, .. } = entry.reason() else {
                continue;
            };
            for source in explanation.sources() {
                let channel = source.reason().source_channel();
                if channel == SourceChannel::Semantic {
                    return Err(
                        "Task Lens admitted a semantic source without a semantic port".into(),
                    );
                }
                retrieval_channels.insert(channel);
            }
        }
        if !retrieval_channels.contains(&SourceChannel::Exact)
            || !retrieval_channels.contains(&SourceChannel::Graph)
        {
            return Err("Task Lens did not exercise exact and graph retrieval".into());
        }

        let embeddings = GenerateSemanticEmbeddings::disabled();
        if embeddings.mode() != EmbeddingExecutionMode::Disabled {
            return Err("embedding execution mode was not disabled".into());
        }
        let card = NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([7; 32]),
            published.run().snapshot_id(),
            "graph-linker repository card",
        )?;
        let cards = SemanticCardBatch::new(published.run().snapshot_id(), vec![card])?;
        let unused_profile = EmbeddingModelProfile::v1(
            EmbeddingProviderId::new("disabled".to_owned())?,
            EmbeddingModelId::new("not-loaded".to_owned())?,
            EmbeddingDimension::new(2)?,
            EmbeddingBatchSize::new(1)?,
        );
        let outcome = embeddings
            .execute(&project, &unused_profile, cards, &OfflineControl)
            .await?;
        if outcome != (GenerateSemanticEmbeddingsOutcome::Disabled { card_count: 1 }) {
            return Err("disabled embedding use case did not skip the complete card batch".into());
        }

        Ok(())
    })
}

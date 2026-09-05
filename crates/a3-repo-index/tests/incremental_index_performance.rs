//! Reproducible manual S11 P95 baseline from watcher observation to atomic publication.

mod support;

use a3_application::{
    KnowledgeIndexStore, KnowledgeStore, RefreshRepositoryIndex, RepositoryChangeBatch,
    RepositoryIndexControl, RepositoryIndexControlError, RepositoryIndexMode,
    RepositoryRescanReason,
};
use a3_domain::Progress;
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize, PollingRepositoryWatcher, RepositoryWatcherConfig,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use futures::executor::block_on;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::TempDirectory;

const FILE_COUNT: usize = 200;
const LINES_PER_FILE: usize = 500;
const SAMPLE_COUNT: usize = 30;
const P95_TARGET: Duration = Duration::from_secs(2);

#[derive(Debug)]
struct SilentControl;

impl RepositoryIndexControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

#[test]
#[ignore = "manual 100,000-LOC watcher-to-publish P95 baseline"]
fn one_file_delta_meets_the_two_second_p95_target() -> Result<(), Box<dyn Error>> {
    block_on(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write(
            "Cargo.toml",
            b"[package]\nname='incremental-benchmark'\nversion='0.1.0'\n",
        )?;
        for index in 0..FILE_COUNT {
            repository.write(
                format!("src/file_{index:03}.rs"),
                benchmark_source(index, false).as_bytes(),
            )?;
        }
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let index_store: Arc<dyn KnowledgeIndexStore> = store;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            index_store,
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
                &SilentControl,
            )
            .await?;

        let watcher =
            PollingRepositoryWatcher::start(project.clone(), RepositoryWatcherConfig::v1())?;
        let _initial = watcher
            .next_batch(Duration::from_secs(5))?
            .ok_or("watcher initialization timed out")?;
        let sample_count = std::env::var("A3_INCREMENTAL_PERF_SAMPLES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0 && *value <= SAMPLE_COUNT)
            .unwrap_or(SAMPLE_COUNT);
        let mut samples = Vec::with_capacity(sample_count);
        let mut watcher_samples = Vec::with_capacity(sample_count);
        let mut refresh_samples = Vec::with_capacity(sample_count);
        for sample in 0..sample_count {
            let started = Instant::now();
            repository.write(
                "src/file_000.rs",
                benchmark_source(0, sample % 2 == 0).as_bytes(),
            )?;
            let batch = watcher
                .next_batch(Duration::from_secs(5))?
                .ok_or("watcher delta timed out")?;
            let watcher_elapsed = started.elapsed();
            let refresh_started = Instant::now();
            let result = refresh
                .execute(&project, &batch, &mut compiler, &SilentControl)
                .await?;
            assert_eq!(result.hashed_paths().len(), 1);
            assert_eq!(
                result.compilation().mode(),
                RepositoryIndexMode::Incremental
            );
            assert_eq!(result.compilation().parsed_paths().len(), 1);
            assert!(result.published());
            watcher_samples.push(watcher_elapsed);
            refresh_samples.push(refresh_started.elapsed());
            samples.push(started.elapsed());
        }
        watcher.shutdown()?;

        samples.sort_unstable();
        watcher_samples.sort_unstable();
        refresh_samples.sort_unstable();
        let percentile_index = sample_count
            .saturating_mul(95)
            .div_ceil(100)
            .saturating_sub(1);
        let p50 = samples[sample_count / 2];
        let p95 = samples[percentile_index];
        let watcher_p95 = watcher_samples[percentile_index];
        let refresh_p95 = refresh_samples[percentile_index];
        println!(
            "A^3 S11 incremental baseline: {} files, {} LOC, {} samples, watcher-to-publish P50={p50:?}, P95={p95:?}, watcher P95={watcher_p95:?}, refresh P95={refresh_p95:?}",
            FILE_COUNT,
            FILE_COUNT.saturating_mul(LINES_PER_FILE),
            sample_count,
        );
        assert!(
            p95 <= P95_TARGET,
            "one-file P95 {p95:?} exceeded {P95_TARGET:?}"
        );
        Ok::<(), Box<dyn Error>>(())
    })
}

fn benchmark_source(file: usize, alternate: bool) -> String {
    let variant = if alternate { "variant_b" } else { "variant_a" };
    let mut source = String::new();
    for function in 0..10 {
        source.push_str(&format!(
            "pub fn file_{file:03}_{variant}_{function:02}() -> usize {{ {function} }}\n"
        ));
    }
    for line in 10..LINES_PER_FILE {
        source.push_str(&format!("// bounded fixture line {line:03}\n"));
    }
    source
}

#[test]
#[ignore = "manual cold Fast Index and targeted flow-read P95 baseline"]
fn cold_index_and_flow_reads_keep_existing_budgets() -> Result<(), Box<dyn Error>> {
    block_on(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write(
            "Cargo.toml",
            b"[package]\nname='cold-flow-benchmark'\nversion='0.1.0'\n",
        )?;
        for index in 0..FILE_COUNT {
            repository.write(
                format!("src/file_{index:03}.rs"),
                benchmark_source(index, false).as_bytes(),
            )?;
        }
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let mut cold = Vec::new();
        let mut reads = Vec::new();
        for _ in 0..5 {
            let data = TempDirectory::new()?;
            let store = Arc::new(
                LibsqlKnowledgeStore::open(&StorageLayout::prepare(data.path().join("app-data"))?)
                    .await?,
            );
            store.record_opened_project(&project).await?;
            let refresh = RefreshRepositoryIndex::new(
                Arc::new(Blake3RepositorySnapshotBuilder::new()),
                store.clone(),
                Arc::new(Blake3IndexRunIdFactory),
            );
            let started = Instant::now();
            let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
            let result = refresh
                .execute(
                    &project,
                    &RepositoryChangeBatch::full_rescan(
                        Vec::new(),
                        RepositoryRescanReason::InitialObservation,
                    )?,
                    &mut compiler,
                    &SilentControl,
                )
                .await?;
            assert!(result.published());
            assert_eq!(result.compilation().parsed_paths().len(), FILE_COUNT + 1);
            cold.push(started.elapsed());
            let explorer = a3_application::ExploreFunctionFlows::new(store);
            let page = explorer
                .catalog(&project, "file_000_variant_a_00", 0, &FlowReadControl)
                .await?
                .ok_or("catalog")?;
            let owner = page.symbols.first().ok_or("flow owner")?;
            let selection = a3_application::FunctionFlowSelection {
                run_id: page.run_id,
                root: owner.id(),
                call_path: Vec::new(),
            };
            for _ in 0..6 {
                let started = Instant::now();
                assert!(
                    explorer
                        .inspect(&project, &selection, &FlowReadControl)
                        .await?
                        .is_some()
                );
                reads.push(started.elapsed());
            }
        }
        cold.sort_unstable();
        reads.sort_unstable();
        let p95 = cold[cold.len() * 95 / 100];
        let read_p95 = reads[(reads.len() * 95).div_ceil(100) - 1];
        println!(
            "A^3 flow-v1: {FILE_COUNT} files, 100000 LOC, 5 cold-index samples P50={:?} P95={p95:?}; 30 targeted reads P50={:?} P95={read_p95:?}",
            cold[cold.len() / 2],
            reads[reads.len() / 2]
        );
        assert!(p95 <= Duration::from_secs(30));
        assert!(read_p95 <= Duration::from_secs(2));
        Ok(())
    })
}
#[derive(Debug)]
struct FlowReadControl;
impl a3_application::IndexPersistenceControl for FlowReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(
        &self,
        _: Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        Ok(())
    }
}

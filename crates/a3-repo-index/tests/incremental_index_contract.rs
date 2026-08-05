//! End-to-end S11 contracts from confirmed watcher hints to atomic publication.

mod support;

use a3_application::{
    ExactSearchControl, KnowledgeIndexStore, KnowledgeSearchStore, KnowledgeStore,
    RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryIndexMode, RepositoryRescanReason,
};
use a3_domain::{
    ExactSearchPageSize, ExactSearchQuery, ExactSearchRole, ExactSearchTerm, IndexSchemaVersion,
    Progress, RepositoryPath, SnapshotChangeKind,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use futures::executor::block_on;
use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::TempDirectory;

#[derive(Debug, Default)]
struct RecordingControl {
    progress: Mutex<Vec<Progress>>,
}

impl RepositoryIndexControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, progress: Progress) -> Result<(), RepositoryIndexControlError> {
        self.progress
            .lock()
            .map_err(|_| RepositoryIndexControlError::Unavailable)?
            .push(progress);
        Ok(())
    }
}

impl ExactSearchControl for RecordingControl {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct CancelledControl;

impl RepositoryIndexControl for CancelledControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

#[test]
fn one_file_refresh_hashes_and_parses_only_that_file_then_publishes() -> Result<(), Box<dyn Error>>
{
    block_on(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write(
            "Cargo.toml",
            b"[package]\nname='fixture'\nversion='0.1.0'\n",
        )?;
        repository.write("src/lib.rs", b"pub mod alpha;\npub mod beta;\n")?;
        repository.write("src/alpha.rs", b"pub fn alpha() -> u8 { 1 }\n")?;
        repository.write("src/beta.rs", b"pub fn beta() -> u8 { 2 }\n")?;
        repository.write(
            ".env",
            b"API_TOKEN=not-a-real-secret-fixture-value-1234567890\n",
        )?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;

        let app_data = TempDirectory::new()?;
        let layout = StorageLayout::prepare(app_data.path().join("app-data"))?;
        let store = Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        store.record_opened_project(&project).await?;
        let index_store: Arc<dyn KnowledgeIndexStore> = store.clone();
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            index_store,
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let control = RecordingControl::default();

        let initial = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &control,
            )
            .await?;
        assert_eq!(initial.compilation().mode(), RepositoryIndexMode::Full);
        assert_eq!(
            initial.snapshot().index_schema_version(),
            IndexSchemaVersion::v2()
        );
        assert_eq!(initial.compilation().parsed_paths().len(), 4);
        assert_eq!(
            initial.compilation().publication().manifest_files().len(),
            1
        );
        assert_eq!(
            initial.compilation().publication().manifest_files()[0]
                .path()
                .as_bytes(),
            b"Cargo.toml"
        );
        assert!(
            initial
                .compilation()
                .publication()
                .graph()
                .files()
                .iter()
                .all(|revision| revision.path().as_bytes() != b".env")
        );
        assert!(initial.published());

        repository.write("src/alpha.rs", b"pub fn omega() -> u8 { 1 }\n")?;
        let changed = RepositoryPath::try_from_bytes(b"src/alpha.rs".to_vec())?;
        let incremental = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::incremental(vec![changed.clone()])?,
                &mut compiler,
                &control,
            )
            .await?;

        assert_eq!(incremental.hashed_paths(), std::slice::from_ref(&changed));
        assert_eq!(
            incremental.compilation().mode(),
            RepositoryIndexMode::Incremental
        );
        assert_eq!(incremental.compilation().parsed_paths(), &[changed]);
        assert!(incremental.published());
        assert!(
            incremental
                .compilation()
                .publication()
                .graph()
                .symbols()
                .iter()
                .any(|symbol| symbol.parsed().name().as_str() == "omega")
        );
        let omega_query =
            ExactSearchQuery::Symbol(ExactSearchTerm::try_from_string("omega".to_owned())?);
        assert_eq!(
            store
                .search_exact(
                    &project,
                    &omega_query,
                    ExactSearchPageSize::DEFAULT,
                    None,
                    &control,
                )
                .await?
                .hits()
                .len(),
            1
        );
        assert_eq!(
            store
                .search_exact(
                    &project,
                    &ExactSearchQuery::Role(ExactSearchRole::Manifest),
                    ExactSearchPageSize::DEFAULT,
                    None,
                    &control,
                )
                .await?
                .hits()
                .len(),
            1
        );
        let published = store
            .latest_published_index_run(&project)
            .await?
            .ok_or("published run missing")?;
        assert_eq!(published.snapshot_id(), incremental.snapshot().id());

        drop(refresh);
        drop(store);
        let reopened = Arc::new(LibsqlKnowledgeStore::open(&layout).await?);
        let reopened_index_store: Arc<dyn KnowledgeIndexStore> = reopened.clone();
        let restarted_refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            reopened_index_store,
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut restarted_compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let warmed = restarted_refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut restarted_compiler,
                &RecordingControl::default(),
            )
            .await?;
        assert_eq!(warmed.compilation().mode(), RepositoryIndexMode::Full);
        assert!(!warmed.published());

        repository.write("src/beta.rs", b"pub fn zeta() -> u8 { 2 }\n")?;
        let beta = RepositoryPath::try_from_bytes(b"src/beta.rs".to_vec())?;
        let after_restart = restarted_refresh
            .execute(
                &project,
                &RepositoryChangeBatch::incremental(vec![beta.clone()])?,
                &mut restarted_compiler,
                &RecordingControl::default(),
            )
            .await?;
        assert_eq!(
            after_restart.compilation().mode(),
            RepositoryIndexMode::Incremental
        );
        assert_eq!(
            after_restart.compilation().parsed_paths(),
            std::slice::from_ref(&beta)
        );
        assert!(after_restart.published());
        Ok::<(), Box<dyn Error>>(())
    })
}

#[test]
fn burst_add_modify_delete_and_rename_produces_one_consistent_delta() -> Result<(), Box<dyn Error>>
{
    block_on(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("src/rename_me.rs", b"pub fn retained() {}\n")?;
        repository.write("src/modify.rs", b"pub fn before() {}\n")?;
        repository.write("src/delete.rs", b"pub fn deleted() {}\n")?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store.clone(),
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let control = RecordingControl::default();
        refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(
                    Vec::new(),
                    RepositoryRescanReason::InitialObservation,
                )?,
                &mut compiler,
                &control,
            )
            .await?;

        fs::rename(
            repository.path().join("src/rename_me.rs"),
            repository.path().join("src/renamed.rs"),
        )?;
        repository.write("src/modify.rs", b"pub fn after_() {}\n")?;
        fs::remove_file(repository.path().join("src/delete.rs"))?;
        repository.write("src/added.rs", b"pub fn added() {}\n")?;
        let paths = [
            "src/rename_me.rs",
            "src/renamed.rs",
            "src/modify.rs",
            "src/delete.rs",
            "src/added.rs",
        ]
        .into_iter()
        .map(|path| RepositoryPath::try_from_bytes(path.as_bytes().to_vec()))
        .collect::<Result<Vec<_>, _>>()?;
        let result = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::incremental(paths)?,
                &mut compiler,
                &control,
            )
            .await?;

        assert_eq!(
            result.compilation().mode(),
            RepositoryIndexMode::Incremental
        );
        assert_eq!(result.hashed_paths().len(), 3);
        assert_eq!(result.compilation().parsed_paths().len(), 3);
        assert_eq!(result.snapshot().changes().len(), 5);
        assert_eq!(
            result
                .snapshot()
                .changes()
                .iter()
                .filter(|change| change.kind() == SnapshotChangeKind::Delete)
                .count(),
            2
        );
        assert!(result.published());
        Ok::<(), Box<dyn Error>>(())
    })
}

#[test]
fn cancellation_is_observed_within_the_quality_gate() -> Result<(), Box<dyn Error>> {
    block_on(async {
        let repository = TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        repository.write("src/lib.rs", b"pub fn value() {}\n")?;
        repository.git(["add", "."])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let app_data = TempDirectory::new()?;
        let store = Arc::new(
            LibsqlKnowledgeStore::open(&StorageLayout::prepare(app_data.path().join("app-data"))?)
                .await?,
        );
        store.record_opened_project(&project).await?;
        let refresh = RefreshRepositoryIndex::new(
            Arc::new(Blake3RepositorySnapshotBuilder::new()),
            store,
            Arc::new(Blake3IndexRunIdFactory),
        );
        let mut compiler = BuiltinIncrementalIndexCompiler::new(ParserPoolSize::new(1)?)?;
        let started = Instant::now();
        let result = refresh
            .execute(
                &project,
                &RepositoryChangeBatch::full_rescan(Vec::new(), RepositoryRescanReason::Explicit)?,
                &mut compiler,
                &CancelledControl,
            )
            .await;
        assert!(result.is_err());
        assert!(started.elapsed() <= Duration::from_millis(500));
        Ok::<(), Box<dyn Error>>(())
    })
}

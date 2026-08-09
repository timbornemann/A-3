//! E5 acceptance from real multilingual fixture files through the published Fast Index.

mod support;

use a3_application::{
    DiscoverProjectCommands, IndexPersistenceControl, IndexPersistenceControlError,
    KnowledgeIndexStore, KnowledgeStore, RefreshRepositoryIndex, RepositoryChangeBatch,
    RepositoryIndexControl, RepositoryIndexControlError, RepositoryRescanReason,
};
use a3_domain::{
    AgentRunId, DiscoveredCommandKind, PolicyDisposition, ProcessPlanBinding, Progress,
    ProjectCommandCatalog, SystemPolicyV1, WorkspaceDirectory,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::error::Error;
use std::sync::Arc;
use support::{TempDirectory, run_libsql_test};

const RUST_FILES: &[(&str, &[u8])] = &[
    (
        "Cargo.toml",
        include_bytes!("../../../fixtures/rust-adapter/Cargo.toml"),
    ),
    (
        "src/main.rs",
        include_bytes!("../../../fixtures/rust-adapter/src/main.rs"),
    ),
];

const NODE_FILES: &[(&str, &[u8])] = &[
    (
        "package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/package.json"),
    ),
    (
        "pnpm-workspace.yaml",
        include_bytes!("../../../fixtures/typescript-monorepo/pnpm-workspace.yaml"),
    ),
    (
        "packages/web/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/web/package.json"),
    ),
    (
        "packages/web/src/App.tsx",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/web/src/App.tsx"),
    ),
];

const PYTHON_FILES: &[(&str, &[u8])] = &[
    (
        "pyproject.toml",
        include_bytes!("../../../fixtures/python-package/pyproject.toml"),
    ),
    (
        "setup.cfg",
        include_bytes!("../../../fixtures/python-package/setup.cfg"),
    ),
    (
        "setup.py",
        include_bytes!("../../../fixtures/python-package/setup.py"),
    ),
    (
        "tests/test_service.py",
        include_bytes!("../../../fixtures/python-package/tests/test_service.py"),
    ),
];

#[derive(Debug)]
struct AcceptanceControl;

impl RepositoryIndexControl for AcceptanceControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
}

impl IndexPersistenceControl for AcceptanceControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

#[test]
fn published_fixture_manifests_produce_bounded_safe_package_commands() -> Result<(), Box<dyn Error>>
{
    run_libsql_test(async {
        let rust = catalog(RUST_FILES).await?;
        assert_eq!(rust.commands().len(), 4);
        assert_kinds(
            &rust,
            &[
                DiscoveredCommandKind::Test,
                DiscoveredCommandKind::Build,
                DiscoveredCommandKind::Lint,
                DiscoveredCommandKind::Format,
            ],
        );
        assert!(rust.commands().iter().all(|command| {
            command.executable().as_str() == "cargo"
                && command.working_directory() == &WorkspaceDirectory::Root
        }));
        let rust_test = command(&rust, DiscoveredCommandKind::Test, None)?;
        assert_eq!(arguments(rust_test), ["test", "--offline", "--locked"]);

        let node = catalog(NODE_FILES).await?;
        assert_eq!(node.commands().len(), 5);
        assert_kinds(
            &node,
            &[
                DiscoveredCommandKind::Test,
                DiscoveredCommandKind::Build,
                DiscoveredCommandKind::Lint,
                DiscoveredCommandKind::Test,
                DiscoveredCommandKind::Format,
            ],
        );
        assert!(
            node.commands()
                .iter()
                .all(|command| command.executable().as_str() == "pnpm")
        );
        let web_test = command(&node, DiscoveredCommandKind::Test, Some(b"packages/web"))?;
        assert_eq!(arguments(web_test), ["run", "test"]);
        let web_format = command(&node, DiscoveredCommandKind::Format, Some(b"packages/web"))?;
        assert_eq!(arguments(web_format), ["run", "format"]);
        assert!(node.commands().iter().all(|command| {
            command.kind().as_str() != "install"
                && command
                    .arguments()
                    .iter()
                    .all(|argument| !argument.as_str().contains("install"))
        }));
        for discovered in node.commands() {
            let preview = node.preview(AgentRunId::from_bytes([91; 32]), discovered.id())?;
            assert_eq!(preview.plan_binding(), ProcessPlanBinding::Unbound);
            assert_ne!(
                SystemPolicyV1.disposition(&preview.policy_action()),
                PolicyDisposition::Automatic
            );
        }

        let python = catalog(PYTHON_FILES).await?;
        assert_eq!(python.commands().len(), 2);
        assert_kinds(
            &python,
            &[DiscoveredCommandKind::Test, DiscoveredCommandKind::Build],
        );
        let python_test = command(&python, DiscoveredCommandKind::Test, None)?;
        assert_eq!(python_test.executable().as_str(), "python");
        assert_eq!(arguments(python_test), ["-m", "pytest"]);
        let python_build = command(&python, DiscoveredCommandKind::Build, None)?;
        assert_eq!(arguments(python_build), ["-m", "build", "--no-isolation"]);
        Ok(())
    })
}

async fn catalog(files: &[(&str, &[u8])]) -> Result<ProjectCommandCatalog, Box<dyn Error>> {
    let repository = TempDirectory::new()?;
    repository.git(["init", "--initial-branch=main"])?;
    for (path, source) in files {
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
            &AcceptanceControl,
        )
        .await?;
    if !indexed.published() {
        return Err(std::io::Error::other("fixture index was not published").into());
    }
    let published = store
        .latest_published_index(&project, &AcceptanceControl)
        .await?
        .ok_or_else(|| std::io::Error::other("fixture published index is missing"))?;
    Ok(DiscoverProjectCommands.execute(project.worktree().id(), &published)?)
}

fn assert_kinds(catalog: &ProjectCommandCatalog, expected: &[DiscoveredCommandKind]) {
    let actual = catalog
        .commands()
        .iter()
        .map(|command| command.kind())
        .collect::<Vec<_>>();
    for kind in expected {
        assert!(actual.contains(kind), "missing {kind:?} command");
    }
}

fn command<'a>(
    catalog: &'a ProjectCommandCatalog,
    kind: DiscoveredCommandKind,
    directory: Option<&[u8]>,
) -> Result<&'a a3_domain::DiscoveredCommand, Box<dyn Error>> {
    catalog
        .commands()
        .iter()
        .find(|command| {
            command.kind() == kind
                && command
                    .working_directory()
                    .path()
                    .map(a3_domain::RepositoryPath::as_bytes)
                    == directory
        })
        .ok_or_else(|| std::io::Error::other("expected fixture command is missing").into())
}

fn arguments(command: &a3_domain::DiscoveredCommand) -> Vec<&str> {
    command
        .arguments()
        .iter()
        .map(a3_domain::ProcessArgument::as_str)
        .collect()
}

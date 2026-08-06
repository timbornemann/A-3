//! Multilingual published-index Deep Map acceptance contract for Gate M4/M5.

mod support;

use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexStore, KnowledgeStore,
    PlanDeepMap, RefreshRepositoryIndex, RepositoryChangeBatch, RepositoryIndexControl,
    RepositoryIndexControlError, RepositoryRescanReason,
};
use a3_domain::{
    ExploreBudget, ExploreEvidenceRequirement, ExplorePlan, ExplorePlanStopReason,
    ExplorePolicyVersion, ExploreStepStatus, ExploreTarget, ExploreVerificationMethod,
    IndexLanguage, ModuleCardSchemaVersion, ModuleCoverageSnapshot, ModuleId, ModuleKind,
    ModulePolicyVersion, ModuleRoot, Progress, PublishedIndex, RepositoryModule, SymbolId,
};
use a3_repo_index::{
    Blake3IndexRunIdFactory, Blake3RepositorySnapshotBuilder, BuiltinIncrementalIndexCompiler,
    ParserPoolSize,
};
use a3_storage_libsql::{LibsqlKnowledgeStore, StorageLayout};
use a3_workspace::RepositoryInspector;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write;
use std::sync::Arc;
use support::{TempDirectory, run_libsql_test};

const ACCEPTANCE_SCHEMA_VERSION: u16 = 1;
const EXPECTED_ACCEPTANCE: &str = include_str!("fixtures/deep_map_fixtures_v1.golden");

const RUST_FILES: &[(&str, &[u8])] = &[
    (
        "Cargo.toml",
        include_bytes!("../../../fixtures/rust-adapter/Cargo.toml"),
    ),
    (
        "invalid.rs",
        include_bytes!("../../../fixtures/rust-adapter/invalid.rs"),
    ),
    (
        "src/main.rs",
        include_bytes!("../../../fixtures/rust-adapter/src/main.rs"),
    ),
];

const TYPESCRIPT_FILES: &[(&str, &[u8])] = &[
    (
        "invalid.ts",
        include_bytes!("../../../fixtures/typescript-monorepo/invalid.ts"),
    ),
    (
        "package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/package.json"),
    ),
    (
        "pnpm-workspace.yaml",
        include_bytes!("../../../fixtures/typescript-monorepo/pnpm-workspace.yaml"),
    ),
    (
        "packages/core/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/core/package.json"),
    ),
    (
        "packages/core/src/index.ts",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/core/src/index.ts"),
    ),
    (
        "packages/legacy/package.json",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/legacy/package.json"),
    ),
    (
        "packages/legacy/src/index.cjs",
        include_bytes!("../../../fixtures/typescript-monorepo/packages/legacy/src/index.cjs"),
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
        "invalid.py",
        include_bytes!("../../../fixtures/python-package/invalid.py"),
    ),
    (
        "pyproject.toml",
        include_bytes!("../../../fixtures/python-package/pyproject.toml"),
    ),
    (
        "requirements/base.in",
        include_bytes!("../../../fixtures/python-package/requirements/base.in"),
    ),
    (
        "requirements-dev.txt",
        include_bytes!("../../../fixtures/python-package/requirements-dev.txt"),
    ),
    (
        "requirements.txt",
        include_bytes!("../../../fixtures/python-package/requirements.txt"),
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
        "src/sample/__init__.py",
        include_bytes!("../../../fixtures/python-package/src/sample/__init__.py"),
    ),
    (
        "src/sample/base.py",
        include_bytes!("../../../fixtures/python-package/src/sample/base.py"),
    ),
    (
        "src/sample/cli.py",
        include_bytes!("../../../fixtures/python-package/src/sample/cli.py"),
    ),
    (
        "src/sample/helpers.py",
        include_bytes!("../../../fixtures/python-package/src/sample/helpers.py"),
    ),
    (
        "src/sample/service.py",
        include_bytes!("../../../fixtures/python-package/src/sample/service.py"),
    ),
    (
        "tests/test_service.py",
        include_bytes!("../../../fixtures/python-package/tests/test_service.py"),
    ),
];

#[derive(Debug, Clone, Copy)]
struct FixtureDefinition {
    name: &'static str,
    language: IndexLanguage,
    files: &'static [(&'static str, &'static [u8])],
}

const FIXTURES: &[FixtureDefinition] = &[
    FixtureDefinition {
        name: "rust-adapter-v1",
        language: IndexLanguage::Rust,
        files: RUST_FILES,
    },
    FixtureDefinition {
        name: "typescript-monorepo-v1",
        language: IndexLanguage::TypeScriptJavaScript,
        files: TYPESCRIPT_FILES,
    },
    FixtureDefinition {
        name: "python-package-v1",
        language: IndexLanguage::Python,
        files: PYTHON_FILES,
    },
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
fn deep_map_v1_accepts_rust_typescript_and_python_fixtures() -> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let mut normalized = String::new();
        writeln!(
            normalized,
            "deep_map_fixture_schema={ACCEPTANCE_SCHEMA_VERSION}"
        )?;
        writeln!(normalized, "planner_policy=1 card_schema=1")?;
        for fixture in FIXTURES {
            normalized.push_str(&evaluate_fixture(*fixture).await?);
        }
        if normalized != EXPECTED_ACCEPTANCE {
            return Err(format!(
                "Deep Map fixture acceptance differs\n--- expected ---\n{EXPECTED_ACCEPTANCE}--- actual ---\n{normalized}"
            )
            .into());
        }
        Ok(())
    })
}

async fn evaluate_fixture(fixture: FixtureDefinition) -> Result<String, Box<dyn Error>> {
    let repository = TempDirectory::new()?;
    repository.git(["init", "--initial-branch=main"])?;
    for (path, source) in fixture.files {
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
        return Err(format!("{} did not produce a published index", fixture.name).into());
    }
    let published = store
        .latest_published_index(&project, &AcceptanceControl)
        .await?
        .ok_or_else(|| format!("{} published index is missing", fixture.name))?;
    validate_publication(fixture, &published, indexed.snapshot().id())?;

    let coverage =
        ModuleCoverageSnapshot::empty(published.run().snapshot_id(), ModuleCardSchemaVersion::V1);
    let first =
        PlanDeepMap::version_one().execute(&published, &coverage, ExploreBudget::DEFAULT)?;
    let second =
        PlanDeepMap::version_one().execute(&published, &coverage, ExploreBudget::DEFAULT)?;
    if first != second {
        return Err(format!("{} Deep Map plan is not deterministic", fixture.name).into());
    }
    validate_plan(fixture, &published, &first)?;
    normalize_fixture(fixture, &published, &first)
}

fn validate_publication(
    fixture: FixtureDefinition,
    published: &PublishedIndex,
    expected_snapshot: a3_domain::SnapshotId,
) -> Result<(), Box<dyn Error>> {
    if published.run().snapshot_id() != expected_snapshot
        || published.publication().modules().snapshot_id() != expected_snapshot
    {
        return Err(format!(
            "{} Deep Map publication is snapshot-incoherent",
            fixture.name
        )
        .into());
    }
    let graph = published.publication().graph();
    let modules = published.publication().modules();
    let card = modules.repository_card();
    if modules.policy_version() != ModulePolicyVersion::v1()
        || card.policy_version() != ModulePolicyVersion::v1()
        || card.snapshot_id() != expected_snapshot
        || !card.languages().contains(&fixture.language)
    {
        return Err(format!(
            "{} repository card has incompatible policy, snapshot, or language",
            fixture.name
        )
        .into());
    }
    if usize::try_from(card.file_count())? != graph.files().len()
        || usize::try_from(card.symbol_count())? != graph.symbols().len()
        || modules.modules().is_empty()
    {
        return Err(format!("{} repository card does not cover its graph", fixture.name).into());
    }
    let primary_counts = modules.memberships().iter().fold(
        BTreeMap::<SymbolId, usize>::new(),
        |mut counts, membership| {
            if membership.evidence().kind().is_primary() {
                *counts.entry(membership.symbol_id()).or_default() += 1;
            }
            counts
        },
    );
    if graph
        .symbols()
        .iter()
        .any(|symbol| primary_counts.get(&symbol.id()).copied() != Some(1))
    {
        return Err(format!("{} has a symbol without one primary module", fixture.name).into());
    }
    for membership in modules.memberships() {
        if !graph
            .files()
            .contains(membership.evidence().member_revision())
            || membership
                .evidence()
                .manifest_revision()
                .is_some_and(|revision| !graph.files().contains(revision))
            || membership
                .evidence()
                .relationships()
                .iter()
                .any(|evidence| !graph.files().contains(evidence.revision()))
        {
            return Err(format!("{} contains stale module evidence", fixture.name).into());
        }
    }
    Ok(())
}

fn validate_plan(
    fixture: FixtureDefinition,
    published: &PublishedIndex,
    plan: &ExplorePlan,
) -> Result<(), Box<dyn Error>> {
    if plan.index_run_id() != published.run().id()
        || plan.snapshot_id() != published.run().snapshot_id()
        || plan.schema_version() != ModuleCardSchemaVersion::V1
        || plan.policy_version() != ExplorePolicyVersion::V1
        || plan.stop_reason() != ExplorePlanStopReason::CoveragePlanned
        || !plan.budget().contains(plan.reserved_cost())
        || plan.steps().is_empty()
    {
        return Err(format!("{} produced an incomplete Deep Map plan", fixture.name).into());
    }
    if !plan
        .steps()
        .iter()
        .enumerate()
        .all(|(index, step)| usize::from(step.sequence()) == index + 1)
    {
        return Err(format!("{} plan sequence is not contiguous", fixture.name).into());
    }
    let modules = published.publication().modules();
    let planned_modules = plan
        .steps()
        .iter()
        .map(|step| step.module_id())
        .collect::<BTreeSet<_>>();
    if modules
        .modules()
        .iter()
        .any(|module| !planned_modules.contains(&module.id()))
    {
        return Err(format!("{} left a module without a plan step", fixture.name).into());
    }
    for step in plan.steps() {
        if step.status() != ExploreStepStatus::Planned
            || step.coverage_fields().is_empty()
            || step.verification_method()
                != ExploreVerificationMethod::ResolveFieldEvidenceAgainstPublishedIndex
        {
            return Err(format!("{} contains an unverifiable plan step", fixture.name).into());
        }
        validate_step_target(fixture, published, step)?;
    }
    Ok(())
}

fn validate_step_target(
    fixture: FixtureDefinition,
    published: &PublishedIndex,
    step: &a3_domain::ExploreStep,
) -> Result<(), Box<dyn Error>> {
    let modules = published.publication().modules();
    let module = modules
        .modules()
        .iter()
        .find(|module| module.id() == step.module_id())
        .ok_or_else(|| format!("{} step targets an unknown module", fixture.name))?;
    let valid = match step.target() {
        ExploreTarget::Module(module_id) => {
            *module_id == module.id()
                && step.evidence_requirement()
                    == ExploreEvidenceRequirement::CurrentModuleProjection
        }
        ExploreTarget::Manifest { path, content_hash } => {
            module
                .manifests()
                .iter()
                .any(|revision| revision.path() == path && revision.content_hash() == *content_hash)
                && step.evidence_requirement()
                    == ExploreEvidenceRequirement::CurrentManifestRevision
        }
        ExploreTarget::Symbol(symbol_id) => {
            published
                .publication()
                .graph()
                .symbols()
                .iter()
                .any(|symbol| symbol.id() == *symbol_id)
                && modules.memberships().iter().any(|membership| {
                    membership.module_id() == module.id() && membership.symbol_id() == *symbol_id
                })
                && step.evidence_requirement() == ExploreEvidenceRequirement::CurrentSymbolRevision
        }
    };
    if !valid {
        return Err(format!(
            "{} step target has no current module evidence",
            fixture.name
        )
        .into());
    }
    Ok(())
}

fn normalize_fixture(
    fixture: FixtureDefinition,
    published: &PublishedIndex,
    plan: &ExplorePlan,
) -> Result<String, Box<dyn Error>> {
    let graph = published.publication().graph();
    let projection = published.publication().modules();
    let labels = module_labels(projection.modules())?;
    let card = projection.repository_card();
    let languages = card
        .languages()
        .iter()
        .map(|language| language.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let primary = projection
        .modules()
        .iter()
        .filter(|module| module.kind().is_primary())
        .count();
    let communities = projection.modules().len() - primary;
    let mut output = String::new();
    writeln!(
        output,
        "fixture name={} digest={} files={} symbols={} languages={} modules={} primary={} communities={} memberships={}",
        fixture.name,
        fixture_digest(fixture),
        graph.files().len(),
        graph.symbols().len(),
        languages,
        projection.modules().len(),
        primary,
        communities,
        projection.memberships().len()
    )?;
    for module in projection.modules() {
        writeln!(
            output,
            "module label={} kind={:?} manifests={} central={} entrypoints={} tests={}",
            labels
                .get(&module.id())
                .ok_or("normalized module label is missing")?,
            module.kind(),
            module
                .manifests()
                .iter()
                .map(|revision| path_text(revision.path()))
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
            symbol_set_labels(module.central_symbols().symbols(), published)?.join(","),
            symbol_set_labels(module.entrypoints().symbols(), published)?.join(","),
            symbol_set_labels(module.tests().symbols(), published)?.join(",")
        )?;
    }
    writeln!(
        output,
        "plan steps={} stop={:?} tokens={} milliseconds={} tools={}",
        plan.steps().len(),
        plan.stop_reason(),
        plan.reserved_cost().tokens(),
        plan.reserved_cost().milliseconds(),
        plan.reserved_cost().tool_calls()
    )?;
    for step in plan.steps() {
        writeln!(
            output,
            "step sequence={} module={} target={} reason={:?} fields={} gain={} cost={}/{}/{} evidence={:?} verify={:?} status={:?}",
            step.sequence(),
            labels
                .get(&step.module_id())
                .ok_or("normalized step module label is missing")?,
            target_label(step.target(), published, &labels)?,
            step.reason(),
            step.coverage_fields()
                .iter()
                .map(|field| format!("{field:?}"))
                .collect::<Vec<_>>()
                .join(","),
            step.expected_information_gain().basis_points(),
            step.reserved_cost().tokens(),
            step.reserved_cost().milliseconds(),
            step.reserved_cost().tool_calls(),
            step.evidence_requirement(),
            step.verification_method(),
            step.status()
        )?;
    }
    Ok(output)
}

fn module_labels(
    modules: &[RepositoryModule],
) -> Result<BTreeMap<ModuleId, String>, Box<dyn Error>> {
    let mut labels = BTreeMap::new();
    let mut community_index = 0_u16;
    for module in modules {
        let label = match (module.kind(), module.root()) {
            (ModuleKind::ManifestBoundary, Some(root)) => {
                format!("manifest:{}", root_label(root)?)
            }
            (ModuleKind::PathBoundary, Some(root)) => format!("path:{}", root_label(root)?),
            (ModuleKind::GraphCommunity, None) => {
                community_index = community_index
                    .checked_add(1)
                    .ok_or("graph-community label overflow")?;
                format!("graph-community-{community_index}")
            }
            _ => return Err("published module has an invalid kind/root shape".into()),
        };
        if labels.insert(module.id(), label).is_some() {
            return Err("published module label repeated an ID".into());
        }
    }
    Ok(labels)
}

fn root_label(root: &ModuleRoot) -> Result<String, Box<dyn Error>> {
    match root {
        ModuleRoot::Repository => Ok(".".to_owned()),
        ModuleRoot::Directory(path) => path_text(path),
    }
}

fn target_label(
    target: &ExploreTarget,
    published: &PublishedIndex,
    module_labels: &BTreeMap<ModuleId, String>,
) -> Result<String, Box<dyn Error>> {
    match target {
        ExploreTarget::Module(module_id) => Ok(format!(
            "module:{}",
            module_labels
                .get(module_id)
                .ok_or("target module label is missing")?
        )),
        ExploreTarget::Manifest { path, .. } => Ok(format!("manifest:{}", path_text(path)?)),
        ExploreTarget::Symbol(symbol_id) => {
            Ok(format!("symbol:{}", symbol_label(*symbol_id, published)?))
        }
    }
}

fn symbol_set_labels(
    symbols: &[SymbolId],
    published: &PublishedIndex,
) -> Result<Vec<String>, Box<dyn Error>> {
    symbols
        .iter()
        .map(|symbol_id| symbol_label(*symbol_id, published))
        .collect()
}

fn symbol_label(symbol_id: SymbolId, published: &PublishedIndex) -> Result<String, Box<dyn Error>> {
    let symbol = published
        .publication()
        .graph()
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == symbol_id)
        .ok_or("normalized symbol is missing")?;
    Ok(format!(
        "{}::{}",
        path_text(symbol.revision().path())?,
        symbol.parsed().name().as_str()
    ))
}

fn path_text(path: &a3_domain::RepositoryPath) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(path.as_bytes().to_vec())?)
}

fn fixture_digest(fixture: FixtureDefinition) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    for (path, source) in fixture.files {
        hasher.update(path.as_bytes());
        hasher.update(&[0]);
        hasher.update(source);
        hasher.update(&[u8::MAX]);
    }
    hasher.finalize()
}

//! Actual frozen A02 fixtures through the normal native index and cold-read ports.
use super::*;
use a3_application::{
    IndexPersistenceControl, IndexPersistenceControlError, LanguageAdapter, LanguageParseControl,
    LanguageParseControlError, LanguageParseInput, LanguageParsePolicy,
    RefreshRepositoryIndexError,
};
use a3_domain::{
    ContentHash, DiscoveredFileRoles, FileRevision, IndexLanguage, IndexRunStatus, ParseCoverage,
    PublishedIndex, RepositoryFileState, SourcePosition, SymbolKind,
};
use a3_repo_index::PythonLanguageAdapter;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MATERIALIZE: &str = r#"
import hashlib, json, pathlib, sys
repo, destination = map(pathlib.Path, sys.argv[1:3])
variant = sys.argv[3]
known = {
    'fixtures/local-agent-eval-v1/cases.py': '562aeb02e095ddb6e1e49b3c37233b0cd27e07944920ffa802b00e01a06b2ca6',
    'fixtures/local-agent-eval-v1/suite.json': '692744baa0dc4f3457c1838c39a2d06eaddc79eaa4e28a11367e0cd495c4ce21',
}
freeze_path = repo / 'fixtures/local-agent-eval-v1/p0-freeze.json'
freeze_bytes = freeze_path.read_bytes()
assert hashlib.sha256(freeze_bytes).hexdigest() == '9e6e563d62ae157d396f224acaf84b2a6f332daae382c8a845e560b1056819bb'
freeze = json.loads(freeze_bytes.decode('utf-8'))
for relative, expected in known.items():
    assert freeze['inputs_sha256'][relative] == expected
    assert hashlib.sha256((repo / relative).read_bytes()).hexdigest() == expected
sys.path.insert(0, str(repo / 'fixtures/local-agent-eval-v1'))
import cases
suite = json.loads((repo / 'fixtures/local-agent-eval-v1/suite.json').read_text(encoding='utf-8'))
assert cases.suite_manifest() == suite
entry, = [v for v in suite['cases'] if v['case_id'] == 'A02' and v['variant'] == variant]
expected = entry['initial_file_sha256']
assert set(expected) == {'subject.py', 'pyproject.toml', 'tests/__init__.py', 'tests/test_behavior.py'}
request = cases.materialize('A02', variant, destination)
assert request == {k: v for k, v in entry.items() if k not in ('initial_file_sha256', 'intervention_file_sha256')}
assert json.loads((destination / 'request.json').read_text(encoding='utf-8')) == request
root = destination / 'worktree'
actual = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}
assert actual == expected
"#;

#[derive(Debug, Clone, Copy)]
enum Variant {
    Development,
    Holdout,
}

impl Variant {
    const fn name(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Holdout => "holdout",
        }
    }

    const fn subject_len(self) -> usize {
        match self {
            Self::Development => 308_099,
            Self::Holdout => 308_100,
        }
    }

    const fn window_end(self) -> u32 {
        match self {
            Self::Development => 154_080,
            Self::Holdout => 154_081,
        }
    }

    const fn subject_sha256(self) -> &'static str {
        match self {
            Self::Development => "100fdf6d7956aba37ea9ff50bcd07a85529fb3d7e81a466c3992f64251866cab",
            Self::Holdout => "41040213d74b48feb80064c6bbbb16b155af6625ef2fb433a1db416af0806f39",
        }
    }

    const fn test_sha256(self) -> &'static str {
        match self {
            Self::Development => "c802d8780e993188a26f860570d94571a8263d7e9ed2f5f6a7b313fba29ea3d4",
            Self::Holdout => "42c90ca80928ae5e551044d4507e5f84fbce348a5b0a67469218bdb16c5e1fd3",
        }
    }
}

#[test]
fn frozen_a02_development_full_native_index_survives_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async { assert_frozen_index(Variant::Development).await })
}

#[test]
fn frozen_a02_holdout_full_native_index_survives_cold_reopen() -> TestResult {
    crate::run_native_libsql_test(async { assert_frozen_index(Variant::Holdout).await })
}

async fn assert_frozen_index(variant: Variant) -> TestResult {
    let mut fixture = Fixture::new()?;
    let result = exercise_index(&mut fixture, variant).await;
    let cleanup = fixture.cleanup();
    result.and(cleanup)
}

async fn exercise_index(fixture: &mut Fixture, variant: Variant) -> TestResult {
    assert_default_parse_limits();
    let started = Instant::now();
    let destination = fixture.root.join("frozen");
    materialize(&destination, variant)?;
    eprintln!(
        "a02_index variant={} phase=materialize elapsed_ms={}",
        variant.name(),
        started.elapsed().as_millis()
    );
    let worktree = destination.join("worktree");
    initialize_git(&worktree)?;
    fixture.project = RepositoryInspector::new().inspect(&worktree)?;
    let before = inventory(&worktree)?;
    assert_inventory(&before, variant)?;
    let expected_state = revision_state(&before)?;
    let subject = before
        .iter()
        .find(|(path, _)| path == Path::new("subject.py"))
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or("frozen A02 subject is missing")?;

    assert_fresh_candidate(fixture).await?;
    let started = Instant::now();
    let control = IndexProbeControl::new(variant);
    let (store, refresh, compiler, indexed) = match index_fixture_refresh(fixture, &control).await {
        Ok(value) => value,
        Err(error) => {
            report_index_failure(variant, started, control.last_phase(), error.as_ref());
            if matches!(
                error.downcast_ref::<RefreshRepositoryIndexError>(),
                Some(RefreshRepositoryIndexError::Compiler(_))
            ) {
                // A diagnostic cannot turn a failed full refresh into a passing test.
                if diagnose_same_subject(variant, subject).is_err() {
                    eprintln!(
                        "a02_index variant={} phase=direct_same_subject failure=FixtureSetup",
                        variant.name()
                    );
                }
            }
            return Err(error);
        }
    };
    eprintln!(
        "a02_index variant={} phase=full_refresh elapsed_ms={} status=published",
        variant.name(),
        started.elapsed().as_millis()
    );
    assert_eq!(
        indexed.compilation().mode(),
        a3_application::RepositoryIndexMode::Full
    );
    assert!(indexed.published());
    let published = indexed.published_index().clone();
    drop(indexed);
    assert_eq!(published.run().status(), IndexRunStatus::Published);
    assert_eq!(
        store.current_file_state(&fixture.project).await?,
        expected_state
    );
    assert_publication(&published, &before, variant)?;
    assert!(
        inventory(&worktree)? == before,
        "indexing changed frozen A02 bytes"
    );

    // Refresh owns a store Arc; compiler owns parser caches. Keep only plain value evidence.
    drop((compiler, refresh, store));
    let started = Instant::now();
    let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    let reopened = cold
        .latest_published_index(&fixture.project, &ReadControl)
        .await?
        .ok_or("cold A02 publication is missing")?;
    assert!(
        reopened == published,
        "cold A02 publication identity or payload changed"
    );
    assert_eq!(
        cold.latest_published_index_run(&fixture.project).await?,
        Some(published.run())
    );
    assert_eq!(
        cold.current_file_state(&fixture.project).await?,
        expected_state
    );
    assert_publication(&reopened, &before, variant)?;
    assert_inventory(&inventory(&worktree)?, variant)?;
    assert!(
        inventory(&worktree)? == before,
        "cold reopen changed frozen A02 bytes"
    );
    drop(cold);
    eprintln!(
        "a02_index variant={} phase=cold_reopen elapsed_ms={} status=identical",
        variant.name(),
        started.elapsed().as_millis()
    );
    Ok(())
}

async fn assert_fresh_candidate(fixture: &Fixture) -> TestResult {
    let store = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
    store.record_opened_project(&fixture.project).await?;
    assert!(store.latest_snapshot(&fixture.project).await?.is_none());
    assert!(store.latest_index_run(&fixture.project).await?.is_none());
    assert!(store.current_file_state(&fixture.project).await?.is_empty());
    assert!(
        store
            .latest_published_index_run(&fixture.project)
            .await?
            .is_none()
    );
    drop(store);
    Ok(())
}

fn assert_default_parse_limits() {
    let policy = LanguageParsePolicy::v1();
    assert_eq!(policy.max_source_bytes(), 4 * 1024 * 1024);
    assert_eq!(policy.parse_timeout(), Duration::from_secs(2));
    assert_eq!(policy.pool_wait_timeout(), Duration::from_millis(500));
    assert_eq!(policy.max_tree_nodes(), 1_000_000);
    assert_eq!(policy.max_tree_depth(), 4_096);
    assert_eq!(policy.max_symbols(), 100_000);
    assert_eq!(policy.max_relations(), 200_000);
}

fn assert_inventory(values: &[(PathBuf, Vec<u8>)], variant: Variant) -> TestResult {
    let actual = values
        .iter()
        .map(|(path, bytes)| {
            Ok((
                path.to_str()
                    .ok_or("A02 path is not UTF-8")?
                    .replace('\\', "/"),
                format!("{:x}", Sha256::digest(bytes)),
            ))
        })
        .collect::<TestResult<BTreeMap<_, _>>>()?;
    let expected = BTreeMap::from([
        ("subject.py".to_owned(), variant.subject_sha256().to_owned()),
        (
            "pyproject.toml".to_owned(),
            "aacbeb7a8129c5f2b962a340dc4e9225fd62c0a809adad580a2b73afd0b62aa5".to_owned(),
        ),
        (
            "tests/__init__.py".to_owned(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        ),
        (
            "tests/test_behavior.py".to_owned(),
            variant.test_sha256().to_owned(),
        ),
    ]);
    assert_eq!(values.len(), 4);
    assert_eq!(actual, expected);
    let subject = &values
        .iter()
        .find(|(path, _)| path == Path::new("subject.py"))
        .ok_or("A02 subject bytes are missing")?
        .1;
    assert_eq!(subject.len(), variant.subject_len());
    assert!(subject.starts_with(b"\xef\xbb\xbf"));
    assert_eq!(
        subject.windows(2).filter(|pair| *pair == b"\r\n").count(),
        5_604
    );
    assert_eq!(subject.iter().filter(|byte| **byte == b'\n').count(), 5_604);
    Ok(())
}

fn revision_state(values: &[(PathBuf, Vec<u8>)]) -> TestResult<RepositoryFileState> {
    let revisions = values
        .iter()
        .map(|(path, source)| {
            Ok(FileRevision::new(
                RepositoryPath::try_from_bytes(
                    path.to_str()
                        .ok_or("A02 path is not UTF-8")?
                        .replace('\\', "/")
                        .into_bytes(),
                )?,
                ContentHash::from_bytes(*blake3::hash(source).as_bytes()),
            ))
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(RepositoryFileState::new(revisions)?)
}

fn assert_publication(
    published: &PublishedIndex,
    values: &[(PathBuf, Vec<u8>)],
    variant: Variant,
) -> TestResult {
    let publication = published.publication();
    let expected = revision_state(values)?;
    assert_eq!(publication.graph().files(), expected.revisions());
    assert_eq!(publication.file_analyses().len(), 4);
    for revision in expected.revisions() {
        let analysis = publication
            .file_analyses()
            .iter()
            .find(|analysis| analysis.revision() == revision)
            .ok_or("A02 exact file analysis is missing")?;
        let source = values
            .iter()
            .find(|(path, _)| {
                path.to_str().is_some_and(|path| {
                    path.replace('\\', "/").as_bytes() == revision.path().as_bytes()
                })
            })
            .map(|(_, source)| source)
            .ok_or("A02 revision has no frozen source")?;
        // All four frozen files have a Python source or Python-manifest adapter.
        assert_eq!(analysis.language(), IndexLanguage::Python);
        assert_eq!(
            analysis.coverage(),
            Some(ParseCoverage::complete(source.len())?)
        );
        assert!(
            analysis.diagnostics().is_empty(),
            "A02 parse emitted diagnostics"
        );
    }
    let subject_revision = expected
        .revisions()
        .iter()
        .find(|revision| revision.path().as_bytes() == b"subject.py")
        .ok_or("A02 subject revision is missing")?;
    let symbols = publication
        .graph()
        .symbols()
        .iter()
        .filter(|symbol| symbol.revision() == subject_revision)
        .collect::<Vec<_>>();
    let modules = symbols
        .iter()
        .filter(|symbol| symbol.parsed().kind() == SymbolKind::Module)
        .collect::<Vec<_>>();
    assert_eq!(modules.len(), 1);
    let module = modules[0].parsed();
    assert!(module.selection_range().is_empty());
    assert!(
        module
            .declaration_range()
            .contains(module.selection_range())
    );
    assert!(module.declaration_range().end_byte() as usize <= variant.subject_len());
    let functions = symbols
        .iter()
        .filter(|symbol| {
            symbol.parsed().kind() == SymbolKind::Function
                && symbol.parsed().name().as_str() == "adjust"
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1);
    let adjust = functions[0].parsed();
    let declaration = adjust.declaration_range();
    let selection = adjust.selection_range();
    assert_eq!(declaration.start_byte(), 154_024);
    assert!(declaration.end_byte() <= variant.window_end());
    assert_eq!(declaration.start_position(), SourcePosition::new(2_801, 0));
    assert!(declaration.contains(selection));
    assert_eq!(selection.start_byte(), 154_028);
    assert_eq!(selection.end_byte(), 154_034);
    assert_eq!(selection.start_position(), SourcePosition::new(2_801, 4));
    assert_eq!(selection.end_position(), SourcePosition::new(2_801, 10));
    let source = values
        .iter()
        .find(|(path, _)| path == Path::new("subject.py"))
        .map(|(_, source)| source)
        .ok_or("A02 subject selection source is missing")?;
    assert!(
        source.get(selection.start_byte() as usize..selection.end_byte() as usize)
            == Some(b"adjust".as_slice()),
        "A02 selection does not name adjust"
    );
    Ok(())
}

struct OwnedMaterializer(std::process::Child);
impl Drop for OwnedMaterializer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn materialize(destination: &Path, variant: Variant) -> TestResult {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let mut child = OwnedMaterializer(
        Command::new("python")
            .args(["-B", "-c", MATERIALIZE])
            .arg(&repo)
            .arg(destination)
            .arg(variant.name())
            .current_dir(&repo)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err("frozen A02 materialization or hash verification failed".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("frozen A02 materialization timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn report_index_failure(
    variant: Variant,
    started: Instant,
    last_phase: Option<RepositoryIndexPhase>,
    error: &(dyn std::error::Error + 'static),
) {
    if let Some(failure) = error.downcast_ref::<RefreshRepositoryIndexError>() {
        eprintln!(
            "a02_index variant={} phase=full_refresh elapsed_ms={} last_observed_phase={last_phase:?} failure={failure:?}",
            variant.name(),
            started.elapsed().as_millis()
        );
    } else {
        eprintln!(
            "a02_index variant={} phase=full_refresh elapsed_ms={} last_observed_phase={last_phase:?} failure=FixtureSetup",
            variant.name(),
            started.elapsed().as_millis()
        );
    }
}

fn diagnose_same_subject(variant: Variant, source: &[u8]) -> TestResult {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"subject.py".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source).as_bytes()),
    );
    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let started = Instant::now();
    match adapter.parse(
        LanguageParseInput::new(&revision, source, DiscoveredFileRoles::empty()),
        LanguageParsePolicy::v1(),
        &ReadControl,
    ) {
        Ok(result) => eprintln!(
            "a02_index variant={} phase=direct_same_subject elapsed_ms={} status=parsed complete={} diagnostics={}",
            variant.name(),
            started.elapsed().as_millis(),
            result.coverage().is_complete(),
            result.diagnostics().len()
        ),
        Err(failure) => eprintln!(
            "a02_index variant={} phase=direct_same_subject elapsed_ms={} failure={failure:?}",
            variant.name(),
            started.elapsed().as_millis()
        ),
    }
    Ok(())
}

#[derive(Debug)]
struct IndexProbeControl {
    variant: Variant,
    started: Instant,
    phase: Mutex<Option<RepositoryIndexPhase>>,
}

impl IndexProbeControl {
    fn new(variant: Variant) -> Self {
        Self {
            variant,
            started: Instant::now(),
            phase: Mutex::new(None),
        }
    }

    fn last_phase(&self) -> Option<RepositoryIndexPhase> {
        self.phase.lock().ok().and_then(|phase| *phase)
    }
}

impl RepositoryIndexControl for IndexProbeControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), RepositoryIndexControlError> {
        Ok(())
    }
    fn report_phase(&self, phase: RepositoryIndexPhase) -> Result<(), RepositoryIndexControlError> {
        let mut last = self
            .phase
            .lock()
            .map_err(|_| RepositoryIndexControlError::Unavailable)?;
        if *last != Some(phase) {
            eprintln!(
                "a02_index variant={} phase={phase:?} elapsed_ms={}",
                self.variant.name(),
                self.started.elapsed().as_millis()
            );
            *last = Some(phase);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ReadControl;
impl IndexPersistenceControl for ReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}
impl LanguageParseControl for ReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&self, _: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

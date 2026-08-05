//! Reproducible manual S9 link-and-rank baseline; excluded from the default test run.

use a3_application::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseInput,
    LanguageParsePolicy,
};
use a3_domain::{
    ContentHash, DiscoveredFileRoles, FileRevision, GitHead, GitReferenceName, IndexSchemaVersion,
    Progress, RepositoryFileState, RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind,
    SnapshotId, WorktreeGeneration, WorktreeId,
};
use a3_repo_index::{
    DeterministicGraphLinker, DeterministicGraphRanker, GraphComputationControl,
    GraphComputationControlError, GraphLinkInput, GraphLinkPolicy, ParserPoolSize,
    PythonLanguageAdapter, RankingPolicy,
};
use std::error::Error;
use std::time::Instant;

const STRUCTURAL_LINES: usize = 100_000;
const FUNCTION_COUNT: usize = STRUCTURAL_LINES / 2;

#[derive(Debug)]
struct SilentControl;

impl LanguageParseControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), LanguageParseControlError> {
        Ok(())
    }
}

impl GraphComputationControl for SilentControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), GraphComputationControlError> {
        Ok(())
    }
}

#[test]
#[ignore = "manual 100,000-line graph link-and-rank baseline"]
fn link_and_rank_python_100k_line_fixture() -> Result<(), Box<dyn Error>> {
    let mut source = String::with_capacity(FUNCTION_COUNT.saturating_mul(40));
    for index in 0..FUNCTION_COUNT {
        source.push_str(&format!("def function_{index:05}():\n    helper()\n"));
    }
    assert_eq!(source.lines().count(), STRUCTURAL_LINES);

    let adapter = PythonLanguageAdapter::new(ParserPoolSize::new(1)?)?;
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"benchmark/module.py".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
    );
    let parse = adapter.parse(
        LanguageParseInput::new(&revision, source.as_bytes(), DiscoveredFileRoles::empty()),
        LanguageParsePolicy::v1(),
        &SilentControl,
    )?;
    let files = RepositoryFileState::new(vec![revision.clone()])?;
    let snapshot = Snapshot::new(
        SnapshotId::from_bytes([0x63; 32]),
        WorktreeId::from_bytes([0x36; 32]),
        None,
        WorktreeGeneration::new(1)?,
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
        IndexSchemaVersion::new(1)?,
        vec![adapter.revision().clone()],
        vec![SnapshotChange::new(
            revision.path().clone(),
            revision.content_hash(),
            SnapshotChangeKind::Upsert,
        )],
    )?;
    let parses = vec![parse];

    let link_started = Instant::now();
    let graph = DeterministicGraphLinker.link(
        GraphLinkInput::new(&snapshot, &files, &parses),
        GraphLinkPolicy::v1(),
        &SilentControl,
    )?;
    let link_elapsed = link_started.elapsed();

    let rank_started = Instant::now();
    let ranks = DeterministicGraphRanker.rank(&graph, RankingPolicy::v1(), &SilentControl)?;
    let rank_elapsed = rank_started.elapsed();

    assert_eq!(graph.symbols().len(), FUNCTION_COUNT.saturating_add(1));
    assert_eq!(
        graph.edges().len().saturating_add(graph.unresolved().len()),
        parses[0].relations().len()
    );
    assert_eq!(ranks.symbols().len(), graph.symbols().len());
    assert!(link_elapsed <= GraphLinkPolicy::v1().timeout());
    assert!(rank_elapsed <= RankingPolicy::v1().timeout());

    println!(
        "A^3 S9 graph baseline: {STRUCTURAL_LINES} structural lines, {} bytes, {} symbols, {} resolved edges, {} unresolved candidates, link={link_elapsed:?}, rank={rank_elapsed:?}",
        source.len(),
        graph.symbols().len(),
        graph.edges().len(),
        graph.unresolved().len(),
    );
    Ok(())
}

use super::*;
use crate::{ContextToolResultDigest, ContextToolResultPreview};
use a3_domain::{
    AgentToolEvidenceSet, CandidateFreshness, CandidateTokenCost, CanonicalDirectory, Confidence,
    ContentHash, ExactSearchExplanation, ExactSearchHit, FusionPolicy, FusionResultLimit, GitHead,
    GitReferenceName, IndexPublication, IndexRunRecord, IndexRunSequence, IndexRunStatus,
    LinkResolution, LinkedGraph, ModulePolicyVersion, ModuleProjection, ModuleSymbolSet,
    NormalizedRetrievalSignal, RankProjection, RankingPolicyVersion, RepositoryCard, RepositoryId,
    RepositoryIdentity, RepositoryPath, RetrievalCandidate, RetrievalCandidateSet,
    RetrievalCandidateSets, RetrievalCandidateSignals, SourceChannel, SourceRange, SyntaxProvider,
    TaskLensPolicy, TaskLensSeed, TaskLensSeedSet, TaskLensSeedText, TaskLensTokenBudget,
    WorktreeAnchorId, WorktreeIdentity,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
const SNAPSHOT: SnapshotId = SnapshotId::from_bytes([1; 32]);
const TOOL: ToolRunId = ToolRunId::from_bytes([2; 32]);
const ORIGINAL: &str = "def caf\u{e9}(x):\r\n    return x + 1\r\n";
const START: usize = 100;
const FIRST_ROW: u32 = 9;

struct Fixture {
    project: ProjectIdentity,
    published: PublishedIndex,
    required: EvidenceRef,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let root =
            CanonicalDirectory::from_canonicalized(std::env::current_dir()?.canonicalize()?)?;
        let repository = RepositoryId::from_bytes([3; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository, root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([4; 32]),
                WorktreeAnchorId::from_bytes([5; 32]),
                repository,
                root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/subject.py".to_vec())?,
            ContentHash::from_bytes([6; 32]),
        );
        let required = EvidenceRef::new(
            revision.clone(),
            SourceRange::new(
                START + 4,
                START + ORIGINAL.len(),
                SourcePosition::new(FIRST_ROW, 4),
                SourcePosition::new(FIRST_ROW + 2, 0),
            )?,
        );
        let graph = LinkedGraph::new(SNAPSHOT, vec![revision], Vec::new(), Vec::new(), Vec::new())?;
        let ranking = RankProjection::new(SNAPSHOT, RankingPolicyVersion::v1(), Vec::new())?;
        let policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            SNAPSHOT,
            policy,
            Vec::new(),
            Vec::new(),
            ModuleSymbolSet::empty(),
            1,
            0,
        )?;
        let modules = ModuleProjection::new(SNAPSHOT, policy, Vec::new(), Vec::new(), card)?;
        let published = PublishedIndex::new(
            IndexRunRecord::new(
                IndexRunId::from_bytes([7; 32]),
                SNAPSHOT,
                RankingPolicyVersion::v1(),
                IndexRunSequence::new(1)?,
                IndexRunStatus::Published,
            ),
            IndexPublication::new(graph, ranking, Vec::new(), modules)?,
        )?;
        Ok(Self {
            project,
            published,
            required,
        })
    }

    fn lens(&self, selected: bool) -> TestResult<TaskLens> {
        let run = self.published.run();
        let sets = if selected {
            let hit = ExactSearchHit::file(
                self.required.revision().clone(),
                ExactSearchExplanation::NormalizedPathExact,
            )?;
            let candidate = RetrievalCandidate::from_exact(
                &hit,
                RetrievalCandidateSignals::new(
                    NormalizedRetrievalSignal::FULL,
                    NormalizedRetrievalSignal::FULL,
                    CandidateFreshness::Current,
                    CandidateTokenCost::new(16)?,
                    NormalizedRetrievalSignal::ZERO,
                ),
            );
            vec![RetrievalCandidateSet::complete(
                run.id(),
                run.snapshot_id(),
                SourceChannel::Exact,
                vec![candidate],
            )?]
        } else {
            Vec::new()
        };
        let fused = FusionPolicy::v1().fuse(
            RetrievalCandidateSets::new(run.id(), run.snapshot_id(), sets)?,
            FusionResultLimit::new(4)?,
        )?;
        let seed = TaskLensSeedText::try_from_string("Correct the declared function".to_owned())?;
        Ok(TaskLensPolicy::v1().compile(
            &self.published,
            TaskLensSeedSet::new(
                seed.clone(),
                seed,
                vec![TaskLensSeed::ExplicitPath(
                    self.required.revision().path().clone(),
                )],
            )?,
            &fused,
            Vec::new(),
            false,
            TaskLensTokenBudget::new(2_048)?,
        )?)
    }

    fn bound(&self) -> TestResult<BoundEditSource> {
        Ok(BoundEditSource::for_file_span(
            &self.project,
            &self.published,
            &self.lens(true)?,
            &self.required,
        )?)
    }

    fn page(&self, lines: u32, text: &str, next: u32) -> TestResult<AgentSourcePage> {
        Ok(AgentSourcePage::new(
            self.required.revision().clone(),
            SourceRange::new(
                START,
                START + text.len(),
                SourcePosition::new(FIRST_ROW, 0),
                SourcePosition::new(FIRST_ROW + lines, 0),
            )?,
            AgentFileStartLine::new(FIRST_ROW + 1)?,
            text.to_owned(),
            Some(AgentFileStartLine::new(next)?),
            true,
        )?)
    }
}

fn result(
    tool: ToolRunId,
    snapshot: SnapshotId,
    page: Option<AgentSourcePage>,
) -> TestResult<AgentReadResult> {
    let evidence = page.iter().map(AgentSourcePage::evidence).collect();
    let read = AgentReadResult::new(
        tool,
        ContextToolResultStatus::Succeeded,
        ContextToolResultPreview::try_from_string("Bounded tool metadata".to_owned())?,
        ContextToolResultDigest::from_bytes([8; 32]),
        false,
        snapshot,
        AgentToolEvidenceSet::new(snapshot, evidence)?,
        32,
    )?;
    Ok(match page {
        Some(page) => read.with_original_page(page)?,
        None => read,
    })
}

#[test]
fn selected_span_binds_complete_unicode_crlf_lines_and_only_releases_original() -> TestResult {
    let fixture = Fixture::new()?;
    let bound = fixture.bound()?;
    let AgentReadAction::Inspect(inspect) =
        bound.read_action(&fixture.project, &fixture.published)?
    else {
        return Err("expected one read-only inspection".into());
    };
    let AgentInspectTarget::File(request) = inspect.target() else {
        return Err("expected file inspection".into());
    };
    assert_eq!(request.path(), fixture.required.revision().path());
    assert_eq!(request.start_line().get(), 10);
    assert_eq!(request.line_count().get(), 2);
    assert_eq!(bound.required(), &fixture.required);
    assert_eq!(bound.snapshot_id(), SNAPSHOT);

    let page = fixture.page(2, ORIGINAL, 12)?;
    let mut read = result(TOOL, SNAPSHOT, Some(page.clone()))?;
    assert_eq!(
        bound.take_original(&fixture.project, &fixture.published, TOOL, &mut read)?,
        page
    );
    assert_eq!(
        bound.take_original(&fixture.project, &fixture.published, TOOL, &mut read),
        Err(BoundEditSourceFailure::MissingOriginal)
    );
    Ok(())
}

#[test]
fn metadata_and_unselected_targets_never_produce_an_original_page() -> TestResult {
    let fixture = Fixture::new()?;
    assert!(matches!(
        BoundEditSource::for_file_span(
            &fixture.project,
            &fixture.published,
            &fixture.lens(false)?,
            &fixture.required,
        ),
        Err(BoundEditSourceFailure::MissingSelection)
    ));
    assert!(matches!(
        BoundEditSource::for_symbol(
            &fixture.project,
            &fixture.published,
            &fixture.lens(true)?,
            SymbolId::from_bytes([9; 32]),
        ),
        Err(BoundEditSourceFailure::MissingSelection)
    ));
    let bound = fixture.bound()?;
    let mut read = result(TOOL, SNAPSHOT, None)?;
    assert_eq!(
        bound.take_original(&fixture.project, &fixture.published, TOOL, &mut read),
        Err(BoundEditSourceFailure::MissingOriginal)
    );
    Ok(())
}

#[test]
fn capacity_halts_without_clipping_the_mandatory_span() -> TestResult {
    let fixture = Fixture::new()?;
    let bound = fixture.bound()?;
    let first_line = ORIGINAL
        .split_inclusive('\n')
        .next()
        .ok_or("missing first line")?;
    let mut short = result(TOOL, SNAPSHOT, Some(fixture.page(1, first_line, 11)?))?;
    assert_eq!(
        bound.take_original(&fixture.project, &fixture.published, TOOL, &mut short),
        Err(BoundEditSourceFailure::TooLarge)
    );
    let too_many_lines = EvidenceRef::new(
        fixture.required.revision().clone(),
        SourceRange::new(
            0,
            501,
            SourcePosition::new(0, 0),
            SourcePosition::new(501, 0),
        )?,
    );
    assert!(matches!(
        BoundEditSource::for_file_span(
            &fixture.project,
            &fixture.published,
            &fixture.lens(true)?,
            &too_many_lines,
        ),
        Err(BoundEditSourceFailure::TooLarge)
    ));
    Ok(())
}

#[test]
fn changed_index_tool_snapshot_and_bad_line_geometry_are_rejected() -> TestResult {
    let fixture = Fixture::new()?;
    let bound = fixture.bound()?;
    let republished = PublishedIndex::new(
        IndexRunRecord::new(
            IndexRunId::from_bytes([20; 32]),
            SNAPSHOT,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(2)?,
            IndexRunStatus::Published,
        ),
        fixture.published.publication().clone(),
    )?;
    assert_eq!(
        bound.read_action(&fixture.project, &republished),
        Err(BoundEditSourceFailure::StalePublication)
    );
    let valid = fixture.page(2, ORIGINAL, 12)?;
    for (tool, snapshot, page, expected) in [
        (
            ToolRunId::from_bytes([21; 32]),
            SNAPSHOT,
            valid.clone(),
            BoundEditSourceFailure::InvalidReadResult,
        ),
        (
            TOOL,
            SnapshotId::from_bytes([22; 32]),
            valid,
            BoundEditSourceFailure::StalePublication,
        ),
        (
            TOOL,
            SNAPSHOT,
            fixture.page(2, ORIGINAL, 13)?,
            BoundEditSourceFailure::InvalidReadResult,
        ),
    ] {
        let mut read = result(tool, snapshot, Some(page))?;
        assert_eq!(
            bound.take_original(&fixture.project, &fixture.published, TOOL, &mut read),
            Err(expected)
        );
    }
    Ok(())
}

#[test]
fn mandatory_utf8_points_must_match_actual_byte_offsets() -> TestResult {
    let fixture = Fixture::new()?;
    let inside_e_acute = START + "def caf".len() + 1;
    let bad = EvidenceRef::new(
        fixture.required.revision().clone(),
        SourceRange::new(
            inside_e_acute,
            START + ORIGINAL.len(),
            SourcePosition::new(FIRST_ROW, 8),
            SourcePosition::new(FIRST_ROW + 2, 0),
        )?,
    );
    let bound = BoundEditSource::for_file_span(
        &fixture.project,
        &fixture.published,
        &fixture.lens(true)?,
        &bad,
    )?;
    let mut read = result(TOOL, SNAPSHOT, Some(fixture.page(2, ORIGINAL, 12)?))?;
    assert_eq!(
        bound.take_original(&fixture.project, &fixture.published, TOOL, &mut read),
        Err(BoundEditSourceFailure::InvalidReadResult)
    );
    Ok(())
}

#[test]
fn caller_requires_the_exact_direction_relation_and_contained_call_site() -> TestResult {
    let fixture = Fixture::new()?;
    let caller = SymbolId::from_bytes([31; 32]);
    let callee = SymbolId::from_bytes([32; 32]);
    let edge = |source, target, kind, evidence| -> TestResult<GraphEdge> {
        Ok(GraphEdge::new(
            GraphEndpoint::Symbol(source),
            GraphEndpoint::Symbol(target),
            kind,
            SyntaxProvider::TreeSitter,
            Confidence::from_basis_points(500)?,
            LinkResolution::AdapterLocalSymbol,
            SNAPSHOT,
            evidence,
        ))
    };
    for unrelated in [
        edge(
            callee,
            caller,
            SyntaxRelationKind::Calls,
            fixture.required.clone(),
        )?,
        edge(
            caller,
            callee,
            SyntaxRelationKind::Imports,
            fixture.required.clone(),
        )?,
        edge(
            caller,
            callee,
            SyntaxRelationKind::Calls,
            EvidenceRef::new(
                fixture.required.revision().clone(),
                SourceRange::new(0, 2, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?,
            ),
        )?,
    ] {
        assert!(!has_call_in_span(
            &[unrelated],
            caller,
            callee,
            &fixture.required
        ));
    }
    let actual = edge(
        caller,
        callee,
        SyntaxRelationKind::Calls,
        fixture.required.clone(),
    )?;
    assert!(has_call_in_span(
        &[actual],
        caller,
        callee,
        &fixture.required
    ));
    Ok(())
}

//! Versioned golden evaluation for deterministic retrieval fusion.

use a3_domain::{
    CandidateFreshness, CandidateTokenCost, Confidence, ContentHash, EvidenceRef,
    ExactSearchExplanation, ExactSearchHit, ExactSearchPage, ExactSearchPageSize,
    ExactSearchTarget, FileRevision, FusedRetrievalResult, FusionPolicy, FusionPriority,
    FusionResultLimit, GraphEdge, GraphEndpoint, GraphTraversalHit, GraphTraversalResult,
    IndexRunId, LexicalScore, LexicalSearchHit, LexicalSearchPage, LexicalSearchPageSize,
    LinkResolution, MemoryCandidateExplanation, NormalizedRetrievalSignal, RepositoryPath,
    RetrievalCandidate, RetrievalCandidateReason, RetrievalCandidateSet, RetrievalCandidateSets,
    RetrievalCandidateSignals, SnapshotId, SourceChannel, SourcePosition, SourceRange,
    SyntaxProvider, SyntaxRelationKind, TraversalDepth, TraversalDirection, TraversalQuery,
    TraversalResultLimit,
};
use std::fmt::Write;

const GOLDEN: &str = include_str!("fixtures/retrieval_fusion_v1.golden");

struct FusionEvalCase {
    name: &'static str,
    candidates: RetrievalCandidateSets,
    limit: FusionResultLimit,
}

#[test]
fn fusion_policy_v1_matches_the_versioned_golden() -> Result<(), Box<dyn std::error::Error>> {
    let first = run_eval(cases()?)?;
    let second = run_eval(cases()?)?;
    assert_eq!(first, second);
    assert_eq!(first, GOLDEN);
    Ok(())
}

fn run_eval(cases: Vec<FusionEvalCase>) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();
    for case in cases {
        let result = FusionPolicy::v1().fuse(case.candidates, case.limit)?;
        write_result(&mut output, case.name, &result)?;
    }
    Ok(output)
}

fn write_result(
    output: &mut String,
    name: &str,
    result: &FusedRetrievalResult,
) -> Result<(), std::fmt::Error> {
    writeln!(
        output,
        "case={name} policy={} truncated={}",
        result.policy_version().get(),
        result.truncated()
    )?;
    for (index, hit) in result.hits().iter().enumerate() {
        let explanation = hit.explanation();
        let sources = explanation
            .sources()
            .iter()
            .map(|source| {
                format!(
                    "{}:{}",
                    channel_name(source.reason().source_channel()),
                    source.normalized_score().get()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            output,
            "rank={} priority={} target={} score={} source_points={} sources={} goal={}/{} step={}/{} freshness={}/{} token={}/{}/{} corroboration={}/{} redundancy={}/{}",
            index + 1,
            priority_name(explanation.priority()),
            target_name(hit.target()),
            explanation.final_score().get(),
            explanation.source_contribution().get(),
            sources,
            explanation.goal().signal().get(),
            explanation.goal().contribution().get(),
            explanation.step().signal().get(),
            explanation.step().contribution().get(),
            explanation.freshness().signal().get(),
            explanation.freshness().contribution().get(),
            explanation.token().token_cost().get(),
            explanation.token().efficiency().get(),
            explanation.token().contribution().get(),
            explanation.corroboration().signal().get(),
            explanation.corroboration().contribution().get(),
            explanation.redundancy().signal().get(),
            explanation.redundancy().contribution().get(),
        )?;
    }
    Ok(())
}

fn cases() -> Result<Vec<FusionEvalCase>, Box<dyn std::error::Error>> {
    Ok(vec![mixed_channels()?, stable_tie_with_limit()?])
}

fn mixed_channels() -> Result<FusionEvalCase, Box<dyn std::error::Error>> {
    let run_id = IndexRunId::from_bytes([11; 32]);
    let snapshot_id = SnapshotId::from_bytes([12; 32]);
    let exact = revision(b"src/exact.rs", [1; 32])?;
    let popular = revision(b"src/popular.rs", [2; 32])?;
    let goal = revision(b"src/goal.rs", [3; 32])?;
    let redundant = revision(b"src/redundant.rs", [4; 32])?;
    let test = revision(b"tests/exact_test.rs", [5; 32])?;
    let dependency = revision(b"src/dependency.rs", [6; 32])?;

    let exact_hit =
        ExactSearchHit::file(exact.clone(), ExactSearchExplanation::NormalizedPathExact)?;
    let exact_page = ExactSearchPage::new(
        run_id,
        snapshot_id,
        vec![exact_hit],
        None,
        ExactSearchPageSize::DEFAULT,
    )?;
    let exact_signals = signals(100, 100, CandidateFreshness::Current, 500, 0)?;
    let exact_set = RetrievalCandidateSet::from_exact_page(&exact_page, &[exact_signals])?;

    let lexical_page = LexicalSearchPage::new(
        run_id,
        snapshot_id,
        vec![
            LexicalSearchHit::file(exact.clone(), LexicalScore::new(90_000)?),
            LexicalSearchHit::file(goal, LexicalScore::new(80_000)?),
            LexicalSearchHit::file(redundant, LexicalScore::new(80_000)?),
        ],
        None,
        LexicalSearchPageSize::new(3)?,
    )?;
    let lexical_set = RetrievalCandidateSet::from_lexical_page(
        &lexical_page,
        &[
            exact_signals,
            signals(9_000, 9_000, CandidateFreshness::Current, 100, 0)?,
            signals(10_000, 10_000, CandidateFreshness::Current, 1, 10_000)?,
        ],
    )?;

    let range = SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?;
    let edge = GraphEdge::new(
        GraphEndpoint::File(test.path().clone()),
        GraphEndpoint::File(exact.path().clone()),
        SyntaxRelationKind::Tests,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterFile,
        snapshot_id,
        EvidenceRef::new(test.clone(), range),
    );
    let test_query = TraversalQuery::tests(
        GraphEndpoint::File(exact.path().clone()),
        TraversalResultLimit::DEFAULT,
    );
    let test_hit = GraphTraversalHit::new(
        ExactSearchTarget::File(test),
        vec![edge],
        &test_query,
        snapshot_id,
    )?;
    let test_result =
        GraphTraversalResult::new(run_id, snapshot_id, test_query, vec![test_hit], false)?;
    let test_set = RetrievalCandidateSet::from_graph_result(
        &test_result,
        &[signals(8_000, 8_000, CandidateFreshness::Current, 200, 0)?],
    )?;

    let graph_edge = GraphEdge::new(
        GraphEndpoint::File(exact.path().clone()),
        GraphEndpoint::File(dependency.path().clone()),
        SyntaxRelationKind::Calls,
        SyntaxProvider::TreeSitter,
        Confidence::certain(),
        LinkResolution::AdapterFile,
        snapshot_id,
        EvidenceRef::new(exact.clone(), range),
    );
    let graph_query = TraversalQuery::new(
        GraphEndpoint::File(exact.path().clone()),
        TraversalDirection::Outgoing,
        SyntaxRelationKind::Calls,
        TraversalDepth::DIRECT,
        TraversalResultLimit::DEFAULT,
    );
    let graph_hit = GraphTraversalHit::new(
        ExactSearchTarget::File(dependency),
        vec![graph_edge],
        &graph_query,
        snapshot_id,
    )?;
    let graph_result =
        GraphTraversalResult::new(run_id, snapshot_id, graph_query, vec![graph_hit], false)?;
    let graph_set = RetrievalCandidateSet::from_graph_result(
        &graph_result,
        &[signals(7_000, 9_000, CandidateFreshness::Current, 150, 0)?],
    )?;

    let semantic_set = RetrievalCandidateSet::complete(
        run_id,
        snapshot_id,
        SourceChannel::Semantic,
        vec![RetrievalCandidate::semantic(
            ExactSearchTarget::File(popular),
            NormalizedRetrievalSignal::FULL,
            signals(10_000, 10_000, CandidateFreshness::Current, 1, 0)?,
        )],
    )?;
    let candidates = RetrievalCandidateSets::new(
        run_id,
        snapshot_id,
        vec![semantic_set, test_set, graph_set, lexical_set, exact_set],
    )?;
    Ok(FusionEvalCase {
        name: "mixed-channels",
        candidates,
        limit: FusionResultLimit::DEFAULT,
    })
}

fn stable_tie_with_limit() -> Result<FusionEvalCase, Box<dyn std::error::Error>> {
    let run_id = IndexRunId::from_bytes([21; 32]);
    let snapshot_id = SnapshotId::from_bytes([22; 32]);
    let shared_signals = signals(5_000, 5_000, CandidateFreshness::Compatible, 1_000, 0)?;
    let a = revision(b"src/a.rs", [6; 32])?;
    let b = revision(b"src/b.rs", [7; 32])?;
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let memory_set = RetrievalCandidateSet::complete(
        run_id,
        snapshot_id,
        SourceChannel::Memory,
        vec![
            RetrievalCandidate::memory(
                ExactSearchTarget::File(b.clone()),
                MemoryCandidateExplanation::new(
                    NormalizedRetrievalSignal::new(6_000)?,
                    vec![EvidenceRef::new(b, range)],
                )?,
                shared_signals,
            ),
            RetrievalCandidate::memory(
                ExactSearchTarget::File(a.clone()),
                MemoryCandidateExplanation::new(
                    NormalizedRetrievalSignal::new(6_000)?,
                    vec![EvidenceRef::new(a, range)],
                )?,
                shared_signals,
            ),
        ],
    )?;
    Ok(FusionEvalCase {
        name: "stable-tie-limit",
        candidates: RetrievalCandidateSets::new(run_id, snapshot_id, vec![memory_set])?,
        limit: FusionResultLimit::new(1)?,
    })
}

fn signals(
    goal: u16,
    step: u16,
    freshness: CandidateFreshness,
    tokens: u32,
    redundancy: u16,
) -> Result<RetrievalCandidateSignals, Box<dyn std::error::Error>> {
    Ok(RetrievalCandidateSignals::new(
        NormalizedRetrievalSignal::new(goal)?,
        NormalizedRetrievalSignal::new(step)?,
        freshness,
        CandidateTokenCost::new(tokens)?,
        NormalizedRetrievalSignal::new(redundancy)?,
    ))
}

fn revision(path: &[u8], hash: [u8; 32]) -> Result<FileRevision, Box<dyn std::error::Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.to_vec())?,
        ContentHash::from_bytes(hash),
    ))
}

fn target_name(target: &ExactSearchTarget) -> String {
    match target {
        ExactSearchTarget::File(revision) => {
            format!(
                "file:{}",
                String::from_utf8_lossy(revision.path().as_bytes())
            )
        }
        ExactSearchTarget::Symbol(symbol) => format!("symbol:{}", symbol.symbol().id()),
    }
}

const fn priority_name(priority: FusionPriority) -> &'static str {
    match priority {
        FusionPriority::Exact => "exact",
        FusionPriority::Evidence => "evidence",
        FusionPriority::Semantic => "semantic",
    }
}

const fn channel_name(channel: SourceChannel) -> &'static str {
    match channel {
        SourceChannel::Exact => "exact",
        SourceChannel::Lexical => "lexical",
        SourceChannel::Graph => "graph",
        SourceChannel::Test => "test",
        SourceChannel::Memory => "memory",
        SourceChannel::Semantic => "semantic",
    }
}

#[test]
fn semantic_sources_remain_explicitly_non_evidentiary() -> Result<(), Box<dyn std::error::Error>> {
    let result =
        FusionPolicy::v1().fuse(mixed_channels()?.candidates, FusionResultLimit::DEFAULT)?;
    let semantic = result
        .hits()
        .iter()
        .find(|hit| hit.explanation().priority() == FusionPriority::Semantic)
        .ok_or("golden fixture has no semantic-only result")?;
    assert!(matches!(
        semantic.explanation().sources()[0].reason(),
        RetrievalCandidateReason::Semantic(_)
    ));
    Ok(())
}

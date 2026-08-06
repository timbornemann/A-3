//! Contract tests for direct and one-hop Module Card invalidation.

use a3_domain::{
    Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
    GraphSymbol, IndexInvalidationPlan, IndexLanguage, IndexPublication, IndexRunId,
    IndexRunRecord, IndexRunSequence, IndexRunStatus, InvalidationPlanError, InvalidationReason,
    LinkResolution, LinkedGraph, LocalSymbolId, MapperProfileVersion, ModuleCardId,
    ModuleCardInvalidationCandidate, ModuleCardStatus, ModuleId, ModuleKind, ModuleMembership,
    ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet,
    ParsedSymbol, PublishedIndex, RankProjection, RankScore, RankingPolicyVersion, RemapPriority,
    RepositoryCard, RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId, SymbolKind,
    SymbolName, SymbolRank, SymbolRankSignals, SyntaxProvider, SyntaxRelationKind,
};

#[test]
fn direct_change_only_invalidates_the_owner_and_one_hop_dependents()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let candidates = vec![
        candidate(1, fixture.dependency, true, false),
        candidate(2, fixture.dependent, true, true),
        candidate(3, fixture.unrelated, true, true),
    ];

    let plan =
        IndexInvalidationPlan::compile(&fixture.published, MapperProfileVersion::V1, candidates)?;

    assert_eq!(plan.invalidations().len(), 2);
    assert_eq!(plan.invalidations()[0].module_id(), fixture.dependency);
    assert_eq!(plan.invalidations()[0].status(), ModuleCardStatus::Stale);
    assert_eq!(
        plan.invalidations()[0].reason(),
        InvalidationReason::EvidenceChanged
    );
    assert_eq!(plan.invalidations()[1].module_id(), fixture.dependent);
    assert_eq!(
        plan.invalidations()[1].status(),
        ModuleCardStatus::NeedsReview
    );
    assert_eq!(plan.remaps().len(), 2);
    assert_eq!(plan.remaps()[0].module_id(), fixture.dependency);
    assert_eq!(plan.remaps()[0].priority(), RemapPriority::Direct);
    assert_eq!(plan.remaps()[1].module_id(), fixture.dependent);
    assert_eq!(plan.remaps()[1].priority(), RemapPriority::Dependent);
    assert!(
        plan.invalidations()
            .iter()
            .all(|item| item.module_id() != fixture.unrelated)
    );
    Ok(())
}

#[test]
fn parser_mapper_and_removed_module_are_explicit_direct_reasons()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let removed = ModuleId::from_bytes([99; 32]);
    let candidates = vec![
        ModuleCardInvalidationCandidate::new(
            IndexRunId::from_bytes([10; 32]),
            SnapshotId::from_bytes([10; 32]),
            ModuleCardId::from_bytes([10; 32]),
            fixture.dependency,
            MapperProfileVersion::new(2)?,
            true,
            true,
        ),
        candidate(2, fixture.dependent, false, true),
        candidate(3, fixture.unrelated, true, true),
        candidate(4, removed, true, true),
    ];

    let plan =
        IndexInvalidationPlan::compile(&fixture.published, MapperProfileVersion::V1, candidates)?;
    let by_module = plan
        .invalidations()
        .iter()
        .map(|item| (item.module_id(), item.reason()))
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(
        by_module.get(&fixture.dependency),
        Some(&InvalidationReason::MapperVersionChanged)
    );
    assert_eq!(
        by_module.get(&fixture.dependent),
        Some(&InvalidationReason::ParserVersionChanged)
    );
    assert_eq!(
        by_module.get(&removed),
        Some(&InvalidationReason::ModuleRemoved)
    );
    assert!(!plan.remaps().iter().any(|item| item.module_id() == removed));
    assert!(!by_module.contains_key(&fixture.unrelated));
    Ok(())
}

#[test]
fn duplicate_latest_card_candidates_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let result = IndexInvalidationPlan::compile(
        &fixture.published,
        MapperProfileVersion::V1,
        vec![
            candidate(1, fixture.dependency, true, true),
            candidate(2, fixture.dependency, true, true),
        ],
    );
    assert_eq!(result, Err(InvalidationPlanError::DuplicateModuleCandidate));
    assert!(MapperProfileVersion::new(0).is_err());
    Ok(())
}

struct Fixture {
    published: PublishedIndex,
    dependency: ModuleId,
    dependent: ModuleId,
    unrelated: ModuleId,
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let snapshot_id = SnapshotId::from_bytes([70; 32]);
    let dependency_revision = revision("src/dependency.rs", 1)?;
    let dependent_revision = revision("src/dependent.rs", 2)?;
    let unrelated_revision = revision("src/unrelated.rs", 3)?;
    let dependency_symbol_id = SymbolId::from_bytes([11; 32]);
    let dependent_symbol_id = SymbolId::from_bytes([12; 32]);
    let unrelated_symbol_id = SymbolId::from_bytes([13; 32]);
    let symbols = vec![
        symbol(
            dependency_symbol_id,
            dependency_revision.clone(),
            "dependency",
            1,
        )?,
        symbol(
            dependent_symbol_id,
            dependent_revision.clone(),
            "dependent",
            2,
        )?,
        symbol(
            unrelated_symbol_id,
            unrelated_revision.clone(),
            "unrelated",
            3,
        )?,
    ];
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![
            dependency_revision.clone(),
            dependent_revision.clone(),
            unrelated_revision.clone(),
        ],
        symbols,
        vec![GraphEdge::new(
            GraphEndpoint::Symbol(dependent_symbol_id),
            GraphEndpoint::Symbol(dependency_symbol_id),
            SyntaxRelationKind::Calls,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterLocalSymbol,
            snapshot_id,
            EvidenceRef::new(dependent_revision.clone(), range()?),
        )],
        Vec::new(),
    )?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![
            rank(dependency_symbol_id, 3_000)?,
            rank(dependent_symbol_id, 2_000)?,
            rank(unrelated_symbol_id, 1_000)?,
        ],
    )?;
    let dependency = ModuleId::from_bytes([21; 32]);
    let dependent = ModuleId::from_bytes([22; 32]);
    let unrelated = ModuleId::from_bytes([23; 32]);
    let modules = vec![
        module(dependency, dependency_symbol_id)?,
        module(dependent, dependent_symbol_id)?,
        module(unrelated, unrelated_symbol_id)?,
    ];
    let memberships = vec![
        ModuleMembership::new(
            dependency,
            dependency_symbol_id,
            ModuleMembershipEvidence::path(dependency_revision),
        ),
        ModuleMembership::new(
            dependent,
            dependent_symbol_id,
            ModuleMembershipEvidence::path(dependent_revision),
        ),
        ModuleMembership::new(
            unrelated,
            unrelated_symbol_id,
            ModuleMembershipEvidence::path(unrelated_revision),
        ),
    ];
    let card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![dependency, dependent, unrelated],
        vec![IndexLanguage::Rust],
        ModuleSymbolSet::empty(),
        3,
        3,
    )?;
    let projection = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        modules,
        memberships,
        card,
    )?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), projection)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([71; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(7)?,
        IndexRunStatus::Published,
    );
    Ok(Fixture {
        published: PublishedIndex::new(run, publication)?,
        dependency,
        dependent,
        unrelated,
    })
}

fn candidate(
    id: u8,
    module_id: ModuleId,
    parser_versions_compatible: bool,
    evidence_is_current: bool,
) -> ModuleCardInvalidationCandidate {
    ModuleCardInvalidationCandidate::new(
        IndexRunId::from_bytes([id; 32]),
        SnapshotId::from_bytes([id.saturating_add(20); 32]),
        ModuleCardId::from_bytes([id.saturating_add(40); 32]),
        module_id,
        MapperProfileVersion::V1,
        parser_versions_compatible,
        evidence_is_current,
    )
}

fn module(
    module_id: ModuleId,
    symbol_id: SymbolId,
) -> Result<a3_domain::RepositoryModule, Box<dyn std::error::Error>> {
    Ok(a3_domain::RepositoryModule::new(
        module_id,
        ModuleKind::PathBoundary,
        Some(ModuleRoot::Repository),
        Vec::new(),
        ModuleSymbolSet::new(vec![symbol_id], false)?,
        ModuleSymbolSet::empty(),
        ModuleSymbolSet::empty(),
    )?)
}

fn symbol(
    id: SymbolId,
    revision: FileRevision,
    name: &str,
    local_id: u32,
) -> Result<GraphSymbol, Box<dyn std::error::Error>> {
    Ok(GraphSymbol::new(
        id,
        revision,
        ParsedSymbol::new(
            LocalSymbolId::new(local_id)?,
            SymbolKind::Function,
            SymbolName::try_from_string(name.to_owned())?,
            range()?,
            range()?,
        )?,
    ))
}

fn rank(id: SymbolId, score: u32) -> Result<SymbolRank, Box<dyn std::error::Error>> {
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

fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn std::error::Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}

fn range() -> Result<SourceRange, Box<dyn std::error::Error>> {
    Ok(SourceRange::new(
        0,
        1,
        SourcePosition::new(0, 0),
        SourcePosition::new(0, 1),
    )?)
}

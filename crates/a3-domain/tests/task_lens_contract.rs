//! R10 Task Lens domain contracts over a deterministic bug fixture.

use a3_domain::{
    CandidateFreshness, CandidateTokenCost, Centrality, Confidence, ContentHash,
    ExactSearchExplanation, ExactSearchHit, ExactSearchSymbol, FileRevision, FusionPolicy,
    FusionResultLimit, GraphSymbol, IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord,
    IndexRunSequence, IndexRunStatus, LinkedGraph, LocalSymbolId, ModuleCardClaimId,
    ModuleCardEvidenceId, ModuleClaimPolarity, ModuleClaimPredicate, ModuleClaimStatement,
    ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion,
    ModuleProjection, ModuleRoot, ModuleSymbolSet, NormalizedRetrievalSignal, ParsedSymbol,
    PublishedIndex, QualifiedSymbolName, RankProjection, RankScore, RankingPolicyVersion,
    RepositoryCard, RepositoryModule, RepositoryPath, ResolvedModuleCardEvidence,
    RetrievalCandidate, RetrievalCandidateSet, RetrievalCandidateSets, RetrievalCandidateSignals,
    SnapshotId, SourceChannel, SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName,
    SymbolRank, SymbolRankSignals, SymbolRole, TaskLensClaim, TaskLensPolicy, TaskLensSeed,
    TaskLensSeedSet, TaskLensSeedText, TaskLensTarget, TaskLensTokenBudget, TaskLensZoomLevel,
    VerifiedClaimKind, VerifiedClaimStatus,
};

#[test]
fn bug_lens_keeps_production_and_test_but_excludes_irrelevant_module_and_stale_fact()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture(1)?;
    let seeds = seeds()?;
    let fused = fused(&fixture.published, &[fixture.production, fixture.test])?;
    let stale_fact = TaskLensClaim::new(
        IndexRunId::from_bytes([200; 32]),
        fixture.published.run().snapshot_id(),
        ModuleCardClaimId::from_bytes([21; 32]),
        fixture.relevant_module,
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Symbol(fixture.production),
        VerifiedClaimKind::Fact,
        VerifiedClaimStatus::Active,
        Confidence::certain(),
        vec![resolved_symbol(&fixture.published, fixture.production)?],
    )?;
    let current_hypothesis = TaskLensClaim::new(
        fixture.published.run().id(),
        fixture.published.run().snapshot_id(),
        ModuleCardClaimId::from_bytes([22; 32]),
        fixture.relevant_module,
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::ArchitecturalIntent(ModuleClaimStatement::try_from_string(
            "production and regression test intentionally evolve together".to_owned(),
        )?),
        VerifiedClaimKind::Hypothesis,
        VerifiedClaimStatus::Active,
        Confidence::from_basis_points(5_000)?,
        Vec::new(),
    )?;
    assert!(
        TaskLensClaim::new(
            fixture.published.run().id(),
            fixture.published.run().snapshot_id(),
            ModuleCardClaimId::from_bytes([23; 32]),
            fixture.relevant_module,
            ModuleClaimPolarity::Affirms,
            ModuleClaimPredicate::ArchitecturalIntent(ModuleClaimStatement::try_from_string(
                "must never be reconstructed as a Fact".to_owned(),
            )?),
            VerifiedClaimKind::Fact,
            VerifiedClaimStatus::Active,
            Confidence::certain(),
            vec![resolved_symbol(&fixture.published, fixture.production)?],
        )
        .is_err()
    );

    let claims = vec![stale_fact, current_hypothesis];
    let policy = TaskLensPolicy::v1();
    let lens = policy.compile(
        &fixture.published,
        seeds.clone(),
        &fused,
        claims.clone(),
        TaskLensTokenBudget::DEFAULT,
    )?;
    let repeated = policy.compile(
        &fixture.published,
        seeds,
        &fused,
        claims,
        TaskLensTokenBudget::DEFAULT,
    )?;

    assert!(lens.estimated_tokens() <= lens.token_budget().get());
    assert_eq!(lens.excluded_stale_claims(), 1);
    assert_eq!(lens.claims().len(), 1);
    assert_eq!(lens.claims()[0].kind(), VerifiedClaimKind::Hypothesis);
    assert!(lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Symbol(symbol) if symbol.id() == fixture.production
    )));
    assert!(lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Symbol(symbol) if symbol.id() == fixture.test
    )));
    assert!(!lens.entries().iter().any(|entry| matches!(
        entry.target(),
        TaskLensTarget::Module(module) if module.id() == fixture.irrelevant_module
    )));
    assert!(
        lens.entries()
            .iter()
            .any(|entry| entry.zoom_level() == TaskLensZoomLevel::L0Repository)
    );
    assert!(
        lens.entries()
            .iter()
            .any(|entry| entry.zoom_level() == TaskLensZoomLevel::L1Module)
    );
    assert!(
        lens.entries()
            .iter()
            .any(|entry| entry.zoom_level() == TaskLensZoomLevel::L2Symbol)
    );
    assert!(
        lens.entries()
            .iter()
            .any(|entry| entry.zoom_level() == TaskLensZoomLevel::L3SourceSpan)
    );
    assert_eq!(lens.digest(), repeated.digest());
    Ok(())
}

#[test]
fn seed_order_is_canonical_and_index_delta_changes_lens_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let first = fixture(1)?;
    let second = fixture(2)?;
    let left = TaskLensSeedSet::new(
        seed_text("fix the parser failure")?,
        seed_text("locate implementation and test")?,
        vec![
            TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
            TaskLensSeed::ExplicitIdentifier(seed_text("broken")?),
        ],
    )?;
    let right = TaskLensSeedSet::new(
        seed_text("fix the parser failure")?,
        seed_text("locate implementation and test")?,
        vec![
            TaskLensSeed::ExplicitIdentifier(seed_text("broken")?),
            TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
        ],
    )?;
    assert_eq!(left, right);
    assert!(
        TaskLensSeedSet::new(
            seed_text("goal")?,
            seed_text("step")?,
            vec![
                TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
                TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
            ],
        )
        .is_err()
    );
    assert!(TaskLensTokenBudget::new(255).is_err());

    let first_fused = fused(&first.published, &[first.production])?;
    let second_fused = fused(&second.published, &[second.production])?;
    let first_lens = TaskLensPolicy::v1().compile(
        &first.published,
        left,
        &first_fused,
        Vec::new(),
        TaskLensTokenBudget::DEFAULT,
    )?;
    let second_lens = TaskLensPolicy::v1().compile(
        &second.published,
        right,
        &second_fused,
        Vec::new(),
        TaskLensTokenBudget::DEFAULT,
    )?;

    assert_ne!(first_lens.digest(), second_lens.digest());
    assert!(!first_lens.is_current_for(&second.published));
    assert!(second_lens.is_current_for(&second.published));
    Ok(())
}

struct Fixture {
    published: PublishedIndex,
    production: SymbolId,
    test: SymbolId,
    relevant_module: ModuleId,
    irrelevant_module: ModuleId,
}

fn fixture(generation: u8) -> Result<Fixture, Box<dyn std::error::Error>> {
    let snapshot_id = SnapshotId::from_bytes([generation; 32]);
    let production_revision = revision("src/bug.rs", generation.saturating_add(10))?;
    let test_revision = revision("tests/bug_test.rs", generation.saturating_add(20))?;
    let irrelevant_revision = revision("vendor/huge.rs", generation.saturating_add(30))?;
    let production = SymbolId::from_bytes([11; 32]);
    let test = SymbolId::from_bytes([12; 32]);
    let irrelevant = SymbolId::from_bytes([13; 32]);
    let production_symbol = symbol(production, production_revision.clone(), "broken", false)?;
    let test_symbol = symbol(test, test_revision.clone(), "regression", true)?;
    let irrelevant_symbol = symbol(
        irrelevant,
        irrelevant_revision.clone(),
        "generated_vendor_blob",
        false,
    )?;
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![
            production_revision.clone(),
            test_revision.clone(),
            irrelevant_revision.clone(),
        ],
        vec![production_symbol, test_symbol, irrelevant_symbol],
        Vec::new(),
        Vec::new(),
    )?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![
            rank(production, 3_000)?,
            rank(test, 2_000)?,
            rank(irrelevant, 1_000)?,
        ],
    )?;
    let relevant_module = ModuleId::from_bytes([31; 32]);
    let irrelevant_module = ModuleId::from_bytes([32; 32]);
    let relevant = RepositoryModule::new(
        relevant_module,
        ModuleKind::PathBoundary,
        Some(ModuleRoot::Repository),
        Vec::new(),
        ModuleSymbolSet::new(vec![production], false)?,
        ModuleSymbolSet::empty(),
        ModuleSymbolSet::new(vec![test], false)?,
    )?;
    let irrelevant_boundary = RepositoryModule::new(
        irrelevant_module,
        ModuleKind::PathBoundary,
        Some(ModuleRoot::Directory(path("vendor")?)),
        Vec::new(),
        ModuleSymbolSet::new(vec![irrelevant], false)?,
        ModuleSymbolSet::empty(),
        ModuleSymbolSet::empty(),
    )?;
    let memberships = vec![
        ModuleMembership::new(
            relevant_module,
            production,
            ModuleMembershipEvidence::path(production_revision),
        ),
        ModuleMembership::new(
            relevant_module,
            test,
            ModuleMembershipEvidence::path(test_revision),
        ),
        ModuleMembership::new(
            irrelevant_module,
            irrelevant,
            ModuleMembershipEvidence::path(irrelevant_revision),
        ),
    ];
    let card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![relevant_module, irrelevant_module],
        vec![IndexLanguage::Rust],
        ModuleSymbolSet::empty(),
        3,
        3,
    )?;
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![relevant, irrelevant_boundary],
        memberships,
        card,
    )?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([generation.saturating_add(100); 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(u64::from(generation))?,
        IndexRunStatus::Published,
    );
    Ok(Fixture {
        published: PublishedIndex::new(run, publication)?,
        production,
        test,
        relevant_module,
        irrelevant_module,
    })
}

fn fused(
    published: &PublishedIndex,
    symbols: &[SymbolId],
) -> Result<a3_domain::FusedRetrievalResult, Box<dyn std::error::Error>> {
    let signals = RetrievalCandidateSignals::new(
        NormalizedRetrievalSignal::FULL,
        NormalizedRetrievalSignal::FULL,
        CandidateFreshness::Current,
        CandidateTokenCost::new(128)?,
        NormalizedRetrievalSignal::ZERO,
    );
    let candidates = symbols
        .iter()
        .map(|symbol_id| {
            let symbol = published
                .publication()
                .graph()
                .symbols()
                .iter()
                .find(|symbol| symbol.id() == *symbol_id)
                .ok_or("fixture symbol is missing")?
                .clone();
            let exact = ExactSearchHit::symbol(
                ExactSearchSymbol::new(
                    symbol.clone(),
                    QualifiedSymbolName::try_from_string(
                        symbol.parsed().name().as_str().to_owned(),
                    )?,
                ),
                ExactSearchExplanation::QualifiedNameExact,
            )?;
            Ok::<_, Box<dyn std::error::Error>>(RetrievalCandidate::from_exact(&exact, signals))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let set = RetrievalCandidateSet::complete(
        published.run().id(),
        published.run().snapshot_id(),
        SourceChannel::Exact,
        candidates,
    )?;
    Ok(FusionPolicy::v1().fuse(
        RetrievalCandidateSets::new(
            published.run().id(),
            published.run().snapshot_id(),
            vec![set],
        )?,
        FusionResultLimit::DEFAULT,
    )?)
}

fn resolved_symbol(
    published: &PublishedIndex,
    symbol_id: SymbolId,
) -> Result<ResolvedModuleCardEvidence, Box<dyn std::error::Error>> {
    let symbol = published
        .publication()
        .graph()
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == symbol_id)
        .ok_or("fixture symbol is missing")?
        .clone();
    Ok(ResolvedModuleCardEvidence::Symbol {
        id: ModuleCardEvidenceId::for_symbol_v1(&symbol),
        symbol,
    })
}

fn symbol(
    id: SymbolId,
    revision: FileRevision,
    name: &str,
    is_test: bool,
) -> Result<GraphSymbol, Box<dyn std::error::Error>> {
    let range = SourceRange::new(0, 32, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?;
    let parsed = ParsedSymbol::new(
        LocalSymbolId::new(u32::from(id.as_bytes()[0]))?,
        SymbolKind::Function,
        SymbolName::try_from_string(name.to_owned())?,
        range,
        range,
    )?;
    Ok(GraphSymbol::new(
        id,
        revision,
        if is_test {
            parsed.with_role(SymbolRole::Test)
        } else {
            parsed
        },
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

fn path(value: &str) -> Result<RepositoryPath, Box<dyn std::error::Error>> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

fn seed_text(value: &str) -> Result<TaskLensSeedText, Box<dyn std::error::Error>> {
    Ok(TaskLensSeedText::try_from_string(value.to_owned())?)
}

fn seeds() -> Result<TaskLensSeedSet, Box<dyn std::error::Error>> {
    Ok(TaskLensSeedSet::new(
        seed_text("fix the parser failure")?,
        seed_text("locate implementation and regression test")?,
        vec![
            TaskLensSeed::ExplicitIdentifier(seed_text("broken")?),
            TaskLensSeed::ExplicitPath(path("src/bug.rs")?),
        ],
    )?)
}

//! Contract coverage for the bounded progressive Code Atlas projections.

use a3_application::{
    ModuleCardClaimPresentation, ModuleCardClaimState, ModuleCardDetail, ModuleCardDetailField,
    ModuleCardLifecycle, ModuleCardValuePresentation, ProjectMapAtlasLevel,
    ProjectMapAtlasModuleInsight, ProjectMapAtlasNodeKind, ProjectMapAtlasSceneQuery,
    ProjectMapEntitySelection, ProjectMapFlowPreset, ProjectMapFlowSceneQuery,
    ProjectMapIndexEvidenceSelection, ProjectMapInventoryPageQuery, ProjectMapInventoryView,
    build_project_map_atlas_scene, build_project_map_atlas_scene_with_insights,
    build_project_map_entity_context, build_project_map_entity_context_with_insights,
    build_project_map_flow_scene, build_project_map_inventory_page,
    resolve_project_map_index_evidence,
};
use a3_domain::{
    Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
    GraphSymbol, IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence,
    IndexRunStatus, LinkResolution, LinkedGraph, LocalSymbolId, MapperProfileVersion,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleCardField, ModuleCardId,
    ModuleCardSchemaVersion, ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
    ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, ParsedSymbol,
    PublishedIndex, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
    RepositoryModule, RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId,
    SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SymbolReference, SymbolRole,
    SymbolVisibility, SyntaxProvider, SyntaxRelationKind, UnresolvedEdgeCandidate,
    UnresolvedGraphTarget, UnresolvedReason, VerifiedClaimKind,
};
use std::error::Error;

#[test]
fn progressive_scenes_rank_and_revalidate_every_semantic_level() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let overview =
        build_project_map_atlas_scene(&fixture.published, &ProjectMapAtlasSceneQuery::new(None))?
            .ok_or("missing project scene")?;
    assert_eq!(overview.level(), ProjectMapAtlasLevel::Project);
    assert_eq!(overview.nodes().len(), 1);

    let module_selection = overview.nodes()[0]
        .selection()
        .ok_or("missing module selection")?;
    let module_scene = build_project_map_atlas_scene(
        &fixture.published,
        &ProjectMapAtlasSceneQuery::new(Some(module_selection)),
    )?
    .ok_or("missing module scene")?;
    assert_eq!(module_scene.level(), ProjectMapAtlasLevel::Module);
    assert_eq!(module_scene.nodes()[0].display_name(), "lib.rs");
    assert_eq!(module_scene.nodes()[1].display_name(), "atlas_test.rs");

    let file_selection = module_scene.nodes()[0]
        .selection()
        .ok_or("missing file selection")?;
    let file_scene = build_project_map_atlas_scene(
        &fixture.published,
        &ProjectMapAtlasSceneQuery::new(Some(file_selection)),
    )?
    .ok_or("missing file scene")?;
    assert_eq!(file_scene.level(), ProjectMapAtlasLevel::File);
    assert_eq!(file_scene.nodes()[0].display_name(), "Atlas");

    let symbol_selection = file_scene.nodes()[0]
        .selection()
        .ok_or("missing symbol selection")?;
    let symbol_scene = build_project_map_atlas_scene(
        &fixture.published,
        &ProjectMapAtlasSceneQuery::new(Some(symbol_selection)),
    )?
    .ok_or("missing symbol scene")?;
    assert_eq!(symbol_scene.level(), ProjectMapAtlasLevel::Symbol);
    assert!(
        symbol_scene
            .nodes()
            .iter()
            .any(|node| node.display_name() == "run")
    );

    let context = build_project_map_entity_context(&fixture.published, symbol_selection)?
        .ok_or("missing symbol context")?;
    assert_eq!(context.relation_counts().len(), 13);
    assert_eq!(context.document_relation_count(), 1);

    let page = build_project_map_inventory_page(
        &fixture.published,
        &ProjectMapInventoryPageQuery::new(module_selection, ProjectMapInventoryView::Files, None),
    )?
    .ok_or("missing inventory page")?;
    assert_eq!(page.items().len(), 2);
    assert_eq!(page.total_count(), 2);
    Ok(())
}

#[test]
fn namespace_scene_ignores_self_edges() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::namespace_with_self_edge()?;
    let namespace_selection = ProjectMapEntitySelection::Symbol {
        module_id: fixture.module,
        symbol_id: fixture.root,
        evidence_id: ModuleCardEvidenceId::for_symbol_id_v1(fixture.root),
    };

    let scene = build_project_map_atlas_scene(
        &fixture.published,
        &ProjectMapAtlasSceneQuery::new(Some(namespace_selection)),
    )?
    .ok_or("missing namespace scene")?;

    assert_eq!(scene.level(), ProjectMapAtlasLevel::Symbol);
    assert_eq!(
        scene
            .nodes()
            .iter()
            .filter(|node| node.selection() == Some(namespace_selection))
            .count(),
        1
    );
    assert_eq!(scene.nodes()[0].kind(), ProjectMapAtlasNodeKind::Namespace);
    Ok(())
}

#[test]
fn flow_and_preview_evidence_remain_publication_bound() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let selection = ProjectMapEntitySelection::Symbol {
        module_id: fixture.module,
        symbol_id: fixture.root,
        evidence_id: a3_domain::ModuleCardEvidenceId::for_symbol_id_v1(fixture.root),
    };
    let flow = build_project_map_flow_scene(
        &fixture.published,
        &ProjectMapFlowSceneQuery::new(selection, ProjectMapFlowPreset::Callees),
    )?
    .ok_or("missing flow")?;
    assert!(flow.nodes().iter().any(|node| node.display_name() == "run"));
    assert!(flow.targets().iter().all(|target| target.depth() <= 2));
    let relation_evidence = flow.targets()[0].path()[0].evidence();
    let resolved = resolve_project_map_index_evidence(&fixture.published, relation_evidence)?
        .ok_or("missing relation evidence")?;
    assert_eq!(resolved.revision().path(), fixture.source.path());
    assert!(resolved.range().is_some());

    let stale = match relation_evidence {
        ProjectMapIndexEvidenceSelection::Relation {
            module_id,
            edge_sequence,
            ..
        } => ProjectMapIndexEvidenceSelection::Relation {
            module_id,
            edge_sequence,
            evidence_id: a3_domain::ModuleCardEvidenceId::from_bytes([99; 32]),
        },
        _ => return Err("expected relation evidence".into()),
    };
    assert!(resolve_project_map_index_evidence(&fixture.published, stale)?.is_none());
    Ok(())
}

#[test]
fn verified_card_insights_enrich_only_exact_current_evidence() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let evidence = ModuleCardEvidenceId::for_symbol_id_v1(fixture.root);
    let insight =
        ProjectMapAtlasModuleInsight::from_detail(&module_card_detail(fixture.module, evidence)?)?;
    let overview = build_project_map_atlas_scene_with_insights(
        &fixture.published,
        &ProjectMapAtlasSceneQuery::new(None),
        std::slice::from_ref(&insight),
    )?
    .ok_or("missing enriched overview")?;
    assert_eq!(
        overview.nodes()[0].purpose(),
        Some("Orchestriert den Atlas.")
    );
    assert_eq!(overview.nodes()[0].current_risk_count(), 2);

    let selection = ProjectMapEntitySelection::Symbol {
        module_id: fixture.module,
        symbol_id: fixture.root,
        evidence_id: evidence,
    };
    let context =
        build_project_map_entity_context_with_insights(&fixture.published, selection, &[insight])?
            .ok_or("missing enriched context")?;
    assert_eq!(context.entity().claim_badge_count(), 3);
    assert_eq!(context.claims().len(), 3);
    Ok(())
}

fn module_card_detail(
    module_id: ModuleId,
    evidence: ModuleCardEvidenceId,
) -> Result<ModuleCardDetail, Box<dyn Error>> {
    let purpose = detail_field(
        ModuleCardField::Purpose,
        &["Orchestriert den Atlas."],
        41,
        evidence,
    )?;
    let risks = detail_field(
        ModuleCardField::Risks,
        &["Grenze A", "Grenze B"],
        42,
        evidence,
    )?;
    Ok(ModuleCardDetail::new(
        IndexRunId::from_bytes([30; 32]),
        SnapshotId::from_bytes([1; 32]),
        IndexRunId::from_bytes([30; 32]),
        SnapshotId::from_bytes([1; 32]),
        ModuleCardId::from_bytes([40; 32]),
        module_id,
        ModuleCardSchemaVersion::V1,
        MapperProfileVersion::V1,
        Confidence::certain(),
        ModuleCardLifecycle::Current,
        vec![purpose, risks],
    )?)
}

fn detail_field(
    field: ModuleCardField,
    values: &[&str],
    first_claim: u8,
    evidence: ModuleCardEvidenceId,
) -> Result<ModuleCardDetailField, Box<dyn Error>> {
    let values = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let offset = u8::try_from(index)?;
            let claim = ModuleCardClaimPresentation::new(
                ModuleCardClaimId::from_bytes([first_claim.saturating_add(offset); 32]),
                VerifiedClaimKind::Fact,
                Confidence::certain(),
                ModuleCardClaimState::Current,
                vec![evidence],
            )?;
            Ok(ModuleCardValuePresentation::new((*value).to_owned(), claim))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(ModuleCardDetailField::new(field, values, vec![evidence])?)
}

struct Fixture {
    published: PublishedIndex,
    module: ModuleId,
    root: SymbolId,
    source: FileRevision,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::build(SymbolKind::Class, false)
    }

    fn namespace_with_self_edge() -> Result<Self, Box<dyn Error>> {
        Self::build(SymbolKind::Module, true)
    }

    fn build(root_kind: SymbolKind, include_self_edge: bool) -> Result<Self, Box<dyn Error>> {
        let snapshot = SnapshotId::from_bytes([1; 32]);
        let manifest = revision("Cargo.toml", 2)?;
        let source = revision("src/lib.rs", 3)?;
        let tests = revision("tests/atlas_test.rs", 4)?;
        let root = SymbolId::from_bytes([10; 32]);
        let member = SymbolId::from_bytes([11; 32]);
        let test = SymbolId::from_bytes([12; 32]);
        let root_symbol = symbol(root, source.clone(), 1, root_kind, "Atlas")?
            .with_visibility(SymbolVisibility::Public)
            .with_role(SymbolRole::Entrypoint);
        let member_symbol = symbol(member, source.clone(), 2, SymbolKind::Method, "run")?
            .with_visibility(SymbolVisibility::Public);
        let test_symbol = symbol(test, tests.clone(), 3, SymbolKind::Function, "maps_atlas")?
            .with_role(SymbolRole::Test);
        let range = root_symbol.declaration_range();
        let graph_symbols = vec![
            GraphSymbol::new(root, source.clone(), root_symbol),
            GraphSymbol::new(member, source.clone(), member_symbol),
            GraphSymbol::new(test, tests.clone(), test_symbol),
        ];
        let relations = [
            SyntaxRelationKind::Contains,
            SyntaxRelationKind::Defines,
            SyntaxRelationKind::Imports,
            SyntaxRelationKind::Exports,
            SyntaxRelationKind::Calls,
            SyntaxRelationKind::Implements,
            SyntaxRelationKind::Extends,
            SyntaxRelationKind::Reads,
            SyntaxRelationKind::Writes,
            SyntaxRelationKind::Configures,
            SyntaxRelationKind::Tests,
            SyntaxRelationKind::Builds,
            SyntaxRelationKind::Documents,
        ];
        let mut edges = relations
            .into_iter()
            .map(|kind| {
                GraphEdge::new(
                    GraphEndpoint::Symbol(root),
                    GraphEndpoint::Symbol(member),
                    kind,
                    SyntaxProvider::TreeSitter,
                    Confidence::certain(),
                    LinkResolution::AdapterLocalSymbol,
                    snapshot,
                    EvidenceRef::new(source.clone(), range),
                )
            })
            .collect::<Vec<_>>();
        edges.push(GraphEdge::new(
            GraphEndpoint::Symbol(test),
            GraphEndpoint::Symbol(member),
            SyntaxRelationKind::Tests,
            SyntaxProvider::TreeSitter,
            Confidence::certain(),
            LinkResolution::AdapterLocalSymbol,
            snapshot,
            EvidenceRef::new(tests.clone(), range),
        ));
        if include_self_edge {
            edges.push(GraphEdge::new(
                GraphEndpoint::Symbol(root),
                GraphEndpoint::Symbol(root),
                SyntaxRelationKind::Contains,
                SyntaxProvider::TreeSitter,
                Confidence::certain(),
                LinkResolution::AdapterLocalSymbol,
                snapshot,
                EvidenceRef::new(source.clone(), range),
            ));
        }
        let unresolved = UnresolvedEdgeCandidate::new(
            GraphEndpoint::Symbol(root),
            UnresolvedGraphTarget::Reference(SymbolReference::try_from_string(
                "runtime_target".to_owned(),
            )?),
            SyntaxRelationKind::Calls,
            SyntaxProvider::LanguageHeuristic,
            Confidence::certain(),
            UnresolvedReason::DynamicReference,
            snapshot,
            EvidenceRef::new(source.clone(), range),
        );
        let graph = LinkedGraph::new(
            snapshot,
            vec![manifest.clone(), source.clone(), tests.clone()],
            graph_symbols,
            edges,
            vec![unresolved],
        )?;
        let ranking = RankProjection::new(
            snapshot,
            RankingPolicyVersion::v1(),
            vec![rank(root, 3_000)?, rank(member, 2_000)?, rank(test, 1_000)?],
        )?;
        let module = ModuleId::from_bytes([20; 32]);
        let repository_module = RepositoryModule::new(
            module,
            ModuleKind::ManifestBoundary,
            Some(ModuleRoot::Repository),
            vec![manifest.clone()],
            ModuleSymbolSet::new(vec![root, member], false)?,
            ModuleSymbolSet::new(vec![root], false)?,
            ModuleSymbolSet::new(vec![test], false)?,
        )?;
        let projection = ModuleProjection::new(
            snapshot,
            ModulePolicyVersion::v1(),
            vec![repository_module],
            vec![
                ModuleMembership::new(
                    module,
                    root,
                    ModuleMembershipEvidence::manifest(source.clone(), manifest.clone()),
                ),
                ModuleMembership::new(
                    module,
                    member,
                    ModuleMembershipEvidence::manifest(source.clone(), manifest.clone()),
                ),
                ModuleMembership::new(
                    module,
                    test,
                    ModuleMembershipEvidence::manifest(tests.clone(), manifest.clone()),
                ),
            ],
            RepositoryCard::new(
                snapshot,
                ModulePolicyVersion::v1(),
                vec![module],
                vec![IndexLanguage::Rust],
                ModuleSymbolSet::new(vec![root], false)?,
                3,
                3,
            )?,
        )?;
        let publication = IndexPublication::new(graph, ranking, vec![manifest], projection)?;
        let run = IndexRunRecord::new(
            IndexRunId::from_bytes([30; 32]),
            snapshot,
            RankingPolicyVersion::v1(),
            IndexRunSequence::new(1)?,
            IndexRunStatus::Published,
        );
        Ok(Self {
            published: PublishedIndex::new(run, publication)?,
            module,
            root,
            source,
        })
    }
}

fn symbol(
    _global: SymbolId,
    revision: FileRevision,
    local: u32,
    kind: SymbolKind,
    name: &str,
) -> Result<ParsedSymbol, Box<dyn Error>> {
    let start_byte = usize::try_from(local)?.saturating_mul(10);
    let start_row = local.saturating_mul(10);
    let range = SourceRange::new(
        start_byte,
        start_byte.saturating_add(8),
        SourcePosition::new(start_row, 0),
        SourcePosition::new(start_row, 8),
    )?;
    let _ = revision;
    Ok(ParsedSymbol::new(
        LocalSymbolId::new(local)?,
        kind,
        SymbolName::try_from_string(name.to_owned())?,
        range,
        range,
    )?)
}

fn rank(symbol: SymbolId, score: u64) -> Result<SymbolRank, Box<dyn Error>> {
    Ok(SymbolRank::new(
        symbol,
        RankScore::try_from_sum(score)?,
        SymbolRankSignals {
            in_degree: 1,
            out_degree: 1,
            centrality: Centrality::from_basis_points(1_000)?,
            degree_contribution: 0,
            centrality_contribution: u32::try_from(score)?,
            entrypoint_contribution: 0,
            public_export_contribution: 0,
            manifest_contribution: 0,
            test_contribution: 0,
        },
    ))
}

fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}

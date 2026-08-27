use a3_application::{
    ProjectMapAtlasBreadcrumb, ProjectMapAtlasLevel, ProjectMapAtlasNode, ProjectMapAtlasNodeKind,
    ProjectMapAtlasRelation, ProjectMapAtlasScene, ProjectMapAtlasUncertainty,
    ProjectMapClaimReference, ProjectMapEntityContext, ProjectMapEntitySelection,
    ProjectMapFileOrdinal, ProjectMapFlowPreset, ProjectMapFlowScene, ProjectMapFlowSceneQuery,
    ProjectMapFlowStep, ProjectMapFlowTarget, ProjectMapIndexEvidenceSelection,
    ProjectMapInventoryCursor, ProjectMapInventoryPage, ProjectMapInventoryPageQuery,
    ProjectMapInventoryView, ProjectMapMappingStatus, ProjectMapRelationCount,
};
use a3_domain::{ModuleCardEvidenceId, ModuleId, SymbolId, SyntaxProvider, SyntaxRelationKind};
use a3_protocol::{
    ProjectMapAtlasBreadcrumbV1, ProjectMapAtlasLevelV1, ProjectMapAtlasNodeKindV1,
    ProjectMapAtlasNodeV1, ProjectMapAtlasRelationV1, ProjectMapAtlasSceneV1,
    ProjectMapAtlasUncertaintyV1, ProjectMapClaimReferenceV1, ProjectMapEntityContextV1,
    ProjectMapEntitySelectionV1, ProjectMapFlowPresetV1, ProjectMapFlowSceneV1,
    ProjectMapFlowStepV1, ProjectMapFlowTargetV1, ProjectMapIndexEvidenceSelectionV1,
    ProjectMapInventoryPageV1, ProjectMapInventoryViewV1, ProjectMapMappingStatusV1,
    ProjectMapRelationCountV1, ProjectMapRelationKindV1, ProjectMapRelationProviderV1,
    QueryProjectMapFlowSceneRequestV1, QueryProjectMapInventoryPageRequestV1,
};

pub(crate) fn map_selection_from_v1(
    selection: &ProjectMapEntitySelectionV1,
) -> Result<ProjectMapEntitySelection, ()> {
    match selection {
        ProjectMapEntitySelectionV1::Module { module_id } => {
            Ok(ProjectMapEntitySelection::Module {
                module_id: ModuleId::from_bytes(decode_id(module_id)?),
            })
        }
        ProjectMapEntitySelectionV1::File {
            module_id,
            ordinal,
            evidence_id,
        } => Ok(ProjectMapEntitySelection::File {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            ordinal: ProjectMapFileOrdinal::new(*ordinal).map_err(|_| ())?,
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
        ProjectMapEntitySelectionV1::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } => Ok(ProjectMapEntitySelection::Symbol {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            symbol_id: SymbolId::from_bytes(decode_id(symbol_id)?),
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
    }
}

pub(crate) fn map_index_evidence_from_v1(
    selection: &ProjectMapIndexEvidenceSelectionV1,
) -> Result<ProjectMapIndexEvidenceSelection, ()> {
    match selection {
        ProjectMapIndexEvidenceSelectionV1::File {
            module_id,
            ordinal,
            evidence_id,
        } => Ok(ProjectMapIndexEvidenceSelection::File {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            ordinal: ProjectMapFileOrdinal::new(*ordinal).map_err(|_| ())?,
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
        ProjectMapIndexEvidenceSelectionV1::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } => Ok(ProjectMapIndexEvidenceSelection::Symbol {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            symbol_id: SymbolId::from_bytes(decode_id(symbol_id)?),
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
        ProjectMapIndexEvidenceSelectionV1::Relation {
            module_id,
            edge_sequence,
            evidence_id,
        } => Ok(ProjectMapIndexEvidenceSelection::Relation {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            edge_sequence: decode_sequence(edge_sequence)?,
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
        ProjectMapIndexEvidenceSelectionV1::UnresolvedRelation {
            module_id,
            candidate_sequence,
            evidence_id,
        } => Ok(ProjectMapIndexEvidenceSelection::UnresolvedRelation {
            module_id: ModuleId::from_bytes(decode_id(module_id)?),
            candidate_sequence: decode_sequence(candidate_sequence)?,
            evidence_id: ModuleCardEvidenceId::from_bytes(decode_id(evidence_id)?),
        }),
    }
}

pub(crate) fn map_inventory_query_from_v1(
    request: &QueryProjectMapInventoryPageRequestV1,
) -> Result<ProjectMapInventoryPageQuery, ()> {
    let cursor = request
        .cursor()
        .map(|value| ProjectMapInventoryCursor::try_from_string(value.to_owned()).map_err(|_| ()))
        .transpose()?;
    Ok(ProjectMapInventoryPageQuery::new(
        map_selection_from_v1(request.selection())?,
        map_inventory_view_from_v1(request.view()),
        cursor,
    ))
}

pub(crate) fn map_flow_query_from_v1(
    request: &QueryProjectMapFlowSceneRequestV1,
) -> Result<ProjectMapFlowSceneQuery, ()> {
    Ok(ProjectMapFlowSceneQuery::new(
        map_selection_from_v1(request.selection())?,
        map_flow_preset_from_v1(request.preset()),
    ))
}

pub(crate) fn map_scene_to_v1(scene: &ProjectMapAtlasScene) -> ProjectMapAtlasSceneV1 {
    ProjectMapAtlasSceneV1::new(
        encode_id(scene.index_run_id().as_bytes()),
        encode_id(scene.snapshot_id().as_bytes()),
        map_level(scene.level()),
        scene.selection().map(map_selection_to_v1),
        scene.breadcrumb().iter().map(map_breadcrumb).collect(),
        scene.nodes().iter().map(map_node).collect(),
        scene.node_count().to_string(),
        scene.relations().iter().map(map_relation).collect(),
        scene.relation_count().to_string(),
        scene.boundary_count().to_string(),
        scene.unresolved_count().to_string(),
        scene.inspected_edge_count().to_string(),
        scene.nodes_truncated(),
        scene.relations_truncated(),
        scene.boundaries_truncated(),
        scene.source_edges_truncated(),
    )
}

pub(crate) fn map_context_to_v1(context: &ProjectMapEntityContext) -> ProjectMapEntityContextV1 {
    ProjectMapEntityContextV1::new(
        encode_id(context.index_run_id().as_bytes()),
        encode_id(context.snapshot_id().as_bytes()),
        map_node(context.entity()),
        context
            .relation_counts()
            .iter()
            .map(map_relation_count)
            .collect(),
        context.related_nodes().iter().map(map_node).collect(),
        context
            .architecture_relations()
            .iter()
            .map(map_relation)
            .collect(),
        context.architecture_relation_count().to_string(),
        context.boundary_nodes().iter().map(map_node).collect(),
        context
            .boundary_relations()
            .iter()
            .map(map_relation)
            .collect(),
        context.boundary_count().to_string(),
        context.document_relation_count().to_string(),
        context.claims().iter().map(map_claim).collect(),
        context.source_edges_truncated(),
    )
}

pub(crate) fn map_inventory_to_v1(page: &ProjectMapInventoryPage) -> ProjectMapInventoryPageV1 {
    ProjectMapInventoryPageV1::new(
        encode_id(page.index_run_id().as_bytes()),
        encode_id(page.snapshot_id().as_bytes()),
        map_selection_to_v1(page.selection()),
        map_inventory_view(page.view()),
        page.page_number(),
        page.total_count().to_string(),
        page.items().iter().map(map_node).collect(),
        page.previous_cursor()
            .map(|cursor| cursor.as_str().to_owned()),
        page.next_cursor().map(|cursor| cursor.as_str().to_owned()),
    )
}

pub(crate) fn map_flow_to_v1(flow: &ProjectMapFlowScene) -> ProjectMapFlowSceneV1 {
    ProjectMapFlowSceneV1::new(
        encode_id(flow.index_run_id().as_bytes()),
        encode_id(flow.snapshot_id().as_bytes()),
        map_flow_preset(flow.preset()),
        map_node(flow.root()),
        flow.nodes().iter().map(map_node).collect(),
        flow.targets().iter().map(map_flow_target).collect(),
        flow.target_count().to_string(),
        flow.inspected_edge_count().to_string(),
        flow.targets_truncated(),
        flow.source_edges_truncated(),
    )
}

fn map_selection_to_v1(selection: ProjectMapEntitySelection) -> ProjectMapEntitySelectionV1 {
    match selection {
        ProjectMapEntitySelection::Module { module_id } => ProjectMapEntitySelectionV1::Module {
            module_id: encode_id(module_id.as_bytes()),
        },
        ProjectMapEntitySelection::File {
            module_id,
            ordinal,
            evidence_id,
        } => ProjectMapEntitySelectionV1::File {
            module_id: encode_id(module_id.as_bytes()),
            ordinal: ordinal.get(),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
        ProjectMapEntitySelection::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } => ProjectMapEntitySelectionV1::Symbol {
            module_id: encode_id(module_id.as_bytes()),
            symbol_id: encode_id(symbol_id.as_bytes()),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
    }
}

fn map_evidence_to_v1(
    selection: ProjectMapIndexEvidenceSelection,
) -> ProjectMapIndexEvidenceSelectionV1 {
    match selection {
        ProjectMapIndexEvidenceSelection::File {
            module_id,
            ordinal,
            evidence_id,
        } => ProjectMapIndexEvidenceSelectionV1::File {
            module_id: encode_id(module_id.as_bytes()),
            ordinal: ordinal.get(),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
        ProjectMapIndexEvidenceSelection::Symbol {
            module_id,
            symbol_id,
            evidence_id,
        } => ProjectMapIndexEvidenceSelectionV1::Symbol {
            module_id: encode_id(module_id.as_bytes()),
            symbol_id: encode_id(symbol_id.as_bytes()),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
        ProjectMapIndexEvidenceSelection::Relation {
            module_id,
            edge_sequence,
            evidence_id,
        } => ProjectMapIndexEvidenceSelectionV1::Relation {
            module_id: encode_id(module_id.as_bytes()),
            edge_sequence: edge_sequence.to_string(),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
        ProjectMapIndexEvidenceSelection::UnresolvedRelation {
            module_id,
            candidate_sequence,
            evidence_id,
        } => ProjectMapIndexEvidenceSelectionV1::UnresolvedRelation {
            module_id: encode_id(module_id.as_bytes()),
            candidate_sequence: candidate_sequence.to_string(),
            evidence_id: encode_id(evidence_id.as_bytes()),
        },
    }
}

fn map_node(node: &ProjectMapAtlasNode) -> ProjectMapAtlasNodeV1 {
    ProjectMapAtlasNodeV1::new(
        encode_id(node.id().as_bytes()),
        node.parent_id().map(|id| encode_id(id.as_bytes())),
        node.selection().map(map_selection_to_v1),
        map_node_kind(node.kind()),
        node.display_name().to_owned(),
        node.detail().map(str::to_owned),
        node.rank(),
        node.volume().to_string(),
        node.file_count().to_string(),
        node.symbol_count().to_string(),
        node.member_count().to_string(),
        node.mapping_status().map(map_mapping_status),
        node.purpose().map(str::to_owned),
        node.current_risk_count().to_string(),
        node.evidence_id().map(|id| encode_id(id.as_bytes())),
        node.claim_badge_count(),
        node.dimmed(),
    )
}

fn map_relation(relation: &ProjectMapAtlasRelation) -> ProjectMapAtlasRelationV1 {
    ProjectMapAtlasRelationV1::new(
        encode_id(relation.source_node_id().as_bytes()),
        encode_id(relation.target_node_id().as_bytes()),
        map_relation_kind(relation.relation()),
        relation.evidence_count().to_string(),
        relation.confidence_basis_points(),
        map_provider(relation.provider()),
        relation.evidence().map(map_evidence_to_v1),
        relation.claim_badge_count(),
        relation.uncertainty().map(map_uncertainty),
    )
}

fn map_breadcrumb(value: &ProjectMapAtlasBreadcrumb) -> ProjectMapAtlasBreadcrumbV1 {
    ProjectMapAtlasBreadcrumbV1::new(
        value.label().to_owned(),
        value.selection().map(map_selection_to_v1),
    )
}

fn map_relation_count(value: &ProjectMapRelationCount) -> ProjectMapRelationCountV1 {
    ProjectMapRelationCountV1::new(
        map_relation_kind(value.relation()),
        value.incoming().to_string(),
        value.outgoing().to_string(),
    )
}

fn map_claim(value: &ProjectMapClaimReference) -> ProjectMapClaimReferenceV1 {
    ProjectMapClaimReferenceV1::new(
        encode_id(value.card_id().as_bytes()),
        encode_id(value.claim_id().as_bytes()),
        value.confidence_basis_points(),
    )
}

fn map_flow_target(value: &ProjectMapFlowTarget) -> ProjectMapFlowTargetV1 {
    ProjectMapFlowTargetV1::new(
        encode_id(value.node_id().as_bytes()),
        value.depth(),
        value.path().iter().map(map_flow_step).collect(),
    )
}

fn map_flow_step(value: &ProjectMapFlowStep) -> ProjectMapFlowStepV1 {
    ProjectMapFlowStepV1::new(
        encode_id(value.source_node_id().as_bytes()),
        encode_id(value.target_node_id().as_bytes()),
        map_relation_kind(value.relation()),
        map_evidence_to_v1(value.evidence()),
    )
}

const fn map_level(value: ProjectMapAtlasLevel) -> ProjectMapAtlasLevelV1 {
    match value {
        ProjectMapAtlasLevel::Project => ProjectMapAtlasLevelV1::Project,
        ProjectMapAtlasLevel::Module => ProjectMapAtlasLevelV1::Module,
        ProjectMapAtlasLevel::File => ProjectMapAtlasLevelV1::File,
        ProjectMapAtlasLevel::Symbol => ProjectMapAtlasLevelV1::Symbol,
    }
}
const fn map_node_kind(value: ProjectMapAtlasNodeKind) -> ProjectMapAtlasNodeKindV1 {
    match value {
        ProjectMapAtlasNodeKind::ManifestModule => ProjectMapAtlasNodeKindV1::ManifestModule,
        ProjectMapAtlasNodeKind::PathModule => ProjectMapAtlasNodeKindV1::PathModule,
        ProjectMapAtlasNodeKind::File => ProjectMapAtlasNodeKindV1::File,
        ProjectMapAtlasNodeKind::Namespace => ProjectMapAtlasNodeKindV1::Namespace,
        ProjectMapAtlasNodeKind::Type => ProjectMapAtlasNodeKindV1::Type,
        ProjectMapAtlasNodeKind::Callable => ProjectMapAtlasNodeKindV1::Callable,
        ProjectMapAtlasNodeKind::Member => ProjectMapAtlasNodeKindV1::Member,
        ProjectMapAtlasNodeKind::Boundary => ProjectMapAtlasNodeKindV1::Boundary,
    }
}
const fn map_inventory_view(value: ProjectMapInventoryView) -> ProjectMapInventoryViewV1 {
    match value {
        ProjectMapInventoryView::Files => ProjectMapInventoryViewV1::Files,
        ProjectMapInventoryView::Symbols => ProjectMapInventoryViewV1::Symbols,
        ProjectMapInventoryView::Members => ProjectMapInventoryViewV1::Members,
    }
}
const fn map_inventory_view_from_v1(value: ProjectMapInventoryViewV1) -> ProjectMapInventoryView {
    match value {
        ProjectMapInventoryViewV1::Files => ProjectMapInventoryView::Files,
        ProjectMapInventoryViewV1::Symbols => ProjectMapInventoryView::Symbols,
        ProjectMapInventoryViewV1::Members => ProjectMapInventoryView::Members,
    }
}
const fn map_flow_preset(value: ProjectMapFlowPreset) -> ProjectMapFlowPresetV1 {
    match value {
        ProjectMapFlowPreset::Callers => ProjectMapFlowPresetV1::Callers,
        ProjectMapFlowPreset::Callees => ProjectMapFlowPresetV1::Callees,
        ProjectMapFlowPreset::Tests => ProjectMapFlowPresetV1::Tests,
        ProjectMapFlowPreset::DataAccess => ProjectMapFlowPresetV1::DataAccess,
    }
}
const fn map_flow_preset_from_v1(value: ProjectMapFlowPresetV1) -> ProjectMapFlowPreset {
    match value {
        ProjectMapFlowPresetV1::Callers => ProjectMapFlowPreset::Callers,
        ProjectMapFlowPresetV1::Callees => ProjectMapFlowPreset::Callees,
        ProjectMapFlowPresetV1::Tests => ProjectMapFlowPreset::Tests,
        ProjectMapFlowPresetV1::DataAccess => ProjectMapFlowPreset::DataAccess,
    }
}
const fn map_mapping_status(value: ProjectMapMappingStatus) -> ProjectMapMappingStatusV1 {
    match value {
        ProjectMapMappingStatus::Current => ProjectMapMappingStatusV1::Current,
        ProjectMapMappingStatus::Stale => ProjectMapMappingStatusV1::Stale,
        ProjectMapMappingStatus::NeedsReview => ProjectMapMappingStatusV1::NeedsReview,
        ProjectMapMappingStatus::Unmapped => ProjectMapMappingStatusV1::Unmapped,
    }
}
const fn map_provider(value: SyntaxProvider) -> ProjectMapRelationProviderV1 {
    match value {
        SyntaxProvider::TreeSitter => ProjectMapRelationProviderV1::TreeSitter,
        SyntaxProvider::Manifest => ProjectMapRelationProviderV1::Manifest,
        SyntaxProvider::LanguageHeuristic => ProjectMapRelationProviderV1::LanguageHeuristic,
    }
}
const fn map_uncertainty(value: ProjectMapAtlasUncertainty) -> ProjectMapAtlasUncertaintyV1 {
    match value {
        ProjectMapAtlasUncertainty::External => ProjectMapAtlasUncertaintyV1::External,
        ProjectMapAtlasUncertainty::NoDeterministicMatch => {
            ProjectMapAtlasUncertaintyV1::NoDeterministicMatch
        }
        ProjectMapAtlasUncertainty::AmbiguousMatch => ProjectMapAtlasUncertaintyV1::AmbiguousMatch,
        ProjectMapAtlasUncertainty::DynamicReference => {
            ProjectMapAtlasUncertaintyV1::DynamicReference
        }
        ProjectMapAtlasUncertainty::MissingFile => ProjectMapAtlasUncertaintyV1::MissingFile,
    }
}
const fn map_relation_kind(value: SyntaxRelationKind) -> ProjectMapRelationKindV1 {
    match value {
        SyntaxRelationKind::Contains => ProjectMapRelationKindV1::Contains,
        SyntaxRelationKind::Defines => ProjectMapRelationKindV1::Defines,
        SyntaxRelationKind::Imports => ProjectMapRelationKindV1::Imports,
        SyntaxRelationKind::Exports => ProjectMapRelationKindV1::Exports,
        SyntaxRelationKind::Calls => ProjectMapRelationKindV1::Calls,
        SyntaxRelationKind::Implements => ProjectMapRelationKindV1::Implements,
        SyntaxRelationKind::Extends => ProjectMapRelationKindV1::Extends,
        SyntaxRelationKind::Reads => ProjectMapRelationKindV1::Reads,
        SyntaxRelationKind::Writes => ProjectMapRelationKindV1::Writes,
        SyntaxRelationKind::Configures => ProjectMapRelationKindV1::Configures,
        SyntaxRelationKind::Tests => ProjectMapRelationKindV1::Tests,
        SyntaxRelationKind::Builds => ProjectMapRelationKindV1::Builds,
        SyntaxRelationKind::Documents => ProjectMapRelationKindV1::Documents,
    }
}

fn decode_id(value: &str) -> Result<[u8; 32], ()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(());
    }
    let mut bytes = [0_u8; 32];
    for (index, target) in bytes.iter_mut().enumerate() {
        *target = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| ())?;
    }
    Ok(bytes)
}

fn decode_sequence(value: &str) -> Result<u64, ()> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.starts_with('0')
    {
        return Err(());
    }
    value.parse::<u64>().map_err(|_| ())
}

fn encode_id(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

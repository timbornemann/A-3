use crate::catalog::is_corruption;
use crate::module_dependency_graph_repository;
use a3_application::{
    KnowledgeStoreFailure, ModuleDependencyGraphControl, ModuleDependencyGraphControlError,
    ModuleDependencyGraphLoadResult, ModuleDependencyGraphQuery, ModuleDependencyNodeLimit,
    ModuleDependencyRelation, PROJECT_MAP_SCENE_FOCUS_MODULE_LIMIT,
    PROJECT_MAP_SCENE_OVERVIEW_MODULE_LIMIT, PROJECT_MAP_SCENE_RELATION_LIMIT,
    ProjectMapCardBinding, ProjectMapMappingStatus, ProjectMapScene, ProjectMapSceneControl,
    ProjectMapSceneFailure, ProjectMapSceneLoadResult, ProjectMapSceneModule, ProjectMapSceneQuery,
    ProjectMapSceneRelation, ScenePolicyVersion,
};
use a3_domain::{
    ContentHash, FileRevision, IndexRunId, ModuleCardEvidenceId, ModuleCardId, ModuleId,
    ModuleKind, Progress, RepositoryPath, SnapshotId, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, Value, params, params_from_iter};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_SCENE_READ_DURATION: Duration = Duration::from_secs(2);
const MAX_INSPECTED_EDGES: usize = 4_096;
const MODULE_CARD_FIELD_COUNT: u64 = 12;

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ProjectMapSceneQuery,
    control: &dyn ProjectMapSceneControl,
) -> Result<ProjectMapSceneLoadResult, ProjectMapSceneRepositoryError> {
    let guard = SceneReadGuard::new(control)?;
    let focused_graph = match load_focus_graph(connection, worktree_id, query, control).await? {
        FocusGraph::None => None,
        FocusGraph::Unavailable => return Ok(ProjectMapSceneLoadResult::FocusUnavailable),
        FocusGraph::Graph(graph) => Some(graph),
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ProjectMapSceneRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id).await? else {
            return Ok(ProjectMapSceneLoadResult::NoPublishedIndex);
        };
        let Some(expected_module_count) = publication.expected_module_count else {
            return Ok(ProjectMapSceneLoadResult::ProjectionUnavailable);
        };
        let primary_count = primary_module_count(
            &transaction,
            publication.index_run_id,
            expected_module_count,
        )
        .await?;
        let selected = if let Some(graph) = &focused_graph {
            if graph.index_run_id() != publication.index_run_id
                || graph.snapshot_id() != publication.snapshot_id
            {
                return Err(ProjectMapSceneRepositoryError::SelectionChanged);
            }
            let mut ids = vec![graph.center_module_id()];
            ids.extend(
                graph
                    .nodes()
                    .iter()
                    .map(|node| node.module_id())
                    .filter(|id| *id != graph.center_module_id()),
            );
            ids
        } else {
            select_overview_modules(&transaction, publication.index_run_id, &guard).await?
        };
        let modules = load_modules(
            &transaction,
            worktree_id,
            publication.index_run_id,
            &selected,
            &guard,
        )
        .await?;
        let relation_projection = if let Some(graph) = focused_graph {
            let routes = graph
                .edges()
                .iter()
                .take(PROJECT_MAP_SCENE_RELATION_LIMIT)
                .map(|edge| {
                    ProjectMapSceneRelation::new(
                        edge.source_module_id(),
                        edge.target_module_id(),
                        edge.relation(),
                        edge.observed_evidence_count(),
                        Some(edge.evidence_id()),
                    )
                    .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
                })
                .collect::<Result<Vec<_>, _>>()?;
            RelationProjection {
                routes,
                observed_group_count: graph.observed_edge_group_count(),
                inspected_edge_count: graph.inspected_edge_count(),
                unmapped_edge_count: graph.unmapped_edge_count(),
                source_edges_truncated: graph.source_edges_truncated(),
            }
        } else {
            load_overview_relations(&transaction, publication.index_run_id, &selected, &guard)
                .await?
        };
        guard.checkpoint()?;
        ProjectMapScene::new(
            publication.index_run_id,
            publication.snapshot_id,
            ScenePolicyVersion::V1,
            query.focus_module_id(),
            primary_count,
            modules,
            relation_projection.observed_group_count,
            relation_projection.routes,
            relation_projection.inspected_edge_count,
            relation_projection.unmapped_edge_count,
            relation_projection.source_edges_truncated,
        )
        .map(ProjectMapSceneLoadResult::Scene)
        .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(scene) => {
            transaction
                .commit()
                .await
                .map_err(ProjectMapSceneRepositoryError::Commit)?;
            Ok(scene)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn load_focus_graph(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ProjectMapSceneQuery,
    control: &dyn ProjectMapSceneControl,
) -> Result<FocusGraph, ProjectMapSceneRepositoryError> {
    let Some(focus) = query.focus_module_id() else {
        return Ok(FocusGraph::None);
    };
    let limit = u16::try_from(PROJECT_MAP_SCENE_FOCUS_MODULE_LIMIT)
        .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)?;
    let graph_query = ModuleDependencyGraphQuery::new(
        focus,
        ModuleDependencyNodeLimit::new(limit)
            .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)?,
    );
    match module_dependency_graph_repository::load(
        connection,
        worktree_id,
        &graph_query,
        &SceneDependencyControl(control),
    )
    .await
    .map_err(map_dependency_error)?
    {
        ModuleDependencyGraphLoadResult::NoPublishedIndex => Ok(FocusGraph::None),
        ModuleDependencyGraphLoadResult::ProjectionUnavailable => Ok(FocusGraph::None),
        ModuleDependencyGraphLoadResult::CenterUnavailable => Ok(FocusGraph::Unavailable),
        ModuleDependencyGraphLoadResult::Graph(graph) => Ok(FocusGraph::Graph(graph)),
    }
}

enum FocusGraph {
    None,
    Unavailable,
    Graph(a3_application::ModuleDependencyGraph),
}

struct Publication {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    expected_module_count: Option<u64>,
}

async fn latest_publication(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<Option<Publication>, ProjectMapSceneRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT run.index_run_id, run.snapshot_id, projection.module_count\n\
             FROM index_runs run LEFT JOIN module_projections projection\n\
               ON projection.index_run_id = run.index_run_id\n\
             WHERE run.worktree_id = ?1 AND run.status = 'published'\n\
             ORDER BY run.run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
    else {
        return Ok(None);
    };
    Ok(Some(Publication {
        index_run_id: IndexRunId::from_bytes(read_id(&row, 0)?),
        snapshot_id: SnapshotId::from_bytes(read_id(&row, 1)?),
        expected_module_count: read_optional_count(&row, 2)?,
    }))
}

async fn primary_module_count(
    transaction: &Transaction,
    run_id: IndexRunId,
    expected: u64,
) -> Result<u64, ProjectMapSceneRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*),\n\
               COALESCE(SUM(CASE WHEN kind IN ('manifest', 'path') THEN 1 ELSE 0 END), 0)\n\
             FROM modules WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .ok_or(ProjectMapSceneRepositoryError::InvalidStoredProjection)?;
    if read_count(&row, 0)? != expected {
        return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
    }
    read_count(&row, 1)
}

async fn select_overview_modules(
    transaction: &Transaction,
    run_id: IndexRunId,
    guard: &SceneReadGuard<'_>,
) -> Result<Vec<ModuleId>, ProjectMapSceneRepositoryError> {
    let limit = i64::try_from(PROJECT_MAP_SCENE_OVERVIEW_MODULE_LIMIT)
        .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)?;
    let mut rows = transaction
        .query(
            "SELECT module.module_id FROM modules module\n\
             WHERE module.index_run_id = ?1 AND module.kind IN ('manifest', 'path')\n\
             ORDER BY CASE WHEN module.kind = 'manifest' THEN 0 ELSE 1 END,\n\
               CASE WHEN EXISTS (SELECT 1 FROM module_entrypoints feature\n\
                 WHERE feature.index_run_id = module.index_run_id\n\
                   AND feature.module_id = module.module_id) THEN 0 ELSE 1 END,\n\
               CASE WHEN EXISTS (SELECT 1 FROM module_tests feature\n\
                 WHERE feature.index_run_id = module.index_run_id\n\
                   AND feature.module_id = module.module_id) THEN 0 ELSE 1 END,\n\
               CASE WHEN EXISTS (SELECT 1 FROM module_central_symbols feature\n\
                 WHERE feature.index_run_id = module.index_run_id\n\
                   AND feature.module_id = module.module_id) THEN 0 ELSE 1 END,\n\
               module.module_id LIMIT ?2",
            params![run_id.as_bytes().to_vec(), limit],
        )
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let mut selected = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
    {
        guard.checkpoint()?;
        selected.push(ModuleId::from_bytes(read_id(&row, 0)?));
    }
    Ok(selected)
}

#[derive(Clone, Debug)]
struct RawModule {
    id: ModuleId,
    kind: ModuleKind,
    root_kind: String,
    root_path: Option<RepositoryPath>,
    manifests: u64,
    files: u64,
    symbols: u64,
    central_symbols: u64,
    entrypoints: u64,
    tests: u64,
    representative: Option<FileRevision>,
    status: ProjectMapMappingStatus,
    coverage: Option<u16>,
    card: Option<ProjectMapCardBinding>,
}

async fn load_modules(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    run_id: IndexRunId,
    selected: &[ModuleId],
    guard: &SceneReadGuard<'_>,
) -> Result<Vec<ProjectMapSceneModule>, ProjectMapSceneRepositoryError> {
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let module_parameters = (4..selected.len() + 4)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH ranked_cards AS (\n\
           SELECT card.module_id, card.source_index_run_id, card.snapshot_id, card.card_id,\n\
             lifecycle.status,\n\
             (SELECT COUNT(*) FROM module_card_fields field\n\
               WHERE field.source_index_run_id = card.source_index_run_id\n\
                 AND field.card_id = card.card_id) field_count,\n\
             ROW_NUMBER() OVER (PARTITION BY card.module_id ORDER BY snapshot.generation DESC,\n\
               CASE WHEN card.source_index_run_id = ?2 THEN 1 ELSE 0 END DESC,\n\
               source_run.run_sequence DESC, card.card_id DESC) card_rank\n\
           FROM module_cards card\n\
           JOIN module_card_lifecycle lifecycle\n\
             ON lifecycle.source_index_run_id = card.source_index_run_id\n\
            AND lifecycle.card_id = card.card_id\n\
           JOIN snapshots snapshot ON snapshot.snapshot_id = card.snapshot_id\n\
           JOIN index_runs source_run ON source_run.index_run_id = card.source_index_run_id\n\
             AND source_run.snapshot_id = card.snapshot_id\n\
             AND source_run.worktree_id = ?1 AND source_run.status = 'published'\n\
           WHERE snapshot.worktree_id = ?1 AND card.status = 'published'\n\
         )\n\
         SELECT module.module_id, module.kind, module.root_kind, module.root_path,\n\
           (SELECT COUNT(*) FROM module_manifests feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id),\n\
           (SELECT COUNT(DISTINCT member_path) FROM module_members feature\n\
             WHERE feature.index_run_id = ?3 AND feature.module_id = module.module_id),\n\
           (SELECT COUNT(*) FROM module_members feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id),\n\
           (SELECT COUNT(*) FROM module_central_symbols feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id),\n\
           (SELECT COUNT(*) FROM module_entrypoints feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id),\n\
           (SELECT COUNT(*) FROM module_tests feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id),\n\
           (SELECT member_path FROM module_members feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id ORDER BY symbol_id LIMIT 1),\n\
           (SELECT member_hash FROM module_members feature WHERE feature.index_run_id = ?3\n\
             AND feature.module_id = module.module_id ORDER BY symbol_id LIMIT 1),\n\
           card.source_index_run_id, card.snapshot_id, card.card_id, card.status, card.field_count\n\
         FROM modules module LEFT JOIN ranked_cards card\n\
           ON card.module_id = module.module_id AND card.card_rank = 1\n\
         WHERE module.index_run_id = ?3 AND module.kind IN ('manifest', 'path')\n\
           AND module.module_id IN ({module_parameters}) ORDER BY module.module_id"
    );
    let mut values = vec![
        Value::Blob(worktree_id.as_bytes().to_vec()),
        Value::Blob(run_id.as_bytes().to_vec()),
        Value::Blob(run_id.as_bytes().to_vec()),
    ];
    values.extend(
        selected
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec())),
    );
    let mut rows = transaction
        .query(&sql, params_from_iter(values))
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let mut by_id = BTreeMap::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
    {
        guard.checkpoint()?;
        let raw = raw_module(&row)?;
        if by_id.insert(raw.id, raw).is_some() {
            return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
        }
    }
    if by_id.len() != selected.len() {
        return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
    }
    let ordered_raw = selected
        .iter()
        .map(|id| {
            by_id
                .get(id)
                .cloned()
                .ok_or(ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ordered_raw
        .iter()
        .enumerate()
        .map(|(index, raw)| {
            let parent = nearest_parent(raw, &ordered_raw);
            let rank = u16::try_from(index + 1)
                .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)?;
            ProjectMapSceneModule::new(
                raw.id,
                parent,
                raw.kind,
                display_name(raw),
                rank,
                raw.manifests,
                raw.files,
                raw.symbols,
                raw.central_symbols,
                raw.entrypoints,
                raw.tests,
                raw.status,
                raw.coverage,
                raw.card,
                raw.representative
                    .as_ref()
                    .map(ModuleCardEvidenceId::for_file_revision_v1),
            )
            .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .collect()
}

fn raw_module(row: &libsql::Row) -> Result<RawModule, ProjectMapSceneRepositoryError> {
    let id = ModuleId::from_bytes(read_id(row, 0)?);
    let kind = read_primary_kind(row, 1)?;
    let root_kind = read_text(row, 2)?;
    let root_path = read_optional_path(row, 3)?;
    validate_root(&root_kind, root_path.as_ref())?;
    let symbols = read_count(row, 6)?;
    let representative = read_optional_revision(row, 10, 11)?;
    if representative.is_none() != (symbols == 0) {
        return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
    }
    let source_run = read_optional_id(row, 12)?.map(IndexRunId::from_bytes);
    let source_snapshot = read_optional_id(row, 13)?.map(SnapshotId::from_bytes);
    let card_id = read_optional_id(row, 14)?.map(ModuleCardId::from_bytes);
    let status = read_optional_text(row, 15)?
        .map(|value| parse_status(&value))
        .transpose()?
        .unwrap_or(ProjectMapMappingStatus::Unmapped);
    let field_count = read_optional_count(row, 16)?;
    let card = match (source_run, source_snapshot, card_id) {
        (Some(run), Some(snapshot), Some(card)) => {
            Some(ProjectMapCardBinding::new(card, run, snapshot))
        }
        (None, None, None) => None,
        _ => return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    };
    if (status == ProjectMapMappingStatus::Unmapped) != card.is_none()
        || field_count.is_some() != card.is_some()
    {
        return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
    }
    let coverage = field_count
        .map(|count| {
            if count > MODULE_CARD_FIELD_COUNT {
                return Err(ProjectMapSceneRepositoryError::InvalidStoredProjection);
            }
            u16::try_from(count.saturating_mul(10_000) / MODULE_CARD_FIELD_COUNT)
                .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .transpose()?;
    Ok(RawModule {
        id,
        kind,
        root_kind,
        root_path,
        manifests: read_count(row, 4)?,
        files: read_count(row, 5)?,
        symbols,
        central_symbols: read_count(row, 7)?,
        entrypoints: read_count(row, 8)?,
        tests: read_count(row, 9)?,
        representative,
        status,
        coverage,
        card,
    })
}

fn nearest_parent(module: &RawModule, modules: &[RawModule]) -> Option<ModuleId> {
    modules
        .iter()
        .filter(|candidate| candidate.id != module.id && is_ancestor(candidate, module))
        .map(|candidate| {
            (
                candidate
                    .root_path
                    .as_ref()
                    .map_or(0, |path| path.as_bytes().len()),
                candidate.id,
            )
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)))
        .map(|(_, id)| id)
}

fn is_ancestor(parent: &RawModule, child: &RawModule) -> bool {
    match (
        parent.root_kind.as_str(),
        parent.root_path.as_ref(),
        child.root_path.as_ref(),
    ) {
        ("repository", None, Some(_)) => true,
        ("directory", Some(parent), Some(child)) => {
            child.as_bytes().len() > parent.as_bytes().len()
                && child.as_bytes().starts_with(parent.as_bytes())
                && child.as_bytes().get(parent.as_bytes().len()) == Some(&b'/')
        }
        _ => false,
    }
}

struct RelationProjection {
    routes: Vec<ProjectMapSceneRelation>,
    observed_group_count: u64,
    inspected_edge_count: u64,
    unmapped_edge_count: u64,
    source_edges_truncated: bool,
}

async fn load_overview_relations(
    transaction: &Transaction,
    run_id: IndexRunId,
    selected: &[ModuleId],
    guard: &SceneReadGuard<'_>,
) -> Result<RelationProjection, ProjectMapSceneRepositoryError> {
    if selected.is_empty() {
        return Ok(RelationProjection {
            routes: Vec::new(),
            observed_group_count: 0,
            inspected_edge_count: 0,
            unmapped_edge_count: 0,
            source_edges_truncated: false,
        });
    }
    let stats_sql = format!(
        "WITH {} SELECT (SELECT COUNT(*) FROM inspected),\n\
         (SELECT COUNT(*) FROM mapped_edges WHERE source_module_id IS NULL\n\
           OR target_module_id IS NULL),\n\
         CASE WHEN (SELECT COUNT(*) FROM edge_prefix) > {MAX_INSPECTED_EDGES}\n\
           THEN 1 ELSE 0 END",
        edge_ctes("?1")
    );
    let mut rows = transaction
        .query(&stats_sql, [run_id.as_bytes().to_vec()])
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .ok_or(ProjectMapSceneRepositoryError::InvalidStoredProjection)?;
    let inspected_edge_count = read_count(&row, 0)?;
    let unmapped_edge_count = read_count(&row, 1)?;
    let source_edges_truncated = read_bool(&row, 2)?;
    guard.checkpoint()?;

    let selected_values = (1..=selected.len())
        .map(|index| format!("(?{index})"))
        .collect::<Vec<_>>()
        .join(", ");
    let run_parameter = format!("?{}", selected.len() + 1);
    let relation_sql = format!(
        "WITH selected(module_id) AS (VALUES {selected_values}), {},\n\
         visible_groups AS (\n\
           SELECT source_module_id, target_module_id, relation_kind, COUNT(*) evidence_count\n\
           FROM mapped_edges WHERE source_module_id IN (SELECT module_id FROM selected)\n\
             AND target_module_id IN (SELECT module_id FROM selected)\n\
             AND source_module_id <> target_module_id\n\
           GROUP BY source_module_id, target_module_id, relation_kind\n\
         )\n\
         SELECT source_module_id, target_module_id, relation_kind, evidence_count,\n\
           COUNT(*) OVER () FROM visible_groups\n\
         ORDER BY evidence_count DESC, source_module_id, target_module_id, relation_kind\n\
         LIMIT {}",
        edge_ctes(&run_parameter),
        PROJECT_MAP_SCENE_RELATION_LIMIT
    );
    let mut values = selected
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    values.push(Value::Blob(run_id.as_bytes().to_vec()));
    let mut rows = transaction
        .query(&relation_sql, params_from_iter(values))
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?;
    let mut routes = Vec::new();
    let mut observed_group_count = 0;
    while let Some(row) = rows
        .next()
        .await
        .map_err(ProjectMapSceneRepositoryError::Read)?
    {
        guard.checkpoint()?;
        observed_group_count = read_count(&row, 4)?;
        routes.push(
            ProjectMapSceneRelation::new(
                ModuleId::from_bytes(read_id(&row, 0)?),
                ModuleId::from_bytes(read_id(&row, 1)?),
                parse_relation(&read_text(&row, 2)?)?,
                read_count(&row, 3)?,
                None,
            )
            .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)?,
        );
    }
    Ok(RelationProjection {
        routes,
        observed_group_count,
        inspected_edge_count,
        unmapped_edge_count,
        source_edges_truncated,
    })
}

fn edge_ctes(run: &str) -> String {
    format!(
        "symbol_map AS (SELECT symbol_id, MIN(module_id) module_id FROM module_members\n\
           WHERE index_run_id = {run} AND membership_kind IN ('manifest', 'path')\n\
           GROUP BY symbol_id HAVING COUNT(DISTINCT module_id) = 1),\n\
         file_map AS (SELECT member_path, MIN(module_id) module_id FROM module_members\n\
           WHERE index_run_id = {run} AND membership_kind IN ('manifest', 'path')\n\
           GROUP BY member_path HAVING COUNT(DISTINCT module_id) = 1),\n\
         edge_prefix AS (SELECT edge_sequence, source_kind, source_value, target_kind,\n\
           target_value, relation_kind FROM symbol_edges WHERE index_run_id = {run}\n\
           AND relation_kind NOT IN ('contains', 'defines') ORDER BY edge_sequence\n\
           LIMIT {}),\n\
         inspected AS (SELECT * FROM edge_prefix LIMIT {MAX_INSPECTED_EDGES}),\n\
         mapped_edges AS (SELECT relation_kind,\n\
           CASE source_kind WHEN 'symbol' THEN (SELECT module_id FROM symbol_map\n\
             WHERE symbol_id = source_value) ELSE (SELECT module_id FROM file_map\n\
             WHERE member_path = source_value) END source_module_id,\n\
           CASE target_kind WHEN 'symbol' THEN (SELECT module_id FROM symbol_map\n\
             WHERE symbol_id = target_value) ELSE (SELECT module_id FROM file_map\n\
             WHERE member_path = target_value) END target_module_id FROM inspected)",
        MAX_INSPECTED_EDGES + 1
    )
}

fn display_name(module: &RawModule) -> String {
    let bytes = match (module.root_kind.as_str(), module.root_path.as_ref()) {
        ("repository", None) => return "Repository".to_owned(),
        ("directory", Some(path)) => path
            .as_bytes()
            .rsplit(|byte| *byte == b'/')
            .next()
            .unwrap_or(path.as_bytes()),
        _ => return "Module".to_owned(),
    };
    String::from_utf8_lossy(bytes)
        .chars()
        .take(256)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn validate_root(
    kind: &str,
    path: Option<&RepositoryPath>,
) -> Result<(), ProjectMapSceneRepositoryError> {
    match (kind, path) {
        ("repository", None) | ("directory", Some(_)) => Ok(()),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

fn parse_status(value: &str) -> Result<ProjectMapMappingStatus, ProjectMapSceneRepositoryError> {
    match value {
        "published" => Ok(ProjectMapMappingStatus::Current),
        "stale" => Ok(ProjectMapMappingStatus::Stale),
        "needs-review" => Ok(ProjectMapMappingStatus::NeedsReview),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

fn parse_relation(value: &str) -> Result<ModuleDependencyRelation, ProjectMapSceneRepositoryError> {
    match value {
        "imports" => Ok(ModuleDependencyRelation::Imports),
        "exports" => Ok(ModuleDependencyRelation::Exports),
        "calls" => Ok(ModuleDependencyRelation::Calls),
        "implements" => Ok(ModuleDependencyRelation::Implements),
        "extends" => Ok(ModuleDependencyRelation::Extends),
        "reads" => Ok(ModuleDependencyRelation::Reads),
        "writes" => Ok(ModuleDependencyRelation::Writes),
        "configures" => Ok(ModuleDependencyRelation::Configures),
        "tests" => Ok(ModuleDependencyRelation::Tests),
        "builds" => Ok(ModuleDependencyRelation::Builds),
        "documents" => Ok(ModuleDependencyRelation::Documents),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

fn read_primary_kind(
    row: &libsql::Row,
    index: i32,
) -> Result<ModuleKind, ProjectMapSceneRepositoryError> {
    match read_text(row, index)?.as_str() {
        "manifest" => Ok(ModuleKind::ManifestBoundary),
        "path" => Ok(ModuleKind::PathBoundary),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ProjectMapSceneRepositoryError> {
    row.get::<Vec<u8>>(index)
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .try_into()
        .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, ProjectMapSceneRepositoryError> {
    row.get::<Option<Vec<u8>>>(index)
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_count(row: &libsql::Row, index: i32) -> Result<u64, ProjectMapSceneRepositoryError> {
    u64::try_from(
        row.get::<i64>(index)
            .map_err(ProjectMapSceneRepositoryError::Read)?,
    )
    .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
}

fn read_optional_count(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u64>, ProjectMapSceneRepositoryError> {
    row.get::<Option<i64>>(index)
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, ProjectMapSceneRepositoryError> {
    row.get(index).map_err(ProjectMapSceneRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, ProjectMapSceneRepositoryError> {
    row.get(index).map_err(ProjectMapSceneRepositoryError::Read)
}

fn read_optional_path(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<RepositoryPath>, ProjectMapSceneRepositoryError> {
    row.get::<Option<Vec<u8>>>(index)
        .map_err(ProjectMapSceneRepositoryError::Read)?
        .map(|bytes| {
            RepositoryPath::try_from_bytes(bytes)
                .map_err(|_| ProjectMapSceneRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_optional_revision(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
) -> Result<Option<FileRevision>, ProjectMapSceneRepositoryError> {
    match (
        read_optional_path(row, path_index)?,
        read_optional_id(row, hash_index)?.map(ContentHash::from_bytes),
    ) {
        (Some(path), Some(hash)) => Ok(Some(FileRevision::new(path, hash))),
        (None, None) => Ok(None),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, ProjectMapSceneRepositoryError> {
    match row
        .get::<i64>(index)
        .map_err(ProjectMapSceneRepositoryError::Read)?
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProjectMapSceneRepositoryError::InvalidStoredProjection),
    }
}

struct SceneReadGuard<'a> {
    control: &'a dyn ProjectMapSceneControl,
    started_at: Instant,
}

impl<'a> SceneReadGuard<'a> {
    fn new(
        control: &'a dyn ProjectMapSceneControl,
    ) -> Result<Self, ProjectMapSceneRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ProjectMapSceneRepositoryError> {
        if self.control.is_cancelled() {
            Err(ProjectMapSceneRepositoryError::Cancelled)
        } else if self.started_at.elapsed() >= MAX_SCENE_READ_DURATION {
            Err(ProjectMapSceneRepositoryError::TimedOut)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct SceneDependencyControl<'a>(&'a dyn ProjectMapSceneControl);

impl ModuleDependencyGraphControl for SceneDependencyControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn report_progress(
        &self,
        _progress: Progress,
    ) -> Result<(), ModuleDependencyGraphControlError> {
        Ok(())
    }
}

fn map_dependency_error(
    error: module_dependency_graph_repository::ModuleDependencyGraphRepositoryError,
) -> ProjectMapSceneRepositoryError {
    match error.classify() {
        a3_application::ModuleDependencyGraphFailure::Storage(error) => {
            ProjectMapSceneRepositoryError::Storage(error)
        }
        a3_application::ModuleDependencyGraphFailure::InvalidStoredProjection => {
            ProjectMapSceneRepositoryError::InvalidStoredProjection
        }
        a3_application::ModuleDependencyGraphFailure::Cancelled => {
            ProjectMapSceneRepositoryError::Cancelled
        }
        a3_application::ModuleDependencyGraphFailure::TimedOut => {
            ProjectMapSceneRepositoryError::TimedOut
        }
        a3_application::ModuleDependencyGraphFailure::ProgressUnavailable => {
            ProjectMapSceneRepositoryError::InvalidStoredProjection
        }
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ProjectMapSceneRepositoryError,
) -> Result<T, ProjectMapSceneRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ProjectMapSceneRepositoryError::Rollback(source)),
    }
}

#[derive(Debug)]
pub(crate) enum ProjectMapSceneRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Storage(KnowledgeStoreFailure),
    InvalidStoredProjection,
    SelectionChanged,
    Cancelled,
    TimedOut,
}

impl ProjectMapSceneRepositoryError {
    pub(crate) fn classify(&self) -> ProjectMapSceneFailure {
        match self {
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ProjectMapSceneFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ProjectMapSceneFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
            Self::Storage(error) => ProjectMapSceneFailure::Storage(*error),
            Self::InvalidStoredProjection => ProjectMapSceneFailure::InvalidStoredProjection,
            Self::SelectionChanged | Self::TimedOut => ProjectMapSceneFailure::TimedOut,
            Self::Cancelled => ProjectMapSceneFailure::Cancelled,
        }
    }
}

impl fmt::Display for ProjectMapSceneRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "project map scene transaction could not begin",
            Self::Read(_) => "project map scene rows could not be read",
            Self::Commit(_) => "project map scene transaction could not commit",
            Self::Rollback(_) => "project map scene transaction could not roll back",
            Self::Storage(_) => "project map scene dependency storage failed",
            Self::InvalidStoredProjection => "stored project map scene is invalid",
            Self::SelectionChanged => "project map publication changed during the read",
            Self::Cancelled => "project map scene read was cancelled",
            Self::TimedOut => "project map scene read timed out",
        })
    }
}

impl Error for ProjectMapSceneRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::SelectionChanged
            | Self::Cancelled
            | Self::TimedOut => None,
        }
    }
}

use crate::catalog::is_corruption;
use crate::module_card_detail_repository::{
    ModuleCardDetailRepositoryError, SelectedModule, latest_card, latest_publication,
    validate_invalidation_run, validate_selected_module,
};
use a3_application::{
    KnowledgeStoreFailure, ModuleCardEvidenceControl, ModuleCardEvidenceDetail,
    ModuleCardEvidenceFailure, ModuleCardEvidenceFreshness, ModuleCardEvidenceLoadResult,
    ModuleCardEvidencePayload, ModuleCardEvidenceQuery, ModuleCardLifecycle,
};
use a3_domain::{
    Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint, IndexRunId,
    LinkResolution, RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId,
    SyntaxProvider, SyntaxRelationKind, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_MODULE_CARD_EVIDENCE_READ_DURATION: Duration = Duration::from_secs(2);

pub(crate) async fn load(
    connection: &Connection,
    worktree_id: WorktreeId,
    query: &ModuleCardEvidenceQuery,
    control: &dyn ModuleCardEvidenceControl,
) -> Result<ModuleCardEvidenceLoadResult, ModuleCardEvidenceRepositoryError> {
    let guard = ModuleCardEvidenceReadGuard::new(control)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Begin)?;
    let result = async {
        guard.checkpoint()?;
        let Some(publication) = latest_publication(&transaction, worktree_id)
            .await
            .map_err(ModuleCardEvidenceRepositoryError::Selection)?
        else {
            return Ok(ModuleCardEvidenceLoadResult::NoPublishedIndex);
        };
        if publication.index_run_id != query.current_index_run_id()
            || publication.snapshot_id != query.current_snapshot_id()
        {
            return Ok(ModuleCardEvidenceLoadResult::SelectionChanged);
        }
        let Some(expected_module_count) = publication.expected_module_count else {
            return Ok(ModuleCardEvidenceLoadResult::ProjectionUnavailable);
        };
        match validate_selected_module(
            &transaction,
            publication.index_run_id,
            expected_module_count,
            query.module_id(),
        )
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Selection)?
        {
            SelectedModule::Unavailable => {
                return Ok(ModuleCardEvidenceLoadResult::ModuleUnavailable);
            }
            SelectedModule::Primary => {}
        }
        let Some(card) = latest_card(
            &transaction,
            worktree_id,
            publication.index_run_id,
            query.module_id(),
        )
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Selection)?
        else {
            return Ok(ModuleCardEvidenceLoadResult::CardUnavailable);
        };
        if card.source_index_run_id != query.source_index_run_id()
            || card.source_snapshot_id != query.source_snapshot_id()
            || card.id != query.card_id()
        {
            return Ok(ModuleCardEvidenceLoadResult::SelectionChanged);
        }
        validate_invalidation_run(
            &transaction,
            worktree_id,
            publication.index_run_id,
            card.lifecycle,
        )
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Selection)?;

        let Some(payload) = load_linked_evidence(&transaction, query).await? else {
            return Ok(ModuleCardEvidenceLoadResult::EvidenceUnavailable);
        };
        guard.checkpoint()?;
        let resolves_current = evidence_resolves_current(
            &transaction,
            publication.index_run_id,
            publication.snapshot_id,
            &payload,
        )
        .await?;
        let freshness = if resolves_current {
            ModuleCardEvidenceFreshness::Current
        } else if matches!(card.lifecycle, ModuleCardLifecycle::Stale { .. }) {
            ModuleCardEvidenceFreshness::Stale
        } else {
            return Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection);
        };
        ModuleCardEvidenceDetail::new(
            publication.index_run_id,
            publication.snapshot_id,
            card.source_index_run_id,
            card.source_snapshot_id,
            card.id,
            query.module_id(),
            query.evidence_id(),
            card.lifecycle,
            freshness,
            payload,
        )
        .map(Box::new)
        .map(ModuleCardEvidenceLoadResult::Detail)
        .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
    }
    .await;
    match result {
        Ok(detail) => {
            transaction
                .commit()
                .await
                .map_err(ModuleCardEvidenceRepositoryError::Commit)?;
            Ok(detail)
        }
        Err(error) => rollback(transaction, error).await,
    }
}

async fn load_linked_evidence(
    transaction: &Transaction,
    query: &ModuleCardEvidenceQuery,
) -> Result<Option<ModuleCardEvidencePayload>, ModuleCardEvidenceRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT evidence.snapshot_id, evidence.evidence_kind, evidence.repository_path,\n\
               evidence.content_hash, evidence.symbol_id, evidence.source_kind,\n\
               evidence.source_value, evidence.target_kind, evidence.target_value,\n\
               evidence.relation_kind, evidence.provider, evidence.edge_confidence,\n\
               evidence.resolution, evidence.start_byte, evidence.end_byte,\n\
               evidence.start_row, evidence.start_column, evidence.end_row, evidence.end_column,\n\
               symbol.declaration_start_byte, symbol.declaration_end_byte,\n\
               symbol.declaration_start_row, symbol.declaration_start_column,\n\
               symbol.declaration_end_row, symbol.declaration_end_column\n\
             FROM evidence_refs evidence\n\
             LEFT JOIN symbols symbol\n\
               ON evidence.evidence_kind = 'symbol'\n\
              AND symbol.index_run_id = evidence.source_index_run_id\n\
              AND symbol.symbol_id = evidence.symbol_id\n\
              AND symbol.repository_path = evidence.repository_path\n\
              AND symbol.content_hash = evidence.content_hash\n\
             WHERE evidence.source_index_run_id = ?1 AND evidence.evidence_id = ?2\n\
               AND EXISTS (\n\
                 SELECT 1 FROM module_card_field_evidence membership\n\
                 WHERE membership.source_index_run_id = evidence.source_index_run_id\n\
                   AND membership.card_id = ?3\n\
                   AND membership.evidence_id = evidence.evidence_id\n\
               ) LIMIT 1",
            params![
                query.source_index_run_id().as_bytes().to_vec(),
                query.evidence_id().as_bytes().to_vec(),
                query.card_id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Read)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Read)?
    else {
        return Ok(None);
    };
    let snapshot_id = SnapshotId::from_bytes(read_id(&row, 0)?);
    if snapshot_id != query.source_snapshot_id() {
        return Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection);
    }
    let kind = read_text(&row, 1)?;
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(read_blob(&row, 2)?)
            .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
        ContentHash::from_bytes(read_id(&row, 3)?),
    );
    let symbol_id = read_optional_id(&row, 4)?.map(SymbolId::from_bytes);
    let source_kind = read_optional_text(&row, 5)?;
    let source_value = read_optional_blob(&row, 6)?;
    let target_kind = read_optional_text(&row, 7)?;
    let target_value = read_optional_blob(&row, 8)?;
    let relation_kind = read_optional_text(&row, 9)?;
    let provider = read_optional_text(&row, 10)?;
    let confidence = read_optional_u16(&row, 11)?;
    let resolution = read_optional_text(&row, 12)?;
    let coordinates = [13, 14, 15, 16, 17, 18]
        .map(|index| read_optional_u32(&row, index))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let declaration_coordinates = [19, 20, 21, 22, 23, 24]
        .map(|index| read_optional_u32(&row, index))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    match kind.as_str() {
        "file"
            if symbol_id.is_none()
                && source_kind.is_none()
                && source_value.is_none()
                && target_kind.is_none()
                && target_value.is_none()
                && relation_kind.is_none()
                && provider.is_none()
                && confidence.is_none()
                && resolution.is_none()
                && coordinates.iter().all(Option::is_none)
                && declaration_coordinates.iter().all(Option::is_none) =>
        {
            Ok(Some(ModuleCardEvidencePayload::File { revision }))
        }
        "symbol"
            if symbol_id.is_some()
                && source_kind.is_none()
                && source_value.is_none()
                && target_kind.is_none()
                && target_value.is_none()
                && relation_kind.is_none()
                && provider.is_none()
                && confidence.is_none()
                && resolution.is_none()
                && coordinates.iter().all(Option::is_none)
                && declaration_coordinates.iter().all(Option::is_some) =>
        {
            let declaration_values = declaration_coordinates
                .into_iter()
                .map(|value| {
                    value.ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let declaration_range = SourceRange::new(
                usize::try_from(declaration_values[0])
                    .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                usize::try_from(declaration_values[1])
                    .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                SourcePosition::new(declaration_values[2], declaration_values[3]),
                SourcePosition::new(declaration_values[4], declaration_values[5]),
            )
            .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?;
            Ok(Some(ModuleCardEvidencePayload::Symbol {
                symbol_id: symbol_id
                    .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                revision,
                declaration_range,
            }))
        }
        "graph-edge"
            if symbol_id.is_none()
                && source_kind.is_some()
                && source_value.is_some()
                && target_kind.is_some()
                && target_value.is_some()
                && relation_kind.is_some()
                && provider.is_some()
                && confidence.is_some()
                && resolution.is_some()
                && coordinates.iter().all(Option::is_some)
                && declaration_coordinates.iter().all(Option::is_none) =>
        {
            let source = parse_endpoint(
                source_kind
                    .as_deref()
                    .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                source_value.ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
            )?;
            let target = parse_endpoint(
                target_kind
                    .as_deref()
                    .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                target_value.ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
            )?;
            let values = coordinates
                .into_iter()
                .map(|value| {
                    value.ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let range = SourceRange::new(
                usize::try_from(values[0])
                    .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                usize::try_from(values[1])
                    .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                SourcePosition::new(values[2], values[3]),
                SourcePosition::new(values[4], values[5]),
            )
            .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?;
            let edge = GraphEdge::new(
                source,
                target,
                parse_relation(
                    relation_kind
                        .as_deref()
                        .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                )?,
                parse_provider(
                    provider
                        .as_deref()
                        .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                )?,
                Confidence::from_basis_points(
                    confidence.ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                )
                .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                parse_resolution(
                    resolution
                        .as_deref()
                        .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?,
                )?,
                snapshot_id,
                EvidenceRef::new(revision, range),
            );
            Ok(Some(ModuleCardEvidencePayload::GraphEdge { edge }))
        }
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

async fn evidence_resolves_current(
    transaction: &Transaction,
    current_index_run_id: IndexRunId,
    current_snapshot_id: SnapshotId,
    payload: &ModuleCardEvidencePayload,
) -> Result<bool, ModuleCardEvidenceRepositoryError> {
    let count = match payload {
        ModuleCardEvidencePayload::File { revision } => {
            count_rows(
                transaction,
                "SELECT COUNT(*) FROM file_revisions\n\
                 WHERE index_run_id = ?1 AND repository_path = ?2 AND content_hash = ?3",
                params![
                    current_index_run_id.as_bytes().to_vec(),
                    revision.path().as_bytes().to_vec(),
                    revision.content_hash().as_bytes().to_vec(),
                ],
            )
            .await?
        }
        ModuleCardEvidencePayload::Symbol {
            symbol_id,
            revision,
            ..
        } => {
            count_rows(
                transaction,
                "SELECT COUNT(*) FROM symbols\n\
                 WHERE index_run_id = ?1 AND symbol_id = ?2 AND repository_path = ?3\n\
                   AND content_hash = ?4",
                params![
                    current_index_run_id.as_bytes().to_vec(),
                    symbol_id.as_bytes().to_vec(),
                    revision.path().as_bytes().to_vec(),
                    revision.content_hash().as_bytes().to_vec(),
                ],
            )
            .await?
        }
        ModuleCardEvidencePayload::GraphEdge { edge } => {
            if edge.snapshot_id() != current_snapshot_id {
                return Ok(false);
            }
            let (source_kind, source_value) = endpoint_parts(edge.source());
            let (target_kind, target_value) = endpoint_parts(edge.target());
            let range = edge.evidence().range();
            count_rows(
                transaction,
                "SELECT COUNT(*) FROM symbol_edges WHERE index_run_id = ?1\n\
                   AND source_kind = ?2 AND source_value = ?3\n\
                   AND target_kind = ?4 AND target_value = ?5 AND relation_kind = ?6\n\
                   AND provider = ?7 AND confidence = ?8 AND resolution = ?9\n\
                   AND evidence_path = ?10 AND evidence_hash = ?11\n\
                   AND evidence_start_byte = ?12 AND evidence_end_byte = ?13\n\
                   AND evidence_start_row = ?14 AND evidence_start_column = ?15\n\
                   AND evidence_end_row = ?16 AND evidence_end_column = ?17",
                params![
                    current_index_run_id.as_bytes().to_vec(),
                    source_kind,
                    source_value,
                    target_kind,
                    target_value,
                    relation_text(edge.kind()),
                    provider_text(edge.provider()),
                    i64::from(edge.confidence().basis_points()),
                    resolution_text(edge.resolution()),
                    edge.evidence().revision().path().as_bytes().to_vec(),
                    edge.evidence()
                        .revision()
                        .content_hash()
                        .as_bytes()
                        .to_vec(),
                    i64::from(range.start_byte()),
                    i64::from(range.end_byte()),
                    i64::from(range.start_position().row()),
                    i64::from(range.start_position().column()),
                    i64::from(range.end_position().row()),
                    i64::from(range.end_position().column()),
                ],
            )
            .await?
        }
    };
    match count {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

async fn count_rows(
    transaction: &Transaction,
    sql: &str,
    parameters: impl libsql::params::IntoParams,
) -> Result<u64, ModuleCardEvidenceRepositoryError> {
    let mut rows = transaction
        .query(sql, parameters)
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Read)?
        .ok_or(ModuleCardEvidenceRepositoryError::InvalidStoredProjection)?;
    let count = read_u64(&row, 0)?;
    if rows
        .next()
        .await
        .map_err(ModuleCardEvidenceRepositoryError::Read)?
        .is_some()
    {
        return Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection);
    }
    Ok(count)
}

fn parse_endpoint(
    kind: &str,
    value: Vec<u8>,
) -> Result<GraphEndpoint, ModuleCardEvidenceRepositoryError> {
    match kind {
        "file" => RepositoryPath::try_from_bytes(value)
            .map(GraphEndpoint::File)
            .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
        "symbol" => value
            .try_into()
            .map(SymbolId::from_bytes)
            .map(GraphEndpoint::Symbol)
            .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

fn endpoint_parts(endpoint: &GraphEndpoint) -> (&'static str, Vec<u8>) {
    match endpoint {
        GraphEndpoint::File(path) => ("file", path.as_bytes().to_vec()),
        GraphEndpoint::Symbol(id) => ("symbol", id.as_bytes().to_vec()),
    }
}

fn parse_relation(value: &str) -> Result<SyntaxRelationKind, ModuleCardEvidenceRepositoryError> {
    match value {
        "contains" => Ok(SyntaxRelationKind::Contains),
        "defines" => Ok(SyntaxRelationKind::Defines),
        "imports" => Ok(SyntaxRelationKind::Imports),
        "exports" => Ok(SyntaxRelationKind::Exports),
        "calls" => Ok(SyntaxRelationKind::Calls),
        "implements" => Ok(SyntaxRelationKind::Implements),
        "extends" => Ok(SyntaxRelationKind::Extends),
        "reads" => Ok(SyntaxRelationKind::Reads),
        "writes" => Ok(SyntaxRelationKind::Writes),
        "configures" => Ok(SyntaxRelationKind::Configures),
        "tests" => Ok(SyntaxRelationKind::Tests),
        "builds" => Ok(SyntaxRelationKind::Builds),
        "documents" => Ok(SyntaxRelationKind::Documents),
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

const fn relation_text(value: SyntaxRelationKind) -> &'static str {
    match value {
        SyntaxRelationKind::Contains => "contains",
        SyntaxRelationKind::Defines => "defines",
        SyntaxRelationKind::Imports => "imports",
        SyntaxRelationKind::Exports => "exports",
        SyntaxRelationKind::Calls => "calls",
        SyntaxRelationKind::Implements => "implements",
        SyntaxRelationKind::Extends => "extends",
        SyntaxRelationKind::Reads => "reads",
        SyntaxRelationKind::Writes => "writes",
        SyntaxRelationKind::Configures => "configures",
        SyntaxRelationKind::Tests => "tests",
        SyntaxRelationKind::Builds => "builds",
        SyntaxRelationKind::Documents => "documents",
    }
}

fn parse_provider(value: &str) -> Result<SyntaxProvider, ModuleCardEvidenceRepositoryError> {
    match value {
        "tree-sitter" => Ok(SyntaxProvider::TreeSitter),
        "manifest" => Ok(SyntaxProvider::Manifest),
        "language-heuristic" => Ok(SyntaxProvider::LanguageHeuristic),
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

const fn provider_text(value: SyntaxProvider) -> &'static str {
    match value {
        SyntaxProvider::TreeSitter => "tree-sitter",
        SyntaxProvider::Manifest => "manifest",
        SyntaxProvider::LanguageHeuristic => "language-heuristic",
    }
}

fn parse_resolution(value: &str) -> Result<LinkResolution, ModuleCardEvidenceRepositoryError> {
    match value {
        "adapter-local-symbol" => Ok(LinkResolution::AdapterLocalSymbol),
        "adapter-file" => Ok(LinkResolution::AdapterFile),
        "exact-module-reference" => Ok(LinkResolution::ExactModuleReference),
        "unique-file-local-name" => Ok(LinkResolution::UniqueFileLocalName),
        "unique-qualified-name" => Ok(LinkResolution::UniqueQualifiedName),
        _ => Err(ModuleCardEvidenceRepositoryError::InvalidStoredProjection),
    }
}

const fn resolution_text(value: LinkResolution) -> &'static str {
    match value {
        LinkResolution::AdapterLocalSymbol => "adapter-local-symbol",
        LinkResolution::AdapterFile => "adapter-file",
        LinkResolution::ExactModuleReference => "exact-module-reference",
        LinkResolution::UniqueFileLocalName => "unique-file-local-name",
        LinkResolution::UniqueQualifiedName => "unique-qualified-name",
    }
}

struct ModuleCardEvidenceReadGuard<'a> {
    control: &'a dyn ModuleCardEvidenceControl,
    started_at: Instant,
}

impl<'a> ModuleCardEvidenceReadGuard<'a> {
    fn new(
        control: &'a dyn ModuleCardEvidenceControl,
    ) -> Result<Self, ModuleCardEvidenceRepositoryError> {
        let guard = Self {
            control,
            started_at: Instant::now(),
        };
        guard.checkpoint()?;
        Ok(guard)
    }

    fn checkpoint(&self) -> Result<(), ModuleCardEvidenceRepositoryError> {
        if self.control.is_cancelled() {
            return Err(ModuleCardEvidenceRepositoryError::Cancelled);
        }
        if self.started_at.elapsed() >= MAX_MODULE_CARD_EVIDENCE_READ_DURATION {
            return Err(ModuleCardEvidenceRepositoryError::TimedOut);
        }
        Ok(())
    }
}

async fn rollback<T>(
    transaction: Transaction,
    error: ModuleCardEvidenceRepositoryError,
) -> Result<T, ModuleCardEvidenceRepositoryError> {
    match transaction.rollback().await {
        Ok(()) => Err(error),
        Err(source) => Err(ModuleCardEvidenceRepositoryError::Rollback(source)),
    }
}

fn read_blob(row: &libsql::Row, index: i32) -> Result<Vec<u8>, ModuleCardEvidenceRepositoryError> {
    row.get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)
}

fn read_optional_blob(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<Vec<u8>>, ModuleCardEvidenceRepositoryError> {
    row.get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)
}

fn read_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ModuleCardEvidenceRepositoryError> {
    read_blob(row, index)?
        .try_into()
        .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
}

fn read_optional_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, ModuleCardEvidenceRepositoryError> {
    read_optional_blob(row, index)?
        .map(|bytes| {
            bytes
                .try_into()
                .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_text(row: &libsql::Row, index: i32) -> Result<String, ModuleCardEvidenceRepositoryError> {
    row.get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)
}

fn read_optional_text(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<String>, ModuleCardEvidenceRepositoryError> {
    row.get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)
}

fn read_optional_u16(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u16>, ModuleCardEvidenceRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)?;
    value
        .map(|value| {
            u16::try_from(value)
                .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_optional_u32(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<u32>, ModuleCardEvidenceRepositoryError> {
    let value: Option<i64> = row
        .get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)?;
    value
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
        })
        .transpose()
}

fn read_u64(row: &libsql::Row, index: i32) -> Result<u64, ModuleCardEvidenceRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(ModuleCardEvidenceRepositoryError::Read)?;
    u64::try_from(value).map_err(|_| ModuleCardEvidenceRepositoryError::InvalidStoredProjection)
}

#[derive(Debug)]
pub(crate) enum ModuleCardEvidenceRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Selection(ModuleCardDetailRepositoryError),
    InvalidStoredProjection,
    Cancelled,
    TimedOut,
}

impl ModuleCardEvidenceRepositoryError {
    pub(crate) fn classify(&self) -> ModuleCardEvidenceFailure {
        match self {
            Self::Selection(error) => match error.classify() {
                a3_application::ModuleCardDetailFailure::Storage(error) => {
                    ModuleCardEvidenceFailure::Storage(error)
                }
                a3_application::ModuleCardDetailFailure::InvalidStoredProjection => {
                    ModuleCardEvidenceFailure::InvalidStoredProjection
                }
                a3_application::ModuleCardDetailFailure::Cancelled => {
                    ModuleCardEvidenceFailure::Cancelled
                }
                a3_application::ModuleCardDetailFailure::TimedOut => {
                    ModuleCardEvidenceFailure::TimedOut
                }
                a3_application::ModuleCardDetailFailure::ProgressUnavailable => {
                    ModuleCardEvidenceFailure::ProgressUnavailable
                }
            },
            Self::InvalidStoredProjection => ModuleCardEvidenceFailure::InvalidStoredProjection,
            Self::Cancelled => ModuleCardEvidenceFailure::Cancelled,
            Self::TimedOut => ModuleCardEvidenceFailure::TimedOut,
            Self::Begin(error)
            | Self::Read(error)
            | Self::Commit(error)
            | Self::Rollback(error) => {
                if is_corruption(error) {
                    ModuleCardEvidenceFailure::Storage(KnowledgeStoreFailure::Corrupt)
                } else {
                    ModuleCardEvidenceFailure::Storage(KnowledgeStoreFailure::Unavailable)
                }
            }
        }
    }
}

impl fmt::Display for ModuleCardEvidenceRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin Module Card Evidence read",
            Self::Read(_) => "could not read Module Card Evidence",
            Self::Commit(_) => "could not commit Module Card Evidence read",
            Self::Rollback(_) => "could not roll back Module Card Evidence read",
            Self::Selection(_) => "could not select the current Module Card",
            Self::InvalidStoredProjection => "stored Module Card Evidence projection is invalid",
            Self::Cancelled => "Module Card Evidence read was cancelled",
            Self::TimedOut => "Module Card Evidence read timed out",
        })
    }
}

impl Error for ModuleCardEvidenceRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Commit(source)
            | Self::Rollback(source) => Some(source),
            Self::Selection(source) => Some(source),
            Self::InvalidStoredProjection | Self::Cancelled | Self::TimedOut => None,
        }
    }
}

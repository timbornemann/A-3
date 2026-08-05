use crate::index_publication::{IndexPublicationRepositoryError, MutationProgress, read_stable_id};
use a3_domain::{
    ExactSearchTextError, FileRevision, GraphEndpoint, GraphSymbol, IndexPublication, IndexRunId,
    QualifiedSymbolName, RepositoryPath, SymbolId, SyntaxRelationKind,
};
use libsql::{Transaction, Value, params, params_from_iter};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BATCH_PARAMETERS: usize = 30_000;
const MAX_BATCH_ROWS: usize = 1_024;
const PROJECTION_VERSION: i64 = 1;

pub(crate) struct ExactSearchProjection {
    symbols: Vec<(SymbolId, QualifiedSymbolName)>,
    manifests: Vec<FileRevision>,
}

impl ExactSearchProjection {
    pub(crate) fn symbols(&self) -> &[(SymbolId, QualifiedSymbolName)] {
        &self.symbols
    }

    pub(crate) fn work_units(&self) -> Result<u64, IndexPublicationRepositoryError> {
        [self.symbols.len(), self.manifests.len()]
            .into_iter()
            .try_fold(1_u64, |total, length| {
                u64::try_from(length)
                    .ok()
                    .and_then(|length| total.checked_add(length))
                    .ok_or(IndexPublicationRepositoryError::ResourceLimit)
            })
    }
}

pub(crate) fn build_projection(
    publication: &IndexPublication,
) -> Result<ExactSearchProjection, IndexPublicationRepositoryError> {
    let symbols = publication
        .graph()
        .symbols()
        .iter()
        .map(|symbol| (symbol.id(), symbol))
        .collect::<BTreeMap<_, _>>();
    let mut parents = BTreeMap::new();
    for edge in publication.graph().edges() {
        if edge.kind() != SyntaxRelationKind::Contains {
            continue;
        }
        let (GraphEndpoint::Symbol(parent), GraphEndpoint::Symbol(child)) =
            (edge.source(), edge.target())
        else {
            continue;
        };
        if parent == child || !symbols.contains_key(parent) || !symbols.contains_key(child) {
            return Err(IndexPublicationRepositoryError::PublicationMismatch);
        }
        if parents
            .insert(*child, *parent)
            .is_some_and(|existing| existing != *parent)
        {
            return Err(IndexPublicationRepositoryError::PublicationMismatch);
        }
    }

    let mut cache = BTreeMap::new();
    let mut qualified = Vec::with_capacity(symbols.len());
    for id in symbols.keys().copied() {
        let name = qualified_name(id, &symbols, &parents, &mut cache)?;
        qualified.push((id, name));
    }
    Ok(ExactSearchProjection {
        symbols: qualified,
        manifests: publication.manifest_files().to_vec(),
    })
}

fn qualified_name(
    id: SymbolId,
    symbols: &BTreeMap<SymbolId, &GraphSymbol>,
    parents: &BTreeMap<SymbolId, SymbolId>,
    cache: &mut BTreeMap<SymbolId, QualifiedSymbolName>,
) -> Result<QualifiedSymbolName, IndexPublicationRepositoryError> {
    if let Some(name) = cache.get(&id) {
        return Ok(name.clone());
    }
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = id;
    let prefix = loop {
        if let Some(name) = cache.get(&current) {
            break Some(name.as_str().to_owned());
        }
        if !visited.insert(current) {
            return Err(IndexPublicationRepositoryError::PublicationMismatch);
        }
        if !symbols.contains_key(&current) {
            return Err(IndexPublicationRepositoryError::PublicationMismatch);
        }
        chain.push(current);
        match parents.get(&current) {
            Some(parent) => current = *parent,
            None => break None,
        }
    };

    let mut assembled = prefix.unwrap_or_default();
    for member in chain.into_iter().rev() {
        let symbol = symbols
            .get(&member)
            .ok_or(IndexPublicationRepositoryError::PublicationMismatch)?;
        if !assembled.is_empty() {
            assembled.push_str("::");
        }
        assembled.push_str(symbol.parsed().name().as_str());
        let name =
            QualifiedSymbolName::try_from_string(assembled.clone()).map_err(
                |error| match error {
                    ExactSearchTextError::InvalidLength(_) => {
                        IndexPublicationRepositoryError::ResourceLimit
                    }
                    ExactSearchTextError::InvalidCharacter => {
                        IndexPublicationRepositoryError::PublicationMismatch
                    }
                },
            )?;
        cache.insert(member, name);
    }
    cache
        .get(&id)
        .cloned()
        .ok_or(IndexPublicationRepositoryError::PublicationMismatch)
}

pub(crate) async fn write_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    projection: &ExactSearchProjection,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    let symbol_count = i64::try_from(projection.symbols.len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    let manifest_count = i64::try_from(projection.manifests.len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    transaction
        .execute(
            "INSERT INTO exact_search_projections\n\
             (index_run_id, projection_version, symbol_count, manifest_count)\n\
             VALUES (?1, ?2, ?3, ?4)",
            params![
                run_id.as_bytes().to_vec(),
                PROJECTION_VERSION,
                symbol_count,
                manifest_count
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    progress.advance(1)?;

    write_batches(
        transaction,
        &projection.symbols,
        "INSERT INTO exact_search_symbols\n\
         (index_run_id, symbol_id, qualified_name) VALUES ",
        |(symbol_id, qualified_name)| {
            vec![
                Value::Blob(run_id.as_bytes().to_vec()),
                Value::Blob(symbol_id.as_bytes().to_vec()),
                Value::Text(qualified_name.as_str().to_owned()),
            ]
        },
        progress,
    )
    .await?;
    write_batches(
        transaction,
        &projection.manifests,
        "INSERT INTO exact_search_manifests\n\
         (index_run_id, repository_path, content_hash) VALUES ",
        |revision| {
            vec![
                Value::Blob(run_id.as_bytes().to_vec()),
                Value::Blob(revision.path().as_bytes().to_vec()),
                Value::Blob(revision.content_hash().as_bytes().to_vec()),
            ]
        },
        progress,
    )
    .await
}

async fn write_batches<T, F>(
    transaction: &Transaction,
    items: &[T],
    prefix: &str,
    values: F,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError>
where
    F: Fn(&T) -> Vec<Value>,
{
    const COLUMNS: usize = 3;
    let rows_per_batch = MAX_BATCH_PARAMETERS
        .checked_div(COLUMNS)
        .map(|rows| rows.min(MAX_BATCH_ROWS))
        .filter(|rows| *rows > 0)
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)?;
    for chunk in items.chunks(rows_per_batch) {
        progress.checkpoint()?;
        let mut sql = String::from(prefix);
        let mut parameters = Vec::with_capacity(chunk.len().saturating_mul(COLUMNS));
        for (index, item) in chunk.iter().enumerate() {
            if index > 0 {
                sql.push(',');
            }
            sql.push_str("(?, ?, ?)");
            parameters.extend(values(item));
        }
        transaction
            .execute(&sql, params_from_iter(parameters))
            .await
            .map_err(IndexPublicationRepositoryError::Write)?;
        progress.advance(
            u64::try_from(chunk.len())
                .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?,
        )?;
    }
    Ok(())
}

pub(crate) async fn read_manifest_files(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<FileRevision>, IndexPublicationRepositoryError> {
    let mut marker_rows = transaction
        .query(
            "SELECT projection_version, symbol_count, manifest_count\n\
             FROM exact_search_projections WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let Some(marker) = marker_rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    else {
        return Ok(Vec::new());
    };
    let version: i64 = marker
        .get(0)
        .map_err(IndexPublicationRepositoryError::Read)?;
    let expected_symbols: i64 = marker
        .get(1)
        .map_err(IndexPublicationRepositoryError::Read)?;
    let expected_manifests: i64 = marker
        .get(2)
        .map_err(IndexPublicationRepositoryError::Read)?;
    if version != PROJECTION_VERSION
        || expected_symbols < 0
        || expected_manifests < 0
        || marker_rows
            .next()
            .await
            .map_err(IndexPublicationRepositoryError::Read)?
            .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    progress.advance(1)?;

    let actual_symbols = count_projection_rows(transaction, "exact_search_symbols", run_id).await?;
    if actual_symbols != expected_symbols {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    let mut rows = transaction
        .query(
            "SELECT repository_path, content_hash FROM exact_search_manifests\n\
             WHERE index_run_id = ?1 ORDER BY repository_path",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut manifests = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let path: Vec<u8> = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
        manifests.push(FileRevision::new(
            RepositoryPath::try_from_bytes(path)
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            a3_domain::ContentHash::from_bytes(read_stable_id(&row, 1)?),
        ));
        progress.advance(1)?;
    }
    if i64::try_from(manifests.len()).map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?
        != expected_manifests
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(manifests)
}

async fn count_projection_rows(
    transaction: &Transaction,
    table: &str,
    run_id: IndexRunId,
) -> Result<i64, IndexPublicationRepositoryError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE index_run_id = ?1");
    let mut rows = transaction
        .query(&sql, [run_id.as_bytes().to_vec()])
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
    if count < 0
        || rows
            .next()
            .await
            .map_err(IndexPublicationRepositoryError::Read)?
            .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::build_projection;
    use crate::index_publication::IndexPublicationRepositoryError;
    use a3_domain::{
        Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
        GraphSymbol, IndexLanguage, IndexPublication, LinkResolution, LinkedGraph, LocalSymbolId,
        ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion,
        ModuleProjection, ModuleRoot, ModuleSymbolSet, ParsedSymbol, RankProjection, RankScore,
        RankingPolicyVersion, RepositoryCard, RepositoryModule, RepositoryPath, SnapshotId,
        SourcePosition, SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank,
        SymbolRankSignals, SyntaxProvider, SyntaxRelationKind,
    };

    #[test]
    fn qualified_names_reject_multiple_containment_parents()
    -> Result<(), Box<dyn std::error::Error>> {
        let publication = publication(&[(1, 3), (2, 3)])?;
        assert!(matches!(
            build_projection(&publication),
            Err(IndexPublicationRepositoryError::PublicationMismatch)
        ));
        Ok(())
    }

    #[test]
    fn qualified_names_reject_containment_cycles() -> Result<(), Box<dyn std::error::Error>> {
        let publication = publication(&[(1, 2), (2, 1)])?;
        assert!(matches!(
            build_projection(&publication),
            Err(IndexPublicationRepositoryError::PublicationMismatch)
        ));
        Ok(())
    }

    fn publication(edges: &[(u8, u8)]) -> Result<IndexPublication, Box<dyn std::error::Error>> {
        let snapshot_id = SnapshotId::from_bytes([4; 32]);
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())?,
            ContentHash::from_bytes([5; 32]),
        );
        let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
        let symbols = (1_u8..=3)
            .map(|value| {
                Ok(GraphSymbol::new(
                    SymbolId::from_bytes([value; 32]),
                    revision.clone(),
                    ParsedSymbol::new(
                        LocalSymbolId::new(u32::from(value))?,
                        SymbolKind::Module,
                        SymbolName::try_from_string(format!("symbol_{value}"))?,
                        range,
                        range,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let evidence = EvidenceRef::new(revision.clone(), range);
        let edges = edges
            .iter()
            .map(|(source, target)| {
                GraphEdge::new(
                    GraphEndpoint::Symbol(SymbolId::from_bytes([*source; 32])),
                    GraphEndpoint::Symbol(SymbolId::from_bytes([*target; 32])),
                    SyntaxRelationKind::Contains,
                    SyntaxProvider::TreeSitter,
                    Confidence::certain(),
                    LinkResolution::AdapterLocalSymbol,
                    snapshot_id,
                    evidence.clone(),
                )
            })
            .collect();
        let graph = LinkedGraph::new(snapshot_id, vec![revision], symbols, edges, Vec::new())?;
        let ranks = (1_u8..=3)
            .map(|value| {
                Ok(SymbolRank::new(
                    SymbolId::from_bytes([value; 32]),
                    RankScore::try_from_sum(0)?,
                    SymbolRankSignals {
                        in_degree: 0,
                        out_degree: 0,
                        centrality: Centrality::from_basis_points(0)?,
                        degree_contribution: 0,
                        centrality_contribution: 0,
                        entrypoint_contribution: 0,
                        public_export_contribution: 0,
                        manifest_contribution: 0,
                        test_contribution: 0,
                    },
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let ranking = RankProjection::new(snapshot_id, RankingPolicyVersion::v1(), ranks)?;
        let module_id = ModuleId::from_bytes([9; 32]);
        let members = graph
            .symbols()
            .iter()
            .map(|symbol| {
                ModuleMembership::new(
                    module_id,
                    symbol.id(),
                    ModuleMembershipEvidence::path(symbol.revision().clone()),
                )
            })
            .collect();
        let ranked = ranking
            .symbols()
            .iter()
            .map(|rank| rank.symbol_id())
            .collect::<Vec<_>>();
        let module = RepositoryModule::new(
            module_id,
            ModuleKind::PathBoundary,
            Some(ModuleRoot::Repository),
            Vec::new(),
            ModuleSymbolSet::new(ranked, false)?,
            ModuleSymbolSet::empty(),
            ModuleSymbolSet::empty(),
        )?;
        let policy = ModulePolicyVersion::v1();
        let card = RepositoryCard::new(
            snapshot_id,
            policy,
            vec![module_id],
            vec![IndexLanguage::Generic],
            ModuleSymbolSet::empty(),
            1,
            3,
        )?;
        let modules = ModuleProjection::new(snapshot_id, policy, vec![module], members, card)?;
        Ok(IndexPublication::new(graph, ranking, Vec::new(), modules)?)
    }
}

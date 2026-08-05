use crate::index_publication::{IndexPublicationRepositoryError, MutationProgress, read_stable_id};
use a3_domain::{
    Centrality, Confidence, ContentHash, EvidenceRef, FileRevision, GraphEdge, GraphEndpoint,
    GraphSymbol, IndexPublication, IndexRunId, IndexRunRecord, LinkResolution, LinkedGraph,
    LocalSymbolId, ParsedSymbol, RankProjection, RankScore, RepositoryPath, SourcePosition,
    SourceRange, SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, SymbolReference,
    SymbolRole, SymbolRoles, SymbolSignature, SymbolVisibility, SyntaxProvider, SyntaxRelationKind,
    UnresolvedEdgeCandidate, UnresolvedGraphTarget, UnresolvedReason,
};
use libsql::{Transaction, params};

pub(crate) async fn write_publication_rows(
    transaction: &Transaction,
    run_id: IndexRunId,
    publication: &IndexPublication,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    for symbol in publication.graph().symbols() {
        write_symbol(transaction, run_id, symbol).await?;
        progress.advance(1)?;
    }
    for (index, edge) in publication.graph().edges().iter().enumerate() {
        write_edge(transaction, run_id, sequence(index)?, edge).await?;
        progress.advance(1)?;
    }
    for (index, candidate) in publication.graph().unresolved().iter().enumerate() {
        write_candidate(transaction, run_id, sequence(index)?, candidate).await?;
        progress.advance(1)?;
    }
    for (index, rank) in publication.ranking().symbols().iter().enumerate() {
        write_rank(transaction, run_id, sequence(index)?, *rank).await?;
        progress.advance(1)?;
    }
    Ok(())
}

pub(crate) async fn read_publication_rows(
    transaction: &Transaction,
    run: IndexRunRecord,
    progress: &mut MutationProgress<'_>,
) -> Result<IndexPublication, IndexPublicationRepositoryError> {
    let files = read_files(transaction, run.id(), progress).await?;
    let symbols = read_symbols(transaction, run.id(), progress).await?;
    let edges = read_edges(transaction, run, progress).await?;
    let unresolved = read_candidates(transaction, run, progress).await?;
    let graph = LinkedGraph::new(run.snapshot_id(), files, symbols, edges, unresolved)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    let ranks = read_ranks(transaction, run.id(), progress).await?;
    let ranking = RankProjection::new(run.snapshot_id(), run.ranking_policy_version(), ranks)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    IndexPublication::new(graph, ranking)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

async fn write_symbol(
    transaction: &Transaction,
    run_id: IndexRunId,
    symbol: &GraphSymbol,
) -> Result<(), IndexPublicationRepositoryError> {
    let parsed = symbol.parsed();
    let declaration = range_values(parsed.declaration_range());
    let selection = range_values(parsed.selection_range());
    let documentation = optional_range_values(parsed.documentation_range());
    transaction
        .execute(
            "INSERT INTO symbols (\n\
             index_run_id, symbol_id, repository_path, content_hash, local_symbol_id, kind, name,\n\
             signature, declaration_start_byte, declaration_end_byte, declaration_start_row,\n\
             declaration_start_column, declaration_end_row, declaration_end_column,\n\
             selection_start_byte, selection_end_byte, selection_start_row, selection_start_column,\n\
             selection_end_row, selection_end_column, documentation_start_byte,\n\
             documentation_end_byte, documentation_start_row, documentation_start_column,\n\
             documentation_end_row, documentation_end_column, visibility, roles\n\
             ) VALUES (\n\
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,\n\
             ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28\n\
             )",
            params![
                run_id.as_bytes().to_vec(),
                symbol.id().as_bytes().to_vec(),
                symbol.revision().path().as_bytes().to_vec(),
                symbol.revision().content_hash().as_bytes().to_vec(),
                i64::from(parsed.id().get()),
                symbol_kind_to_stored(parsed.kind()),
                parsed.name().as_str(),
                parsed.signature().map(SymbolSignature::as_str),
                declaration[0], declaration[1], declaration[2], declaration[3], declaration[4],
                declaration[5], selection[0], selection[1], selection[2], selection[3], selection[4],
                selection[5], documentation[0], documentation[1], documentation[2], documentation[3],
                documentation[4], documentation[5], visibility_to_stored(parsed.visibility()),
                i64::from(roles_to_stored(parsed.roles()))
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn write_edge(
    transaction: &Transaction,
    run_id: IndexRunId,
    edge_sequence: i64,
    edge: &GraphEdge,
) -> Result<(), IndexPublicationRepositoryError> {
    let (source_kind, source_value) = endpoint_to_stored(edge.source());
    let (target_kind, target_value) = endpoint_to_stored(edge.target());
    let evidence = edge.evidence();
    let range = range_values(evidence.range());
    transaction
        .execute(
            "INSERT INTO symbol_edges (\n\
             index_run_id, edge_sequence, source_kind, source_value, target_kind, target_value,\n\
             relation_kind, provider, confidence, resolution, evidence_path, evidence_hash,\n\
             evidence_start_byte, evidence_end_byte, evidence_start_row, evidence_start_column,\n\
             evidence_end_row, evidence_end_column\n\
             ) VALUES (\n\
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18\n\
             )",
            params![
                run_id.as_bytes().to_vec(),
                edge_sequence,
                source_kind,
                source_value,
                target_kind,
                target_value,
                relation_kind_to_stored(edge.kind()),
                provider_to_stored(edge.provider()),
                i64::from(edge.confidence().basis_points()),
                resolution_to_stored(edge.resolution()),
                evidence.revision().path().as_bytes().to_vec(),
                evidence.revision().content_hash().as_bytes().to_vec(),
                range[0],
                range[1],
                range[2],
                range[3],
                range[4],
                range[5]
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn write_candidate(
    transaction: &Transaction,
    run_id: IndexRunId,
    candidate_sequence: i64,
    candidate: &UnresolvedEdgeCandidate,
) -> Result<(), IndexPublicationRepositoryError> {
    let (source_kind, source_value) = endpoint_to_stored(candidate.source());
    let (target_kind, target_value) = unresolved_target_to_stored(candidate.target());
    let evidence = candidate.evidence();
    let range = range_values(evidence.range());
    transaction
        .execute(
            "INSERT INTO unresolved_edges (\n\
             index_run_id, candidate_sequence, source_kind, source_value, target_kind, target_value,\n\
             relation_kind, provider, confidence, reason, evidence_path, evidence_hash,\n\
             evidence_start_byte, evidence_end_byte, evidence_start_row, evidence_start_column,\n\
             evidence_end_row, evidence_end_column\n\
             ) VALUES (\n\
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18\n\
             )",
            params![
                run_id.as_bytes().to_vec(),
                candidate_sequence,
                source_kind,
                source_value,
                target_kind,
                target_value,
                relation_kind_to_stored(candidate.kind()),
                provider_to_stored(candidate.provider()),
                i64::from(candidate.confidence().basis_points()),
                unresolved_reason_to_stored(candidate.reason()),
                evidence.revision().path().as_bytes().to_vec(),
                evidence.revision().content_hash().as_bytes().to_vec(),
                range[0], range[1], range[2], range[3], range[4], range[5]
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn write_rank(
    transaction: &Transaction,
    run_id: IndexRunId,
    rank_order: i64,
    rank: SymbolRank,
) -> Result<(), IndexPublicationRepositoryError> {
    let signals = rank.signals();
    transaction
        .execute(
            "INSERT INTO ranking_projections (\n\
             index_run_id, symbol_id, rank_order, score, in_degree, out_degree, centrality,\n\
             degree_contribution, centrality_contribution, entrypoint_contribution,\n\
             public_export_contribution, manifest_contribution, test_contribution\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                run_id.as_bytes().to_vec(),
                rank.symbol_id().as_bytes().to_vec(),
                rank_order,
                i64::from(rank.score().get()),
                i64::from(signals.in_degree),
                i64::from(signals.out_degree),
                i64::from(signals.centrality.basis_points()),
                i64::from(signals.degree_contribution),
                i64::from(signals.centrality_contribution),
                i64::from(signals.entrypoint_contribution),
                i64::from(signals.public_export_contribution),
                i64::from(signals.manifest_contribution),
                i64::from(signals.test_contribution)
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    Ok(())
}

async fn read_files(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<FileRevision>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT repository_path, content_hash FROM file_revisions\n\
             WHERE index_run_id = ?1 ORDER BY repository_path",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut files = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        files.push(FileRevision::new(
            read_path(&row, 0)?,
            ContentHash::from_bytes(read_stable_id(&row, 1)?),
        ));
        progress.advance(1)?;
    }
    Ok(files)
}

async fn read_symbols(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<GraphSymbol>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT symbol_id, repository_path, content_hash, local_symbol_id, kind, name, signature,\n\
             declaration_start_byte, declaration_end_byte, declaration_start_row,\n\
             declaration_start_column, declaration_end_row, declaration_end_column,\n\
             selection_start_byte, selection_end_byte, selection_start_row, selection_start_column,\n\
             selection_end_row, selection_end_column, documentation_start_byte,\n\
             documentation_end_byte, documentation_start_row, documentation_start_column,\n\
             documentation_end_row, documentation_end_column, visibility, roles\n\
             FROM symbols WHERE index_run_id = ?1 ORDER BY symbol_id",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut symbols = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let id = SymbolId::from_bytes(read_stable_id(&row, 0)?);
        let revision = FileRevision::new(
            read_path(&row, 1)?,
            ContentHash::from_bytes(read_stable_id(&row, 2)?),
        );
        let local_id = LocalSymbolId::new(read_u32(&row, 3)?)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        let kind: String = row.get(4).map_err(IndexPublicationRepositoryError::Read)?;
        let name: String = row.get(5).map_err(IndexPublicationRepositoryError::Read)?;
        let signature: Option<String> =
            row.get(6).map_err(IndexPublicationRepositoryError::Read)?;
        let declaration = read_range(&row, 7)?;
        let selection = read_range(&row, 13)?;
        let documentation = read_optional_range(&row, 19)?;
        let visibility: String = row.get(25).map_err(IndexPublicationRepositoryError::Read)?;
        let roles = read_u8(&row, 26)?;
        let mut parsed = ParsedSymbol::new(
            local_id,
            symbol_kind_from_stored(&kind)?,
            SymbolName::try_from_string(name)
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            declaration,
            selection,
        )
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?
        .with_visibility(visibility_from_stored(&visibility)?);
        if let Some(signature) = signature {
            parsed = parsed.with_signature(
                SymbolSignature::try_from_string(signature)
                    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            );
        }
        if roles & 1 != 0 {
            parsed = parsed.with_role(SymbolRole::Test);
        }
        if roles & 2 != 0 {
            parsed = parsed.with_role(SymbolRole::Entrypoint);
        }
        if roles & !3 != 0 {
            return Err(IndexPublicationRepositoryError::InvalidStoredData);
        }
        if let Some(documentation) = documentation {
            parsed = parsed.with_documentation_range(documentation);
        }
        symbols.push(GraphSymbol::new(id, revision, parsed));
        progress.advance(1)?;
    }
    Ok(symbols)
}

async fn read_edges(
    transaction: &Transaction,
    run: IndexRunRecord,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<GraphEdge>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT source_kind, source_value, target_kind, target_value, relation_kind, provider,\n\
             confidence, resolution, evidence_path, evidence_hash, evidence_start_byte,\n\
             evidence_end_byte, evidence_start_row, evidence_start_column, evidence_end_row,\n\
             evidence_end_column FROM symbol_edges WHERE index_run_id = ?1 ORDER BY edge_sequence",
            [run.id().as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut edges = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let source_kind: String = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
        let source_value: Vec<u8> = row.get(1).map_err(IndexPublicationRepositoryError::Read)?;
        let target_kind: String = row.get(2).map_err(IndexPublicationRepositoryError::Read)?;
        let target_value: Vec<u8> = row.get(3).map_err(IndexPublicationRepositoryError::Read)?;
        let relation: String = row.get(4).map_err(IndexPublicationRepositoryError::Read)?;
        let provider: String = row.get(5).map_err(IndexPublicationRepositoryError::Read)?;
        let confidence = Confidence::from_basis_points(read_u16(&row, 6)?)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        let resolution: String = row.get(7).map_err(IndexPublicationRepositoryError::Read)?;
        let evidence = read_evidence(&row, 8, 9, 10)?;
        edges.push(GraphEdge::new(
            endpoint_from_stored(&source_kind, source_value)?,
            endpoint_from_stored(&target_kind, target_value)?,
            relation_kind_from_stored(&relation)?,
            provider_from_stored(&provider)?,
            confidence,
            resolution_from_stored(&resolution)?,
            run.snapshot_id(),
            evidence,
        ));
        progress.advance(1)?;
    }
    Ok(edges)
}

async fn read_candidates(
    transaction: &Transaction,
    run: IndexRunRecord,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<UnresolvedEdgeCandidate>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT source_kind, source_value, target_kind, target_value, relation_kind, provider,\n\
             confidence, reason, evidence_path, evidence_hash, evidence_start_byte,\n\
             evidence_end_byte, evidence_start_row, evidence_start_column, evidence_end_row,\n\
             evidence_end_column FROM unresolved_edges WHERE index_run_id = ?1\n\
             ORDER BY candidate_sequence",
            [run.id().as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let source_kind: String = row.get(0).map_err(IndexPublicationRepositoryError::Read)?;
        let source_value: Vec<u8> = row.get(1).map_err(IndexPublicationRepositoryError::Read)?;
        let target_kind: String = row.get(2).map_err(IndexPublicationRepositoryError::Read)?;
        let target_value: Vec<u8> = row.get(3).map_err(IndexPublicationRepositoryError::Read)?;
        let relation: String = row.get(4).map_err(IndexPublicationRepositoryError::Read)?;
        let provider: String = row.get(5).map_err(IndexPublicationRepositoryError::Read)?;
        let confidence = Confidence::from_basis_points(read_u16(&row, 6)?)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        let reason: String = row.get(7).map_err(IndexPublicationRepositoryError::Read)?;
        let evidence = read_evidence(&row, 8, 9, 10)?;
        candidates.push(UnresolvedEdgeCandidate::new(
            endpoint_from_stored(&source_kind, source_value)?,
            unresolved_target_from_stored(&target_kind, target_value)?,
            relation_kind_from_stored(&relation)?,
            provider_from_stored(&provider)?,
            confidence,
            unresolved_reason_from_stored(&reason)?,
            run.snapshot_id(),
            evidence,
        ));
        progress.advance(1)?;
    }
    Ok(candidates)
}

async fn read_ranks(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<SymbolRank>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT symbol_id, score, in_degree, out_degree, centrality, degree_contribution,\n\
             centrality_contribution, entrypoint_contribution, public_export_contribution,\n\
             manifest_contribution, test_contribution FROM ranking_projections\n\
             WHERE index_run_id = ?1 ORDER BY rank_order",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut ranks = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let score = RankScore::try_from_sum(u64::from(read_u32(&row, 1)?))
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        let centrality = Centrality::from_basis_points(read_u16(&row, 4)?)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
        ranks.push(SymbolRank::new(
            SymbolId::from_bytes(read_stable_id(&row, 0)?),
            score,
            SymbolRankSignals {
                in_degree: read_u32(&row, 2)?,
                out_degree: read_u32(&row, 3)?,
                centrality,
                degree_contribution: read_u32(&row, 5)?,
                centrality_contribution: read_u32(&row, 6)?,
                entrypoint_contribution: read_u32(&row, 7)?,
                public_export_contribution: read_u32(&row, 8)?,
                manifest_contribution: read_u32(&row, 9)?,
                test_contribution: read_u32(&row, 10)?,
            },
        ));
        progress.advance(1)?;
    }
    Ok(ranks)
}

fn read_evidence(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
    range_index: i32,
) -> Result<EvidenceRef, IndexPublicationRepositoryError> {
    Ok(EvidenceRef::new(
        FileRevision::new(
            read_path(row, path_index)?,
            ContentHash::from_bytes(read_stable_id(row, hash_index)?),
        ),
        read_range(row, range_index)?,
    ))
}

fn read_path(
    row: &libsql::Row,
    index: i32,
) -> Result<RepositoryPath, IndexPublicationRepositoryError> {
    let bytes: Vec<u8> = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    RepositoryPath::try_from_bytes(bytes)
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_range(
    row: &libsql::Row,
    start: i32,
) -> Result<SourceRange, IndexPublicationRepositoryError> {
    SourceRange::new(
        read_usize(row, start)?,
        read_usize(row, start + 1)?,
        SourcePosition::new(read_u32(row, start + 2)?, read_u32(row, start + 3)?),
        SourcePosition::new(read_u32(row, start + 4)?, read_u32(row, start + 5)?),
    )
    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_optional_range(
    row: &libsql::Row,
    start: i32,
) -> Result<Option<SourceRange>, IndexPublicationRepositoryError> {
    let start_byte: Option<i64> = row
        .get(start)
        .map_err(IndexPublicationRepositoryError::Read)?;
    let values = [
        start_byte,
        row.get(start + 1)
            .map_err(IndexPublicationRepositoryError::Read)?,
        row.get(start + 2)
            .map_err(IndexPublicationRepositoryError::Read)?,
        row.get(start + 3)
            .map_err(IndexPublicationRepositoryError::Read)?,
        row.get(start + 4)
            .map_err(IndexPublicationRepositoryError::Read)?,
        row.get(start + 5)
            .map_err(IndexPublicationRepositoryError::Read)?,
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let values =
        values.map(|value| value.ok_or(IndexPublicationRepositoryError::InvalidStoredData));
    let [
        start_byte,
        end_byte,
        start_row,
        start_column,
        end_row,
        end_column,
    ] = values;
    let range = SourceRange::new(
        checked_usize(start_byte?)?,
        checked_usize(end_byte?)?,
        SourcePosition::new(checked_u32(start_row?)?, checked_u32(start_column?)?),
        SourcePosition::new(checked_u32(end_row?)?, checked_u32(end_column?)?),
    )
    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    Ok(Some(range))
}

fn read_u8(row: &libsql::Row, index: i32) -> Result<u8, IndexPublicationRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    u8::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_u16(row: &libsql::Row, index: i32) -> Result<u16, IndexPublicationRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    u16::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, IndexPublicationRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    checked_u32(value)
}

fn read_usize(row: &libsql::Row, index: i32) -> Result<usize, IndexPublicationRepositoryError> {
    let value: i64 = row
        .get(index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    checked_usize(value)
}

fn checked_u32(value: i64) -> Result<u32, IndexPublicationRepositoryError> {
    u32::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn checked_usize(value: i64) -> Result<usize, IndexPublicationRepositoryError> {
    usize::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn sequence(index: usize) -> Result<i64, IndexPublicationRepositoryError> {
    index
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)
}

fn range_values(range: SourceRange) -> [i64; 6] {
    [
        i64::from(range.start_byte()),
        i64::from(range.end_byte()),
        i64::from(range.start_position().row()),
        i64::from(range.start_position().column()),
        i64::from(range.end_position().row()),
        i64::from(range.end_position().column()),
    ]
}

fn optional_range_values(range: Option<SourceRange>) -> [Option<i64>; 6] {
    range
        .map(range_values)
        .map_or([None; 6], |values| values.map(Some))
}

fn endpoint_to_stored(endpoint: &GraphEndpoint) -> (&'static str, Vec<u8>) {
    match endpoint {
        GraphEndpoint::File(path) => ("file", path.as_bytes().to_vec()),
        GraphEndpoint::Symbol(id) => ("symbol", id.as_bytes().to_vec()),
    }
}

fn endpoint_from_stored(
    kind: &str,
    value: Vec<u8>,
) -> Result<GraphEndpoint, IndexPublicationRepositoryError> {
    match kind {
        "file" => RepositoryPath::try_from_bytes(value)
            .map(GraphEndpoint::File)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData),
        "symbol" => value
            .try_into()
            .map(SymbolId::from_bytes)
            .map(GraphEndpoint::Symbol)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn unresolved_target_to_stored(target: &UnresolvedGraphTarget) -> (&'static str, Vec<u8>) {
    match target {
        UnresolvedGraphTarget::File(path) => ("file", path.as_bytes().to_vec()),
        UnresolvedGraphTarget::Reference(reference) => {
            ("reference", reference.as_str().as_bytes().to_vec())
        }
    }
}

fn unresolved_target_from_stored(
    kind: &str,
    value: Vec<u8>,
) -> Result<UnresolvedGraphTarget, IndexPublicationRepositoryError> {
    match kind {
        "file" => RepositoryPath::try_from_bytes(value)
            .map(UnresolvedGraphTarget::File)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData),
        "reference" => String::from_utf8(value)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
            .and_then(|value| {
                SymbolReference::try_from_string(value)
                    .map(UnresolvedGraphTarget::Reference)
                    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
            }),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn symbol_kind_to_stored(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Module => "module",
        SymbolKind::Namespace => "namespace",
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Interface => "interface",
        SymbolKind::Class => "class",
        SymbolKind::Implementation => "implementation",
        SymbolKind::TypeAlias => "type-alias",
        SymbolKind::Constant => "constant",
        SymbolKind::Static => "static",
        SymbolKind::Variable => "variable",
        SymbolKind::Field => "field",
        SymbolKind::Variant => "variant",
        SymbolKind::Parameter => "parameter",
    }
}

fn symbol_kind_from_stored(kind: &str) -> Result<SymbolKind, IndexPublicationRepositoryError> {
    match kind {
        "module" => Ok(SymbolKind::Module),
        "namespace" => Ok(SymbolKind::Namespace),
        "function" => Ok(SymbolKind::Function),
        "method" => Ok(SymbolKind::Method),
        "struct" => Ok(SymbolKind::Struct),
        "enum" => Ok(SymbolKind::Enum),
        "trait" => Ok(SymbolKind::Trait),
        "interface" => Ok(SymbolKind::Interface),
        "class" => Ok(SymbolKind::Class),
        "implementation" => Ok(SymbolKind::Implementation),
        "type-alias" => Ok(SymbolKind::TypeAlias),
        "constant" => Ok(SymbolKind::Constant),
        "static" => Ok(SymbolKind::Static),
        "variable" => Ok(SymbolKind::Variable),
        "field" => Ok(SymbolKind::Field),
        "variant" => Ok(SymbolKind::Variant),
        "parameter" => Ok(SymbolKind::Parameter),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn visibility_to_stored(visibility: SymbolVisibility) -> &'static str {
    match visibility {
        SymbolVisibility::Public => "public",
        SymbolVisibility::Protected => "protected",
        SymbolVisibility::Private => "private",
        SymbolVisibility::Internal => "internal",
        SymbolVisibility::Local => "local",
        SymbolVisibility::Unknown => "unknown",
    }
}

fn visibility_from_stored(
    visibility: &str,
) -> Result<SymbolVisibility, IndexPublicationRepositoryError> {
    match visibility {
        "public" => Ok(SymbolVisibility::Public),
        "protected" => Ok(SymbolVisibility::Protected),
        "private" => Ok(SymbolVisibility::Private),
        "internal" => Ok(SymbolVisibility::Internal),
        "local" => Ok(SymbolVisibility::Local),
        "unknown" => Ok(SymbolVisibility::Unknown),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn roles_to_stored(roles: SymbolRoles) -> u8 {
    u8::from(roles.contains(SymbolRole::Test))
        | (u8::from(roles.contains(SymbolRole::Entrypoint)) << 1)
}

fn relation_kind_to_stored(kind: SyntaxRelationKind) -> &'static str {
    match kind {
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

fn relation_kind_from_stored(
    kind: &str,
) -> Result<SyntaxRelationKind, IndexPublicationRepositoryError> {
    match kind {
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
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn provider_to_stored(provider: SyntaxProvider) -> &'static str {
    match provider {
        SyntaxProvider::TreeSitter => "tree-sitter",
        SyntaxProvider::Manifest => "manifest",
        SyntaxProvider::LanguageHeuristic => "language-heuristic",
    }
}

fn provider_from_stored(provider: &str) -> Result<SyntaxProvider, IndexPublicationRepositoryError> {
    match provider {
        "tree-sitter" => Ok(SyntaxProvider::TreeSitter),
        "manifest" => Ok(SyntaxProvider::Manifest),
        "language-heuristic" => Ok(SyntaxProvider::LanguageHeuristic),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn resolution_to_stored(resolution: LinkResolution) -> &'static str {
    match resolution {
        LinkResolution::AdapterLocalSymbol => "adapter-local-symbol",
        LinkResolution::AdapterFile => "adapter-file",
        LinkResolution::ExactModuleReference => "exact-module-reference",
        LinkResolution::UniqueFileLocalName => "unique-file-local-name",
        LinkResolution::UniqueQualifiedName => "unique-qualified-name",
    }
}

fn resolution_from_stored(
    resolution: &str,
) -> Result<LinkResolution, IndexPublicationRepositoryError> {
    match resolution {
        "adapter-local-symbol" => Ok(LinkResolution::AdapterLocalSymbol),
        "adapter-file" => Ok(LinkResolution::AdapterFile),
        "exact-module-reference" => Ok(LinkResolution::ExactModuleReference),
        "unique-file-local-name" => Ok(LinkResolution::UniqueFileLocalName),
        "unique-qualified-name" => Ok(LinkResolution::UniqueQualifiedName),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn unresolved_reason_to_stored(reason: UnresolvedReason) -> &'static str {
    match reason {
        UnresolvedReason::NoDeterministicMatch => "no-deterministic-match",
        UnresolvedReason::AmbiguousMatch => "ambiguous-match",
        UnresolvedReason::DynamicReference => "dynamic-reference",
        UnresolvedReason::MissingFile => "missing-file",
    }
}

fn unresolved_reason_from_stored(
    reason: &str,
) -> Result<UnresolvedReason, IndexPublicationRepositoryError> {
    match reason {
        "no-deterministic-match" => Ok(UnresolvedReason::NoDeterministicMatch),
        "ambiguous-match" => Ok(UnresolvedReason::AmbiguousMatch),
        "dynamic-reference" => Ok(UnresolvedReason::DynamicReference),
        "missing-file" => Ok(UnresolvedReason::MissingFile),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

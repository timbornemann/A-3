use a3_application::{
    ModuleCardPublicationTimeout, ModuleCardVerificationControl, VerifiedModuleCardPublisherFailure,
};
use a3_domain::{
    GraphEndpoint, LinkResolution, ModuleCardEvidenceId, ModuleCardField, ModuleClaimPolarity,
    ModuleClaimPredicate, Progress, ResolvedModuleCardEvidence, SyntaxProvider, SyntaxRelationKind,
    VerifiedClaimKind, VerifiedModuleCard, VerifiedModuleCardBatch, WorktreeId,
};
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

const MAX_PROGRESS_EVENTS: u64 = 64;
const CANCELLATION_POLL_INTERVAL: u64 = 256;

pub(crate) async fn publish_verified_module_cards(
    connection: &Connection,
    worktree_id: WorktreeId,
    batch: &VerifiedModuleCardBatch,
    timeout: ModuleCardPublicationTimeout,
    control: &dyn ModuleCardVerificationControl,
) -> Result<(), ModuleCardRepositoryError> {
    let total = publication_work_units(batch)?;
    let mut progress = PublicationProgress::new(control, timeout.duration(), total)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(ModuleCardRepositoryError::Begin)?;
    let result = async {
        progress.checkpoint()?;
        validate_publication_target(&transaction, worktree_id, batch).await?;
        progress.advance(1)?;
        for evidence in batch.evidence().evidence() {
            write_evidence(&transaction, batch, evidence).await?;
            progress.advance(1)?;
        }
        for card in batch.cards() {
            write_card(&transaction, batch, card, &mut progress).await?;
        }
        let affected = transaction
            .execute(
                "UPDATE lexical_search_projections\n\
                 SET card_count = (SELECT COUNT(*) FROM card_fts WHERE index_run_id = ?1)\n\
                 WHERE index_run_id = ?1",
                [batch.index_run_id().as_bytes().to_vec()],
            )
            .await
            .map_err(ModuleCardRepositoryError::Write)?;
        if affected != 1 {
            return Err(ModuleCardRepositoryError::Rejected);
        }
        progress.advance(1)?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        return rollback(transaction, error).await;
    }
    if let Err(error) = progress.checkpoint() {
        return rollback(transaction, error).await;
    }
    transaction
        .commit()
        .await
        .map_err(ModuleCardRepositoryError::Commit)
}

async fn validate_publication_target(
    transaction: &Transaction,
    worktree_id: WorktreeId,
    batch: &VerifiedModuleCardBatch,
) -> Result<(), ModuleCardRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT index_run_id, snapshot_id,\n\
             (SELECT COUNT(*) FROM module_cards WHERE source_index_run_id = index_runs.index_run_id),\n\
             (SELECT COUNT(*) FROM card_fts WHERE index_run_id = index_runs.index_run_id),\n\
             (SELECT COUNT(*) FROM lexical_search_projections\n\
               WHERE index_run_id = index_runs.index_run_id),\n\
             (SELECT card_count FROM lexical_search_projections\n\
               WHERE index_run_id = index_runs.index_run_id)\n\
             FROM index_runs\n\
             WHERE worktree_id = ?1 AND status = 'published'\n\
             ORDER BY run_sequence DESC LIMIT 1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleCardRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ModuleCardRepositoryError::Read)?
        .ok_or(ModuleCardRepositoryError::Rejected)?;
    let run_id: Vec<u8> = row.get(0).map_err(ModuleCardRepositoryError::Read)?;
    let snapshot_id: Vec<u8> = row.get(1).map_err(ModuleCardRepositoryError::Read)?;
    let card_count: i64 = row.get(2).map_err(ModuleCardRepositoryError::Read)?;
    let fts_count: i64 = row.get(3).map_err(ModuleCardRepositoryError::Read)?;
    let projection_count: i64 = row.get(4).map_err(ModuleCardRepositoryError::Read)?;
    let projected_cards: Option<i64> = row.get(5).map_err(ModuleCardRepositoryError::Read)?;
    if rows
        .next()
        .await
        .map_err(ModuleCardRepositoryError::Read)?
        .is_some()
        || run_id.as_slice() != batch.index_run_id().as_bytes()
        || snapshot_id.as_slice() != batch.snapshot_id().as_bytes()
        || projection_count != 1
    {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    if card_count > 0 && card_count == fts_count && projected_cards == Some(card_count) {
        return Err(ModuleCardRepositoryError::AlreadyPublished);
    }
    if card_count != 0 || fts_count != 0 || projected_cards != Some(0) {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    Ok(())
}

async fn write_evidence(
    transaction: &Transaction,
    batch: &VerifiedModuleCardBatch,
    evidence: &ResolvedModuleCardEvidence,
) -> Result<(), ModuleCardRepositoryError> {
    let row = EvidenceRow::from_evidence(batch, evidence)?;
    let affected = transaction
        .execute(
            "INSERT INTO evidence_refs (\n\
             source_index_run_id, snapshot_id, evidence_id, evidence_kind, repository_path,\n\
             content_hash, symbol_id, source_kind, source_value, target_kind, target_value,\n\
             relation_kind, provider, edge_confidence, resolution, start_byte, end_byte,\n\
             start_row, start_column, end_row, end_column\n\
             ) VALUES (\n\
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,\n\
             ?17, ?18, ?19, ?20, ?21\n\
             )",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                batch.snapshot_id().as_bytes().to_vec(),
                evidence.id().as_bytes().to_vec(),
                row.kind,
                row.repository_path,
                row.content_hash,
                row.symbol_id,
                row.source_kind,
                row.source_value,
                row.target_kind,
                row.target_value,
                row.relation_kind,
                row.provider,
                row.edge_confidence,
                row.resolution,
                row.start_byte,
                row.end_byte,
                row.start_row,
                row.start_column,
                row.end_row,
                row.end_column,
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    Ok(())
}

struct EvidenceRow {
    kind: &'static str,
    repository_path: Vec<u8>,
    content_hash: Vec<u8>,
    symbol_id: Option<Vec<u8>>,
    source_kind: Option<&'static str>,
    source_value: Option<Vec<u8>>,
    target_kind: Option<&'static str>,
    target_value: Option<Vec<u8>>,
    relation_kind: Option<&'static str>,
    provider: Option<&'static str>,
    edge_confidence: Option<i64>,
    resolution: Option<&'static str>,
    start_byte: Option<i64>,
    end_byte: Option<i64>,
    start_row: Option<i64>,
    start_column: Option<i64>,
    end_row: Option<i64>,
    end_column: Option<i64>,
}

impl EvidenceRow {
    fn from_evidence(
        batch: &VerifiedModuleCardBatch,
        evidence: &ResolvedModuleCardEvidence,
    ) -> Result<Self, ModuleCardRepositoryError> {
        match evidence {
            ResolvedModuleCardEvidence::File { id, revision } => {
                ensure_evidence_id(*id, ModuleCardEvidenceId::for_file_revision_v1(revision))?;
                Ok(Self::revision("file", revision, None))
            }
            ResolvedModuleCardEvidence::Symbol { id, symbol } => {
                ensure_evidence_id(*id, ModuleCardEvidenceId::for_symbol_v1(symbol))?;
                Ok(Self::revision(
                    "symbol",
                    symbol.revision(),
                    Some(symbol.id().as_bytes().to_vec()),
                ))
            }
            ResolvedModuleCardEvidence::GraphEdge { id, edge } => {
                ensure_evidence_id(*id, ModuleCardEvidenceId::for_graph_edge_v1(edge))?;
                if edge.snapshot_id() != batch.snapshot_id() {
                    return Err(ModuleCardRepositoryError::Rejected);
                }
                let (source_kind, source_value) = endpoint_parts(edge.source());
                let (target_kind, target_value) = endpoint_parts(edge.target());
                let range = edge.evidence().range();
                Ok(Self {
                    kind: "graph-edge",
                    repository_path: edge.evidence().revision().path().as_bytes().to_vec(),
                    content_hash: edge
                        .evidence()
                        .revision()
                        .content_hash()
                        .as_bytes()
                        .to_vec(),
                    symbol_id: None,
                    source_kind: Some(source_kind),
                    source_value: Some(source_value),
                    target_kind: Some(target_kind),
                    target_value: Some(target_value),
                    relation_kind: Some(relation_kind(edge.kind())),
                    provider: Some(provider(edge.provider())),
                    edge_confidence: Some(i64::from(edge.confidence().basis_points())),
                    resolution: Some(resolution(edge.resolution())),
                    start_byte: Some(i64::from(range.start_byte())),
                    end_byte: Some(i64::from(range.end_byte())),
                    start_row: Some(i64::from(range.start_position().row())),
                    start_column: Some(i64::from(range.start_position().column())),
                    end_row: Some(i64::from(range.end_position().row())),
                    end_column: Some(i64::from(range.end_position().column())),
                })
            }
        }
    }

    fn revision(
        kind: &'static str,
        revision: &a3_domain::FileRevision,
        symbol_id: Option<Vec<u8>>,
    ) -> Self {
        Self {
            kind,
            repository_path: revision.path().as_bytes().to_vec(),
            content_hash: revision.content_hash().as_bytes().to_vec(),
            symbol_id,
            source_kind: None,
            source_value: None,
            target_kind: None,
            target_value: None,
            relation_kind: None,
            provider: None,
            edge_confidence: None,
            resolution: None,
            start_byte: None,
            end_byte: None,
            start_row: None,
            start_column: None,
            end_row: None,
            end_column: None,
        }
    }
}

fn ensure_evidence_id(
    actual: ModuleCardEvidenceId,
    expected: ModuleCardEvidenceId,
) -> Result<(), ModuleCardRepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ModuleCardRepositoryError::Rejected)
    }
}

async fn write_card(
    transaction: &Transaction,
    batch: &VerifiedModuleCardBatch,
    card: &VerifiedModuleCard,
    progress: &mut PublicationProgress<'_>,
) -> Result<(), ModuleCardRepositoryError> {
    let proposal = card.proposal();
    let affected = transaction
        .execute(
            "INSERT INTO module_cards (\n\
             source_index_run_id, snapshot_id, card_id, module_id, card_schema_version,\n\
             mapper_profile_version, confidence, status\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'published')",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                batch.snapshot_id().as_bytes().to_vec(),
                card.id().as_bytes().to_vec(),
                proposal.module_id().as_bytes().to_vec(),
                i64::from(proposal.schema_version().get()),
                i64::from(proposal.mapper_profile_version().get()),
                i64::from(proposal.confidence().basis_points()),
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    progress.advance(1)?;
    let affected = transaction
        .execute(
            "INSERT INTO module_card_lifecycle\n\
             (source_index_run_id, card_id, status) VALUES (?1, ?2, 'published')",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                card.id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    progress.advance(1)?;
    let affected = transaction
        .execute(
            "DELETE FROM module_remap_queue WHERE module_id = ?1",
            [proposal.module_id().as_bytes().to_vec()],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected > 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    progress.advance(1)?;

    for field in proposal.fields() {
        progress.checkpoint()?;
        let kind = field_kind(field.field());
        transaction
            .execute(
                "INSERT INTO module_card_fields\n\
                 (source_index_run_id, card_id, field_kind) VALUES (?1, ?2, ?3)",
                params![
                    batch.index_run_id().as_bytes().to_vec(),
                    card.id().as_bytes().to_vec(),
                    kind,
                ],
            )
            .await
            .map_err(ModuleCardRepositoryError::Write)?;
        progress.advance(1)?;
        for (value_index, value) in field.values().iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO module_card_field_values\n\
                     (source_index_run_id, card_id, field_kind, value_index, field_value)\n\
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        batch.index_run_id().as_bytes().to_vec(),
                        card.id().as_bytes().to_vec(),
                        kind,
                        i64::try_from(value_index)
                            .map_err(|_| ModuleCardRepositoryError::ResourceLimit)?,
                        value.clone(),
                    ],
                )
                .await
                .map_err(ModuleCardRepositoryError::Write)?;
            progress.advance(1)?;
        }
        for evidence_id in field.evidence_ids() {
            transaction
                .execute(
                    "INSERT INTO module_card_field_evidence\n\
                     (source_index_run_id, card_id, field_kind, evidence_id)\n\
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        batch.index_run_id().as_bytes().to_vec(),
                        card.id().as_bytes().to_vec(),
                        kind,
                        evidence_id.as_bytes().to_vec(),
                    ],
                )
                .await
                .map_err(ModuleCardRepositoryError::Write)?;
            progress.advance(1)?;
        }
    }

    for claim in card.claims() {
        progress.checkpoint()?;
        write_claim(transaction, batch, claim).await?;
        progress.advance(1)?;
        for evidence_id in claim.proposal().evidence_ids() {
            transaction
                .execute(
                    "INSERT INTO claim_evidence\n\
                     (source_index_run_id, claim_id, evidence_id) VALUES (?1, ?2, ?3)",
                    params![
                        batch.index_run_id().as_bytes().to_vec(),
                        claim.proposal().id().as_bytes().to_vec(),
                        evidence_id.as_bytes().to_vec(),
                    ],
                )
                .await
                .map_err(ModuleCardRepositoryError::Write)?;
            progress.advance(1)?;
        }
        if write_structured_predicate(transaction, batch, claim.proposal()).await? {
            progress.advance(1)?;
        }
    }

    let (title, purpose, body) = searchable_card_text(card);
    transaction
        .execute(
            "INSERT INTO card_fts (index_run_id, card_id, title, purpose, body)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                card.id().as_bytes().to_vec(),
                title,
                purpose,
                body,
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    progress.advance(1)
}

async fn write_claim(
    transaction: &Transaction,
    batch: &VerifiedModuleCardBatch,
    claim: &a3_domain::VerifiedModuleClaim,
) -> Result<(), ModuleCardRepositoryError> {
    let proposal = claim.proposal();
    let (predicate_kind, statement) = match proposal.predicate() {
        ModuleClaimPredicate::Path(_) => ("path", None),
        ModuleClaimPredicate::Symbol(_) => ("symbol", None),
        ModuleClaimPredicate::Relation { .. } => ("relation", None),
        ModuleClaimPredicate::Observed(statement) => {
            ("observed", Some(statement.as_str().to_owned()))
        }
        ModuleClaimPredicate::ArchitecturalIntent(statement) => {
            ("architectural-intent", Some(statement.as_str().to_owned()))
        }
    };
    let affected = transaction
        .execute(
            "INSERT INTO claims (\n\
             source_index_run_id, snapshot_id, claim_id, card_id, field_kind, value_index,\n\
             polarity, predicate_kind, statement, claim_kind, status, confidence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active', ?11)",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                batch.snapshot_id().as_bytes().to_vec(),
                proposal.id().as_bytes().to_vec(),
                proposal.card_id().as_bytes().to_vec(),
                field_kind(proposal.field()),
                i64::from(proposal.value_index()),
                polarity(proposal.polarity()),
                predicate_kind,
                statement,
                claim_kind(claim.kind()),
                i64::from(claim.confidence().basis_points()),
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    let affected = transaction
        .execute(
            "INSERT INTO claim_lifecycle\n\
             (source_index_run_id, claim_id, status) VALUES (?1, ?2, 'active')",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                proposal.id().as_bytes().to_vec(),
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    Ok(())
}

async fn write_structured_predicate(
    transaction: &Transaction,
    batch: &VerifiedModuleCardBatch,
    claim: &a3_domain::ModuleClaimProposal,
) -> Result<bool, ModuleCardRepositoryError> {
    let row = match claim.predicate() {
        ModuleClaimPredicate::Path(path) => StructuredPredicateRow {
            kind: "path",
            path: Some(path.as_bytes().to_vec()),
            symbol_id: None,
            source_kind: None,
            source_value: None,
            target_kind: None,
            target_value: None,
            relation_kind: None,
        },
        ModuleClaimPredicate::Symbol(symbol_id) => StructuredPredicateRow {
            kind: "symbol",
            path: None,
            symbol_id: Some(symbol_id.as_bytes().to_vec()),
            source_kind: None,
            source_value: None,
            target_kind: None,
            target_value: None,
            relation_kind: None,
        },
        ModuleClaimPredicate::Relation {
            source,
            target,
            kind,
        } => {
            let (source_kind, source_value) = endpoint_parts(source);
            let (target_kind, target_value) = endpoint_parts(target);
            StructuredPredicateRow {
                kind: "relation",
                path: None,
                symbol_id: None,
                source_kind: Some(source_kind),
                source_value: Some(source_value),
                target_kind: Some(target_kind),
                target_value: Some(target_value),
                relation_kind: Some(relation_kind(*kind)),
            }
        }
        ModuleClaimPredicate::Observed(_) | ModuleClaimPredicate::ArchitecturalIntent(_) => {
            return Ok(false);
        }
    };
    let affected = transaction
        .execute(
            "INSERT INTO claim_relations (\n\
             source_index_run_id, claim_id, predicate_kind, predicate_path, predicate_symbol_id,\n\
             source_kind, source_value, target_kind, target_value, relation_kind\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                batch.index_run_id().as_bytes().to_vec(),
                claim.id().as_bytes().to_vec(),
                row.kind,
                row.path,
                row.symbol_id,
                row.source_kind,
                row.source_value,
                row.target_kind,
                row.target_value,
                row.relation_kind,
            ],
        )
        .await
        .map_err(ModuleCardRepositoryError::Write)?;
    if affected != 1 {
        return Err(ModuleCardRepositoryError::Rejected);
    }
    Ok(true)
}

struct StructuredPredicateRow {
    kind: &'static str,
    path: Option<Vec<u8>>,
    symbol_id: Option<Vec<u8>>,
    source_kind: Option<&'static str>,
    source_value: Option<Vec<u8>>,
    target_kind: Option<&'static str>,
    target_value: Option<Vec<u8>>,
    relation_kind: Option<&'static str>,
}

fn searchable_card_text(card: &VerifiedModuleCard) -> (String, String, String) {
    let mut title = String::new();
    let mut purpose = String::new();
    let mut body = String::new();
    for field in card.proposal().fields() {
        let joined = field.values().join("\n");
        match field.field() {
            ModuleCardField::Title => title = joined.clone(),
            ModuleCardField::Purpose => purpose = joined.clone(),
            _ => {}
        }
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(field_kind(field.field()));
        body.push_str(": ");
        body.push_str(&joined);
    }
    (title, purpose, body)
}

fn publication_work_units(
    batch: &VerifiedModuleCardBatch,
) -> Result<u64, ModuleCardRepositoryError> {
    let mut total = 2_u64;
    add_len(&mut total, batch.evidence().evidence().len())?;
    for card in batch.cards() {
        total = total
            .checked_add(4)
            .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
        for field in card.proposal().fields() {
            total = total
                .checked_add(1)
                .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
            add_len(&mut total, field.values().len())?;
            add_len(&mut total, field.evidence_ids().len())?;
        }
        for claim in card.claims() {
            total = total
                .checked_add(1)
                .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
            add_len(&mut total, claim.proposal().evidence_ids().len())?;
            if matches!(
                claim.proposal().predicate(),
                ModuleClaimPredicate::Path(_)
                    | ModuleClaimPredicate::Symbol(_)
                    | ModuleClaimPredicate::Relation { .. }
            ) {
                total = total
                    .checked_add(1)
                    .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
            }
        }
    }
    Ok(total)
}

fn add_len(total: &mut u64, length: usize) -> Result<(), ModuleCardRepositoryError> {
    *total = total
        .checked_add(u64::try_from(length).map_err(|_| ModuleCardRepositoryError::ResourceLimit)?)
        .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
    Ok(())
}

fn field_kind(field: ModuleCardField) -> &'static str {
    match field {
        ModuleCardField::Title => "title",
        ModuleCardField::Paths => "paths",
        ModuleCardField::Purpose => "purpose",
        ModuleCardField::Responsibilities => "responsibilities",
        ModuleCardField::PublicSurface => "public-surface",
        ModuleCardField::Entrypoints => "entrypoints",
        ModuleCardField::Dependencies => "dependencies",
        ModuleCardField::DataFlows => "data-flows",
        ModuleCardField::Invariants => "invariants",
        ModuleCardField::Tests => "tests",
        ModuleCardField::Risks => "risks",
        ModuleCardField::OpenQuestions => "open-questions",
    }
}

fn endpoint_parts(endpoint: &GraphEndpoint) -> (&'static str, Vec<u8>) {
    match endpoint {
        GraphEndpoint::File(path) => ("file", path.as_bytes().to_vec()),
        GraphEndpoint::Symbol(symbol_id) => ("symbol", symbol_id.as_bytes().to_vec()),
    }
}

fn polarity(value: ModuleClaimPolarity) -> &'static str {
    match value {
        ModuleClaimPolarity::Affirms => "affirms",
        ModuleClaimPolarity::Denies => "denies",
    }
}

fn claim_kind(value: VerifiedClaimKind) -> &'static str {
    match value {
        VerifiedClaimKind::Fact => "fact",
        VerifiedClaimKind::Observation => "observation",
        VerifiedClaimKind::Hypothesis => "hypothesis",
    }
}

fn relation_kind(value: SyntaxRelationKind) -> &'static str {
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

fn provider(value: SyntaxProvider) -> &'static str {
    match value {
        SyntaxProvider::TreeSitter => "tree-sitter",
        SyntaxProvider::Manifest => "manifest",
        SyntaxProvider::LanguageHeuristic => "language-heuristic",
    }
}

fn resolution(value: LinkResolution) -> &'static str {
    match value {
        LinkResolution::AdapterLocalSymbol => "adapter-local-symbol",
        LinkResolution::AdapterFile => "adapter-file",
        LinkResolution::ExactModuleReference => "exact-module-reference",
        LinkResolution::UniqueFileLocalName => "unique-file-local-name",
        LinkResolution::UniqueQualifiedName => "unique-qualified-name",
    }
}

struct PublicationProgress<'a> {
    control: &'a dyn ModuleCardVerificationControl,
    started: Instant,
    timeout: Duration,
    total: u64,
    completed: u64,
    next_report: u64,
    report_interval: u64,
    work_since_checkpoint: u64,
}

impl<'a> PublicationProgress<'a> {
    fn new(
        control: &'a dyn ModuleCardVerificationControl,
        timeout: Duration,
        total: u64,
    ) -> Result<Self, ModuleCardRepositoryError> {
        if total == 0 {
            return Err(ModuleCardRepositoryError::ResourceLimit);
        }
        let report_interval = total.div_ceil(MAX_PROGRESS_EVENTS.saturating_sub(1)).max(1);
        let progress = Self {
            control,
            started: Instant::now(),
            timeout,
            total,
            completed: 0,
            next_report: report_interval,
            report_interval,
            work_since_checkpoint: 0,
        };
        progress.checkpoint()?;
        progress.report(0)?;
        Ok(progress)
    }

    fn advance(&mut self, units: u64) -> Result<(), ModuleCardRepositoryError> {
        self.completed = self
            .completed
            .checked_add(units)
            .filter(|completed| *completed <= self.total)
            .ok_or(ModuleCardRepositoryError::ResourceLimit)?;
        self.work_since_checkpoint = self.work_since_checkpoint.saturating_add(units);
        if self.work_since_checkpoint >= CANCELLATION_POLL_INTERVAL {
            self.checkpoint()?;
            self.work_since_checkpoint = 0;
        }
        if self.completed >= self.next_report || self.completed == self.total {
            self.checkpoint()?;
            self.report(self.completed)?;
            while self.next_report <= self.completed {
                self.next_report = self.next_report.saturating_add(self.report_interval);
            }
        }
        Ok(())
    }

    fn checkpoint(&self) -> Result<(), ModuleCardRepositoryError> {
        if self.control.is_cancelled() {
            Err(ModuleCardRepositoryError::Cancelled)
        } else if self.started.elapsed() >= self.timeout {
            Err(ModuleCardRepositoryError::TimedOut)
        } else {
            Ok(())
        }
    }

    fn report(&self, completed: u64) -> Result<(), ModuleCardRepositoryError> {
        let progress = Progress::determinate(completed, self.total)
            .map_err(|_| ModuleCardRepositoryError::ResourceLimit)?;
        self.control
            .report_progress(progress)
            .map_err(|_| ModuleCardRepositoryError::ProgressUnavailable)
    }
}

async fn rollback(
    transaction: Transaction,
    error: ModuleCardRepositoryError,
) -> Result<(), ModuleCardRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(ModuleCardRepositoryError::Rollback)?;
    Err(error)
}

#[derive(Debug, Clone, Copy)]
enum ModuleCardRepositoryErrorClass {
    AlreadyPublished,
    Rejected,
    Storage,
    Cancelled,
    TimedOut,
    ProgressUnavailable,
    ResourceLimit,
}

#[derive(Debug)]
pub(crate) enum ModuleCardRepositoryError {
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Commit(libsql::Error),
    Rollback(libsql::Error),
    Rejected,
    AlreadyPublished,
    Cancelled,
    TimedOut,
    ProgressUnavailable,
    ResourceLimit,
}

impl ModuleCardRepositoryError {
    fn class(&self) -> ModuleCardRepositoryErrorClass {
        match self {
            Self::AlreadyPublished => ModuleCardRepositoryErrorClass::AlreadyPublished,
            Self::Rejected => ModuleCardRepositoryErrorClass::Rejected,
            Self::Cancelled => ModuleCardRepositoryErrorClass::Cancelled,
            Self::TimedOut => ModuleCardRepositoryErrorClass::TimedOut,
            Self::ProgressUnavailable => ModuleCardRepositoryErrorClass::ProgressUnavailable,
            Self::ResourceLimit => ModuleCardRepositoryErrorClass::ResourceLimit,
            Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Commit(_)
            | Self::Rollback(_) => ModuleCardRepositoryErrorClass::Storage,
        }
    }

    pub(crate) fn classify(&self) -> VerifiedModuleCardPublisherFailure {
        match self.class() {
            ModuleCardRepositoryErrorClass::AlreadyPublished => {
                VerifiedModuleCardPublisherFailure::AlreadyPublished
            }
            ModuleCardRepositoryErrorClass::Rejected
            | ModuleCardRepositoryErrorClass::ResourceLimit => {
                VerifiedModuleCardPublisherFailure::Rejected
            }
            ModuleCardRepositoryErrorClass::Cancelled => {
                VerifiedModuleCardPublisherFailure::Cancelled
            }
            ModuleCardRepositoryErrorClass::TimedOut => {
                VerifiedModuleCardPublisherFailure::TimedOut
            }
            ModuleCardRepositoryErrorClass::ProgressUnavailable => {
                VerifiedModuleCardPublisherFailure::ProgressUnavailable
            }
            ModuleCardRepositoryErrorClass::Storage => VerifiedModuleCardPublisherFailure::Storage,
        }
    }
}

impl fmt::Display for ModuleCardRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Begin(_) => "could not begin verified Module Card publication",
            Self::Read(_) => "could not read verified Module Card publication state",
            Self::Write(_) => "could not write verified Module Card publication",
            Self::Commit(_) => "could not commit verified Module Card publication",
            Self::Rollback(_) => "could not roll back verified Module Card publication",
            Self::Rejected => "verified Module Card publication target was rejected",
            Self::AlreadyPublished => "verified Module Cards are already published",
            Self::Cancelled => "verified Module Card publication was cancelled",
            Self::TimedOut => "verified Module Card publication timed out",
            Self::ProgressUnavailable => "verified Module Card progress is unavailable",
            Self::ResourceLimit => "verified Module Card publication exceeded a resource limit",
        })
    }
}

impl Error for ModuleCardRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Commit(source) => Some(source),
            Self::Rollback(source) => Some(source),
            Self::Rejected
            | Self::AlreadyPublished
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable
            | Self::ResourceLimit => None,
        }
    }
}

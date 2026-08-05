use crate::index_codec::{blob, integer, sequence, text, write_batches};
use crate::index_publication::{IndexPublicationRepositoryError, MutationProgress, read_stable_id};
use a3_domain::{
    ContentHash, EvidenceRef, FileRevision, IndexLanguage, IndexRunId, ModuleId, ModuleKind,
    ModuleMembership, ModuleMembershipEvidence, ModuleMembershipKind, ModulePolicyVersion,
    ModuleProjection, ModuleRoot, ModuleSymbolSet, RepositoryCard, RepositoryModule,
    RepositoryPath, SnapshotId, SourcePosition, SourceRange, SymbolId,
};
use libsql::{Transaction, Value, params};
use std::collections::BTreeMap;

pub(crate) fn work_units(
    projection: &ModuleProjection,
) -> Result<u64, IndexPublicationRepositoryError> {
    let mut total = 1_u64;
    total = add_len(total, projection.modules().len())?;
    total = add_len(total, projection.memberships().len())?;
    total = add_len(
        total,
        projection.repository_card().entrypoints().symbols().len(),
    )?;
    for module in projection.modules() {
        total = add_len(total, module.manifests().len())?;
        total = add_len(total, module.central_symbols().symbols().len())?;
        total = add_len(total, module.entrypoints().symbols().len())?;
        total = add_len(total, module.tests().symbols().len())?;
    }
    for membership in projection.memberships() {
        total = add_len(total, membership.evidence().relationships().len())?;
    }
    Ok(total)
}

pub(crate) async fn write_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    projection: &ModuleProjection,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    let card = projection.repository_card();
    let module_count = i64::try_from(projection.modules().len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    let membership_count = i64::try_from(projection.memberships().len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    let affected = transaction
        .execute(
            "INSERT INTO module_projections (\n\
             index_run_id, policy_version, file_count, symbol_count, module_count, membership_count,\n\
             language_mask, repository_entrypoints_truncated\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run_id.as_bytes().to_vec(),
                i64::from(projection.policy_version().get()),
                i64::from(card.file_count()),
                i64::from(card.symbol_count()),
                module_count,
                membership_count,
                language_mask(card.languages()),
                bool_integer(card.entrypoints().is_truncated()),
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    if affected != 1 {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    progress.advance(1)?;

    write_batches(
        transaction,
        projection.modules(),
        8,
        "INSERT INTO modules (index_run_id, module_id, kind, root_kind, root_path,\n\
         central_symbols_truncated, entrypoints_truncated, tests_truncated) VALUES ",
        progress,
        |module, _| {
            let (root_kind, root_path) = root_values(module.root());
            Ok(vec![
                blob(run_id.as_bytes()),
                blob(module.id().as_bytes()),
                text(module_kind_to_stored(module.kind())),
                text(root_kind),
                root_path,
                integer(bool_integer(module.central_symbols().is_truncated())),
                integer(bool_integer(module.entrypoints().is_truncated())),
                integer(bool_integer(module.tests().is_truncated())),
            ])
        },
    )
    .await?;

    for (index, module) in projection.modules().iter().enumerate() {
        checkpoint_loop(index, progress)?;
        write_batches(
            transaction,
            module.manifests(),
            5,
            "INSERT INTO module_manifests (index_run_id, module_id, manifest_order,\n\
             repository_path, content_hash) VALUES ",
            progress,
            |manifest, index| {
                Ok(vec![
                    blob(run_id.as_bytes()),
                    blob(module.id().as_bytes()),
                    integer(sequence(index)?),
                    blob(manifest.path().as_bytes()),
                    blob(manifest.content_hash().as_bytes()),
                ])
            },
        )
        .await?;
    }

    write_batches(
        transaction,
        projection.memberships(),
        8,
        "INSERT INTO module_members (index_run_id, module_id, symbol_id, membership_kind,\n\
         member_path, member_hash, manifest_path, manifest_hash) VALUES ",
        progress,
        |membership, _| {
            let evidence = membership.evidence();
            let (manifest_path, manifest_hash) =
                evidence
                    .manifest_revision()
                    .map_or((Value::Null, Value::Null), |manifest| {
                        (
                            blob(manifest.path().as_bytes()),
                            blob(manifest.content_hash().as_bytes()),
                        )
                    });
            Ok(vec![
                blob(run_id.as_bytes()),
                blob(membership.module_id().as_bytes()),
                blob(membership.symbol_id().as_bytes()),
                text(membership_kind_to_stored(evidence.kind())),
                blob(evidence.member_revision().path().as_bytes()),
                blob(evidence.member_revision().content_hash().as_bytes()),
                manifest_path,
                manifest_hash,
            ])
        },
    )
    .await?;

    for (index, membership) in projection.memberships().iter().enumerate() {
        checkpoint_loop(index, progress)?;
        write_batches(
            transaction,
            membership.evidence().relationships(),
            12,
            "INSERT INTO module_membership_evidence (index_run_id, module_id, symbol_id,\n\
             evidence_order, repository_path, content_hash, start_byte, end_byte, start_row,\n\
             start_column, end_row, end_column) VALUES ",
            progress,
            |evidence, index| {
                let range = evidence.range();
                Ok(vec![
                    blob(run_id.as_bytes()),
                    blob(membership.module_id().as_bytes()),
                    blob(membership.symbol_id().as_bytes()),
                    integer(sequence(index)?),
                    blob(evidence.revision().path().as_bytes()),
                    blob(evidence.revision().content_hash().as_bytes()),
                    integer(i64::from(range.start_byte())),
                    integer(i64::from(range.end_byte())),
                    integer(i64::from(range.start_position().row())),
                    integer(i64::from(range.start_position().column())),
                    integer(i64::from(range.end_position().row())),
                    integer(i64::from(range.end_position().column())),
                ])
            },
        )
        .await?;
    }

    write_module_symbols(
        transaction,
        run_id,
        projection,
        "module_central_symbols",
        ModuleFeature::Central,
        progress,
    )
    .await?;
    write_module_symbols(
        transaction,
        run_id,
        projection,
        "module_entrypoints",
        ModuleFeature::Entrypoint,
        progress,
    )
    .await?;
    write_module_symbols(
        transaction,
        run_id,
        projection,
        "module_tests",
        ModuleFeature::Test,
        progress,
    )
    .await?;
    write_batches(
        transaction,
        card.entrypoints().symbols(),
        3,
        "INSERT INTO repository_card_entrypoints (index_run_id, rank_order, symbol_id) VALUES ",
        progress,
        |symbol, index| {
            Ok(vec![
                blob(run_id.as_bytes()),
                integer(sequence(index)?),
                blob(symbol.as_bytes()),
            ])
        },
    )
    .await
}

async fn write_module_symbols(
    transaction: &Transaction,
    run_id: IndexRunId,
    projection: &ModuleProjection,
    table: &str,
    feature: ModuleFeature,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    let prefix =
        format!("INSERT INTO {table} (index_run_id, module_id, rank_order, symbol_id) VALUES ");
    for (index, module) in projection.modules().iter().enumerate() {
        checkpoint_loop(index, progress)?;
        write_batches(
            transaction,
            feature.select(module).symbols(),
            4,
            &prefix,
            progress,
            |symbol, index| {
                Ok(vec![
                    blob(run_id.as_bytes()),
                    blob(module.id().as_bytes()),
                    integer(sequence(index)?),
                    blob(symbol.as_bytes()),
                ])
            },
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn read_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    snapshot_id: SnapshotId,
    progress: &mut MutationProgress<'_>,
) -> Result<ModuleProjection, IndexPublicationRepositoryError> {
    let marker = read_marker(transaction, run_id, progress).await?;
    let stored_modules = read_modules(transaction, run_id, progress).await?;
    let manifests = read_manifests(transaction, run_id, progress).await?;
    let central =
        read_module_symbols(transaction, run_id, "module_central_symbols", progress).await?;
    let entrypoints =
        read_module_symbols(transaction, run_id, "module_entrypoints", progress).await?;
    let tests = read_module_symbols(transaction, run_id, "module_tests", progress).await?;
    let mut modules = Vec::with_capacity(stored_modules.len());
    for stored in stored_modules {
        modules.push(
            RepositoryModule::new(
                stored.id,
                stored.kind,
                stored.root,
                manifests.get(&stored.id).cloned().unwrap_or_default(),
                ModuleSymbolSet::new(
                    central.get(&stored.id).cloned().unwrap_or_default(),
                    stored.central_truncated,
                )
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
                ModuleSymbolSet::new(
                    entrypoints.get(&stored.id).cloned().unwrap_or_default(),
                    stored.entrypoints_truncated,
                )
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
                ModuleSymbolSet::new(
                    tests.get(&stored.id).cloned().unwrap_or_default(),
                    stored.tests_truncated,
                )
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
            )
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        );
    }
    let relationships = read_membership_evidence(transaction, run_id, progress).await?;
    let memberships = read_memberships(transaction, run_id, &relationships, progress).await?;
    let repository_entrypoints = read_repository_entrypoints(transaction, run_id, progress).await?;
    if marker.module_count != modules.len() || marker.membership_count != memberships.len() {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    let packages = modules
        .iter()
        .filter(|module| module.kind().is_primary())
        .map(RepositoryModule::id)
        .collect();
    let card = RepositoryCard::new(
        snapshot_id,
        marker.policy_version,
        packages,
        marker.languages,
        ModuleSymbolSet::new(
            repository_entrypoints,
            marker.repository_entrypoints_truncated,
        )
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        marker.file_count,
        marker.symbol_count,
    )
    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    ModuleProjection::new(
        snapshot_id,
        marker.policy_version,
        modules,
        memberships,
        card,
    )
    .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

async fn read_marker(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<StoredMarker, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT policy_version, file_count, symbol_count, module_count, membership_count,\n\
             language_mask, repository_entrypoints_truncated\n\
             FROM module_projections WHERE index_run_id = ?1",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .ok_or(IndexPublicationRepositoryError::InvalidStoredData)?;
    let marker = StoredMarker {
        policy_version: ModulePolicyVersion::new(read_u32(&row, 0)?)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        file_count: read_u32(&row, 1)?,
        symbol_count: read_u32(&row, 2)?,
        module_count: read_usize(&row, 3)?,
        membership_count: read_usize(&row, 4)?,
        languages: languages_from_mask(read_i64(&row, 5)?)?,
        repository_entrypoints_truncated: read_bool(&row, 6)?,
    };
    if rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
        .is_some()
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    progress.advance(1)?;
    Ok(marker)
}

async fn read_modules(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<StoredModule>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT module_id, kind, root_kind, root_path, central_symbols_truncated,\n\
             entrypoints_truncated, tests_truncated FROM modules\n\
             WHERE index_run_id = ?1 ORDER BY module_id",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut modules = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let kind: String = row.get(1).map_err(IndexPublicationRepositoryError::Read)?;
        let root_kind: String = row.get(2).map_err(IndexPublicationRepositoryError::Read)?;
        let root_path: Option<Vec<u8>> =
            row.get(3).map_err(IndexPublicationRepositoryError::Read)?;
        modules.push(StoredModule {
            id: ModuleId::from_bytes(read_stable_id(&row, 0)?),
            kind: module_kind_from_stored(&kind)?,
            root: root_from_stored(&root_kind, root_path)?,
            central_truncated: read_bool(&row, 4)?,
            entrypoints_truncated: read_bool(&row, 5)?,
            tests_truncated: read_bool(&row, 6)?,
        });
        progress.advance(1)?;
    }
    Ok(modules)
}

async fn read_manifests(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<BTreeMap<ModuleId, Vec<FileRevision>>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT module_id, manifest_order, repository_path, content_hash\n\
             FROM module_manifests WHERE index_run_id = ?1\n\
             ORDER BY module_id, manifest_order",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut result = BTreeMap::<ModuleId, Vec<FileRevision>>::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let id = ModuleId::from_bytes(read_stable_id(&row, 0)?);
        let values = result.entry(id).or_default();
        validate_sequence(&row, 1, values.len())?;
        values.push(read_revision(&row, 2, 3)?);
        progress.advance(1)?;
    }
    Ok(result)
}

async fn read_module_symbols(
    transaction: &Transaction,
    run_id: IndexRunId,
    table: &str,
    progress: &mut MutationProgress<'_>,
) -> Result<BTreeMap<ModuleId, Vec<SymbolId>>, IndexPublicationRepositoryError> {
    let sql = format!(
        "SELECT module_id, rank_order, symbol_id FROM {table}\n\
         WHERE index_run_id = ?1 ORDER BY module_id, rank_order"
    );
    let mut rows = transaction
        .query(&sql, [run_id.as_bytes().to_vec()])
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut result = BTreeMap::<ModuleId, Vec<SymbolId>>::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let id = ModuleId::from_bytes(read_stable_id(&row, 0)?);
        let values = result.entry(id).or_default();
        validate_sequence(&row, 1, values.len())?;
        values.push(SymbolId::from_bytes(read_stable_id(&row, 2)?));
        progress.advance(1)?;
    }
    Ok(result)
}

async fn read_membership_evidence(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<BTreeMap<(ModuleId, SymbolId), Vec<EvidenceRef>>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT module_id, symbol_id, evidence_order, repository_path, content_hash,\n\
             start_byte, end_byte, start_row, start_column, end_row, end_column\n\
             FROM module_membership_evidence WHERE index_run_id = ?1\n\
             ORDER BY module_id, symbol_id, evidence_order",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut result = BTreeMap::<(ModuleId, SymbolId), Vec<EvidenceRef>>::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let key = (
            ModuleId::from_bytes(read_stable_id(&row, 0)?),
            SymbolId::from_bytes(read_stable_id(&row, 1)?),
        );
        let values = result.entry(key).or_default();
        validate_sequence(&row, 2, values.len())?;
        values.push(EvidenceRef::new(
            read_revision(&row, 3, 4)?,
            read_range(&row, 5)?,
        ));
        progress.advance(1)?;
    }
    Ok(result)
}

async fn read_memberships(
    transaction: &Transaction,
    run_id: IndexRunId,
    relationships: &BTreeMap<(ModuleId, SymbolId), Vec<EvidenceRef>>,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<ModuleMembership>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT module_id, symbol_id, membership_kind, member_path, member_hash,\n\
             manifest_path, manifest_hash FROM module_members\n\
             WHERE index_run_id = ?1 ORDER BY symbol_id, module_id",
            [run_id.as_bytes().to_vec()],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Read)?;
    let mut memberships = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(IndexPublicationRepositoryError::Read)?
    {
        let module_id = ModuleId::from_bytes(read_stable_id(&row, 0)?);
        let symbol_id = SymbolId::from_bytes(read_stable_id(&row, 1)?);
        let kind: String = row.get(2).map_err(IndexPublicationRepositoryError::Read)?;
        let member = read_revision(&row, 3, 4)?;
        let manifest_path: Option<Vec<u8>> =
            row.get(5).map_err(IndexPublicationRepositoryError::Read)?;
        let manifest_hash: Option<Vec<u8>> =
            row.get(6).map_err(IndexPublicationRepositoryError::Read)?;
        let evidence = match membership_kind_from_stored(&kind)? {
            ModuleMembershipKind::Path if manifest_path.is_none() && manifest_hash.is_none() => {
                ModuleMembershipEvidence::path(member)
            }
            ModuleMembershipKind::Manifest => ModuleMembershipEvidence::manifest(
                member,
                optional_revision(manifest_path, manifest_hash)?,
            ),
            ModuleMembershipKind::GraphCommunity
                if manifest_path.is_none() && manifest_hash.is_none() =>
            {
                ModuleMembershipEvidence::graph(
                    member,
                    relationships
                        .get(&(module_id, symbol_id))
                        .cloned()
                        .unwrap_or_default(),
                )
                .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?
            }
            _ => return Err(IndexPublicationRepositoryError::InvalidStoredData),
        };
        memberships.push(ModuleMembership::new(module_id, symbol_id, evidence));
        progress.advance(1)?;
    }
    let membership_pairs = memberships
        .iter()
        .map(|membership| (membership.module_id(), membership.symbol_id()))
        .collect::<std::collections::BTreeSet<_>>();
    if relationships
        .keys()
        .any(|key| !membership_pairs.contains(key))
    {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(memberships)
}

async fn read_repository_entrypoints(
    transaction: &Transaction,
    run_id: IndexRunId,
    progress: &mut MutationProgress<'_>,
) -> Result<Vec<SymbolId>, IndexPublicationRepositoryError> {
    let mut rows = transaction
        .query(
            "SELECT rank_order, symbol_id FROM repository_card_entrypoints\n\
             WHERE index_run_id = ?1 ORDER BY rank_order",
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
        validate_sequence(&row, 0, symbols.len())?;
        symbols.push(SymbolId::from_bytes(read_stable_id(&row, 1)?));
        progress.advance(1)?;
    }
    Ok(symbols)
}

fn add_len(total: u64, length: usize) -> Result<u64, IndexPublicationRepositoryError> {
    total
        .checked_add(
            u64::try_from(length).map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?,
        )
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)
}

fn checkpoint_loop(
    index: usize,
    progress: &MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    if index.is_multiple_of(1_024) {
        progress.checkpoint()?;
    }
    Ok(())
}

const fn bool_integer(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn root_values(root: Option<&ModuleRoot>) -> (&'static str, Value) {
    match root {
        Some(ModuleRoot::Repository) => ("repository", Value::Null),
        Some(ModuleRoot::Directory(path)) => ("directory", blob(path.as_bytes())),
        None => ("none", Value::Null),
    }
}

fn root_from_stored(
    kind: &str,
    path: Option<Vec<u8>>,
) -> Result<Option<ModuleRoot>, IndexPublicationRepositoryError> {
    match (kind, path) {
        ("repository", None) => Ok(Some(ModuleRoot::Repository)),
        ("directory", Some(path)) => RepositoryPath::try_from_bytes(path)
            .map(ModuleRoot::Directory)
            .map(Some)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData),
        ("none", None) => Ok(None),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

const fn module_kind_to_stored(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::ManifestBoundary => "manifest",
        ModuleKind::PathBoundary => "path",
        ModuleKind::GraphCommunity => "graph-community",
    }
}

fn module_kind_from_stored(value: &str) -> Result<ModuleKind, IndexPublicationRepositoryError> {
    match value {
        "manifest" => Ok(ModuleKind::ManifestBoundary),
        "path" => Ok(ModuleKind::PathBoundary),
        "graph-community" => Ok(ModuleKind::GraphCommunity),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

const fn membership_kind_to_stored(kind: ModuleMembershipKind) -> &'static str {
    match kind {
        ModuleMembershipKind::Manifest => "manifest",
        ModuleMembershipKind::Path => "path",
        ModuleMembershipKind::GraphCommunity => "graph-community",
    }
}

fn membership_kind_from_stored(
    value: &str,
) -> Result<ModuleMembershipKind, IndexPublicationRepositoryError> {
    match value {
        "manifest" => Ok(ModuleMembershipKind::Manifest),
        "path" => Ok(ModuleMembershipKind::Path),
        "graph-community" => Ok(ModuleMembershipKind::GraphCommunity),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn language_mask(languages: &[IndexLanguage]) -> i64 {
    languages.iter().fold(0_i64, |mask, language| {
        mask | match language {
            IndexLanguage::Generic => 1,
            IndexLanguage::Rust => 2,
            IndexLanguage::TypeScriptJavaScript => 4,
            IndexLanguage::Python => 8,
        }
    })
}

fn languages_from_mask(mask: i64) -> Result<Vec<IndexLanguage>, IndexPublicationRepositoryError> {
    if !(0..=15).contains(&mask) {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok([
        (1, IndexLanguage::Generic),
        (2, IndexLanguage::Rust),
        (4, IndexLanguage::TypeScriptJavaScript),
        (8, IndexLanguage::Python),
    ]
    .into_iter()
    .filter_map(|(bit, language)| (mask & bit != 0).then_some(language))
    .collect())
}

fn read_revision(
    row: &libsql::Row,
    path_index: i32,
    hash_index: i32,
) -> Result<FileRevision, IndexPublicationRepositoryError> {
    let path: Vec<u8> = row
        .get(path_index)
        .map_err(IndexPublicationRepositoryError::Read)?;
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        ContentHash::from_bytes(read_stable_id(row, hash_index)?),
    ))
}

fn optional_revision(
    path: Option<Vec<u8>>,
    hash: Option<Vec<u8>>,
) -> Result<FileRevision, IndexPublicationRepositoryError> {
    let (Some(path), Some(hash)) = (path, hash) else {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    };
    let hash: [u8; 32] = hash
        .try_into()
        .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?;
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path)
            .map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)?,
        ContentHash::from_bytes(hash),
    ))
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

fn read_i64(row: &libsql::Row, index: i32) -> Result<i64, IndexPublicationRepositoryError> {
    row.get(index)
        .map_err(IndexPublicationRepositoryError::Read)
}

fn read_u32(row: &libsql::Row, index: i32) -> Result<u32, IndexPublicationRepositoryError> {
    let value = read_i64(row, index)?;
    u32::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_usize(row: &libsql::Row, index: i32) -> Result<usize, IndexPublicationRepositoryError> {
    let value = read_i64(row, index)?;
    usize::try_from(value).map_err(|_| IndexPublicationRepositoryError::InvalidStoredData)
}

fn read_bool(row: &libsql::Row, index: i32) -> Result<bool, IndexPublicationRepositoryError> {
    match read_i64(row, index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IndexPublicationRepositoryError::InvalidStoredData),
    }
}

fn validate_sequence(
    row: &libsql::Row,
    index: i32,
    current_len: usize,
) -> Result<(), IndexPublicationRepositoryError> {
    if read_usize(row, index)? != current_len.saturating_add(1) {
        return Err(IndexPublicationRepositoryError::InvalidStoredData);
    }
    Ok(())
}

struct StoredMarker {
    policy_version: ModulePolicyVersion,
    file_count: u32,
    symbol_count: u32,
    module_count: usize,
    membership_count: usize,
    languages: Vec<IndexLanguage>,
    repository_entrypoints_truncated: bool,
}

struct StoredModule {
    id: ModuleId,
    kind: ModuleKind,
    root: Option<ModuleRoot>,
    central_truncated: bool,
    entrypoints_truncated: bool,
    tests_truncated: bool,
}

#[derive(Clone, Copy)]
enum ModuleFeature {
    Central,
    Entrypoint,
    Test,
}

impl ModuleFeature {
    fn select(self, module: &RepositoryModule) -> &ModuleSymbolSet {
        match self {
            Self::Central => module.central_symbols(),
            Self::Entrypoint => module.entrypoints(),
            Self::Test => module.tests(),
        }
    }
}

use crate::exact_search_projection::ExactSearchProjection;
use crate::index_publication::{IndexPublicationRepositoryError, MutationProgress};
use a3_domain::{IndexPublication, IndexRunId, RepositoryPath, SymbolId};
use libsql::{Transaction, Value, params, params_from_iter};
use std::collections::BTreeMap;

const MAX_BATCH_PARAMETERS: usize = 30_000;
const MAX_BATCH_ROWS: usize = 1_024;
const PROJECTION_VERSION: i64 = 1;

pub(crate) struct LexicalSearchProjection {
    symbols: Vec<LexicalSymbol>,
    paths: Vec<LexicalPath>,
}

struct LexicalSymbol {
    id: SymbolId,
    path_text: String,
    qualified_name: String,
    name: String,
    signature: String,
}

struct LexicalPath {
    path: RepositoryPath,
    path_text: String,
}

impl LexicalSearchProjection {
    pub(crate) fn work_units(&self) -> Result<u64, IndexPublicationRepositoryError> {
        [self.symbols.len(), self.paths.len()]
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
    exact: &ExactSearchProjection,
) -> Result<LexicalSearchProjection, IndexPublicationRepositoryError> {
    let qualified_names = exact
        .symbols()
        .iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect::<BTreeMap<_, _>>();
    let symbols = publication
        .graph()
        .symbols()
        .iter()
        .map(|symbol| {
            let qualified_name = qualified_names
                .get(&symbol.id())
                .ok_or(IndexPublicationRepositoryError::PublicationMismatch)?;
            Ok(LexicalSymbol {
                id: symbol.id(),
                path_text: searchable_path(symbol.revision().path()),
                qualified_name: (*qualified_name).to_owned(),
                name: symbol.parsed().name().as_str().to_owned(),
                signature: symbol
                    .parsed()
                    .signature()
                    .map_or_else(String::new, |signature| signature.as_str().to_owned()),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let paths = publication
        .graph()
        .files()
        .iter()
        .map(|revision| LexicalPath {
            path: revision.path().clone(),
            path_text: searchable_path(revision.path()),
        })
        .collect();
    Ok(LexicalSearchProjection { symbols, paths })
}

pub(crate) fn searchable_path(path: &RepositoryPath) -> String {
    if let Ok(path) = std::str::from_utf8(path.as_bytes()) {
        return path.to_owned();
    }
    let mut encoded = String::with_capacity(path.as_bytes().len().saturating_mul(3));
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-') {
            encoded.push(char::from(*byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) async fn write_projection(
    transaction: &Transaction,
    run_id: IndexRunId,
    projection: &LexicalSearchProjection,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError> {
    let symbol_count = i64::try_from(projection.symbols.len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    let path_count = i64::try_from(projection.paths.len())
        .map_err(|_| IndexPublicationRepositoryError::ResourceLimit)?;
    transaction
        .execute(
            "INSERT INTO lexical_search_projections\n\
             (index_run_id, projection_version, symbol_count, path_count, card_count)\n\
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![
                run_id.as_bytes().to_vec(),
                PROJECTION_VERSION,
                symbol_count,
                path_count
            ],
        )
        .await
        .map_err(IndexPublicationRepositoryError::Write)?;
    progress.advance(1)?;

    write_batches(
        transaction,
        &projection.symbols,
        "INSERT INTO symbol_fts\n\
         (index_run_id, symbol_id, repository_path, qualified_name, name, signature) VALUES ",
        6,
        |symbol| {
            vec![
                Value::Blob(run_id.as_bytes().to_vec()),
                Value::Blob(symbol.id.as_bytes().to_vec()),
                Value::Text(symbol.path_text.clone()),
                Value::Text(symbol.qualified_name.clone()),
                Value::Text(symbol.name.clone()),
                Value::Text(symbol.signature.clone()),
            ]
        },
        progress,
    )
    .await?;
    write_batches(
        transaction,
        &projection.paths,
        "INSERT INTO path_fts (index_run_id, repository_path, path) VALUES ",
        3,
        |path| {
            vec![
                Value::Blob(run_id.as_bytes().to_vec()),
                Value::Blob(path.path.as_bytes().to_vec()),
                Value::Text(path.path_text.clone()),
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
    columns: usize,
    values: F,
    progress: &mut MutationProgress<'_>,
) -> Result<(), IndexPublicationRepositoryError>
where
    F: Fn(&T) -> Vec<Value>,
{
    let rows_per_batch = MAX_BATCH_PARAMETERS
        .checked_div(columns)
        .map(|rows| rows.min(MAX_BATCH_ROWS))
        .filter(|rows| *rows > 0)
        .ok_or(IndexPublicationRepositoryError::ResourceLimit)?;
    for chunk in items.chunks(rows_per_batch) {
        progress.checkpoint()?;
        let mut sql = String::from(prefix);
        let placeholders = format!("({})", vec!["?"; columns].join(", "));
        let mut parameters = Vec::with_capacity(chunk.len().saturating_mul(columns));
        for (index, item) in chunk.iter().enumerate() {
            if index > 0 {
                sql.push(',');
            }
            sql.push_str(&placeholders);
            let row = values(item);
            if row.len() != columns {
                return Err(IndexPublicationRepositoryError::InvalidStoredData);
            }
            parameters.extend(row);
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

#[cfg(test)]
mod tests {
    use super::searchable_path;
    use a3_domain::RepositoryPath;

    #[test]
    fn non_utf8_paths_have_a_deterministic_searchable_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let path =
            RepositoryPath::try_from_bytes(vec![b's', b'r', b'c', b'/', 0xff, b'.', b'r', b's'])?;
        assert_eq!(searchable_path(&path), "src/%FF.rs");
        Ok(())
    }
}

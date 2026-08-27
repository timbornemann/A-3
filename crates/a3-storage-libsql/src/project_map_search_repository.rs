use crate::catalog::is_corruption;
use a3_application::{KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeStoreFailure};
use a3_domain::{ExactSearchTarget, IndexRunId, ModuleId};
use libsql::Connection;
use std::time::{Duration, Instant};

const MAX_BINDING_DURATION: Duration = Duration::from_secs(2);

pub(crate) async fn bind_modules(
    connection: &Connection,
    index_run_id: IndexRunId,
    targets: &[ExactSearchTarget],
    control: &dyn KnowledgeSearchControl,
) -> Result<Vec<Option<ModuleId>>, KnowledgeSearchFailure> {
    let started = Instant::now();
    let mut bindings = Vec::with_capacity(targets.len());
    for target in targets {
        checkpoint(control, started)?;
        let (sql, identity) = match target {
            ExactSearchTarget::File(revision) => (
                "SELECT MIN(member.module_id), COUNT(DISTINCT member.module_id) \
                 FROM module_members member JOIN modules module \
                   ON module.index_run_id = member.index_run_id \
                  AND module.module_id = member.module_id \
                 WHERE member.index_run_id = ?1 AND member.member_path = ?2 \
                   AND member.member_hash = ?3 AND module.kind IN ('manifest', 'path')",
                vec![
                    revision.path().as_bytes().to_vec(),
                    revision.content_hash().as_bytes().to_vec(),
                ],
            ),
            ExactSearchTarget::Symbol(symbol) => (
                "SELECT MIN(member.module_id), COUNT(DISTINCT member.module_id) \
                 FROM module_members member JOIN modules module \
                   ON module.index_run_id = member.index_run_id \
                  AND module.module_id = member.module_id \
                 WHERE member.index_run_id = ?1 AND member.symbol_id = ?2 \
                   AND module.kind IN ('manifest', 'path')",
                vec![symbol.symbol().id().as_bytes().to_vec()],
            ),
        };
        let mut values = vec![libsql::Value::Blob(index_run_id.as_bytes().to_vec())];
        values.extend(identity.into_iter().map(libsql::Value::Blob));
        let mut rows = connection
            .query(sql, libsql::params_from_iter(values))
            .await
            .map_err(classify)?;
        let row = rows
            .next()
            .await
            .map_err(classify)?
            .ok_or(KnowledgeSearchFailure::InvalidStoredProjection)?;
        let count = row.get::<i64>(1).map_err(classify)?;
        let binding = match count {
            0 => None,
            1 => {
                let bytes = row.get::<Vec<u8>>(0).map_err(classify)?;
                let bytes: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| KnowledgeSearchFailure::InvalidStoredProjection)?;
                Some(ModuleId::from_bytes(bytes))
            }
            _ => None,
        };
        bindings.push(binding);
    }
    checkpoint(control, started)?;
    Ok(bindings)
}

fn checkpoint(
    control: &dyn KnowledgeSearchControl,
    started: Instant,
) -> Result<(), KnowledgeSearchFailure> {
    if control.is_cancelled() {
        return Err(KnowledgeSearchFailure::Cancelled);
    }
    if started.elapsed() > MAX_BINDING_DURATION {
        return Err(KnowledgeSearchFailure::TimedOut);
    }
    Ok(())
}

fn classify(error: libsql::Error) -> KnowledgeSearchFailure {
    KnowledgeSearchFailure::Storage(if is_corruption(&error) {
        KnowledgeStoreFailure::Corrupt
    } else {
        KnowledgeStoreFailure::Unavailable
    })
}

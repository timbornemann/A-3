use crate::{
    function_flow_codec,
    index_publication::{IndexPublicationRepositoryError as Error, MutationProgress},
};
use a3_application::IndexPersistenceControl;
use a3_domain::{FunctionFlowBatch, GraphSymbol, IndexRunId, IndexedFunctionFlow, WorktreeId};
use libsql::{Connection, Transaction, params};

pub(crate) async fn write(
    transaction: &Transaction,
    run: IndexRunId,
    batch: &FunctionFlowBatch,
    progress: &mut MutationProgress<'_>,
) -> Result<(), Error> {
    for flow in batch.functions() {
        progress.checkpoint()?;
        let body = function_flow_codec::encode(flow)?;
        transaction.execute("INSERT INTO index_function_flows (index_run_id,symbol_id,schema_version,body) VALUES (?1,?2,1,?3)",params![run.as_bytes().to_vec(),flow.symbol().as_bytes().to_vec(),body]).await.map_err(Error::Write)?;
        progress.advance(1)?;
    }
    Ok(())
}
pub(crate) async fn read(
    connection: &Connection,
    worktree: WorktreeId,
    run: IndexRunId,
    owner: &GraphSymbol,
    control: &dyn IndexPersistenceControl,
) -> Result<Option<IndexedFunctionFlow>, Error> {
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mut rows=connection.query(
        "SELECT f.body FROM index_function_flows f
         JOIN index_runs r ON r.index_run_id=f.index_run_id
         JOIN snapshots s ON s.snapshot_id=r.snapshot_id
         JOIN symbols owner ON owner.index_run_id=f.index_run_id AND owner.symbol_id=f.symbol_id
         WHERE f.index_run_id=?1 AND f.symbol_id=?2 AND f.schema_version=1
           AND r.worktree_id=?3 AND r.status='published'
           AND owner.repository_path=?4 AND owner.content_hash=?5
           AND NOT EXISTS (SELECT 1 FROM snapshots newer WHERE newer.worktree_id=r.worktree_id AND newer.generation>s.generation)
           AND NOT EXISTS (SELECT 1 FROM index_runs newer WHERE newer.worktree_id=r.worktree_id AND newer.status='published' AND newer.run_sequence>r.run_sequence)
         LIMIT 1",
        params![run.as_bytes().to_vec(),owner.id().as_bytes().to_vec(),worktree.as_bytes().to_vec(),owner.revision().path().as_bytes().to_vec(),owner.revision().content_hash().as_bytes().to_vec()]
    ).await.map_err(Error::Read)?;
    let Some(row) = rows.next().await.map_err(Error::Read)? else {
        return Ok(None);
    };
    if control.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let body: String = row.get(0).map_err(Error::Read)?;
    function_flow_codec::decode(owner, &body).map(Some)
}

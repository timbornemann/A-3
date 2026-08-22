use crate::{CatalogDatabase, CatalogOpenError};
use a3_application::{
    KnowledgeStoreFailure, PROJECT_CATALOG_PAGE_SIZE, ProjectCatalogAdminFailure,
    ProjectCatalogCursor, ProjectCatalogDirection, ProjectCatalogPage, ProjectCatalogQuery,
    ProjectCatalogRevision, ProjectOpenPreparation, ProjectPathDisplay,
    ProjectReconciliationEvidence, ProjectReconciliationProposal, RecentProject,
    RecentProjectLimit, StoredProjectTarget,
};
use a3_domain::{
    GitHead, GitObjectId, GitReferenceName, ProjectId, ProjectIdentity, RemoteIdentity,
    RepositoryId, WorktreeAnchorId, WorktreeId,
};
use blake3::Hasher;
use libsql::{Connection, Transaction, TransactionBehavior, params};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

const PROJECT_ID_VERSION: &[u8] = b"a3.catalog-project-id.v1";
const RECONCILIATION_CANDIDATE_LIMIT: i64 = 2;
const SQLITE_CONSTRAINT: i32 = 19;
const SQLITE_CORRUPT: i32 = 11;
const SQLITE_NOT_A_DATABASE: i32 = 26;

impl CatalogDatabase {
    pub(crate) async fn prepare_project_open(
        &self,
        project: &ProjectIdentity,
    ) -> Result<ProjectOpenPreparation, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        prepare_project_open(&connection, project).await
    }

    pub(crate) async fn record_project(
        &self,
        project: &ProjectIdentity,
    ) -> Result<ProjectId, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;

        let result = record_project_in_transaction(&transaction, project).await;
        let project_id = rollback_on_error(transaction, result).await?;
        Ok(project_id)
    }

    pub(crate) async fn prepare_reconciliation(
        &self,
        project: &ProjectIdentity,
        proposal: &ProjectReconciliationProposal,
    ) -> Result<(), ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;
        let result = prepare_reconciliation_in_transaction(&transaction, project, proposal).await;
        rollback_on_error(transaction, result).await
    }

    pub(crate) async fn complete_reconciliation(
        &self,
        project: &ProjectIdentity,
        proposal: &ProjectReconciliationProposal,
    ) -> Result<ProjectId, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;
        let result = complete_reconciliation_in_transaction(&transaction, project, proposal).await;
        rollback_on_error(transaction, result).await
    }

    pub(crate) async fn read_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> Result<Vec<RecentProject>, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let mut rows = connection
            .query(
                "SELECT recent.project_id, recent.repository_id, recent.worktree_id,\n\
                 recent.worktree_root_display, recent.head_kind, recent.head_object_id,\n\
                 recent.head_reference, observations.project_id\n\
                 FROM recent_worktrees AS recent\n\
                 LEFT JOIN repository_observations AS observations\n\
                   ON observations.repository_id = recent.repository_id\n\
                 ORDER BY recent.last_open_sequence DESC\n\
                 LIMIT ?1",
                [i64::from(limit.get())],
            )
            .await
            .map_err(ProjectCatalogError::Read)?;
        let mut projects = Vec::with_capacity(usize::from(limit.get()));
        while let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? {
            projects.push(recent_project_from_row(&row)?);
        }
        Ok(projects)
    }

    pub(crate) async fn read_project_catalog(
        &self,
        query: &ProjectCatalogQuery,
    ) -> Result<ProjectCatalogPage, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let cursor = query
            .cursor()
            .map(|cursor| i64::try_from(cursor.get()))
            .transpose()
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
        let limit = i64::try_from(PROJECT_CATALOG_PAGE_SIZE + 1)
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
        let search = query.search().map(|value| {
            if value.chars().count() < 3 {
                (project_catalog_like_expression(value), false)
            } else {
                (project_catalog_fts_expression(value), true)
            }
        });
        let base_projection = "SELECT recent.project_id, recent.repository_id, recent.worktree_id,\n\
            recent.worktree_root_display, recent.head_kind, recent.head_object_id,\n\
            recent.head_reference, observations.project_id, recent.last_open_sequence\n\
            FROM recent_worktrees AS recent\n\
            LEFT JOIN repository_observations AS observations\n\
              ON observations.repository_id = recent.repository_id";
        let search_join = if search.as_ref().is_some_and(|(_, uses_fts)| *uses_fts) {
            " JOIN project_catalog_fts AS catalog_search\n\
               ON catalog_search.rowid = recent.rowid"
        } else {
            ""
        };
        let search_predicate = if search.as_ref().is_some_and(|(_, uses_fts)| *uses_fts) {
            "project_catalog_fts MATCH ?1"
        } else {
            "recent.worktree_root_display LIKE ?1 ESCAPE '\\'"
        };
        let order = if query.direction() == ProjectCatalogDirection::Previous {
            "ASC"
        } else {
            "DESC"
        };
        let comparator = if query.direction() == ProjectCatalogDirection::Previous {
            ">"
        } else {
            "<"
        };
        let sql = match (search.is_some(), cursor.is_some()) {
            (false, false) => {
                format!("{base_projection} ORDER BY recent.last_open_sequence {order} LIMIT ?1")
            }
            (false, true) => format!(
                "{base_projection} WHERE recent.last_open_sequence {comparator} ?1\n\
                 ORDER BY recent.last_open_sequence {order} LIMIT ?2"
            ),
            (true, false) => format!(
                "{base_projection}{search_join}\n\
                 WHERE {search_predicate}\n\
                 ORDER BY recent.last_open_sequence {order} LIMIT ?2"
            ),
            (true, true) => format!(
                "{base_projection}{search_join}\n\
                 WHERE {search_predicate}\n\
                   AND recent.last_open_sequence {comparator} ?2\n\
                 ORDER BY recent.last_open_sequence {order} LIMIT ?3"
            ),
        };
        let mut rows = match (search.as_ref().map(|(value, _)| value.as_str()), cursor) {
            (None, None) => connection.query(&sql, [limit]).await,
            (None, Some(cursor)) => connection.query(&sql, params![cursor, limit]).await,
            (Some(search), None) => connection.query(&sql, params![search, limit]).await,
            (Some(search), Some(cursor)) => {
                connection.query(&sql, params![search, cursor, limit]).await
            }
        }
        .map_err(ProjectCatalogError::Read)?;
        let mut entries = Vec::with_capacity(PROJECT_CATALOG_PAGE_SIZE + 1);
        while let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? {
            let sequence: i64 = row.get(8).map_err(ProjectCatalogError::Read)?;
            let sequence =
                u64::try_from(sequence).map_err(|_| ProjectCatalogError::InvalidStoredData)?;
            entries.push((recent_project_from_row(&row)?, sequence));
        }
        let has_extra = entries.len() > PROJECT_CATALOG_PAGE_SIZE;
        if has_extra {
            entries.pop();
        }
        if query.direction() == ProjectCatalogDirection::Previous {
            entries.reverse();
        }
        let first_cursor = entries
            .first()
            .map(|(_, sequence)| ProjectCatalogCursor::new(*sequence))
            .transpose()
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
        let last_cursor = entries
            .last()
            .map(|(_, sequence)| ProjectCatalogCursor::new(*sequence))
            .transpose()
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
        let previous_cursor = match query.direction() {
            ProjectCatalogDirection::Initial => None,
            ProjectCatalogDirection::Next => first_cursor,
            ProjectCatalogDirection::Previous if has_extra => first_cursor,
            ProjectCatalogDirection::Previous => None,
        };
        let next_cursor = match query.direction() {
            ProjectCatalogDirection::Initial | ProjectCatalogDirection::Next if has_extra => {
                last_cursor
            }
            ProjectCatalogDirection::Previous => last_cursor,
            _ => None,
        };
        Ok(ProjectCatalogPage::new(
            entries.into_iter().map(|(project, _)| project).collect(),
            previous_cursor,
            next_cursor,
        ))
    }

    pub(crate) async fn resolve_project_catalog_entry(
        &self,
        worktree_id: Option<WorktreeId>,
    ) -> Result<Option<StoredProjectTarget>, ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let (sql, parameter) = match worktree_id {
            Some(worktree_id) => (
                "SELECT project_id, repository_id, worktree_id, worktree_root,\n\
                 worktree_path_encoding FROM recent_worktrees WHERE worktree_id = ?1",
                Some(worktree_id.as_bytes().to_vec()),
            ),
            None => (
                "SELECT project_id, repository_id, worktree_id, worktree_root,\n\
                 worktree_path_encoding FROM recent_worktrees\n\
                 ORDER BY last_open_sequence DESC LIMIT 1",
                None,
            ),
        };
        let mut rows = match parameter {
            Some(parameter) => connection.query(sql, [parameter]).await,
            None => connection.query(sql, ()).await,
        }
        .map_err(ProjectCatalogError::Read)?;
        let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
            return Ok(None);
        };
        let bytes: Vec<u8> = row.get(3).map_err(ProjectCatalogError::Read)?;
        let encoding: String = row.get(4).map_err(ProjectCatalogError::Read)?;
        let target = StoredProjectTarget::new(
            ProjectId::from_bytes(read_stable_id(&row, 0)?),
            RepositoryId::from_bytes(read_stable_id(&row, 1)?),
            WorktreeId::from_bytes(read_stable_id(&row, 2)?),
            decode_path(&encoding, bytes)?,
        );
        if rows
            .next()
            .await
            .map_err(ProjectCatalogError::Read)?
            .is_some()
        {
            return Err(ProjectCatalogError::InvalidStoredData);
        }
        Ok(Some(target))
    }

    pub(crate) async fn remove_recent_worktree(
        &self,
        project: &ProjectIdentity,
        project_id: ProjectId,
    ) -> Result<(), ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;
        let result = remove_recent_worktree_in_transaction(&transaction, project, project_id).await;
        rollback_on_error(transaction, result).await
    }

    pub(crate) async fn remove_catalog_worktree(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<(), ProjectCatalogError> {
        let connection = self
            .connection_for_operation()
            .await
            .map_err(ProjectCatalogError::Open)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(ProjectCatalogError::Begin)?;
        let key = worktree_id.as_bytes().to_vec();
        transaction
            .execute(
                "DELETE FROM worktree_reconciliations\n\
                 WHERE source_worktree_id = ?1 OR target_worktree_id = ?1",
                [key.clone()],
            )
            .await
            .map_err(ProjectCatalogError::Write)?;
        let deleted = transaction
            .execute("DELETE FROM recent_worktrees WHERE worktree_id = ?1", [key])
            .await
            .map_err(ProjectCatalogError::Write)?;
        if deleted != 1 {
            return rollback_on_error(transaction, Err(ProjectCatalogError::NotFound)).await;
        }
        rollback_on_error(transaction, Ok(())).await
    }
}

fn project_catalog_fts_expression(search: &str) -> String {
    format!("\"{}\"", search.replace('"', "\"\""))
}

fn project_catalog_like_expression(search: &str) -> String {
    let mut expression = String::with_capacity(search.len().saturating_add(2));
    expression.push('%');
    for character in search.chars() {
        if matches!(character, '%' | '_' | '\\') {
            expression.push('\\');
        }
        expression.push(character);
    }
    expression.push('%');
    expression
}

async fn remove_recent_worktree_in_transaction(
    transaction: &Transaction,
    project: &ProjectIdentity,
    project_id: ProjectId,
) -> Result<(), ProjectCatalogError> {
    let worktree_id = project.worktree().id().as_bytes().to_vec();
    let mut rows = transaction
        .query(
            "SELECT project_id, repository_id FROM recent_worktrees WHERE worktree_id = ?1",
            [worktree_id.clone()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Err(ProjectCatalogError::NotFound);
    };
    let stored_project_id = ProjectId::from_bytes(read_stable_id(&row, 0)?);
    let stored_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 1)?);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || stored_project_id != project_id
        || stored_repository_id != project.repository().id()
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }

    transaction
        .execute(
            "DELETE FROM worktree_reconciliations\n\
             WHERE project_id = ?1 AND (source_worktree_id = ?2 OR target_worktree_id = ?2)",
            params![project_id.as_bytes().to_vec(), worktree_id.clone()],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    let deleted = transaction
        .execute(
            "DELETE FROM recent_worktrees\n\
             WHERE worktree_id = ?1 AND project_id = ?2 AND repository_id = ?3",
            params![
                worktree_id,
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec()
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if deleted != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn rollback_on_error<T>(
    transaction: Transaction,
    result: Result<T, ProjectCatalogError>,
) -> Result<T, ProjectCatalogError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(ProjectCatalogError::Commit)?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(source) => Err(ProjectCatalogError::Rollback(source)),
        },
    }
}

async fn prepare_project_open(
    connection: &Connection,
    project: &ProjectIdentity,
) -> Result<ProjectOpenPreparation, ProjectCatalogError> {
    if let Some(proposal) = pending_reconciliation(connection, project).await? {
        return Ok(ProjectOpenPreparation::ResumeConfirmed(proposal));
    }
    if existing_target_is_compatible(connection, project).await? {
        return Ok(ProjectOpenPreparation::Ready);
    }

    let candidates = reconciliation_candidates(connection, project).await?;
    let same_repository: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.previous_repository_id() == project.repository().id())
        .cloned()
        .collect();
    if same_repository.len() == 1 {
        return Ok(ProjectOpenPreparation::ConfirmationRequired(
            same_repository[0].clone(),
        ));
    }
    if !same_repository.is_empty() {
        return Ok(ProjectOpenPreparation::Ready);
    }

    let remote_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.evidence() == ProjectReconciliationEvidence::RemoteAndWorktreeAnchor
        })
        .collect();
    if remote_candidates.len() == 1 {
        Ok(ProjectOpenPreparation::ConfirmationRequired(
            remote_candidates[0].clone(),
        ))
    } else {
        Ok(ProjectOpenPreparation::Ready)
    }
}

async fn pending_reconciliation(
    connection: &Connection,
    project: &ProjectIdentity,
) -> Result<Option<ProjectReconciliationProposal>, ProjectCatalogError> {
    let mut rows = connection
        .query(
            "SELECT intent.project_id, intent.source_repository_id, intent.source_worktree_id,\n\
             intent.worktree_anchor_id, source.worktree_root_display,\n\
             intent.source_last_open_sequence, intent.evidence_kind, intent.target_repository_id\n\
             FROM worktree_reconciliations AS intent\n\
             JOIN recent_worktrees AS source\n\
               ON source.worktree_id = intent.source_worktree_id\n\
             WHERE intent.target_worktree_id = ?1 AND intent.status = 'prepared'",
            [project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(None);
    };
    let proposal = proposal_from_row(&row)?;
    let target_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 7)?);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || target_repository_id != project.repository().id()
        || !proposal_matches_target(connection, &proposal, project).await?
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(Some(proposal))
}

async fn existing_target_is_compatible(
    connection: &Connection,
    project: &ProjectIdentity,
) -> Result<bool, ProjectCatalogError> {
    let mut rows = connection
        .query(
            "SELECT repository_id, worktree_anchor_id\n\
             FROM recent_worktrees WHERE worktree_id = ?1",
            [project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(false);
    };
    let repository_id = RepositoryId::from_bytes(read_stable_id(&row, 0)?);
    let anchor_id = read_optional_stable_id(&row, 1)?.map(WorktreeAnchorId::from_bytes);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || repository_id != project.repository().id()
        || anchor_id.is_some_and(|anchor| anchor != project.worktree().anchor_id())
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(true)
}

async fn reconciliation_candidates(
    connection: &Connection,
    project: &ProjectIdentity,
) -> Result<Vec<ProjectReconciliationProposal>, ProjectCatalogError> {
    let target_remote = project
        .repository()
        .main_remote()
        .map(|remote| remote.as_bytes().to_vec());
    let mut rows = connection
        .query(
            "SELECT recent.project_id, recent.repository_id, recent.worktree_id,\n\
             recent.worktree_anchor_id, recent.worktree_root_display,\n\
             recent.last_open_sequence, observations.main_remote_id\n\
             FROM recent_worktrees AS recent\n\
             JOIN repository_observations AS observations\n\
               ON observations.project_id = recent.project_id\n\
              AND observations.repository_id = recent.repository_id\n\
             WHERE recent.worktree_id <> ?1 AND recent.worktree_anchor_id = ?2\n\
               AND (recent.repository_id = ?3 OR (?4 IS NOT NULL AND observations.main_remote_id = ?4))\n\
             ORDER BY (recent.repository_id = ?3) DESC, recent.last_open_sequence DESC\n\
             LIMIT ?5",
            params![
                project.worktree().id().as_bytes().to_vec(),
                project.worktree().anchor_id().as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                target_remote,
                RECONCILIATION_CANDIDATE_LIMIT
            ],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? {
        let source_repository_id = RepositoryId::from_bytes(read_stable_id(&row, 1)?);
        let source_remote = read_optional_stable_id(&row, 6)?.map(RemoteIdentity::from_bytes);
        let evidence = if source_repository_id == project.repository().id() {
            Some(ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor)
        } else if project.repository().main_remote().is_some()
            && source_remote == project.repository().main_remote()
        {
            Some(ProjectReconciliationEvidence::RemoteAndWorktreeAnchor)
        } else {
            None
        };
        if let Some(evidence) = evidence {
            candidates.push(ProjectReconciliationProposal::new(
                ProjectId::from_bytes(read_stable_id(&row, 0)?),
                source_repository_id,
                WorktreeId::from_bytes(read_stable_id(&row, 2)?),
                WorktreeAnchorId::from_bytes(read_stable_id(&row, 3)?),
                ProjectPathDisplay::try_from_stored(row.get(4).map_err(ProjectCatalogError::Read)?)
                    .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
                revision_from_i64(row.get(5).map_err(ProjectCatalogError::Read)?)?,
                evidence,
            ));
        }
    }
    Ok(candidates)
}

async fn proposal_matches_target(
    connection: &Connection,
    proposal: &ProjectReconciliationProposal,
    project: &ProjectIdentity,
) -> Result<bool, ProjectCatalogError> {
    if proposal.previous_worktree_id() == project.worktree().id()
        || proposal.previous_worktree_anchor_id() != project.worktree().anchor_id()
    {
        return Ok(false);
    }
    match proposal.evidence() {
        ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor => {
            Ok(proposal.previous_repository_id() == project.repository().id())
        }
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor => {
            let Some(target_remote) = project.repository().main_remote() else {
                return Ok(false);
            };
            let source_remote =
                repository_remote(connection, proposal.previous_repository_id()).await?;
            Ok(
                proposal.previous_repository_id() != project.repository().id()
                    && source_remote == Some(target_remote),
            )
        }
    }
}

async fn repository_remote(
    connection: &Connection,
    repository_id: RepositoryId,
) -> Result<Option<RemoteIdentity>, ProjectCatalogError> {
    let mut rows = connection
        .query(
            "SELECT main_remote_id FROM repository_observations WHERE repository_id = ?1",
            [repository_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let remote = read_optional_stable_id(&row, 0)?.map(RemoteIdentity::from_bytes);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(remote)
}

async fn record_project_in_transaction(
    transaction: &Transaction,
    project: &ProjectIdentity,
) -> Result<ProjectId, ProjectCatalogError> {
    if prepared_intent_exists(transaction, project.worktree().id()).await? {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    let sequence = next_open_sequence(transaction).await?;
    let repository_id = project.repository().id();
    let project_id = match worktree_ownership(transaction, project).await? {
        Some(project_id) => project_id,
        None => match project_for_repository(transaction, repository_id).await? {
            Some(existing) => existing,
            None => {
                let created = derive_project_id(repository_id);
                insert_project(transaction, created, sequence).await?;
                created
            }
        },
    };

    upsert_repository_observation(transaction, project_id, project, sequence).await?;
    update_project(transaction, project_id, sequence).await?;
    upsert_worktree(transaction, project_id, project, sequence).await?;
    Ok(project_id)
}

async fn prepare_reconciliation_in_transaction(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<(), ProjectCatalogError> {
    if existing_intent_matches(transaction, project, proposal).await? {
        return Ok(());
    }
    validate_source_proposal(transaction, project, proposal).await?;
    if worktree_exists(transaction, project.worktree().id()).await? {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    transaction
        .execute(
            "INSERT INTO worktree_reconciliations (\n\
             target_worktree_id, source_worktree_id, project_id, source_repository_id,\n\
             target_repository_id, worktree_anchor_id, evidence_kind,\n\
             source_last_open_sequence, status, completed_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'prepared', NULL)",
            params![
                project.worktree().id().as_bytes().to_vec(),
                proposal.previous_worktree_id().as_bytes().to_vec(),
                proposal.project_id().as_bytes().to_vec(),
                proposal.previous_repository_id().as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                proposal.previous_worktree_anchor_id().as_bytes().to_vec(),
                evidence_name(proposal.evidence()),
                revision_to_i64(proposal.expected_revision())?
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    Ok(())
}

async fn complete_reconciliation_in_transaction(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<ProjectId, ProjectCatalogError> {
    let status = reconciliation_status(transaction, project, proposal).await?;
    if status == ReconciliationStatus::Completed {
        return completed_target_project(transaction, project, proposal).await;
    }
    validate_source_proposal(transaction, project, proposal).await?;
    if worktree_exists(transaction, project.worktree().id()).await? {
        return Err(ProjectCatalogError::IdentityConflict);
    }

    let sequence = next_open_sequence(transaction).await?;
    upsert_repository_observation(transaction, proposal.project_id(), project, sequence).await?;
    update_project(transaction, proposal.project_id(), sequence).await?;
    let deleted = transaction
        .execute(
            "DELETE FROM recent_worktrees\n\
             WHERE worktree_id = ?1 AND project_id = ?2 AND repository_id = ?3\n\
               AND worktree_anchor_id = ?4 AND last_open_sequence = ?5",
            params![
                proposal.previous_worktree_id().as_bytes().to_vec(),
                proposal.project_id().as_bytes().to_vec(),
                proposal.previous_repository_id().as_bytes().to_vec(),
                proposal.previous_worktree_anchor_id().as_bytes().to_vec(),
                revision_to_i64(proposal.expected_revision())?
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if deleted != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    insert_worktree(transaction, proposal.project_id(), project, sequence).await?;
    let updated = transaction
        .execute(
            "UPDATE worktree_reconciliations\n\
             SET status = 'completed', completed_open_sequence = ?1\n\
             WHERE target_worktree_id = ?2 AND status = 'prepared'",
            params![sequence, project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if updated != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(proposal.project_id())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconciliationStatus {
    Prepared,
    Completed,
}

async fn existing_intent_matches(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<bool, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT source_worktree_id, project_id, source_repository_id, target_repository_id,\n\
             worktree_anchor_id, evidence_kind, source_last_open_sequence\n\
             FROM worktree_reconciliations WHERE target_worktree_id = ?1",
            [project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(false);
    };
    let matches = WorktreeId::from_bytes(read_stable_id(&row, 0)?)
        == proposal.previous_worktree_id()
        && ProjectId::from_bytes(read_stable_id(&row, 1)?) == proposal.project_id()
        && RepositoryId::from_bytes(read_stable_id(&row, 2)?) == proposal.previous_repository_id()
        && RepositoryId::from_bytes(read_stable_id(&row, 3)?) == project.repository().id()
        && WorktreeAnchorId::from_bytes(read_stable_id(&row, 4)?)
            == proposal.previous_worktree_anchor_id()
        && parse_evidence_name(&row.get::<String>(5).map_err(ProjectCatalogError::Read)?)?
            == proposal.evidence()
        && revision_from_i64(row.get(6).map_err(ProjectCatalogError::Read)?)?
            == proposal.expected_revision();
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || !matches
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(true)
}

async fn reconciliation_status(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<ReconciliationStatus, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT status FROM worktree_reconciliations\n\
             WHERE target_worktree_id = ?1 AND source_worktree_id = ?2 AND project_id = ?3\n\
               AND source_repository_id = ?4 AND target_repository_id = ?5\n\
               AND worktree_anchor_id = ?6 AND evidence_kind = ?7\n\
               AND source_last_open_sequence = ?8",
            params![
                project.worktree().id().as_bytes().to_vec(),
                proposal.previous_worktree_id().as_bytes().to_vec(),
                proposal.project_id().as_bytes().to_vec(),
                proposal.previous_repository_id().as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                proposal.previous_worktree_anchor_id().as_bytes().to_vec(),
                evidence_name(proposal.evidence()),
                revision_to_i64(proposal.expected_revision())?
            ],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::IdentityConflict)?;
    let status: String = row.get(0).map_err(ProjectCatalogError::Read)?;
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    match status.as_str() {
        "prepared" => Ok(ReconciliationStatus::Prepared),
        "completed" => Ok(ReconciliationStatus::Completed),
        _ => Err(ProjectCatalogError::InvalidStoredData),
    }
}

async fn validate_source_proposal(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<(), ProjectCatalogError> {
    if proposal.previous_worktree_id() == project.worktree().id()
        || proposal.previous_worktree_anchor_id() != project.worktree().anchor_id()
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    let mut rows = transaction
        .query(
            "SELECT recent.project_id, recent.repository_id, recent.worktree_anchor_id,\n\
             recent.last_open_sequence, observations.main_remote_id\n\
             FROM recent_worktrees AS recent\n\
             JOIN repository_observations AS observations\n\
               ON observations.project_id = recent.project_id\n\
              AND observations.repository_id = recent.repository_id\n\
             WHERE recent.worktree_id = ?1",
            [proposal.previous_worktree_id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::IdentityConflict)?;
    let source_remote = read_optional_stable_id(&row, 4)?.map(RemoteIdentity::from_bytes);
    let base_matches = ProjectId::from_bytes(read_stable_id(&row, 0)?) == proposal.project_id()
        && RepositoryId::from_bytes(read_stable_id(&row, 1)?) == proposal.previous_repository_id()
        && WorktreeAnchorId::from_bytes(read_stable_id(&row, 2)?)
            == proposal.previous_worktree_anchor_id()
        && revision_from_i64(row.get(3).map_err(ProjectCatalogError::Read)?)?
            == proposal.expected_revision();
    let evidence_matches = match proposal.evidence() {
        ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor => {
            proposal.previous_repository_id() == project.repository().id()
        }
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor => {
            proposal.previous_repository_id() != project.repository().id()
                && project.repository().main_remote().is_some()
                && source_remote == project.repository().main_remote()
        }
    };
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || !base_matches
        || !evidence_matches
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn completed_target_project(
    transaction: &Transaction,
    project: &ProjectIdentity,
    proposal: &ProjectReconciliationProposal,
) -> Result<ProjectId, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT project_id, repository_id, worktree_anchor_id\n\
             FROM recent_worktrees WHERE worktree_id = ?1",
            [project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let valid = ProjectId::from_bytes(read_stable_id(&row, 0)?) == proposal.project_id()
        && RepositoryId::from_bytes(read_stable_id(&row, 1)?) == project.repository().id()
        && WorktreeAnchorId::from_bytes(read_stable_id(&row, 2)?) == project.worktree().anchor_id();
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || !valid
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(proposal.project_id())
}

async fn next_open_sequence(transaction: &Transaction) -> Result<i64, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT COALESCE(MAX(last_open_sequence), 0) FROM (\n\
             SELECT last_open_sequence FROM projects\n\
             UNION ALL\n\
             SELECT last_open_sequence FROM repository_observations\n\
             UNION ALL\n\
             SELECT last_open_sequence FROM recent_worktrees\n\
             )",
            (),
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let current: i64 = row.get(0).map_err(ProjectCatalogError::Read)?;
    current
        .checked_add(1)
        .filter(|value| *value > 0)
        .ok_or(ProjectCatalogError::SequenceExhausted)
}

async fn worktree_ownership(
    transaction: &Transaction,
    project: &ProjectIdentity,
) -> Result<Option<ProjectId>, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT project_id, repository_id, worktree_anchor_id\n\
             FROM recent_worktrees WHERE worktree_id = ?1",
            [project.worktree().id().as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(None);
    };
    let project_id = ProjectId::from_bytes(read_stable_id(&row, 0)?);
    let repository_id = RepositoryId::from_bytes(read_stable_id(&row, 1)?);
    let anchor = read_optional_stable_id(&row, 2)?.map(WorktreeAnchorId::from_bytes);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
        || repository_id != project.repository().id()
        || anchor.is_some_and(|value| value != project.worktree().anchor_id())
    {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(Some(project_id))
}

async fn project_for_repository(
    transaction: &Transaction,
    repository_id: RepositoryId,
) -> Result<Option<ProjectId>, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT project_id FROM repository_observations WHERE repository_id = ?1",
            [repository_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let Some(row) = rows.next().await.map_err(ProjectCatalogError::Read)? else {
        return Ok(None);
    };
    let project_id = ProjectId::from_bytes(read_stable_id(&row, 0)?);
    if rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .is_some()
    {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(Some(project_id))
}

async fn insert_project(
    transaction: &Transaction,
    project_id: ProjectId,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    transaction
        .execute(
            "INSERT INTO projects (project_id, created_open_sequence, last_open_sequence)\n\
             VALUES (?1, ?2, ?3)",
            params![project_id.as_bytes().to_vec(), sequence, sequence],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    Ok(())
}

async fn update_project(
    transaction: &Transaction,
    project_id: ProjectId,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let affected = transaction
        .execute(
            "UPDATE projects SET last_open_sequence = ?1 WHERE project_id = ?2",
            params![sequence, project_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if affected != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn upsert_repository_observation(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let common_directory = encode_path(project.repository().common_directory().as_path());
    let remote = project
        .repository()
        .main_remote()
        .map(|identity| identity.as_bytes().to_vec());
    let affected = transaction
        .execute(
            "INSERT INTO repository_observations (\n\
             repository_id, project_id, repository_common_directory, repository_path_encoding,\n\
             main_remote_id, first_open_sequence, last_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n\
             ON CONFLICT(repository_id) DO UPDATE SET\n\
             repository_common_directory = excluded.repository_common_directory,\n\
             repository_path_encoding = excluded.repository_path_encoding,\n\
             main_remote_id = excluded.main_remote_id,\n\
             last_open_sequence = excluded.last_open_sequence\n\
             WHERE repository_observations.project_id = excluded.project_id",
            params![
                project.repository().id().as_bytes().to_vec(),
                project_id.as_bytes().to_vec(),
                common_directory.bytes,
                common_directory.encoding,
                remote,
                sequence,
                sequence
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if affected != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn upsert_worktree(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let root = encode_path(project.worktree().root().as_path());
    let display = ProjectPathDisplay::from_path(project.worktree().root().as_path());
    let head = HeadFields::from(project.head());
    let affected = transaction
        .execute(
            "INSERT INTO recent_worktrees (\n\
             worktree_id, project_id, repository_id, worktree_anchor_id, worktree_root,\n\
             worktree_path_encoding, worktree_root_display, head_kind, head_object_id,\n\
             head_reference, last_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)\n\
             ON CONFLICT(worktree_id) DO UPDATE SET\n\
             project_id = excluded.project_id, repository_id = excluded.repository_id,\n\
             worktree_anchor_id = excluded.worktree_anchor_id, worktree_root = excluded.worktree_root,\n\
             worktree_path_encoding = excluded.worktree_path_encoding,\n\
             worktree_root_display = excluded.worktree_root_display,\n\
             head_kind = excluded.head_kind, head_object_id = excluded.head_object_id,\n\
             head_reference = excluded.head_reference,\n\
             last_open_sequence = excluded.last_open_sequence",
            params![
                project.worktree().id().as_bytes().to_vec(),
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                project.worktree().anchor_id().as_bytes().to_vec(),
                root.bytes,
                root.encoding,
                display.as_str(),
                head.kind,
                head.object_id,
                head.reference,
                sequence
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    if affected != 1 {
        return Err(ProjectCatalogError::IdentityConflict);
    }
    Ok(())
}

async fn insert_worktree(
    transaction: &Transaction,
    project_id: ProjectId,
    project: &ProjectIdentity,
    sequence: i64,
) -> Result<(), ProjectCatalogError> {
    let root = encode_path(project.worktree().root().as_path());
    let display = ProjectPathDisplay::from_path(project.worktree().root().as_path());
    let head = HeadFields::from(project.head());
    transaction
        .execute(
            "INSERT INTO recent_worktrees (\n\
             worktree_id, project_id, repository_id, worktree_anchor_id, worktree_root,\n\
             worktree_path_encoding, worktree_root_display, head_kind, head_object_id,\n\
             head_reference, last_open_sequence\n\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                project.worktree().id().as_bytes().to_vec(),
                project_id.as_bytes().to_vec(),
                project.repository().id().as_bytes().to_vec(),
                project.worktree().anchor_id().as_bytes().to_vec(),
                root.bytes,
                root.encoding,
                display.as_str(),
                head.kind,
                head.object_id,
                head.reference,
                sequence
            ],
        )
        .await
        .map_err(ProjectCatalogError::Write)?;
    Ok(())
}

async fn prepared_intent_exists(
    transaction: &Transaction,
    target_worktree_id: WorktreeId,
) -> Result<bool, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM worktree_reconciliations\n\
             WHERE target_worktree_id = ?1 AND status = 'prepared'",
            [target_worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(ProjectCatalogError::Read)?;
    Ok(count == 1)
}

async fn worktree_exists(
    transaction: &Transaction,
    worktree_id: WorktreeId,
) -> Result<bool, ProjectCatalogError> {
    let mut rows = transaction
        .query(
            "SELECT COUNT(*) FROM recent_worktrees WHERE worktree_id = ?1",
            [worktree_id.as_bytes().to_vec()],
        )
        .await
        .map_err(ProjectCatalogError::Read)?;
    let row = rows
        .next()
        .await
        .map_err(ProjectCatalogError::Read)?
        .ok_or(ProjectCatalogError::InvalidStoredData)?;
    let count: i64 = row.get(0).map_err(ProjectCatalogError::Read)?;
    Ok(count == 1)
}

fn proposal_from_row(
    row: &libsql::Row,
) -> Result<ProjectReconciliationProposal, ProjectCatalogError> {
    Ok(ProjectReconciliationProposal::new(
        ProjectId::from_bytes(read_stable_id(row, 0)?),
        RepositoryId::from_bytes(read_stable_id(row, 1)?),
        WorktreeId::from_bytes(read_stable_id(row, 2)?),
        WorktreeAnchorId::from_bytes(read_stable_id(row, 3)?),
        ProjectPathDisplay::try_from_stored(row.get(4).map_err(ProjectCatalogError::Read)?)
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
        revision_from_i64(row.get(5).map_err(ProjectCatalogError::Read)?)?,
        parse_evidence_name(&row.get::<String>(6).map_err(ProjectCatalogError::Read)?)?,
    ))
}

fn recent_project_from_row(row: &libsql::Row) -> Result<RecentProject, ProjectCatalogError> {
    let project_id = ProjectId::from_bytes(read_stable_id(row, 0)?);
    let repository_id = RepositoryId::from_bytes(read_stable_id(row, 1)?);
    let observation_project_id = ProjectId::from_bytes(
        read_optional_stable_id(row, 7)?.ok_or(ProjectCatalogError::InvalidStoredData)?,
    );
    if project_id != observation_project_id {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    let worktree_id = WorktreeId::from_bytes(read_stable_id(row, 2)?);
    let display =
        ProjectPathDisplay::try_from_stored(row.get(3).map_err(ProjectCatalogError::Read)?)
            .map_err(|_| ProjectCatalogError::InvalidStoredData)?;
    let kind: String = row.get(4).map_err(ProjectCatalogError::Read)?;
    let object_id: Option<String> = row.get(5).map_err(ProjectCatalogError::Read)?;
    let reference: Option<String> = row.get(6).map_err(ProjectCatalogError::Read)?;
    Ok(RecentProject::new(
        project_id,
        repository_id,
        worktree_id,
        display,
        parse_head(&kind, object_id, reference)?,
    ))
}

fn parse_head(
    kind: &str,
    object_id: Option<String>,
    reference: Option<String>,
) -> Result<GitHead, ProjectCatalogError> {
    match (kind, object_id, reference) {
        ("born", Some(object_id), reference) => Ok(GitHead::Born {
            object_id: GitObjectId::try_from_hex(object_id)
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
            reference: reference
                .map(GitReferenceName::try_from_full_name)
                .transpose()
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
        }),
        ("unborn", None, Some(reference)) => Ok(GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name(reference)
                .map_err(|_| ProjectCatalogError::InvalidStoredData)?,
        }),
        _ => Err(ProjectCatalogError::InvalidStoredData),
    }
}

fn read_stable_id(row: &libsql::Row, index: i32) -> Result<[u8; 32], ProjectCatalogError> {
    let bytes: Vec<u8> = row.get(index).map_err(ProjectCatalogError::Read)?;
    bytes
        .try_into()
        .map_err(|_| ProjectCatalogError::InvalidStoredData)
}

fn read_optional_stable_id(
    row: &libsql::Row,
    index: i32,
) -> Result<Option<[u8; 32]>, ProjectCatalogError> {
    let bytes: Option<Vec<u8>> = row.get(index).map_err(ProjectCatalogError::Read)?;
    bytes
        .map(|value| {
            value
                .try_into()
                .map_err(|_| ProjectCatalogError::InvalidStoredData)
        })
        .transpose()
}

fn revision_from_i64(value: i64) -> Result<ProjectCatalogRevision, ProjectCatalogError> {
    let value = u64::try_from(value).map_err(|_| ProjectCatalogError::InvalidStoredData)?;
    ProjectCatalogRevision::new(value).map_err(|_| ProjectCatalogError::InvalidStoredData)
}

fn revision_to_i64(revision: ProjectCatalogRevision) -> Result<i64, ProjectCatalogError> {
    i64::try_from(revision.get()).map_err(|_| ProjectCatalogError::SequenceExhausted)
}

const fn evidence_name(evidence: ProjectReconciliationEvidence) -> &'static str {
    match evidence {
        ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor => "repository-anchor",
        ProjectReconciliationEvidence::RemoteAndWorktreeAnchor => "remote-anchor",
    }
}

fn parse_evidence_name(value: &str) -> Result<ProjectReconciliationEvidence, ProjectCatalogError> {
    match value {
        "repository-anchor" => Ok(ProjectReconciliationEvidence::RepositoryAndWorktreeAnchor),
        "remote-anchor" => Ok(ProjectReconciliationEvidence::RemoteAndWorktreeAnchor),
        _ => Err(ProjectCatalogError::InvalidStoredData),
    }
}

fn derive_project_id(repository_id: RepositoryId) -> ProjectId {
    let mut hasher = Hasher::new();
    hasher.update(PROJECT_ID_VERSION);
    hasher.update(repository_id.as_bytes());
    ProjectId::from_bytes(*hasher.finalize().as_bytes())
}

struct EncodedPath {
    encoding: &'static str,
    bytes: Vec<u8>,
}

#[cfg(unix)]
fn decode_path(encoding: &str, bytes: Vec<u8>) -> Result<PathBuf, ProjectCatalogError> {
    use std::os::unix::ffi::OsStringExt;

    if encoding != "unix-bytes-v1" || bytes.is_empty() {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(windows)]
fn decode_path(encoding: &str, bytes: Vec<u8>) -> Result<PathBuf, ProjectCatalogError> {
    use std::os::windows::ffi::OsStringExt;

    if encoding != "windows-utf16le-v1" || bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    let wide = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}

#[cfg(not(any(unix, windows)))]
fn decode_path(encoding: &str, bytes: Vec<u8>) -> Result<PathBuf, ProjectCatalogError> {
    if encoding != "utf8-lossy-v1" || bytes.is_empty() {
        return Err(ProjectCatalogError::InvalidStoredData);
    }
    String::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|_| ProjectCatalogError::InvalidStoredData)
}

#[cfg(unix)]
fn encode_path(path: &Path) -> EncodedPath {
    use std::os::unix::ffi::OsStrExt;

    EncodedPath {
        encoding: "unix-bytes-v1",
        bytes: path.as_os_str().as_bytes().to_vec(),
    }
}

#[cfg(windows)]
fn encode_path(path: &Path) -> EncodedPath {
    use std::os::windows::ffi::OsStrExt;

    EncodedPath {
        encoding: "windows-utf16le-v1",
        bytes: path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect(),
    }
}

#[cfg(not(any(unix, windows)))]
fn encode_path(path: &Path) -> EncodedPath {
    EncodedPath {
        encoding: "utf8-lossy-v1",
        bytes: path.to_string_lossy().into_owned().into_bytes(),
    }
}

struct HeadFields {
    kind: &'static str,
    object_id: Option<String>,
    reference: Option<String>,
}

impl From<&GitHead> for HeadFields {
    fn from(head: &GitHead) -> Self {
        match head {
            GitHead::Born {
                object_id,
                reference,
            } => Self {
                kind: "born",
                object_id: Some(object_id.as_str().to_owned()),
                reference: reference
                    .as_ref()
                    .map(|reference| reference.as_str().to_owned()),
            },
            GitHead::Unborn { reference } => Self {
                kind: "unborn",
                object_id: None,
                reference: Some(reference.as_str().to_owned()),
            },
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProjectCatalogError {
    Open(CatalogOpenError),
    Begin(libsql::Error),
    Read(libsql::Error),
    Write(libsql::Error),
    Rollback(libsql::Error),
    Commit(libsql::Error),
    InvalidStoredData,
    IdentityConflict,
    NotFound,
    SequenceExhausted,
}

impl ProjectCatalogError {
    pub(crate) fn classify(self) -> KnowledgeStoreFailure {
        match self {
            Self::Open(
                CatalogOpenError::CorruptDatabase | CatalogOpenError::IntegrityCheckFailed,
            ) => KnowledgeStoreFailure::Corrupt,
            Self::Read(ref source) | Self::Write(ref source) if is_corruption(source) => {
                KnowledgeStoreFailure::Corrupt
            }
            Self::Open(CatalogOpenError::NewerSchema { .. }) => {
                KnowledgeStoreFailure::UnsupportedSchema
            }
            Self::Open(
                CatalogOpenError::MigrationHistoryMismatch { .. }
                | CatalogOpenError::UnexpectedSchemaVersion { .. }
                | CatalogOpenError::ConnectionPolicyMismatch,
            )
            | Self::InvalidStoredData => KnowledgeStoreFailure::InvalidStoredData,
            Self::IdentityConflict => KnowledgeStoreFailure::IdentityConflict,
            Self::NotFound => KnowledgeStoreFailure::InvalidStoredData,
            Self::Write(ref source) if sqlite_primary_code(source) == Some(SQLITE_CONSTRAINT) => {
                KnowledgeStoreFailure::IdentityConflict
            }
            Self::Open(_)
            | Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Rollback(_)
            | Self::Commit(_)
            | Self::SequenceExhausted => KnowledgeStoreFailure::Unavailable,
        }
    }

    pub(crate) fn classify_admin(self) -> ProjectCatalogAdminFailure {
        match self {
            Self::Open(
                CatalogOpenError::CorruptDatabase | CatalogOpenError::IntegrityCheckFailed,
            ) => ProjectCatalogAdminFailure::Corrupt,
            Self::Read(ref source) | Self::Write(ref source) if is_corruption(source) => {
                ProjectCatalogAdminFailure::Corrupt
            }
            Self::Open(CatalogOpenError::NewerSchema { .. }) => {
                ProjectCatalogAdminFailure::UnsupportedSchema
            }
            Self::Open(
                CatalogOpenError::MigrationHistoryMismatch { .. }
                | CatalogOpenError::UnexpectedSchemaVersion { .. }
                | CatalogOpenError::ConnectionPolicyMismatch,
            )
            | Self::InvalidStoredData => ProjectCatalogAdminFailure::InvalidStoredData,
            Self::IdentityConflict => ProjectCatalogAdminFailure::IdentityConflict,
            Self::NotFound => ProjectCatalogAdminFailure::NotFound,
            Self::Write(ref source) if sqlite_primary_code(source) == Some(SQLITE_CONSTRAINT) => {
                ProjectCatalogAdminFailure::IdentityConflict
            }
            Self::Open(_)
            | Self::Begin(_)
            | Self::Read(_)
            | Self::Write(_)
            | Self::Rollback(_)
            | Self::Commit(_)
            | Self::SequenceExhausted => ProjectCatalogAdminFailure::Unavailable,
        }
    }
}

impl fmt::Display for ProjectCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(_) => formatter.write_str("could not open a catalog operation connection"),
            Self::Begin(_) => formatter.write_str("could not begin a catalog write transaction"),
            Self::Read(_) => formatter.write_str("could not read project catalog data"),
            Self::Write(_) => formatter.write_str("could not write project catalog data"),
            Self::Rollback(_) => formatter.write_str("could not roll back project catalog data"),
            Self::Commit(_) => formatter.write_str("could not commit project catalog data"),
            Self::InvalidStoredData => formatter.write_str("project catalog data is invalid"),
            Self::IdentityConflict => formatter.write_str("project catalog identity conflicts"),
            Self::NotFound => formatter.write_str("project is not in the recent-project list"),
            Self::SequenceExhausted => formatter.write_str("project open sequence is exhausted"),
        }
    }
}

impl Error for ProjectCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Open(source) => Some(source),
            Self::Begin(source)
            | Self::Read(source)
            | Self::Write(source)
            | Self::Rollback(source)
            | Self::Commit(source) => Some(source),
            Self::InvalidStoredData
            | Self::IdentityConflict
            | Self::NotFound
            | Self::SequenceExhausted => None,
        }
    }
}

fn sqlite_primary_code(error: &libsql::Error) -> Option<i32> {
    match error {
        libsql::Error::SqliteFailure(code, _) => Some(code & 0xff),
        _ => None,
    }
}

fn is_corruption(error: &libsql::Error) -> bool {
    matches!(
        sqlite_primary_code(error),
        Some(SQLITE_CORRUPT | SQLITE_NOT_A_DATABASE)
    )
}

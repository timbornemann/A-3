use crate::project_catalog::ProjectCatalogError;
use crate::{
    CatalogDatabase, CatalogOpenError, KnowledgeDatabase, KnowledgeOpenError,
    ProjectStorageLayoutError, StorageLayout,
};
use crate::{
    agent_recovery_repository, command_allowlist_repository, exact_search_repository,
    goal_contract_repository, graph_traversal_repository, index_publication, index_repository,
    index_repository::IndexRepositoryError, lexical_search_repository,
    module_card_detail_repository, module_card_freshness_repository, module_card_repository,
    module_dependency_graph_repository, module_remap_queue_repository, module_runtime_repository,
    module_tree_repository, policy_repository, repository_tree_repository, run_journal_repository,
    semantic_embedding_repository, task_ledger_repository, task_lens_claim_repository,
    verification_evidence_repository,
};
use a3_application::{
    AgentActionStore, AgentActionStoreFailure, AgentActionStoreFuture, AgentControllerControl,
    AgentMutationResultRecord, AgentRecoveryChoice, AgentRecoveryStore, AgentRecoveryStoreFailure,
    AgentRecoveryStoreFuture, CommandAllowlistStore, CommandAllowlistStoreFailure,
    CommandAllowlistStoreFuture, CommandAllowlistStoreVersion, EmbeddingOperationControl,
    EvaluatedPolicyAction, GoalContractStore, GoalContractStoreFailure, GoalContractStoreFuture,
    IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeIndexFuture, KnowledgeIndexStore,
    KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ModuleCardDetailControl,
    ModuleCardDetailFailure, ModuleCardDetailFuture, ModuleCardDetailQuery, ModuleCardDetailStore,
    ModuleCardFreshnessControl, ModuleCardFreshnessFailure, ModuleCardFreshnessFuture,
    ModuleCardFreshnessStore, ModuleCardPublicationTimeout, ModuleCardVerificationControl,
    ModuleDependencyGraphControl, ModuleDependencyGraphFailure, ModuleDependencyGraphFuture,
    ModuleDependencyGraphQuery, ModuleDependencyGraphStore, ModuleRemapQueueFailure,
    ModuleRemapQueueFuture, ModuleRemapQueueStore, ModuleRuntimeControl, ModuleRuntimeFailure,
    ModuleRuntimeFlowQuery, ModuleRuntimeFlowRootValidation, ModuleRuntimeFuture,
    ModuleRuntimeMapLoadResult, ModuleRuntimeMapQuery, ModuleRuntimeStore, ModuleTreeControl,
    ModuleTreeFailure, ModuleTreeFuture, ModuleTreeQuery, ModuleTreeStore, PolicyStore,
    PolicyStoreFailure, PolicyStoreFuture, ProjectCatalogAdmin, ProjectCatalogAdminFuture,
    ProjectOpenPreparation, ProjectReconciliationProposal, ProjectStorageControl,
    ProjectStorageFailure, ProjectStorageFuture, ProjectStorageStore, ProjectStorageUsage,
    RecentProject, RecentProjectLimit, RecordedAgentRead, RemapQueueControl, RemapQueueLimit,
    RepositoryTreeControl, RepositoryTreeFailure, RepositoryTreeFuture, RepositoryTreeQuery,
    RepositoryTreeStore, RunEventPage, RunEventPageLimit, RunJournalStore, RunJournalStoreFailure,
    RunJournalStoreFuture, SemanticCacheRebuildControl, SemanticEmbeddingStore,
    SemanticEmbeddingStoreFailure, SemanticEmbeddingStoreFuture, StoredProjectCommandAllowlist,
    TaskLedgerStore, TaskLedgerStoreFailure, TaskLedgerStoreFuture, TaskLedgerStoreVersion,
    TaskLensClaimLimit, TaskLensClaimReadFuture, TaskLensClaimStore, TaskLensClaimStoreFailure,
    TaskLensClaimStoreFuture, TaskLensControl, TaskLensIndexStore, TaskLensIndexStoreFuture,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure, VerificationEvidenceStoreFuture,
    VerifiedModuleCardPublisher, VerifiedModuleCardPublisherFuture,
};
use a3_domain::{
    AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind, AgentRun, AgentRunId,
    AgentRunTimestamp, AgentToolAttempt, AgentToolAttemptNumber, AgentToolAttemptStatus,
    AgentToolEvidence, ApprovalGrant, ApprovalGrantState, ApprovalId, ApprovalRequest,
    ApprovalRequestId, EmbeddingCacheKey, EmbeddingModelProfile, EmbeddingVector,
    ExactSearchCursor, ExactSearchPage, ExactSearchPageSize, ExactSearchQuery, GoalContract,
    GoalContractRevision, GraphTraversalResult, IndexPublication, IndexRunId, IndexRunRecord,
    IndexRunStart, IndexRunTerminalOutcome, LexicalSearchCursor, LexicalSearchPage,
    LexicalSearchPageSize, LexicalSearchQuery, ModuleCardClaimId, MutationActionFingerprint,
    PolicyDecision, PolicyDecisionId, ProjectCommandAllowlist, ProjectId, ProjectIdentity,
    PublishedIndex, RepositoryId, RunEvent, RunEventSequence, SemanticEmbedding, Snapshot,
    SnapshotId, TaskEvidenceId, TaskId, TaskLedger, ToolRunId, TraversalQuery,
    VectorSearchCapability, VectorSearchLimit, VectorSearchResult, VerificationEvidence,
    VerifiedModuleCardBatch, WorktreeId,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const MAX_SEARCH_DATABASES: usize = 4;
const MAX_MUTATION_DATABASES: usize = 4;
const MAX_PUBLISHED_INDEX_CACHE_ENTRIES: usize = 1;

/// Local libSQL implementation of the application knowledge-store boundary.
///
/// A project is recorded in the global catalog only after its identity-bound
/// per-worktree database has been opened and verified successfully.
pub struct LibsqlKnowledgeStore {
    layout: StorageLayout,
    catalog: CatalogDatabase,
    reconciliation_active: AtomicBool,
    mutation_databases: Mutex<Vec<Arc<KnowledgeDatabase>>>,
    search_databases: Mutex<Vec<Arc<KnowledgeDatabase>>>,
    published_indexes: Mutex<Vec<CachedPublishedIndex>>,
}

impl LibsqlKnowledgeStore {
    /// Opens the global catalog and retains the validated app-data layout used
    /// to derive private per-worktree database paths.
    pub async fn open(layout: &StorageLayout) -> Result<Self, CatalogOpenError> {
        let catalog = CatalogDatabase::open(layout).await?;
        Ok(Self {
            layout: layout.clone(),
            catalog,
            reconciliation_active: AtomicBool::new(false),
            mutation_databases: Mutex::new(Vec::new()),
            search_databases: Mutex::new(Vec::new()),
            published_indexes: Mutex::new(Vec::new()),
        })
    }
}

impl std::fmt::Debug for LibsqlKnowledgeStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LibsqlKnowledgeStore")
            .field("layout", &self.layout)
            .field("catalog", &self.catalog)
            .finish()
    }
}

impl KnowledgeStore for LibsqlKnowledgeStore {
    fn prepare_project_open<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeStoreFuture<'a, ProjectOpenPreparation> {
        Box::pin(async move {
            let preparation = self
                .catalog
                .prepare_project_open(project)
                .await
                .map_err(ProjectCatalogError::classify)?;
            match &preparation {
                ProjectOpenPreparation::Ready => Ok(preparation),
                ProjectOpenPreparation::ConfirmationRequired(proposal) => {
                    let source = self
                        .layout
                        .existing_project(proposal.previous_worktree_id())
                        .map_err(classify_project_layout_error)?;
                    let target = self
                        .layout
                        .existing_project(project.worktree().id())
                        .map_err(classify_project_layout_error)?;
                    match (source, target) {
                        (Some(source), None) => {
                            KnowledgeDatabase::preflight_reconciliation(
                                &source,
                                proposal.previous_repository_id(),
                                proposal.previous_worktree_id(),
                                project,
                            )
                            .await
                            .map_err(classify_knowledge_open_error)?;
                            Ok(preparation)
                        }
                        (None, None) => Ok(ProjectOpenPreparation::Ready),
                        _ => Err(KnowledgeStoreFailure::IdentityConflict),
                    }
                }
                ProjectOpenPreparation::ResumeConfirmed(proposal) => {
                    let source = self
                        .layout
                        .existing_project(proposal.previous_worktree_id())
                        .map_err(classify_project_layout_error)?;
                    let target = self
                        .layout
                        .existing_project(project.worktree().id())
                        .map_err(classify_project_layout_error)?;
                    match (source, target) {
                        (Some(layout), None) | (None, Some(layout)) => {
                            KnowledgeDatabase::preflight_reconciliation(
                                &layout,
                                proposal.previous_repository_id(),
                                proposal.previous_worktree_id(),
                                project,
                            )
                            .await
                            .map_err(classify_knowledge_open_error)?;
                            Ok(preparation)
                        }
                        (Some(_), Some(_)) => Err(KnowledgeStoreFailure::IdentityConflict),
                        (None, None) => Err(KnowledgeStoreFailure::InvalidStoredData),
                    }
                }
            }
        })
    }

    fn record_opened_project<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeStoreFuture<'a, ProjectId> {
        Box::pin(async move {
            let project_layout = self
                .layout
                .prepare_project(project.worktree())
                .map_err(classify_project_layout_error)?;
            let knowledge = Arc::new(
                KnowledgeDatabase::open(&project_layout, project)
                    .await
                    .map_err(classify_knowledge_open_error)?,
            );
            let project_id = self
                .catalog
                .record_project(project)
                .await
                .map_err(ProjectCatalogError::classify)?;
            self.cache_mutation_database(knowledge);
            Ok(project_id)
        })
    }

    fn reconcile_project<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        proposal: &'a ProjectReconciliationProposal,
    ) -> KnowledgeStoreFuture<'a, ProjectId> {
        Box::pin(async move {
            let _permit = self.acquire_reconciliation()?;
            self.clear_cached_project_state();
            let source = self
                .layout
                .existing_project(proposal.previous_worktree_id())
                .map_err(classify_project_layout_error)?;
            let target = self
                .layout
                .existing_project(project.worktree().id())
                .map_err(classify_project_layout_error)?;
            let existing = match (source, target) {
                (Some(layout), None) | (None, Some(layout)) => layout,
                (Some(_), Some(_)) => return Err(KnowledgeStoreFailure::IdentityConflict),
                (None, None) => return Err(KnowledgeStoreFailure::InvalidStoredData),
            };
            KnowledgeDatabase::preflight_reconciliation(
                &existing,
                proposal.previous_repository_id(),
                proposal.previous_worktree_id(),
                project,
            )
            .await
            .map_err(classify_knowledge_open_error)?;
            self.catalog
                .prepare_reconciliation(project, proposal)
                .await
                .map_err(ProjectCatalogError::classify)?;
            let target_layout = self
                .layout
                .relocate_project(proposal.previous_worktree_id(), project.worktree())
                .map_err(classify_project_layout_error)?;
            let knowledge = Arc::new(
                KnowledgeDatabase::reconcile_identity(
                    &target_layout,
                    proposal.previous_repository_id(),
                    proposal.previous_worktree_id(),
                    project,
                )
                .await
                .map_err(classify_knowledge_open_error)?,
            );
            let project_id = self
                .catalog
                .complete_reconciliation(project, proposal)
                .await
                .map_err(ProjectCatalogError::classify)?;
            self.cache_mutation_database(knowledge);
            Ok(project_id)
        })
    }

    fn list_recent_projects(
        &self,
        limit: RecentProjectLimit,
    ) -> KnowledgeStoreFuture<'_, Vec<RecentProject>> {
        Box::pin(async move {
            self.catalog
                .read_recent_projects(limit)
                .await
                .map_err(ProjectCatalogError::classify)
        })
    }
}

impl ProjectCatalogAdmin for LibsqlKnowledgeStore {
    fn remove_recent_worktree<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        project_id: ProjectId,
    ) -> ProjectCatalogAdminFuture<'a, ()> {
        Box::pin(async move {
            self.catalog
                .remove_recent_worktree(project, project_id)
                .await
                .map_err(ProjectCatalogError::classify_admin)
        })
    }
}

impl ProjectStorageStore for LibsqlKnowledgeStore {
    fn measure_project_storage<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn ProjectStorageControl,
    ) -> ProjectStorageFuture<'a, ProjectStorageUsage> {
        Box::pin(async move {
            let layout = self
                .layout
                .existing_project(project.worktree().id())
                .map_err(classify_project_storage_layout_error)?
                .ok_or(ProjectStorageFailure::InvalidLayout)?;
            layout.measure_usage(control)
        })
    }
}

impl GoalContractStore for LibsqlKnowledgeStore {
    fn create_goal_contract<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        contract: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_goal_contract(project)
                .await?;
            goal_contract_repository::create(
                database.connection(),
                project.worktree().id(),
                contract,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn append_goal_contract_revision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        contract: &'a GoalContract,
    ) -> GoalContractStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_goal_contract(project)
                .await?;
            goal_contract_repository::append_revision(
                database.connection(),
                project.worktree().id(),
                contract,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_current_goal_contract<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_goal_contract(project)
                .await?;
            goal_contract_repository::load_current(
                database.connection(),
                project.worktree().id(),
                task_id,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_goal_contract_revision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
        revision: GoalContractRevision,
    ) -> GoalContractStoreFuture<'a, Option<GoalContract>> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_goal_contract(project)
                .await?;
            goal_contract_repository::load_revision(
                database.connection(),
                project.worktree().id(),
                task_id,
                revision,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl TaskLedgerStore for LibsqlKnowledgeStore {
    fn create_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_task_ledger(project).await?;
            task_ledger_repository::create(database.connection(), project.worktree().id(), ledger)
                .await
                .map_err(|error| error.classify())
        })
    }

    fn replace_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_version: TaskLedgerStoreVersion,
        ledger: &'a TaskLedger,
    ) -> TaskLedgerStoreFuture<'a, TaskLedgerStoreVersion> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_task_ledger(project).await?;
            task_ledger_repository::replace(
                database.connection(),
                project.worktree().id(),
                expected_version,
                ledger,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_task_ledger<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
    ) -> TaskLedgerStoreFuture<'a, Option<a3_application::StoredTaskLedger>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_task_ledger(project).await?;
            task_ledger_repository::load(database.connection(), project.worktree().id(), task_id)
                .await
                .map_err(|error| error.classify())
        })
    }
}

impl RunJournalStore for LibsqlKnowledgeStore {
    fn create_agent_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run: &'a AgentRun,
        start_event: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_run_journal(project).await?;
            run_journal_repository::create(
                database.connection(),
                project.worktree().id(),
                run,
                start_event,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn append_run_event<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_run_journal(project).await?;
            run_journal_repository::append(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                event,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn append_agent_read<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        read: &'a RecordedAgentRead,
    ) -> RunJournalStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_run_journal(project).await?;
            run_journal_repository::append_agent_read(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                read,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_agent_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
    ) -> RunJournalStoreFuture<'a, Option<AgentRun>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_run_journal(project).await?;
            run_journal_repository::load_run(database.connection(), project.worktree().id(), run_id)
                .await
                .map_err(|error| error.classify())
        })
    }

    fn load_run_events<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        after_sequence: Option<RunEventSequence>,
        limit: RunEventPageLimit,
    ) -> RunJournalStoreFuture<'a, RunEventPage> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_run_journal(project).await?;
            run_journal_repository::load_events(
                database.connection(),
                project.worktree().id(),
                run_id,
                after_sequence,
                limit,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl PolicyStore for LibsqlKnowledgeStore {
    fn record_policy_evaluation<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        evaluation: &'a EvaluatedPolicyAction,
    ) -> PolicyStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::record_evaluation(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                evaluation,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_approval_request<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request_id: ApprovalRequestId,
    ) -> PolicyStoreFuture<'a, Option<ApprovalRequest>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::load_request(database.connection(), request_id)
                .await
                .map_err(|error| error.classify())
        })
    }

    fn load_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        approval_id: ApprovalId,
    ) -> PolicyStoreFuture<'a, Option<ApprovalGrant>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::load_approval(database.connection(), approval_id)
                .await
                .map_err(|error| error.classify())
        })
    }

    fn load_policy_decision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        decision_id: PolicyDecisionId,
    ) -> PolicyStoreFuture<'a, Option<PolicyDecision>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::load_decision(database.connection(), decision_id)
                .await
                .map_err(|error| error.classify())
        })
    }

    fn grant_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        approval: &'a ApprovalGrant,
        event: &'a RunEvent,
    ) -> PolicyStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::grant(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                approval,
                event,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn revoke_approval<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        expected_state: ApprovalGrantState,
        approval: &'a ApprovalGrant,
        event: &'a RunEvent,
    ) -> PolicyStoreFuture<'a, ()> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_policy(project).await?;
            policy_repository::revoke(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                expected_state,
                approval,
                event,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl CommandAllowlistStore for LibsqlKnowledgeStore {
    fn load_current<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> CommandAllowlistStoreFuture<'a, Option<StoredProjectCommandAllowlist>> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_command_allowlist(project)
                .await?;
            command_allowlist_repository::load_current(
                database.connection(),
                project.worktree().id(),
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn append<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected: Option<CommandAllowlistStoreVersion>,
        confirmation: &'a ProjectCommandAllowlist,
    ) -> CommandAllowlistStoreFuture<'a, StoredProjectCommandAllowlist> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_command_allowlist(project)
                .await?;
            command_allowlist_repository::append(
                database.connection(),
                project.worktree().id(),
                expected,
                confirmation,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl AgentRecoveryStore for LibsqlKnowledgeStore {
    fn begin_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        started_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::begin_tool_attempt(
                database.connection(),
                project.worktree().id(),
                run_id,
                snapshot_id,
                tool_run_id,
                started_at,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn begin_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        fingerprint: MutationActionFingerprint,
        kind: AgentMutationKind,
        started_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::begin_mutation_attempt(
                database.connection(),
                project.worktree().id(),
                run_id,
                snapshot_id,
                tool_run_id,
                fingerprint,
                kind,
                started_at,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn finish_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        status: AgentToolAttemptStatus,
        finished_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::finish_tool_attempt(
                database.connection(),
                project.worktree().id(),
                tool_run_id,
                attempt,
                status,
                finished_at,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn finish_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        status: AgentToolAttemptStatus,
        disposition: AgentMutationDisposition,
        finished_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::finish_mutation_attempt(
                database.connection(),
                project.worktree().id(),
                tool_run_id,
                attempt,
                status,
                disposition,
                finished_at,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn complete_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::complete_tool_attempt(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                event,
                tool_run_id,
                attempt,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn complete_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        result: AgentMutationResultRecord,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::complete_mutation_attempt(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                event,
                tool_run_id,
                attempt,
                result,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn interrupt_agent_tool_attempts<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        interrupted_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, u32> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::interrupt_tool_attempts(
                database.connection(),
                project.worktree().id(),
                run_id,
                interrupted_at,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_agent_mutation_attempts<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
    ) -> AgentRecoveryStoreFuture<'a, Vec<AgentMutationAttempt>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::load_mutation_attempts(
                database.connection(),
                project.worktree().id(),
                run_id,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn reconcile_agent_mutation<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::reconcile_mutation(
                database.connection(),
                project.worktree().id(),
                expected_last_sequence,
                run,
                event,
                tool_run_id,
                attempt,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_agent_tool_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        evidence_ids: &'a [TaskEvidenceId],
    ) -> AgentRecoveryStoreFuture<'a, Vec<AgentToolEvidence>> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::load_tool_evidence(
                database.connection(),
                project.worktree().id(),
                run_id,
                evidence_ids,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn commit_agent_recovery<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        choice: AgentRecoveryChoice,
        expected_published_snapshot: SnapshotId,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentRecoveryStoreFuture<'a, TaskLedgerStoreVersion> {
        Box::pin(async move {
            let database = self.open_project_knowledge_for_recovery(project).await?;
            agent_recovery_repository::commit_recovery(
                database.connection(),
                project.worktree().id(),
                choice,
                expected_published_snapshot,
                expected_ledger_version,
                expected_last_sequence,
                ledger,
                run,
                event,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl AgentActionStore for LibsqlKnowledgeStore {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        Box::pin(async move {
            let database = self
                .open_project_knowledge_for_run_journal(project)
                .await
                .map_err(classify_run_journal_for_agent_action)?;
            run_journal_repository::append_ledger_action(
                database.connection(),
                project.worktree().id(),
                expected_ledger_version,
                expected_last_sequence,
                ledger,
                run,
                event,
            )
            .await
            .map_err(|error| error.classify_agent_action())
        })
    }
}

impl KnowledgeIndexStore for LibsqlKnowledgeStore {
    fn append_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot: &'a Snapshot,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::append_snapshot(
                knowledge.connection(),
                project.worktree().id(),
                snapshot,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_snapshot<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<Snapshot>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_snapshot(knowledge.connection(), project.worktree().id())
                .await
                .map_err(IndexRepositoryError::classify)
        })
    }

    fn current_file_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, a3_domain::RepositoryFileState> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::current_file_state(knowledge.connection(), project.worktree().id())
                .await
                .map_err(IndexRepositoryError::classify)
        })
    }

    fn start_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        request: IndexRunStart,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::start_index_run(
                knowledge.connection(),
                project.worktree().id(),
                request,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn finish_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        outcome: IndexRunTerminalOutcome,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::finish_index_run(
                knowledge.connection(),
                project.worktree().id(),
                run_id,
                outcome,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn publish_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: IndexRunId,
        publication: &'a IndexPublication,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, IndexRunRecord> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            let published = index_publication::publish_index(
                knowledge.connection(),
                project.worktree().id(),
                run_id,
                publication,
                control,
            )
            .await
            .map_err(|error| error.classify())?;
            let record = published.run();
            self.cache_published_index(project, published);
            Ok(record)
        })
    }

    fn latest_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_index_run(
                knowledge.connection(),
                project.worktree().id(),
                false,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_published_index_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> KnowledgeIndexFuture<'a, Option<IndexRunRecord>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_repository::latest_index_run(
                knowledge.connection(),
                project.worktree().id(),
                true,
            )
            .await
            .map_err(IndexRepositoryError::classify)
        })
    }

    fn latest_published_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, Option<PublishedIndex>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_publication::latest_published_index(
                knowledge.connection(),
                project.worktree().id(),
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn rebuild_regenerable_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn IndexPersistenceControl,
    ) -> KnowledgeIndexFuture<'a, ()> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge(project).await?;
            index_publication::rebuild_regenerable_index(
                knowledge.connection(),
                project.worktree().id(),
                control,
            )
            .await
            .map_err(|error| error.classify())?;
            self.remove_cached_published_index(project);
            Ok(())
        })
    }
}

impl VerificationEvidenceStore for LibsqlKnowledgeStore {
    fn append_verification_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        evidence: &'a VerificationEvidence,
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, ()> {
        Box::pin(async move {
            if timeout.is_zero() {
                return Err(VerificationEvidenceStoreFailure::TimedOut);
            }
            let operation = VerificationIndexControl::new(control, timeout);
            let knowledge = self
                .open_project_knowledge(project)
                .await
                .map_err(classify_verification_index_failure)?;
            operation.checkpoint()?;
            let remaining = timeout
                .checked_sub(operation.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or(VerificationEvidenceStoreFailure::TimedOut)?;
            verification_evidence_repository::append(
                knowledge.connection(),
                project.worktree().id(),
                evidence,
                remaining,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_verification_state<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        task_id: TaskId,
        evidence_ids: &'a [TaskEvidenceId],
        expected_snapshot_id: SnapshotId,
        timeout: Duration,
        control: &'a dyn AgentControllerControl,
    ) -> VerificationEvidenceStoreFuture<'a, a3_application::StoredVerificationState> {
        Box::pin(async move {
            if timeout.is_zero() {
                return Err(VerificationEvidenceStoreFailure::TimedOut);
            }
            let operation = VerificationIndexControl::new(control, timeout);
            let knowledge = self
                .open_project_knowledge(project)
                .await
                .map_err(classify_verification_index_failure)?;
            operation.checkpoint()?;
            let published = index_publication::latest_published_index(
                knowledge.connection(),
                project.worktree().id(),
                &operation,
            )
            .await
            .map_err(|error| operation.classify_index_failure(error.classify()))?
            .ok_or(VerificationEvidenceStoreFailure::InvalidStoredData)?;
            operation.checkpoint()?;
            let remaining = timeout
                .checked_sub(operation.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or(VerificationEvidenceStoreFailure::TimedOut)?;
            verification_evidence_repository::load_state(
                knowledge.connection(),
                verification_evidence_repository::VerificationStateQuery::new(
                    project.worktree().id(),
                    task_id,
                    evidence_ids,
                    expected_snapshot_id,
                    published,
                    remaining,
                ),
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl VerifiedModuleCardPublisher for LibsqlKnowledgeStore {
    fn publish<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        batch: &'a VerifiedModuleCardBatch,
        timeout: ModuleCardPublicationTimeout,
        control: &'a dyn ModuleCardVerificationControl,
    ) -> VerifiedModuleCardPublisherFuture<'a> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge(project)
                .await
                .map_err(|_| a3_application::VerifiedModuleCardPublisherFailure::Storage)?;
            module_card_repository::publish_verified_module_cards(
                knowledge.connection(),
                project.worktree().id(),
                batch,
                timeout,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl TaskLensIndexStore for LibsqlKnowledgeStore {
    fn load_current_index<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensIndexStoreFuture<'a> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(KnowledgeIndexFailure::Cancelled);
            }
            let knowledge = self
                .open_project_knowledge_for_task_lens_index(project)
                .await?;
            let latest = index_repository::latest_index_run(
                knowledge.connection(),
                project.worktree().id(),
                true,
            )
            .await
            .map_err(IndexRepositoryError::classify)?;
            if control.is_cancelled() {
                return Err(KnowledgeIndexFailure::Cancelled);
            }
            let Some(record) = latest else {
                self.remove_cached_published_index(project);
                return Ok(None);
            };
            if let Some(index) = self.shared_cached_published_index(project, record) {
                return Ok(Some(index));
            }
            let index_control = SharedIndexControl(control);
            let published = index_publication::latest_published_index(
                knowledge.connection(),
                project.worktree().id(),
                &index_control,
            )
            .await
            .map_err(|error| error.classify())?
            .ok_or(KnowledgeIndexFailure::InvalidIndexRunTransition)?;
            let shared = Arc::new(published);
            self.cache_shared_published_index(project, Arc::clone(&shared));
            Ok(Some(shared))
        })
    }
}

impl TaskLensClaimStore for LibsqlKnowledgeStore {
    fn load_claims<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        limit: TaskLensClaimLimit,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimStoreFuture<'a> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_task_lens(project).await?;
            task_lens_claim_repository::load_claims(
                knowledge.connection(),
                project.worktree().id(),
                published,
                limit.get(),
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn load_claim<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        published: &'a PublishedIndex,
        claim_id: ModuleCardClaimId,
        control: &'a dyn TaskLensControl,
    ) -> TaskLensClaimReadFuture<'a> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_task_lens(project).await?;
            task_lens_claim_repository::load_claim(
                knowledge.connection(),
                project.worktree().id(),
                published,
                claim_id,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleRemapQueueStore for LibsqlKnowledgeStore {
    fn load_pending<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        limit: RemapQueueLimit,
        control: &'a dyn RemapQueueControl,
    ) -> ModuleRemapQueueFuture<'a> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_remap_queue(project).await?;
            module_remap_queue_repository::load_pending(
                knowledge.connection(),
                project.worktree().id(),
                limit,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleCardFreshnessStore for LibsqlKnowledgeStore {
    fn load_module_card_freshness<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn ModuleCardFreshnessControl,
    ) -> ModuleCardFreshnessFuture<'a> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_module_card_freshness(project)
                .await?;
            module_card_freshness_repository::load(
                knowledge.connection(),
                project.worktree().id(),
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl RepositoryTreeStore for LibsqlKnowledgeStore {
    fn load_repository_tree_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a RepositoryTreeQuery,
        control: &'a dyn RepositoryTreeControl,
    ) -> RepositoryTreeFuture<'a> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_repository_tree(project)
                .await?;
            repository_tree_repository::load(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleTreeStore for LibsqlKnowledgeStore {
    fn load_module_tree_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleTreeQuery,
        control: &'a dyn ModuleTreeControl,
    ) -> ModuleTreeFuture<'a> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_module_tree(project).await?;
            module_tree_repository::load(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleCardDetailStore for LibsqlKnowledgeStore {
    fn load_module_card_detail<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleCardDetailQuery,
        control: &'a dyn ModuleCardDetailControl,
    ) -> ModuleCardDetailFuture<'a> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_module_card_detail(project)
                .await?;
            module_card_detail_repository::load(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleDependencyGraphStore for LibsqlKnowledgeStore {
    fn load_module_dependency_graph<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleDependencyGraphQuery,
        control: &'a dyn ModuleDependencyGraphControl,
    ) -> ModuleDependencyGraphFuture<'a> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_module_dependency_graph(project)
                .await?;
            module_dependency_graph_repository::load(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl ModuleRuntimeStore for LibsqlKnowledgeStore {
    fn load_module_runtime_map<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleRuntimeMapQuery,
        control: &'a dyn ModuleRuntimeControl,
    ) -> ModuleRuntimeFuture<'a, ModuleRuntimeMapLoadResult> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_module_runtime(project)
                .await?;
            module_runtime_repository::load_map(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn validate_module_runtime_flow_root<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleRuntimeFlowQuery,
        control: &'a dyn ModuleRuntimeControl,
    ) -> ModuleRuntimeFuture<'a, ModuleRuntimeFlowRootValidation> {
        Box::pin(async move {
            let knowledge = self
                .open_project_knowledge_for_module_runtime(project)
                .await?;
            module_runtime_repository::validate_flow_root(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }
}

impl KnowledgeSearchStore for LibsqlKnowledgeStore {
    fn search_exact<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ExactSearchQuery,
        page_size: ExactSearchPageSize,
        cursor: Option<&'a ExactSearchCursor>,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, ExactSearchPage> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_search(project).await?;
            exact_search_repository::search_exact(
                knowledge.connection(),
                project.worktree().id(),
                query,
                page_size,
                cursor,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn search_lexical<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a LexicalSearchQuery,
        page_size: LexicalSearchPageSize,
        cursor: Option<&'a LexicalSearchCursor>,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, LexicalSearchPage> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_search(project).await?;
            lexical_search_repository::search_lexical(
                knowledge.connection(),
                project.worktree().id(),
                query,
                page_size,
                cursor,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn traverse_graph<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a TraversalQuery,
        control: &'a dyn KnowledgeSearchControl,
    ) -> KnowledgeSearchFuture<'a, GraphTraversalResult> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_search(project).await?;
            graph_traversal_repository::traverse_graph(
                knowledge.connection(),
                project.worktree().id(),
                query,
                control,
            )
            .await
            .map_err(|error| error.classify(query))
        })
    }
}

impl SemanticEmbeddingStore for LibsqlKnowledgeStore {
    fn find_cached<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        keys: &'a [EmbeddingCacheKey],
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, Vec<EmbeddingCacheKey>> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_semantic(project).await?;
            semantic_embedding_repository::find_cached(
                knowledge.connection(),
                profile,
                keys,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn store_batch<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        embeddings: &'a [SemanticEmbedding],
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, ()> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_semantic(project).await?;
            semantic_embedding_repository::store_batch(
                knowledge.connection(),
                project.worktree().id(),
                profile,
                embeddings,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn vector_search_capability<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        profile: &'a EmbeddingModelProfile,
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchCapability> {
        Box::pin(async move {
            if !profile.has_compatible_identity() {
                return Err(SemanticEmbeddingStoreFailure::ProfileConflict);
            }
            let _knowledge = self.open_project_knowledge_for_semantic(project).await?;
            self.semantic_vector_capability(profile, control).await
        })
    }

    fn search_similar<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        snapshot_id: SnapshotId,
        profile: &'a EmbeddingModelProfile,
        query: &'a EmbeddingVector,
        limit: VectorSearchLimit,
        control: &'a dyn EmbeddingOperationControl,
    ) -> SemanticEmbeddingStoreFuture<'a, VectorSearchResult> {
        Box::pin(async move {
            let capability = self.semantic_vector_capability(profile, control).await?;
            let knowledge = self.open_project_knowledge_for_semantic(project).await?;
            semantic_embedding_repository::search_similar(
                knowledge.connection(),
                snapshot_id,
                profile,
                query,
                limit,
                capability,
                control,
            )
            .await
            .map_err(|error| error.classify())
        })
    }

    fn rebuild_semantic_cache<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        control: &'a dyn SemanticCacheRebuildControl,
    ) -> SemanticEmbeddingStoreFuture<'a, ()> {
        Box::pin(async move {
            let knowledge = self.open_project_knowledge_for_semantic(project).await?;
            semantic_embedding_repository::rebuild_semantic_cache(knowledge.connection(), control)
                .await
                .map_err(|error| error.classify())
        })
    }
}

impl LibsqlKnowledgeStore {
    async fn open_project_knowledge_for_goal_contract(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, GoalContractStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_goal_contract_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_goal_contract_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_task_ledger(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, TaskLedgerStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_task_ledger_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_task_ledger_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_run_journal(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, RunJournalStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_run_journal_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_run_journal_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_policy(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, PolicyStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_policy_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_policy_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_command_allowlist(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, CommandAllowlistStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_command_allowlist_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_command_allowlist_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_recovery(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, AgentRecoveryStoreFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(classify_agent_recovery_storage_failure)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(classify_agent_recovery_storage_failure)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    fn acquire_reconciliation(&self) -> Result<ReconciliationPermit<'_>, KnowledgeStoreFailure> {
        self.reconciliation_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| KnowledgeStoreFailure::Unavailable)?;
        Ok(ReconciliationPermit {
            active: &self.reconciliation_active,
        })
    }

    async fn open_project_knowledge(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, KnowledgeIndexFailure> {
        if let Some(database) =
            self.cached_mutation_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(KnowledgeIndexFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(KnowledgeIndexFailure::Storage)?,
        );
        Ok(self.cache_mutation_database(database))
    }

    async fn open_project_knowledge_for_search(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, KnowledgeSearchFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(KnowledgeSearchFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(KnowledgeSearchFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_semantic(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, SemanticEmbeddingStoreFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(SemanticEmbeddingStoreFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(SemanticEmbeddingStoreFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_task_lens(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, TaskLensClaimStoreFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(TaskLensClaimStoreFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(TaskLensClaimStoreFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_remap_queue(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleRemapQueueFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleRemapQueueFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleRemapQueueFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_module_card_freshness(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleCardFreshnessFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleCardFreshnessFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleCardFreshnessFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_repository_tree(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, RepositoryTreeFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(RepositoryTreeFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(RepositoryTreeFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_module_tree(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleTreeFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleTreeFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleTreeFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_module_card_detail(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleCardDetailFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleCardDetailFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleCardDetailFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_module_dependency_graph(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleDependencyGraphFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleDependencyGraphFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleDependencyGraphFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_module_runtime(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, ModuleRuntimeFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(ModuleRuntimeFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(ModuleRuntimeFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn open_project_knowledge_for_task_lens_index(
        &self,
        project: &ProjectIdentity,
    ) -> Result<Arc<KnowledgeDatabase>, KnowledgeIndexFailure> {
        if let Some(database) =
            self.cached_search_database(project.repository().id(), project.worktree().id())
        {
            return Ok(database);
        }
        let project_layout = self
            .layout
            .prepare_project(project.worktree())
            .map_err(classify_project_layout_error)
            .map_err(KnowledgeIndexFailure::Storage)?;
        let database = Arc::new(
            KnowledgeDatabase::open(&project_layout, project)
                .await
                .map_err(classify_knowledge_open_error)
                .map_err(KnowledgeIndexFailure::Storage)?,
        );
        Ok(self.cache_search_database(database))
    }

    async fn semantic_vector_capability(
        &self,
        profile: &EmbeddingModelProfile,
        control: &dyn EmbeddingOperationControl,
    ) -> Result<VectorSearchCapability, SemanticEmbeddingStoreFailure> {
        semantic_embedding_repository::probe_vector_capability(profile.dimension(), control)
            .await
            .map_err(|error| error.classify())
    }

    fn cached_search_database(
        &self,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
    ) -> Option<Arc<KnowledgeDatabase>> {
        let mut databases = lock_recovering_poison(&self.search_databases);
        let position = databases.iter().position(|database| {
            database.repository_id() == repository_id && database.worktree_id() == worktree_id
        })?;
        let database = databases.remove(position);
        databases.push(Arc::clone(&database));
        Some(database)
    }

    fn cached_mutation_database(
        &self,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
    ) -> Option<Arc<KnowledgeDatabase>> {
        let mut databases = lock_recovering_poison(&self.mutation_databases);
        let position = databases.iter().position(|database| {
            database.repository_id() == repository_id && database.worktree_id() == worktree_id
        })?;
        let database = databases.remove(position);
        databases.push(Arc::clone(&database));
        Some(database)
    }

    fn cache_mutation_database(&self, database: Arc<KnowledgeDatabase>) -> Arc<KnowledgeDatabase> {
        let mut databases = lock_recovering_poison(&self.mutation_databases);
        if let Some(position) = databases.iter().position(|cached| {
            cached.repository_id() == database.repository_id()
                && cached.worktree_id() == database.worktree_id()
        }) {
            let cached = databases.remove(position);
            databases.push(Arc::clone(&cached));
            return cached;
        }
        if databases.len() == MAX_MUTATION_DATABASES {
            databases.remove(0);
        }
        databases.push(Arc::clone(&database));
        database
    }

    fn cache_search_database(&self, database: Arc<KnowledgeDatabase>) -> Arc<KnowledgeDatabase> {
        let mut databases = lock_recovering_poison(&self.search_databases);
        if let Some(position) = databases.iter().position(|cached| {
            cached.repository_id() == database.repository_id()
                && cached.worktree_id() == database.worktree_id()
        }) {
            let cached = databases.remove(position);
            databases.push(Arc::clone(&cached));
            return cached;
        }
        if databases.len() == MAX_SEARCH_DATABASES {
            databases.remove(0);
        }
        databases.push(Arc::clone(&database));
        database
    }

    fn shared_cached_published_index(
        &self,
        project: &ProjectIdentity,
        record: IndexRunRecord,
    ) -> Option<Arc<PublishedIndex>> {
        let cached = {
            let mut indexes = lock_recovering_poison(&self.published_indexes);
            let position = indexes.iter().position(|entry| {
                entry.repository_id == project.repository().id()
                    && entry.worktree_id == project.worktree().id()
                    && entry.index.run() == record
            })?;
            let entry = indexes.remove(position);
            let index = Arc::clone(&entry.index);
            indexes.push(entry);
            index
        };
        Some(cached)
    }

    fn cache_published_index(&self, project: &ProjectIdentity, index: PublishedIndex) {
        self.cache_shared_published_index(project, Arc::new(index));
    }

    fn cache_shared_published_index(&self, project: &ProjectIdentity, index: Arc<PublishedIndex>) {
        let mut indexes = lock_recovering_poison(&self.published_indexes);
        indexes.retain(|entry| {
            entry.repository_id != project.repository().id()
                || entry.worktree_id != project.worktree().id()
        });
        indexes.push(CachedPublishedIndex {
            repository_id: project.repository().id(),
            worktree_id: project.worktree().id(),
            index,
        });
        if indexes.len() > MAX_PUBLISHED_INDEX_CACHE_ENTRIES {
            indexes.remove(0);
        }
    }

    fn remove_cached_published_index(&self, project: &ProjectIdentity) {
        let mut indexes = lock_recovering_poison(&self.published_indexes);
        indexes.retain(|entry| {
            entry.repository_id != project.repository().id()
                || entry.worktree_id != project.worktree().id()
        });
    }

    fn clear_cached_project_state(&self) {
        lock_recovering_poison(&self.mutation_databases).clear();
        lock_recovering_poison(&self.search_databases).clear();
        lock_recovering_poison(&self.published_indexes).clear();
    }
}

struct CachedPublishedIndex {
    repository_id: RepositoryId,
    worktree_id: WorktreeId,
    index: Arc<PublishedIndex>,
}

struct SharedIndexControl<'a>(&'a dyn TaskLensControl);

impl std::fmt::Debug for SharedIndexControl<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedIndexControl")
    }
}

impl IndexPersistenceControl for SharedIndexControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        if self.0.is_cancelled() {
            Err(a3_application::IndexPersistenceControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}

struct VerificationIndexControl<'a> {
    parent: &'a dyn AgentControllerControl,
    started: Instant,
    timeout: Duration,
}

impl<'a> VerificationIndexControl<'a> {
    fn new(parent: &'a dyn AgentControllerControl, timeout: Duration) -> Self {
        Self {
            parent,
            started: Instant::now(),
            timeout,
        }
    }

    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn checkpoint(&self) -> Result<(), VerificationEvidenceStoreFailure> {
        if self.parent.is_cancelled() {
            return Err(VerificationEvidenceStoreFailure::Cancelled);
        }
        if self.elapsed() >= self.timeout {
            return Err(VerificationEvidenceStoreFailure::TimedOut);
        }
        Ok(())
    }

    fn classify_index_failure(
        &self,
        failure: KnowledgeIndexFailure,
    ) -> VerificationEvidenceStoreFailure {
        if self.parent.is_cancelled() {
            VerificationEvidenceStoreFailure::Cancelled
        } else if self.elapsed() >= self.timeout {
            VerificationEvidenceStoreFailure::TimedOut
        } else {
            classify_verification_index_failure(failure)
        }
    }
}

impl std::fmt::Debug for VerificationIndexControl<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VerificationIndexControl")
    }
}

impl IndexPersistenceControl for VerificationIndexControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.parent.is_cancelled() || self.elapsed() >= self.timeout
    }

    fn report_progress(
        &self,
        _progress: a3_domain::Progress,
    ) -> Result<(), a3_application::IndexPersistenceControlError> {
        if self.is_cancelled() {
            Err(a3_application::IndexPersistenceControlError::Unavailable)
        } else {
            Ok(())
        }
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

struct ReconciliationPermit<'a> {
    active: &'a AtomicBool,
}

impl Drop for ReconciliationPermit<'_> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

fn classify_project_layout_error(error: ProjectStorageLayoutError) -> KnowledgeStoreFailure {
    match error {
        ProjectStorageLayoutError::SymbolicLink { .. }
        | ProjectStorageLayoutError::NotDirectory { .. }
        | ProjectStorageLayoutError::NotRegularFile { .. }
        | ProjectStorageLayoutError::OutsideParent { .. } => {
            KnowledgeStoreFailure::InvalidStoredData
        }
        ProjectStorageLayoutError::ReconciliationIdentityUnchanged
        | ProjectStorageLayoutError::ReconciliationSourceMissing(_)
        | ProjectStorageLayoutError::ReconciliationTargetExists(_) => {
            KnowledgeStoreFailure::IdentityConflict
        }
        ProjectStorageLayoutError::StorageInsideWorktree { .. }
        | ProjectStorageLayoutError::Create { .. }
        | ProjectStorageLayoutError::Inspect { .. }
        | ProjectStorageLayoutError::Canonicalize { .. }
        | ProjectStorageLayoutError::Move { .. } => KnowledgeStoreFailure::Unavailable,
    }
}

fn classify_project_storage_layout_error(
    error: ProjectStorageLayoutError,
) -> ProjectStorageFailure {
    match error {
        ProjectStorageLayoutError::Inspect { .. }
        | ProjectStorageLayoutError::Canonicalize { .. } => ProjectStorageFailure::Unavailable,
        ProjectStorageLayoutError::StorageInsideWorktree { .. }
        | ProjectStorageLayoutError::Create { .. }
        | ProjectStorageLayoutError::SymbolicLink { .. }
        | ProjectStorageLayoutError::NotDirectory { .. }
        | ProjectStorageLayoutError::NotRegularFile { .. }
        | ProjectStorageLayoutError::OutsideParent { .. }
        | ProjectStorageLayoutError::ReconciliationIdentityUnchanged
        | ProjectStorageLayoutError::ReconciliationSourceMissing(_)
        | ProjectStorageLayoutError::ReconciliationTargetExists(_)
        | ProjectStorageLayoutError::Move { .. } => ProjectStorageFailure::InvalidLayout,
    }
}

fn classify_knowledge_open_error(error: KnowledgeOpenError) -> KnowledgeStoreFailure {
    match error {
        KnowledgeOpenError::Layout(error) => classify_project_layout_error(error),
        KnowledgeOpenError::IntegrityCheckFailed | KnowledgeOpenError::CorruptDatabase => {
            KnowledgeStoreFailure::Corrupt
        }
        KnowledgeOpenError::NewerSchema { .. } => KnowledgeStoreFailure::UnsupportedSchema,
        KnowledgeOpenError::ConnectionPolicyMismatch
        | KnowledgeOpenError::MigrationHistoryMismatch { .. }
        | KnowledgeOpenError::InspectIdentity(_)
        | KnowledgeOpenError::InvalidStoredData
        | KnowledgeOpenError::UnexpectedSchemaVersion { .. } => {
            KnowledgeStoreFailure::InvalidStoredData
        }
        KnowledgeOpenError::IdentityConflict => KnowledgeStoreFailure::IdentityConflict,
        KnowledgeOpenError::BeginReconciliation(_)
        | KnowledgeOpenError::WriteReconciliation(_)
        | KnowledgeOpenError::RollbackReconciliation(_)
        | KnowledgeOpenError::CommitReconciliation(_) => KnowledgeStoreFailure::Unavailable,
        KnowledgeOpenError::Open(_)
        | KnowledgeOpenError::Connect(_)
        | KnowledgeOpenError::Configure(_)
        | KnowledgeOpenError::InspectConnectionPolicy(_)
        | KnowledgeOpenError::InspectIntegrity(_)
        | KnowledgeOpenError::InspectSchema(_)
        | KnowledgeOpenError::InspectMigrationHistory(_)
        | KnowledgeOpenError::BeginMigration { .. }
        | KnowledgeOpenError::ApplyMigration { .. }
        | KnowledgeOpenError::RollbackMigration { .. }
        | KnowledgeOpenError::CommitMigration { .. } => KnowledgeStoreFailure::Unavailable,
    }
}

const fn classify_verification_index_failure(
    failure: KnowledgeIndexFailure,
) -> VerificationEvidenceStoreFailure {
    match failure {
        KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::Unavailable)
        | KnowledgeIndexFailure::ProgressUnavailable => {
            VerificationEvidenceStoreFailure::Unavailable
        }
        KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::Corrupt) => {
            VerificationEvidenceStoreFailure::Corrupt
        }
        KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::UnsupportedSchema) => {
            VerificationEvidenceStoreFailure::UnsupportedSchema
        }
        KnowledgeIndexFailure::Cancelled => VerificationEvidenceStoreFailure::Cancelled,
        KnowledgeIndexFailure::TimedOut => VerificationEvidenceStoreFailure::TimedOut,
        KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::InvalidStoredData)
        | KnowledgeIndexFailure::Storage(KnowledgeStoreFailure::IdentityConflict)
        | KnowledgeIndexFailure::SnapshotConflict
        | KnowledgeIndexFailure::SnapshotNotFound
        | KnowledgeIndexFailure::IndexRunAlreadyActive
        | KnowledgeIndexFailure::IndexRunNotFound
        | KnowledgeIndexFailure::InvalidIndexRunTransition
        | KnowledgeIndexFailure::IndexPublicationMismatch
        | KnowledgeIndexFailure::IndexPublicationTooLarge => {
            VerificationEvidenceStoreFailure::InvalidStoredData
        }
    }
}

fn classify_goal_contract_storage_failure(
    error: KnowledgeStoreFailure,
) -> GoalContractStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => GoalContractStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => GoalContractStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => GoalContractStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData | KnowledgeStoreFailure::IdentityConflict => {
            GoalContractStoreFailure::InvalidStoredData
        }
    }
}

fn classify_task_ledger_storage_failure(error: KnowledgeStoreFailure) -> TaskLedgerStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => TaskLedgerStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => TaskLedgerStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => TaskLedgerStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData | KnowledgeStoreFailure::IdentityConflict => {
            TaskLedgerStoreFailure::InvalidStoredData
        }
    }
}

fn classify_run_journal_storage_failure(error: KnowledgeStoreFailure) -> RunJournalStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => RunJournalStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => RunJournalStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => RunJournalStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData | KnowledgeStoreFailure::IdentityConflict => {
            RunJournalStoreFailure::InvalidStoredData
        }
    }
}

fn classify_policy_storage_failure(error: KnowledgeStoreFailure) -> PolicyStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => PolicyStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => PolicyStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => PolicyStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData | KnowledgeStoreFailure::IdentityConflict => {
            PolicyStoreFailure::InvalidStoredData
        }
    }
}

fn classify_command_allowlist_storage_failure(
    error: KnowledgeStoreFailure,
) -> CommandAllowlistStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => CommandAllowlistStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => CommandAllowlistStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => CommandAllowlistStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData => CommandAllowlistStoreFailure::InvalidStoredData,
        KnowledgeStoreFailure::IdentityConflict => CommandAllowlistStoreFailure::ProjectMismatch,
    }
}

fn classify_agent_recovery_storage_failure(
    error: KnowledgeStoreFailure,
) -> AgentRecoveryStoreFailure {
    match error {
        KnowledgeStoreFailure::Unavailable => AgentRecoveryStoreFailure::Unavailable,
        KnowledgeStoreFailure::Corrupt => AgentRecoveryStoreFailure::Corrupt,
        KnowledgeStoreFailure::UnsupportedSchema => AgentRecoveryStoreFailure::UnsupportedSchema,
        KnowledgeStoreFailure::InvalidStoredData | KnowledgeStoreFailure::IdentityConflict => {
            AgentRecoveryStoreFailure::InvalidStoredData
        }
    }
}

const fn classify_run_journal_for_agent_action(
    failure: RunJournalStoreFailure,
) -> AgentActionStoreFailure {
    match failure {
        RunJournalStoreFailure::Unavailable => AgentActionStoreFailure::Unavailable,
        RunJournalStoreFailure::Corrupt => AgentActionStoreFailure::Corrupt,
        RunJournalStoreFailure::UnsupportedSchema => AgentActionStoreFailure::UnsupportedSchema,
        RunJournalStoreFailure::InvalidStoredData | RunJournalStoreFailure::RunAlreadyExists => {
            AgentActionStoreFailure::InvalidStoredData
        }
        RunJournalStoreFailure::RunNotFound => AgentActionStoreFailure::RunNotFound,
        RunJournalStoreFailure::SequenceConflict => AgentActionStoreFailure::RunSequenceConflict,
    }
}

//! Application use cases and ports for A^3.

mod agent_action_codec;
mod agent_actions;
mod agent_controller;
mod agent_prompt;
mod agent_read_result;
mod agent_recovery;
mod agent_source_reader;
mod agent_turn;
mod command_discovery;
mod context_compiler;
mod deep_map_action_codec;
mod deep_map_execution;
mod deep_map_explorer;
mod deep_map_planner;
mod deep_map_read_tools;
mod embedding_provider;
mod exact_search;
mod explorer_model_provider;
mod goal_contract;
mod graph_traversal;
mod health_query;
mod index_overview;
mod jobs;
mod knowledge_index_store;
mod knowledge_search_store;
mod knowledge_store;
mod language_adapter;
mod lexical_search;
mod model_capability;
mod model_provider;
mod module_card_claim_codec;
mod module_card_freshness;
mod module_card_verification;
mod module_remap_queue;
mod mutating_agent_controller;
mod mutation_coordinator;
mod mutation_reconciliation;
mod open_project;
mod policy;
mod policy_store;
mod process_runner;
mod project_reconciliation;
mod project_removal;
mod project_status;
mod project_storage;
mod recent_projects;
mod repository_discovery;
mod repository_index;
mod repository_snapshot;
mod repository_tree;
mod repository_watcher;
mod retrieval_fusion;
mod run_journal;
mod semantic_embedding_store;
mod semantic_embeddings;
mod task_ledger;
mod task_lens;
mod verification;
mod workspace_directory;
mod workspace_patch;

pub use a3_domain::{MutationActionFingerprint, MutationActionFingerprintError};
pub use agent_action_codec::{
    AgentActionDecodeError, AgentActionJsonSchema, AgentActionSchemaError, DecodeAgentAction,
};
pub use agent_actions::{
    AgentActionStore, AgentActionStoreFailure, AgentActionStoreFuture, AgentLedgerActionOutcome,
    AgentLedgerActionOutcomeKind, ApplyAgentLedgerUpdate, ApplyAgentLedgerUpdateError,
    PersistAgentLedgerMutation, PersistAgentLedgerMutationError, RequestAgentFinish,
};
pub use agent_controller::{
    AcceptanceRejection, AcceptanceVerificationRequest, AcceptanceVerificationRequestError,
    AcceptanceVerifier, AcceptanceVerifierFailure, AcceptanceVerifierFuture,
    AcceptanceVerifierOutcome, AcceptanceVerifierTimeout, AcceptanceVerifierTimeoutError,
    AdvanceAgentController, AgentControllerAdvance, AgentControllerAdvanceKind,
    AgentControllerControl, AgentControllerError, AgentControllerPreflightFailure,
    AgentControllerSignal, VerifyAgentAcceptance,
};
pub use agent_prompt::{
    AgentActionPrimaryOutcome, AgentActionRepair, AgentActionRepairFailure, AgentPromptContract,
    AgentPromptPrepareError, DecodeAgentActionTurn, PreparedAgentActionRepair, PreparedAgentPrompt,
};
pub use agent_read_result::{AgentReadResult, AgentReadResultError, RecordedAgentRead};
pub use agent_recovery::{
    AgentMutationResultRecord, AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryInspection,
    AgentRecoveryOutcome, AgentRecoveryOutcomeKind, AgentRecoveryStore, AgentRecoveryStoreFailure,
    AgentRecoveryStoreFuture, InspectAgentRunRecovery, RecoverAgentRun,
};
pub use agent_source_reader::{
    AgentSourcePage, AgentSourcePageError, AgentSourceReadControl, AgentSourceReadFailure,
    AgentSourceReader, AgentSourceReaderFuture,
};
pub use agent_turn::{
    AgentReadAction, AgentReadTimeout, AgentReadTimeoutError, AgentReadToolFailure, AgentReadTools,
    AgentReadToolsFuture, AgentTurnExecution, AgentTurnOutcome, AgentTurnRejectionReason,
    ExecuteAgentTurn, ExecuteAgentTurnFailure, ExecuteReadOnlyAgentTurn, RejectedAgentTurn,
};
pub use command_discovery::{
    CommandAllowlistStore, CommandAllowlistStoreFailure, CommandAllowlistStoreFuture,
    CommandAllowlistStoreVersion, CommandAllowlistStoreVersionError, CommandDiscoveryFailure,
    ConfirmProjectCommandAllowlist, ConfirmProjectCommandAllowlistError, DiscoverProjectCommands,
    LoadProjectCommandAllowlist, PrepareDiscoveredCommand, StoredProjectCommandAllowlist,
};
pub use context_compiler::{
    AgentContextCompileInput, AgentContextCompileInputError, AgentContextCompiler,
    AgentContextCompilerFuture, CompiledAgentContext, ContextCompileControl, ContextCompileFailure,
    ContextCompilePhase, ContextToolResult, ContextToolResultDigest, ContextToolResultPreview,
    ContextToolResultPreviewError, ContextToolResultStatus,
};
pub use deep_map_action_codec::{
    DecodeExplorerAction, ExplorerActionDecodeError, ExplorerActionJsonSchema,
};
pub use deep_map_execution::{
    DeepMapExecutionFailure, DeepMapExecutionFuture, DeepMapExecutionOutcome,
    DeepMapExecutionRequest, DeepMapExecutor, DeepMapModelDescriptor, DeepMapModelDescriptorError,
    DeepMapResumeState,
};
pub use deep_map_explorer::{
    DeepMapExplorerFailure, DeepMapExplorerFuture, DeepMapExplorerOutcome, DeepMapExplorerStatus,
    ExploreDeepMap,
};
pub use deep_map_planner::PlanDeepMap;
pub use deep_map_read_tools::{
    DeepMapReadControl, DeepMapReadFailure, DeepMapReadFuture, DeepMapReadTimeout,
    DeepMapReadTools, ExplorerObservation, ExplorerObservationError, ExplorerObservationStatus,
};
pub use embedding_provider::{
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
    EmbeddingProviderFuture, EmbeddingRequestTimeout, EmbeddingRequestTimeoutError,
    RawEmbeddingBatch, RawEmbeddingBatchError,
};
pub use exact_search::SearchExactIndex;
pub use explorer_model_provider::{
    ExplorerModelControl, ExplorerModelFailure, ExplorerModelFuture, ExplorerModelProvider,
    ExplorerModelRequest, ExplorerModelRequestPhase, ExplorerModelTimeout,
    ExplorerModelTimeoutError, ExplorerRepairReason, RawExplorerModelOutput,
    RawExplorerModelOutputError,
};
pub use goal_contract::{
    CreateGoalContract, CreateGoalContractFailure, GoalContractStore, GoalContractStoreFailure,
    GoalContractStoreFuture, ReviseGoalContract, ReviseGoalContractFailure,
};
pub use graph_traversal::TraverseKnowledgeGraph;
pub use health_query::{GetHealth, HealthQuery};
pub use index_overview::{
    GetPublishedIndexOverview, GetPublishedIndexOverviewError, PublishedDiagnostic,
    PublishedFileDiagnostics, PublishedIndexOverview, RepositoryPathDisplay,
};
pub use jobs::{
    CancellationToken, JobCancelResult, JobCancellationError, JobClock, JobCompletion, JobContext,
    JobEvent, JobEventKind, JobEventSequence, JobEventStream, JobEventStreamClosed, JobScheduler,
    JobSchedulerConfig, JobSchedulerConfigError, JobSchedulerCreateError,
    JobSchedulerShutdownError, JobSchedulerSubmitError, JobSnapshot, JobSubmitter, JobTask,
    JobTimestamp, ProgressReportError, ShutdownMode, ShutdownReport,
};
pub use knowledge_index_store::{
    IndexPersistenceControl, IndexPersistenceControlError, KnowledgeIndexFailure,
    KnowledgeIndexFuture, KnowledgeIndexStore,
};
pub use knowledge_search_store::{
    KnowledgeSearchControl, KnowledgeSearchFailure, KnowledgeSearchFuture, KnowledgeSearchStore,
};
pub use knowledge_store::{
    KnowledgeStore, KnowledgeStoreFailure, KnowledgeStoreFuture, ProjectPathDisplay,
    ProjectPathDisplayError, RecentProject, RecentProjectLimit, RecentProjectLimitError,
};
pub use language_adapter::{
    LanguageAdapter, LanguageParseControl, LanguageParseControlError, LanguageParseFailure,
    LanguageParseInput, LanguageParsePolicy,
};
pub use lexical_search::SearchLexicalIndex;
pub use model_capability::{
    ModelCapabilityObservation, ModelCapabilityProbe, ModelCapabilityProbeFuture,
    ModelCapabilityProbeRequest, ProbeModelProfile, ProbeModelProfileFailure,
    ReportedModelContextLimit, ReportedModelContextLimitError,
};
pub use model_provider::{
    ModelCancellationFuture, ModelFinishReason, ModelMessage, ModelMessageError, ModelMessageRole,
    ModelOperationControl, ModelOutputChunk, ModelOutputChunkError, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture, ModelProviderRequest,
    ModelProviderRequestError, ModelProviderUsage, ModelRequestTimeout, ModelRequestTimeoutError,
    ProviderEvent, ProviderEventStream, StructuredOutputSchema, StructuredOutputSchemaError,
};
pub use module_card_claim_codec::{
    DecodeModuleCardClaims, ModuleCardClaimDecodeError, ModuleCardClaimJsonSchema,
};
pub use module_card_freshness::{
    GetModuleCardFreshness, ModuleCardFreshness, ModuleCardFreshnessControl,
    ModuleCardFreshnessControlError, ModuleCardFreshnessError, ModuleCardFreshnessFailure,
    ModuleCardFreshnessFuture, ModuleCardFreshnessReasonCount, ModuleCardFreshnessReasonCountError,
    ModuleCardFreshnessStatus, ModuleCardFreshnessStore,
};
pub use module_card_verification::{
    ModuleCardEvidenceResolutionTimeout, ModuleCardEvidenceResolutionTimeoutError,
    ModuleCardEvidenceResolver, ModuleCardEvidenceResolverFailure,
    ModuleCardEvidenceResolverFuture, ModuleCardPublicationTimeout,
    ModuleCardPublicationTimeoutError, ModuleCardVerificationControl,
    ModuleCardVerificationControlError, PublishVerifiedModuleCards,
    PublishVerifiedModuleCardsFailure, PublishedIndexEvidenceResolver, PublishedModuleCardReceipt,
    VerifiedModuleCardPublisher, VerifiedModuleCardPublisherFailure,
    VerifiedModuleCardPublisherFuture, VerifyModuleCards, VerifyModuleCardsFailure,
};
pub use module_remap_queue::{
    LoadPendingModuleRemaps, ModuleRemapQueueFailure, ModuleRemapQueueFuture,
    ModuleRemapQueueStore, PendingRemapQueue, PendingRemapQueueError, RemapQueueControl,
    RemapQueueControlError, RemapQueueLimit, RemapQueueLimitError,
};
pub use mutating_agent_controller::{
    ConservativeProcessVerificationEvidenceFactory, ExecuteMutatingAgentAction,
    MutationCommandSelection, MutationContextSeed, MutationControllerFailure,
    MutationControllerOutcome, MutationExecutionIds, ProcessVerificationEvidenceFactory,
    ProcessVerificationEvidenceFailure, ProcessVerificationEvidenceRequest,
};
pub use mutation_coordinator::{
    MutationFailureClass, MutationProgressDecision, WorktreeMutationBusy,
    WorktreeMutationCoordinator, WorktreeMutationLease,
};
pub use mutation_reconciliation::{
    MutationReconciliationError, MutationReconciliationOutcome, ReconcileUnknownMutation,
};
pub use open_project::{
    OpenProject, OpenProjectError, OpenProjectOutcome, ProjectDirectoryPicker,
    ProjectDirectorySelectionError, ProjectInspectionFailure, ProjectInspector,
    ProjectReconciliationChoice, ProjectReconciliationConfirmationError,
    ProjectReconciliationConfirmer,
};
pub use policy::{
    EvaluateActionPolicy, EvaluateActionPolicyError, EvaluatedPolicyAction, PolicyEvaluationContext,
};
pub use policy_store::{
    GrantPolicyApproval, GrantPolicyApprovalError, PersistPolicyEvaluation, PolicyStore,
    PolicyStoreFailure, PolicyStoreFuture, RevokePolicyApproval, RevokePolicyApprovalError,
};
pub use process_runner::{
    AuthorizedProcessSpec, ProcessAuthorizationError, ProcessEventSink, ProcessEventSinkError,
    ProcessRunControl, ProcessRunFailure, ProcessRunFuture, ProcessRunner,
};
pub use project_reconciliation::{
    ProjectCatalogRevision, ProjectCatalogRevisionError, ProjectOpenPreparation,
    ProjectReconciliationEvidence, ProjectReconciliationProposal,
};
pub use project_removal::{
    ProjectCatalogAdmin, ProjectCatalogAdminFailure, ProjectCatalogAdminFuture,
    RemoveProjectFromList, RemoveProjectFromListError, RemovedProject,
};
pub use project_status::{
    GetProjectIndexStatus, GetProjectIndexStatusError, ProjectIndexStatus, ProjectSnapshotStatus,
};
pub use project_storage::{
    GetProjectStorageUsage, GetProjectStorageUsageError, ProjectStorageControl,
    ProjectStorageControlError, ProjectStorageFailure, ProjectStorageFuture, ProjectStorageStore,
    ProjectStorageUsage,
};
pub use recent_projects::{ListRecentProjects, ListRecentProjectsError};
pub use repository_discovery::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositoryDiscoveryFailure,
};
pub use repository_index::{
    IndexRunIdFactory, IndexRunIdFactoryFailure, RefreshRepositoryIndex,
    RefreshRepositoryIndexError, RepositoryIndexCompilation, RepositoryIndexCompiler,
    RepositoryIndexCompilerFailure, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryIndexMode, RepositoryIndexPhase, RepositoryIndexRefresh,
};
pub use repository_snapshot::{
    IncrementalRepositorySnapshotBuild, IncrementalRepositorySnapshotBuilder,
    RepositorySnapshotBuild, RepositorySnapshotBuilder, RepositorySnapshotControl,
    RepositorySnapshotControlError, RepositorySnapshotFailure, RepositorySnapshotPhase,
    RepositorySnapshotPolicy, SnapshotBaseline, SnapshotBaselineError, SnapshotCompatibility,
    SnapshotCompatibilityError,
};
pub use repository_tree::{
    GetRepositoryTreePage, RepositoryTreeChildName, RepositoryTreeChildNameError,
    RepositoryTreeControl, RepositoryTreeControlError, RepositoryTreeDisplayName,
    RepositoryTreeEntry, RepositoryTreeEntryError, RepositoryTreeEntryKind, RepositoryTreeFailure,
    RepositoryTreeFuture, RepositoryTreePage, RepositoryTreePageError, RepositoryTreePageSize,
    RepositoryTreePageSizeError, RepositoryTreeQuery, RepositoryTreeStore,
};
pub use repository_watcher::{
    RepositoryChangeBatch, RepositoryChangeBatchError, RepositoryRescanReason,
};
pub use retrieval_fusion::FuseRetrievalCandidates;
pub use run_journal::{
    AppendAgentRead, AppendRunEvent, CreateAgentRun, ExportRunJournal, RunEventPage,
    RunEventPageError, RunEventPageLimit, RunEventPageLimitError, RunJournalExport,
    RunJournalExportControl, RunJournalExportControlError, RunJournalExportError,
    RunJournalExportSchemaVersion, RunJournalRetentionPolicy, RunJournalStore,
    RunJournalStoreFailure, RunJournalStoreFuture,
};
pub use semantic_embedding_store::{
    SemanticCacheRebuildControl, SemanticCacheRebuildProgressError, SemanticEmbeddingStore,
    SemanticEmbeddingStoreFailure, SemanticEmbeddingStoreFuture,
};
pub use semantic_embeddings::{
    EmbeddingClock, EmbeddingClockFailure, EmbeddingExecutionMode, EmbeddingProgressError,
    GenerateSemanticEmbeddings, GenerateSemanticEmbeddingsError, GenerateSemanticEmbeddingsOutcome,
    SemanticEmbeddingBatchJob, SemanticEmbeddingJobControl,
};
pub use task_ledger::{
    CreateTaskLedger, SaveTaskLedger, StoredTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure,
    TaskLedgerStoreFuture, TaskLedgerStoreVersion, TaskLedgerStoreVersionError,
};
pub use task_lens::{
    CompileTaskLens, CompileTaskLensFailure, TaskLensClaimLimit, TaskLensClaimReadFuture,
    TaskLensClaimResult, TaskLensClaimResultError, TaskLensClaimStore, TaskLensClaimStoreFailure,
    TaskLensClaimStoreFuture, TaskLensControl, TaskLensControlError, TaskLensIndexStore,
    TaskLensIndexStoreFuture, TaskLensSemanticHit, TaskLensSemanticLimit, TaskLensSemanticResult,
    TaskLensSemanticResultError, TaskLensSemanticSearch, TaskLensSemanticSearchFailure,
    TaskLensSemanticSearchFuture, TaskLensTimeout, TaskLensTimeoutError,
};
pub use verification::{
    DeterministicAcceptanceVerifier, EvaluateStepVerification, EvaluateStepVerificationError,
    OrderVerificationSpecs, StoredVerificationState, StoredVerificationStateError,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure, VerificationEvidenceStoreFuture,
    VerificationOrderingError,
};
pub use workspace_directory::{
    WorkspaceDirectoryLister, WorkspaceDirectoryListerFuture, WorkspaceDirectoryProgressError,
    WorkspaceDirectoryReadControl, WorkspaceDirectoryReadFailure,
};
pub use workspace_patch::{
    AuthorizedPatchAction, PatchApplyFailure, PatchApplyFuture, PatchAuthorizationError,
    PatchPreviewFailure, PatchPreviewFuture, WorkspacePatchControl, WorkspacePatchProgressError,
    WorkspacePatchTool,
};

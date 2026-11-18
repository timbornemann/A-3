//! Application use cases and ports for A^3.

mod execute_http_context_intent;
mod execute_http_no_change_replan;
mod http_no_change_replan;
mod http_no_change_window_apply;
mod http_no_change_window_edit;
mod http_no_change_window_material;
pub use execute_http_no_change_replan::{
    ExecuteHttpNoChangeReplan, HttpNoChangeReplanExecutionFailure,
    HttpNoChangeReplanExecutionReport,
};
pub use http_no_change_replan::{
    HttpNoChangeReplanAnchor, HttpNoChangeReplanDecision, HttpNoChangeReplanFailure,
    HttpNoChangeReplanHalt, HttpNoChangeReplanResume, HttpNoChangeReplanResumeState,
    HttpNoChangeReplanSettlement, PreparedHttpNoChangeReplanV6Job,
    replay_http_no_change_replan_job,
};
pub use http_no_change_window_apply::ApplyHttpNoChangeWindowPlan;
pub use http_no_change_window_edit::{
    ActivateHttpNoChangeWindowEdit, HttpNoChangeWindowEditDiagnostic,
};
pub use http_no_change_window_material::{
    AppliedHttpNoChangeWindowOrigin, HttpNoChangeWindowFailure, HttpNoChangeWindowMaterialPlan,
    HttpNoChangeWindowMaterialProof, derive_http_no_change_window_material_plan,
    http_no_change_window_material_identity,
};
mod http_context_intent;
pub use execute_http_context_intent::{
    ExecuteHttpContextIntent, HttpContextIntentExecutionFailure, HttpContextIntentExecutionReport,
};
pub use http_context_intent::{
    CoreBoundHttpContextReply, HttpContextIntentAnchor, HttpContextIntentReply,
    HttpContextIntentReservation, HttpContextIntentSettlement, HttpContextReadOrigin,
    PreparedHttpContextIntent,
};
mod execute_http_correction_plan;
mod execute_initial_plan;
mod execute_intake_phase;
mod goal_intake_review;
mod http_correction_apply;
mod http_correction_edit;
pub use http_correction_edit::{
    ActivateHttpCorrectionEdit, HttpCorrectionEditFailure, HttpCorrectionOrigin,
    HttpCorrectionProducer, HttpFailureEditDiagnostic, http_correction_definition_page_matches,
    http_correction_definition_source,
};
mod http_correction_plan;
pub use http_correction_apply::{
    ApplyHttpCorrectionPlan, HttpCorrectionMaterialPlan, HttpCorrectionPlanApplied,
    HttpCorrectionPlanApplyFailure, derive_http_correction_material_plan,
};
mod initial_plan_reservation;
pub use execute_http_correction_plan::{
    ExecuteHttpCorrectionPlan, HttpCorrectionPlanExecutionFailure,
    HttpCorrectionPlanExecutionReport,
};
pub use http_correction_plan::{
    HttpCorrectionPlanAnchor, HttpCorrectionPlanDecision, HttpCorrectionPlanFailure,
    HttpCorrectionPlanIntent, HttpCorrectionPlanProposal, HttpCorrectionPlanReservation,
    HttpCorrectionPlanSourceOffer, HttpCorrectionPlanStore, PreparedHttpCorrectionPlan,
};
mod intake_phase;
mod intake_phase_anchor;
mod intake_phase_reservation;
mod intake_window;
pub use goal_intake_review::{RecordGoalIntakeReview, RecordGoalIntakeReviewFailure};
mod prepare_goal_from_intake_review;
pub use prepare_goal_from_intake_review::{
    PrepareGoalFromIntakeReviewFailure, prepare_goal_from_intake_review,
};

mod agent_action_codec;
mod agent_machine_codec;
mod agent_permissions;
mod loopback_startup_verification;
mod loopback_verification;
mod machine_file_tool;
mod machine_network;
mod machine_process;
pub use agent_permissions::{
    AgentPermissionFuture, AgentPermissionStore, AgentPermissionStoreFailure,
};
pub use loopback_startup_verification::{
    AuthorizedLoopbackStartupCoverage, LoopbackStartupCleanupProof, LoopbackStartupDecisions,
    LoopbackStartupVerificationFuture, LoopbackStartupVerificationOutcome,
    LoopbackStartupVerificationReport, PreparedLoopbackStartupCoverage,
};
pub use loopback_verification::{
    AuthorizedLoopbackVerification, LoopbackCleanupProof, LoopbackMethod, LoopbackProbe,
    LoopbackVerificationFailure, LoopbackVerificationFuture, LoopbackVerificationOutcome,
    LoopbackVerificationReport, LoopbackVerificationRunner, PreparedLoopbackVerification,
};
pub use machine_file_tool::{
    AuthorizedMachineFileAction, MachineFileControl, MachineFileReceipt, MachineFileTool,
    MachineFileToolFailure, MachineFileToolFuture, PreparedMachineFileAction,
};
pub use machine_network::{
    AuthorizedMachineHttpGet, MachineHttpReceipt, MachineNetworkFailure, MachineNetworkFuture,
    MachineNetworkTool,
};
pub use machine_process::{
    MachineProcessPreparationError, PrepareMachineProcess, classify_machine_process,
};
mod agent_actions;
mod agent_activity;
mod agent_approval;
mod agent_ask_research;
mod agent_controller;
mod agent_execution_checkpoint;
mod closed_http_no_change;
mod edit_job_digest;
mod edit_no_change;
mod edit_phase;
mod edit_phase_request;
mod edit_phase_reservation;
mod edit_scope_frame;
mod execute_edit_phase;
pub use agent_execution_checkpoint::{
    AgentExecutionCheckpoint, AgentExecutionCheckpointError, ExecutedAgentMutation,
    LoadAgentExecutionCheckpoint,
};
mod agent_goal;
mod agent_goal_admission;
mod source_failure_replan;
mod source_step_admission;
pub use source_failure_replan::{
    ApplySourceTestCorrection, SourceFailureEditDiagnostic, SourceTestCorrection,
    SourceTestCorrectionFailure,
};
pub use source_step_admission::{AdmitSourceTaskStep, SourceTaskStepAdmissionFailure};
mod goal_intake;
pub use agent_goal_admission::{AdmitAgentGoalExecution, AgentGoalAdmissionFailure};
pub use goal_intake::{
    CaptureGoalIntake, CaptureGoalIntakeFailure, GoalIntakeLoadResult, GoalIntakeStoreVersion,
    GoalIntakeStoreVersionError, PENDING_GOAL_INTAKE_PAGE_SIZE, PendingGoalIntakePage,
    PendingGoalIntakePageError, PromoteGoalIntake, PromoteGoalIntakeFailure,
    StoredPendingGoalIntake,
};
mod agent_inspection;
mod agent_prompt;
mod agent_read_result;
mod replan_analysis;
pub use replan_analysis::ReplanAnalysisFailure;
mod replan_evidence_need;
mod replan_research;
pub use replan_evidence_need::ReplanEvidenceNeed;
pub use replan_research::{ReplanReadRejection, ReplanResearchCheckpoint, ReplanResearchContext};
mod agent_recovery;
mod agent_research_controller;
mod research_request_commands;
mod research_response_codec;
pub use research_request_commands::ResearchRequestCommands;
mod research_design_basis;
mod research_source_review;
pub use research_design_basis::ResearchDesignBasis;
mod research_work_admission;
pub use research_source_review::{
    ResearchAnalysisMethod, ResearchSourceReview, ResearchSourceReviewError,
    research_source_review_schema, research_source_review_system_prompt,
};
mod research_work_codec;
mod schema_projection;
pub use research_work_admission::{
    ResearchEvidenceWindow, ResearchWorkAdmissionError, admit_research_work,
};
pub use research_work_codec::{
    ResearchEvidenceAnchorId, ResearchEvidenceNeed, ResearchOutputPhase, ResearchQuoteProposal,
    ResearchResultProposal, ResearchWorkUpdate, research_work_current_phase_schema,
    research_work_decision_schema, research_work_phase_schema, research_work_v6_phase_schema,
};
mod agent_runtime;
mod agent_session;
mod agent_source_reader;
mod agent_task_control;
mod agent_turn;
mod ask_research_action_codec;
mod bound_edit_source;
mod command_discovery;
mod context_compiler;
mod context_original_source;
mod create_project;
mod deep_map_action_codec;
mod deep_map_activity;
mod deep_map_dashboard;
mod deep_map_execution;
mod deep_map_explorer;
mod deep_map_journal;
mod deep_map_model_adapter;
mod deep_map_planner;
mod deep_map_publication_state;
mod deep_map_read_tools;
mod deep_map_runner;
mod embedding_provider;
mod evidence_diagram;
mod exact_search;
mod explorer_model_provider;
mod function_flow;
mod goal_contract;
mod materialize_original_task;
pub use materialize_original_task::{
    MaterializeOriginalTask, MaterializeOriginalTaskFailure, MaterializedOriginalTask,
    OriginalPlanPreview, OriginalPlanPreviewStep,
};
mod graph_traversal;
pub use function_flow::{
    ExploreFunctionFlows, FUNCTION_FLOW_MAX_CONTEXTS, FUNCTION_FLOW_PAGE_SIZE, FlowTraceDirection,
    FlowValueAddress, FlowValueTrace, FlowValueTraceNode, FunctionFlowCatalog, FunctionFlowFrame,
    FunctionFlowInspection, FunctionFlowReadDocument, FunctionFlowReadFailure,
    FunctionFlowSelection,
};
mod caller_relocalization;
mod health_query;
mod index_overview;
mod index_trace;
mod jobs;
mod knowledge_index_store;
mod knowledge_search_store;
mod knowledge_store;
mod language_adapter;
mod lexical_search;
mod model_capability;
mod model_catalog;
mod model_provider;
mod model_stream_collector;
mod model_wire;
mod module_card_claim_codec;
mod module_card_claim_proposer;
mod module_card_detail;
mod module_card_evidence;
mod module_card_freshness;
mod module_card_verification;
mod module_dependency_graph;
mod module_remap_queue;
mod module_runtime;
mod module_tree;
mod mutating_agent_controller;
mod mutation_coordinator;
mod mutation_reconciliation;
mod open_project;
mod phase_json;
mod plan_phase;
mod policy;
mod policy_store;
mod prepare_next_intake_phase;
mod process_runner;
mod project_ask;
mod project_ask_sources;
mod project_catalog;
mod project_map_atlas;
mod project_map_scene;
mod project_map_search;
mod project_map_source_preview;
mod project_reconciliation;
mod project_removal;
mod project_settings;
mod project_status;
mod project_storage;
mod provider_credentials;
mod published_deep_map_tools;
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
mod settings;
mod slash_commands;
mod source_delivery;
mod source_read;
mod task_ledger;
mod task_lens;
mod task_lens_workspace;
mod ui_preferences;
mod verification;
mod verification_inspection;
mod workspace_directory;
mod workspace_patch;

pub use a3_domain::{MutationActionFingerprint, MutationActionFingerprintError};
pub use agent_action_codec::{
    AgentActionDecodeError, AgentActionJsonSchema, AgentActionSchemaError, DecodeAgentAction,
    DecodedAgentAction,
};
pub use agent_actions::{
    AgentActionStore, AgentActionStoreFailure, AgentActionStoreFuture, AgentLedgerActionOutcome,
    AgentLedgerActionOutcomeKind, ApplyAgentLedgerUpdate, ApplyAgentLedgerUpdateError,
    ApplyAgentPlanRevision, ApplyAgentPlanRevisionError, BindDeferredAgentVerification,
    BindDeferredAgentVerificationError, ContinueVerifiedAgentPlan, ContinueVerifiedAgentPlanError,
    ContinueVerifiedAgentPlanOutcome, PersistAgentLedgerMutation, PersistAgentLedgerMutationError,
    RequestAgentFinish,
};
pub use agent_activity::{
    AgentActivity, AgentActivityLoadResult, AgentActivityRun, GetAgentActivity,
    GetAgentActivityFailure,
};
pub use agent_approval::*;
pub use agent_ask_research::{
    AskResearchDataError, AskResearchDetail, AskResearchEvent, AskResearchProjection,
    AskResearchPublicFindingKind, AskResearchPublicNote, AskResearchSource, AskResearchSourcePage,
    AskResearchStore, AskResearchStoreFailure, AskResearchStoreFuture, AskResearchTurn,
    AskResearchTurnPage, AskSourceSearchControl, AskSourceSearchFailure, AskSourceSearcher,
    AskSourceSearcherFuture, AskSourceTextHit, AskSourceTextSearch, AskSourceTextSearchResult,
    SessionEvidenceDiagramArtifact,
};
pub use agent_controller::{
    AcceptanceRejection, AcceptanceVerificationRequest, AcceptanceVerificationRequestError,
    AcceptanceVerifier, AcceptanceVerifierFailure, AcceptanceVerifierFuture,
    AcceptanceVerifierOutcome, AcceptanceVerifierTimeout, AcceptanceVerifierTimeoutError,
    AdvanceAgentController, AgentControllerAdvance, AgentControllerAdvanceKind,
    AgentControllerControl, AgentControllerError, AgentControllerPreflightFailure,
    AgentControllerSignal, VerifyAgentAcceptance,
};
pub use agent_goal::{
    AgentGoalCriterionDraft, AgentGoalDraft, AgentGoalDraftFailure, AgentGoalGeneratedIdentity,
    AgentGoalMetadataFailure, AgentGoalMetadataSource, CreateAgentGoal, CreateAgentGoalFailure,
    GetAgentGoal, ReviseAgentGoal, ReviseAgentGoalFailure,
};
pub use agent_inspection::*;
pub use agent_prompt::{
    AgentActionPrimaryOutcome, AgentActionRepair, AgentActionRepairFailure, AgentPromptContract,
    AgentPromptPrepareError, DecodeAgentActionTurn, PreparedAgentActionRepair, PreparedAgentPrompt,
};
pub use agent_read_result::{AgentReadResult, AgentReadResultError, RecordedAgentRead};
pub use agent_recovery::{
    AgentMutationResultRecord, AgentRecoveryChoice, AgentRecoveryError, AgentRecoveryInspection,
    AgentRecoveryOutcome, AgentRecoveryOutcomeKind, AgentRecoveryStore, AgentRecoveryStoreFailure,
    AgentRecoveryStoreFuture, CompletedDiffRecoveryEvidence, InspectAgentRunRecovery,
    MachineEffectScope, RecoverAgentRun,
};
pub use agent_research_controller::{
    BeginResearchDecision, BoundedResearchController, ResearchActionBatch, ResearchControllerError,
    ResearchHandoff, ResearchLimits, ResearchMemoryCheckpoint, ResearchMemoryFinding,
    ResearchMemoryFindingKind, memory_finding_from_note,
};
pub use agent_runtime::{
    AgentPermissionChangeFuture, AgentRunExecutionFailure, AgentRunExecutionFuture,
    AgentRunExecutionOutcome, AgentRunExecutionRequest, AgentRunExecutionTrigger, AgentRunExecutor,
};
pub use agent_session::{
    AgentSessionCommandPresentation, AgentSessionDetail, AgentSessionListQuery, AgentSessionPage,
    AgentSessionQueue, AgentSessionStore, AgentSessionStoreFailure, AgentSessionStoreFuture,
    validate_agent_session_transition,
};
pub use agent_source_reader::{
    AgentSourcePage, AgentSourcePageError, AgentSourceReadControl, AgentSourceReadFailure,
    AgentSourceReader, AgentSourceReaderFuture,
};
pub use agent_task_control::{
    AgentTaskControlFailure, AgentTaskControlResult, AgentTaskRecovery,
    AgentTaskRecoveryLoadResult, ControlAgentTaskRun, InspectAgentTaskRecovery,
};
pub use agent_turn::{
    AgentActionGeneration, AgentReadAction, AgentReadTimeout, AgentReadTimeoutError,
    AgentReadToolFailure, AgentReadTools, AgentReadToolsFuture, AgentTurnExecution,
    AgentTurnOutcome, AgentTurnRejectionReason, ExecuteAgentTurn, ExecuteAgentTurnFailure,
    ExecuteReadOnlyAgentTurn, RejectedAgentTurn, StagedActionFailure,
};
pub use ask_research_action_codec::{
    AskResearchAction, AskResearchDecision, AskResearchDecisionDecodeError,
    AskResearchDecisionJsonSchema, AskResearchDecisionNote, AskResearchEvidenceStatus,
    AskResearchFindingKind, AskResearchNoteOrigin, AskResearchRelation, DecodeAskResearchDecision,
};
pub use bound_edit_source::{BoundEditSource, BoundEditSourceFailure};
pub use caller_relocalization::{
    CallerRelocalization, CallerRelocalizationAdmission, CallerRelocalizationOutcome,
    CallerRelocalizationReceipt, ObservedCallerRelocalization,
};
pub use closed_http_no_change::{
    ClosedEditNoChangeCall, ClosedEditNoChangeFact, ClosedEditNoChangeOwner, ClosedEditWindow,
    ClosedHttpNoChangeOrigin, ClosedNoChangeProfileLimits, replay_closed_no_change_edit_job,
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
pub use context_original_source::ContextOriginalSource;
pub use create_project::{
    CreateProject, CreateProjectError, EmptyWorktreeInitializationFailure, EmptyWorktreeInitializer,
};
pub use deep_map_action_codec::{
    DecodeExplorerAction, ExplorerActionDecodeError, ExplorerActionJsonSchema,
};
pub use deep_map_activity::{
    DeepMapActivityObserver, DeepMapActivityUpdate, DeepMapPhase, DeepMapSafeAction,
    DeepMapTargetKind, IgnoreDeepMapActivity,
};
pub use deep_map_dashboard::{
    DeepMapDashboardActivity, DeepMapDashboardFreshness, DeepMapDashboardModuleState,
    DeepMapDashboardPhase, DeepMapDashboardPhaseProgress, DeepMapDashboardPhaseState,
    DeepMapDashboardState, DeepMapRunDashboard, derive_deep_map_module_state,
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
pub use deep_map_journal::{
    DEEP_MAP_ENTRY_PAGE_LIMIT, DEEP_MAP_MODULE_PAGE_LIMIT, DEEP_MAP_MODULE_STEP_PAGE_LIMIT,
    DEEP_MAP_RUN_PAGE_LIMIT, DeepMapEntryDetail, DeepMapEntryPage, DeepMapEventResult,
    DeepMapJournalEvent, DeepMapModuleCursor, DeepMapModuleStepPage, DeepMapPlanStep,
    DeepMapPlanTargetReference, DeepMapPublicationResult, DeepMapRunCursor,
    DeepMapRunJournalFailure, DeepMapRunJournalFuture, DeepMapRunJournalStore,
    DeepMapRunModulePage, DeepMapRunModuleSummary, DeepMapRunPage, DeepMapRunStart,
    DeepMapRunSummary, DeepMapStepDetail,
};
pub use deep_map_model_adapter::ModelBackedExplorerProvider;
pub use deep_map_planner::PlanDeepMap;
pub use deep_map_publication_state::{
    DeepMapPublicationAnchor, DeepMapPublicationState, DeepMapPublicationStateFailure,
    DeepMapPublicationStateFuture, DeepMapPublicationStateStore,
};
pub use deep_map_read_tools::{
    DeepMapReadControl, DeepMapReadFailure, DeepMapReadFuture, DeepMapReadTimeout,
    DeepMapReadTools, ExplorerObservation, ExplorerObservationError, ExplorerObservationStatus,
};
pub use deep_map_runner::RunDeepMap;
pub use edit_no_change::{
    CloseEditNoChange, EditNoChangeClosure, EditNoChangeFailure, EditNoChangeReceipt,
    EditNoChangeWindow, derive_edit_no_change_closure,
};
pub use edit_phase::{
    BoundEditPhaseReply, EditEvidenceNeed, EditPhaseAnchor, EditPhaseFailure, EditPhaseOutcome,
    EditPhaseState, PreparedEditPhase,
};
pub use edit_phase_request::{
    CompiledEditPhaseRequest, EditPhasePrerequisites, EditPhaseRequestFailure,
    build_edit_phase_request,
};
pub use edit_phase_reservation::{
    EditPhaseCallKind, EditPhaseHalt, EditPhaseHaltReceipt, EditPhaseObservation,
    EditPhaseObservationOutcome, EditPhaseReservation, EditPhaseReservationFailure,
    EditPhaseReservationFuture, EditPhaseReservationStore, EditPhaseReservedUsage,
    EditPhaseWireCost,
};
pub use embedding_provider::{
    EmbeddingCapabilityProbe, EmbeddingCapabilityProbeFuture, EmbeddingCapabilityProbeRequest,
    EmbeddingOperationControl, EmbeddingProvider, EmbeddingProviderFailure,
    EmbeddingProviderFuture, EmbeddingRequestTimeout, EmbeddingRequestTimeoutError,
    ProbeEmbeddingModelProfile, RawEmbeddingBatch, RawEmbeddingBatchError,
};
pub use evidence_diagram::{
    DecodeEvidenceDiagrams, EvidenceDiagramArtifact, EvidenceDiagramDecodeError,
    EvidenceDiagramDraft, EvidenceDiagramElement, EvidenceDiagramJsonSchema, EvidenceDiagramKind,
    EvidenceDiagramRelationship,
};
pub use exact_search::SearchExactIndex;
pub use execute_edit_phase::{
    EditPhaseExecutionFailure, EditPhaseExecutionReport, ExecuteEditPhase,
};
pub use execute_initial_plan::{
    ExecuteInitialPlan, InitialPlanExecutionFailure, InitialPlanExecutionOutcome,
    InitialPlanExecutionReport,
};
pub use execute_intake_phase::{
    ExecuteIntakePhase, IntakePhaseExecutionFailure, IntakePhaseExecutionReport,
};
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
pub use index_trace::{
    INDEX_TRACE_EVENT_LIMIT, INDEX_TRACE_FILE_DIAGNOSTIC_LIMIT, INDEX_TRACE_FILE_PAGE_LIMIT,
    INDEX_TRACE_SEARCH_BYTES, IndexTraceCheckpoint, IndexTraceCounts, IndexTraceDataError,
    IndexTraceEvent, IndexTraceEventKind, IndexTraceFileFilter, IndexTraceFilePage,
    IndexTraceFileQuery, IndexTraceFileRecord, IndexTracePhaseProgress, IndexTraceRunSummary,
    IndexTraceSnapshot, IndexTraceStore, IndexTraceStoreFailure, IndexTraceStoreFuture,
    RetainedIndexTraces,
};
pub use initial_plan_reservation::{
    InitialPlanFinish, InitialPlanRejection, InitialPlanReservation, InitialPlanReservationFailure,
    InitialPlanReservationFuture, InitialPlanReservationStore,
};
pub use intake_phase::{IntakePhaseFailure, MAX_INTAKE_PHASE_REPLY_BYTES, PreparedIntakePhase};
pub use intake_phase_anchor::{IntakePhaseAnchor, IntakePhaseAnchorFailure};
pub use intake_phase_reservation::{
    IntakePhaseAccounting, IntakePhaseFinish, IntakePhaseRejection, IntakePhaseReservation,
    IntakePhaseReservationFailure, IntakePhaseReservationFuture, IntakePhaseReservationStore,
};
pub use intake_window::{
    IntakeWindowByteLimit, IntakeWindowSelectionFailure, select_intake_window,
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
    StoredProjectTarget,
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
pub use model_catalog::{
    DiscoverProviderModels, ModelCatalogFuture, ModelCatalogProvider, ProviderModelCatalog,
};
pub use model_provider::{
    ModelCancellationFuture, ModelFinishReason, ModelMessage, ModelMessageError, ModelMessageRole,
    ModelOperationControl, ModelOutputChunk, ModelOutputChunkError, ModelProvider,
    ModelProviderCompletion, ModelProviderFailure, ModelProviderFuture, ModelProviderRequest,
    ModelProviderRequestError, ModelProviderUsage, ModelRequestTimeout, ModelRequestTimeoutError,
    ProviderEvent, ProviderEventStream, StructuredOutputSchema, StructuredOutputSchemaError,
};
pub use model_wire::{
    ModelWireProjection, ModelWireThinking, PhaseWireBudget, PhaseWireBudgetError,
};
pub use module_card_claim_codec::{
    DecodeModuleCardClaims, ModuleCardClaimDecodeError, ModuleCardClaimJsonSchema,
};
pub use module_card_claim_proposer::{ProposeModuleCardClaims, ProposeModuleCardClaimsFailure};
pub use module_card_detail::{
    GetModuleCardDetail, ModuleCardClaimPresentation, ModuleCardClaimPresentationError,
    ModuleCardClaimState, ModuleCardCoverage, ModuleCardCoverageBand, ModuleCardDetail,
    ModuleCardDetailControl, ModuleCardDetailControlError, ModuleCardDetailFailure,
    ModuleCardDetailField, ModuleCardDetailFieldError, ModuleCardDetailFuture,
    ModuleCardDetailLoadResult, ModuleCardDetailQuery, ModuleCardDetailStore, ModuleCardLifecycle,
    ModuleCardValuePresentation,
};
pub use module_card_evidence::{
    GetModuleCardEvidence, ModuleCardEvidenceControl, ModuleCardEvidenceControlError,
    ModuleCardEvidenceDetail, ModuleCardEvidenceDetailError, ModuleCardEvidenceFailure,
    ModuleCardEvidenceFreshness, ModuleCardEvidenceFuture, ModuleCardEvidenceLoadResult,
    ModuleCardEvidencePayload, ModuleCardEvidenceQuery, ModuleCardEvidenceStore,
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
pub use module_dependency_graph::{
    GetModuleDependencyGraph, ModuleDependencyEdge, ModuleDependencyEdgeError,
    ModuleDependencyGraph, ModuleDependencyGraphControl, ModuleDependencyGraphControlError,
    ModuleDependencyGraphError, ModuleDependencyGraphFailure, ModuleDependencyGraphFuture,
    ModuleDependencyGraphLoadResult, ModuleDependencyGraphQuery, ModuleDependencyGraphStore,
    ModuleDependencyNode, ModuleDependencyNodeError, ModuleDependencyNodeLimit,
    ModuleDependencyNodeLimitError, ModuleDependencyRelation,
};
pub use module_remap_queue::{
    LoadPendingModuleRemaps, ModuleRemapQueueFailure, ModuleRemapQueueFuture,
    ModuleRemapQueueStore, PendingRemapQueue, PendingRemapQueueError, RemapQueueControl,
    RemapQueueControlError, RemapQueueLimit, RemapQueueLimitError,
};
pub use module_runtime::{
    GetModuleRuntimeMap, ModuleRuntimeControl, ModuleRuntimeControlError, ModuleRuntimeFailure,
    ModuleRuntimeFlowKind, ModuleRuntimeFlowLoadResult, ModuleRuntimeFlowQuery,
    ModuleRuntimeFlowRootValidation, ModuleRuntimeFuture, ModuleRuntimeMap, ModuleRuntimeMapError,
    ModuleRuntimeMapLoadResult, ModuleRuntimeMapQuery, ModuleRuntimeRoot, ModuleRuntimeRootError,
    ModuleRuntimeRootKind, ModuleRuntimeRootLimit, ModuleRuntimeRootLimitError,
    ModuleRuntimeRootSet, ModuleRuntimeRootSetError, ModuleRuntimeStore, TraceModuleRuntimeFlow,
};
pub use module_tree::{
    GetModuleTreePage, ModuleTreeBoundaryEvidence, ModuleTreeChildState, ModuleTreeControl,
    ModuleTreeControlError, ModuleTreeDisplayName, ModuleTreeEntry, ModuleTreeEntryError,
    ModuleTreeEntryKind, ModuleTreeFailure, ModuleTreeFuture, ModuleTreeLoadResult, ModuleTreePage,
    ModuleTreePageError, ModuleTreePageSize, ModuleTreePageSizeError, ModuleTreeQuery,
    ModuleTreeStore,
};
pub use mutating_agent_controller::{
    BuiltinProcessVerificationEvidenceFactory, ConservativeProcessVerificationEvidenceFactory,
    ExecuteMutatingAgentAction, LoopbackApprovalAdmission, LoopbackApprovalSequence,
    LoopbackProbeApproval, LoopbackVerificationAuthority, LoopbackVerificationRequest,
    LoopbackVerificationResume, MutationCommandSelection, MutationContextSeed,
    MutationControllerFailure, MutationControllerOutcome, MutationExecutionIds,
    ProcessVerificationEvidenceFactory, ProcessVerificationEvidenceFailure,
    ProcessVerificationEvidenceRequest, PythonIntegerTranslationVerifier,
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
pub use plan_phase::{
    AdmittedInitialPlan, PlanEvidenceKind, PlanPhaseAnchor, PlanPhaseFailure, PlanPhaseHalt,
    PlanPhaseOutcome, PlanPhaseState, PlanStepOffer, PreparedPlanPhase,
};
pub use policy::{
    EvaluateActionPolicy, EvaluateActionPolicyError, EvaluatedPolicyAction, PolicyEvaluationContext,
};
pub use policy_store::{
    GrantPolicyApproval, GrantPolicyApprovalError, PersistPolicyEvaluation, PolicyStore,
    PolicyStoreFailure, PolicyStoreFuture, RevokePolicyApproval, RevokePolicyApprovalError,
};
pub use prepare_next_intake_phase::{PrepareNextIntakePhaseFailure, prepare_next_intake_phase};
pub use process_runner::{
    AuthorizedProcessSpec, ProcessAuthorizationError, ProcessEventSink, ProcessEventSinkError,
    ProcessRunControl, ProcessRunFailure, ProcessRunFuture, ProcessRunner,
};
pub use project_ask::{
    AnswerProjectQuestion, ProjectAskAnswer, ProjectAskCallReceipt, ProjectAskFailure,
    ProjectAskOutcome, ProjectAskPart, ProjectAskPartKind, ProjectAskPhaseReceipt,
    ProjectAskRequest,
};
pub use project_catalog::{
    ActivateCatalogProject, ActivateCatalogProjectError, PROJECT_CATALOG_PAGE_SIZE,
    ProjectCatalogCursor, ProjectCatalogDirection, ProjectCatalogPage, ProjectCatalogQuery,
    ProjectCatalogQueryError,
};
pub use project_map_atlas::*;
pub use project_map_scene::{
    GetProjectMapScene, PROJECT_MAP_SCENE_FOCUS_MODULE_LIMIT,
    PROJECT_MAP_SCENE_OVERVIEW_MODULE_LIMIT, PROJECT_MAP_SCENE_RELATION_LIMIT,
    ProjectMapCardBinding, ProjectMapMappingStatus, ProjectMapScene, ProjectMapSceneControl,
    ProjectMapSceneControlError, ProjectMapSceneError, ProjectMapSceneFailure,
    ProjectMapSceneFuture, ProjectMapSceneLoadResult, ProjectMapSceneModule,
    ProjectMapSceneModuleKind, ProjectMapSceneQuery, ProjectMapSceneRelation, ProjectMapSceneStore,
    ScenePolicyVersion,
};
pub use project_map_search::{
    ProjectMapSearchQuery, ProjectMapSearchQueryError, ProjectMapSearchResult, SearchProjectMap,
    SearchProjectMapFailure,
};
pub use project_map_source_preview::{
    GetProjectMapSourcePreview, ProjectMapSourceHighlight, ProjectMapSourcePreview,
    ProjectMapSourcePreviewControl, ProjectMapSourcePreviewControlError,
    ProjectMapSourcePreviewFailure, ProjectMapSourcePreviewQuery, ProjectMapSourcePreviewResult,
    read_current_source_preview,
};
pub use project_reconciliation::{
    ProjectCatalogRevision, ProjectCatalogRevisionError, ProjectOpenPreparation,
    ProjectReconciliationEvidence, ProjectReconciliationProposal,
};
pub use project_removal::{
    ProjectCatalogAdmin, ProjectCatalogAdminFailure, ProjectCatalogAdminFuture,
    RemoveProjectFromList, RemoveProjectFromListError, RemovedProject,
};
pub use project_settings::{
    GetProjectSettings, GetProjectSettingsError, ProjectCommandSettings, ProjectIgnoreSettings,
    ProjectIgnoreSettingsError, ProjectIgnoreSettingsFuture, ProjectIgnoreSettingsSource,
    ProjectIgnoreSettingsSourceFailure, ProjectSettingsSnapshot,
};
pub use project_status::{
    GetProjectIndexStatus, GetProjectIndexStatusError, ProjectIndexStatus, ProjectSnapshotStatus,
};
pub use project_storage::{
    GetProjectStorageUsage, GetProjectStorageUsageError, ProjectStorageControl,
    ProjectStorageControlError, ProjectStorageFailure, ProjectStorageFuture, ProjectStorageStore,
    ProjectStorageUsage,
};
pub use provider_credentials::*;
pub use published_deep_map_tools::PublishedIndexDeepMapReadTools;
pub use recent_projects::{ListRecentProjects, ListRecentProjectsError};
pub use repository_discovery::{
    RepositoryDiscoverer, RepositoryDiscoveryControl, RepositoryDiscoveryControlError,
    RepositoryDiscoveryFailure,
};
pub use repository_index::{
    IndexRunIdFactory, IndexRunIdFactoryFailure, RefreshRepositoryIndex,
    RefreshRepositoryIndexError, RepositoryIndexCompilation, RepositoryIndexCompiler,
    RepositoryIndexCompilerFailure, RepositoryIndexControl, RepositoryIndexControlError,
    RepositoryIndexMode, RepositoryIndexObservation, RepositoryIndexPhase, RepositoryIndexRefresh,
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
pub use settings::*;
pub use slash_commands::{
    SLASH_COMMAND_LENSES, SLASH_COMMANDS, SlashCommandDescriptor, SlashCommandExecutionProfile,
    SlashCommandLensDescriptor,
};
pub use source_delivery::{
    DeliveredSourceRange, SourceAcquisition, SourceAcquisitionAdmission, SourceAcquisitionOrdinal,
    SourceDeliveryFailure, SourceDeliveryOutcome, SourceDeliveryProgress, SourceDeliveryReceipt,
    SourceDeliveryScope, SourceWireDelivery,
};
pub use source_read::{
    AcquiredSourceRead, SourceReadAdmission, SourceReadFailure, SourceReadFuture, SourceReadState,
    SourceReadStore,
};
pub use task_ledger::{
    CreateTaskLedger, SaveTaskLedger, StoredTaskLedger, TaskLedgerStore, TaskLedgerStoreFailure,
    TaskLedgerStoreFuture, TaskLedgerStoreVersion, TaskLedgerStoreVersionError,
};
pub use task_lens::{
    CompileTaskLens, CompileTaskLensFailure, TaskLensChannelTrace, TaskLensClaimLimit,
    TaskLensClaimReadFuture, TaskLensClaimResult, TaskLensClaimResultError, TaskLensClaimStore,
    TaskLensClaimStoreFailure, TaskLensClaimStoreFuture, TaskLensCompilationTrace, TaskLensControl,
    TaskLensControlError, TaskLensIndexStore, TaskLensIndexStoreFuture, TaskLensSemanticHit,
    TaskLensSemanticLimit, TaskLensSemanticResult, TaskLensSemanticResultError,
    TaskLensSemanticSearch, TaskLensSemanticSearchFailure, TaskLensSemanticSearchFuture,
    TaskLensTimeout, TaskLensTimeoutError,
};
pub use task_lens_workspace::{
    CompileWorkspaceTaskLens, CompileWorkspaceTaskLensFailure, CompileWorkspaceTaskLensResult,
    GetTaskLensTask, ListTaskLensTasks, TaskLensCompilation, TaskLensTaskAnchor,
    TaskLensTaskLoadResult, TaskLensWorkspaceControl, TaskLensWorkspaceFailure,
    TaskLensWorkspaceFuture, TaskLensWorkspaceGoalPage, TaskLensWorkspaceGoalPageError,
    TaskLensWorkspaceStore, TaskLensWorkspaceTask, TaskLensWorkspaceTaskLimit,
    TaskLensWorkspaceTaskLimitError,
};
pub use ui_preferences::{
    AgentWorkspaceLayout, StoredUiPreferences, UiPreferencesError, UiPreferencesStore,
    UiPreferencesStoreFuture, UiPreferencesStoreVersion,
};
pub use verification::{
    DeterministicAcceptanceVerifier, EvaluateStepVerification, EvaluateStepVerificationError,
    OrderVerificationSpecs, StoredVerificationState, StoredVerificationStateError,
    VerificationEvidenceStore, VerificationEvidenceStoreFailure, VerificationEvidenceStoreFuture,
    VerificationOrderingError,
};
pub use verification_inspection::*;
pub use workspace_directory::{
    WorkspaceDirectoryLister, WorkspaceDirectoryListerFuture, WorkspaceDirectoryProgressError,
    WorkspaceDirectoryReadControl, WorkspaceDirectoryReadFailure,
};
pub use workspace_patch::{
    AuthorizedPatchAction, PatchApplyFailure, PatchApplyFuture, PatchAuthorizationError,
    PatchConflictKind, PatchPreviewFailure, PatchPreviewFuture, PatchUpdateBaseFuture,
    WorkspacePatchControl, WorkspacePatchProgressError, WorkspacePatchTool,
};

mod agent_horizon;
pub use agent_horizon::{
    ActivateAgentHorizonPlan, AgentHorizonFailure, AppendAgentHorizonPlan, ContinueSourceAgentPlan,
    PendingPlanningPairDisposition, SourcePlanContinuationOutcome, load_pending_planning_pair,
    validate_pending_planning_pair, validate_pending_planning_transition,
};

//! Versioned, infrastructure-independent IPC boundary types for A^3.

mod agent_goal;
mod deep_map;
mod error;
mod goal_contract;
mod health;
mod index_activity;
mod index_overview;
mod module_card_detail;
mod module_card_evidence;
mod module_card_freshness;
mod module_dependency_graph;
mod module_runtime;
mod module_tree;
mod project;
mod project_map_search;
mod project_rebuild;
mod project_removal;
mod project_status;
mod recent_projects;
mod repository_tree;
mod task_lens;
mod version;

pub use agent_goal::{
    AgentGoalContractV1, AgentGoalCriterionInputV1, AgentGoalCriterionRequirementV1,
    AgentGoalCriterionV1, AgentGoalDraftInputV1, AgentGoalMutationResponseV1, AgentGoalResponseV1,
    AgentGoalResultV1, CreateAgentGoalRequestV1, QueryAgentGoalRequestV1, ReviseAgentGoalRequestV1,
};
pub use deep_map::{
    ControlDeepMapRequestV1, DeepMapActivityStateV1, DeepMapActivityV1, DeepMapBudgetV1,
    DeepMapConfigurationV1, DeepMapControlResponseV1, DeepMapModelV1, DeepMapProgressV1,
    DeepMapStatusResponseV1, DeepMapStatusResultV1, QueryDeepMapRequestV1, StartDeepMapRequestV1,
};
pub use error::{CommandErrorV1, ErrorCodeV1};
pub use goal_contract::{AcceptanceCriterionV1, GoalContractDraftV1, GoalContractV1};
pub use health::{HealthRequestV1, HealthResponseV1, HealthStatusV1, PlatformV1};
pub use index_activity::{
    IndexActivityResponseV1, IndexActivityResultV1, IndexActivityStateV1, IndexActivityV1,
    IndexPhaseV1, QueryIndexActivityRequestV1,
};
pub use index_overview::{
    IndexDiagnosticCodeV1, IndexDiagnosticSeverityV1, IndexDiagnosticV1, IndexFileDiagnosticsV1,
    IndexLanguageV1, IndexOverviewCountsV1, IndexOverviewResponseV1, IndexOverviewResultV1,
    IndexOverviewV1, QueryIndexOverviewRequestV1,
};
pub use module_card_detail::{
    ModuleCardClaimKindV1, ModuleCardClaimStateV1, ModuleCardClaimV1, ModuleCardCoverageBandV1,
    ModuleCardCoverageV1, ModuleCardDetailFieldV1, ModuleCardDetailResponseV1,
    ModuleCardDetailResultV1, ModuleCardDetailV1, ModuleCardFieldKindV1, ModuleCardLifecycleV1,
    ModuleCardValueV1, QueryModuleCardDetailRequestV1,
};
pub use module_card_evidence::{
    ModuleCardEvidenceFreshnessV1, ModuleCardEvidencePayloadV1, ModuleCardEvidenceRelationV1,
    ModuleCardEvidenceResponseV1, ModuleCardEvidenceResultV1, ModuleCardEvidenceRevisionV1,
    ModuleCardEvidenceV1, QueryModuleCardEvidenceRequestV1,
};
pub use module_card_freshness::{
    ModuleCardFreshnessCountsV1, ModuleCardFreshnessReasonCountV1, ModuleCardFreshnessReasonV1,
    ModuleCardFreshnessResponseV1, ModuleCardFreshnessResultV1, ModuleCardFreshnessStatusV1,
    ModuleCardFreshnessV1, QueryModuleCardFreshnessRequestV1,
};
pub use module_dependency_graph::{
    ModuleDependencyEdgeEvidenceV1, ModuleDependencyEdgeV1, ModuleDependencyEndpointV1,
    ModuleDependencyGraphResponseV1, ModuleDependencyGraphResultV1, ModuleDependencyGraphV1,
    ModuleDependencyNodeEvidenceV1, ModuleDependencyNodeV1, ModuleDependencyProviderV1,
    ModuleDependencyRelationV1, ModuleDependencyResolutionV1, ModuleDependencySourcePositionV1,
    ModuleDependencySourceRangeV1, QueryModuleDependencyGraphRequestV1,
};
pub use module_runtime::{
    ModuleRuntimeFlowEdgeV1, ModuleRuntimeFlowHitV1, ModuleRuntimeFlowKindV1,
    ModuleRuntimeFlowRelationV1, ModuleRuntimeFlowResponseV1, ModuleRuntimeFlowResultV1,
    ModuleRuntimeFlowTargetV1, ModuleRuntimeFlowV1, ModuleRuntimeMapResponseV1,
    ModuleRuntimeMapResultV1, ModuleRuntimeMapV1, ModuleRuntimeRootKindV1, ModuleRuntimeRootSetV1,
    ModuleRuntimeRootV1, ModuleRuntimeSymbolKindV1, ModuleRuntimeSymbolV1,
    QueryModuleRuntimeFlowRequestV1, QueryModuleRuntimeMapRequestV1,
};
pub use module_tree::{
    ModuleTreeBoundaryEvidenceV1, ModuleTreeChildStateV1, ModuleTreeEntryKindV1, ModuleTreeEntryV1,
    ModuleTreeFeatureCountV1, ModuleTreePageV1, ModuleTreeResponseV1, ModuleTreeResultV1,
    ModuleTreeRevisionV1, QueryModuleTreeRequestV1,
};
pub use project::{
    GitHeadV1, OpenProjectRequestV1, OpenProjectResponseV1, OpenProjectResultV1, ProjectSummaryV1,
};
pub use project_map_search::{
    ProjectMapExactExplanationV1, ProjectMapLexicalExplanationV1, ProjectMapSearchChannelV1,
    ProjectMapSearchEvidenceV1, ProjectMapSearchHitV1, ProjectMapSearchPriorityV1,
    ProjectMapSearchResponseV1, ProjectMapSearchResultV1, ProjectMapSearchSourceV1,
    ProjectMapSearchSymbolKindV1, ProjectMapSearchTargetV1, ProjectMapSearchV1,
    QueryProjectMapSearchRequestV1,
};
pub use project_rebuild::{RebuildProjectIndexRequestV1, RebuildProjectIndexResponseV1};
pub use project_removal::{RemoveProjectRequestV1, RemoveProjectResponseV1, RemoveProjectResultV1};
pub use project_status::{
    IndexStateV1, ProjectIndexStatusV1, ProjectSnapshotV1, ProjectStatusResponseV1,
    ProjectStatusResultV1, QueryProjectStatusRequestV1, RebuildStateV1,
};
pub use recent_projects::{
    ListRecentProjectsRequestV1, RecentProjectSummaryV1, RecentProjectsResponseV1,
};
pub use repository_tree::{
    QueryRepositoryTreeRequestV1, RepositoryTreeEntryKindV1, RepositoryTreeEntryV1,
    RepositoryTreePageV1, RepositoryTreeResponseV1, RepositoryTreeResultV1,
};
pub use task_lens::{
    CompileTaskLensRequestV1, QueryTaskLensTaskRequestV1, QueryTaskLensTasksRequestV1,
    TaskLensClaimEvidenceV1, TaskLensClaimKindV1, TaskLensClaimPolarityV1,
    TaskLensClaimPredicateV1, TaskLensClaimV1, TaskLensCompileResponseV1, TaskLensCompileResultV1,
    TaskLensEntryReasonV1, TaskLensEntryTargetV1, TaskLensEntryV1, TaskLensModuleKindV1,
    TaskLensPathV1, TaskLensPriorityV1, TaskLensRetrievalChannelV1, TaskLensRetrievalSourceV1,
    TaskLensStepStatusV1, TaskLensStepV1, TaskLensTaskResponseV1, TaskLensTaskResultV1,
    TaskLensTaskSummaryV1, TaskLensTasksResponseV1, TaskLensTasksResultV1, TaskLensV1,
};
pub use version::ProtocolVersion;

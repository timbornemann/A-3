<script lang="ts">
  import { onMount } from 'svelte';
  import GlobalStatusBar from './lib/GlobalStatusBar.svelte';
  import PrimaryNavigation from './lib/PrimaryNavigation.svelte';
  import { UiScheduler } from './lib/ui-scheduler';
  import type { AgentActivityResponseV1 } from './lib/agent-activity';
  import type {
    AgentApprovalControlActionV1,
    AgentApprovalControlResponseV1,
    AgentApprovalResponseV1,
    AgentApprovalV1,
  } from './lib/agent-approval';
  import type {
    AgentTaskControlActionV1,
    AgentTaskControlResponseV1,
    AgentTaskRecoveryResponseV1,
  } from './lib/agent-control';
  import {
    createAgentGoal,
    queryAgentGoal,
    reviseAgentGoal,
    type AgentGoalDraftInputV1,
    type AgentGoalMutationResponseV1,
    type AgentGoalResponseV1,
  } from './lib/agent-goal';
  import type {
    AgentInspectionLogResponseV1,
    AgentInspectionResponseV1,
    AgentInspectionStreamV1,
  } from './lib/agent-inspection';
  import {
    deepMapRecoveryMessage,
    projectActionRecoveryMessage,
    projectOpenRecoveryMessage,
  } from './lib/command-error';
  import {
    cancelDeepMap,
    pauseDeepMap,
    queryDeepMap,
    resumeDeepMap,
    startDeepMap,
    type DeepMapActivityStateV1,
    type DeepMapBudgetV1,
    type DeepMapControlResponseV1,
    type DeepMapStatusResponseV1,
  } from './lib/deep-map';
  import { queryHealth, type HealthResponseV1 } from './lib/health';
  import {
    workspaceAreaFromHash,
    type GlobalRunStatus,
    type GlobalStatusItem,
    type WorkspaceArea,
  } from './lib/global-status';
  import {
    queryIndexActivity,
    type IndexActivityResponseV1,
    type IndexActivityStateV1,
    type IndexPhaseV1,
  } from './lib/index-activity';
  import {
    queryIndexOverview,
    type IndexDiagnosticCodeV1,
    type IndexDiagnosticSeverityV1,
    type IndexLanguageV1,
    type IndexOverviewResponseV1,
  } from './lib/index-overview';
  import {
    queryModuleCardDetail,
    type ModuleCardClaimKindV1,
    type ModuleCardDetailQueryV1,
    type ModuleCardDetailResponseV1,
    type ModuleCardFieldKindV1,
  } from './lib/module-card-detail';
  import {
    queryModuleCardEvidence,
    type ModuleCardEvidenceQueryV1,
    type ModuleCardEvidenceRelationV1,
    type ModuleCardEvidenceResponseV1,
  } from './lib/module-card-evidence';
  import {
    queryModuleCardFreshness,
    type ModuleCardFreshnessReasonV1,
    type ModuleCardFreshnessResponseV1,
  } from './lib/module-card-freshness';
  import {
    queryModuleDependencyGraph,
    type ModuleDependencyEdgeEvidenceV1,
    type ModuleDependencyGraphQueryV1,
    type ModuleDependencyGraphResponseV1,
  } from './lib/module-dependency-graph';
  import {
    queryModuleTree,
    type ModuleTreeEntryV1,
    type ModuleTreeQueryV1,
    type ModuleTreeResponseV1,
  } from './lib/module-tree';
  import {
    queryModuleRuntimeFlow,
    queryModuleRuntimeMap,
    type ModuleRuntimeFlowQueryV1,
    type ModuleRuntimeFlowResponseV1,
    type ModuleRuntimeFlowTargetV1,
    type ModuleRuntimeMapQueryV1,
    type ModuleRuntimeMapResponseV1,
    type ModuleRuntimeRootV1,
    type ModuleRuntimeSymbolV1,
  } from './lib/module-runtime';
  import { openProject, type GitHeadV1, type OpenProjectResponseV1 } from './lib/project';
  import {
    queryProjectMapSearch,
    type ProjectMapExactExplanationV1,
    type ProjectMapLexicalExplanationV1,
    type ProjectMapSearchQueryV1,
    type ProjectMapSearchResponseV1,
    type ProjectMapSearchSourceV1,
    type ProjectMapSearchSymbolKindV1,
    type ProjectMapSearchTargetV1,
  } from './lib/project-map-search';
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
  import { removeProject, type RemoveProjectResponseV1 } from './lib/project-removal';
  import {
    queryProjectStatus,
    type IndexStateV1,
    type ProjectStatusResponseV1,
    type RebuildStateV1,
  } from './lib/project-status';
  import {
    queryRepositoryTree,
    type RepositoryTreeEntryV1,
    type RepositoryTreeQueryV1,
    type RepositoryTreeResponseV1,
  } from './lib/repository-tree';
  import {
    compileTaskLens,
    queryTaskLensTask,
    queryTaskLensTasks,
    type TaskLensClaimPredicateV1,
    type TaskLensClaimV1,
    type TaskLensCompileQueryV1,
    type TaskLensCompileResponseV1,
    type TaskLensEntryTargetV1,
    type TaskLensRetrievalChannelV1,
    type TaskLensStepStatusV1,
    type TaskLensTaskQueryV1,
    type TaskLensTaskResponseV1,
    type TaskLensTasksResponseV1,
  } from './lib/task-lens';

  interface Props {
    agentActivityLoader?: (taskId: string) => Promise<AgentActivityResponseV1>;
    agentApprovalController?: (
      taskId: string,
      approval: AgentApprovalV1,
      action: AgentApprovalControlActionV1,
    ) => Promise<AgentApprovalControlResponseV1>;
    agentApprovalLoader?: (taskId: string) => Promise<AgentApprovalResponseV1>;
    agentGoalCreator?: (draft: AgentGoalDraftInputV1) => Promise<AgentGoalMutationResponseV1>;
    agentGoalLoader?: (taskId: string) => Promise<AgentGoalResponseV1>;
    agentGoalReviser?: (
      taskId: string,
      expectedRevision: number,
      reason: string,
      draft: AgentGoalDraftInputV1,
    ) => Promise<AgentGoalMutationResponseV1>;
    agentGoalTasksLoader?: () => Promise<TaskLensTasksResponseV1>;
    agentInspectionLoader?: (taskId: string) => Promise<AgentInspectionResponseV1>;
    agentInspectionLogLoader?: (
      taskId: string,
      revision: string,
      inspectionId: string,
      stream: AgentInspectionStreamV1,
      offset: number,
    ) => Promise<AgentInspectionLogResponseV1>;
    agentRecoveryLoader?: (taskId: string) => Promise<AgentTaskRecoveryResponseV1>;
    agentRunController?: (
      taskId: string,
      expectedLedgerRevision: number,
      expectedLedgerStoreVersion: string,
      action: AgentTaskControlActionV1,
    ) => Promise<AgentTaskControlResponseV1>;
    healthLoader?: () => Promise<HealthResponseV1>;
    deepMapStatusLoader?: () => Promise<DeepMapStatusResponseV1>;
    deepMapStarter?: (budget: DeepMapBudgetV1) => Promise<DeepMapControlResponseV1>;
    deepMapPauser?: () => Promise<DeepMapControlResponseV1>;
    deepMapResumer?: () => Promise<DeepMapControlResponseV1>;
    deepMapCanceller?: () => Promise<DeepMapControlResponseV1>;
    indexActivityLoader?: () => Promise<IndexActivityResponseV1>;
    indexOverviewLoader?: () => Promise<IndexOverviewResponseV1>;
    moduleCardFreshnessLoader?: () => Promise<ModuleCardFreshnessResponseV1>;
    moduleCardDetailLoader?: (
      query: ModuleCardDetailQueryV1,
    ) => Promise<ModuleCardDetailResponseV1>;
    moduleCardEvidenceLoader?: (
      query: ModuleCardEvidenceQueryV1,
    ) => Promise<ModuleCardEvidenceResponseV1>;
    moduleDependencyGraphLoader?: (
      query: ModuleDependencyGraphQueryV1,
    ) => Promise<ModuleDependencyGraphResponseV1>;
    moduleRuntimeMapLoader?: (
      query: ModuleRuntimeMapQueryV1,
    ) => Promise<ModuleRuntimeMapResponseV1>;
    moduleRuntimeFlowLoader?: (
      query: ModuleRuntimeFlowQueryV1,
    ) => Promise<ModuleRuntimeFlowResponseV1>;
    moduleTreeLoader?: (query: ModuleTreeQueryV1) => Promise<ModuleTreeResponseV1>;
    projectOpener?: () => Promise<OpenProjectResponseV1>;
    projectMapSearchLoader?: (
      query: ProjectMapSearchQueryV1,
    ) => Promise<ProjectMapSearchResponseV1>;
    projectRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    projectRemover?: () => Promise<RemoveProjectResponseV1>;
    projectStatusLoader?: () => Promise<ProjectStatusResponseV1>;
    repositoryTreeLoader?: (query: RepositoryTreeQueryV1) => Promise<RepositoryTreeResponseV1>;
    taskLensTasksLoader?: () => Promise<TaskLensTasksResponseV1>;
    taskLensTaskLoader?: (query: TaskLensTaskQueryV1) => Promise<TaskLensTaskResponseV1>;
    taskLensCompiler?: (query: TaskLensCompileQueryV1) => Promise<TaskLensCompileResponseV1>;
  }

  type AgentGoalWorkspaceComponent = typeof import('./lib/AgentGoalWorkspace.svelte').default;
  type ModuleDependencyGraphComponent =
    typeof import('./lib/ModuleDependencyGraphView.svelte').default;
  type SettingsPanelComponent = typeof import('./lib/SettingsPanel.svelte').default;
  type LazySurfaceState = 'error' | 'idle' | 'loading' | 'ready';

  type ProjectView =
    | { kind: 'idle' }
    | { kind: 'opening' }
    | { kind: 'cancelled' }
    | { kind: 'opened' }
    | { kind: 'error'; message: string };
  type ProjectStatusView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'active'; result: Extract<ProjectStatusResponseV1['result'], { status: 'active' }> }
    | { kind: 'error' };
  type IndexActivityView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'active'; result: Extract<IndexActivityResponseV1['result'], { status: 'active' }> }
    | { kind: 'error' };
  type IndexOverviewView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'published';
        result: Extract<IndexOverviewResponseV1['result'], { status: 'published' }>;
      }
    | { kind: 'error' };
  type ModuleCardFreshnessView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'available';
        result: Extract<ModuleCardFreshnessResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleCardDetailView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | { kind: 'moduleUnavailable' }
    | { kind: 'cardUnavailable' }
    | {
        kind: 'available';
        result: Extract<ModuleCardDetailResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleCardEvidenceView =
    | { kind: 'idle' }
    | { evidenceId: string; kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | { kind: 'moduleUnavailable' }
    | { kind: 'cardUnavailable' }
    | { kind: 'selectionChanged' }
    | { kind: 'evidenceUnavailable' }
    | {
        kind: 'available';
        result: Extract<ModuleCardEvidenceResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type RepositoryTreeView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'available';
        result: Extract<RepositoryTreeResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ProjectMapSearchView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { channel: 'exact' | 'lexical'; kind: 'projectionUnavailable' }
    | {
        kind: 'available';
        result: Extract<ProjectMapSearchResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type TaskLensTasksView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | {
        kind: 'available';
        result: Extract<TaskLensTasksResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type TaskLensTaskView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'taskNotFound' }
    | { kind: 'ledgerUnavailable' }
    | { currentGoalRevision: number; kind: 'goalRevisionMismatch'; ledgerGoalRevision: number }
    | {
        kind: 'available';
        result: Extract<TaskLensTaskResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type TaskLensView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'taskNotFound' }
    | { kind: 'ledgerUnavailable' }
    | { currentGoalRevision: number; kind: 'goalRevisionMismatch'; ledgerGoalRevision: number }
    | { kind: 'stepUnavailable' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'available';
        result: Extract<TaskLensCompileResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleTreeView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | {
        kind: 'available';
        result: Extract<ModuleTreeResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleDependencyGraphView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | { kind: 'centerUnavailable' }
    | {
        kind: 'available';
        result: Extract<ModuleDependencyGraphResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleRuntimeMapView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | { kind: 'moduleUnavailable' }
    | { kind: 'stale' }
    | {
        kind: 'available';
        result: Extract<ModuleRuntimeMapResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleRuntimeFlowView =
    | { kind: 'idle' }
    | { kind: 'loading'; rootName: string }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | { kind: 'projectionUnavailable' }
    | { kind: 'publicationChanged' }
    | { kind: 'moduleUnavailable' }
    | { kind: 'rootUnavailable' }
    | {
        kind: 'available';
        result: Extract<ModuleRuntimeFlowResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type ModuleRuntimeEvidence =
    | { kind: 'symbol'; symbol: ModuleRuntimeSymbolV1 }
    | { kind: 'edge'; evidence: ModuleDependencyEdgeEvidenceV1 }
    | {
        kind: 'file';
        contentHash: string;
        evidenceId: string;
        pathHex: string;
      };
  interface ModuleTreeBreadcrumb {
    moduleId: string;
    name: string;
  }
  interface RepositoryTreeBreadcrumb {
    name: string;
    pathHex: string;
  }
  type DeepMapView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'unavailable' }
    | {
        kind: 'available';
        result: Extract<DeepMapStatusResponseV1['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type DeepMapActionView =
    { kind: 'idle' } | { kind: 'submitting' } | { kind: 'error'; message: string };
  type RebuildView = { kind: 'idle' } | { kind: 'submitting' } | { kind: 'error'; message: string };
  type RemovalView =
    | { kind: 'idle' }
    | { kind: 'confirming' }
    | { kind: 'submitting' }
    | { kind: 'removed' }
    | { kind: 'error'; message: string };
  type ProjectDialogView = 'index' | 'overview' | 'maintenance';

  let {
    agentActivityLoader,
    agentApprovalController,
    agentApprovalLoader,
    agentGoalCreator = createAgentGoal,
    agentGoalLoader = queryAgentGoal,
    agentGoalReviser = reviseAgentGoal,
    agentGoalTasksLoader = queryTaskLensTasks,
    agentInspectionLoader,
    agentInspectionLogLoader,
    agentRecoveryLoader,
    agentRunController,
    healthLoader = queryHealth,
    deepMapStatusLoader = queryDeepMap,
    deepMapStarter = startDeepMap,
    deepMapPauser = pauseDeepMap,
    deepMapResumer = resumeDeepMap,
    deepMapCanceller = cancelDeepMap,
    indexActivityLoader = queryIndexActivity,
    indexOverviewLoader = queryIndexOverview,
    moduleCardFreshnessLoader = queryModuleCardFreshness,
    moduleCardDetailLoader = queryModuleCardDetail,
    moduleCardEvidenceLoader = queryModuleCardEvidence,
    moduleDependencyGraphLoader = queryModuleDependencyGraph,
    moduleRuntimeMapLoader = queryModuleRuntimeMap,
    moduleRuntimeFlowLoader = queryModuleRuntimeFlow,
    moduleTreeLoader = queryModuleTree,
    projectOpener = openProject,
    projectMapSearchLoader = queryProjectMapSearch,
    projectRebuilder = rebuildProjectIndex,
    projectRemover = removeProject,
    projectStatusLoader = queryProjectStatus,
    repositoryTreeLoader = queryRepositoryTree,
    taskLensTasksLoader = queryTaskLensTasks,
    taskLensTaskLoader = queryTaskLensTask,
    taskLensCompiler = compileTaskLens,
  }: Props = $props();
  let projectView = $state<ProjectView>({ kind: 'idle' });
  let projectStatusView = $state<ProjectStatusView>({ kind: 'loading' });
  let indexActivityView = $state<IndexActivityView>({ kind: 'loading' });
  let indexOverviewView = $state<IndexOverviewView>({ kind: 'loading' });
  let moduleCardFreshnessView = $state<ModuleCardFreshnessView>({ kind: 'loading' });
  let moduleCardDetailView = $state<ModuleCardDetailView>({ kind: 'idle' });
  let moduleCardSelection = $state<{ moduleId: string; name: string } | null>(null);
  let moduleCardEvidenceView = $state<ModuleCardEvidenceView>({ kind: 'idle' });
  let selectedModuleCardEvidenceId = $state<string | null>(null);
  let moduleTreeView = $state<ModuleTreeView>({ kind: 'loading' });
  let moduleTreeBreadcrumbs = $state<ModuleTreeBreadcrumb[]>([]);
  let moduleTreeLoadingMore = $state(false);
  let moduleTreePageCursors = $state<(string | null)[]>([null]);
  let moduleTreePageIndex = $state(0);
  let moduleDependencyGraphView = $state<ModuleDependencyGraphView>({ kind: 'idle' });
  let moduleDependencySelection = $state<{ moduleId: string; name: string } | null>(null);
  let selectedDependencyEvidence = $state<ModuleDependencyEdgeEvidenceV1 | null>(null);
  let moduleRuntimeMapView = $state<ModuleRuntimeMapView>({ kind: 'idle' });
  let moduleRuntimeFlowView = $state<ModuleRuntimeFlowView>({ kind: 'idle' });
  let moduleRuntimeSelection = $state<{ moduleId: string; name: string } | null>(null);
  let moduleRuntimeEntrypointLimit = $state(20);
  let moduleRuntimeTestLimit = $state(20);
  let selectedModuleRuntimeEvidence = $state<ModuleRuntimeEvidence | null>(null);
  let repositoryTreeView = $state<RepositoryTreeView>({ kind: 'loading' });
  let repositoryTreeBreadcrumbs = $state<RepositoryTreeBreadcrumb[]>([]);
  let repositoryTreeLoadingMore = $state(false);
  let repositoryTreePageCursors = $state<(string | null)[]>([null]);
  let repositoryTreePageIndex = $state(0);
  let projectMapSearchText = $state('');
  let projectMapSearchView = $state<ProjectMapSearchView>({ kind: 'idle' });
  let projectMapMode = $state<'search' | 'taskLens'>('search');
  let mapWorkspaceView = $state<'search' | 'explore' | 'module' | 'mapping'>('search');
  let moduleWorkspaceView = $state<'card' | 'runtime' | 'dependencies'>('card');
  let taskLensTasksView = $state<TaskLensTasksView>({ kind: 'idle' });
  let taskLensTaskView = $state<TaskLensTaskView>({ kind: 'idle' });
  let taskLensView = $state<TaskLensView>({ kind: 'idle' });
  let selectedTaskLensTaskId = $state('');
  let selectedTaskLensStepId = $state('');
  let deepMapView = $state<DeepMapView>({ kind: 'loading' });
  let deepMapActionView = $state<DeepMapActionView>({ kind: 'idle' });
  let deepMapBudget = $state<DeepMapBudgetV1>({
    tokenLimit: 32_000,
    timeLimitMillis: 120_000,
    toolCallLimit: 64,
  });
  let deepMapBudgetProfile = $state<string | null>(null);
  let rebuildView = $state<RebuildView>({ kind: 'idle' });
  let removalView = $state<RemovalView>({ kind: 'idle' });
  let projectDialogOpen = $state(false);
  let projectDialogView = $state<ProjectDialogView>('overview');
  let indexActivityObserved = false;
  let moduleRuntimeMapRequestSequence = 0;
  let moduleRuntimeFlowRequestSequence = 0;
  let moduleCardDetailRequestSequence = 0;
  let moduleCardEvidenceRequestSequence = 0;
  let moduleDependencyGraphRequestSequence = 0;
  let moduleTreeRequestSequence = 0;
  let projectMapSearchRequestSequence = 0;
  let repositoryTreeRequestSequence = 0;
  let taskLensTasksRequestSequence = 0;
  let taskLensTaskRequestSequence = 0;
  let taskLensCompileRequestSequence = 0;
  let currentWorkspaceArea = $state<WorkspaceArea>('projects');
  let globalRunStatus = $state<GlobalRunStatus>({ kind: 'loading' });
  let uiScheduler: UiScheduler | null = null;
  let appMounted = false;
  let workspaceContent: HTMLElement;
  let agentWorkspaceBoundary: HTMLElement;
  let agentWorkspaceComponent = $state<AgentGoalWorkspaceComponent | null>(null);
  let agentWorkspaceState = $state<LazySurfaceState>('idle');
  let settingsBoundary: HTMLElement;
  let settingsComponent = $state<SettingsPanelComponent | null>(null);
  let settingsState = $state<LazySurfaceState>('idle');
  let ModuleDependencyGraph = $state<ModuleDependencyGraphComponent | null>(null);
  let moduleDependencyGraphChunkState = $state<LazySurfaceState>('idle');

  const workspaceTitles: Record<WorkspaceArea, string> = {
    projects: 'Projects',
    map: 'Project Map',
    agent: 'Agent',
    settings: 'Settings',
  };

  function navigateWorkspace(area: WorkspaceArea): void {
    currentWorkspaceArea = area;
    resetWorkspaceScroll();
    if (area === 'agent') void loadAgentWorkspaceChunk();
    if (area === 'settings') void loadSettingsChunk();
    document.getElementById(area)?.focus({ preventScroll: true });
  }

  function resetWorkspaceScroll(): void {
    workspaceContent.scrollTop = 0;
    workspaceContent.scrollLeft = 0;
  }

  function syncWorkspaceRoute(focusTarget: boolean): void {
    currentWorkspaceArea = workspaceAreaFromHash(window.location.hash);
    if (focusTarget) resetWorkspaceScroll();
    if (currentWorkspaceArea === 'agent') void loadAgentWorkspaceChunk();
    if (currentWorkspaceArea === 'settings') void loadSettingsChunk();
    if (focusTarget) document.getElementById(currentWorkspaceArea)?.focus({ preventScroll: true });
  }

  function updateGlobalRunStatus(status: GlobalRunStatus): void {
    if (projectStatusView.kind === 'error') {
      globalRunStatus = { kind: 'error' };
    } else if (projectStatusView.kind === 'loading') {
      globalRunStatus = { kind: 'loading' };
    } else if (projectStatusView.kind === 'noProject') {
      globalRunStatus = { kind: 'noProject' };
    } else {
      globalRunStatus = status;
    }
  }

  function globalProjectItem(): GlobalStatusItem {
    switch (projectStatusView.kind) {
      case 'active':
        return { tone: 'ready', value: projectStatusView.result.project.worktreeRootDisplay };
      case 'error':
        return { tone: 'failed', value: 'Projektstatus nicht verfügbar' };
      case 'loading':
        return { tone: 'pending', value: 'Projektstatus wird geladen' };
      case 'noProject':
        return { tone: 'neutral', value: 'Kein Projekt geöffnet' };
    }
  }

  function globalIndexItem(): GlobalStatusItem {
    if (projectStatusView.kind === 'noProject') {
      return { tone: 'neutral', value: 'Kein Projekt geöffnet' };
    }
    if (projectStatusView.kind === 'error' || indexActivityView.kind === 'error') {
      return { tone: 'failed', value: 'Indexstatus nicht verfügbar' };
    }
    if (projectStatusView.kind === 'loading' || indexActivityView.kind === 'loading') {
      return { tone: 'pending', value: 'Indexstatus wird geladen' };
    }
    if (indexActivityView.kind === 'active' && indexActivityView.result.activity.phase !== null) {
      const activity = indexActivityView.result.activity;
      const activePhase = activity.phase;
      if (activePhase === null) return { tone: 'pending', value: 'Indexstatus wird geladen' };
      const phaseOrdinal = Math.min(activity.completedPhases + 1, activity.totalPhases);
      const phase = `${indexPhaseLabel(activePhase)} · ${phaseOrdinal}/${activity.totalPhases}`;
      const tone = ['failed', 'cancelled'].includes(activity.state)
        ? 'failed'
        : activity.state === 'succeeded'
          ? 'ready'
          : 'pending';
      return { tone, value: phase };
    }
    if (indexOverviewView.kind === 'published') {
      return {
        tone: 'ready',
        value: `Snapshot ${indexOverviewView.result.overview.snapshotId.slice(0, 12)}`,
      };
    }
    if (indexOverviewView.kind === 'error') {
      return { tone: 'failed', value: 'Publikation nicht verfügbar' };
    }
    if (indexOverviewView.kind === 'noPublishedIndex') {
      return { tone: 'warning', value: 'Noch kein veröffentlichter Snapshot' };
    }
    return { tone: 'pending', value: 'Indexstatus wird geladen' };
  }

  function globalModelItem(): GlobalStatusItem {
    switch (deepMapView.kind) {
      case 'available':
        return {
          tone: 'ready',
          value: `Mapping bereit · ${deepMapView.result.configuration.model.modelId}`,
        };
      case 'error':
        return { tone: 'failed', value: 'Modellstatus nicht verfügbar' };
      case 'loading':
        return { tone: 'pending', value: 'Modellstatus wird geladen' };
      case 'noProject':
        return { tone: 'neutral', value: 'Kein Projekt geöffnet' };
      case 'unavailable':
        return { tone: 'warning', value: 'Kein verifiziertes Mapping-Modell' };
    }
  }

  function globalRunItem(): GlobalStatusItem {
    if (globalRunStatus.kind === 'available') {
      const labels = {
        awaitApproval: 'Wartet auf Freigabe',
        cancelled: 'Abgebrochen',
        done: 'Done',
        execute: 'Ausführung',
        failed: 'Fehlgeschlagen',
        intake: 'Aufnahme',
        localize: 'Kontextsuche',
        plan: 'Planung',
        replan: 'Neuplanung',
        verify: 'Verifikation',
      } as const;
      const tone =
        globalRunStatus.state === 'done'
          ? 'ready'
          : ['failed', 'cancelled'].includes(globalRunStatus.state)
            ? 'failed'
            : globalRunStatus.state === 'awaitApproval'
              ? 'warning'
              : 'pending';
      return { tone, value: labels[globalRunStatus.state] };
    }
    const projections: Record<Exclude<GlobalRunStatus['kind'], 'available'>, GlobalStatusItem> = {
      error: { tone: 'failed', value: 'Runstatus nicht verfügbar' },
      idle: { tone: 'neutral', value: 'Kein Run ausgewählt' },
      loading: { tone: 'pending', value: 'Runstatus wird geladen' },
      noProject: { tone: 'neutral', value: 'Kein Projekt geöffnet' },
      unavailable: { tone: 'warning', value: 'Kein aktueller Run verfügbar' },
    };
    return projections[globalRunStatus.kind];
  }

  function projectGeneration(): number | null {
    return uiScheduler?.generation ?? null;
  }

  function isCurrentProjectGeneration(generation: number | null): boolean {
    return generation === null || uiScheduler?.isCurrent(generation) === true;
  }

  function commitProjectView(key: string, generation: number | null, commit: () => void): void {
    if (generation === null || uiScheduler === null) {
      commit();
      return;
    }
    uiScheduler.queueCommit(key, generation, commit);
  }

  async function loadAgentWorkspaceChunk(): Promise<void> {
    if (agentWorkspaceState === 'loading' || agentWorkspaceState === 'ready') return;
    agentWorkspaceState = 'loading';
    try {
      const component = await import('./lib/AgentGoalWorkspace.svelte');
      if (!appMounted) return;
      agentWorkspaceComponent = component.default;
      agentWorkspaceState = 'ready';
    } catch {
      if (appMounted) agentWorkspaceState = 'error';
    }
  }

  async function loadSettingsChunk(): Promise<void> {
    if (settingsState === 'loading' || settingsState === 'ready') return;
    settingsState = 'loading';
    try {
      const component = await import('./lib/SettingsPanel.svelte');
      if (!appMounted) return;
      settingsComponent = component.default;
      settingsState = 'ready';
    } catch {
      if (appMounted) settingsState = 'error';
    }
  }

  async function loadModuleDependencyGraphChunk(): Promise<void> {
    if (
      moduleDependencyGraphChunkState === 'loading' ||
      moduleDependencyGraphChunkState === 'ready'
    )
      return;
    moduleDependencyGraphChunkState = 'loading';
    try {
      const component = await import('./lib/ModuleDependencyGraphView.svelte');
      if (!appMounted) return;
      ModuleDependencyGraph = component.default;
      moduleDependencyGraphChunkState = 'ready';
    } catch {
      if (appMounted) moduleDependencyGraphChunkState = 'error';
    }
  }

  function observeLazySurface(element: HTMLElement, load: () => Promise<void>): void {
    if (typeof IntersectionObserver === 'undefined') {
      void load();
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        void load();
      },
      { rootMargin: '600px 0px' },
    );
    observer.observe(element);
    uiScheduler?.ownAppCleanup(() => observer.disconnect());
  }

  function resetModuleCardEvidence(): void {
    moduleCardEvidenceRequestSequence += 1;
    moduleCardEvidenceView = { kind: 'idle' };
    selectedModuleCardEvidenceId = null;
  }

  function resetModuleCardDetail(kind: 'idle' | 'noProject'): void {
    moduleCardDetailRequestSequence += 1;
    resetModuleCardEvidence();
    moduleCardDetailView = { kind };
    moduleCardSelection = null;
  }

  function resetModuleRuntime(kind: 'idle' | 'noProject'): void {
    moduleRuntimeMapRequestSequence += 1;
    moduleRuntimeFlowRequestSequence += 1;
    moduleRuntimeMapView = { kind };
    moduleRuntimeFlowView = { kind };
    moduleRuntimeSelection = null;
    moduleRuntimeEntrypointLimit = 20;
    moduleRuntimeTestLimit = 20;
    selectedModuleRuntimeEvidence = null;
  }

  function resetProjectMapSearch(kind: 'idle' | 'noProject'): void {
    projectMapSearchRequestSequence += 1;
    projectMapSearchView = { kind };
  }

  function resetTaskLens(kind: 'idle' | 'noProject'): void {
    taskLensTasksRequestSequence += 1;
    taskLensTaskRequestSequence += 1;
    taskLensCompileRequestSequence += 1;
    taskLensTasksView = { kind };
    taskLensTaskView = { kind };
    taskLensView = { kind };
    selectedTaskLensTaskId = '';
    selectedTaskLensStepId = '';
  }

  function resetProjectOwnedUi(kind: 'idle' | 'noProject'): void {
    const noProject = kind === 'noProject';
    indexActivityView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    indexOverviewView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    moduleCardFreshnessView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    moduleTreeRequestSequence += 1;
    moduleTreeView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    moduleTreeBreadcrumbs = [];
    moduleTreeLoadingMore = false;
    moduleTreePageCursors = [null];
    moduleTreePageIndex = 0;
    moduleDependencyGraphRequestSequence += 1;
    moduleDependencyGraphView = { kind };
    moduleDependencySelection = null;
    selectedDependencyEvidence = null;
    resetModuleCardDetail(kind);
    resetModuleRuntime(kind);
    repositoryTreeRequestSequence += 1;
    repositoryTreeView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    repositoryTreeBreadcrumbs = [];
    repositoryTreeLoadingMore = false;
    repositoryTreePageCursors = [null];
    repositoryTreePageIndex = 0;
    projectMapSearchText = '';
    resetProjectMapSearch(kind);
    projectMapMode = 'search';
    resetTaskLens(kind);
    deepMapView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    deepMapActionView = { kind: 'idle' };
    deepMapBudgetProfile = null;
    projectDialogOpen = false;
    projectDialogView = 'overview';
    globalRunStatus = noProject ? { kind: 'noProject' } : { kind: 'idle' };
    indexActivityObserved = false;
  }

  onMount(() => {
    const scheduler = new UiScheduler({
      cancel: (frameId) => window.cancelAnimationFrame(frameId),
      request: (callback) => window.requestAnimationFrame(callback),
    });
    uiScheduler = scheduler;
    appMounted = true;
    syncWorkspaceRoute(false);
    const handleHashChange = () => syncWorkspaceRoute(true);
    window.addEventListener('hashchange', handleHashChange);
    scheduler.ownAppCleanup(() => window.removeEventListener('hashchange', handleHashChange));
    observeLazySurface(agentWorkspaceBoundary, loadAgentWorkspaceChunk);
    observeLazySurface(settingsBoundary, loadSettingsChunk);
    void initializeProjectViews();
    const activityTimer = window.setInterval(() => {
      pollProjectActivity();
    }, 500);
    scheduler.ownAppCleanup(() => window.clearInterval(activityTimer));
    return () => {
      appMounted = false;
      scheduler.dispose();
      if (uiScheduler === scheduler) uiScheduler = null;
    };
  });

  async function initializeProjectViews(): Promise<void> {
    await loadProjectStatus();
    pollProjectActivity();
    await Promise.all([
      loadIndexOverview(),
      loadModuleCardFreshness(),
      loadModuleTreeRoot(),
      loadRepositoryTreeRoot(),
    ]);
  }

  function pollProjectActivity(): void {
    if (uiScheduler === null) {
      void Promise.all([loadIndexActivity(), loadDeepMap()]);
      return;
    }
    uiScheduler.poll('project-activity', async (generation) => {
      await Promise.all([loadIndexActivity(generation), loadDeepMap(generation)]);
    });
  }

  async function loadIndexActivity(generation = projectGeneration()): Promise<void> {
    try {
      const response = await indexActivityLoader();
      commitProjectView('index-activity', generation, () => {
        const previousSucceeded =
          indexActivityView.kind === 'active' &&
          indexActivityView.result.activity.state === 'succeeded';
        indexActivityView =
          response.result.status === 'active'
            ? { kind: 'active', result: response.result }
            : { kind: 'noProject' };
        if (
          indexActivityObserved &&
          response.result.status === 'active' &&
          response.result.activity.state === 'succeeded' &&
          !previousSucceeded
        ) {
          void loadIndexOverview();
          void loadModuleCardFreshness();
          void loadModuleTreeRoot();
          if (moduleDependencySelection !== null) {
            void loadModuleDependencyGraph(
              moduleDependencySelection.moduleId,
              moduleDependencySelection.name,
            );
          }
          if (moduleRuntimeSelection !== null) {
            void loadModuleRuntimeMap(moduleRuntimeSelection.moduleId, moduleRuntimeSelection.name);
          }
          if (moduleCardSelection !== null) {
            void loadModuleCardDetail(moduleCardSelection.moduleId, moduleCardSelection.name);
          }
          resetProjectMapSearch('idle');
          taskLensCompileRequestSequence += 1;
          taskLensView = { kind: 'idle' };
          void loadRepositoryTreeRoot();
        } else if (response.result.status === 'noProject') {
          resetProjectOwnedUi('noProject');
        }
        indexActivityObserved = true;
      });
    } catch {
      commitProjectView('index-activity', generation, () => {
        indexActivityView = { kind: 'error' };
      });
    }
  }

  async function loadIndexOverview(generation = projectGeneration()): Promise<void> {
    if (!isCurrentProjectGeneration(generation)) return;
    indexOverviewView = { kind: 'loading' };
    try {
      const response = await indexOverviewLoader();
      if (!isCurrentProjectGeneration(generation)) return;
      if (response.result.status === 'published') {
        indexOverviewView = { kind: 'published', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        indexOverviewView = { kind: 'noPublishedIndex' };
      } else {
        indexOverviewView = { kind: 'noProject' };
      }
    } catch {
      if (isCurrentProjectGeneration(generation)) indexOverviewView = { kind: 'error' };
    }
  }

  async function loadModuleCardFreshness(generation = projectGeneration()): Promise<void> {
    if (!isCurrentProjectGeneration(generation)) return;
    moduleCardFreshnessView = { kind: 'loading' };
    try {
      const response = await moduleCardFreshnessLoader();
      if (!isCurrentProjectGeneration(generation)) return;
      if (response.result.status === 'available') {
        moduleCardFreshnessView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleCardFreshnessView = { kind: 'noPublishedIndex' };
      } else {
        moduleCardFreshnessView = { kind: 'noProject' };
      }
    } catch {
      if (isCurrentProjectGeneration(generation)) moduleCardFreshnessView = { kind: 'error' };
    }
  }

  async function loadModuleTree(
    parentModuleId: string | null,
    afterModuleId: string | null = null,
    generation = projectGeneration(),
    expectedPublication: { indexRunId: string; snapshotId: string } | null = null,
  ): Promise<void> {
    if (!isCurrentProjectGeneration(generation)) return;
    const requestSequence = ++moduleTreeRequestSequence;
    if (expectedPublication !== null) {
      moduleTreeLoadingMore = true;
    } else {
      moduleTreeView = { kind: 'loading' };
    }
    try {
      const response = await moduleTreeLoader({ afterModuleId, limit: 50, parentModuleId });
      if (requestSequence !== moduleTreeRequestSequence || !isCurrentProjectGeneration(generation))
        return;
      if (response.result.status === 'available') {
        const page = response.result.page;
        if (expectedPublication !== null) {
          const compatible =
            expectedPublication.indexRunId === page.indexRunId &&
            expectedPublication.snapshotId === page.snapshotId &&
            page.parentModuleId === parentModuleId;
          if (!compatible) {
            moduleTreeView = { kind: 'error' };
            moduleTreePageCursors = [null];
            moduleTreePageIndex = 0;
            return;
          }
        }
        moduleTreeView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'projectionUnavailable') {
        moduleTreeView = { kind: 'projectionUnavailable' };
        moduleTreePageCursors = [null];
        moduleTreePageIndex = 0;
      } else if (response.result.status === 'noPublishedIndex') {
        moduleTreeView = { kind: 'noPublishedIndex' };
        moduleTreePageCursors = [null];
        moduleTreePageIndex = 0;
      } else {
        moduleTreeView = { kind: 'noProject' };
        moduleTreeBreadcrumbs = [];
        moduleTreePageCursors = [null];
        moduleTreePageIndex = 0;
      }
    } catch {
      if (requestSequence === moduleTreeRequestSequence && isCurrentProjectGeneration(generation))
        moduleTreeView = { kind: 'error' };
    } finally {
      if (requestSequence === moduleTreeRequestSequence) moduleTreeLoadingMore = false;
    }
  }

  async function loadModuleCardDetail(moduleId: string, name: string): Promise<void> {
    const requestSequence = ++moduleCardDetailRequestSequence;
    resetModuleCardEvidence();
    moduleCardSelection = { moduleId, name };
    moduleCardDetailView = { kind: 'loading' };
    try {
      const response = await moduleCardDetailLoader({ moduleId });
      if (requestSequence !== moduleCardDetailRequestSequence) return;
      if (response.result.status === 'available') {
        moduleCardDetailView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'cardUnavailable') {
        moduleCardDetailView = { kind: 'cardUnavailable' };
      } else if (response.result.status === 'moduleUnavailable') {
        moduleCardDetailView = { kind: 'moduleUnavailable' };
      } else if (response.result.status === 'projectionUnavailable') {
        moduleCardDetailView = { kind: 'projectionUnavailable' };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleCardDetailView = { kind: 'noPublishedIndex' };
      } else {
        resetModuleCardDetail('noProject');
      }
    } catch {
      if (requestSequence === moduleCardDetailRequestSequence) {
        moduleCardDetailView = { kind: 'error' };
      }
    }
  }

  async function openModuleCard(entry: ModuleTreeEntryV1): Promise<void> {
    mapWorkspaceView = 'module';
    moduleWorkspaceView = 'card';
    await loadModuleCardDetail(entry.moduleId, entry.name);
  }

  async function reloadModuleCardDetail(): Promise<void> {
    if (moduleCardSelection === null) return;
    await loadModuleCardDetail(moduleCardSelection.moduleId, moduleCardSelection.name);
  }

  async function inspectModuleCardEvidence(evidenceId: string): Promise<void> {
    if (moduleCardDetailView.kind !== 'available') return;
    const card = moduleCardDetailView.result.detail;
    const requestSequence = ++moduleCardEvidenceRequestSequence;
    selectedModuleCardEvidenceId = evidenceId;
    moduleCardEvidenceView = { evidenceId, kind: 'loading' };
    try {
      const response = await moduleCardEvidenceLoader({
        cardId: card.cardId,
        currentIndexRunId: card.currentIndexRunId,
        currentSnapshotId: card.currentSnapshotId,
        evidenceId,
        moduleId: card.moduleId,
        sourceIndexRunId: card.sourceIndexRunId,
        sourceSnapshotId: card.sourceSnapshotId,
      });
      if (requestSequence !== moduleCardEvidenceRequestSequence) return;
      if (response.result.status === 'available') {
        moduleCardEvidenceView = { kind: 'available', result: response.result };
      } else {
        moduleCardEvidenceView = { kind: response.result.status };
      }
    } catch {
      if (requestSequence === moduleCardEvidenceRequestSequence) {
        moduleCardEvidenceView = { kind: 'error' };
      }
    }
  }

  async function loadModuleTreeRoot(): Promise<void> {
    moduleTreeBreadcrumbs = [];
    moduleTreePageCursors = [null];
    moduleTreePageIndex = 0;
    await loadModuleTree(null);
  }

  async function openModule(entry: ModuleTreeEntryV1): Promise<void> {
    if (entry.childState !== 'hasChildren') return;
    moduleTreeBreadcrumbs = [
      ...moduleTreeBreadcrumbs,
      { moduleId: entry.moduleId, name: entry.name },
    ];
    moduleTreePageCursors = [null];
    moduleTreePageIndex = 0;
    await loadModuleTree(entry.moduleId);
  }

  async function openModuleBreadcrumb(index: number): Promise<void> {
    if (index < 0) {
      await loadModuleTreeRoot();
      return;
    }
    const target = moduleTreeBreadcrumbs[index];
    if (target === undefined) return;
    moduleTreeBreadcrumbs = moduleTreeBreadcrumbs.slice(0, index + 1);
    moduleTreePageCursors = [null];
    moduleTreePageIndex = 0;
    await loadModuleTree(target.moduleId);
  }

  async function loadNextModulePage(): Promise<void> {
    if (moduleTreeView.kind !== 'available') return;
    const page = moduleTreeView.result.page;
    if (page.nextAfterModuleId === null) return;
    const nextCursor = page.nextAfterModuleId;
    await loadModuleTree(page.parentModuleId, nextCursor, projectGeneration(), {
      indexRunId: page.indexRunId,
      snapshotId: page.snapshotId,
    });
    if (moduleTreeView.kind !== 'available') return;
    moduleTreePageCursors = [
      ...moduleTreePageCursors.slice(0, moduleTreePageIndex + 1),
      nextCursor,
    ];
    moduleTreePageIndex += 1;
  }

  async function loadPreviousModulePage(): Promise<void> {
    if (moduleTreeView.kind !== 'available' || moduleTreePageIndex === 0) return;
    const page = moduleTreeView.result.page;
    const previousCursor = moduleTreePageCursors[moduleTreePageIndex - 1];
    if (previousCursor === undefined) return;
    await loadModuleTree(page.parentModuleId, previousCursor, projectGeneration(), {
      indexRunId: page.indexRunId,
      snapshotId: page.snapshotId,
    });
    if (moduleTreeView.kind === 'available') moduleTreePageIndex -= 1;
  }

  async function loadModuleDependencyGraph(
    moduleId: string,
    name: string,
    generation = projectGeneration(),
  ): Promise<void> {
    if (!isCurrentProjectGeneration(generation)) return;
    const requestSequence = ++moduleDependencyGraphRequestSequence;
    moduleDependencySelection = { moduleId, name };
    moduleDependencyGraphView = { kind: 'loading' };
    selectedDependencyEvidence = null;
    try {
      const response = await moduleDependencyGraphLoader({
        centerModuleId: moduleId,
        nodeLimit: 50,
      });
      if (
        requestSequence !== moduleDependencyGraphRequestSequence ||
        !isCurrentProjectGeneration(generation)
      )
        return;
      if (response.result.status === 'available') {
        moduleDependencyGraphView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'centerUnavailable') {
        moduleDependencyGraphView = { kind: 'centerUnavailable' };
      } else if (response.result.status === 'projectionUnavailable') {
        moduleDependencyGraphView = { kind: 'projectionUnavailable' };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleDependencyGraphView = { kind: 'noPublishedIndex' };
      } else {
        moduleDependencyGraphView = { kind: 'noProject' };
        moduleDependencySelection = null;
      }
    } catch {
      if (
        requestSequence === moduleDependencyGraphRequestSequence &&
        isCurrentProjectGeneration(generation)
      )
        moduleDependencyGraphView = { kind: 'error' };
    }
  }

  async function openModuleDependencies(entry: ModuleTreeEntryV1): Promise<void> {
    mapWorkspaceView = 'module';
    moduleWorkspaceView = 'dependencies';
    await Promise.all([
      loadModuleDependencyGraphChunk(),
      loadModuleDependencyGraph(entry.moduleId, entry.name),
    ]);
  }

  async function reloadModuleDependencies(): Promise<void> {
    if (moduleDependencySelection === null) return;
    await Promise.all([
      loadModuleDependencyGraphChunk(),
      loadModuleDependencyGraph(moduleDependencySelection.moduleId, moduleDependencySelection.name),
    ]);
  }

  async function loadModuleRuntimeMap(moduleId: string, name: string): Promise<void> {
    const requestSequence = ++moduleRuntimeMapRequestSequence;
    moduleRuntimeFlowRequestSequence += 1;
    moduleRuntimeSelection = { moduleId, name };
    moduleRuntimeMapView = { kind: 'loading' };
    moduleRuntimeFlowView = { kind: 'idle' };
    selectedModuleRuntimeEvidence = null;
    try {
      const response = await moduleRuntimeMapLoader({
        entrypointLimit: moduleRuntimeEntrypointLimit,
        moduleId,
        testLimit: moduleRuntimeTestLimit,
      });
      if (requestSequence !== moduleRuntimeMapRequestSequence) return;
      if (response.result.status === 'available') {
        moduleRuntimeMapView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'moduleUnavailable') {
        moduleRuntimeMapView = { kind: 'moduleUnavailable' };
      } else if (response.result.status === 'projectionUnavailable') {
        moduleRuntimeMapView = { kind: 'projectionUnavailable' };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleRuntimeMapView = { kind: 'noPublishedIndex' };
      } else {
        moduleRuntimeMapView = { kind: 'noProject' };
        moduleRuntimeSelection = null;
      }
    } catch {
      if (requestSequence === moduleRuntimeMapRequestSequence) {
        moduleRuntimeMapView = { kind: 'error' };
      }
    }
  }

  async function openModuleRuntime(entry: ModuleTreeEntryV1): Promise<void> {
    mapWorkspaceView = 'module';
    moduleWorkspaceView = 'runtime';
    moduleRuntimeEntrypointLimit = 20;
    moduleRuntimeTestLimit = 20;
    await loadModuleRuntimeMap(entry.moduleId, entry.name);
  }

  async function reloadModuleRuntime(): Promise<void> {
    if (moduleRuntimeSelection === null) return;
    await loadModuleRuntimeMap(moduleRuntimeSelection.moduleId, moduleRuntimeSelection.name);
  }

  async function loadMoreModuleRuntimeRoots(kind: 'entrypoint' | 'test'): Promise<void> {
    if (moduleRuntimeMapView.kind !== 'available' || moduleRuntimeSelection === null) return;
    if (kind === 'entrypoint') {
      moduleRuntimeEntrypointLimit = Math.min(256, moduleRuntimeEntrypointLimit + 20);
    } else {
      moduleRuntimeTestLimit = Math.min(256, moduleRuntimeTestLimit + 20);
    }
    await loadModuleRuntimeMap(moduleRuntimeSelection.moduleId, moduleRuntimeSelection.name);
  }

  async function loadModuleRuntimeFlow(root: ModuleRuntimeRootV1): Promise<void> {
    if (moduleRuntimeMapView.kind !== 'available') return;
    const map = moduleRuntimeMapView.result.map;
    const requestSequence = ++moduleRuntimeFlowRequestSequence;
    selectedModuleRuntimeEvidence = { kind: 'symbol', symbol: root.symbol };
    moduleRuntimeFlowView = { kind: 'loading', rootName: root.symbol.name };
    try {
      const response = await moduleRuntimeFlowLoader({
        expectedIndexRunId: map.indexRunId,
        expectedSnapshotId: map.snapshotId,
        kind: root.kind === 'entrypoint' ? 'entrypointCalls' : 'testTargets',
        moduleId: map.moduleId,
        resultLimit: 20,
        rootSymbolId: root.symbol.symbolId,
      });
      if (requestSequence !== moduleRuntimeFlowRequestSequence) return;
      if (response.result.status === 'available') {
        moduleRuntimeFlowView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'publicationChanged') {
        selectedModuleRuntimeEvidence = null;
        moduleRuntimeMapView = { kind: 'stale' };
        moduleRuntimeFlowView = { kind: 'publicationChanged' };
      } else if (response.result.status === 'rootUnavailable') {
        selectedModuleRuntimeEvidence = null;
        moduleRuntimeMapView = { kind: 'stale' };
        moduleRuntimeFlowView = { kind: 'rootUnavailable' };
      } else if (response.result.status === 'moduleUnavailable') {
        selectedModuleRuntimeEvidence = null;
        moduleRuntimeMapView = { kind: 'moduleUnavailable' };
        moduleRuntimeFlowView = { kind: 'moduleUnavailable' };
      } else if (response.result.status === 'projectionUnavailable') {
        moduleRuntimeFlowView = { kind: 'projectionUnavailable' };
      } else if (response.result.status === 'noPublishedIndex') {
        selectedModuleRuntimeEvidence = null;
        moduleRuntimeMapView = { kind: 'noPublishedIndex' };
        moduleRuntimeFlowView = { kind: 'noPublishedIndex' };
      } else {
        resetModuleRuntime('noProject');
      }
    } catch {
      if (requestSequence === moduleRuntimeFlowRequestSequence) {
        moduleRuntimeFlowView = { kind: 'error' };
      }
    }
  }

  async function loadRepositoryTree(
    directoryPathHex: string | null,
    afterNameHex: string | null = null,
    generation = projectGeneration(),
    expectedPublication: { indexRunId: string; snapshotId: string } | null = null,
  ): Promise<void> {
    if (!isCurrentProjectGeneration(generation)) return;
    const requestSequence = ++repositoryTreeRequestSequence;
    if (expectedPublication !== null) {
      repositoryTreeLoadingMore = true;
    } else {
      repositoryTreeView = { kind: 'loading' };
    }
    try {
      const response = await repositoryTreeLoader({
        afterNameHex,
        directoryPathHex,
        limit: 50,
      });
      if (
        requestSequence !== repositoryTreeRequestSequence ||
        !isCurrentProjectGeneration(generation)
      )
        return;
      if (response.result.status === 'available') {
        const page = response.result.page;
        if (expectedPublication !== null) {
          const compatible =
            expectedPublication.indexRunId === page.indexRunId &&
            expectedPublication.snapshotId === page.snapshotId &&
            page.directoryPathHex === directoryPathHex;
          if (!compatible) {
            repositoryTreeView = { kind: 'error' };
            repositoryTreePageCursors = [null];
            repositoryTreePageIndex = 0;
            return;
          }
        }
        repositoryTreeView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        repositoryTreeView = { kind: 'noPublishedIndex' };
        repositoryTreePageCursors = [null];
        repositoryTreePageIndex = 0;
      } else {
        repositoryTreeView = { kind: 'noProject' };
        repositoryTreeBreadcrumbs = [];
        repositoryTreePageCursors = [null];
        repositoryTreePageIndex = 0;
      }
    } catch {
      if (
        requestSequence === repositoryTreeRequestSequence &&
        isCurrentProjectGeneration(generation)
      )
        repositoryTreeView = { kind: 'error' };
    } finally {
      if (requestSequence === repositoryTreeRequestSequence) repositoryTreeLoadingMore = false;
    }
  }

  async function submitProjectMapSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    await runProjectMapSearch();
  }

  async function runProjectMapSearch(): Promise<void> {
    const query = projectMapSearchText.trim();
    if (query.length < 3 || projectMapSearchView.kind === 'loading') return;
    const requestSequence = ++projectMapSearchRequestSequence;
    projectMapSearchView = { kind: 'loading' };
    try {
      const response = await projectMapSearchLoader({ query });
      if (requestSequence !== projectMapSearchRequestSequence) return;
      if (response.result.status === 'available') {
        projectMapSearchView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'projectionUnavailable') {
        projectMapSearchView = {
          channel: response.result.channel,
          kind: 'projectionUnavailable',
        };
      } else if (response.result.status === 'noPublishedIndex') {
        projectMapSearchView = { kind: 'noPublishedIndex' };
      } else {
        projectMapSearchView = { kind: 'noProject' };
      }
    } catch {
      if (requestSequence === projectMapSearchRequestSequence) {
        projectMapSearchView = { kind: 'error' };
      }
    }
  }

  function showProjectMapSearch(): void {
    projectMapMode = 'search';
  }

  function showTaskLens(): void {
    projectMapMode = 'taskLens';
    if (taskLensTasksView.kind === 'idle') void loadTaskLensTasks();
  }

  async function loadTaskLensTasks(): Promise<void> {
    const requestSequence = ++taskLensTasksRequestSequence;
    taskLensTasksView = { kind: 'loading' };
    taskLensTaskRequestSequence += 1;
    taskLensCompileRequestSequence += 1;
    taskLensTaskView = { kind: 'idle' };
    taskLensView = { kind: 'idle' };
    selectedTaskLensTaskId = '';
    selectedTaskLensStepId = '';
    try {
      const response = await taskLensTasksLoader();
      if (requestSequence !== taskLensTasksRequestSequence) return;
      taskLensTasksView =
        response.result.status === 'available'
          ? { kind: 'available', result: response.result }
          : { kind: 'noProject' };
    } catch {
      if (requestSequence === taskLensTasksRequestSequence) taskLensTasksView = { kind: 'error' };
    }
  }

  async function selectTaskLensTask(event: Event): Promise<void> {
    const target = event.currentTarget;
    if (!(target instanceof HTMLSelectElement)) return;
    selectedTaskLensTaskId = target.value;
    selectedTaskLensStepId = '';
    taskLensCompileRequestSequence += 1;
    taskLensView = { kind: 'idle' };
    if (selectedTaskLensTaskId === '') {
      taskLensTaskRequestSequence += 1;
      taskLensTaskView = { kind: 'idle' };
      return;
    }
    const requestSequence = ++taskLensTaskRequestSequence;
    taskLensTaskView = { kind: 'loading' };
    try {
      const response = await taskLensTaskLoader({ taskId: selectedTaskLensTaskId });
      if (requestSequence !== taskLensTaskRequestSequence) return;
      switch (response.result.status) {
        case 'available':
          taskLensTaskView = { kind: 'available', result: response.result };
          break;
        case 'ledgerUnavailable':
          taskLensTaskView = { kind: 'ledgerUnavailable' };
          break;
        case 'goalRevisionMismatch':
          taskLensTaskView = {
            currentGoalRevision: response.result.currentGoalRevision,
            kind: 'goalRevisionMismatch',
            ledgerGoalRevision: response.result.ledgerGoalRevision,
          };
          break;
        case 'taskNotFound':
          taskLensTaskView = { kind: 'taskNotFound' };
          break;
        case 'noProject':
          taskLensTaskView = { kind: 'noProject' };
          break;
      }
    } catch {
      if (requestSequence === taskLensTaskRequestSequence) taskLensTaskView = { kind: 'error' };
    }
  }

  function selectTaskLensStep(event: Event): void {
    const target = event.currentTarget;
    if (!(target instanceof HTMLSelectElement)) return;
    selectedTaskLensStepId = target.value;
    taskLensCompileRequestSequence += 1;
    taskLensView = { kind: 'idle' };
  }

  async function runTaskLensCompile(): Promise<void> {
    if (
      selectedTaskLensTaskId === '' ||
      selectedTaskLensStepId === '' ||
      taskLensView.kind === 'loading'
    ) {
      return;
    }
    const requestSequence = ++taskLensCompileRequestSequence;
    taskLensView = { kind: 'loading' };
    try {
      const response = await taskLensCompiler({
        stepId: selectedTaskLensStepId,
        taskId: selectedTaskLensTaskId,
      });
      if (requestSequence !== taskLensCompileRequestSequence) return;
      switch (response.result.status) {
        case 'available':
          taskLensView = { kind: 'available', result: response.result };
          break;
        case 'goalRevisionMismatch':
          taskLensView = {
            currentGoalRevision: response.result.currentGoalRevision,
            kind: 'goalRevisionMismatch',
            ledgerGoalRevision: response.result.ledgerGoalRevision,
          };
          break;
        case 'ledgerUnavailable':
          taskLensView = { kind: 'ledgerUnavailable' };
          break;
        case 'taskNotFound':
          taskLensView = { kind: 'taskNotFound' };
          break;
        case 'stepUnavailable':
          taskLensView = { kind: 'stepUnavailable' };
          break;
        case 'noPublishedIndex':
          taskLensView = { kind: 'noPublishedIndex' };
          break;
        case 'noProject':
          taskLensView = { kind: 'noProject' };
          break;
      }
    } catch {
      if (requestSequence === taskLensCompileRequestSequence) taskLensView = { kind: 'error' };
    }
  }

  async function loadRepositoryTreeRoot(): Promise<void> {
    repositoryTreeBreadcrumbs = [];
    repositoryTreePageCursors = [null];
    repositoryTreePageIndex = 0;
    await loadRepositoryTree(null);
  }

  async function openRepositoryDirectory(entry: RepositoryTreeEntryV1): Promise<void> {
    if (entry.kind !== 'directory') return;
    repositoryTreeBreadcrumbs = [
      ...repositoryTreeBreadcrumbs,
      { name: entry.name, pathHex: entry.pathHex },
    ];
    repositoryTreePageCursors = [null];
    repositoryTreePageIndex = 0;
    await loadRepositoryTree(entry.pathHex);
  }

  async function openRepositoryBreadcrumb(index: number): Promise<void> {
    if (index < 0) {
      await loadRepositoryTreeRoot();
      return;
    }
    const target = repositoryTreeBreadcrumbs[index];
    if (target === undefined) return;
    repositoryTreeBreadcrumbs = repositoryTreeBreadcrumbs.slice(0, index + 1);
    repositoryTreePageCursors = [null];
    repositoryTreePageIndex = 0;
    await loadRepositoryTree(target.pathHex);
  }

  async function loadNextRepositoryPage(): Promise<void> {
    if (
      repositoryTreeView.kind !== 'available' ||
      repositoryTreeView.result.page.nextAfterNameHex === null
    )
      return;
    const page = repositoryTreeView.result.page;
    const nextCursor = page.nextAfterNameHex;
    await loadRepositoryTree(page.directoryPathHex, nextCursor, projectGeneration(), {
      indexRunId: page.indexRunId,
      snapshotId: page.snapshotId,
    });
    if (repositoryTreeView.kind !== 'available') return;
    repositoryTreePageCursors = [
      ...repositoryTreePageCursors.slice(0, repositoryTreePageIndex + 1),
      nextCursor,
    ];
    repositoryTreePageIndex += 1;
  }

  async function loadPreviousRepositoryPage(): Promise<void> {
    if (repositoryTreeView.kind !== 'available' || repositoryTreePageIndex === 0) return;
    const page = repositoryTreeView.result.page;
    const previousCursor = repositoryTreePageCursors[repositoryTreePageIndex - 1];
    if (previousCursor === undefined) return;
    await loadRepositoryTree(page.directoryPathHex, previousCursor, projectGeneration(), {
      indexRunId: page.indexRunId,
      snapshotId: page.snapshotId,
    });
    if (repositoryTreeView.kind === 'available') repositoryTreePageIndex -= 1;
  }

  async function loadDeepMap(generation = projectGeneration()): Promise<void> {
    try {
      const response = await deepMapStatusLoader();
      commitProjectView('deep-map', generation, () => {
        if (response.result.status === 'available') {
          deepMapView = { kind: 'available', result: response.result };
          if (deepMapBudgetProfile !== response.result.configuration.model.profileId) {
            deepMapBudget = { ...response.result.configuration.defaultBudget };
            deepMapBudgetProfile = response.result.configuration.model.profileId;
          }
        } else if (response.result.status === 'unavailable') {
          deepMapView = { kind: 'unavailable' };
          deepMapBudgetProfile = null;
        } else {
          deepMapView = { kind: 'noProject' };
          deepMapBudgetProfile = null;
        }
      });
    } catch {
      commitProjectView('deep-map', generation, () => {
        deepMapView = { kind: 'error' };
      });
    }
  }

  async function loadProjectStatus(): Promise<void> {
    projectStatusView = { kind: 'loading' };
    try {
      const response = await projectStatusLoader();
      const projectKey =
        response.result.status === 'active' ? response.result.project.worktreeId : null;
      const projectChanged = uiScheduler?.beginProject(projectKey) ?? false;
      if (projectChanged) {
        resetProjectOwnedUi(response.result.status === 'active' ? 'idle' : 'noProject');
      }
      projectStatusView =
        response.result.status === 'active'
          ? { kind: 'active', result: response.result }
          : { kind: 'noProject' };
      if (response.result.status === 'noProject') resetProjectOwnedUi('noProject');
      if (projectChanged && response.result.status === 'active') pollProjectActivity();
    } catch {
      projectStatusView = { kind: 'error' };
      globalRunStatus = { kind: 'error' };
    }
  }

  async function refreshProjectDetails(): Promise<void> {
    await Promise.all([
      loadProjectStatus(),
      loadIndexOverview(),
      loadModuleCardFreshness(),
      reloadModuleCardDetail(),
      loadModuleTreeRoot(),
      loadRepositoryTreeRoot(),
      loadDeepMap(),
    ]);
  }

  async function chooseProject(): Promise<void> {
    projectDialogOpen = false;
    projectView = { kind: 'opening' };
    try {
      const response = await projectOpener();
      if (response.result.status === 'opened') {
        projectView = { kind: 'opened' };
        removalView = { kind: 'idle' };
        if (uiScheduler?.beginProject(response.result.project.worktreeId) ?? false) {
          resetProjectOwnedUi('idle');
        }
        await loadProjectStatus();
        pollProjectActivity();
        await Promise.all([
          loadIndexOverview(),
          loadModuleCardFreshness(),
          loadModuleTreeRoot(),
          loadRepositoryTreeRoot(),
        ]);
      } else {
        projectView = { kind: 'cancelled' };
      }
    } catch (error) {
      projectView = { kind: 'error', message: projectOpenRecoveryMessage(error) };
    }
  }

  async function requestIndexRebuild(): Promise<void> {
    rebuildView = { kind: 'submitting' };
    try {
      await projectRebuilder();
      rebuildView = { kind: 'idle' };
      await loadProjectStatus();
    } catch (error) {
      rebuildView = {
        kind: 'error',
        message: projectActionRecoveryMessage(error, 'rebuild'),
      };
    }
  }

  async function requestDeepMapStart(): Promise<void> {
    deepMapActionView = { kind: 'submitting' };
    try {
      await deepMapStarter({ ...deepMapBudget });
      deepMapActionView = { kind: 'idle' };
      await loadDeepMap();
    } catch (error) {
      deepMapActionView = { kind: 'error', message: deepMapRecoveryMessage(error) };
    }
  }

  async function requestDeepMapControl(
    control: () => Promise<DeepMapControlResponseV1>,
  ): Promise<void> {
    deepMapActionView = { kind: 'submitting' };
    try {
      await control();
      deepMapActionView = { kind: 'idle' };
      await loadDeepMap();
    } catch (error) {
      deepMapActionView = { kind: 'error', message: deepMapRecoveryMessage(error) };
    }
  }

  function requestRemovalConfirmation(): void {
    removalView = { kind: 'confirming' };
  }

  function openProjectDialog(view: ProjectDialogView = 'overview'): void {
    projectDialogView = view;
    projectDialogOpen = true;
  }

  function closeProjectDialog(): void {
    projectDialogOpen = false;
  }

  function cancelRemoval(): void {
    removalView = { kind: 'idle' };
  }

  function presentModal(node: HTMLDialogElement): { destroy: () => void } {
    if (typeof node.showModal === 'function') {
      node.showModal();
    } else {
      node.setAttribute('open', '');
    }
    return {
      destroy: () => {
        if (typeof node.close === 'function' && node.open) node.close();
      },
    };
  }

  async function confirmProjectRemoval(): Promise<void> {
    removalView = { kind: 'submitting' };
    try {
      await projectRemover();
      removalView = { kind: 'removed' };
      projectView = { kind: 'idle' };
      projectStatusView = { kind: 'noProject' };
      projectDialogOpen = false;
      uiScheduler?.beginProject(null);
      resetProjectOwnedUi('noProject');
    } catch (error) {
      removalView = {
        kind: 'error',
        message: projectActionRecoveryMessage(error, 'remove'),
      };
    }
  }

  function branchLabel(head: GitHeadV1): string {
    if (head.kind === 'born') {
      return head.reference === null
        ? 'Detached HEAD'
        : head.reference.replace(/^refs\/heads\//, '');
    }
    return `${head.reference.replace(/^refs\/heads\//, '')} (unborn)`;
  }

  function indexStateLabel(state: IndexStateV1): string {
    const labels: Record<IndexStateV1, string> = {
      notStarted: 'Noch nicht gestartet',
      building: 'Index wird aufgebaut',
      published: 'Veröffentlicht',
      failed: 'Letzter Lauf fehlgeschlagen',
      cancelled: 'Letzter Lauf abgebrochen',
    };
    return labels[state];
  }

  function storageSizeLabel(bytes: string | null): string {
    return bytes === null
      ? 'Nicht verfügbar'
      : `${new Intl.NumberFormat('de-DE').format(BigInt(bytes))} Bytes`;
  }

  function rebuildStateLabel(state: RebuildStateV1): string {
    const labels = {
      idle: 'Bereit',
      queued: 'Rebuild wartet',
      running: 'Regenerierbare Daten werden entfernt',
      succeeded: 'Rebuild abgeschlossen; Neuindexierung angefordert',
      failed: 'Rebuild fehlgeschlagen',
      cancelled: 'Rebuild abgebrochen',
    } as const;
    return labels[state];
  }

  function indexActivityStateLabel(state: IndexActivityStateV1): string {
    const labels: Record<IndexActivityStateV1, string> = {
      idle: 'Noch kein Lauf in dieser Sitzung',
      queued: 'Indexlauf wartet auf einen Worker',
      running: 'Fast Index läuft',
      cancelling: 'Indexlauf wird kontrolliert beendet',
      succeeded: 'Fast Index abgeschlossen',
      failed: 'Indexlauf fehlgeschlagen; veröffentlichter Snapshot bleibt lesbar',
      cancelled: 'Indexlauf abgebrochen; veröffentlichter Snapshot bleibt lesbar',
    };
    return labels[state];
  }

  function indexPhaseLabel(phase: IndexPhaseV1): string {
    const labels: Record<IndexPhaseV1, string> = {
      discover: 'Dateien ermitteln',
      hash: 'Inhalte hashen',
      parse: 'Quellcode parsen',
      link: 'Beziehungen verknüpfen',
      rank: 'Symbole und Module gewichten',
      publish: 'Snapshot atomar veröffentlichen',
    };
    return labels[phase];
  }

  function countLabel(value: string): string {
    return new Intl.NumberFormat('de-DE').format(BigInt(value));
  }

  function moduleKindLabel(entry: ModuleTreeEntryV1): string {
    return entry.kind === 'manifestBoundary' ? 'Manifest-Grenze' : 'Pfad-Grenze';
  }

  function moduleFeatureLabel(feature: ModuleTreeEntryV1['centralSymbols']): string {
    return `${countLabel(feature.count)}${feature.truncated ? '+' : ''}`;
  }

  function moduleRuntimeTargetLabel(target: ModuleRuntimeFlowTargetV1): string {
    return target.kind === 'symbol' ? target.symbol.name : pathDisplayFromHex(target.pathHex);
  }

  function selectModuleRuntimeTargetEvidence(target: ModuleRuntimeFlowTargetV1): void {
    selectedModuleRuntimeEvidence =
      target.kind === 'symbol'
        ? { kind: 'symbol', symbol: target.symbol }
        : {
            kind: 'file',
            contentHash: target.contentHash,
            evidenceId: target.evidenceId,
            pathHex: target.pathHex,
          };
  }

  function pathDisplayFromHex(pathHex: string): string {
    const bytes = new Uint8Array(pathHex.length / 2);
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Number.parseInt(pathHex.slice(index * 2, index * 2 + 2), 16);
    }
    return Array.from(new TextDecoder().decode(bytes))
      .slice(0, 256)
      .map((character) => {
        const codePoint = character.codePointAt(0);
        return codePoint !== undefined &&
          (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f))
          ? '�'
          : character;
      })
      .join('');
  }

  function projectMapSearchTargetLabel(target: ProjectMapSearchTargetV1): string {
    return target.kind === 'symbol' ? target.qualifiedName : target.evidence.pathDisplay;
  }

  function projectMapSearchSourceLabel(source: ProjectMapSearchSourceV1): string {
    return source.channel === 'exact' ? 'Exact' : 'Lexical';
  }

  function projectMapSearchExplanationLabel(source: ProjectMapSearchSourceV1): string {
    if (source.channel === 'lexical') {
      const labels: Record<ProjectMapLexicalExplanationV1, string> = {
        path: 'Pfad',
        qualifiedName: 'qualifizierter Name',
        signature: 'Signatur',
        symbolName: 'Symbolname',
      };
      return labels[source.explanation];
    }
    const labels: Record<ProjectMapExactExplanationV1, string> = {
      entrypointRole: 'Entry-Point-Rolle',
      manifestRole: 'Manifest-Rolle',
      normalizedPathExact: 'exakter Pfad',
      qualifiedNameExact: 'exakter qualifizierter Name',
      qualifiedNamePrefix: 'Präfix des qualifizierten Namens',
      signatureExact: 'exakte Signatur',
      signaturePrefix: 'Signaturpräfix',
      symbolNameExact: 'exakter Symbolname',
      symbolNamePrefix: 'Symbolpräfix',
      testRole: 'Test-Rolle',
    };
    return labels[source.explanation];
  }

  function projectMapSearchSymbolKindLabel(kind: ProjectMapSearchSymbolKindV1): string {
    const labels: Record<ProjectMapSearchSymbolKindV1, string> = {
      class: 'Klasse',
      constant: 'Konstante',
      enum: 'Enum',
      field: 'Feld',
      function: 'Funktion',
      implementation: 'Implementierung',
      interface: 'Interface',
      method: 'Methode',
      module: 'Modul',
      namespace: 'Namespace',
      parameter: 'Parameter',
      static: 'Static',
      struct: 'Struct',
      trait: 'Trait',
      typeAlias: 'Typalias',
      variable: 'Variable',
      variant: 'Variante',
    };
    return labels[kind];
  }

  function taskLensStepStatusLabel(status: TaskLensStepStatusV1): string {
    const labels: Record<TaskLensStepStatusV1, string> = {
      awaitingApproval: 'wartet auf Freigabe',
      blocked: 'blockiert',
      cancelled: 'abgebrochen',
      completed: 'verifiziert abgeschlossen',
      failed: 'fehlgeschlagen',
      inProgress: 'in Arbeit',
      pending: 'wartet auf Abhängigkeiten',
      ready: 'bereit',
      stale: 'erneut zu verifizieren',
      verifying: 'wird verifiziert',
    };
    return labels[status];
  }

  function taskLensTargetLabel(target: TaskLensEntryTargetV1): string {
    switch (target.kind) {
      case 'repository':
        return 'Repository-Anchor';
      case 'module':
        return target.root?.pathDisplay ?? `Graph-Community ${target.moduleId.slice(0, 12)}…`;
      case 'file':
        return target.evidence.pathDisplay;
      case 'symbol':
        return target.name;
      case 'sourceSpan':
        return `${target.evidence.pathDisplay} · Deklaration`;
    }
  }

  function taskLensTargetLevel(target: TaskLensEntryTargetV1): string {
    switch (target.kind) {
      case 'repository':
        return 'L0 Repository';
      case 'module':
        return 'L1 Modul';
      case 'symbol':
        return 'L2 Symbol';
      case 'file':
      case 'sourceSpan':
        return 'L3 Source/Evidence';
    }
  }

  function taskLensChannelLabel(channel: TaskLensRetrievalChannelV1): string {
    const labels: Record<TaskLensRetrievalChannelV1, string> = {
      exact: 'Exact',
      graph: 'Graph',
      lexical: 'Lexical',
      memory: 'Memory + Evidence',
      semantic: 'Semantic · nur Kandidat',
      test: 'Test-Beziehung',
    };
    return labels[channel];
  }

  function taskLensClaimKindLabel(kind: TaskLensClaimV1['kind']): string {
    const labels: Record<TaskLensClaimV1['kind'], string> = {
      fact: 'Fact',
      hypothesis: 'Hypothese · unbewiesen',
      observation: 'Observation',
    };
    return labels[kind];
  }

  function taskLensPredicateLabel(predicate: TaskLensClaimPredicateV1): string {
    switch (predicate.kind) {
      case 'path':
        return `Pfad ${predicate.path.pathDisplay}`;
      case 'symbol':
        return `Symbol ${predicate.symbolId}`;
      case 'relation':
        return `${taskLensEndpointLabel(predicate.source)} ${predicate.relation} ${taskLensEndpointLabel(predicate.target)}`;
      case 'observed':
      case 'architecturalIntent':
        return predicate.statement;
    }
  }

  function taskLensEndpointLabel(
    endpoint: Extract<TaskLensClaimPredicateV1, { kind: 'relation' }>['source'],
  ): string {
    return endpoint.kind === 'symbol' ? endpoint.symbolId : endpoint.pathHex;
  }

  function moduleCardFreshnessReasonLabel(reason: ModuleCardFreshnessReasonV1): string {
    const labels: Record<ModuleCardFreshnessReasonV1, string> = {
      directDependencyChanged: 'Direkte Abhängigkeit geändert',
      evidenceChanged: 'Direkte Evidenz geändert',
      mapperVersionChanged: 'Mapper-Version geändert',
      moduleRemoved: 'Modul entfernt',
      parserVersionChanged: 'Parser-Version geändert',
    };
    return labels[reason];
  }

  function moduleCardFieldLabel(field: ModuleCardFieldKindV1): string {
    const labels: Record<ModuleCardFieldKindV1, string> = {
      dataFlows: 'Datenflüsse',
      dependencies: 'Abhängigkeiten',
      entrypoints: 'Entry Points',
      invariants: 'Invarianten',
      openQuestions: 'Offene Fragen',
      paths: 'Pfade',
      publicSurface: 'Öffentliche Oberfläche',
      purpose: 'Zweck',
      responsibilities: 'Verantwortlichkeiten',
      risks: 'Risiken',
      tests: 'Tests',
      title: 'Titel',
    };
    return labels[field];
  }

  function moduleCardClaimKindLabel(kind: ModuleCardClaimKindV1): string {
    const labels: Record<ModuleCardClaimKindV1, string> = {
      fact: 'Fact',
      hypothesis: 'Hypothesis',
      observation: 'Observation',
    };
    return labels[kind];
  }

  function moduleCardEvidenceRelationLabel(relation: ModuleCardEvidenceRelationV1): string {
    const labels: Record<ModuleCardEvidenceRelationV1, string> = {
      builds: 'baut',
      calls: 'ruft auf',
      configures: 'konfiguriert',
      contains: 'enthält',
      defines: 'definiert',
      documents: 'dokumentiert',
      exports: 'exportiert',
      extends: 'erweitert',
      implements: 'implementiert',
      imports: 'importiert',
      reads: 'liest',
      tests: 'testet',
      writes: 'schreibt',
    };
    return labels[relation];
  }

  function percentageLabel(value: number | null): string {
    return value === null
      ? 'Keine strukturellen Parserdaten'
      : new Intl.NumberFormat('de-DE', {
          maximumFractionDigits: 2,
          minimumFractionDigits: 2,
          style: 'percent',
        }).format(value / 10_000);
  }

  function indexLanguageLabel(language: IndexLanguageV1): string {
    const labels: Record<IndexLanguageV1, string> = {
      generic: 'Generisch',
      python: 'Python',
      rust: 'Rust',
      typeScriptJavaScript: 'TypeScript/JavaScript',
    };
    return labels[language];
  }

  function diagnosticCodeLabel(code: IndexDiagnosticCodeV1): string {
    const labels: Record<IndexDiagnosticCodeV1, string> = {
      invalidEncoding: 'Ungültige Zeichenkodierung',
      missingSyntax: 'Fehlende Syntax',
      outputTruncated: 'Begrenzte Parserausgabe',
      syntaxError: 'Syntaxfehler',
      unsupportedSyntax: 'Nicht unterstützte Syntax',
    };
    return labels[code];
  }

  function diagnosticSeverityLabel(severity: IndexDiagnosticSeverityV1): string {
    const labels: Record<IndexDiagnosticSeverityV1, string> = {
      error: 'Fehler',
      information: 'Hinweis',
      warning: 'Warnung',
    };
    return labels[severity];
  }

  function deepMapStateLabel(state: DeepMapActivityStateV1): string {
    const labels: Record<DeepMapActivityStateV1, string> = {
      idle: 'Bereit für einen bewussten Start',
      queued: 'Deep Map wartet auf einen Worker',
      running: 'Deep Map läuft mit dem angezeigten Budget',
      pausing: 'Pause wird kontrolliert und checkpoint-sicher vorbereitet',
      paused: 'Pausiert; es läuft keine Modellarbeit',
      cancelling: 'Deep Map wird kontrolliert abgebrochen',
      succeeded: 'Deep Map abgeschlossen',
      failed: 'Deep Map fehlgeschlagen',
      cancelled: 'Deep Map abgebrochen',
    };
    return labels[state];
  }

  function deepMapCanStart(state: DeepMapActivityStateV1): boolean {
    return ['idle', 'succeeded', 'failed', 'cancelled'].includes(state);
  }
</script>

<svelte:head>
  <title>A^3</title>
</svelte:head>

<main id="main-content" class="app-shell" data-workspace-area={currentWorkspaceArea} tabindex="-1">
  <a class="skip-link" href="#workspace-content">Zum Arbeitsbereich springen</a>
  <aside class="app-sidebar" aria-label="A^3 Anwendungsnavigation">
    <header class="product-header">
      <img class="product-mark" src="/src-tauri/icons/32x32.png" alt="" />
      <div class="product-identity">
        <h1>A^3</h1>
        <p class="subtitle">Autonomous Agent Assistant</p>
      </div>
    </header>
    <PrimaryNavigation current={currentWorkspaceArea} onNavigate={navigateWorkspace} />
  </aside>

  <section
    class="workspace-shell"
    aria-label={`${workspaceTitles[currentWorkspaceArea]} workspace`}
  >
    <header class="workspace-toolbar">
      <h2>{workspaceTitles[currentWorkspaceArea]}</h2>
      <GlobalStatusBar
        project={globalProjectItem()}
        index={globalIndexItem()}
        model={globalModelItem()}
        run={globalRunItem()}
      />
    </header>

    <div class="workspace-layout">
      <div
        id="workspace-content"
        class="workspace-content"
        tabindex="-1"
        bind:this={workspaceContent}
      >
        <section
          id="projects"
          class="project-card"
          class:project-active={projectStatusView.kind === 'active'}
          aria-label={currentWorkspaceArea === 'map' ? 'Project Map' : undefined}
          aria-labelledby={currentWorkspaceArea === 'projects'
            ? projectStatusView.kind === 'active'
              ? 'active-project-heading'
              : 'project-heading'
            : undefined}
          tabindex="-1"
        >
          {#if projectStatusView.kind !== 'active'}
            <div class="section-heading">
              <div>
                <h2 id="project-heading">Projekt öffnen</h2>
              </div>
            </div>

            <p class="project-copy">
              Wähle den Root eines Git-Worktrees. A^3 erhält nur Zugriff auf diesen ausdrücklich
              gewählten Ordner.
            </p>
            <button
              class="primary-action"
              type="button"
              disabled={projectView.kind === 'opening'}
              onclick={chooseProject}
            >
              {projectView.kind === 'opening'
                ? 'Ordnerdialog geöffnet …'
                : 'Projektordner auswählen'}
            </button>
          {/if}

          {#if projectView.kind === 'cancelled'}
            <p class="project-status" role="status" aria-live="polite">Auswahl abgebrochen.</p>
          {:else if projectView.kind === 'opened' && projectStatusView.kind !== 'active'}
            <p class="ready-label" role="status" aria-live="polite">Worktree sicher geöffnet</p>
          {:else if projectView.kind === 'error'}
            <p class="project-error" role="alert">{projectView.message}</p>
          {/if}

          {#if projectStatusView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Projektstatus wird geladen …
            </p>
          {:else if projectStatusView.kind === 'active'}
            <div class="project-result" aria-labelledby="active-project-heading">
              <div class="projects-workspace-view">
                <div class="active-project-card">
                  <div class="project-folder-mark" aria-hidden="true">
                    <svg viewBox="0 0 24 24">
                      <path d="M3 6.5h7l2 2h9v9H3z"></path>
                    </svg>
                  </div>
                  <div>
                    <h3 id="active-project-heading">Aktives Projekt</h3>
                    <strong>{projectStatusView.result.project.worktreeRootDisplay}</strong>
                    <p>{branchLabel(projectStatusView.result.project.head)}</p>
                  </div>
                </div>
                <div class="project-launcher-actions" aria-label="Projektaktionen">
                  <button
                    class="primary-action"
                    type="button"
                    onclick={() => navigateWorkspace('map')}>Project Map öffnen</button
                  >
                  <button type="button" onclick={() => navigateWorkspace('agent')}
                    >Agent öffnen</button
                  >
                  <button type="button" onclick={() => openProjectDialog()}
                    >Projekt verwalten</button
                  >
                  <button
                    type="button"
                    disabled={projectView.kind === 'opening'}
                    onclick={chooseProject}
                  >
                    {projectView.kind === 'opening'
                      ? 'Ordnerdialog geöffnet …'
                      : 'Anderen Worktree auswählen'}
                  </button>
                </div>

                {#if projectDialogOpen}
                  <dialog
                    class="modal-dialog project-dialog"
                    aria-labelledby="project-dialog-heading"
                    use:presentModal
                    oncancel={(event) => {
                      event.preventDefault();
                      closeProjectDialog();
                    }}
                  >
                    <div class="modal-heading">
                      <h3 id="project-dialog-heading">Projekt verwalten</h3>
                      <button
                        type="button"
                        aria-label="Dialog schließen"
                        onclick={closeProjectDialog}>×</button
                      >
                    </div>
                    <nav class="surface-tabs" aria-label="Projektverwaltung">
                      <button
                        type="button"
                        aria-pressed={projectDialogView === 'overview'}
                        onclick={() => (projectDialogView = 'overview')}>Übersicht</button
                      >
                      <button
                        type="button"
                        aria-pressed={projectDialogView === 'index'}
                        onclick={() => (projectDialogView = 'index')}>Index</button
                      >
                      <button
                        type="button"
                        aria-pressed={projectDialogView === 'maintenance'}
                        onclick={() => (projectDialogView = 'maintenance')}>Wartung</button
                      >
                    </nav>

                    <div class="project-dialog-content">
                      {#if projectDialogView === 'overview'}
                        <dl class="project-grid">
                          <div>
                            <dt>Root</dt>
                            <dd>{projectStatusView.result.project.worktreeRootDisplay}</dd>
                          </div>
                          <div>
                            <dt>Branch</dt>
                            <dd>{branchLabel(projectStatusView.result.project.head)}</dd>
                          </div>
                          <div>
                            <dt>Worktree-ID</dt>
                            <dd>{projectStatusView.result.project.worktreeId}</dd>
                          </div>
                          <div>
                            <dt>Indexstatus</dt>
                            <dd>{indexStateLabel(projectStatusView.result.index.state)}</dd>
                          </div>
                          <div>
                            <dt>Aktueller Indexlauf</dt>
                            {#if indexActivityView.kind === 'active'}
                              <dd>
                                {indexActivityStateLabel(indexActivityView.result.activity.state)}
                              </dd>
                            {:else if indexActivityView.kind === 'loading'}
                              <dd>Wird geladen …</dd>
                            {:else}
                              <dd>Nicht verfügbar</dd>
                            {/if}
                          </div>
                          <div>
                            <dt>A^3-Speicher</dt>
                            <dd>{storageSizeLabel(projectStatusView.result.storageBytes)}</dd>
                          </div>
                          <div>
                            <dt>Letzter Snapshot</dt>
                            {#if projectStatusView.result.index.latestSnapshot === null}
                              <dd>Noch kein Snapshot</dd>
                            {:else}
                              <dd>
                                Generation {projectStatusView.result.index.latestSnapshot
                                  .generation}<br />
                                {projectStatusView.result.index.latestSnapshot.snapshotId}
                              </dd>
                            {/if}
                          </div>
                        </dl>
                      {:else if projectDialogView === 'index'}
                        {#if indexActivityView.kind === 'active' && indexActivityView.result.activity.phase !== null}
                          <div class="index-progress" aria-labelledby="index-progress-heading">
                            <h4 id="index-progress-heading">Fast-Index-Fortschritt</h4>
                            <p role="status" aria-live="polite">
                              {#if indexActivityView.result.activity.completedPhases === indexActivityView.result.activity.totalPhases}
                                Alle {indexActivityView.result.activity.totalPhases} Phasen abgeschlossen:
                                {indexPhaseLabel(indexActivityView.result.activity.phase)}
                              {:else}
                                Phase {indexActivityView.result.activity.completedPhases + 1} von
                                {indexActivityView.result.activity.totalPhases}:
                                {indexPhaseLabel(indexActivityView.result.activity.phase)}
                              {/if}
                            </p>
                            <progress
                              aria-label="Fast-Index-Fortschritt"
                              max={indexActivityView.result.activity.totalPhases}
                              value={indexActivityView.result.activity.completedPhases}
                            ></progress>
                            {#if (indexActivityView.result.activity.state === 'queued' || indexActivityView.result.activity.state === 'running' || indexActivityView.result.activity.state === 'cancelling') && projectStatusView.result.index.publishedSnapshotId !== null}
                              <p>
                                Der zuletzt veröffentlichte Snapshot bleibt während dieses Laufs
                                vollständig lesbar.
                              </p>
                            {/if}
                          </div>
                        {/if}
                        <div class="index-overview" aria-labelledby="index-overview-heading">
                          <h4 id="index-overview-heading">Veröffentlichter Fast Index</h4>
                          {#if indexOverviewView.kind === 'loading'}
                            <p class="project-status" role="status" aria-live="polite">
                              Veröffentlichter Index wird gelesen …
                            </p>
                          {:else if indexOverviewView.kind === 'noPublishedIndex'}
                            <p class="project-status">
                              Noch kein vollständiger Snapshot veröffentlicht. Ein laufender Aufbau
                              bleibt davon getrennt.
                            </p>
                          {:else if indexOverviewView.kind === 'published'}
                            <p class="index-snapshot">
                              Snapshot <code>{indexOverviewView.result.overview.snapshotId}</code>
                            </p>
                            <dl class="index-metrics">
                              <div>
                                <dt>Dateien</dt>
                                <dd>
                                  {countLabel(indexOverviewView.result.overview.counts.fileCount)}
                                </dd>
                              </div>
                              <div>
                                <dt>Symbole</dt>
                                <dd>
                                  {countLabel(indexOverviewView.result.overview.counts.symbolCount)}
                                </dd>
                              </div>
                              <div>
                                <dt>Diagnostics</dt>
                                <dd>
                                  {countLabel(
                                    indexOverviewView.result.overview.counts.diagnosticCount,
                                  )}
                                </dd>
                              </div>
                              <div>
                                <dt>Parse Coverage</dt>
                                <dd>
                                  {percentageLabel(
                                    indexOverviewView.result.overview.coverageBasisPoints,
                                  )}
                                </dd>
                              </div>
                            </dl>
                            <p class="index-coverage-note">
                              {countLabel(indexOverviewView.result.overview.counts.parsedFileCount)} von
                              {countLabel(indexOverviewView.result.overview.counts.fileCount)} Dateien
                              strukturell geparst.
                            </p>
                            {#if indexOverviewView.result.overview.diagnosticFiles.length === 0}
                              <p class="ready-label">
                                Keine Parser-Diagnostics im veröffentlichten Snapshot.
                              </p>
                            {:else}
                              <div
                                class="file-diagnostics"
                                aria-labelledby="file-diagnostics-heading"
                              >
                                <h5 id="file-diagnostics-heading">Indexfehler pro Datei</h5>
                                <ul>
                                  {#each indexOverviewView.result.overview.diagnosticFiles as file, fileIndex (fileIndex)}
                                    <li>
                                      <div class="diagnostic-file-heading">
                                        <code
                                          >{file.pathDisplay}{file.pathDisplayTruncated
                                            ? '…'
                                            : ''}</code
                                        >
                                        <span>{indexLanguageLabel(file.language)}</span>
                                      </div>
                                      <p>
                                        {countLabel(file.diagnosticCount)} Diagnostics · Coverage
                                        {percentageLabel(file.coverageBasisPoints)}
                                      </p>
                                      <ul>
                                        {#each file.diagnostics as diagnostic, diagnosticIndex (diagnosticIndex)}
                                          <li>
                                            <strong
                                              >{diagnosticSeverityLabel(
                                                diagnostic.severity,
                                              )}:</strong
                                            >
                                            {diagnosticCodeLabel(diagnostic.code)} · {diagnostic.message}
                                            <span
                                              >Bytes {diagnostic.startByte}–{diagnostic.endByte}</span
                                            >
                                          </li>
                                        {/each}
                                      </ul>
                                      {#if file.diagnosticsTruncated}
                                        <p>
                                          Weitere Diagnostics dieser Datei sind in dieser begrenzten
                                          Ansicht verborgen.
                                        </p>
                                      {/if}
                                    </li>
                                  {/each}
                                </ul>
                                {#if indexOverviewView.result.overview.diagnosticFilesTruncated}
                                  <p>
                                    Weitere fehlerhafte Dateien sind in dieser auf 64 Dateien
                                    begrenzten Ansicht verborgen.
                                  </p>
                                {/if}
                              </div>
                            {/if}
                          {:else if indexOverviewView.kind === 'error'}
                            <div class="recent-projects-error" role="alert">
                              <p>Der veröffentlichte Index konnte nicht sicher gelesen werden.</p>
                              <button type="button" onclick={() => void loadIndexOverview()}
                                >Indexübersicht erneut laden</button
                              >
                            </div>
                          {/if}
                        </div>
                      {:else}
                        <div class="project-dialog-maintenance">
                          <section class="project-maintenance" aria-labelledby="rebuild-heading">
                            <h4 id="rebuild-heading">Index neu aufbauen</h4>
                            <p>
                              Entfernt ausschließlich regenerierbare Indexprojektionen. Quellcode,
                              Snapshots, Aufgaben, Entscheidungen und User-Evidence bleiben
                              erhalten.
                            </p>
                            <p class="project-status" role="status" aria-live="polite">
                              {rebuildStateLabel(projectStatusView.result.rebuildState)}
                            </p>
                            <div class="project-actions">
                              <button
                                type="button"
                                disabled={rebuildView.kind === 'submitting' ||
                                  projectStatusView.result.rebuildState === 'queued' ||
                                  projectStatusView.result.rebuildState === 'running'}
                                onclick={requestIndexRebuild}
                              >
                                {rebuildView.kind === 'submitting'
                                  ? 'Rebuild wird angefordert …'
                                  : 'Regenerierbaren Index neu aufbauen'}
                              </button>
                              <button type="button" onclick={refreshProjectDetails}
                                >Status aktualisieren</button
                              >
                            </div>
                            {#if rebuildView.kind === 'error'}
                              <p class="project-error" role="alert">{rebuildView.message}</p>
                            {/if}
                          </section>
                          <section
                            class="project-maintenance project-removal"
                            aria-labelledby="removal-heading"
                          >
                            <h4 id="removal-heading">Projekt aus A^3 entfernen</h4>
                            <p>
                              Entfernt nur den Eintrag aus A^3. Repository und private Projektdaten
                              bleiben vollständig erhalten.
                            </p>
                            {#if removalView.kind === 'confirming'}
                              <dialog
                                class="removal-confirmation modal-dialog"
                                aria-labelledby="removal-confirmation-heading"
                                aria-describedby="removal-confirmation-copy"
                                use:presentModal
                                oncancel={(event) => {
                                  event.preventDefault();
                                  cancelRemoval();
                                }}
                              >
                                <div class="modal-heading">
                                  <h3 id="removal-confirmation-heading">
                                    Worktree aus A^3 entfernen?
                                  </h3>
                                  <button
                                    type="button"
                                    aria-label="Dialog schließen"
                                    onclick={cancelRemoval}>×</button
                                  >
                                </div>
                                <p id="removal-confirmation-copy">
                                  Nur der Eintrag wird entfernt. Repository, private A^3-Daten und
                                  der lokale Worktree bleiben vollständig bestehen.
                                </p>
                                <div class="modal-actions">
                                  <button type="button" onclick={cancelRemoval}>Abbrechen</button>
                                  <button
                                    class="risk-action"
                                    type="button"
                                    onclick={confirmProjectRemoval}>Entfernen bestätigen</button
                                  >
                                </div>
                              </dialog>
                            {:else}
                              <div class="project-actions">
                                <button
                                  class="risk-action"
                                  type="button"
                                  disabled={removalView.kind === 'submitting'}
                                  onclick={requestRemovalConfirmation}
                                >
                                  {removalView.kind === 'submitting'
                                    ? 'Worktree wird entfernt …'
                                    : 'Nur aus A^3 entfernen'}
                                </button>
                              </div>
                            {/if}
                            {#if removalView.kind === 'error'}
                              <p class="project-error" role="alert">
                                {removalView.message} Repository und private A^3-Daten wurden nicht gelöscht.
                              </p>
                            {/if}
                          </section>
                        </div>
                      {/if}
                    </div>
                  </dialog>
                {/if}
              </div>
              <div class="map-workspace-view">
                <nav class="surface-tabs" aria-label="Project-Map-Arbeitsansicht">
                  <button
                    type="button"
                    aria-pressed={mapWorkspaceView === 'search'}
                    onclick={() => (mapWorkspaceView = 'search')}>Recherche</button
                  >
                  <button
                    type="button"
                    aria-pressed={mapWorkspaceView === 'explore'}
                    onclick={() => (mapWorkspaceView = 'explore')}>Explorer</button
                  >
                  <button
                    type="button"
                    aria-pressed={mapWorkspaceView === 'module'}
                    onclick={() => (mapWorkspaceView = 'module')}>Modul</button
                  >
                  <button
                    type="button"
                    aria-pressed={mapWorkspaceView === 'mapping'}
                    onclick={() => (mapWorkspaceView = 'mapping')}>Mapping</button
                  >
                </nav>
                {#if mapWorkspaceView === 'module'}
                  <nav class="surface-subtabs" aria-label="Modulansicht">
                    <button
                      type="button"
                      aria-pressed={moduleWorkspaceView === 'card'}
                      onclick={() => (moduleWorkspaceView = 'card')}>Card</button
                    >
                    <button
                      type="button"
                      aria-pressed={moduleWorkspaceView === 'runtime'}
                      onclick={() => (moduleWorkspaceView = 'runtime')}>Entry Points</button
                    >
                    <button
                      type="button"
                      aria-pressed={moduleWorkspaceView === 'dependencies'}
                      onclick={() => (moduleWorkspaceView = 'dependencies')}>Abhängigkeiten</button
                    >
                  </nav>
                {/if}
                {#if mapWorkspaceView === 'search'}
                  <section
                    id="map"
                    class="repository-tree-panel project-map-search"
                    aria-labelledby="project-map-search-heading"
                    tabindex="-1"
                  >
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="project-map-search-heading">Project Map</h4>
                        <p>
                          Wechsle zwischen bewusster Repository-Suche und dauerhafter Task Lens.
                        </p>
                      </div>
                    </div>
                    <div
                      class="project-map-mode-switch"
                      role="group"
                      aria-label="Project-Map-Ansicht"
                    >
                      <button
                        type="button"
                        aria-pressed={projectMapMode === 'search'}
                        class:active={projectMapMode === 'search'}
                        onclick={showProjectMapSearch}>Suche</button
                      >
                      <button
                        type="button"
                        aria-pressed={projectMapMode === 'taskLens'}
                        class:active={projectMapMode === 'taskLens'}
                        onclick={showTaskLens}>Task Lens</button
                      >
                    </div>
                    {#if projectMapMode === 'search'}
                      <form
                        class="project-map-search-form"
                        role="search"
                        onsubmit={submitProjectMapSearch}
                      >
                        <label for="project-map-search-query"
                          >Pfad, Symbol oder Signatur suchen</label
                        >
                        <div>
                          <input
                            id="project-map-search-query"
                            name="query"
                            type="search"
                            autocomplete="off"
                            maxlength="4096"
                            bind:value={projectMapSearchText}
                            disabled={projectMapSearchView.kind === 'loading'}
                            aria-describedby="project-map-search-help"
                          />
                          <button
                            type="submit"
                            disabled={projectMapSearchText.trim().length < 3 ||
                              projectMapSearchView.kind === 'loading'}
                          >
                            {projectMapSearchView.kind === 'loading' ? 'Suche läuft …' : 'Suchen'}
                          </button>
                        </div>
                        <p id="project-map-search-help">
                          Mindestens ein durchsuchbarer Begriff mit drei Zeichen. Source wird nicht
                          an die WebView übertragen.
                        </p>
                      </form>
                      {#if projectMapSearchView.kind === 'idle'}
                        <p class="project-status">
                          Die Suche läuft nie automatisch. Gib einen konkreten Identifier, Pfad oder
                          eine Signatur ein.
                        </p>
                      {:else if projectMapSearchView.kind === 'loading'}
                        <p class="project-status" role="status" aria-live="polite">
                          Exact- und Lexical-Projektion werden begrenzt gelesen und fusioniert …
                        </p>
                      {:else if projectMapSearchView.kind === 'noProject'}
                        <p class="project-status">Öffne zuerst einen lokalen Worktree.</p>
                      {:else if projectMapSearchView.kind === 'noPublishedIndex'}
                        <p class="project-status">
                          Noch kein veröffentlichter Index für die Suche verfügbar.
                        </p>
                      {:else if projectMapSearchView.kind === 'projectionUnavailable'}
                        <p class="project-status">
                          Die {projectMapSearchView.channel === 'exact'
                            ? 'Exact-'
                            : 'Lexical-'}Projektion fehlt im historischen Index. Ein Rebuild erzeugt
                          sie mit dem aktuellen Schema.
                        </p>
                      {:else if projectMapSearchView.kind === 'available'}
                        {@const search = projectMapSearchView.result.search}
                        <div class="project-map-search-summary">
                          <p>
                            <strong>{search.hits.length} Treffer</strong> für „{search.query}“ ·
                            Fusion V{search.fusionPolicyVersion}
                          </p>
                          <p>
                            Indexlauf <code>{search.indexRunId}</code> · Snapshot
                            <code>{search.snapshotId}</code>
                          </p>
                        </div>
                        <p class="module-card-safety-note">
                          Exact und Lexical liefern aktuelle Index-Evidence. Semantische Ähnlichkeit
                          ist in dieser faktentragenden Trefferliste nicht zugelassen und wäre
                          niemals ein Beweis.
                        </p>
                        {#if search.truncated}
                          <p class="project-map-search-truncated" role="status">
                            Die begrenzte Ansicht lässt weitere Kandidaten sichtbar aus. Verfeinere
                            die Suche, um andere Treffer zu prüfen.
                          </p>
                        {/if}
                        {#if search.hits.length === 0}
                          <p class="project-status">Keine aktuellen Exact- oder Lexical-Treffer.</p>
                        {:else}
                          <ol
                            class="project-map-search-results"
                            aria-label="Project-Map-Suchergebnisse"
                          >
                            {#each search.hits as hit (hit.rank)}
                              <li>
                                <div class="project-map-search-hit-heading">
                                  <div>
                                    <span>#{hit.rank}</span>
                                    <strong>{projectMapSearchTargetLabel(hit.target)}</strong>
                                  </div>
                                  <span class:project-map-search-exact={hit.priority === 'exact'}>
                                    {hit.priority === 'exact'
                                      ? 'Exact-Priorität'
                                      : 'Evidence-Priorität'}
                                  </span>
                                </div>
                                <p class="project-map-search-target-kind">
                                  {hit.target.kind === 'symbol'
                                    ? projectMapSearchSymbolKindLabel(hit.target.symbolKind)
                                    : 'Dateirevision'}
                                  · Fusionsscore {countLabel(String(hit.finalScore))}
                                </p>
                                <ul
                                  class="project-map-search-sources"
                                  aria-label={`Herkunft von Treffer ${hit.rank}`}
                                >
                                  {#each hit.sources as source (source.channel)}
                                    <li>
                                      <strong>{projectMapSearchSourceLabel(source)}</strong>
                                      <span>{projectMapSearchExplanationLabel(source)}</span>
                                      <span
                                        >{percentageLabel(source.normalizedScoreBasisPoints)}</span
                                      >
                                    </li>
                                  {/each}
                                </ul>
                                <details class="project-map-search-evidence">
                                  <summary>Evidence anzeigen</summary>
                                  <dl>
                                    <div>
                                      <dt>Aktueller Pfad</dt>
                                      <dd><code>{hit.target.evidence.pathDisplay}</code></dd>
                                    </div>
                                    <div>
                                      <dt>Content-Hash</dt>
                                      <dd><code>{hit.target.evidence.contentHash}</code></dd>
                                    </div>
                                    {#if hit.target.kind === 'symbol'}
                                      <div>
                                        <dt>Symbol-ID</dt>
                                        <dd><code>{hit.target.symbolId}</code></dd>
                                      </div>
                                      {#if hit.target.signature !== null}
                                        <div>
                                          <dt>Signatur</dt>
                                          <dd><code>{hit.target.signature}</code></dd>
                                        </div>
                                      {/if}
                                    {/if}
                                    {#if hit.target.evidence.declarationRange !== null}
                                      <div>
                                        <dt>Deklaration</dt>
                                        <dd>
                                          Bytes {hit.target.evidence.declarationRange
                                            .startByte}–{hit.target.evidence.declarationRange
                                            .endByte}
                                        </dd>
                                      </div>
                                    {/if}
                                  </dl>
                                </details>
                              </li>
                            {/each}
                          </ol>
                        {/if}
                      {:else if projectMapSearchView.kind === 'error'}
                        <div class="recent-projects-error" role="alert">
                          <p>Die Project-Map-Suche konnte nicht sicher ausgewertet werden.</p>
                          <button type="button" onclick={runProjectMapSearch}>
                            Suche erneut ausführen
                          </button>
                        </div>
                      {/if}
                    {:else}
                      <div class="task-lens-selector" aria-label="Dauerhafte Task-Lens-Anker">
                        <p class="module-card-safety-note">
                          Goal Contract und aktiver Plan-Schritt kommen ausschließlich aus dem
                          aktuellen Task Ledger. Die WebView kann weder Seeds noch Projektpfade
                          erfinden.
                        </p>
                        {#if taskLensTasksView.kind === 'idle' || taskLensTasksView.kind === 'loading'}
                          <p class="project-status" role="status" aria-live="polite">
                            Dauerhafte Tasks werden begrenzt gelesen …
                          </p>
                        {:else if taskLensTasksView.kind === 'noProject'}
                          <p class="project-status">Öffne zuerst einen lokalen Worktree.</p>
                        {:else if taskLensTasksView.kind === 'error'}
                          <div class="recent-projects-error" role="alert">
                            <p>Die dauerhaften Task-Anker konnten nicht sicher gelesen werden.</p>
                            <button type="button" onclick={loadTaskLensTasks}
                              >Tasks erneut laden</button
                            >
                          </div>
                        {:else}
                          <label for="task-lens-task">Goal Contract</label>
                          <select
                            id="task-lens-task"
                            value={selectedTaskLensTaskId}
                            onchange={selectTaskLensTask}
                            disabled={taskLensTaskView.kind === 'loading' ||
                              taskLensView.kind === 'loading'}
                          >
                            <option value="">Task auswählen …</option>
                            {#each taskLensTasksView.result.tasks as task (task.taskId)}
                              <option value={task.taskId}
                                >R{task.goalRevision} · {task.objective}</option
                              >
                            {/each}
                          </select>
                          {#if taskLensTasksView.result.truncated}
                            <p class="project-map-search-truncated" role="status">
                              Die Auswahl zeigt höchstens 20 Tasks; weitere aktuelle Goal Contracts
                              bleiben sichtbar ausgelassen.
                            </p>
                          {/if}
                          {#if taskLensTasksView.result.tasks.length === 0}
                            <p class="project-status">
                              Noch kein dauerhafter Goal Contract. Task-Erstellung folgt im Agent
                              Workspace.
                            </p>
                          {/if}

                          {#if taskLensTaskView.kind === 'loading'}
                            <p class="project-status" role="status">
                              Aktueller Task Ledger wird gelesen …
                            </p>
                          {:else if taskLensTaskView.kind === 'ledgerUnavailable'}
                            <p class="project-status">
                              Für diesen Goal Contract existiert noch kein materialisierter Task
                              Ledger.
                            </p>
                          {:else if taskLensTaskView.kind === 'goalRevisionMismatch'}
                            <p class="project-map-search-truncated" role="status">
                              Goal R{taskLensTaskView.currentGoalRevision} ist aktueller als Ledger-Goal
                              R{taskLensTaskView.ledgerGoalRevision}. Die Lens bleibt gesperrt, bis
                              der Plan neu erstellt wurde.
                            </p>
                          {:else if taskLensTaskView.kind === 'taskNotFound'}
                            <p class="project-status">
                              Der ausgewählte Task ist nicht mehr aktuell.
                            </p>
                          {:else if taskLensTaskView.kind === 'noProject'}
                            <p class="project-status">Der aktive Worktree wurde geschlossen.</p>
                          {:else if taskLensTaskView.kind === 'error'}
                            <p class="project-status" role="alert">
                              Der aktuelle Task Ledger konnte nicht sicher gelesen werden.
                            </p>
                          {:else if taskLensTaskView.kind === 'available'}
                            <label for="task-lens-step">Aktueller Fokus-Schritt</label>
                            <select
                              id="task-lens-step"
                              value={selectedTaskLensStepId}
                              onchange={selectTaskLensStep}
                              disabled={taskLensView.kind === 'loading'}
                            >
                              <option value="">Plan-Schritt auswählen …</option>
                              {#each taskLensTaskView.result.steps as step (step.stepId)}
                                <option value={step.stepId}>
                                  {taskLensStepStatusLabel(step.status)} · {step.intendedOutcome}
                                </option>
                              {/each}
                            </select>
                            <p>
                              Ledger R{taskLensTaskView.result.ledgerRevision} · Store-Version
                              {taskLensTaskView.result.ledgerStoreVersion}
                            </p>
                            <button
                              type="button"
                              onclick={runTaskLensCompile}
                              disabled={selectedTaskLensStepId === '' ||
                                taskLensView.kind === 'loading'}
                            >
                              {taskLensView.kind === 'loading'
                                ? 'Task Lens wird kompiliert …'
                                : 'Task Lens aktualisieren'}
                            </button>
                          {/if}
                        {/if}
                      </div>

                      {#if taskLensView.kind === 'idle'}
                        <p class="project-status">
                          Wähle Task und aktiven Plan-Schritt; die Lens läuft nie als offener
                          Chat-Loop.
                        </p>
                      {:else if taskLensView.kind === 'loading'}
                        <p class="project-status" role="status" aria-live="polite">
                          Exact → Lexical → Graph/Test → aktuelle Claims werden begrenzt kompiliert
                          …
                        </p>
                      {:else if taskLensView.kind === 'noPublishedIndex'}
                        <p class="project-status">
                          Noch kein veröffentlichter Index für die Task Lens.
                        </p>
                      {:else if taskLensView.kind === 'stepUnavailable'}
                        <p class="project-map-search-truncated" role="status">
                          Der ausgewählte Schritt wurde inzwischen entfernt oder retirert. Lade den
                          Task erneut.
                        </p>
                      {:else if taskLensView.kind === 'goalRevisionMismatch'}
                        <p class="project-map-search-truncated" role="status">
                          Goal R{taskLensView.currentGoalRevision} und Ledger-Goal R{taskLensView.ledgerGoalRevision}
                          stimmen nicht mehr überein.
                        </p>
                      {:else if taskLensView.kind === 'ledgerUnavailable' || taskLensView.kind === 'taskNotFound'}
                        <p class="project-status">
                          Der dauerhafte Task-Anker ist nicht mehr verfügbar.
                        </p>
                      {:else if taskLensView.kind === 'noProject'}
                        <p class="project-status">Der aktive Worktree wurde geschlossen.</p>
                      {:else if taskLensView.kind === 'error'}
                        <div class="recent-projects-error" role="alert">
                          <p>
                            Die Task Lens konnte nicht sicher aus aktueller Evidence kompiliert
                            werden.
                          </p>
                          <button type="button" onclick={runTaskLensCompile}
                            >Erneut kompilieren</button
                          >
                        </div>
                      {:else if taskLensView.kind === 'available'}
                        {@const lens = taskLensView.result.lens}
                        <div class="project-map-search-summary task-lens-summary">
                          <p>
                            <strong
                              >{lens.entries.length} Einträge · {lens.claims.length} Claims</strong
                            >
                            · {countLabel(String(lens.estimatedTokens))}/{countLabel(
                              String(lens.tokenBudget),
                            )}
                            Tokens
                          </p>
                          <p>
                            Goal R{lens.goalRevision} · Ledger R{lens.ledgerRevision} · Task-Lens V{lens.policyVersion}
                            · Fusion V{lens.fusionPolicyVersion}
                          </p>
                          <p>
                            Indexlauf <code>{lens.indexRunId}</code> · Snapshot
                            <code>{lens.snapshotId}</code>
                          </p>
                        </div>
                        <p class="module-card-safety-note">
                          Semantische Ähnlichkeit erzeugt ausschließlich Kandidaten. Facts benötigen
                          weiterhin aktuelle, aufgelöste Evidence; {lens.excludedStaleClaims} stale Claims
                          wurden vollständig ausgeschlossen.
                        </p>
                        {#if lens.truncated}
                          <p class="project-map-search-truncated" role="status">
                            Mindestens eine feste Kandidaten-, Manifest-, Claim- oder Token-Grenze
                            hat weitere Details sichtbar ausgelassen.
                          </p>
                        {/if}
                        <ol
                          class="project-map-search-results task-lens-entries"
                          aria-label="Task-Lens-Einträge"
                        >
                          {#each lens.entries as entry (entry.position)}
                            <li>
                              <div class="project-map-search-hit-heading">
                                <div>
                                  <span>#{entry.position}</span>
                                  <strong>{taskLensTargetLabel(entry.target)}</strong>
                                </div>
                                <span>{taskLensTargetLevel(entry.target)}</span>
                              </div>
                              <p class="project-map-search-target-kind">
                                {entry.estimatedTokens} geschätzte Tokens
                                {#if entry.reason.kind === 'repositoryAnchor'}
                                  · deterministischer Pflichtanker
                                {:else if entry.reason.kind === 'claim'}
                                  · Claim <code>{entry.reason.claimId}</code>
                                {:else}
                                  · Fusionsrang {entry.reason.rank} · Score {countLabel(
                                    String(entry.reason.finalScore),
                                  )}
                                {/if}
                              </p>
                              {#if entry.reason.kind === 'retrieval'}
                                <ul
                                  class="project-map-search-sources"
                                  aria-label={`Herkunft von Lens-Eintrag ${entry.position}`}
                                >
                                  {#each entry.reason.sources as source (source.channel)}
                                    <li
                                      class:task-lens-semantic-source={source.channel ===
                                        'semantic'}
                                    >
                                      <strong>{taskLensChannelLabel(source.channel)}</strong>
                                      <span
                                        >{percentageLabel(source.normalizedScoreBasisPoints)}</span
                                      >
                                      {#if source.channel === 'semantic'}<span>kein Beweis</span
                                        >{/if}
                                    </li>
                                  {/each}
                                </ul>
                              {/if}
                              <details class="project-map-search-evidence">
                                <summary>Evidence anzeigen</summary>
                                <dl>
                                  {#if entry.target.kind === 'repository'}
                                    <div>
                                      <dt>Publikation</dt>
                                      <dd><code>{lens.indexRunId}</code></dd>
                                    </div>
                                    <div>
                                      <dt>Snapshot</dt>
                                      <dd><code>{lens.snapshotId}</code></dd>
                                    </div>
                                    <div>
                                      <dt>Struktur</dt>
                                      <dd>
                                        {entry.target.fileCount} Dateien · {entry.target
                                          .symbolCount} Symbole
                                      </dd>
                                    </div>
                                  {:else if entry.target.kind === 'module'}
                                    <div>
                                      <dt>Modul-ID</dt>
                                      <dd><code>{entry.target.moduleId}</code></dd>
                                    </div>
                                    {#if entry.target.root !== null}
                                      <div>
                                        <dt>Grenze</dt>
                                        <dd><code>{entry.target.root.pathDisplay}</code></dd>
                                      </div>
                                    {/if}
                                    {#each entry.target.manifests as manifest (manifest.pathHex)}
                                      <div>
                                        <dt>Manifest</dt>
                                        <dd>
                                          <code>{manifest.pathDisplay}</code> ·
                                          <code>{manifest.contentHash}</code>
                                        </dd>
                                      </div>
                                    {/each}
                                    {#if entry.target.manifests.length === 0}
                                      <div>
                                        <dt>Projektion</dt>
                                        <dd>
                                          Strukturell an Indexlauf und Snapshot gebunden; keine
                                          eigenständige Source-Behauptung.
                                        </dd>
                                      </div>
                                    {/if}
                                  {:else}
                                    <div>
                                      <dt>Pfad</dt>
                                      <dd><code>{entry.target.evidence.pathDisplay}</code></dd>
                                    </div>
                                    <div>
                                      <dt>Content-Hash</dt>
                                      <dd><code>{entry.target.evidence.contentHash}</code></dd>
                                    </div>
                                    {#if entry.target.kind === 'symbol' || entry.target.kind === 'sourceSpan'}
                                      <div>
                                        <dt>Symbol-ID</dt>
                                        <dd><code>{entry.target.symbolId}</code></dd>
                                      </div>
                                    {/if}
                                    {#if entry.target.evidence.declarationRange !== null}
                                      <div>
                                        <dt>Deklaration</dt>
                                        <dd>
                                          Bytes {entry.target.evidence.declarationRange
                                            .startByte}–{entry.target.evidence.declarationRange
                                            .endByte}
                                        </dd>
                                      </div>
                                    {/if}
                                  {/if}
                                </dl>
                              </details>
                            </li>
                          {/each}
                        </ol>

                        <div class="task-lens-claims" aria-label="Aktuelle Task-Lens-Claims">
                          <h5>Evidence-gebundene Claims</h5>
                          {#if lens.claims.length === 0}
                            <p class="project-status">
                              Keine aktuellen Claims für die ausgewählten Ziele.
                            </p>
                          {:else}
                            <ul>
                              {#each lens.claims as claim (claim.claimId)}
                                <li class:task-lens-hypothesis={claim.kind === 'hypothesis'}>
                                  <div class="project-map-search-hit-heading">
                                    <strong>{taskLensClaimKindLabel(claim.kind)}</strong>
                                    <span>{percentageLabel(claim.confidenceBasisPoints)}</span>
                                  </div>
                                  <p>{taskLensPredicateLabel(claim.predicate)}</p>
                                  <details class="project-map-search-evidence">
                                    <summary>Evidence / Beweisstatus</summary>
                                    {#if claim.evidence.length === 0}
                                      <p>
                                        Keine beweisende Evidence vorhanden. Diese
                                        Architekturabsicht bleibt ausdrücklich eine Hypothese.
                                      </p>
                                    {:else}
                                      <dl>
                                        {#each claim.evidence as evidence (evidence.kind === 'graphEdge' ? evidence.edge.evidenceId : evidence.evidenceId)}
                                          {#if evidence.kind === 'graphEdge'}
                                            <div>
                                              <dt>Graph-Edge</dt>
                                              <dd>
                                                {evidence.relation} ·
                                                <code>{evidence.edge.evidenceId}</code>
                                              </dd>
                                            </div>
                                            <div>
                                              <dt>Source-Pfad (hex)</dt>
                                              <dd><code>{evidence.edge.pathHex}</code></dd>
                                            </div>
                                            <div>
                                              <dt>Range</dt>
                                              <dd>
                                                Bytes {evidence.edge.range.startByte}–{evidence.edge
                                                  .range.endByte}
                                              </dd>
                                            </div>
                                          {:else}
                                            <div>
                                              <dt>Evidence-ID</dt>
                                              <dd><code>{evidence.evidenceId}</code></dd>
                                            </div>
                                            <div>
                                              <dt>Pfad</dt>
                                              <dd><code>{evidence.revision.pathDisplay}</code></dd>
                                            </div>
                                            <div>
                                              <dt>Content-Hash</dt>
                                              <dd><code>{evidence.revision.contentHash}</code></dd>
                                            </div>
                                          {/if}
                                        {/each}
                                      </dl>
                                    {/if}
                                  </details>
                                </li>
                              {/each}
                            </ul>
                          {/if}
                        </div>
                      {/if}
                    {/if}
                  </section>
                {:else if mapWorkspaceView === 'explore'}
                  <div class="repository-tree-panel" aria-labelledby="repository-tree-heading">
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="repository-tree-heading">Repository-Baum</h4>
                        <p>
                          Direkte Kinder des veröffentlichten Index, progressiv und ohne
                          Vollbaum-Ladung.
                        </p>
                      </div>
                      <button type="button" onclick={loadRepositoryTreeRoot}>Zum Root</button>
                    </div>
                    {#if repositoryTreeView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Repository-Baum wird gelesen …
                      </p>
                    {:else if repositoryTreeView.kind === 'noPublishedIndex'}
                      <p class="project-status">
                        Noch kein vollständiger Snapshot veröffentlicht; der Repository-Baum bleibt
                        leer.
                      </p>
                    {:else if repositoryTreeView.kind === 'available'}
                      <p class="index-snapshot">
                        Indexlauf <code>{repositoryTreeView.result.page.indexRunId}</code>
                      </p>
                      <nav class="repository-tree-breadcrumbs" aria-label="Repository-Pfad">
                        <button type="button" onclick={() => openRepositoryBreadcrumb(-1)}
                          >Repository</button
                        >
                        {#each repositoryTreeBreadcrumbs as breadcrumb, breadcrumbIndex (breadcrumb.pathHex)}
                          <span aria-hidden="true">/</span>
                          <button
                            type="button"
                            aria-current={breadcrumbIndex === repositoryTreeBreadcrumbs.length - 1
                              ? 'page'
                              : undefined}
                            onclick={() => openRepositoryBreadcrumb(breadcrumbIndex)}
                          >
                            {breadcrumb.name}
                          </button>
                        {/each}
                      </nav>
                      {#if repositoryTreeView.result.page.entries.length === 0}
                        <p class="ready-label">
                          Keine weiteren indexierten Einträge in diesem Bereich.
                        </p>
                      {:else}
                        <ul class="repository-tree-entries">
                          {#each repositoryTreeView.result.page.entries as entry (entry.pathHex)}
                            <li>
                              {#if entry.kind === 'directory'}
                                <button
                                  class="repository-directory"
                                  type="button"
                                  aria-label={`Verzeichnis ${entry.name} öffnen`}
                                  onclick={() => openRepositoryDirectory(entry)}
                                >
                                  <span aria-hidden="true">▸</span>
                                  <code>{entry.name}{entry.nameTruncated ? '…' : ''}</code>
                                </button>
                                <span>{countLabel(entry.descendantFileCount)} Dateien</span>
                              {:else}
                                <div class="repository-file">
                                  <span aria-hidden="true">·</span>
                                  <code>{entry.name}{entry.nameTruncated ? '…' : ''}</code>
                                </div>
                                <span>Revision {entry.contentHash?.slice(0, 12)}</span>
                              {/if}
                            </li>
                          {/each}
                        </ul>
                      {/if}
                      {#if repositoryTreePageIndex > 0 || repositoryTreeView.result.page.nextAfterNameHex !== null}
                        <nav class="repository-tree-pagination" aria-label="Repository-Baum-Seiten">
                          <button
                            type="button"
                            disabled={repositoryTreeLoadingMore || repositoryTreePageIndex === 0}
                            onclick={loadPreviousRepositoryPage}
                          >
                            Vorherige Seite
                          </button>
                          <span
                            >Seite {repositoryTreePageIndex + 1} · höchstens 50 Einträge im DOM</span
                          >
                          <button
                            type="button"
                            disabled={repositoryTreeLoadingMore ||
                              repositoryTreeView.result.page.nextAfterNameHex === null}
                            onclick={loadNextRepositoryPage}
                          >
                            {repositoryTreeLoadingMore ? 'Seite wird geladen …' : 'Nächste Seite'}
                          </button>
                        </nav>
                      {/if}
                    {:else if repositoryTreeView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Der Repository-Baum konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={loadRepositoryTreeRoot}
                          >Vom Root neu laden</button
                        >
                      </div>
                    {/if}
                  </div>
                  <div
                    class="repository-tree-panel module-tree-panel"
                    aria-labelledby="module-tree-heading"
                  >
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="module-tree-heading">Modulbaum</h4>
                        <p>
                          Direkte deterministische Modulgrenzen; Graph-Communities bleiben
                          Zusatzsignale.
                        </p>
                      </div>
                      <button type="button" onclick={loadModuleTreeRoot}>Zum Root</button>
                    </div>
                    {#if moduleTreeView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Modulbaum wird gelesen …
                      </p>
                    {:else if moduleTreeView.kind === 'noPublishedIndex'}
                      <p class="project-status">
                        Noch kein vollständiger Snapshot veröffentlicht; der Modulbaum bleibt leer.
                      </p>
                    {:else if moduleTreeView.kind === 'projectionUnavailable'}
                      <p class="project-status">
                        Der veröffentlichte historische Index enthält noch keine deterministische
                        Modulprojektion. Ein Rebuild erzeugt sie mit dem aktuellen Schema.
                      </p>
                    {:else if moduleTreeView.kind === 'available'}
                      <p class="index-snapshot">
                        Indexlauf <code>{moduleTreeView.result.page.indexRunId}</code>
                      </p>
                      <dl class="module-tree-summary">
                        <div>
                          <dt>Primäre Module</dt>
                          <dd>{countLabel(moduleTreeView.result.page.primaryModuleCount)}</dd>
                        </div>
                        <div>
                          <dt>Graph-Communities</dt>
                          <dd>{countLabel(moduleTreeView.result.page.graphCommunityCount)}</dd>
                        </div>
                      </dl>
                      <nav class="repository-tree-breadcrumbs" aria-label="Modulpfad">
                        <button type="button" onclick={() => openModuleBreadcrumb(-1)}
                          >Modul-Root</button
                        >
                        {#each moduleTreeBreadcrumbs as breadcrumb, breadcrumbIndex (breadcrumb.moduleId)}
                          <span aria-hidden="true">/</span>
                          <button
                            type="button"
                            aria-current={breadcrumbIndex === moduleTreeBreadcrumbs.length - 1
                              ? 'page'
                              : undefined}
                            onclick={() => openModuleBreadcrumb(breadcrumbIndex)}
                          >
                            {breadcrumb.name}
                          </button>
                        {/each}
                      </nav>
                      {#if moduleTreeView.result.page.entries.length === 0}
                        <p class="ready-label">Keine direkten primären Module in diesem Bereich.</p>
                      {:else}
                        <ul class="module-tree-entries">
                          {#each moduleTreeView.result.page.entries as entry (entry.moduleId)}
                            <li>
                              <div class="module-tree-entry-heading">
                                {#if entry.childState === 'hasChildren'}
                                  <button
                                    type="button"
                                    aria-label={`Modul ${entry.name} öffnen`}
                                    onclick={() => openModule(entry)}
                                  >
                                    <span aria-hidden="true">▸</span>
                                    <strong>{entry.name}{entry.nameTruncated ? '…' : ''}</strong>
                                  </button>
                                {:else}
                                  <div>
                                    <span aria-hidden="true">·</span>
                                    <strong>{entry.name}{entry.nameTruncated ? '…' : ''}</strong>
                                  </div>
                                {/if}
                                <span>{moduleKindLabel(entry)}</span>
                              </div>
                              <dl class="module-tree-entry-metrics">
                                <div>
                                  <dt>Manifeste</dt>
                                  <dd>{countLabel(entry.manifestCount)}</dd>
                                </div>
                                <div>
                                  <dt>Dateien</dt>
                                  <dd>{countLabel(entry.fileCount)}</dd>
                                </div>
                                <div>
                                  <dt>Symbole</dt>
                                  <dd>{countLabel(entry.symbolCount)}</dd>
                                </div>
                                <div>
                                  <dt>Zentral</dt>
                                  <dd>{moduleFeatureLabel(entry.centralSymbols)}</dd>
                                </div>
                                <div>
                                  <dt>Einstiege</dt>
                                  <dd>{moduleFeatureLabel(entry.entrypoints)}</dd>
                                </div>
                                <div>
                                  <dt>Tests</dt>
                                  <dd>{moduleFeatureLabel(entry.tests)}</dd>
                                </div>
                              </dl>
                              <p class="module-tree-evidence">
                                {#if entry.boundaryEvidence.manifestRevision !== null}
                                  Manifest-Evidenz
                                  <code
                                    >{entry.boundaryEvidence.manifestRevision.contentHash.slice(
                                      0,
                                      12,
                                    )}</code
                                  >
                                {:else if entry.boundaryEvidence.representativeRevision !== null}
                                  Repräsentative Revision
                                  <code
                                    >{entry.boundaryEvidence.representativeRevision.contentHash.slice(
                                      0,
                                      12,
                                    )}</code
                                  >
                                {:else}
                                  Leeres strukturelles Modul ohne Revisionsrepräsentant
                                {/if}
                              </p>
                              <div class="module-entry-actions">
                                <button
                                  class="module-card-open"
                                  type="button"
                                  aria-pressed={moduleCardSelection?.moduleId === entry.moduleId}
                                  onclick={() => openModuleCard(entry)}
                                >
                                  Module Card
                                </button>
                                <button
                                  class="module-dependency-open"
                                  type="button"
                                  aria-pressed={moduleRuntimeSelection?.moduleId === entry.moduleId}
                                  onclick={() => openModuleRuntime(entry)}
                                >
                                  Entry Points &amp; Tests
                                </button>
                                <button
                                  class="module-dependency-open"
                                  type="button"
                                  aria-pressed={moduleDependencySelection?.moduleId ===
                                    entry.moduleId}
                                  onclick={() => openModuleDependencies(entry)}
                                >
                                  Abhängigkeiten anzeigen
                                </button>
                              </div>
                            </li>
                          {/each}
                        </ul>
                      {/if}
                      {#if moduleTreePageIndex > 0 || moduleTreeView.result.page.nextAfterModuleId !== null}
                        <nav class="repository-tree-pagination" aria-label="Modulbaum-Seiten">
                          <button
                            type="button"
                            disabled={moduleTreeLoadingMore || moduleTreePageIndex === 0}
                            onclick={loadPreviousModulePage}
                          >
                            Vorherige Seite
                          </button>
                          <span>Seite {moduleTreePageIndex + 1} · höchstens 50 Module im DOM</span>
                          <button
                            type="button"
                            disabled={moduleTreeLoadingMore ||
                              moduleTreeView.result.page.nextAfterModuleId === null}
                            onclick={loadNextModulePage}
                          >
                            {moduleTreeLoadingMore ? 'Seite wird geladen …' : 'Nächste Seite'}
                          </button>
                        </nav>
                      {/if}
                    {:else if moduleTreeView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Der Modulbaum konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={loadModuleTreeRoot}
                          >Vom Root neu laden</button
                        >
                      </div>
                    {/if}
                  </div>
                {:else if mapWorkspaceView === 'module' && moduleWorkspaceView === 'card'}
                  <div
                    class="repository-tree-panel module-card-panel"
                    aria-labelledby="module-card-heading"
                  >
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="module-card-heading">Module Card</h4>
                        <p>
                          Verifizierte Felder mit getrennt sichtbarer Klassifikation und Aktualität.
                        </p>
                      </div>
                      <button
                        type="button"
                        disabled={moduleCardSelection === null ||
                          moduleCardDetailView.kind === 'loading'}
                        onclick={reloadModuleCardDetail}
                      >
                        Aktualisieren
                      </button>
                    </div>
                    {#if moduleCardDetailView.kind === 'idle' || moduleCardDetailView.kind === 'noProject'}
                      <p class="project-status">
                        Wähle im Modulbaum „Module Card“, um die neueste dauerhaft verifizierte
                        Karte bewusst zu laden.
                      </p>
                    {:else if moduleCardDetailView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Module Card für {moduleCardSelection?.name ?? 'das Modul'} wird atomar gelesen
                        …
                      </p>
                    {:else if moduleCardDetailView.kind === 'noPublishedIndex'}
                      <p class="project-status">Noch kein veröffentlichter Index.</p>
                    {:else if moduleCardDetailView.kind === 'projectionUnavailable'}
                      <p class="project-status">
                        Der historische Index enthält noch keine deterministische Modulprojektion.
                        Ein Rebuild erzeugt sie mit dem aktuellen Schema.
                      </p>
                    {:else if moduleCardDetailView.kind === 'moduleUnavailable'}
                      <div class="recent-projects-error" role="status">
                        <p>
                          Das ausgewählte primäre Modul gehört nicht mehr zur aktuellen Publikation.
                        </p>
                        <button type="button" onclick={loadModuleTreeRoot}
                          >Modulbaum neu laden</button
                        >
                      </div>
                    {:else if moduleCardDetailView.kind === 'cardUnavailable'}
                      <p class="project-status">
                        Für {moduleCardSelection?.name ?? 'dieses Modul'} wurde noch keine verifizierte
                        Module Card veröffentlicht.
                      </p>
                    {:else if moduleCardDetailView.kind === 'available'}
                      {@const card = moduleCardDetailView.result.detail}
                      <section
                        class="module-card-signals"
                        aria-labelledby="module-card-signals-heading"
                      >
                        <div class="module-card-signals-heading">
                          <h5 id="module-card-signals-heading">
                            Confidence, Coverage und Freshness
                          </h5>
                          <p>
                            Drei unabhängige Signale der ausgewählten, publikationsgebundenen Card.
                          </p>
                        </div>
                        <div class="module-card-signal-grid">
                          <article class="module-card-signal module-card-confidence">
                            <h6>Confidence</h6>
                            <strong>{percentageLabel(card.confidenceBasisPoints)}</strong>
                            <p>
                              Numerische Einschätzung der verifizierten Card, kein Faktenstatus.
                            </p>
                          </article>
                          <article class="module-card-signal module-card-coverage">
                            <h6>Coverage</h6>
                            <strong>
                              {card.coverage.coveredFieldCount} von {card.coverage.totalFieldCount} Feldern
                              ·
                              {percentageLabel(card.coverage.basisPoints)}
                            </strong>
                            <p>
                              {card.coverage.must.coveredFieldCount} von {card.coverage.must
                                .totalFieldCount}
                              Muss-Feldern · {card.coverage.should.coveredFieldCount} von
                              {card.coverage.should.totalFieldCount} Soll-Feldern
                            </p>
                            <details class="module-card-coverage-gaps">
                              <summary>Feldabdeckung im Detail</summary>
                              <div>
                                <section aria-labelledby="module-card-missing-must">
                                  <h6 id="module-card-missing-must">Fehlende Muss-Felder</h6>
                                  {#if card.coverage.must.missingFields.length === 0}
                                    <p>Alle Muss-Felder sind verifiziert abgedeckt.</p>
                                  {:else}
                                    <ul>
                                      {#each card.coverage.must.missingFields as field (field)}
                                        <li>{moduleCardFieldLabel(field)}</li>
                                      {/each}
                                    </ul>
                                  {/if}
                                </section>
                                <section aria-labelledby="module-card-missing-should">
                                  <h6 id="module-card-missing-should">Fehlende Soll-Felder</h6>
                                  {#if card.coverage.should.missingFields.length === 0}
                                    <p>Alle Soll-Felder sind verifiziert abgedeckt.</p>
                                  {:else}
                                    <ul>
                                      {#each card.coverage.should.missingFields as field (field)}
                                        <li>{moduleCardFieldLabel(field)}</li>
                                      {/each}
                                    </ul>
                                  {/if}
                                </section>
                              </div>
                            </details>
                          </article>
                          <article
                            class:module-card-lifecycle-current={card.lifecycle.status ===
                              'current'}
                            class:module-card-lifecycle-stale={card.lifecycle.status === 'stale'}
                            class:module-card-lifecycle-review={card.lifecycle.status ===
                              'needsReview'}
                            class="module-card-signal module-card-lifecycle"
                            role={card.lifecycle.status === 'current' ? 'note' : 'alert'}
                          >
                            <h6>Freshness</h6>
                            <strong>
                              {card.lifecycle.status === 'current'
                                ? 'Current'
                                : card.lifecycle.status === 'stale'
                                  ? 'Stale — keine aktuelle Faktenquelle'
                                  : 'NeedsReview — keine aktuelle Faktenquelle'}
                            </strong>
                            <p>
                              {card.lifecycle.status === 'current'
                                ? 'Card und sichtbare Claims lösen gegen die aktuelle Publikation auf.'
                                : moduleCardFreshnessReasonLabel(card.lifecycle.reason)}
                            </p>
                          </article>
                        </div>
                      </section>
                      <p class="module-card-safety-note" role="note">
                        Confidence, Coverage, Claim-Typ und Aktualität werden nicht miteinander
                        verrechnet. Ein als „Fact“ klassifizierter, aber „Stale“ oder „NeedsReview“
                        markierter Wert wird nicht als aktuelles Faktum verwendet.
                      </p>
                      <dl class="module-card-envelope">
                        <div>
                          <dt>Ausgewähltes Modul</dt>
                          <dd>{moduleCardSelection?.name ?? card.moduleId.slice(0, 12)}</dd>
                        </div>
                        <div>
                          <dt>Schema und Mapper</dt>
                          <dd>V{card.schemaVersion} · Mapper V{card.mapperProfileVersion}</dd>
                        </div>
                        <div>
                          <dt>Aktueller Indexlauf</dt>
                          <dd><code>{card.currentIndexRunId}</code></dd>
                        </div>
                        <div>
                          <dt>Verifiziert in</dt>
                          <dd><code>{card.sourceIndexRunId}</code></dd>
                        </div>
                      </dl>
                      <div class="module-card-fields">
                        {#each card.fields as field (field.kind)}
                          <section
                            class="module-card-field"
                            aria-labelledby={`module-card-${field.kind}`}
                          >
                            <div class="module-card-field-heading">
                              <h5 id={`module-card-${field.kind}`}>
                                {moduleCardFieldLabel(field.kind)}
                              </h5>
                              <span>{field.evidenceIds.length} Feld-Evidence</span>
                            </div>
                            <ol>
                              {#each field.values as item (item.claim.claimId)}
                                <li
                                  class:module-card-value-current={item.claim.state === 'current'}
                                  class:module-card-value-stale={item.claim.state === 'stale'}
                                  class:module-card-value-review={item.claim.state ===
                                    'needsReview'}
                                >
                                  <div class="module-card-claim-badges">
                                    <span
                                      class={`module-card-claim-kind module-card-claim-${item.claim.kind}`}
                                    >
                                      {moduleCardClaimKindLabel(item.claim.kind)}
                                    </span>
                                    <span
                                      class={`module-card-claim-state module-card-claim-${item.claim.state}`}
                                    >
                                      {item.claim.state === 'current'
                                        ? 'Current'
                                        : item.claim.state === 'stale'
                                          ? 'Stale'
                                          : 'NeedsReview'}
                                    </span>
                                    <span>{percentageLabel(item.claim.confidenceBasisPoints)}</span>
                                  </div>
                                  <p>{item.value}</p>
                                  <details class="module-card-evidence-identities">
                                    <summary
                                      >{item.claim.evidenceIds.length} Claim-Evidence-ID(s)</summary
                                    >
                                    {#if item.claim.evidenceIds.length === 0}
                                      <p>Architecture-Hypothese ohne deterministische Evidence.</p>
                                    {:else}
                                      <ul>
                                        {#each item.claim.evidenceIds as evidenceId (evidenceId)}
                                          <li>
                                            <button
                                              type="button"
                                              aria-label={`Evidence ${evidenceId} für „${item.value}“ untersuchen`}
                                              aria-pressed={selectedModuleCardEvidenceId ===
                                                evidenceId}
                                              onclick={() => inspectModuleCardEvidence(evidenceId)}
                                            >
                                              <code>{evidenceId}</code>
                                              <span>Untersuchen</span>
                                            </button>
                                          </li>
                                        {/each}
                                      </ul>
                                    {/if}
                                  </details>
                                </li>
                              {/each}
                            </ol>
                          </section>
                        {/each}
                      </div>
                      <aside
                        class="dependency-evidence module-card-evidence-inspector"
                        aria-labelledby="module-card-evidence-heading"
                      >
                        <div>
                          <div>
                            <h5 id="module-card-evidence-heading">Evidence Inspector</h5>
                            <p>Card-gebundene, typisierte Provenienz ohne Quelltextzugriff.</p>
                          </div>
                          {#if moduleCardEvidenceView.kind !== 'idle'}
                            <button type="button" onclick={resetModuleCardEvidence}
                              >Schließen</button
                            >
                          {/if}
                        </div>
                        {#if moduleCardEvidenceView.kind === 'idle'}
                          <p class="project-status">
                            Öffne eine Claim-Evidence-ID, um ihre exakte Revision oder
                            Graph-Beziehung zu prüfen.
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'loading'}
                          <p class="project-status" role="status" aria-live="polite">
                            Evidence {moduleCardEvidenceView.evidenceId.slice(0, 12)} wird gegen die sichtbare
                            Card geprüft …
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'selectionChanged'}
                          <div class="recent-projects-error" role="alert">
                            <p>
                              Publikation oder neueste Card haben sich geändert. Die alte Auswahl
                              wird nicht gegen einen anderen Stand aufgelöst.
                            </p>
                            <button type="button" onclick={reloadModuleCardDetail}
                              >Module Card neu laden</button
                            >
                          </div>
                        {:else if moduleCardEvidenceView.kind === 'evidenceUnavailable'}
                          <p class="project-status" role="status">
                            Diese ID gehört nicht zur aktuell ausgewählten neuesten Card und wird
                            deshalb nicht geöffnet.
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'cardUnavailable'}
                          <p class="project-status" role="status">
                            Die ausgewählte Card ist nicht mehr die neueste dauerhaft verifizierte
                            Karte.
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'moduleUnavailable'}
                          <p class="project-status" role="status">
                            Das ausgewählte primäre Modul ist in der aktuellen Publikation nicht
                            mehr verfügbar.
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'projectionUnavailable'}
                          <p class="project-status">
                            Für diese Publikation existiert keine Modulprojektion.
                          </p>
                        {:else if moduleCardEvidenceView.kind === 'noPublishedIndex'}
                          <p class="project-status">Noch kein veröffentlichter Index.</p>
                        {:else if moduleCardEvidenceView.kind === 'noProject'}
                          <p class="project-status">Kein Projekt ist aktiv.</p>
                        {:else if moduleCardEvidenceView.kind === 'available'}
                          {@const evidence = moduleCardEvidenceView.result.detail}
                          <div
                            class:module-card-evidence-current={evidence.freshness === 'current'}
                            class:module-card-evidence-stale={evidence.freshness === 'stale'}
                            class="module-card-evidence-freshness"
                            role={evidence.freshness === 'stale' ? 'alert' : 'note'}
                          >
                            <strong>
                              {evidence.freshness === 'current'
                                ? 'Evidence Current'
                                : 'Evidence Stale — nur historische Provenienz'}
                            </strong>
                            <span>
                              {evidence.freshness === 'current'
                                ? 'Die exakte Evidence löst im aktuellen veröffentlichten Index auf.'
                                : 'Die exakte Evidence ist im aktuellen veröffentlichten Index nicht mehr vorhanden.'}
                            </span>
                          </div>
                          <p class="module-card-evidence-card-state" role="note">
                            <strong>Card-Zustand:</strong>
                            {evidence.cardLifecycle.status === 'current'
                              ? ' Current'
                              : evidence.cardLifecycle.status === 'stale'
                                ? ' Stale — keine aktuelle Faktenquelle'
                                : ' NeedsReview — keine aktuelle Faktenquelle'}
                            {#if evidence.cardLifecycle.status !== 'current'}
                              · {moduleCardFreshnessReasonLabel(evidence.cardLifecycle.reason)}
                            {/if}
                          </p>
                          <dl>
                            <div>
                              <dt>Evidence-ID</dt>
                              <dd><code>{evidence.evidenceId}</code></dd>
                            </div>
                            <div>
                              <dt>Aktueller Indexlauf</dt>
                              <dd><code>{evidence.currentIndexRunId}</code></dd>
                            </div>
                            <div>
                              <dt>Evidence-Quelle</dt>
                              <dd><code>{evidence.sourceIndexRunId}</code></dd>
                            </div>
                          </dl>
                          {#if evidence.payload.kind === 'file'}
                            <section
                              class="module-card-evidence-payload"
                              aria-label="Datei-Evidence"
                            >
                              <h6>Dateirevision</h6>
                              <dl>
                                <div>
                                  <dt>Pfad</dt>
                                  <dd>
                                    <code
                                      >{pathDisplayFromHex(evidence.payload.revision.pathHex)}</code
                                    >
                                  </dd>
                                </div>
                                <div>
                                  <dt>Content Hash</dt>
                                  <dd><code>{evidence.payload.revision.contentHash}</code></dd>
                                </div>
                              </dl>
                            </section>
                          {:else if evidence.payload.kind === 'symbol'}
                            <section
                              class="module-card-evidence-payload"
                              aria-label="Symbol-Evidence"
                            >
                              <h6>Strukturelles Symbol</h6>
                              <dl>
                                <div>
                                  <dt>Symbol-ID</dt>
                                  <dd><code>{evidence.payload.symbolId}</code></dd>
                                </div>
                                <div>
                                  <dt>Revision</dt>
                                  <dd>
                                    <code
                                      >{pathDisplayFromHex(evidence.payload.revision.pathHex)}</code
                                    >
                                    · {evidence.payload.revision.contentHash}
                                  </dd>
                                </div>
                              </dl>
                            </section>
                          {:else}
                            {@const edge = evidence.payload.edge}
                            <section
                              class="module-card-evidence-payload"
                              aria-label="Graph-Evidence"
                            >
                              <h6>Deterministische Graph-Beziehung</h6>
                              <dl>
                                <div>
                                  <dt>Relation</dt>
                                  <dd>
                                    {moduleCardEvidenceRelationLabel(evidence.payload.relation)}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Quelle</dt>
                                  <dd>
                                    {#if edge.source.kind === 'file'}
                                      Datei <code>{pathDisplayFromHex(edge.source.pathHex)}</code>
                                    {:else}
                                      Symbol <code>{edge.source.symbolId}</code>
                                    {/if}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Ziel</dt>
                                  <dd>
                                    {#if edge.target.kind === 'file'}
                                      Datei <code>{pathDisplayFromHex(edge.target.pathHex)}</code>
                                    {:else}
                                      Symbol <code>{edge.target.symbolId}</code>
                                    {/if}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Quellrevision</dt>
                                  <dd>
                                    <code>{pathDisplayFromHex(edge.pathHex)}</code> · {edge.contentHash}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Bereich</dt>
                                  <dd>
                                    Bytes {edge.range.startByte}–{edge.range.endByte} · Zeile {edge
                                      .range.start.row + 1}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Beobachtung</dt>
                                  <dd>
                                    {edge.provider} · {edge.resolution} · {percentageLabel(
                                      edge.confidenceBasisPoints,
                                    )}
                                  </dd>
                                </div>
                              </dl>
                            </section>
                          {/if}
                        {:else if moduleCardEvidenceView.kind === 'error'}
                          <div class="recent-projects-error" role="alert">
                            <p>Die Evidence konnte nicht sicher gelesen werden.</p>
                          </div>
                        {/if}
                      </aside>
                    {:else if moduleCardDetailView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Die Module Card konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={reloadModuleCardDetail}>Erneut laden</button>
                      </div>
                    {/if}
                  </div>
                {:else if mapWorkspaceView === 'module' && moduleWorkspaceView === 'runtime'}
                  <div
                    class="repository-tree-panel module-runtime-panel"
                    aria-labelledby="module-runtime-heading"
                  >
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="module-runtime-heading">Entry Points &amp; Tests</h4>
                        <p>
                          Aktuelle strukturelle Wurzeln und bewusst geladene, begrenzte
                          Evidence-Pfade.
                        </p>
                      </div>
                      <button
                        type="button"
                        disabled={moduleRuntimeSelection === null ||
                          moduleRuntimeMapView.kind === 'loading'}
                        onclick={reloadModuleRuntime}
                      >
                        Aktualisieren
                      </button>
                    </div>
                    {#if moduleRuntimeMapView.kind === 'idle' || moduleRuntimeMapView.kind === 'noProject'}
                      <p class="project-status">
                        Wähle im Modulbaum „Entry Points &amp; Tests“, um die aktuellen Root-Symbole
                        zu laden.
                      </p>
                    {:else if moduleRuntimeMapView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Entry Points und Tests für {moduleRuntimeSelection?.name ?? 'das Modul'} werden
                        gelesen …
                      </p>
                    {:else if moduleRuntimeMapView.kind === 'noPublishedIndex'}
                      <p class="project-status">Noch kein vollständiger Snapshot veröffentlicht.</p>
                    {:else if moduleRuntimeMapView.kind === 'projectionUnavailable'}
                      <p class="project-status">
                        Der historische Index enthält noch keine deterministische
                        V8-Modulprojektion. Ein Rebuild erzeugt sie mit dem aktuellen Schema.
                      </p>
                    {:else if moduleRuntimeMapView.kind === 'moduleUnavailable'}
                      <p class="project-status" role="alert">
                        Das gewählte Primärmodul ist im aktuellen Index nicht mehr vorhanden.
                      </p>
                    {:else if moduleRuntimeMapView.kind === 'stale'}
                      <div class="recent-projects-error" role="alert">
                        <p>
                          Die sichtbare Root-Liste ist nicht mehr verifizierbar. Alte Roots und
                          Evidence bleiben ausgeblendet, bis der aktuelle Index erneut gelesen
                          wurde.
                        </p>
                        <button type="button" onclick={reloadModuleRuntime}>Roots neu laden</button>
                      </div>
                    {:else if moduleRuntimeMapView.kind === 'available'}
                      {@const runtimeMap = moduleRuntimeMapView.result.map}
                      <p class="index-snapshot">
                        Indexlauf <code>{runtimeMap.indexRunId}</code>
                      </p>
                      <div class="runtime-observation-note" role="note">
                        <strong>Strukturelle Beobachtung.</strong> Entry Points, Tests und Beziehungen
                        stammen aus deterministischen Adaptern. Sie belegen Quellstruktur, nicht eine
                        tatsächlich ausgeführte Laufzeitspur.
                      </div>
                      <div class="runtime-root-columns">
                        <section aria-labelledby="runtime-entrypoints-heading">
                          <div class="runtime-root-heading">
                            <h5 id="runtime-entrypoints-heading">Entry Points</h5>
                            <span>{countLabel(runtimeMap.entrypoints.storedCount)} gespeichert</span
                            >
                          </div>
                          {#if runtimeMap.entrypoints.roots.length === 0}
                            <p class="project-status">
                              Keine strukturellen Entry Points beobachtet.
                            </p>
                          {:else}
                            <ol class="runtime-root-list">
                              {#each runtimeMap.entrypoints.roots as root (root.symbol.symbolId)}
                                <li>
                                  <button
                                    class="runtime-root-flow"
                                    type="button"
                                    aria-label={`Aufrufpfad für Entry Point ${root.symbol.name} anzeigen`}
                                    onclick={() => loadModuleRuntimeFlow(root)}
                                  >
                                    <strong>{root.symbol.name}</strong>
                                    <span
                                      >Rang {root.rank} · {pathDisplayFromHex(
                                        root.symbol.pathHex,
                                      )}</span
                                    >
                                  </button>
                                  <button
                                    type="button"
                                    onclick={() =>
                                      (selectedModuleRuntimeEvidence = {
                                        kind: 'symbol',
                                        symbol: root.symbol,
                                      })}
                                  >
                                    Symbol-Evidence
                                  </button>
                                </li>
                              {/each}
                            </ol>
                          {/if}
                          {#if runtimeMap.entrypoints.roots.length < Number(runtimeMap.entrypoints.storedCount)}
                            <button
                              class="repository-tree-more"
                              type="button"
                              onclick={() => loadMoreModuleRuntimeRoots('entrypoint')}
                            >
                              Weitere Entry Points laden
                            </button>
                          {/if}
                          {#if runtimeMap.entrypoints.projectionTruncated}
                            <p class="runtime-truncation-note">
                              Die Modulbildung hat weitere, niedriger gerankte Entry Points hinter
                              ihrer festen 256-Root-Grenze ausgelassen.
                            </p>
                          {/if}
                        </section>
                        <section aria-labelledby="runtime-tests-heading">
                          <div class="runtime-root-heading">
                            <h5 id="runtime-tests-heading">Tests</h5>
                            <span>{countLabel(runtimeMap.tests.storedCount)} gespeichert</span>
                          </div>
                          {#if runtimeMap.tests.roots.length === 0}
                            <p class="project-status">
                              Keine strukturellen Testdefinitionen beobachtet.
                            </p>
                          {:else}
                            <ol class="runtime-root-list">
                              {#each runtimeMap.tests.roots as root (root.symbol.symbolId)}
                                <li>
                                  <button
                                    class="runtime-root-flow"
                                    type="button"
                                    aria-label={`Testziele für Test ${root.symbol.name} anzeigen`}
                                    onclick={() => loadModuleRuntimeFlow(root)}
                                  >
                                    <strong>{root.symbol.name}</strong>
                                    <span
                                      >Rang {root.rank} · {pathDisplayFromHex(
                                        root.symbol.pathHex,
                                      )}</span
                                    >
                                  </button>
                                  <button
                                    type="button"
                                    onclick={() =>
                                      (selectedModuleRuntimeEvidence = {
                                        kind: 'symbol',
                                        symbol: root.symbol,
                                      })}
                                  >
                                    Symbol-Evidence
                                  </button>
                                </li>
                              {/each}
                            </ol>
                          {/if}
                          {#if runtimeMap.tests.roots.length < Number(runtimeMap.tests.storedCount)}
                            <button
                              class="repository-tree-more"
                              type="button"
                              onclick={() => loadMoreModuleRuntimeRoots('test')}
                            >
                              Weitere Tests laden
                            </button>
                          {/if}
                          {#if runtimeMap.tests.projectionTruncated}
                            <p class="runtime-truncation-note">
                              Die Modulbildung hat weitere, niedriger gerankte Tests hinter ihrer
                              festen 256-Root-Grenze ausgelassen.
                            </p>
                          {/if}
                        </section>
                      </div>

                      <section class="runtime-flow" aria-labelledby="runtime-flow-heading">
                        <h5 id="runtime-flow-heading">Expliziter Evidence-Pfad</h5>
                        {#if moduleRuntimeFlowView.kind === 'idle'}
                          <p class="project-status">
                            Wähle einen Root: Entry Points folgen höchstens zwei „Calls“-Kanten,
                            Tests genau einer direkten „Tests“-Kante.
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'loading'}
                          <p class="project-status" role="status" aria-live="polite">
                            Evidence-Pfad für {moduleRuntimeFlowView.rootName} wird gelesen …
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'publicationChanged'}
                          <div class="recent-projects-error" role="alert">
                            <p>
                              Seit der Root-Auswahl wurde ein anderer Index veröffentlicht. Die alte
                              Evidence wird nicht mit dem neuen Snapshot gemischt.
                            </p>
                            <button type="button" onclick={reloadModuleRuntime}
                              >Roots neu laden</button
                            >
                          </div>
                        {:else if moduleRuntimeFlowView.kind === 'rootUnavailable'}
                          <p class="project-status" role="alert">
                            Das Symbol ist kein aktueller Root dieser Rolle mehr. Lade die
                            Root-Liste neu.
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'moduleUnavailable'}
                          <p class="project-status" role="alert">
                            Das Primärmodul ist nicht mehr aktuell.
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'projectionUnavailable'}
                          <p class="project-status">
                            Die erforderliche Graphprojektion ist nicht verfügbar.
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'noPublishedIndex' || moduleRuntimeFlowView.kind === 'noProject'}
                          <p class="project-status">
                            Kein aktueller veröffentlichter Index verfügbar.
                          </p>
                        {:else if moduleRuntimeFlowView.kind === 'available'}
                          {@const flow = moduleRuntimeFlowView.result.flow}
                          {#if flow.hits.length === 0}
                            <p class="ready-label">
                              Keine Ziele für das feste Relationspreset beobachtet.
                            </p>
                          {:else}
                            <ol class="runtime-flow-list">
                              {#each flow.hits as hit, hitIndex (hitIndex)}
                                <li>
                                  <div class="runtime-flow-target">
                                    <strong>{moduleRuntimeTargetLabel(hit.target)}</strong>
                                    <button
                                      type="button"
                                      onclick={() => selectModuleRuntimeTargetEvidence(hit.target)}
                                    >
                                      Ziel-Evidence
                                    </button>
                                  </div>
                                  <ol
                                    aria-label={`Evidence-Pfad zu ${moduleRuntimeTargetLabel(hit.target)}`}
                                  >
                                    {#each hit.path as step, stepIndex (step.evidence.evidenceId)}
                                      <li>
                                        <span>
                                          Schritt {stepIndex + 1}: {step.relation === 'calls'
                                            ? 'beobachteter Aufruf'
                                            : 'beobachtete Testbeziehung'}
                                        </span>
                                        <button
                                          type="button"
                                          onclick={() =>
                                            (selectedModuleRuntimeEvidence = {
                                              kind: 'edge',
                                              evidence: step.evidence,
                                            })}
                                        >
                                          Kanten-Evidence
                                        </button>
                                      </li>
                                    {/each}
                                  </ol>
                                </li>
                              {/each}
                            </ol>
                          {/if}
                          {#if flow.truncated}
                            <p class="runtime-truncation-note">
                              Weitere Ziele liegen hinter der festen Ergebnis- oder
                              Kanteninspektionsgrenze.
                            </p>
                          {/if}
                        {:else if moduleRuntimeFlowView.kind === 'error'}
                          <p class="project-error" role="alert">
                            Der Evidence-Pfad konnte nicht sicher gelesen werden.
                          </p>
                        {/if}
                      </section>

                      {#if selectedModuleRuntimeEvidence !== null}
                        <aside
                          class="dependency-evidence"
                          aria-labelledby="runtime-evidence-heading"
                        >
                          <div>
                            <h5 id="runtime-evidence-heading">
                              {selectedModuleRuntimeEvidence.kind === 'edge'
                                ? 'Graph-Kanten-Evidence'
                                : selectedModuleRuntimeEvidence.kind === 'symbol'
                                  ? 'Symbol-Evidence'
                                  : 'Datei-Evidence'}
                            </h5>
                            <button
                              type="button"
                              onclick={() => (selectedModuleRuntimeEvidence = null)}
                            >
                              Schließen
                            </button>
                          </div>
                          {#if selectedModuleRuntimeEvidence.kind === 'symbol'}
                            {@const selectedSymbol = selectedModuleRuntimeEvidence.symbol}
                            <dl>
                              <div>
                                <dt>Name</dt>
                                <dd>{selectedSymbol.name}</dd>
                              </div>
                              <div>
                                <dt>Evidence-ID</dt>
                                <dd><code>{selectedSymbol.evidenceId}</code></dd>
                              </div>
                              <div>
                                <dt>Aktuelle Revision</dt>
                                <dd>
                                  <code>{pathDisplayFromHex(selectedSymbol.pathHex)}</code> ·
                                  {selectedSymbol.contentHash.slice(0, 12)}
                                </dd>
                              </div>
                              <div>
                                <dt>Auswahlbereich</dt>
                                <dd>
                                  Bytes {selectedSymbol.selectionRange.startByte}–{selectedSymbol
                                    .selectionRange.endByte}
                                  · Zeile {selectedSymbol.selectionRange.start.row + 1}
                                </dd>
                              </div>
                            </dl>
                          {:else if selectedModuleRuntimeEvidence.kind === 'edge'}
                            {@const selectedEdge = selectedModuleRuntimeEvidence.evidence}
                            <dl>
                              <div>
                                <dt>Evidence-ID</dt>
                                <dd><code>{selectedEdge.evidenceId}</code></dd>
                              </div>
                              <div>
                                <dt>Aktuelle Revision</dt>
                                <dd>
                                  <code>{pathDisplayFromHex(selectedEdge.pathHex)}</code> ·
                                  {selectedEdge.contentHash.slice(0, 12)}
                                </dd>
                              </div>
                              <div>
                                <dt>Bereich</dt>
                                <dd>
                                  Bytes {selectedEdge.range.startByte}–{selectedEdge.range.endByte} ·
                                  Zeile
                                  {selectedEdge.range.start.row + 1}
                                </dd>
                              </div>
                              <div>
                                <dt>Confidence</dt>
                                <dd>{percentageLabel(selectedEdge.confidenceBasisPoints)}</dd>
                              </div>
                            </dl>
                          {:else}
                            <dl>
                              <div>
                                <dt>Evidence-ID</dt>
                                <dd><code>{selectedModuleRuntimeEvidence.evidenceId}</code></dd>
                              </div>
                              <div>
                                <dt>Aktuelle Revision</dt>
                                <dd>
                                  <code
                                    >{pathDisplayFromHex(
                                      selectedModuleRuntimeEvidence.pathHex,
                                    )}</code
                                  >
                                  ·
                                  {selectedModuleRuntimeEvidence.contentHash.slice(0, 12)}
                                </dd>
                              </div>
                            </dl>
                          {/if}
                        </aside>
                      {/if}
                    {:else if moduleRuntimeMapView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Entry Points und Tests konnten nicht sicher gelesen werden.</p>
                        <button type="button" onclick={reloadModuleRuntime}>Erneut laden</button>
                      </div>
                    {/if}
                  </div>
                {:else if mapWorkspaceView === 'module' && moduleWorkspaceView === 'dependencies'}
                  <div
                    class="repository-tree-panel module-dependency-panel"
                    aria-labelledby="module-dependency-heading"
                  >
                    <div class="repository-tree-heading">
                      <div>
                        <h4 id="module-dependency-heading">Modulabhängigkeiten</h4>
                        <p>
                          Direkte, belegte Beziehungen eines Primärmoduls; große Nachbarschaften
                          bleiben sichtbar begrenzt.
                        </p>
                      </div>
                      <button
                        type="button"
                        disabled={moduleDependencySelection === null ||
                          moduleDependencyGraphView.kind === 'loading'}
                        onclick={reloadModuleDependencies}
                      >
                        Aktualisieren
                      </button>
                    </div>
                    {#if moduleDependencyGraphView.kind === 'idle'}
                      <p class="project-status">
                        Wähle im Modulbaum „Abhängigkeiten anzeigen“, um einen direkten Ausschnitt
                        zu laden.
                      </p>
                    {:else if moduleDependencyGraphView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Abhängigkeiten für {moduleDependencySelection?.name ?? 'das Modul'} werden gelesen
                        …
                      </p>
                    {:else if moduleDependencyGraphView.kind === 'noPublishedIndex'}
                      <p class="project-status">Noch kein vollständiger Snapshot veröffentlicht.</p>
                    {:else if moduleDependencyGraphView.kind === 'projectionUnavailable'}
                      <p class="project-status">
                        Der historische Index enthält noch keine deterministische Modulprojektion.
                        Ein Rebuild erzeugt sie mit dem aktuellen Schema.
                      </p>
                    {:else if moduleDependencyGraphView.kind === 'centerUnavailable'}
                      <p class="project-status" role="alert">
                        Das gewählte Primärmodul ist im aktuellen veröffentlichten Index nicht mehr
                        vorhanden.
                      </p>
                    {:else if moduleDependencyGraphView.kind === 'available'}
                      {@const graph = moduleDependencyGraphView.result.graph}
                      {#if ModuleDependencyGraph !== null}
                        <ModuleDependencyGraph
                          {graph}
                          selectedEvidence={selectedDependencyEvidence}
                          onSelectEvidence={(evidence) => (selectedDependencyEvidence = evidence)}
                          onClearEvidence={() => (selectedDependencyEvidence = null)}
                        />
                      {:else if moduleDependencyGraphChunkState === 'error'}
                        <div class="recent-projects-error" role="alert">
                          <p>Die lokale Graphdarstellung konnte nicht geladen werden.</p>
                          <button type="button" onclick={loadModuleDependencyGraphChunk}
                            >Erneut laden</button
                          >
                        </div>
                      {:else}
                        <p class="project-status" role="status">Graphdarstellung wird geladen …</p>
                      {/if}
                    {:else if moduleDependencyGraphView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Der Modulabhängigkeitsgraph konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={reloadModuleDependencies}
                          >Erneut laden</button
                        >
                      </div>
                    {/if}
                  </div>
                {:else if mapWorkspaceView === 'mapping'}
                  <div
                    class="index-overview module-card-freshness"
                    aria-labelledby="module-card-freshness-heading"
                  >
                    <div class="module-card-freshness-heading">
                      <div>
                        <h4 id="module-card-freshness-heading">Module-Card-Aktualität</h4>
                        <p>Autoritative Lebenszyklen der jeweils neuesten Karte pro Modul.</p>
                      </div>
                      <button type="button" onclick={() => void loadModuleCardFreshness()}
                        >Aktualisieren</button
                      >
                    </div>
                    {#if moduleCardFreshnessView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Module-Card-Aktualität wird gelesen …
                      </p>
                    {:else if moduleCardFreshnessView.kind === 'noPublishedIndex'}
                      <p class="project-status">
                        Noch kein veröffentlichter Index; daher existiert noch keine aktuelle
                        Lebenszyklusprojektion.
                      </p>
                    {:else if moduleCardFreshnessView.kind === 'available'}
                      <p class="index-snapshot">
                        Indexlauf <code>{moduleCardFreshnessView.result.freshness.indexRunId}</code>
                      </p>
                      <dl class="index-metrics module-card-freshness-metrics">
                        <div>
                          <dt>Current</dt>
                          <dd>
                            {countLabel(
                              moduleCardFreshnessView.result.freshness.counts.publishedCount,
                            )}
                          </dd>
                        </div>
                        <div>
                          <dt>Stale</dt>
                          <dd>
                            {countLabel(moduleCardFreshnessView.result.freshness.counts.staleCount)}
                          </dd>
                        </div>
                        <div>
                          <dt>NeedsReview</dt>
                          <dd>
                            {countLabel(
                              moduleCardFreshnessView.result.freshness.counts.needsReviewCount,
                            )}
                          </dd>
                        </div>
                        <div>
                          <dt>Gesamt</dt>
                          <dd>
                            {countLabel(moduleCardFreshnessView.result.freshness.counts.totalCount)}
                          </dd>
                        </div>
                      </dl>
                      {#if moduleCardFreshnessView.result.freshness.reasons.length === 0}
                        <p class="ready-label">Alle bekannten Module Cards sind aktuell.</p>
                      {:else}
                        <ul class="module-card-freshness-reasons">
                          {#each moduleCardFreshnessView.result.freshness.reasons as reason (reason.status + reason.reason)}
                            <li>
                              <strong>{reason.status === 'stale' ? 'Stale' : 'NeedsReview'}:</strong
                              >
                              {moduleCardFreshnessReasonLabel(reason.reason)} · {countLabel(
                                reason.count,
                              )}
                            </li>
                          {/each}
                        </ul>
                      {/if}
                    {:else if moduleCardFreshnessView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Die Module-Card-Aktualität konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={() => void loadModuleCardFreshness()}
                          >Erneut laden</button
                        >
                      </div>
                    {/if}
                  </div>
                  <div class="deep-map-panel" aria-labelledby="deep-map-heading">
                    <div class="deep-map-heading">
                      <div>
                        <h4 id="deep-map-heading">Deep Map</h4>
                        <p>
                          Startet niemals automatisch. Modell und harte Budgets werden vor jeder
                          neuen Exploration sichtbar festgelegt.
                        </p>
                      </div>
                      <button type="button" onclick={() => void loadDeepMap()}
                        >Status aktualisieren</button
                      >
                    </div>
                    {#if deepMapView.kind === 'loading'}
                      <p class="project-status" role="status" aria-live="polite">
                        Deep-Map-Status wird geladen …
                      </p>
                    {:else if deepMapView.kind === 'unavailable'}
                      <div class="deep-map-unavailable" role="status">
                        <strong>Keine Modellarbeit aktiv</strong>
                        <p>
                          Es ist noch kein live verifiziertes lokales Mapping-Modell konfiguriert.
                          Fast Index und veröffentlichte Daten bleiben ohne Modell vollständig
                          nutzbar.
                        </p>
                        <button type="button" disabled>Deep Map bewusst starten</button>
                      </div>
                    {:else if deepMapView.kind === 'available'}
                      <p class="deep-map-state" role="status" aria-live="polite">
                        {deepMapStateLabel(deepMapView.result.activity.state)}
                      </p>
                      <dl class="deep-map-model">
                        <div>
                          <dt>Mapping-Modell</dt>
                          <dd>
                            {deepMapView.result.configuration.model.providerId} /
                            {deepMapView.result.configuration.model.modelId}
                          </dd>
                        </div>
                        <div>
                          <dt>Kontextlimit</dt>
                          <dd>
                            {countLabel(
                              String(deepMapView.result.configuration.model.contextTokens),
                            )} Tokens
                          </dd>
                        </div>
                        <div>
                          <dt>Outputlimit je Antwort</dt>
                          <dd>
                            {countLabel(
                              String(deepMapView.result.configuration.model.outputTokens),
                            )} Tokens
                          </dd>
                        </div>
                        <div>
                          <dt>Verifiziertes Profil</dt>
                          <dd><code>{deepMapView.result.configuration.model.profileId}</code></dd>
                        </div>
                      </dl>
                      <fieldset
                        class="deep-map-budget"
                        disabled={deepMapActionView.kind === 'submitting' ||
                          !deepMapCanStart(deepMapView.result.activity.state)}
                      >
                        <legend>Harte Budgets vor Start</legend>
                        <label>
                          Tokenbudget
                          <input
                            type="number"
                            min={deepMapView.result.configuration.minimumBudget.tokenLimit}
                            max={deepMapView.result.configuration.maximumBudget.tokenLimit}
                            bind:value={deepMapBudget.tokenLimit}
                          />
                        </label>
                        <label>
                          Zeitbudget in Millisekunden
                          <input
                            type="number"
                            min={deepMapView.result.configuration.minimumBudget.timeLimitMillis}
                            max={deepMapView.result.configuration.maximumBudget.timeLimitMillis}
                            bind:value={deepMapBudget.timeLimitMillis}
                          />
                        </label>
                        <label>
                          Read-only-Werkzeugaufrufe
                          <input
                            type="number"
                            min={deepMapView.result.configuration.minimumBudget.toolCallLimit}
                            max={deepMapView.result.configuration.maximumBudget.toolCallLimit}
                            bind:value={deepMapBudget.toolCallLimit}
                          />
                        </label>
                      </fieldset>
                      {#if deepMapView.result.activity.budget !== null}
                        <p class="deep-map-run-budget">
                          Laufbudget: {countLabel(
                            String(deepMapView.result.activity.budget.tokenLimit),
                          )}
                          Tokens · {countLabel(
                            String(deepMapView.result.activity.budget.timeLimitMillis),
                          )} ms ·
                          {countLabel(String(deepMapView.result.activity.budget.toolCallLimit))} Read-Aufrufe
                        </p>
                      {/if}
                      {#if deepMapView.result.activity.progress !== null}
                        <progress
                          aria-label="Deep-Map-Fortschritt"
                          max={deepMapView.result.activity.progress.total}
                          value={deepMapView.result.activity.progress.completed}
                        ></progress>
                      {/if}
                      {#if deepMapView.result.activity.totalSteps !== '0'}
                        <p>
                          Bestätigte Schritte:
                          {countLabel(deepMapView.result.activity.confirmedSteps)} von
                          {countLabel(deepMapView.result.activity.totalSteps)}
                        </p>
                      {/if}
                      <div class="project-actions deep-map-actions">
                        <button
                          class="primary-action"
                          type="button"
                          disabled={deepMapActionView.kind === 'submitting' ||
                            !deepMapCanStart(deepMapView.result.activity.state)}
                          onclick={requestDeepMapStart}>Deep Map bewusst starten</button
                        >
                        <button
                          type="button"
                          disabled={deepMapActionView.kind === 'submitting' ||
                            deepMapView.result.activity.state !== 'running'}
                          onclick={() => requestDeepMapControl(deepMapPauser)}>Pausieren</button
                        >
                        <button
                          type="button"
                          disabled={deepMapActionView.kind === 'submitting' ||
                            deepMapView.result.activity.state !== 'paused'}
                          onclick={() => requestDeepMapControl(deepMapResumer)}>Fortsetzen</button
                        >
                        <button
                          class="risk-action"
                          type="button"
                          disabled={deepMapActionView.kind === 'submitting' ||
                            !['queued', 'running', 'pausing', 'paused', 'cancelling'].includes(
                              deepMapView.result.activity.state,
                            )}
                          onclick={() => requestDeepMapControl(deepMapCanceller)}>Abbrechen</button
                        >
                      </div>
                      {#if deepMapActionView.kind === 'error'}
                        <p class="project-error" role="alert">{deepMapActionView.message}</p>
                      {/if}
                    {:else if deepMapView.kind === 'error'}
                      <div class="recent-projects-error" role="alert">
                        <p>Der Deep-Map-Status konnte nicht sicher gelesen werden.</p>
                        <button type="button" onclick={() => void loadDeepMap()}
                          >Erneut laden</button
                        >
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
          {:else if projectStatusView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der aktive Projektstatus konnte nicht sicher geladen werden.</p>
              <button type="button" onclick={loadProjectStatus}>Status erneut laden</button>
            </div>
          {/if}

          {#if removalView.kind === 'removed'}
            <p class="ready-label" role="status" aria-live="polite">
              Worktree aus der A^3-Projektliste entfernt. Repository und private A^3-Daten bleiben
              erhalten.
            </p>
          {/if}
        </section>

        {#if projectStatusView.kind !== 'active'}
          <section
            id="map"
            class="route-placeholder"
            aria-labelledby="map-placeholder-heading"
            tabindex="-1"
          >
            <h2 id="map-placeholder-heading">Project Map</h2>
            {#if projectStatusView.kind === 'loading'}
              <p role="status">Project Map wartet auf den Projektstatus …</p>
            {:else if projectStatusView.kind === 'error'}
              <p role="alert">
                Project Map ist verfügbar, sobald der Projektstatus wieder geladen wurde.
              </p>
            {:else}
              <p>Öffne einen lokalen Worktree, um Project Map und Evidence zu verwenden.</p>
            {/if}
          </section>
        {/if}

        <div id="agent" class="lazy-boundary" bind:this={agentWorkspaceBoundary} tabindex="-1">
          {#if agentWorkspaceComponent !== null}
            {@const AgentWorkspace = agentWorkspaceComponent}
            <AgentWorkspace
              activeProject={projectStatusView.kind === 'active'}
              activityLoader={agentActivityLoader}
              approvalController={agentApprovalController}
              approvalLoader={agentApprovalLoader}
              goalCreator={agentGoalCreator}
              goalLoader={agentGoalLoader}
              goalReviser={agentGoalReviser}
              inspectionLoader={agentInspectionLoader}
              inspectionLogLoader={agentInspectionLogLoader}
              ledgerLoader={taskLensTaskLoader}
              onRunStatusChange={updateGlobalRunStatus}
              recoveryLoader={agentRecoveryLoader}
              runController={agentRunController}
              tasksLoader={agentGoalTasksLoader}
            />
          {:else}
            <section class="lazy-surface" aria-labelledby="lazy-agent-heading">
              <h2 id="lazy-agent-heading">Agent Workspace</h2>
              {#if agentWorkspaceState === 'error'}
                <p role="alert">Der lokale Agent-Workspace-Chunk konnte nicht geladen werden.</p>
                <button type="button" onclick={loadAgentWorkspaceChunk}>Erneut laden</button>
              {:else}
                <p role="status">Agent Workspace wird bei Sichtbarkeit geladen …</p>
                <button type="button" onclick={loadAgentWorkspaceChunk}>Jetzt laden</button>
              {/if}
            </section>
          {/if}
        </div>

        <div id="settings" class="lazy-boundary" bind:this={settingsBoundary} tabindex="-1">
          {#if settingsComponent !== null}
            {@const Settings = settingsComponent}
            <Settings {healthLoader} />
          {:else}
            <section class="lazy-surface" aria-labelledby="lazy-settings-heading">
              <h2 id="lazy-settings-heading">Modelle, Ressourcen und Datenschutz</h2>
              {#if settingsState === 'error'}
                <p role="alert">Der lokale Settings-Chunk konnte nicht geladen werden.</p>
                <button type="button" onclick={loadSettingsChunk}>Erneut laden</button>
              {:else}
                <p role="status">Settings werden bei Sichtbarkeit geladen …</p>
                <button type="button" onclick={loadSettingsChunk}>Jetzt laden</button>
              {/if}
            </section>
          {/if}
        </div>
      </div>
    </div>
  </section>
</main>

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
  import type {
    AgentGoalDraftInputV1,
    AgentGoalMutationResponseV1,
    AgentGoalResponseV1,
  } from './lib/agent-goal';
  import type {
    AgentInspectionLogResponseV1,
    AgentInspectionResponseV1,
    AgentInspectionStreamV1,
  } from './lib/agent-inspection';
  import type {
    AgentSessionControlActionV1,
    AgentSessionModeV1,
    AgentSessionResponseV1,
    AgentSessionsResponseV1,
    AgentResearchDepthSelectionV1,
  } from './lib/agent-session';
  import { projectActionRecoveryMessage, projectOpenRecoveryMessage } from './lib/command-error';
  import {
    cancelDeepMap,
    pauseDeepMap,
    queryDeepMap,
    resumeDeepMap,
    startDeepMap,
    type DeepMapControlResponseV1,
    type DeepMapModeV2,
    type DeepMapStartResponseV2,
    type DeepMapStatusResponseV3,
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
    type IndexDiagnosticSeverityV1,
    type IndexLanguageV1,
    type IndexOverviewResponseV1,
  } from './lib/index-overview';
  import type {
    ModuleCardDetailQueryV1,
    ModuleCardDetailResponseV1,
  } from './lib/module-card-detail';
  import type {
    ModuleCardEvidenceQueryV1,
    ModuleCardEvidenceResponseV1,
  } from './lib/module-card-evidence';
  import type { ModuleCardFreshnessResponseV1 } from './lib/module-card-freshness';
  import type {
    ModuleDependencyGraphQueryV1,
    ModuleDependencyGraphResponseV1,
  } from './lib/module-dependency-graph';
  import type { ModuleTreeQueryV1, ModuleTreeResponseV1 } from './lib/module-tree';
  import type {
    ModuleRuntimeFlowQueryV1,
    ModuleRuntimeFlowResponseV1,
    ModuleRuntimeMapQueryV1,
    ModuleRuntimeMapResponseV1,
  } from './lib/module-runtime';
  import { openProject, type GitHeadV1, type OpenProjectResponseV1 } from './lib/project';
  import {
    activateCatalogProject,
    queryProjectCatalog,
    removeCatalogProject,
    restoreLastProject,
    type ProjectActivationResponseV1,
    type ProjectCatalogEntryV1,
    type ProjectCatalogQueryV1,
    type ProjectCatalogResponseV1,
  } from './lib/project-catalog';
  import type {
    ProjectMapSearchQueryV1,
    ProjectMapSearchResponseV1,
  } from './lib/project-map-search';
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
  import { removeProject, type RemoveProjectResponseV1 } from './lib/project-removal';
  import {
    queryProjectStatus,
    type IndexStateV1,
    type ProjectStatusResponseV1,
    type RebuildStateV1,
  } from './lib/project-status';
  import type { RepositoryTreeQueryV1, RepositoryTreeResponseV1 } from './lib/repository-tree';
  import {
    compileTaskLens,
    queryTaskLensTask,
    queryTaskLensTasks,
    type TaskLensCompileQueryV1,
    type TaskLensCompileResponseV1,
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
    agentSessionController?: (
      sessionId: string,
      revision: string,
      action: AgentSessionControlActionV1,
    ) => Promise<AgentSessionResponseV1>;
    agentSessionLoader?: (sessionId: string) => Promise<AgentSessionResponseV1>;
    agentSessionsLoader?: (options?: {
      includeArchived?: boolean;
      search?: string | null;
    }) => Promise<AgentSessionsResponseV1>;
    agentMessageSubmitter?: (input: {
      expectedSessionRevision?: string | null;
      message: string;
      mode?: AgentSessionModeV1;
      researchDepth?: AgentResearchDepthSelectionV1;
      sessionId?: string | null;
    }) => Promise<AgentSessionResponseV1>;
    agentRecoveryLoader?: (taskId: string) => Promise<AgentTaskRecoveryResponseV1>;
    agentRunController?: (
      taskId: string,
      expectedLedgerRevision: number,
      expectedLedgerStoreVersion: string,
      action: AgentTaskControlActionV1,
    ) => Promise<AgentTaskControlResponseV1>;
    healthLoader?: () => Promise<HealthResponseV1>;
    deepMapStatusLoader?: () => Promise<DeepMapStatusResponseV3>;
    deepMapStarter?: (mode: DeepMapModeV2) => Promise<DeepMapStartResponseV2>;
    deepMapPauser?: () => Promise<DeepMapControlResponseV1>;
    deepMapResumer?: () => Promise<DeepMapControlResponseV1>;
    deepMapCanceller?: () => Promise<DeepMapControlResponseV1>;
    indexActivityLoader?: () => Promise<IndexActivityResponseV1>;
    indexOverviewLoader?: () => Promise<IndexOverviewResponseV1>;
    /** @deprecated Legacy U10 test seam; no longer consumed by the map UI. */
    moduleCardFreshnessLoader?: () => Promise<ModuleCardFreshnessResponseV1>;
    moduleCardDetailLoader?: (
      query: ModuleCardDetailQueryV1,
    ) => Promise<ModuleCardDetailResponseV1>;
    moduleCardEvidenceLoader?: (
      query: ModuleCardEvidenceQueryV1,
    ) => Promise<ModuleCardEvidenceResponseV1>;
    /** @deprecated Legacy U10 test seam; no longer consumed by the map UI. */
    moduleDependencyGraphLoader?: (
      query: ModuleDependencyGraphQueryV1,
    ) => Promise<ModuleDependencyGraphResponseV1>;
    moduleRuntimeMapLoader?: (
      query: ModuleRuntimeMapQueryV1,
    ) => Promise<ModuleRuntimeMapResponseV1>;
    /** @deprecated Legacy U10 test seam; no longer consumed by the map UI. */
    moduleRuntimeFlowLoader?: (
      query: ModuleRuntimeFlowQueryV1,
    ) => Promise<ModuleRuntimeFlowResponseV1>;
    /** @deprecated Legacy U10 test seam; no longer consumed by the map UI. */
    moduleTreeLoader?: (query: ModuleTreeQueryV1) => Promise<ModuleTreeResponseV1>;
    projectOpener?: () => Promise<OpenProjectResponseV1>;
    projectCatalogActivator?: (worktreeId: string) => Promise<ProjectActivationResponseV1>;
    projectCatalogLoader?: (query: ProjectCatalogQueryV1) => Promise<ProjectCatalogResponseV1>;
    projectCatalogRemover?: (worktreeId: string) => Promise<RemoveProjectResponseV1>;
    projectRestorer?: () => Promise<ProjectActivationResponseV1>;
    /** @deprecated U11 test seam; the lazy U12 workspace owns Project Map transport. */
    projectMapSearchLoader?: (
      query: ProjectMapSearchQueryV1,
    ) => Promise<ProjectMapSearchResponseV1>;
    projectRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    projectRemover?: () => Promise<RemoveProjectResponseV1>;
    projectStatusLoader?: () => Promise<ProjectStatusResponseV1>;
    /** @deprecated Legacy U10 test seam; no longer consumed by the map UI. */
    repositoryTreeLoader?: (query: RepositoryTreeQueryV1) => Promise<RepositoryTreeResponseV1>;
    taskLensTasksLoader?: () => Promise<TaskLensTasksResponseV1>;
    taskLensTaskLoader?: (query: TaskLensTaskQueryV1) => Promise<TaskLensTaskResponseV1>;
    taskLensCompiler?: (query: TaskLensCompileQueryV1) => Promise<TaskLensCompileResponseV1>;
  }

  type AgentWorkspaceComponent = typeof import('./lib/AgentWorkspace.svelte').default;
  type MapWorkspaceComponent = typeof import('./lib/MapWorkspace.svelte').default;
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
  type DeepMapView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'unavailable' }
    | {
        kind: 'available';
        result: Extract<DeepMapStatusResponseV3['result'], { status: 'available' }>;
      }
    | { kind: 'error' };
  type RebuildView = { kind: 'idle' } | { kind: 'submitting' } | { kind: 'error'; message: string };
  type RemovalView =
    | { kind: 'idle' }
    | { kind: 'confirming' }
    | { kind: 'submitting' }
    | { kind: 'removed' }
    | { kind: 'error'; message: string };
  type ProjectDialogView = 'index' | 'overview' | 'maintenance';
  type ProjectCatalogView =
    | { kind: 'loading' }
    | { kind: 'available'; page: ProjectCatalogResponseV1 }
    | { kind: 'error'; message: string };

  let {
    agentActivityLoader,
    agentApprovalController,
    agentApprovalLoader,
    agentInspectionLoader,
    agentInspectionLogLoader,
    agentSessionController,
    agentSessionLoader,
    agentSessionsLoader,
    agentMessageSubmitter,
    healthLoader = queryHealth,
    deepMapStatusLoader = queryDeepMap,
    deepMapStarter = startDeepMap,
    deepMapPauser = pauseDeepMap,
    deepMapResumer = resumeDeepMap,
    deepMapCanceller = cancelDeepMap,
    indexActivityLoader = queryIndexActivity,
    indexOverviewLoader = queryIndexOverview,
    projectOpener = openProject,
    projectCatalogActivator = activateCatalogProject,
    projectCatalogLoader = queryProjectCatalog,
    projectCatalogRemover = removeCatalogProject,
    projectRestorer = restoreLastProject,
    projectMapSearchLoader,
    projectRebuilder = rebuildProjectIndex,
    projectRemover = removeProject,
    projectStatusLoader = queryProjectStatus,
    taskLensTasksLoader = queryTaskLensTasks,
    taskLensTaskLoader = queryTaskLensTask,
    taskLensCompiler = compileTaskLens,
  }: Props = $props();
  let projectView = $state<ProjectView>({ kind: 'idle' });
  let projectStatusView = $state<ProjectStatusView>({ kind: 'loading' });
  let projectCatalogView = $state<ProjectCatalogView>({ kind: 'loading' });
  let projectCatalogSearchInput = $state('');
  let projectCatalogSearch = $state<string | null>(null);
  let projectCatalogPageNumber = $state(1);
  let projectCatalogActivatingId = $state<string | null>(null);
  let projectCatalogRemovalTarget = $state<ProjectCatalogEntryV1 | null>(null);
  let projectCatalogRemoving = $state(false);
  let projectRestoreMessage = $state<string | null>(null);
  let indexActivityView = $state<IndexActivityView>({ kind: 'loading' });
  let indexOverviewView = $state<IndexOverviewView>({ kind: 'loading' });
  let deepMapView = $state<DeepMapView>({ kind: 'loading' });
  let rebuildView = $state<RebuildView>({ kind: 'idle' });
  let removalView = $state<RemovalView>({ kind: 'idle' });
  let projectDialogOpen = $state(false);
  let projectDialogView = $state<ProjectDialogView>('overview');
  let indexActivityObserved = false;
  let currentWorkspaceArea = $state<WorkspaceArea>('projects');
  let globalRunStatus = $state<GlobalRunStatus>({ kind: 'loading' });
  let uiScheduler: UiScheduler | null = null;
  let appMounted = false;
  let workspaceContent: HTMLElement;
  let agentWorkspaceBoundary: HTMLElement;
  let agentWorkspaceComponent = $state<AgentWorkspaceComponent | null>(null);
  let agentWorkspaceState = $state<LazySurfaceState>('idle');
  let settingsBoundary: HTMLElement;
  let settingsComponent = $state<SettingsPanelComponent | null>(null);
  let settingsState = $state<LazySurfaceState>('idle');
  let mapWorkspaceComponent = $state<MapWorkspaceComponent | null>(null);
  let mapWorkspaceState = $state<LazySurfaceState>('idle');

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
    if (area === 'map') void loadMapWorkspaceChunk();
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
    if (currentWorkspaceArea === 'map') void loadMapWorkspaceChunk();
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
          value: `Mapping bereit · ${deepMapView.result.model.modelId}`,
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
      const component = await import('./lib/AgentWorkspace.svelte');
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

  async function loadMapWorkspaceChunk(): Promise<void> {
    if (mapWorkspaceState === 'loading' || mapWorkspaceState === 'ready') return;
    mapWorkspaceState = 'loading';
    try {
      const component = await import('./lib/MapWorkspace.svelte');
      if (!appMounted) return;
      mapWorkspaceComponent = component.default;
      mapWorkspaceState = 'ready';
    } catch {
      if (appMounted) mapWorkspaceState = 'error';
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

  function resetProjectOwnedUi(kind: 'idle' | 'noProject'): void {
    const noProject = kind === 'noProject';
    indexActivityView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    indexOverviewView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
    deepMapView = noProject ? { kind: 'noProject' } : { kind: 'loading' };
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
    try {
      await projectRestorer();
      projectRestoreMessage = null;
    } catch (error) {
      projectRestoreMessage = projectOpenRecoveryMessage(error);
    }
    await loadProjectStatus();
    await loadProjectCatalog('initial', null, projectCatalogSearch);
    pollProjectActivity();
    await loadIndexOverview();
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

  async function loadDeepMap(generation = projectGeneration()): Promise<void> {
    try {
      const response = await deepMapStatusLoader();
      commitProjectView('deep-map', generation, () => {
        if (response.result.status === 'available') {
          deepMapView = { kind: 'available', result: response.result };
        } else if (response.result.status === 'unavailable') {
          deepMapView = { kind: 'unavailable' };
        } else {
          deepMapView = { kind: 'noProject' };
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
          loadProjectCatalog('initial', null, projectCatalogSearch),
          loadIndexOverview(),
        ]);
      } else {
        projectView = { kind: 'cancelled' };
      }
    } catch (error) {
      projectView = { kind: 'error', message: projectOpenRecoveryMessage(error) };
    }
  }

  async function loadProjectCatalog(
    direction: ProjectCatalogQueryV1['direction'],
    cursor: string | null,
    search: string | null,
  ): Promise<void> {
    projectCatalogView = { kind: 'loading' };
    try {
      const page = await projectCatalogLoader({ cursor, direction, search });
      projectCatalogView = { kind: 'available', page };
    } catch (error) {
      projectCatalogView = {
        kind: 'error',
        message: projectActionRecoveryMessage(error, 'remove'),
      };
    }
  }

  async function submitProjectCatalogSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const search = projectCatalogSearchInput.trim();
    projectCatalogSearch = search.length === 0 ? null : search;
    projectCatalogPageNumber = 1;
    await loadProjectCatalog('initial', null, projectCatalogSearch);
  }

  async function clearProjectCatalogSearch(): Promise<void> {
    projectCatalogSearchInput = '';
    projectCatalogSearch = null;
    projectCatalogPageNumber = 1;
    await loadProjectCatalog('initial', null, null);
  }

  async function navigateProjectCatalog(direction: 'next' | 'previous'): Promise<void> {
    if (projectCatalogView.kind !== 'available') return;
    const cursor =
      direction === 'next'
        ? projectCatalogView.page.nextCursor
        : projectCatalogView.page.previousCursor;
    if (cursor === null) return;
    await loadProjectCatalog(direction, cursor, projectCatalogSearch);
    if (projectCatalogView.kind === 'available') {
      projectCatalogPageNumber += direction === 'next' ? 1 : -1;
    }
  }

  async function activateProjectFromCatalog(entry: ProjectCatalogEntryV1): Promise<void> {
    projectCatalogActivatingId = entry.project.worktreeId;
    projectRestoreMessage = null;
    try {
      const response = await projectCatalogActivator(entry.project.worktreeId);
      if (response.result.status !== 'activated') return;
      if (uiScheduler?.beginProject(response.result.project.worktreeId) ?? false) {
        resetProjectOwnedUi('idle');
      }
      await loadProjectStatus();
      pollProjectActivity();
      await Promise.all([
        loadProjectCatalog('initial', null, projectCatalogSearch),
        loadIndexOverview(),
      ]);
      projectCatalogPageNumber = 1;
    } catch (error) {
      projectRestoreMessage = projectOpenRecoveryMessage(error);
    } finally {
      projectCatalogActivatingId = null;
    }
  }

  function requestCatalogRemoval(entry: ProjectCatalogEntryV1): void {
    projectCatalogRemovalTarget = entry;
  }

  function cancelCatalogRemoval(): void {
    if (!projectCatalogRemoving) projectCatalogRemovalTarget = null;
  }

  async function confirmCatalogRemoval(): Promise<void> {
    const target = projectCatalogRemovalTarget;
    if (target === null) return;
    projectCatalogRemoving = true;
    try {
      await projectCatalogRemover(target.project.worktreeId);
      if (
        projectStatusView.kind === 'active' &&
        projectStatusView.result.project.worktreeId === target.project.worktreeId
      ) {
        projectStatusView = { kind: 'noProject' };
        projectView = { kind: 'idle' };
        projectDialogOpen = false;
        uiScheduler?.beginProject(null);
        resetProjectOwnedUi('noProject');
      }
      projectCatalogRemovalTarget = null;
      projectCatalogPageNumber = 1;
      await loadProjectCatalog('initial', null, projectCatalogSearch);
    } catch (error) {
      projectRestoreMessage = projectActionRecoveryMessage(error, 'remove');
      projectCatalogRemovalTarget = null;
    } finally {
      projectCatalogRemoving = false;
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

  function projectDisplayName(path: string): string {
    const parts = path.split(/[\\/]/).filter((part) => part.length > 0);
    return parts.at(-1) ?? path;
  }

  function projectAnalysisSummary(state: IndexStateV1): {
    copy: string;
    title: string;
    tone: 'attention' | 'idle' | 'ready' | 'running';
  } {
    const summaries = {
      notStarted: {
        copy: 'A^3 beginnt automatisch, sobald das Projekt bereit ist.',
        title: 'Analyse wird vorbereitet',
        tone: 'idle',
      },
      building: {
        copy: 'A^3 liest deinen Code lokal ein. Du kannst währenddessen weiterarbeiten.',
        title: 'Code wird analysiert',
        tone: 'running',
      },
      published: {
        copy: 'Project Map und Agent können den aktuellen Projektstand verwenden.',
        title: 'Analyse ist bereit',
        tone: 'ready',
      },
      failed: {
        copy: 'Die letzte Analyse konnte nicht abgeschlossen werden. Dein Projekt bleibt nutzbar.',
        title: 'Analyse braucht Aufmerksamkeit',
        tone: 'attention',
      },
      cancelled: {
        copy: 'Die letzte Analyse wurde beendet. Du kannst sie in den Optionen neu erstellen.',
        title: 'Analyse wurde angehalten',
        tone: 'attention',
      },
    } as const;
    return summaries[state];
  }

  function storageSizeLabel(bytes: string | null): string {
    if (bytes === null) return 'Nicht verfügbar';
    const value = BigInt(bytes);
    const kibibyte = BigInt(1024);
    const units = [
      { divisor: kibibyte ** BigInt(4), label: 'TB' },
      { divisor: kibibyte ** BigInt(3), label: 'GB' },
      { divisor: kibibyte ** BigInt(2), label: 'MB' },
      { divisor: kibibyte, label: 'KB' },
    ];
    const unit = units.find(({ divisor }) => value >= divisor);
    if (unit === undefined) {
      return `${new Intl.NumberFormat('de-DE').format(value)} Bytes`;
    }
    return `${new Intl.NumberFormat('de-DE', { maximumFractionDigits: 1 }).format(
      Number(value) / Number(unit.divisor),
    )} ${unit.label}`;
  }

  function rebuildStateLabel(state: RebuildStateV1): string {
    const labels = {
      idle: 'Bereit für einen Neuaufbau',
      queued: 'Die neue Analyse startet gleich',
      running: 'Die Analyse wird neu erstellt',
      succeeded: 'Neuaufbau wurde gestartet',
      failed: 'Die Analyse konnte nicht neu erstellt werden',
      cancelled: 'Der Neuaufbau wurde beendet',
    } as const;
    return labels[state];
  }

  function indexActivityStateLabel(state: IndexActivityStateV1): string {
    const labels: Record<IndexActivityStateV1, string> = {
      idle: 'Noch nicht gestartet',
      queued: 'Startet in Kürze',
      running: 'Wird gerade analysiert',
      cancelling: 'Wird beendet',
      succeeded: 'Aktuell',
      failed: 'Konnte nicht abgeschlossen werden',
      cancelled: 'Wurde angehalten',
    };
    return labels[state];
  }

  function indexActivityIsInProgress(state: IndexActivityStateV1): boolean {
    return state === 'queued' || state === 'running' || state === 'cancelling';
  }

  function indexPhaseLabel(phase: IndexPhaseV1): string {
    const labels: Record<IndexPhaseV1, string> = {
      discover: 'Dateien suchen',
      hash: 'Dateien vorbereiten',
      parse: 'Quellcode lesen',
      link: 'Zusammenhänge erkennen',
      rank: 'Wichtige Bereiche ordnen',
      publish: 'Ergebnisse bereitstellen',
    };
    return labels[phase];
  }

  function countLabel(value: string): string {
    return new Intl.NumberFormat('de-DE').format(BigInt(value));
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

  function diagnosticSeverityLabel(severity: IndexDiagnosticSeverityV1): string {
    const labels: Record<IndexDiagnosticSeverityV1, string> = {
      error: 'Fehler',
      information: 'Hinweis',
      warning: 'Warnung',
    };
    return labels[severity];
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
                <h2 id="project-heading">Deine Projekte</h2>
              </div>
            </div>

            <p class="project-copy">
              Füge einen lokalen Git-Worktree hinzu oder aktiviere ein bereits gespeichertes Projekt
              aus deinem Katalog.
            </p>
            <button
              class="primary-action"
              type="button"
              disabled={projectView.kind === 'opening'}
              onclick={chooseProject}
            >
              {projectView.kind === 'opening' ? 'Ordnerdialog geöffnet …' : 'Projekt hinzufügen'}
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
                    <div class="modal-heading project-dialog-heading">
                      <div>
                        <p class="modal-eyebrow">Projektverwaltung</p>
                        <h3 id="project-dialog-heading">
                          {projectDisplayName(projectStatusView.result.project.worktreeRootDisplay)}
                        </h3>
                        <p class="project-dialog-context">
                          Lokales Projekt · {branchLabel(projectStatusView.result.project.head)}
                        </p>
                      </div>
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
                        onclick={() => (projectDialogView = 'index')}>Code-Analyse</button
                      >
                      <button
                        type="button"
                        aria-pressed={projectDialogView === 'maintenance'}
                        onclick={() => (projectDialogView = 'maintenance')}>Optionen</button
                      >
                    </nav>

                    <div class="project-dialog-content">
                      {#if projectDialogView === 'overview'}
                        {@const analysis = projectAnalysisSummary(
                          projectStatusView.result.index.state,
                        )}
                        <div class="project-overview-page">
                          <section
                            class="project-identity-card"
                            aria-labelledby="project-identity-heading"
                          >
                            <div class="project-folder-mark" aria-hidden="true">
                              <svg viewBox="0 0 24 24">
                                <path d="M3 6.5h7l2 2h9v9H3z"></path>
                              </svg>
                            </div>
                            <div>
                              <p class="section-kicker">Aktives Projekt</p>
                              <h4 id="project-identity-heading">
                                {projectDisplayName(
                                  projectStatusView.result.project.worktreeRootDisplay,
                                )}
                              </h4>
                              <p>
                                Branch
                                <strong>{branchLabel(projectStatusView.result.project.head)}</strong
                                >
                              </p>
                            </div>
                          </section>

                          <section
                            class="project-analysis-summary"
                            data-tone={analysis.tone}
                            aria-labelledby="project-analysis-summary-heading"
                          >
                            <span class="analysis-status-mark" aria-hidden="true"></span>
                            <div>
                              <p class="section-kicker">Code-Analyse</p>
                              <h4 id="project-analysis-summary-heading">{analysis.title}</h4>
                              <p>{analysis.copy}</p>
                            </div>
                            <button type="button" onclick={() => (projectDialogView = 'index')}
                              >Analyse ansehen</button
                            >
                          </section>

                          <details class="project-technical-details">
                            <summary>Technische Details</summary>
                            <dl>
                              <div>
                                <dt>Lokaler A^3-Speicher</dt>
                                <dd>{storageSizeLabel(projectStatusView.result.storageBytes)}</dd>
                              </div>
                              <div>
                                <dt>Worktree-ID</dt>
                                <dd><code>{projectStatusView.result.project.worktreeId}</code></dd>
                              </div>
                              <div>
                                <dt>Letzte Analyse</dt>
                                <dd>
                                  {projectStatusView.result.index.latestSnapshot === null
                                    ? 'Noch keine Analyse vorhanden'
                                    : `Generation ${projectStatusView.result.index.latestSnapshot.generation}`}
                                </dd>
                              </div>
                              {#if projectStatusView.result.index.latestSnapshot !== null}
                                <div>
                                  <dt>Snapshot-ID</dt>
                                  <dd>
                                    <code
                                      >{projectStatusView.result.index.latestSnapshot
                                        .snapshotId}</code
                                    >
                                  </dd>
                                </div>
                              {/if}
                            </dl>
                          </details>
                        </div>
                      {:else if projectDialogView === 'index'}
                        {@const analysis = projectAnalysisSummary(
                          projectStatusView.result.index.state,
                        )}
                        <div class="project-analysis-page">
                          <header class="dialog-page-heading">
                            <p class="section-kicker">Lokale Code-Analyse</p>
                            <h4>Wie gut A^3 dein Projekt kennt</h4>
                            <p>
                              A^3 liest deinen Code lokal, damit Suche, Projektkarte und Agent
                              verlässliche Zusammenhänge finden können.
                            </p>
                          </header>

                          <section
                            class="analysis-progress-card"
                            data-tone={analysis.tone}
                            aria-labelledby="analysis-progress-heading"
                          >
                            <div class="analysis-progress-heading">
                              <span class="analysis-status-mark" aria-hidden="true"></span>
                              <div>
                                <h5 id="analysis-progress-heading">{analysis.title}</h5>
                                {#if indexActivityView.kind === 'active' && indexActivityIsInProgress(indexActivityView.result.activity.state)}
                                  <p>
                                    {indexActivityStateLabel(
                                      indexActivityView.result.activity.state,
                                    )}
                                  </p>
                                {:else}
                                  <p>{analysis.copy}</p>
                                {/if}
                              </div>
                            </div>

                            {#if indexActivityView.kind === 'active' && indexActivityIsInProgress(indexActivityView.result.activity.state) && indexActivityView.result.activity.phase !== null}
                              <p class="analysis-step" role="status" aria-live="polite">
                                {#if indexActivityView.result.activity.completedPhases === indexActivityView.result.activity.totalPhases}
                                  Abgeschlossen: {indexPhaseLabel(
                                    indexActivityView.result.activity.phase,
                                  )}
                                {:else}
                                  Schritt {indexActivityView.result.activity.completedPhases + 1} von
                                  {indexActivityView.result.activity.totalPhases}: {indexPhaseLabel(
                                    indexActivityView.result.activity.phase,
                                  )}
                                {/if}
                              </p>
                              <progress
                                aria-label="Fortschritt der Code-Analyse"
                                max={indexActivityView.result.activity.totalPhases}
                                value={indexActivityView.result.activity.completedPhases}
                              ></progress>
                              {#if (indexActivityView.result.activity.state === 'queued' || indexActivityView.result.activity.state === 'running' || indexActivityView.result.activity.state === 'cancelling') && projectStatusView.result.index.publishedSnapshotId !== null}
                                <p class="analysis-supporting-copy">
                                  Die zuletzt fertige Analyse bleibt währenddessen nutzbar.
                                </p>
                              {/if}
                            {/if}
                          </section>

                          <section
                            class="analysis-results"
                            aria-labelledby="analysis-results-heading"
                          >
                            <h5 id="analysis-results-heading">Erkannter Projektstand</h5>
                            {#if indexOverviewView.kind === 'loading'}
                              <p class="project-status" role="status" aria-live="polite">
                                Analyseergebnisse werden geladen …
                              </p>
                            {:else if indexOverviewView.kind === 'noPublishedIndex'}
                              <p class="project-status">
                                Noch keine fertige Analyse vorhanden. Sobald sie bereit ist,
                                erscheinen hier Dateien, Symbole und Abdeckung.
                              </p>
                            {:else if indexOverviewView.kind === 'published'}
                              <dl class="analysis-metrics">
                                <div>
                                  <dt>Erfasste Dateien</dt>
                                  <dd>
                                    {countLabel(indexOverviewView.result.overview.counts.fileCount)}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Gefundene Symbole</dt>
                                  <dd>
                                    {countLabel(
                                      indexOverviewView.result.overview.counts.symbolCount,
                                    )}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Analyseabdeckung</dt>
                                  <dd>
                                    {percentageLabel(
                                      indexOverviewView.result.overview.coverageBasisPoints,
                                    )}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Hinweise</dt>
                                  <dd>
                                    {countLabel(
                                      indexOverviewView.result.overview.counts.diagnosticCount,
                                    )}
                                  </dd>
                                </div>
                              </dl>
                              <p class="analysis-coverage-note">
                                {countLabel(
                                  indexOverviewView.result.overview.counts.parsedFileCount,
                                )} von
                                {countLabel(indexOverviewView.result.overview.counts.fileCount)} Dateien
                                hat A^3 vollständig strukturell ausgewertet.
                              </p>

                              {#if indexOverviewView.result.overview.diagnosticFiles.length === 0}
                                <p class="analysis-ready-label">
                                  Keine zusätzlichen Analysehinweise gefunden.
                                </p>
                              {:else}
                                <details class="analysis-issues">
                                  <summary>
                                    Hinweise zu {countLabel(
                                      indexOverviewView.result.overview.counts.diagnosticFileCount,
                                    )} Dateien
                                  </summary>
                                  <p>
                                    A^3 konnte Teile dieser Dateien nicht vollständig verstehen. Der
                                    restliche Projektstand bleibt nutzbar.
                                  </p>
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
                                        <ul>
                                          {#each file.diagnostics as diagnostic, diagnosticIndex (diagnosticIndex)}
                                            <li>
                                              <strong
                                                >{diagnosticSeverityLabel(
                                                  diagnostic.severity,
                                                )}:</strong
                                              >
                                              {diagnostic.message}
                                            </li>
                                          {/each}
                                        </ul>
                                        {#if file.diagnosticsTruncated}
                                          <p>Weitere Hinweise zu dieser Datei sind ausgeblendet.</p>
                                        {/if}
                                      </li>
                                    {/each}
                                  </ul>
                                  {#if indexOverviewView.result.overview.diagnosticFilesTruncated}
                                    <p>Weitere betroffene Dateien sind ausgeblendet.</p>
                                  {/if}
                                </details>
                              {/if}

                              <details class="project-technical-details analysis-technical-details">
                                <summary>Technische Analyse-Details</summary>
                                <dl>
                                  <div>
                                    <dt>Snapshot-ID</dt>
                                    <dd>
                                      <code>{indexOverviewView.result.overview.snapshotId}</code>
                                    </dd>
                                  </div>
                                </dl>
                              </details>
                            {:else if indexOverviewView.kind === 'error'}
                              <div class="analysis-load-error" role="alert">
                                <p>Die Analyseergebnisse konnten nicht geladen werden.</p>
                                <button type="button" onclick={() => void loadIndexOverview()}
                                  >Erneut versuchen</button
                                >
                              </div>
                            {/if}
                          </section>
                        </div>
                      {:else}
                        <div class="project-options-page">
                          <header class="dialog-page-heading">
                            <p class="section-kicker">Projektoptionen</p>
                            <h4>Weitere Projektoptionen</h4>
                            <p>
                              Hier kannst du die Code-Analyse erneuern oder das Projekt aus deiner
                              A^3-Projektliste entfernen.
                            </p>
                          </header>

                          <section class="project-option-card" aria-labelledby="rebuild-heading">
                            <div class="option-card-heading">
                              <span class="option-card-mark" aria-hidden="true">↻</span>
                              <div>
                                <h5 id="rebuild-heading">Code-Analyse neu erstellen</h5>
                                <p>
                                  Nutze diese Option, wenn Suche oder Projektkarte veraltet wirken.
                                  Dein Code und deine Projektdaten bleiben unverändert.
                                </p>
                              </div>
                            </div>
                            <p class="option-status" role="status" aria-live="polite">
                              {rebuildStateLabel(projectStatusView.result.rebuildState)}
                            </p>
                            <button
                              type="button"
                              disabled={rebuildView.kind === 'submitting' ||
                                projectStatusView.result.rebuildState === 'queued' ||
                                projectStatusView.result.rebuildState === 'running'}
                              onclick={requestIndexRebuild}
                            >
                              {rebuildView.kind === 'submitting'
                                ? 'Wird vorbereitet …'
                                : 'Analyse neu erstellen'}
                            </button>
                            {#if rebuildView.kind === 'error'}
                              <p class="project-error" role="alert">{rebuildView.message}</p>
                            {/if}
                          </section>

                          {#if removalView.kind === 'confirming' || removalView.kind === 'submitting'}
                            <section
                              class="project-removal-confirmation"
                              aria-labelledby="removal-confirmation-heading"
                            >
                              <p class="section-kicker">Bestätigung</p>
                              <h5 id="removal-confirmation-heading">Projekt aus A^3 entfernen?</h5>
                              <p>
                                <strong
                                  >{projectDisplayName(
                                    projectStatusView.result.project.worktreeRootDisplay,
                                  )}</strong
                                >
                                verschwindet aus deiner Projektliste.
                              </p>
                              <ul class="removal-effects">
                                <li>Der Projektordner und alle Dateien bleiben erhalten.</li>
                                <li>Lokale A^3-Projektdaten werden nicht gelöscht.</li>
                                <li>Nur der Eintrag in der A^3-Projektliste wird entfernt.</li>
                              </ul>
                              <div class="confirmation-actions">
                                <button
                                  type="button"
                                  disabled={removalView.kind === 'submitting'}
                                  onclick={cancelRemoval}>Zurück</button
                                >
                                <button
                                  class="risk-action"
                                  type="button"
                                  disabled={removalView.kind === 'submitting'}
                                  onclick={confirmProjectRemoval}
                                >
                                  {removalView.kind === 'submitting'
                                    ? 'Wird entfernt …'
                                    : 'Aus A^3 entfernen'}
                                </button>
                              </div>
                            </section>
                          {:else}
                            <section
                              class="project-option-card project-danger-zone"
                              aria-labelledby="removal-heading"
                            >
                              <p class="section-kicker">Projektliste</p>
                              <h5 id="removal-heading">Projekt aus A^3 entfernen</h5>
                              <p>
                                Entfernt dieses Projekt aus deiner A^3-Projektliste. Der Ordner und
                                alle Dateien auf deinem Computer bleiben unverändert.
                              </p>
                              <button
                                class="risk-action"
                                type="button"
                                onclick={requestRemovalConfirmation}>Aus A^3 entfernen</button
                              >
                            </section>
                          {/if}

                          {#if removalView.kind === 'error'}
                            <div class="project-removal-error" role="alert">
                              <p>{removalView.message}</p>
                              <p>Dein Projektordner und deine Dateien wurden nicht gelöscht.</p>
                              <button type="button" onclick={requestRemovalConfirmation}
                                >Erneut versuchen</button
                              >
                            </div>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  </dialog>
                {/if}
              </div>
              <div class="map-workspace-view">
                {#if mapWorkspaceComponent !== null}
                  {@const MapWorkspace = mapWorkspaceComponent}
                  <MapWorkspace
                    projectKey={projectStatusView.result.project.worktreeId}
                    indexActivityState={indexActivityView.kind === 'active'
                      ? indexActivityView.result.activity.state
                      : 'idle'}
                    indexRebuilder={projectRebuilder}
                    publicationKey={indexOverviewView.kind === 'published'
                      ? indexOverviewView.result.overview.snapshotId
                      : null}
                    searchLoader={projectMapSearchLoader}
                    {taskLensTasksLoader}
                    {taskLensTaskLoader}
                    {taskLensCompiler}
                    {deepMapStatusLoader}
                    {deepMapStarter}
                    {deepMapPauser}
                    {deepMapResumer}
                    {deepMapCanceller}
                  />
                {:else}
                  <section class="lazy-surface" aria-labelledby="lazy-map-heading">
                    <h2 id="lazy-map-heading">Code Atlas</h2>
                    {#if mapWorkspaceState === 'error'}
                      <p role="alert">
                        Der lokale Map-Workspace-Chunk konnte nicht geladen werden.
                      </p>
                      <button type="button" onclick={loadMapWorkspaceChunk}>Erneut laden</button>
                    {:else}
                      <p role="status">Der Architektur-Atlas wird geladen …</p>
                    {/if}
                  </section>
                {/if}
              </div>
            </div>
          {:else if projectStatusView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der aktive Projektstatus konnte nicht sicher geladen werden.</p>
              <button type="button" onclick={loadProjectStatus}>Status erneut laden</button>
            </div>
          {/if}

          {#if currentWorkspaceArea === 'projects'}
            <section class="project-catalog" aria-labelledby="project-catalog-heading">
              <div class="project-catalog-heading">
                <div>
                  <p class="section-kicker">Projektkatalog</p>
                  <h3 id="project-catalog-heading">Gespeicherte Worktrees</h3>
                  <p>Zuletzt aktiviert zuerst · 25 Projekte pro Seite</p>
                </div>
                {#if projectStatusView.kind === 'active'}
                  <button
                    class="primary-action"
                    type="button"
                    disabled={projectView.kind === 'opening'}
                    onclick={chooseProject}>Projekt hinzufügen</button
                  >
                {/if}
              </div>

              <form
                class="project-catalog-search"
                role="search"
                onsubmit={submitProjectCatalogSearch}
              >
                <label for="project-catalog-search">Projekte durchsuchen</label>
                <div>
                  <input
                    id="project-catalog-search"
                    type="search"
                    maxlength="128"
                    placeholder="Name oder sicherer Root-Anzeigename"
                    bind:value={projectCatalogSearchInput}
                  />
                  <button type="submit">Projekte suchen</button>
                  {#if projectCatalogSearch !== null}
                    <button type="button" onclick={clearProjectCatalogSearch}>Zurücksetzen</button>
                  {/if}
                </div>
              </form>

              {#if projectRestoreMessage !== null}
                <div class="project-catalog-recovery" role="alert">
                  <strong>Projekt konnte nicht automatisch geöffnet werden.</strong>
                  <p>{projectRestoreMessage}</p>
                  <p>
                    Ist der Worktree verschoben, füge seinen neuen Root erneut hinzu. Ist er nicht
                    mehr relevant, entferne nur den Katalogeintrag.
                  </p>
                </div>
              {/if}

              {#if projectCatalogView.kind === 'loading'}
                <p class="project-status" role="status" aria-live="polite">
                  Projektkatalog wird geladen …
                </p>
              {:else if projectCatalogView.kind === 'error'}
                <div class="recent-projects-error" role="alert">
                  <p>Der Projektkatalog konnte nicht sicher gelesen werden.</p>
                  <button
                    type="button"
                    onclick={() => void loadProjectCatalog('initial', null, projectCatalogSearch)}
                    >Erneut laden</button
                  >
                </div>
              {:else if projectCatalogView.page.projects.length === 0}
                <div class="project-catalog-empty">
                  <strong
                    >{projectCatalogSearch === null
                      ? 'Noch keine Projekte gespeichert'
                      : 'Keine Projekte gefunden'}</strong
                  >
                  <p>
                    {projectCatalogSearch === null
                      ? 'Füge deinen ersten lokalen Git-Worktree über den nativen Ordnerdialog hinzu.'
                      : 'Passe den Suchbegriff an oder setze die Suche zurück.'}
                  </p>
                </div>
              {:else}
                <ul class="project-catalog-list" aria-label="Gespeicherte Projekte">
                  {#each projectCatalogView.page.projects as entry (entry.project.worktreeId)}
                    {@const isActive =
                      projectStatusView.kind === 'active' &&
                      projectStatusView.result.project.worktreeId === entry.project.worktreeId}
                    <li class:catalog-project-active={isActive}>
                      <div class="catalog-project-icon" aria-hidden="true">
                        <svg viewBox="0 0 24 24"><path d="M3 6.5h7l2 2h9v9H3z"></path></svg>
                      </div>
                      <div class="catalog-project-main">
                        <div class="catalog-project-title">
                          <strong>{projectDisplayName(entry.project.worktreeRootDisplay)}</strong>
                          {#if isActive}<span>Aktiv</span>{/if}
                        </div>
                        <p title={entry.project.worktreeRootDisplay}>
                          {entry.project.worktreeRootDisplay}
                        </p>
                        <small>Branch: {branchLabel(entry.project.head)}</small>
                      </div>
                      <div class="catalog-project-actions">
                        <button
                          type="button"
                          disabled={isActive || projectCatalogActivatingId !== null}
                          onclick={() => activateProjectFromCatalog(entry)}
                          >{projectCatalogActivatingId === entry.project.worktreeId
                            ? 'Wird aktiviert …'
                            : isActive
                              ? 'Aktiv'
                              : 'Aktivieren'}</button
                        >
                        <button
                          class="risk-action"
                          type="button"
                          disabled={projectCatalogActivatingId !== null}
                          onclick={() => requestCatalogRemoval(entry)}>Nur aus A^3 entfernen</button
                        >
                      </div>
                    </li>
                  {/each}
                </ul>

                <nav class="project-catalog-pagination" aria-label="Projektkatalog Seiten">
                  <button
                    type="button"
                    disabled={projectCatalogView.page.previousCursor === null}
                    onclick={() => navigateProjectCatalog('previous')}>Zurück</button
                  >
                  <span>Seite {projectCatalogPageNumber}</span>
                  <button
                    type="button"
                    disabled={projectCatalogView.page.nextCursor === null}
                    onclick={() => navigateProjectCatalog('next')}>Weiter</button
                  >
                </nav>
              {/if}
            </section>

            {#if projectCatalogRemovalTarget !== null}
              <dialog
                class="modal-dialog removal-confirmation"
                aria-labelledby="catalog-removal-heading"
                aria-describedby="catalog-removal-copy"
                use:presentModal
                oncancel={(event) => {
                  event.preventDefault();
                  cancelCatalogRemoval();
                }}
              >
                <div class="modal-heading">
                  <h3 id="catalog-removal-heading">Projekt nur aus A^3 entfernen?</h3>
                  <button
                    type="button"
                    aria-label="Dialog schließen"
                    disabled={projectCatalogRemoving}
                    onclick={cancelCatalogRemoval}>×</button
                  >
                </div>
                <p id="catalog-removal-copy">
                  <strong
                    >{projectDisplayName(
                      projectCatalogRemovalTarget.project.worktreeRootDisplay,
                    )}</strong
                  >
                  wird aus dem Projektkatalog entfernt. Repository, Worktree, Quellcode und private
                  <code>knowledge.db</code>-Daten bleiben erhalten.
                </p>
                <div class="modal-actions">
                  <button
                    type="button"
                    disabled={projectCatalogRemoving}
                    onclick={cancelCatalogRemoval}>Abbrechen</button
                  >
                  <button
                    class="risk-action"
                    type="button"
                    disabled={projectCatalogRemoving}
                    onclick={confirmCatalogRemoval}
                    >{projectCatalogRemoving ? 'Wird entfernt …' : 'Entfernen bestätigen'}</button
                  >
                </div>
              </dialog>
            {/if}
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
              inspectionLoader={agentInspectionLoader}
              inspectionLogLoader={agentInspectionLogLoader}
              sessionController={agentSessionController}
              sessionLoader={agentSessionLoader}
              sessionsLoader={agentSessionsLoader}
              messageSubmitter={agentMessageSubmitter}
              onRunStatusChange={updateGlobalRunStatus}
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

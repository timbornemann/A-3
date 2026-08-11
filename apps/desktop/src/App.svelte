<script lang="ts">
  import { onMount } from 'svelte';
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
    queryModuleCardFreshness,
    type ModuleCardFreshnessReasonV1,
    type ModuleCardFreshnessResponseV1,
  } from './lib/module-card-freshness';
  import {
    queryModuleDependencyGraph,
    type ModuleDependencyEdgeEvidenceV1,
    type ModuleDependencyGraphQueryV1,
    type ModuleDependencyGraphResponseV1,
    type ModuleDependencyNodeV1,
    type ModuleDependencyRelationV1,
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
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
  import { removeProject, type RemoveProjectResponseV1 } from './lib/project-removal';
  import {
    queryProjectStatus,
    type IndexStateV1,
    type ProjectStatusResponseV1,
    type RebuildStateV1,
  } from './lib/project-status';
  import {
    listRecentProjects,
    type RecentProjectSummaryV1,
    type RecentProjectsResponseV1,
  } from './lib/recent-projects';
  import {
    queryRepositoryTree,
    type RepositoryTreeEntryV1,
    type RepositoryTreeQueryV1,
    type RepositoryTreeResponseV1,
  } from './lib/repository-tree';

  interface Props {
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
    projectRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    projectRemover?: () => Promise<RemoveProjectResponseV1>;
    projectStatusLoader?: () => Promise<ProjectStatusResponseV1>;
    recentProjectsLoader?: () => Promise<RecentProjectsResponseV1>;
    repositoryTreeLoader?: (query: RepositoryTreeQueryV1) => Promise<RepositoryTreeResponseV1>;
  }

  type ViewState =
    { kind: 'loading' } | { health: HealthResponseV1; kind: 'ready' } | { kind: 'error' };
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
  type RepositoryTreeView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'available';
        result: Extract<RepositoryTreeResponseV1['result'], { status: 'available' }>;
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
  type RecentProjectsView =
    { kind: 'loading' } | { kind: 'ready'; projects: RecentProjectSummaryV1[] } | { kind: 'error' };

  let {
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
    moduleDependencyGraphLoader = queryModuleDependencyGraph,
    moduleRuntimeMapLoader = queryModuleRuntimeMap,
    moduleRuntimeFlowLoader = queryModuleRuntimeFlow,
    moduleTreeLoader = queryModuleTree,
    projectOpener = openProject,
    projectRebuilder = rebuildProjectIndex,
    projectRemover = removeProject,
    projectStatusLoader = queryProjectStatus,
    recentProjectsLoader = listRecentProjects,
    repositoryTreeLoader = queryRepositoryTree,
  }: Props = $props();
  let healthView = $state<ViewState>({ kind: 'loading' });
  let projectView = $state<ProjectView>({ kind: 'idle' });
  let projectStatusView = $state<ProjectStatusView>({ kind: 'loading' });
  let indexActivityView = $state<IndexActivityView>({ kind: 'loading' });
  let indexOverviewView = $state<IndexOverviewView>({ kind: 'loading' });
  let moduleCardFreshnessView = $state<ModuleCardFreshnessView>({ kind: 'loading' });
  let moduleCardDetailView = $state<ModuleCardDetailView>({ kind: 'idle' });
  let moduleCardSelection = $state<{ moduleId: string; name: string } | null>(null);
  let moduleTreeView = $state<ModuleTreeView>({ kind: 'loading' });
  let moduleTreeBreadcrumbs = $state<ModuleTreeBreadcrumb[]>([]);
  let moduleTreeLoadingMore = $state(false);
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
  let recentProjectsView = $state<RecentProjectsView>({ kind: 'loading' });
  let indexActivityObserved = false;
  let moduleRuntimeMapRequestSequence = 0;
  let moduleRuntimeFlowRequestSequence = 0;
  let moduleCardDetailRequestSequence = 0;

  function resetModuleCardDetail(kind: 'idle' | 'noProject'): void {
    moduleCardDetailRequestSequence += 1;
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

  async function loadHealth(): Promise<void> {
    healthView = { kind: 'loading' };

    try {
      healthView = { health: await healthLoader(), kind: 'ready' };
    } catch {
      healthView = { kind: 'error' };
    }
  }

  onMount(() => {
    void loadHealth();
    void loadProjectStatus();
    void loadRecentProjects();
    void loadIndexActivity();
    void loadIndexOverview();
    void loadModuleCardFreshness();
    void loadModuleTreeRoot();
    void loadRepositoryTreeRoot();
    void loadDeepMap();
    const activityTimer = window.setInterval(() => {
      void loadIndexActivity();
      void loadDeepMap();
    }, 500);
    return () => window.clearInterval(activityTimer);
  });

  async function loadIndexActivity(): Promise<void> {
    try {
      const previousSucceeded =
        indexActivityView.kind === 'active' &&
        indexActivityView.result.activity.state === 'succeeded';
      const response = await indexActivityLoader();
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
        void loadRepositoryTreeRoot();
      } else if (response.result.status === 'noProject') {
        indexOverviewView = { kind: 'noProject' };
        moduleCardFreshnessView = { kind: 'noProject' };
        moduleTreeView = { kind: 'noProject' };
        moduleTreeBreadcrumbs = [];
        moduleDependencyGraphView = { kind: 'noProject' };
        moduleDependencySelection = null;
        selectedDependencyEvidence = null;
        resetModuleCardDetail('noProject');
        resetModuleRuntime('noProject');
        repositoryTreeView = { kind: 'noProject' };
        repositoryTreeBreadcrumbs = [];
      }
      indexActivityObserved = true;
    } catch {
      indexActivityView = { kind: 'error' };
    }
  }

  async function loadIndexOverview(): Promise<void> {
    indexOverviewView = { kind: 'loading' };
    try {
      const response = await indexOverviewLoader();
      if (response.result.status === 'published') {
        indexOverviewView = { kind: 'published', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        indexOverviewView = { kind: 'noPublishedIndex' };
      } else {
        indexOverviewView = { kind: 'noProject' };
      }
    } catch {
      indexOverviewView = { kind: 'error' };
    }
  }

  async function loadModuleCardFreshness(): Promise<void> {
    moduleCardFreshnessView = { kind: 'loading' };
    try {
      const response = await moduleCardFreshnessLoader();
      if (response.result.status === 'available') {
        moduleCardFreshnessView = { kind: 'available', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleCardFreshnessView = { kind: 'noPublishedIndex' };
      } else {
        moduleCardFreshnessView = { kind: 'noProject' };
      }
    } catch {
      moduleCardFreshnessView = { kind: 'error' };
    }
  }

  async function loadModuleTree(
    parentModuleId: string | null,
    afterModuleId: string | null = null,
  ): Promise<void> {
    const append = afterModuleId !== null;
    if (append) {
      moduleTreeLoadingMore = true;
    } else {
      moduleTreeView = { kind: 'loading' };
    }
    try {
      const response = await moduleTreeLoader({ afterModuleId, limit: 50, parentModuleId });
      if (response.result.status === 'available') {
        if (append && moduleTreeView.kind === 'available') {
          const current = moduleTreeView.result.page;
          const next = response.result.page;
          const compatible =
            current.indexRunId === next.indexRunId &&
            current.snapshotId === next.snapshotId &&
            current.parentModuleId === next.parentModuleId &&
            current.nextAfterModuleId === afterModuleId &&
            !next.entries.some((entry) =>
              current.entries.some((currentEntry) => currentEntry.moduleId === entry.moduleId),
            );
          if (!compatible) {
            moduleTreeView = { kind: 'error' };
            return;
          }
          moduleTreeView = {
            kind: 'available',
            result: {
              page: { ...next, entries: [...current.entries, ...next.entries] },
              status: 'available',
            },
          };
        } else {
          moduleTreeView = { kind: 'available', result: response.result };
        }
      } else if (response.result.status === 'projectionUnavailable') {
        moduleTreeView = { kind: 'projectionUnavailable' };
      } else if (response.result.status === 'noPublishedIndex') {
        moduleTreeView = { kind: 'noPublishedIndex' };
      } else {
        moduleTreeView = { kind: 'noProject' };
        moduleTreeBreadcrumbs = [];
      }
    } catch {
      moduleTreeView = { kind: 'error' };
    } finally {
      moduleTreeLoadingMore = false;
    }
  }

  async function loadModuleCardDetail(moduleId: string, name: string): Promise<void> {
    const requestSequence = ++moduleCardDetailRequestSequence;
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
    await loadModuleCardDetail(entry.moduleId, entry.name);
  }

  async function reloadModuleCardDetail(): Promise<void> {
    if (moduleCardSelection === null) return;
    await loadModuleCardDetail(moduleCardSelection.moduleId, moduleCardSelection.name);
  }

  async function loadModuleTreeRoot(): Promise<void> {
    moduleTreeBreadcrumbs = [];
    await loadModuleTree(null);
  }

  async function openModule(entry: ModuleTreeEntryV1): Promise<void> {
    if (entry.childState !== 'hasChildren') return;
    moduleTreeBreadcrumbs = [
      ...moduleTreeBreadcrumbs,
      { moduleId: entry.moduleId, name: entry.name },
    ];
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
    await loadModuleTree(target.moduleId);
  }

  async function loadMoreModules(): Promise<void> {
    if (moduleTreeView.kind !== 'available') return;
    const page = moduleTreeView.result.page;
    if (page.nextAfterModuleId === null) return;
    await loadModuleTree(page.parentModuleId, page.nextAfterModuleId);
  }

  async function loadModuleDependencyGraph(moduleId: string, name: string): Promise<void> {
    moduleDependencySelection = { moduleId, name };
    moduleDependencyGraphView = { kind: 'loading' };
    selectedDependencyEvidence = null;
    try {
      const response = await moduleDependencyGraphLoader({
        centerModuleId: moduleId,
        nodeLimit: 50,
      });
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
      moduleDependencyGraphView = { kind: 'error' };
    }
  }

  async function openModuleDependencies(entry: ModuleTreeEntryV1): Promise<void> {
    await loadModuleDependencyGraph(entry.moduleId, entry.name);
  }

  async function reloadModuleDependencies(): Promise<void> {
    if (moduleDependencySelection === null) return;
    await loadModuleDependencyGraph(
      moduleDependencySelection.moduleId,
      moduleDependencySelection.name,
    );
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
  ): Promise<void> {
    const append = afterNameHex !== null;
    if (append) {
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
      if (response.result.status === 'available') {
        if (append && repositoryTreeView.kind === 'available') {
          const current = repositoryTreeView.result.page;
          const next = response.result.page;
          const compatible =
            current.indexRunId === next.indexRunId &&
            current.snapshotId === next.snapshotId &&
            current.directoryPathHex === next.directoryPathHex &&
            current.nextAfterNameHex === afterNameHex &&
            !next.entries.some((entry) =>
              current.entries.some((currentEntry) => currentEntry.pathHex === entry.pathHex),
            );
          if (!compatible) {
            repositoryTreeView = { kind: 'error' };
            return;
          }
          repositoryTreeView = {
            kind: 'available',
            result: {
              page: { ...next, entries: [...current.entries, ...next.entries] },
              status: 'available',
            },
          };
        } else {
          repositoryTreeView = { kind: 'available', result: response.result };
        }
      } else if (response.result.status === 'noPublishedIndex') {
        repositoryTreeView = { kind: 'noPublishedIndex' };
      } else {
        repositoryTreeView = { kind: 'noProject' };
        repositoryTreeBreadcrumbs = [];
      }
    } catch {
      repositoryTreeView = { kind: 'error' };
    } finally {
      repositoryTreeLoadingMore = false;
    }
  }

  async function loadRepositoryTreeRoot(): Promise<void> {
    repositoryTreeBreadcrumbs = [];
    await loadRepositoryTree(null);
  }

  async function openRepositoryDirectory(entry: RepositoryTreeEntryV1): Promise<void> {
    if (entry.kind !== 'directory') return;
    repositoryTreeBreadcrumbs = [
      ...repositoryTreeBreadcrumbs,
      { name: entry.name, pathHex: entry.pathHex },
    ];
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
    await loadRepositoryTree(target.pathHex);
  }

  async function loadMoreRepositoryEntries(): Promise<void> {
    if (
      repositoryTreeView.kind !== 'available' ||
      repositoryTreeView.result.page.nextAfterNameHex === null
    )
      return;
    await loadRepositoryTree(
      repositoryTreeView.result.page.directoryPathHex,
      repositoryTreeView.result.page.nextAfterNameHex,
    );
  }

  async function loadDeepMap(): Promise<void> {
    try {
      const response = await deepMapStatusLoader();
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
    } catch {
      deepMapView = { kind: 'error' };
    }
  }

  async function loadProjectStatus(): Promise<void> {
    projectStatusView = { kind: 'loading' };
    try {
      const response = await projectStatusLoader();
      projectStatusView =
        response.result.status === 'active'
          ? { kind: 'active', result: response.result }
          : { kind: 'noProject' };
      if (response.result.status === 'noProject') {
        indexOverviewView = { kind: 'noProject' };
        moduleCardFreshnessView = { kind: 'noProject' };
        moduleTreeView = { kind: 'noProject' };
        moduleTreeBreadcrumbs = [];
        moduleDependencyGraphView = { kind: 'noProject' };
        moduleDependencySelection = null;
        selectedDependencyEvidence = null;
        resetModuleCardDetail('noProject');
        resetModuleRuntime('noProject');
        repositoryTreeView = { kind: 'noProject' };
        repositoryTreeBreadcrumbs = [];
      }
    } catch {
      projectStatusView = { kind: 'error' };
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

  async function loadRecentProjects(): Promise<void> {
    recentProjectsView = { kind: 'loading' };
    try {
      const response = await recentProjectsLoader();
      recentProjectsView = { kind: 'ready', projects: response.projects };
    } catch {
      recentProjectsView = { kind: 'error' };
    }
  }

  async function chooseProject(): Promise<void> {
    projectView = { kind: 'opening' };
    try {
      const response = await projectOpener();
      if (response.result.status === 'opened') {
        projectView = { kind: 'opened' };
        removalView = { kind: 'idle' };
        indexActivityObserved = false;
        moduleDependencyGraphView = { kind: 'idle' };
        moduleDependencySelection = null;
        selectedDependencyEvidence = null;
        resetModuleCardDetail('idle');
        resetModuleRuntime('idle');
        await loadProjectStatus();
        await loadIndexActivity();
        await loadIndexOverview();
        await loadModuleCardFreshness();
        await loadModuleTreeRoot();
        await loadRepositoryTreeRoot();
        await loadDeepMap();
        await loadRecentProjects();
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

  function cancelRemoval(): void {
    removalView = { kind: 'idle' };
  }

  async function confirmProjectRemoval(): Promise<void> {
    removalView = { kind: 'submitting' };
    try {
      await projectRemover();
      removalView = { kind: 'removed' };
      projectView = { kind: 'idle' };
      projectStatusView = { kind: 'noProject' };
      indexActivityView = { kind: 'noProject' };
      indexOverviewView = { kind: 'noProject' };
      moduleCardFreshnessView = { kind: 'noProject' };
      moduleTreeView = { kind: 'noProject' };
      moduleTreeBreadcrumbs = [];
      moduleDependencyGraphView = { kind: 'noProject' };
      moduleDependencySelection = null;
      selectedDependencyEvidence = null;
      resetModuleCardDetail('noProject');
      resetModuleRuntime('noProject');
      repositoryTreeView = { kind: 'noProject' };
      repositoryTreeBreadcrumbs = [];
      deepMapView = { kind: 'noProject' };
      deepMapActionView = { kind: 'idle' };
      deepMapBudgetProfile = null;
      indexActivityObserved = false;
      await loadRecentProjects();
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

  function moduleDependencyRelationLabel(relation: ModuleDependencyRelationV1): string {
    const labels: Record<ModuleDependencyRelationV1, string> = {
      builds: 'baut',
      calls: 'ruft auf',
      configures: 'konfiguriert',
      documents: 'dokumentiert',
      exports: 'exportiert nach',
      extends: 'erweitert',
      implements: 'implementiert',
      imports: 'importiert',
      reads: 'liest',
      tests: 'testet',
      writes: 'schreibt',
    };
    return labels[relation];
  }

  function moduleDependencyNodeName(moduleId: string): string {
    if (moduleDependencyGraphView.kind !== 'available') return moduleId.slice(0, 12);
    return (
      moduleDependencyGraphView.result.graph.nodes.find((node) => node.moduleId === moduleId)
        ?.name ?? moduleId.slice(0, 12)
    );
  }

  function moduleDependencyNodeKind(node: ModuleDependencyNodeV1): string {
    return node.kind === 'manifestBoundary' ? 'Manifest-Grenze' : 'Pfad-Grenze';
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

  function coverageLabel(value: number | null): string {
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

<main class="app-shell">
  <header class="product-header">
    <p class="eyebrow">Local-first coding agent</p>
    <h1>A^3</h1>
    <p class="subtitle">Autonomous Agent Assistant</p>
  </header>

  <section class="health-card" aria-labelledby="health-heading">
    <div class="section-heading">
      <div>
        <p class="section-kicker">Systemstatus</p>
        <h2 id="health-heading">Desktop Core</h2>
      </div>
      <span
        class:pending={healthView.kind === 'loading'}
        class:failed={healthView.kind === 'error'}
        class="status-dot"
        aria-hidden="true"
      ></span>
    </div>

    {#if healthView.kind === 'loading'}
      <p class="status-message" role="status" aria-live="polite">Core wird geprüft …</p>
    {:else if healthView.kind === 'ready'}
      <p class="ready-label" role="status" aria-live="polite">Bereit</p>
      <dl class="health-grid">
        <div>
          <dt>App-Version</dt>
          <dd>{healthView.health.applicationVersion}</dd>
        </div>
        <div>
          <dt>Protokoll</dt>
          <dd>V{healthView.health.protocolVersion}</dd>
        </div>
        <div>
          <dt>Plattform</dt>
          <dd>{healthView.health.platform}</dd>
        </div>
      </dl>
    {:else}
      <div class="error-state" role="alert">
        <p>Die Health-Abfrage ist fehlgeschlagen.</p>
        <button type="button" onclick={loadHealth}>Erneut prüfen</button>
      </div>
    {/if}
  </section>

  <section class="project-card" aria-labelledby="project-heading">
    <div class="section-heading">
      <div>
        <p class="section-kicker">Lokaler Workspace</p>
        <h2 id="project-heading">Projekt öffnen</h2>
      </div>
    </div>

    <p class="project-copy">
      Wähle den Root eines Git-Worktrees. A^3 erhält nur Zugriff auf diesen ausdrücklich gewählten
      Ordner.
    </p>
    <button
      class="primary-action"
      type="button"
      disabled={projectView.kind === 'opening'}
      onclick={chooseProject}
    >
      {projectView.kind === 'opening'
        ? 'Ordnerdialog geöffnet …'
        : projectView.kind === 'opened'
          ? 'Anderen Worktree auswählen'
          : 'Projektordner auswählen'}
    </button>

    {#if projectView.kind === 'cancelled'}
      <p class="project-status" role="status" aria-live="polite">Auswahl abgebrochen.</p>
    {:else if projectView.kind === 'opened'}
      <p class="ready-label" role="status" aria-live="polite">Worktree sicher geöffnet</p>
    {:else if projectView.kind === 'error'}
      <p class="project-error" role="alert">{projectView.message}</p>
    {/if}

    {#if projectStatusView.kind === 'loading'}
      <p class="project-status" role="status" aria-live="polite">Projektstatus wird geladen …</p>
    {:else if projectStatusView.kind === 'active'}
      <div class="project-result" aria-labelledby="active-project-heading">
        <h3 id="active-project-heading">Aktiver Worktree</h3>
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
              <dd>{indexActivityStateLabel(indexActivityView.result.activity.state)}</dd>
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
                Generation {projectStatusView.result.index.latestSnapshot.generation}<br />
                {projectStatusView.result.index.latestSnapshot.snapshotId}
              </dd>
            {/if}
          </div>
        </dl>
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
                Der zuletzt veröffentlichte Snapshot bleibt während dieses Laufs vollständig lesbar.
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
              Noch kein vollständiger Snapshot veröffentlicht. Ein laufender Aufbau bleibt davon
              getrennt.
            </p>
          {:else if indexOverviewView.kind === 'published'}
            <p class="index-snapshot">
              Snapshot <code>{indexOverviewView.result.overview.snapshotId}</code>
            </p>
            <dl class="index-metrics">
              <div>
                <dt>Dateien</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.fileCount)}</dd>
              </div>
              <div>
                <dt>Symbole</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.symbolCount)}</dd>
              </div>
              <div>
                <dt>Diagnostics</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.diagnosticCount)}</dd>
              </div>
              <div>
                <dt>Parse Coverage</dt>
                <dd>{coverageLabel(indexOverviewView.result.overview.coverageBasisPoints)}</dd>
              </div>
            </dl>
            <p class="index-coverage-note">
              {countLabel(indexOverviewView.result.overview.counts.parsedFileCount)} von
              {countLabel(indexOverviewView.result.overview.counts.fileCount)} Dateien strukturell geparst.
            </p>
            {#if indexOverviewView.result.overview.diagnosticFiles.length === 0}
              <p class="ready-label">Keine Parser-Diagnostics im veröffentlichten Snapshot.</p>
            {:else}
              <div class="file-diagnostics" aria-labelledby="file-diagnostics-heading">
                <h5 id="file-diagnostics-heading">Indexfehler pro Datei</h5>
                <ul>
                  {#each indexOverviewView.result.overview.diagnosticFiles as file, fileIndex (fileIndex)}
                    <li>
                      <div class="diagnostic-file-heading">
                        <code>{file.pathDisplay}{file.pathDisplayTruncated ? '…' : ''}</code>
                        <span>{indexLanguageLabel(file.language)}</span>
                      </div>
                      <p>
                        {countLabel(file.diagnosticCount)} Diagnostics · Coverage
                        {coverageLabel(file.coverageBasisPoints)}
                      </p>
                      <ul>
                        {#each file.diagnostics as diagnostic, diagnosticIndex (diagnosticIndex)}
                          <li>
                            <strong>{diagnosticSeverityLabel(diagnostic.severity)}:</strong>
                            {diagnosticCodeLabel(diagnostic.code)} · {diagnostic.message}
                            <span>Bytes {diagnostic.startByte}–{diagnostic.endByte}</span>
                          </li>
                        {/each}
                      </ul>
                      {#if file.diagnosticsTruncated}
                        <p>
                          Weitere Diagnostics dieser Datei sind in dieser begrenzten Ansicht
                          verborgen.
                        </p>
                      {/if}
                    </li>
                  {/each}
                </ul>
                {#if indexOverviewView.result.overview.diagnosticFilesTruncated}
                  <p>
                    Weitere fehlerhafte Dateien sind in dieser auf 64 Dateien begrenzten Ansicht
                    verborgen.
                  </p>
                {/if}
              </div>
            {/if}
          {:else if indexOverviewView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der veröffentlichte Index konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={loadIndexOverview}>Indexübersicht erneut laden</button>
            </div>
          {/if}
        </div>
        <div class="repository-tree-panel" aria-labelledby="repository-tree-heading">
          <div class="repository-tree-heading">
            <div>
              <h4 id="repository-tree-heading">Repository-Baum</h4>
              <p>Direkte Kinder des veröffentlichten Index, progressiv und ohne Vollbaum-Ladung.</p>
            </div>
            <button type="button" onclick={loadRepositoryTreeRoot}>Zum Root</button>
          </div>
          {#if repositoryTreeView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Repository-Baum wird gelesen …
            </p>
          {:else if repositoryTreeView.kind === 'noPublishedIndex'}
            <p class="project-status">
              Noch kein vollständiger Snapshot veröffentlicht; der Repository-Baum bleibt leer.
            </p>
          {:else if repositoryTreeView.kind === 'available'}
            <p class="index-snapshot">
              Indexlauf <code>{repositoryTreeView.result.page.indexRunId}</code>
            </p>
            <nav class="repository-tree-breadcrumbs" aria-label="Repository-Pfad">
              <button type="button" onclick={() => openRepositoryBreadcrumb(-1)}>Repository</button>
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
              <p class="ready-label">Keine weiteren indexierten Einträge in diesem Bereich.</p>
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
            {#if repositoryTreeView.result.page.nextAfterNameHex !== null}
              <button
                class="repository-tree-more"
                type="button"
                disabled={repositoryTreeLoadingMore}
                onclick={loadMoreRepositoryEntries}
              >
                {repositoryTreeLoadingMore ? 'Weitere Einträge werden geladen …' : 'Weitere laden'}
              </button>
            {/if}
          {:else if repositoryTreeView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der Repository-Baum konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={loadRepositoryTreeRoot}>Vom Root neu laden</button>
            </div>
          {/if}
        </div>
        <div class="repository-tree-panel module-tree-panel" aria-labelledby="module-tree-heading">
          <div class="repository-tree-heading">
            <div>
              <h4 id="module-tree-heading">Modulbaum</h4>
              <p>Direkte deterministische Modulgrenzen; Graph-Communities bleiben Zusatzsignale.</p>
            </div>
            <button type="button" onclick={loadModuleTreeRoot}>Zum Root</button>
          </div>
          {#if moduleTreeView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">Modulbaum wird gelesen …</p>
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
              <button type="button" onclick={() => openModuleBreadcrumb(-1)}>Modul-Root</button>
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
                          >{entry.boundaryEvidence.manifestRevision.contentHash.slice(0, 12)}</code
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
                        aria-pressed={moduleDependencySelection?.moduleId === entry.moduleId}
                        onclick={() => openModuleDependencies(entry)}
                      >
                        Abhängigkeiten anzeigen
                      </button>
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
            {#if moduleTreeView.result.page.nextAfterModuleId !== null}
              <button
                class="repository-tree-more"
                type="button"
                disabled={moduleTreeLoadingMore}
                onclick={loadMoreModules}
              >
                {moduleTreeLoadingMore ? 'Weitere Module werden geladen …' : 'Weitere Module laden'}
              </button>
            {/if}
          {:else if moduleTreeView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der Modulbaum konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={loadModuleTreeRoot}>Vom Root neu laden</button>
            </div>
          {/if}
        </div>
        <div class="repository-tree-panel module-card-panel" aria-labelledby="module-card-heading">
          <div class="repository-tree-heading">
            <div>
              <h4 id="module-card-heading">Module Card</h4>
              <p>Verifizierte Felder mit getrennt sichtbarer Klassifikation und Aktualität.</p>
            </div>
            <button
              type="button"
              disabled={moduleCardSelection === null || moduleCardDetailView.kind === 'loading'}
              onclick={reloadModuleCardDetail}
            >
              Aktualisieren
            </button>
          </div>
          {#if moduleCardDetailView.kind === 'idle' || moduleCardDetailView.kind === 'noProject'}
            <p class="project-status">
              Wähle im Modulbaum „Module Card“, um die neueste dauerhaft verifizierte Karte bewusst
              zu laden.
            </p>
          {:else if moduleCardDetailView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Module Card für {moduleCardSelection?.name ?? 'das Modul'} wird atomar gelesen …
            </p>
          {:else if moduleCardDetailView.kind === 'noPublishedIndex'}
            <p class="project-status">Noch kein veröffentlichter Index.</p>
          {:else if moduleCardDetailView.kind === 'projectionUnavailable'}
            <p class="project-status">
              Der historische Index enthält noch keine deterministische Modulprojektion. Ein Rebuild
              erzeugt sie mit dem aktuellen Schema.
            </p>
          {:else if moduleCardDetailView.kind === 'moduleUnavailable'}
            <div class="recent-projects-error" role="status">
              <p>Das ausgewählte primäre Modul gehört nicht mehr zur aktuellen Publikation.</p>
              <button type="button" onclick={loadModuleTreeRoot}>Modulbaum neu laden</button>
            </div>
          {:else if moduleCardDetailView.kind === 'cardUnavailable'}
            <p class="project-status">
              Für {moduleCardSelection?.name ?? 'dieses Modul'} wurde noch keine verifizierte Module Card
              veröffentlicht.
            </p>
          {:else if moduleCardDetailView.kind === 'available'}
            {@const card = moduleCardDetailView.result.detail}
            <div
              class:module-card-lifecycle-current={card.lifecycle.status === 'current'}
              class:module-card-lifecycle-stale={card.lifecycle.status === 'stale'}
              class:module-card-lifecycle-review={card.lifecycle.status === 'needsReview'}
              class="module-card-lifecycle"
              role={card.lifecycle.status === 'current' ? 'note' : 'alert'}
            >
              <strong>
                {card.lifecycle.status === 'current'
                  ? 'Current'
                  : card.lifecycle.status === 'stale'
                    ? 'Stale — keine aktuelle Faktenquelle'
                    : 'NeedsReview — keine aktuelle Faktenquelle'}
              </strong>
              {#if card.lifecycle.status !== 'current'}
                <span>{moduleCardFreshnessReasonLabel(card.lifecycle.reason)}</span>
              {/if}
            </div>
            <p class="module-card-safety-note" role="note">
              Claim-Typ und Aktualität sind unabhängig. Ein als „Fact“ klassifizierter, aber „Stale“
              oder „NeedsReview“ markierter Wert wird nicht als aktuelles Faktum verwendet.
            </p>
            <dl class="module-card-envelope">
              <div>
                <dt>Ausgewähltes Modul</dt>
                <dd>{moduleCardSelection?.name ?? card.moduleId.slice(0, 12)}</dd>
              </div>
              <div>
                <dt>Card Confidence</dt>
                <dd>{coverageLabel(card.confidenceBasisPoints)}</dd>
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
                <section class="module-card-field" aria-labelledby={`module-card-${field.kind}`}>
                  <div class="module-card-field-heading">
                    <h5 id={`module-card-${field.kind}`}>{moduleCardFieldLabel(field.kind)}</h5>
                    <span>{field.evidenceIds.length} Feld-Evidence</span>
                  </div>
                  <ol>
                    {#each field.values as item (item.claim.claimId)}
                      <li
                        class:module-card-value-current={item.claim.state === 'current'}
                        class:module-card-value-stale={item.claim.state === 'stale'}
                        class:module-card-value-review={item.claim.state === 'needsReview'}
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
                          <span>{coverageLabel(item.claim.confidenceBasisPoints)}</span>
                        </div>
                        <p>{item.value}</p>
                        <details class="module-card-evidence-identities">
                          <summary>{item.claim.evidenceIds.length} Claim-Evidence-ID(s)</summary>
                          {#if item.claim.evidenceIds.length === 0}
                            <p>Architecture-Hypothese ohne deterministische Evidence.</p>
                          {:else}
                            <ul>
                              {#each item.claim.evidenceIds as evidenceId (evidenceId)}
                                <li><code>{evidenceId}</code></li>
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
          {:else if moduleCardDetailView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Die Module Card konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={reloadModuleCardDetail}>Erneut laden</button>
            </div>
          {/if}
        </div>
        <div
          class="repository-tree-panel module-runtime-panel"
          aria-labelledby="module-runtime-heading"
        >
          <div class="repository-tree-heading">
            <div>
              <h4 id="module-runtime-heading">Entry Points &amp; Tests</h4>
              <p>Aktuelle strukturelle Wurzeln und bewusst geladene, begrenzte Evidence-Pfade.</p>
            </div>
            <button
              type="button"
              disabled={moduleRuntimeSelection === null || moduleRuntimeMapView.kind === 'loading'}
              onclick={reloadModuleRuntime}
            >
              Aktualisieren
            </button>
          </div>
          {#if moduleRuntimeMapView.kind === 'idle' || moduleRuntimeMapView.kind === 'noProject'}
            <p class="project-status">
              Wähle im Modulbaum „Entry Points &amp; Tests“, um die aktuellen Root-Symbole zu laden.
            </p>
          {:else if moduleRuntimeMapView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Entry Points und Tests für {moduleRuntimeSelection?.name ?? 'das Modul'} werden gelesen
              …
            </p>
          {:else if moduleRuntimeMapView.kind === 'noPublishedIndex'}
            <p class="project-status">Noch kein vollständiger Snapshot veröffentlicht.</p>
          {:else if moduleRuntimeMapView.kind === 'projectionUnavailable'}
            <p class="project-status">
              Der historische Index enthält noch keine deterministische V8-Modulprojektion. Ein
              Rebuild erzeugt sie mit dem aktuellen Schema.
            </p>
          {:else if moduleRuntimeMapView.kind === 'moduleUnavailable'}
            <p class="project-status" role="alert">
              Das gewählte Primärmodul ist im aktuellen Index nicht mehr vorhanden.
            </p>
          {:else if moduleRuntimeMapView.kind === 'stale'}
            <div class="recent-projects-error" role="alert">
              <p>
                Die sichtbare Root-Liste ist nicht mehr verifizierbar. Alte Roots und Evidence
                bleiben ausgeblendet, bis der aktuelle Index erneut gelesen wurde.
              </p>
              <button type="button" onclick={reloadModuleRuntime}>Roots neu laden</button>
            </div>
          {:else if moduleRuntimeMapView.kind === 'available'}
            {@const runtimeMap = moduleRuntimeMapView.result.map}
            <p class="index-snapshot">
              Indexlauf <code>{runtimeMap.indexRunId}</code>
            </p>
            <div class="runtime-observation-note" role="note">
              <strong>Strukturelle Beobachtung.</strong> Entry Points, Tests und Beziehungen stammen aus
              deterministischen Adaptern. Sie belegen Quellstruktur, nicht eine tatsächlich ausgeführte
              Laufzeitspur.
            </div>
            <div class="runtime-root-columns">
              <section aria-labelledby="runtime-entrypoints-heading">
                <div class="runtime-root-heading">
                  <h5 id="runtime-entrypoints-heading">Entry Points</h5>
                  <span>{countLabel(runtimeMap.entrypoints.storedCount)} gespeichert</span>
                </div>
                {#if runtimeMap.entrypoints.roots.length === 0}
                  <p class="project-status">Keine strukturellen Entry Points beobachtet.</p>
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
                          <span>Rang {root.rank} · {pathDisplayFromHex(root.symbol.pathHex)}</span>
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
                    Die Modulbildung hat weitere, niedriger gerankte Entry Points hinter ihrer
                    festen 256-Root-Grenze ausgelassen.
                  </p>
                {/if}
              </section>
              <section aria-labelledby="runtime-tests-heading">
                <div class="runtime-root-heading">
                  <h5 id="runtime-tests-heading">Tests</h5>
                  <span>{countLabel(runtimeMap.tests.storedCount)} gespeichert</span>
                </div>
                {#if runtimeMap.tests.roots.length === 0}
                  <p class="project-status">Keine strukturellen Testdefinitionen beobachtet.</p>
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
                          <span>Rang {root.rank} · {pathDisplayFromHex(root.symbol.pathHex)}</span>
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
                    Die Modulbildung hat weitere, niedriger gerankte Tests hinter ihrer festen
                    256-Root-Grenze ausgelassen.
                  </p>
                {/if}
              </section>
            </div>

            <section class="runtime-flow" aria-labelledby="runtime-flow-heading">
              <h5 id="runtime-flow-heading">Expliziter Evidence-Pfad</h5>
              {#if moduleRuntimeFlowView.kind === 'idle'}
                <p class="project-status">
                  Wähle einen Root: Entry Points folgen höchstens zwei „Calls“-Kanten, Tests genau
                  einer direkten „Tests“-Kante.
                </p>
              {:else if moduleRuntimeFlowView.kind === 'loading'}
                <p class="project-status" role="status" aria-live="polite">
                  Evidence-Pfad für {moduleRuntimeFlowView.rootName} wird gelesen …
                </p>
              {:else if moduleRuntimeFlowView.kind === 'publicationChanged'}
                <div class="recent-projects-error" role="alert">
                  <p>
                    Seit der Root-Auswahl wurde ein anderer Index veröffentlicht. Die alte Evidence
                    wird nicht mit dem neuen Snapshot gemischt.
                  </p>
                  <button type="button" onclick={reloadModuleRuntime}>Roots neu laden</button>
                </div>
              {:else if moduleRuntimeFlowView.kind === 'rootUnavailable'}
                <p class="project-status" role="alert">
                  Das Symbol ist kein aktueller Root dieser Rolle mehr. Lade die Root-Liste neu.
                </p>
              {:else if moduleRuntimeFlowView.kind === 'moduleUnavailable'}
                <p class="project-status" role="alert">Das Primärmodul ist nicht mehr aktuell.</p>
              {:else if moduleRuntimeFlowView.kind === 'projectionUnavailable'}
                <p class="project-status">Die erforderliche Graphprojektion ist nicht verfügbar.</p>
              {:else if moduleRuntimeFlowView.kind === 'noPublishedIndex' || moduleRuntimeFlowView.kind === 'noProject'}
                <p class="project-status">Kein aktueller veröffentlichter Index verfügbar.</p>
              {:else if moduleRuntimeFlowView.kind === 'available'}
                {@const flow = moduleRuntimeFlowView.result.flow}
                {#if flow.hits.length === 0}
                  <p class="ready-label">Keine Ziele für das feste Relationspreset beobachtet.</p>
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
                        <ol aria-label={`Evidence-Pfad zu ${moduleRuntimeTargetLabel(hit.target)}`}>
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
                    Weitere Ziele liegen hinter der festen Ergebnis- oder Kanteninspektionsgrenze.
                  </p>
                {/if}
              {:else if moduleRuntimeFlowView.kind === 'error'}
                <p class="project-error" role="alert">
                  Der Evidence-Pfad konnte nicht sicher gelesen werden.
                </p>
              {/if}
            </section>

            {#if selectedModuleRuntimeEvidence !== null}
              <aside class="dependency-evidence" aria-labelledby="runtime-evidence-heading">
                <div>
                  <h5 id="runtime-evidence-heading">
                    {selectedModuleRuntimeEvidence.kind === 'edge'
                      ? 'Graph-Kanten-Evidence'
                      : selectedModuleRuntimeEvidence.kind === 'symbol'
                        ? 'Symbol-Evidence'
                        : 'Datei-Evidence'}
                  </h5>
                  <button type="button" onclick={() => (selectedModuleRuntimeEvidence = null)}>
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
                        Bytes {selectedEdge.range.startByte}–{selectedEdge.range.endByte} · Zeile
                        {selectedEdge.range.start.row + 1}
                      </dd>
                    </div>
                    <div>
                      <dt>Confidence</dt>
                      <dd>{coverageLabel(selectedEdge.confidenceBasisPoints)}</dd>
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
                        <code>{pathDisplayFromHex(selectedModuleRuntimeEvidence.pathHex)}</code> ·
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
        <div
          class="repository-tree-panel module-dependency-panel"
          aria-labelledby="module-dependency-heading"
        >
          <div class="repository-tree-heading">
            <div>
              <h4 id="module-dependency-heading">Modulabhängigkeiten</h4>
              <p>
                Direkte, belegte Beziehungen eines Primärmoduls; große Nachbarschaften bleiben
                sichtbar begrenzt.
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
              Wähle im Modulbaum „Abhängigkeiten anzeigen“, um einen direkten Ausschnitt zu laden.
            </p>
          {:else if moduleDependencyGraphView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Abhängigkeiten für {moduleDependencySelection?.name ?? 'das Modul'} werden gelesen …
            </p>
          {:else if moduleDependencyGraphView.kind === 'noPublishedIndex'}
            <p class="project-status">Noch kein vollständiger Snapshot veröffentlicht.</p>
          {:else if moduleDependencyGraphView.kind === 'projectionUnavailable'}
            <p class="project-status">
              Der historische Index enthält noch keine deterministische Modulprojektion. Ein Rebuild
              erzeugt sie mit dem aktuellen Schema.
            </p>
          {:else if moduleDependencyGraphView.kind === 'centerUnavailable'}
            <p class="project-status" role="alert">
              Das gewählte Primärmodul ist im aktuellen veröffentlichten Index nicht mehr vorhanden.
            </p>
          {:else if moduleDependencyGraphView.kind === 'available'}
            {@const graph = moduleDependencyGraphView.result.graph}
            {@const centerNode = graph.nodes.find((node) => node.moduleId === graph.centerModuleId)}
            <p class="index-snapshot">
              Indexlauf <code>{graph.indexRunId}</code>
            </p>
            <dl class="module-tree-summary dependency-summary">
              <div>
                <dt>Beobachtete Nachbarn</dt>
                <dd>{countLabel(graph.observedNeighborCount)}{graph.nodesTruncated ? '+' : ''}</dd>
              </div>
              <div>
                <dt>Relationsgruppen</dt>
                <dd>{countLabel(graph.observedEdgeGroupCount)}{graph.edgesTruncated ? '+' : ''}</dd>
              </div>
              <div>
                <dt>Inspizierte Kanten</dt>
                <dd>
                  {countLabel(graph.inspectedEdgeCount)}{graph.sourceEdgesTruncated ? '+' : ''}
                </dd>
              </div>
              <div>
                <dt>Nicht zugeordnet</dt>
                <dd>{countLabel(graph.unmappedEdgeCount)}</dd>
              </div>
            </dl>
            {#if graph.sourceEdgesTruncated || graph.nodesTruncated || graph.edgesTruncated || graph.unmappedEdgeCount !== '0'}
              <div class="dependency-boundary-note" role="note">
                <strong>Begrenzter Ausschnitt.</strong>
                {#if graph.sourceEdgesTruncated}
                  Weitere Graphkanten liegen hinter der 4.096-Kanten-Grenze.
                {/if}
                {#if graph.nodesTruncated}
                  Weitere beobachtete Module sind nicht gerendert.
                {/if}
                {#if graph.edgesTruncated}
                  Weitere Relationsgruppen der sichtbaren Module sind ausgeblendet.
                {/if}
                {#if graph.unmappedEdgeCount !== '0'}
                  {countLabel(graph.unmappedEdgeCount)} inspizierte Kanten besitzen keinen eindeutig zuordenbaren
                  Modulendpunkt.
                {/if}
              </div>
            {/if}
            <div class="module-dependency-graph" aria-label="Begrenzter Modulabhängigkeitsgraph">
              {#if centerNode !== undefined}
                <div class="dependency-center-node">
                  <span>Zentrum</span>
                  <strong>{centerNode.name}{centerNode.nameTruncated ? '…' : ''}</strong>
                  <small>{moduleDependencyNodeKind(centerNode)}</small>
                </div>
              {/if}
              {#if graph.edges.length === 0}
                <p class="ready-label">
                  Keine zugeordneten direkten Modulabhängigkeiten beobachtet.
                </p>
              {:else}
                <ol class="dependency-edge-list">
                  {#each graph.edges as edge (edge.sourceModuleId + edge.targetModuleId + edge.relation)}
                    <li>
                      <div class="dependency-relation">
                        <strong>{moduleDependencyNodeName(edge.sourceModuleId)}</strong>
                        <span>{moduleDependencyRelationLabel(edge.relation)}</span>
                        <strong>{moduleDependencyNodeName(edge.targetModuleId)}</strong>
                      </div>
                      <span>{countLabel(edge.observedEvidenceCount)} beobachtete Belege</span>
                      <button
                        type="button"
                        aria-label={`Evidence für ${moduleDependencyNodeName(edge.sourceModuleId)} ${moduleDependencyRelationLabel(edge.relation)} ${moduleDependencyNodeName(edge.targetModuleId)} anzeigen`}
                        aria-pressed={selectedDependencyEvidence?.evidenceId ===
                          edge.representativeEvidence.evidenceId}
                        onclick={() => (selectedDependencyEvidence = edge.representativeEvidence)}
                      >
                        Evidence anzeigen
                      </button>
                    </li>
                  {/each}
                </ol>
              {/if}
              <ul class="dependency-node-list" aria-label="Gerenderte Module">
                {#each graph.nodes as node (node.moduleId)}
                  <li class:dependency-node-center={node.moduleId === graph.centerModuleId}>
                    <strong>{node.name}{node.nameTruncated ? '…' : ''}</strong>
                    <span>{moduleDependencyNodeKind(node)}</span>
                    {#if node.representativeEvidence !== null}
                      <code>{node.representativeEvidence.evidenceId.slice(0, 12)}</code>
                    {:else}
                      <span>Kein struktureller Repräsentant</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </div>
            {#if selectedDependencyEvidence !== null}
              <aside class="dependency-evidence" aria-labelledby="dependency-evidence-heading">
                <div>
                  <h5 id="dependency-evidence-heading">Repräsentative Graph-Evidence</h5>
                  <button type="button" onclick={() => (selectedDependencyEvidence = null)}>
                    Schließen
                  </button>
                </div>
                <dl>
                  <div>
                    <dt>Evidence-ID</dt>
                    <dd><code>{selectedDependencyEvidence.evidenceId}</code></dd>
                  </div>
                  <div>
                    <dt>Aktuelle Revision</dt>
                    <dd>
                      <code>{pathDisplayFromHex(selectedDependencyEvidence.pathHex)}</code>
                      · {selectedDependencyEvidence.contentHash.slice(0, 12)}
                    </dd>
                  </div>
                  <div>
                    <dt>Bereich</dt>
                    <dd>
                      Bytes {selectedDependencyEvidence.range.startByte}–{selectedDependencyEvidence
                        .range.endByte}
                      · Zeile {selectedDependencyEvidence.range.start.row + 1}
                    </dd>
                  </div>
                  <div>
                    <dt>Confidence</dt>
                    <dd>{coverageLabel(selectedDependencyEvidence.confidenceBasisPoints)}</dd>
                  </div>
                </dl>
              </aside>
            {/if}
          {:else if moduleDependencyGraphView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der Modulabhängigkeitsgraph konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={reloadModuleDependencies}>Erneut laden</button>
            </div>
          {/if}
        </div>
        <div
          class="index-overview module-card-freshness"
          aria-labelledby="module-card-freshness-heading"
        >
          <div class="module-card-freshness-heading">
            <div>
              <h4 id="module-card-freshness-heading">Module-Card-Aktualität</h4>
              <p>Autoritative Lebenszyklen der jeweils neuesten Karte pro Modul.</p>
            </div>
            <button type="button" onclick={loadModuleCardFreshness}>Aktualisieren</button>
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
                  {countLabel(moduleCardFreshnessView.result.freshness.counts.publishedCount)}
                </dd>
              </div>
              <div>
                <dt>Stale</dt>
                <dd>{countLabel(moduleCardFreshnessView.result.freshness.counts.staleCount)}</dd>
              </div>
              <div>
                <dt>NeedsReview</dt>
                <dd>
                  {countLabel(moduleCardFreshnessView.result.freshness.counts.needsReviewCount)}
                </dd>
              </div>
              <div>
                <dt>Gesamt</dt>
                <dd>{countLabel(moduleCardFreshnessView.result.freshness.counts.totalCount)}</dd>
              </div>
            </dl>
            {#if moduleCardFreshnessView.result.freshness.reasons.length === 0}
              <p class="ready-label">Alle bekannten Module Cards sind aktuell.</p>
            {:else}
              <ul class="module-card-freshness-reasons">
                {#each moduleCardFreshnessView.result.freshness.reasons as reason (reason.status + reason.reason)}
                  <li>
                    <strong>{reason.status === 'stale' ? 'Stale' : 'NeedsReview'}:</strong>
                    {moduleCardFreshnessReasonLabel(reason.reason)} · {countLabel(reason.count)}
                  </li>
                {/each}
              </ul>
            {/if}
          {:else if moduleCardFreshnessView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Die Module-Card-Aktualität konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={loadModuleCardFreshness}>Erneut laden</button>
            </div>
          {/if}
        </div>
        <div class="deep-map-panel" aria-labelledby="deep-map-heading">
          <div class="deep-map-heading">
            <div>
              <h4 id="deep-map-heading">Deep Map</h4>
              <p>
                Startet niemals automatisch. Modell und harte Budgets werden vor jeder neuen
                Exploration sichtbar festgelegt.
              </p>
            </div>
            <button type="button" onclick={loadDeepMap}>Status aktualisieren</button>
          </div>
          {#if deepMapView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Deep-Map-Status wird geladen …
            </p>
          {:else if deepMapView.kind === 'unavailable'}
            <div class="deep-map-unavailable" role="status">
              <strong>Keine Modellarbeit aktiv</strong>
              <p>
                Es ist noch kein live verifiziertes lokales Mapping-Modell konfiguriert. Fast Index
                und veröffentlichte Daten bleiben ohne Modell vollständig nutzbar.
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
                  {countLabel(String(deepMapView.result.configuration.model.contextTokens))} Tokens
                </dd>
              </div>
              <div>
                <dt>Outputlimit je Antwort</dt>
                <dd>
                  {countLabel(String(deepMapView.result.configuration.model.outputTokens))} Tokens
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
                Laufbudget: {countLabel(String(deepMapView.result.activity.budget.tokenLimit))}
                Tokens · {countLabel(String(deepMapView.result.activity.budget.timeLimitMillis))} ms ·
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
              <button type="button" onclick={loadDeepMap}>Erneut laden</button>
            </div>
          {/if}
        </div>
        <div class="project-maintenance" aria-labelledby="rebuild-heading">
          <h4 id="rebuild-heading">Index neu aufbauen</h4>
          <p>
            Entfernt ausschließlich regenerierbare Indexprojektionen. Quellcode, Snapshots,
            Aufgaben, Entscheidungen und User-Evidence bleiben erhalten.
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
            <button type="button" onclick={refreshProjectDetails}>Status aktualisieren</button>
          </div>
          {#if rebuildView.kind === 'error'}
            <p class="project-error" role="alert">{rebuildView.message}</p>
          {/if}
        </div>
        <div class="project-maintenance project-removal" aria-labelledby="removal-heading">
          <h4 id="removal-heading">Worktree aus A^3 entfernen</h4>
          <p>
            Entfernt nur diesen Eintrag aus der A^3-Projektliste. Repository-Dateien werden nie
            gelöscht. Private A^3-Daten bleiben erhalten und stehen beim sicheren Wiederöffnen
            erneut bereit.
          </p>
          {#if removalView.kind === 'confirming'}
            <div class="removal-confirmation" role="group" aria-labelledby="removal-confirmation">
              <p id="removal-confirmation">
                Wirklich nur aus der Projektliste entfernen? Der lokale Worktree bleibt vollständig
                bestehen.
              </p>
              <div class="project-actions">
                <button class="risk-action" type="button" onclick={confirmProjectRemoval}
                  >Entfernen bestätigen</button
                >
                <button type="button" onclick={cancelRemoval}>Abbrechen</button>
              </div>
            </div>
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

    <div class="recent-projects" aria-labelledby="recent-projects-heading">
      <h3 id="recent-projects-heading">Zuletzt verwendet</h3>
      {#if recentProjectsView.kind === 'loading'}
        <p class="project-status" role="status" aria-live="polite">Projektliste wird geladen …</p>
      {:else if recentProjectsView.kind === 'error'}
        <div class="recent-projects-error" role="alert">
          <p>Die lokale Projektliste konnte nicht geladen werden.</p>
          <button type="button" onclick={loadRecentProjects}>Erneut laden</button>
        </div>
      {:else if recentProjectsView.projects.length === 0}
        <p class="project-status">Noch keine Projekte gespeichert.</p>
      {:else}
        <ol class="recent-project-list">
          {#each recentProjectsView.projects as recent (recent.project.worktreeId)}
            <li>
              <span>{recent.project.worktreeRootDisplay}</span>
              <span>{branchLabel(recent.project.head)}</span>
              <code>{recent.project.worktreeId}</code>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  </section>

  <footer>
    <span>Offline by default</span>
    <span aria-hidden="true">·</span>
    <span>Typed IPC</span>
    <span aria-hidden="true">·</span>
    <span>Local core</span>
  </footer>
</main>

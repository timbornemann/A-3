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
    queryModuleCardFreshness,
    type ModuleCardFreshnessReasonV1,
    type ModuleCardFreshnessResponseV1,
  } from './lib/module-card-freshness';
  import {
    queryModuleTree,
    type ModuleTreeEntryV1,
    type ModuleTreeQueryV1,
    type ModuleTreeResponseV1,
  } from './lib/module-tree';
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
  let moduleTreeView = $state<ModuleTreeView>({ kind: 'loading' });
  let moduleTreeBreadcrumbs = $state<ModuleTreeBreadcrumb[]>([]);
  let moduleTreeLoadingMore = $state(false);
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
        void loadRepositoryTreeRoot();
      } else if (response.result.status === 'noProject') {
        indexOverviewView = { kind: 'noProject' };
        moduleCardFreshnessView = { kind: 'noProject' };
        moduleTreeView = { kind: 'noProject' };
        moduleTreeBreadcrumbs = [];
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

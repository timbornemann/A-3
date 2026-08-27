import { mount } from 'svelte';
import '../styles.css';
import MapWorkspace from '../lib/MapWorkspace.svelte';
import type { DeepMapStatusResponseV1 } from '../lib/deep-map';
import type { ProjectMapSceneResponseV1 } from '../lib/project-map-scene';

const MODULE_COUNT = 64;
const ROUTE_COUNT = 128;
const EVENT_COUNT = 32;
const DOM_NODE_LIMIT = 1_500;
const UI_BLOCK_BUDGET_MS = 100;

interface ProfileResult {
  budgetMs: number;
  domNodeCount: number;
  domNodeLimit: number;
  feedCommitMs: number;
  feedEvents: number;
  longTaskCount: number;
  maxLongTaskMs: number;
  moduleCount: number;
  mountMs: number;
  panMs: number;
  routeCount: number;
  selectionMs: number;
  status: 'pass' | 'fail';
  userAgent: string;
  zoomMs: number;
}

const stableId = (value: number): string => value.toString(16).padStart(64, '0');
const indexRunId = stableId(10_001);
const snapshotId = stableId(10_002);

const modules = Array.from({ length: MODULE_COUNT }, (_, index) => ({
  cardBinding: null,
  cardCoverageBasisPoints: index % 4 === 3 ? null : 6_000 + index * 50,
  centralSymbolCount: String(index % 7),
  displayName: `module-${String(index + 1).padStart(2, '0')}`,
  entrypointCount: String(index % 5),
  fileCount: String(2 + (index % 17)),
  kind: index % 3 === 0 ? ('manifestBoundary' as const) : ('pathBoundary' as const),
  manifestCount: index % 3 === 0 ? '1' : '0',
  mappingStatus: (['current', 'needsReview', 'stale', 'unmapped'] as const)[index % 4],
  moduleId: stableId(index + 1),
  parentModuleId: null,
  rank: index + 1,
  representativeEvidenceId: index % 4 === 3 ? null : stableId(index + 20_001),
  symbolCount: String(8 + (index % 29)),
  testCount: String(index % 6),
}));

const relations = Array.from({ length: ROUTE_COUNT }, (_, index) => ({
  evidenceId: stableId(index + 30_001),
  observedEvidenceCount: String(1 + (index % 4)),
  relation: (['calls', 'imports', 'tests', 'reads'] as const)[index % 4],
  sourceModuleId: modules[index % MODULE_COUNT].moduleId,
  targetModuleId: modules[(index + 1 + Math.floor(index / MODULE_COUNT)) % MODULE_COUNT].moduleId,
}));

function scene(focusModuleId: string | null): ProjectMapSceneResponseV1 {
  const visibleModules = focusModuleId === null ? modules : modules.slice(0, 32);
  const visibleModuleIds = new Set(visibleModules.map((module) => module.moduleId));
  const visibleRelations =
    focusModuleId === null
      ? relations
      : relations.filter(
          (relation) =>
            visibleModuleIds.has(relation.sourceModuleId) &&
            visibleModuleIds.has(relation.targetModuleId),
        );
  return {
    protocolVersion: 1,
    result: {
      scene: {
        focusModuleId,
        indexRunId,
        inspectedEdgeCount: '512',
        modules: visibleModules,
        modulesTruncated: focusModuleId !== null,
        observedRelationGroupCount: String(ROUTE_COUNT),
        policyVersion: 'v1',
        primaryModuleCount: String(MODULE_COUNT),
        relations: visibleRelations,
        relationsTruncated: focusModuleId !== null,
        snapshotId,
        sourceEdgesTruncated: false,
        unmappedEdgeCount: '17',
      },
      status: 'available',
    },
  };
}

let deepMapPoll = 0;
let feedResolvedAt = 0;
function deepMapStatus(): DeepMapStatusResponseV1 {
  deepMapPoll += 1;
  const visibleEvents = deepMapPoll === 1 ? 16 : EVENT_COUNT;
  if (visibleEvents === EVENT_COUNT) feedResolvedAt = performance.now();
  const events = Array.from({ length: visibleEvents }, (_, index) => ({
    confirmed: true,
    currentModuleId: modules[index % MODULE_COUNT].moduleId,
    phase: 'exploring' as const,
    safeAction: 'inspect' as const,
    sequence: String(index + 1),
    stepPosition: String(index + 1),
    targetKind: 'module' as const,
    totalSteps: String(EVENT_COUNT),
  }));
  return {
    protocolVersion: 1,
    result: {
      activity: {
        budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
        confirmedSteps: String(visibleEvents),
        currentModuleId: modules[visibleEvents - 1].moduleId,
        events,
        failure: null,
        phase: 'exploring',
        progress: { completed: String(visibleEvents), total: String(EVENT_COUNT) },
        publicationSummary: null,
        safeAction: 'inspect',
        state: 'running',
        stepPosition: String(visibleEvents),
        targetKind: 'module',
        totalSteps: String(EVENT_COUNT),
      },
      configuration: {
        defaultBudget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
        maximumBudget: {
          tokenLimit: 1_000_000,
          timeLimitMillis: 86_400_000,
          toolCallLimit: 4_096,
        },
        minimumBudget: { tokenLimit: 1, timeLimitMillis: 1, toolCallLimit: 1 },
        model: {
          contextTokens: 32_000,
          modelId: 'profile-fixture',
          outputTokens: 4_096,
          profileId: stableId(40_001),
          profileVersion: 1,
          providerId: 'local',
        },
      },
      status: 'available',
    },
  };
}

function requiredElement<T extends HTMLElement>(selector: string): T {
  const element = document.querySelector(selector);
  if (!(element instanceof HTMLElement)) throw new Error(`Missing profile element: ${selector}`);
  return element as T;
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

async function mainThreadDuration(action: () => void): Promise<number> {
  const startedAt = performance.now();
  action();
  const duration = performance.now() - startedAt;
  await nextFrame();
  await nextFrame();
  return duration;
}

function rounded(value: number): number {
  return Number(value.toFixed(3));
}

async function waitFor(selector: string, timeoutMs = 3_000): Promise<HTMLElement> {
  const startedAt = performance.now();
  while (performance.now() - startedAt < timeoutMs) {
    const element = document.querySelector(selector);
    if (element instanceof HTMLElement) return element;
    await nextFrame();
  }
  throw new Error(`Profile timed out waiting for ${selector}`);
}

async function runProfile(): Promise<void> {
  const status = requiredElement('#profile-status');
  const resultElement = requiredElement('#profile-result');
  const root = requiredElement('#profile-root');
  const longTasks: number[] = [];
  const observer = PerformanceObserver.supportedEntryTypes.includes('longtask')
    ? new PerformanceObserver((entries) => {
        for (const entry of entries.getEntries()) longTasks.push(entry.duration);
      })
    : null;
  observer?.observe({ entryTypes: ['longtask'] });

  const mountStartedAt = performance.now();
  mount(MapWorkspace, {
    target: root,
    props: {
      cardLoader: async () => ({ protocolVersion: 1, result: { status: 'cardUnavailable' } }),
      deepMapStatusLoader: async () => deepMapStatus(),
      projectKey: stableId(50_001),
      runtimeLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'projectionUnavailable' },
      }),
      sceneLoader: async ({ focusModuleId }) => scene(focusModuleId),
    },
  });
  const mountMs = performance.now() - mountStartedAt;
  await waitFor('.module-region');
  await nextFrame();
  const moduleCount = document.querySelectorAll('.module-region').length;
  const routeCount = document.querySelectorAll('.route').length;

  const selectionMs = await mainThreadDuration(() =>
    requiredElement<HTMLButtonElement>('.module-region').click(),
  );
  const zoomMs = await mainThreadDuration(() =>
    requiredElement<HTMLButtonElement>('button[aria-label="Hineinzoomen"]').click(),
  );
  const atlas = requiredElement('.atlas-viewport');
  const panMs = await mainThreadDuration(() => {
    atlas.scrollLeft = 180;
    atlas.scrollTop = 120;
  });
  const dock = Array.from(document.querySelectorAll('button')).find((button) =>
    button.textContent?.includes('Deep Map'),
  );
  if (!(dock instanceof HTMLButtonElement)) throw new Error('Missing Deep Map dock control');
  dock.click();

  await waitFor('.activity-feed li:nth-child(32)');
  await nextFrame();
  const feedCommitMs = performance.now() - feedResolvedAt;
  observer?.disconnect();

  const domNodeCount = document.querySelectorAll('*').length;
  const feedEvents = document.querySelectorAll('.activity-feed li').length;
  const maxLongTaskMs = Math.max(0, ...longTasks);
  const passes =
    mountMs <= UI_BLOCK_BUDGET_MS &&
    selectionMs <= UI_BLOCK_BUDGET_MS &&
    zoomMs <= UI_BLOCK_BUDGET_MS &&
    panMs <= UI_BLOCK_BUDGET_MS &&
    feedCommitMs <= UI_BLOCK_BUDGET_MS &&
    maxLongTaskMs <= UI_BLOCK_BUDGET_MS &&
    domNodeCount <= DOM_NODE_LIMIT &&
    moduleCount === MODULE_COUNT &&
    routeCount === ROUTE_COUNT &&
    feedEvents === EVENT_COUNT;
  const result: ProfileResult = {
    budgetMs: UI_BLOCK_BUDGET_MS,
    domNodeCount,
    domNodeLimit: DOM_NODE_LIMIT,
    feedCommitMs: rounded(feedCommitMs),
    feedEvents,
    longTaskCount: longTasks.length,
    maxLongTaskMs: rounded(maxLongTaskMs),
    moduleCount,
    mountMs: rounded(mountMs),
    panMs: rounded(panMs),
    routeCount,
    selectionMs: rounded(selectionMs),
    status: passes ? 'pass' : 'fail',
    userAgent: navigator.userAgent,
    zoomMs: rounded(zoomMs),
  };
  status.textContent = passes ? 'Profil bestanden.' : 'Profil fehlgeschlagen.';
  resultElement.textContent = JSON.stringify(result, null, 2);
  document.documentElement.dataset.profileStatus = result.status;
}

void runProfile().catch((error: unknown) => {
  requiredElement('#profile-status').textContent = 'Profil fehlgeschlagen.';
  requiredElement('#profile-result').textContent =
    error instanceof Error ? error.message : 'Unbekannter Profilfehler.';
  document.documentElement.dataset.profileStatus = 'error';
});

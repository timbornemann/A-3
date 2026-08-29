import { mount } from 'svelte';
import '../styles.css';
import MapWorkspace from '../lib/MapWorkspace.svelte';
import type { DeepMapStatusResponseV3 } from '../lib/deep-map';
import type {
  ProjectMapAtlasNodeV1,
  ProjectMapAtlasRelationV1,
  ProjectMapAtlasSceneResponseV1,
  ProjectMapAtlasSceneV1,
  ProjectMapEntityContextResponseV1,
  ProjectMapEntitySelectionV1,
} from '../lib/project-map-atlas';

const MODULE_COUNT = 64;
const FILE_COUNT = 32;
const SYMBOL_COUNT = 48;
const ROUTE_COUNT = 128;
const VISIBLE_ROUTE_COUNT = 24;
const SELECTED_ROUTE_COUNT = 32;
const FLOW_TARGET_COUNT = 31;
const EVENT_COUNT = 32;
const DOM_NODE_LIMIT = 1_500;
const UI_BLOCK_BUDGET_MS = 100;
const PREVIEW_ONLY = new URLSearchParams(window.location.search).has('preview');

interface ProfileResult {
  budgetMs: number;
  domNodeCount: number;
  domNodeLimit: number;
  feedCommitMs: number;
  feedEvents: number;
  fixtureFileCount: number;
  fixtureFlowTargetCount: number;
  fixtureRouteCount: number;
  fixtureSymbolCount: number;
  longTaskCount: number;
  maxLongTaskMs: number;
  moduleCount: number;
  mountMs: number;
  panMs: number;
  routeCount: number;
  selectedRouteCount: number;
  selectionMs: number;
  semanticZoomMs: number;
  status: 'pass' | 'fail';
  userAgent: string;
}

const stableId = (value: number): string => value.toString(16).padStart(64, '0');
const indexRunId = stableId(10_001);
const snapshotId = stableId(10_002);

function moduleNode(index: number): ProjectMapAtlasNodeV1 {
  return {
    claimBadgeCount: index % 5,
    currentRiskCount: String(index % 4),
    detail: `${2 + (index % 17)} Dateien · ${8 + (index % 29)} Symbole`,
    dimmed: false,
    displayName: `module-${String(index + 1).padStart(2, '0')}`,
    evidenceId: null,
    fileCount: String(2 + (index % 17)),
    kind: index % 3 === 0 ? 'manifestModule' : 'pathModule',
    mappingStatus: (['current', 'needsReview', 'stale', 'unmapped'] as const)[index % 4],
    memberCount: '0',
    nodeId: stableId(index + 1),
    parentNodeId: null,
    purpose: index % 4 === 0 ? 'Deterministisch verifizierte Architekturregion.' : null,
    rank: index + 1,
    selection: { kind: 'module', moduleId: stableId(index + 1) },
    symbolCount: String(8 + (index % 29)),
    volume: String(2 + (index % 17)),
  };
}

function fileNode(moduleId: string, index: number): ProjectMapAtlasNodeV1 {
  const evidenceId = stableId(20_001 + index);
  return {
    claimBadgeCount: index % 4 === 0 ? 1 : 0,
    currentRiskCount: '0',
    detail: `entry_${index} · Type${index}`,
    dimmed: false,
    displayName: `src/file_${String(index + 1).padStart(2, '0')}.rs`,
    evidenceId,
    fileCount: '1',
    kind: 'file',
    mappingStatus: null,
    memberCount: '0',
    nodeId: stableId(30_001 + index),
    parentNodeId: moduleId,
    purpose: null,
    rank: index + 1,
    selection: { evidenceId, kind: 'file', moduleId, ordinal: index },
    symbolCount: String(1 + (index % 12)),
    volume: String(1 + (index % 12)),
  };
}

const projectNodes = Array.from({ length: MODULE_COUNT }, (_, index) => moduleNode(index));
const projectRelations: ProjectMapAtlasRelationV1[] = Array.from(
  { length: ROUTE_COUNT },
  (_, index) => {
    const sourceIndex =
      index < MODULE_COUNT - 1
        ? 0
        : index === MODULE_COUNT - 1
          ? MODULE_COUNT - 1
          : (index - MODULE_COUNT) % MODULE_COUNT;
    const targetIndex =
      index < MODULE_COUNT - 1
        ? index + 1
        : index === MODULE_COUNT - 1
          ? 0
          : (sourceIndex + 1) % MODULE_COUNT;
    return {
      claimBadgeCount: index % 7 === 0 ? 1 : 0,
      confidenceBasisPoints: 10_000 - (index % 8) * 250,
      evidence: {
        edgeSequence: String(index),
        evidenceId: stableId(40_001 + index),
        kind: 'relation',
        moduleId: projectNodes[sourceIndex].selection!.moduleId,
      },
      evidenceCount: String(1 + (index % 4)),
      provider: 'treeSitter',
      relation: index < MODULE_COUNT ? 'imports' : 'exports',
      sourceNodeId: projectNodes[sourceIndex].nodeId,
      targetNodeId: projectNodes[targetIndex].nodeId,
      uncertainty: null,
    };
  },
);

function scene(selection: ProjectMapEntitySelectionV1 | null): ProjectMapAtlasSceneV1 {
  if (selection === null) {
    return {
      boundariesTruncated: false,
      boundaryCount: '0',
      breadcrumb: [{ label: 'Projekt', selection: null }],
      indexRunId,
      inspectedEdgeCount: '4096',
      level: 'project',
      nodeCount: String(MODULE_COUNT),
      nodes: projectNodes,
      nodesTruncated: false,
      policyVersion: 1,
      relationCount: String(ROUTE_COUNT),
      relations: projectRelations,
      relationsTruncated: false,
      selection: null,
      snapshotId,
      sourceEdgesTruncated: false,
      unresolvedCount: '0',
    };
  }
  const moduleId = selection.moduleId;
  const nodes = Array.from({ length: FILE_COUNT }, (_, index) => fileNode(moduleId, index));
  return {
    boundariesTruncated: false,
    boundaryCount: '0',
    breadcrumb: [
      { label: 'Projekt', selection: null },
      { label: projectNodes[0].displayName, selection: { kind: 'module', moduleId } },
    ],
    indexRunId,
    inspectedEdgeCount: '512',
    level: 'module',
    nodeCount: String(FILE_COUNT),
    nodes,
    nodesTruncated: false,
    policyVersion: 1,
    relationCount: '0',
    relations: [],
    relationsTruncated: false,
    selection: { kind: 'module', moduleId },
    snapshotId,
    sourceEdgesTruncated: false,
    unresolvedCount: '0',
  };
}

function sceneResponse(
  selection: ProjectMapEntitySelectionV1 | null,
): ProjectMapAtlasSceneResponseV1 {
  return { protocolVersion: 1, result: { scene: scene(selection), status: 'available' } };
}

function contextResponse(node: ProjectMapAtlasNodeV1): ProjectMapEntityContextResponseV1 {
  return {
    protocolVersion: 1,
    result: {
      context: {
        architectureRelationCount: '0',
        architectureRelations: [],
        boundaryCount: '0',
        boundaryNodes: [],
        boundaryRelations: [],
        claims: [],
        documentRelationCount: '0',
        entity: node,
        indexRunId,
        relatedNodes: [],
        relationCounts: [],
        snapshotId,
        sourceEdgesTruncated: false,
      },
      status: 'available',
    },
  };
}

let feedResolvedAt = 0;
function deepMapStatus(): DeepMapStatusResponseV3 {
  return {
    protocolVersion: 1,
    result: {
      lifecycle: {
        detailsIncomplete: false,
        progress: {
          action: 'inspect',
          confirmedSteps: String(EVENT_COUNT),
          phase: 'exploring',
          totalSteps: String(EVENT_COUNT),
        },
        state: 'running',
      },
      model: {
        contextTokens: 32_000,
        modelId: 'profile-fixture',
        outputTokens: 4_096,
        profileId: stableId(50_001),
        profileVersion: 1,
        providerId: 'local',
      },
      status: 'available',
    },
  };
}

const deepMapRunSelection = 'a'.repeat(96);
const deepMapEntries = Array.from({ length: EVENT_COUNT }, (_, index) => ({
  action: 'inspect' as const,
  confirmed: true,
  failure: null,
  occurredAtUnixMillis: String(1_000 + index),
  phase: 'exploring' as const,
  result: 'confirmed' as const,
  selection: (index + 1).toString(16).padStart(48, '0'),
  sequence: String(index + 1),
  state: 'running' as const,
  stepPosition: String(index + 1),
  targetKind: 'module' as const,
  totalSteps: String(EVENT_COUNT),
}));
const deepMapRun = {
  confirmedSteps: String(EVENT_COUNT),
  detailsIncomplete: false,
  failure: null,
  mode: 'standard' as const,
  selection: deepMapRunSelection,
  startedAtUnixMillis: '1000',
  state: 'running' as const,
  totalSteps: String(EVENT_COUNT),
  updatedAtUnixMillis: String(1_000 + EVENT_COUNT),
};

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
async function waitFor(selector: string, timeoutMs = 4_000): Promise<HTMLElement> {
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
      atlasSceneLoader: async (selection) => sceneResponse(selection),
      contextLoader: async (selection) => {
        const node = projectNodes.find(
          (candidate) => candidate.selection?.moduleId === selection.moduleId,
        );
        return contextResponse(node ?? projectNodes[0]);
      },
      deepMapDetailLoader: async (_runSelection, entrySelection) => ({
        durationMillis: '32',
        entry:
          deepMapEntries.find((entry) => entry.selection === entrySelection) ?? deepMapEntries[0],
        indexReference: '123456abcdef',
        modelId: 'profile-fixture',
        nextAction: null,
        planStopReason: 'coveragePlanned',
        profileId: stableId(50_001),
        profileVersion: 1,
        protocolVersion: 1,
        providerId: 'local',
        publicationResult: null,
        run: deepMapRun,
        snapshotReference: 'abcdef123456',
        step: null,
        timeBudgetMillis: '120000',
        tokenBudget: 32_000,
        toolCallBudget: 64,
      }),
      deepMapEntriesLoader: async () => {
        feedResolvedAt = performance.now();
        return { entries: deepMapEntries, nextCursor: null, protocolVersion: 1 };
      },
      deepMapRunsLoader: async () => ({
        nextCursor: null,
        protocolVersion: 1,
        runs: [deepMapRun],
      }),
      deepMapStatusLoader: async () => deepMapStatus(),
      projectKey: stableId(60_001),
    },
  });
  const mountMs = performance.now() - mountStartedAt;
  await waitFor('.atlas-node');
  await nextFrame();
  const moduleCount = document.querySelectorAll('.atlas-node').length;
  const routeCount = document.querySelectorAll('.route').length;
  if (PREVIEW_ONLY) {
    observer?.disconnect();
    status.textContent = 'Atlas-Vorschau bereit.';
    document.documentElement.dataset.profileStatus = 'preview';
    return;
  }

  const selectionMs = await mainThreadDuration(() =>
    requiredElement<HTMLButtonElement>('.atlas-node').click(),
  );
  const selectedRouteCount = document.querySelectorAll('.route').length;
  const semanticZoomMs = await mainThreadDuration(() =>
    requiredElement<HTMLButtonElement>('.inspector .primary').click(),
  );
  await waitFor('.atlas-node[data-kind="file"]');
  const canvas = requiredElement('.canvas-host');
  const panMs = await mainThreadDuration(() => {
    canvas.scrollLeft = 180;
    canvas.scrollTop = 120;
  });
  requiredElement<HTMLButtonElement>('.deep-map-bar .details').click();
  await waitFor('.timeline li:nth-child(32)');
  await nextFrame();
  const feedCommitMs = performance.now() - feedResolvedAt;
  observer?.disconnect();

  const domNodeCount = document.querySelectorAll('*').length;
  const feedEvents = document.querySelectorAll('.timeline li').length;
  const maxLongTaskMs = Math.max(0, ...longTasks);
  const passes =
    mountMs <= UI_BLOCK_BUDGET_MS &&
    selectionMs <= UI_BLOCK_BUDGET_MS &&
    semanticZoomMs <= UI_BLOCK_BUDGET_MS &&
    panMs <= UI_BLOCK_BUDGET_MS &&
    feedCommitMs <= UI_BLOCK_BUDGET_MS &&
    maxLongTaskMs <= UI_BLOCK_BUDGET_MS &&
    domNodeCount <= DOM_NODE_LIMIT &&
    moduleCount === MODULE_COUNT &&
    routeCount === VISIBLE_ROUTE_COUNT &&
    selectedRouteCount === SELECTED_ROUTE_COUNT &&
    feedEvents === EVENT_COUNT;
  const result: ProfileResult = {
    budgetMs: UI_BLOCK_BUDGET_MS,
    domNodeCount,
    domNodeLimit: DOM_NODE_LIMIT,
    feedCommitMs: rounded(feedCommitMs),
    feedEvents,
    fixtureFileCount: FILE_COUNT,
    fixtureFlowTargetCount: FLOW_TARGET_COUNT,
    fixtureRouteCount: ROUTE_COUNT,
    fixtureSymbolCount: SYMBOL_COUNT,
    longTaskCount: longTasks.length,
    maxLongTaskMs: rounded(maxLongTaskMs),
    moduleCount,
    mountMs: rounded(mountMs),
    panMs: rounded(panMs),
    routeCount,
    selectedRouteCount,
    selectionMs: rounded(selectionMs),
    semanticZoomMs: rounded(semanticZoomMs),
    status: passes ? 'pass' : 'fail',
    userAgent: navigator.userAgent,
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

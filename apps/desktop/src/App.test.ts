import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { DeepMapControlResponseV1, DeepMapStatusResponseV1 } from './lib/deep-map';
import type { HealthResponseV1 } from './lib/health';
import type { IndexActivityResponseV1 } from './lib/index-activity';
import type { IndexOverviewResponseV1 } from './lib/index-overview';
import type { ModuleCardDetailResponseV1 } from './lib/module-card-detail';
import type { ModuleCardEvidenceResponseV1 } from './lib/module-card-evidence';
import type { ModuleCardFreshnessResponseV1 } from './lib/module-card-freshness';
import type { ModuleDependencyGraphResponseV1 } from './lib/module-dependency-graph';
import type { ModuleRuntimeFlowResponseV1, ModuleRuntimeMapResponseV1 } from './lib/module-runtime';
import type { ModuleTreeResponseV1 } from './lib/module-tree';
import type { OpenProjectResponseV1, ProjectSummaryV1 } from './lib/project';
import type {
  ProjectActivationResponseV1,
  ProjectCatalogQueryV1,
  ProjectCatalogResponseV1,
} from './lib/project-catalog';
import type { ProjectMapSearchResponseV1 } from './lib/project-map-search';
import type { RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
import type { RemoveProjectResponseV1 } from './lib/project-removal';
import type { ProjectStatusResponseV1 } from './lib/project-status';
import type { RepositoryTreeResponseV1 } from './lib/repository-tree';
import type {
  TaskLensCompileResponseV1,
  TaskLensTaskResponseV1,
  TaskLensTasksResponseV1,
} from './lib/task-lens';

vi.mock('./lib/project-catalog', async (importOriginal) => {
  const original = await importOriginal<typeof import('./lib/project-catalog')>();
  return {
    ...original,
    queryProjectCatalog: vi.fn(async () => ({
      nextCursor: null,
      previousCursor: null,
      projects: [],
      protocolVersion: 1 as const,
    })),
    restoreLastProject: vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'noSavedProject' as const },
    })),
  };
});

const health: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: 1,
  status: 'ready',
};

const projectSummary: ProjectSummaryV1 = {
  head: { kind: 'unborn', reference: 'refs/heads/main' },
  repositoryId: '1'.repeat(64),
  worktreeId: '2'.repeat(64),
  worktreeRootDisplay: 'C:\\worktree',
};

const openedProject: OpenProjectResponseV1 = {
  protocolVersion: 1,
  result: {
    project: projectSummary,
    status: 'opened',
  },
};

const noProjectStatus: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: { status: 'noProject' },
};

const activeProjectResult: Extract<ProjectStatusResponseV1['result'], { status: 'active' }> = {
  index: {
    latestAttemptSnapshotId: '4'.repeat(64),
    latestSnapshot: { generation: '2', snapshotId: '4'.repeat(64) },
    publishedSnapshotId: '4'.repeat(64),
    state: 'published',
  },
  project: projectSummary,
  projectId: '3'.repeat(64),
  rebuildState: 'idle',
  status: 'active',
  storageBytes: '4096',
};

const activeProjectStatus: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: activeProjectResult,
};

const queuedRebuildStatus: ProjectStatusResponseV1 = {
  ...activeProjectStatus,
  result: { ...activeProjectResult, rebuildState: 'queued' },
};

const runningIndexActivity: IndexActivityResponseV1 = {
  protocolVersion: 1,
  result: {
    activity: {
      completedPhases: 3,
      phase: 'link',
      state: 'running',
      totalPhases: 6,
    },
    status: 'active',
  },
};

const publishedIndexOverview: IndexOverviewResponseV1 = {
  protocolVersion: 1,
  result: {
    overview: {
      counts: {
        diagnosticCount: '1',
        diagnosticFileCount: '1',
        fileCount: '2',
        parsedFileCount: '1',
        symbolCount: '3',
      },
      coverageBasisPoints: 8000,
      diagnosticFiles: [
        {
          coverageBasisPoints: 8000,
          diagnosticCount: '1',
          diagnostics: [
            {
              code: 'syntaxError',
              endByte: 10,
              message: 'syntax error',
              severity: 'error',
              startByte: 8,
            },
          ],
          diagnosticsTruncated: false,
          language: 'rust',
          pathDisplay: 'src/lib.rs',
          pathDisplayTruncated: false,
        },
      ],
      diagnosticFilesTruncated: false,
      snapshotId: '4'.repeat(64),
    },
    status: 'published',
  },
};

const moduleCardFreshness: ModuleCardFreshnessResponseV1 = {
  protocolVersion: 1,
  result: {
    freshness: {
      counts: {
        needsReviewCount: '1',
        publishedCount: '7',
        staleCount: '2',
        totalCount: '10',
      },
      indexRunId: '6'.repeat(64),
      reasons: [
        { count: '2', reason: 'evidenceChanged', status: 'stale' },
        { count: '1', reason: 'directDependencyChanged', status: 'needsReview' },
      ],
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const projectMapSearch: ProjectMapSearchResponseV1 = {
  protocolVersion: 1,
  result: {
    search: {
      fusionPolicyVersion: 1,
      hits: [
        {
          finalScore: 52_478,
          priority: 'exact',
          rank: 1,
          sources: [
            {
              channel: 'exact',
              explanation: 'qualifiedNameExact',
              normalizedScoreBasisPoints: 10_000,
            },
            {
              channel: 'lexical',
              explanation: 'symbolName',
              nativeScore: 80_000,
              normalizedScoreBasisPoints: 8_000,
            },
          ],
          target: {
            evidence: {
              contentHash: 'c'.repeat(64),
              declarationRange: {
                end: { column: 11, row: 0 },
                endByte: 11,
                start: { column: 0, row: 0 },
                startByte: 0,
              },
              pathDisplay: 'src/lib.rs',
              pathHex: '7372632f6c69622e7273',
            },
            kind: 'symbol',
            name: 'launch',
            qualifiedName: 'crate::launch',
            signature: 'fn launch()',
            symbolId: 'd'.repeat(64),
            symbolKind: 'function',
          },
        },
      ],
      indexRunId: '6'.repeat(64),
      query: 'launch parser',
      snapshotId: '4'.repeat(64),
      truncated: true,
    },
    status: 'available',
  },
};

const taskLensTaskId = '7'.repeat(64);
const taskLensStepId = '8'.repeat(64);
const taskLensTasks: TaskLensTasksResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    tasks: [{ goalRevision: 3, objective: 'Project Map abschließen', taskId: taskLensTaskId }],
    truncated: false,
  },
};
const taskLensTask: TaskLensTaskResponseV1 = {
  protocolVersion: 1,
  result: {
    ledgerRevision: 5,
    ledgerStoreVersion: '9',
    status: 'available',
    steps: [
      {
        intendedOutcome: 'Suche und Task Lens sicher umschalten',
        status: 'inProgress',
        stepId: taskLensStepId,
      },
    ],
    task: {
      goalRevision: 3,
      objective: 'Project Map abschließen',
      taskId: taskLensTaskId,
    },
  },
};
const taskLensCompilation: TaskLensCompileResponseV1 = {
  protocolVersion: 1,
  result: {
    lens: {
      claims: [
        {
          claimId: 'd'.repeat(64),
          confidenceBasisPoints: 10_000,
          evidence: [
            {
              evidenceId: 'f'.repeat(64),
              kind: 'file',
              revision: {
                contentHash: 'c'.repeat(64),
                declarationRange: null,
                pathDisplay: 'src/lib.rs',
                pathHex: '7372632f6c69622e7273',
              },
            },
          ],
          kind: 'fact',
          moduleId: 'a'.repeat(64),
          polarity: 'affirms',
          predicate: {
            kind: 'path',
            path: { pathDisplay: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
          },
        },
        {
          claimId: 'e'.repeat(64),
          confidenceBasisPoints: 5_000,
          evidence: [],
          kind: 'hypothesis',
          moduleId: 'a'.repeat(64),
          polarity: 'affirms',
          predicate: {
            kind: 'architecturalIntent',
            statement: 'Kann die Orchestrierung besitzen',
          },
        },
      ],
      digest: '9'.repeat(64),
      entries: [
        {
          estimatedTokens: 100,
          position: 1,
          reason: { kind: 'repositoryAnchor' },
          target: {
            entrypointCount: 1,
            fileCount: 4,
            kind: 'repository',
            languageCount: 1,
            modulePolicyVersion: 1,
            packageCount: 1,
            symbolCount: 8,
          },
        },
        {
          estimatedTokens: 50,
          position: 2,
          reason: {
            finalScore: 60_000,
            kind: 'retrieval',
            priority: 'exact',
            rank: 1,
            sources: [
              { channel: 'exact', normalizedScoreBasisPoints: 10_000 },
              { channel: 'semantic', normalizedScoreBasisPoints: 7_000 },
            ],
          },
          target: {
            evidence: {
              contentHash: 'c'.repeat(64),
              declarationRange: null,
              pathDisplay: 'src/lib.rs',
              pathHex: '7372632f6c69622e7273',
            },
            kind: 'file',
          },
        },
      ],
      estimatedTokens: 150,
      excludedStaleClaims: 2,
      fusionPolicyVersion: 1,
      goalRevision: 3,
      goalSeed: 'Project Map abschließen',
      indexRunId: '6'.repeat(64),
      ledgerRevision: 5,
      ledgerStoreVersion: '9',
      policyVersion: 1,
      snapshotId: '4'.repeat(64),
      stepId: taskLensStepId,
      stepSeed: 'Suche und Task Lens sicher umschalten',
      taskId: taskLensTaskId,
      tokenBudget: 8_200,
      truncated: true,
    },
    status: 'available',
  },
};

const staleModuleCard: ModuleCardDetailResponseV1 = {
  protocolVersion: 1,
  result: {
    detail: {
      cardId: 'e'.repeat(64),
      confidenceBasisPoints: 8_000,
      coverage: {
        basisPoints: 833,
        coveredFieldCount: 1,
        must: {
          basisPoints: 1_250,
          coveredFieldCount: 1,
          missingFields: [
            'title',
            'paths',
            'purpose',
            'responsibilities',
            'dependencies',
            'invariants',
            'tests',
          ],
          totalFieldCount: 8,
        },
        should: {
          basisPoints: 0,
          coveredFieldCount: 0,
          missingFields: ['entrypoints', 'dataFlows', 'risks', 'openQuestions'],
          totalFieldCount: 4,
        },
        totalFieldCount: 12,
      },
      currentIndexRunId: '6'.repeat(64),
      currentSnapshotId: '4'.repeat(64),
      fields: [
        {
          evidenceIds: ['f'.repeat(64)],
          kind: 'publicSurface',
          values: [
            {
              claim: {
                claimId: '1'.repeat(64),
                confidenceBasisPoints: 7_000,
                evidenceIds: ['f'.repeat(64)],
                kind: 'fact',
                state: 'stale',
              },
              value: 'exports main',
            },
          ],
        },
      ],
      lifecycle: {
        invalidatedByIndexRunId: '6'.repeat(64),
        reason: 'evidenceChanged',
        status: 'stale',
      },
      mapperProfileVersion: 1,
      moduleId: 'a'.repeat(64),
      schemaVersion: 1,
      sourceIndexRunId: '5'.repeat(64),
      sourceSnapshotId: '3'.repeat(64),
    },
    status: 'available',
  },
};

const staleModuleCardEvidence: ModuleCardEvidenceResponseV1 = {
  protocolVersion: 1,
  result: {
    detail: {
      cardId: 'e'.repeat(64),
      cardLifecycle: {
        invalidatedByIndexRunId: '6'.repeat(64),
        reason: 'evidenceChanged',
        status: 'stale',
      },
      currentIndexRunId: '6'.repeat(64),
      currentSnapshotId: '4'.repeat(64),
      evidenceId: 'f'.repeat(64),
      freshness: 'stale',
      moduleId: 'a'.repeat(64),
      payload: {
        kind: 'file',
        revision: {
          contentHash: '8'.repeat(64),
          pathHex: '7372632f6c69622e7273',
        },
      },
      sourceIndexRunId: '5'.repeat(64),
      sourceSnapshotId: '3'.repeat(64),
    },
    status: 'available',
  },
};

const moduleTreeRoot: ModuleTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      entries: [
        {
          boundaryEvidence: {
            manifestRevision: {
              contentHash: '7'.repeat(64),
              pathHex: '436172676f2e746f6d6c',
            },
            representativeRevision: {
              contentHash: '8'.repeat(64),
              pathHex: '7372632f6c69622e7273',
            },
          },
          centralSymbols: { count: '1', truncated: false },
          childState: 'hasChildren',
          entrypoints: { count: '1', truncated: false },
          fileCount: '1',
          kind: 'manifestBoundary',
          manifestCount: '1',
          moduleId: 'a'.repeat(64),
          name: 'Repository',
          nameTruncated: false,
          rootPathHex: null,
          symbolCount: '1',
          tests: { count: '0', truncated: false },
        },
      ],
      graphCommunityCount: '1',
      indexRunId: '6'.repeat(64),
      nextAfterModuleId: null,
      parentModuleId: null,
      primaryModuleCount: '2',
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const moduleTreeRepository: ModuleTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      entries: [
        {
          boundaryEvidence: {
            manifestRevision: null,
            representativeRevision: {
              contentHash: '9'.repeat(64),
              pathHex: '746f6f6c732f6d61696e2e7273',
            },
          },
          centralSymbols: { count: '1', truncated: true },
          childState: 'leaf',
          entrypoints: { count: '0', truncated: false },
          fileCount: '1',
          kind: 'pathBoundary',
          manifestCount: '0',
          moduleId: 'b'.repeat(64),
          name: 'tools',
          nameTruncated: false,
          rootPathHex: '746f6f6c73',
          symbolCount: '1',
          tests: { count: '0', truncated: false },
        },
      ],
      graphCommunityCount: '1',
      indexRunId: '6'.repeat(64),
      nextAfterModuleId: null,
      parentModuleId: 'a'.repeat(64),
      primaryModuleCount: '2',
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const moduleDependencyGraph: ModuleDependencyGraphResponseV1 = {
  protocolVersion: 1,
  result: {
    graph: {
      centerModuleId: 'a'.repeat(64),
      edges: [
        {
          observedEvidenceCount: '2',
          relation: 'builds',
          representativeEvidence: {
            confidenceBasisPoints: 10_000,
            contentHash: '8'.repeat(64),
            evidenceId: 'c'.repeat(64),
            pathHex: '7372632f6c69622e7273',
            provider: 'treeSitter',
            range: {
              end: { column: 8, row: 1 },
              endByte: 16,
              start: { column: 0, row: 1 },
              startByte: 8,
            },
            resolution: 'adapterFile',
            source: { kind: 'symbol', symbolId: 'd'.repeat(64) },
            target: { kind: 'file', pathHex: '746f6f6c732f6d61696e2e7273' },
          },
          sourceModuleId: 'a'.repeat(64),
          targetModuleId: 'b'.repeat(64),
        },
      ],
      edgesTruncated: false,
      indexRunId: '6'.repeat(64),
      inspectedEdgeCount: '3',
      nodes: [
        {
          kind: 'manifestBoundary',
          moduleId: 'a'.repeat(64),
          name: 'Repository',
          nameTruncated: false,
          representativeEvidence: {
            contentHash: '8'.repeat(64),
            evidenceId: 'e'.repeat(64),
            pathHex: '7372632f6c69622e7273',
          },
          rootPathHex: null,
        },
        {
          kind: 'pathBoundary',
          moduleId: 'b'.repeat(64),
          name: 'tools',
          nameTruncated: false,
          representativeEvidence: {
            contentHash: '9'.repeat(64),
            evidenceId: 'f'.repeat(64),
            pathHex: '746f6f6c732f6d61696e2e7273',
          },
          rootPathHex: '746f6f6c73',
        },
      ],
      nodesTruncated: false,
      observedEdgeGroupCount: '1',
      observedNeighborCount: '1',
      snapshotId: '4'.repeat(64),
      sourceEdgesTruncated: false,
      unmappedEdgeCount: '1',
    },
    status: 'available',
  },
};

const moduleRuntimeMap: ModuleRuntimeMapResponseV1 = {
  protocolVersion: 1,
  result: {
    map: {
      entrypoints: {
        projectionTruncated: false,
        roots: [
          {
            kind: 'entrypoint',
            rank: 1,
            symbol: {
              contentHash: '8'.repeat(64),
              evidenceId: '1'.repeat(64),
              name: 'main',
              pathHex: '7372632f6c69622e7273',
              selectionRange: {
                end: { column: 4, row: 0 },
                endByte: 4,
                start: { column: 0, row: 0 },
                startByte: 0,
              },
              symbolId: 'd'.repeat(64),
              symbolKind: 'function',
            },
          },
        ],
        storedCount: '1',
        visibleTruncated: false,
      },
      indexRunId: '6'.repeat(64),
      moduleId: 'a'.repeat(64),
      snapshotId: '4'.repeat(64),
      tests: {
        projectionTruncated: false,
        roots: [],
        storedCount: '0',
        visibleTruncated: false,
      },
    },
    status: 'available',
  },
};

const moduleRuntimeFlow: ModuleRuntimeFlowResponseV1 = {
  protocolVersion: 1,
  result: {
    flow: {
      hits: [
        {
          path: [
            {
              evidence: {
                confidenceBasisPoints: 10_000,
                contentHash: '8'.repeat(64),
                evidenceId: 'c'.repeat(64),
                pathHex: '7372632f6c69622e7273',
                provider: 'treeSitter',
                range: {
                  end: { column: 8, row: 1 },
                  endByte: 16,
                  start: { column: 0, row: 1 },
                  startByte: 8,
                },
                resolution: 'adapterLocalSymbol',
                source: { kind: 'symbol', symbolId: 'd'.repeat(64) },
                target: { kind: 'symbol', symbolId: 'e'.repeat(64) },
              },
              relation: 'calls',
            },
          ],
          target: {
            kind: 'symbol',
            symbol: {
              contentHash: '8'.repeat(64),
              evidenceId: '2'.repeat(64),
              name: 'run',
              pathHex: '7372632f6c69622e7273',
              selectionRange: {
                end: { column: 7, row: 2 },
                endByte: 24,
                start: { column: 4, row: 2 },
                startByte: 21,
              },
              symbolId: 'e'.repeat(64),
              symbolKind: 'function',
            },
          },
        },
      ],
      indexRunId: '6'.repeat(64),
      kind: 'entrypointCalls',
      moduleId: 'a'.repeat(64),
      rootSymbolId: 'd'.repeat(64),
      snapshotId: '4'.repeat(64),
      truncated: false,
    },
    status: 'available',
  },
};

const repositoryTreeRoot: RepositoryTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      directoryPathHex: null,
      entries: [
        {
          contentHash: '7'.repeat(64),
          descendantFileCount: '1',
          kind: 'file',
          name: 'README.md',
          nameTruncated: false,
          pathHex: '524541444d452e6d64',
        },
        {
          contentHash: null,
          descendantFileCount: '2',
          kind: 'directory',
          name: 'src',
          nameTruncated: false,
          pathHex: '737263',
        },
      ],
      indexRunId: '6'.repeat(64),
      nextAfterNameHex: null,
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const repositoryTreeSrc: RepositoryTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      directoryPathHex: '737263',
      entries: [
        {
          contentHash: '8'.repeat(64),
          descendantFileCount: '1',
          kind: 'file',
          name: 'lib.rs',
          nameTruncated: false,
          pathHex: '7372632f6c69622e7273',
        },
      ],
      indexRunId: '6'.repeat(64),
      nextAfterNameHex: null,
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const idleDeepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    configuration: {
      model: {
        profileId: '5'.repeat(64),
        profileVersion: 1,
        providerId: 'ollama',
        modelId: 'mapper:latest',
        contextTokens: 16_384,
        outputTokens: 2_048,
      },
      minimumBudget: { tokenLimit: 1, timeLimitMillis: 1, toolCallLimit: 1 },
      defaultBudget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      maximumBudget: {
        tokenLimit: 1_000_000,
        timeLimitMillis: 86_400_000,
        toolCallLimit: 4_096,
      },
    },
    activity: {
      state: 'idle',
      budget: null,
      progress: null,
      failure: null,
      confirmedSteps: '0',
      totalSteps: '0',
    },
  },
};

const timedOutDeepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    configuration: (
      idleDeepMapStatus.result as Extract<
        DeepMapStatusResponseV1['result'],
        { status: 'available' }
      >
    ).configuration,
    activity: {
      state: 'failed',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: null,
      failure: 'modelTimedOut',
      confirmedSteps: '0',
      totalSteps: '0',
    },
  },
};

const unavailableGeminiDeepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    configuration: {
      ...(
        idleDeepMapStatus.result as Extract<
          DeepMapStatusResponseV1['result'],
          { status: 'available' }
        >
      ).configuration,
      model: {
        ...(
          idleDeepMapStatus.result as Extract<
            DeepMapStatusResponseV1['result'],
            { status: 'available' }
          >
        ).configuration.model,
        providerId: 'gemini',
        modelId: 'gemini-flash-latest',
      },
    },
    activity: {
      state: 'failed',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: null,
      failure: 'modelUnavailable',
      confirmedSteps: '0',
      totalSteps: '0',
    },
  },
};

const removedProject: RemoveProjectResponseV1 = {
  protocolVersion: 1,
  result: { retainedPrivateStorage: true, status: 'removed' },
};

describe('A^3 desktop shell', () => {
  it('keeps heavy product surfaces unloaded until visibility or explicit activation', async () => {
    class DeferredIntersectionObserver implements IntersectionObserver {
      readonly root = null;
      readonly rootMargin = '0px';
      readonly scrollMargin = '0px';
      readonly thresholds = [0];

      disconnect(): void {}
      observe(): void {}
      takeRecords(): IntersectionObserverEntry[] {
        return [];
      }
      unobserve(): void {}
    }
    vi.stubGlobal('IntersectionObserver', DeferredIntersectionObserver);
    const view = render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
      },
    });
    try {
      expect(
        await screen.findByText(/Agent Workspace wird bei Sichtbarkeit geladen/u),
      ).toBeTruthy();
      expect(screen.getByText(/Settings werden bei Sichtbarkeit geladen/u)).toBeTruthy();
      expect(screen.queryByRole('heading', { name: 'Lokaler Provider' })).toBeNull();

      const explicitLoadButtons = screen.getAllByRole('button', { name: 'Jetzt laden' });
      await fireEvent.click(explicitLoadButtons[1]!);
      expect(await screen.findByRole('navigation', { name: 'Einstellungsbereiche' })).toBeTruthy();
    } finally {
      view.unmount();
      vi.unstubAllGlobals();
    }
  });

  it('releases the owned activity timer when the desktop shell unmounts', async () => {
    const setInterval = vi.spyOn(window, 'setInterval');
    const clearInterval = vi.spyOn(window, 'clearInterval');
    const view = render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
      },
    });
    try {
      await screen.findByRole('heading', { name: 'Deine Projekte' });
      const activityTimer = setInterval.mock.results[0]?.value;
      expect(activityTimer).toBeDefined();
      view.unmount();
      expect(clearInterval).toHaveBeenCalledWith(activityTimer);
    } finally {
      setInterval.mockRestore();
      clearInterval.mockRestore();
    }
  });

  it('shows the exact product identity and keeps technical app information in Settings', async () => {
    const { container } = render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    expect(screen.getByRole('heading', { level: 1, name: 'A^3' })).toBeTruthy();
    expect(screen.getByText('Autonomous Agent Assistant')).toBeTruthy();

    const settings = container.querySelector<HTMLElement>('#settings');
    expect(settings).not.toBeNull();
    await fireEvent.click(screen.getByRole('link', { name: 'Settings' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Info' }));
    await waitFor(() => expect(settings?.textContent).toContain('0.1.0'));
    expect(settings?.textContent).toContain('V1');
    expect(settings?.textContent).toContain('windows');
    expect(container.querySelector('.health-card')).toBeNull();
    expect(container.querySelector('.workspace-menu')).toBeNull();
    expect(container.querySelector('.sidebar-footer')).toBeNull();
  });

  it('keeps an unknown project from being projected as a known empty run', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => {
          throw new Error('status unavailable');
        },
      },
    });

    const globalStatus = screen.getByRole('region', { name: 'Globaler Arbeitsstatus' });
    await waitFor(() => {
      expect(globalStatus.textContent).toContain('Projektstatus nicht verfügbar');
      expect(globalStatus.textContent).toContain('Runstatus nicht verfügbar');
    });
    expect(globalStatus.textContent).not.toContain('Kein Projekt geöffnet');
  });

  it('restores hash navigation and keeps project, index, model, and run status global', async () => {
    const routeBase = `${window.location.pathname}${window.location.search}`;
    window.history.replaceState(null, '', `${routeBase}#settings`);
    const projectStatusLoader = vi.fn(async () => activeProjectStatus);
    const props = {
      agentGoalTasksLoader: async (): Promise<TaskLensTasksResponseV1> => ({
        protocolVersion: 1,
        result: { status: 'available', tasks: [], truncated: false },
      }),
      deepMapStatusLoader: async () => idleDeepMapStatus,
      healthLoader: async () => health,
      indexActivityLoader: async () => runningIndexActivity,
      indexOverviewLoader: async () => publishedIndexOverview,
      projectStatusLoader,
    };
    let view = render(App, { props });
    try {
      const settings = screen.getByRole('link', { name: 'Settings' });
      expect(settings.getAttribute('aria-current')).toBe('page');
      expect(screen.getByRole('main').getAttribute('data-workspace-area')).toBe('settings');
      expect(screen.getByRole('region', { name: 'Settings workspace' })).toBeTruthy();

      const globalStatus = screen.getByRole('region', { name: 'Globaler Arbeitsstatus' });
      const toolbar = globalStatus.closest('header');
      expect(toolbar?.classList.contains('workspace-toolbar')).toBe(true);
      expect(toolbar?.textContent).toContain('Settings');
      await waitFor(() => {
        expect(globalStatus.textContent).toContain('C:\\worktree');
        expect(globalStatus.textContent).toContain('Zusammenhänge erkennen · 4/6');
        expect(globalStatus.textContent).toContain('Mapping bereit · mapper:latest');
        expect(globalStatus.textContent).toContain('Kein Run ausgewählt');
      });

      view.unmount();
      view = render(App, { props });
      expect(screen.getByRole('link', { name: 'Settings' }).getAttribute('aria-current')).toBe(
        'page',
      );
      await waitFor(() => expect(projectStatusLoader).toHaveBeenCalledTimes(2));

      const projects = screen.getByRole('link', { name: 'Projects' });
      const workspace = document.getElementById('workspace-content');
      if (workspace === null) throw new Error('workspace content is missing');
      workspace.scrollTop = 240;
      workspace.scrollLeft = 80;
      await fireEvent.click(projects);
      expect(projects.getAttribute('aria-current')).toBe('page');
      expect(screen.getByRole('main').getAttribute('data-workspace-area')).toBe('projects');
      expect(screen.getByRole('region', { name: 'Projects workspace' })).toBeTruthy();
      expect(document.activeElement?.id).toBe('projects');
      expect(workspace.scrollTop).toBe(0);
      expect(workspace.scrollLeft).toBe(0);
    } finally {
      view.unmount();
      window.history.replaceState(null, '', routeBase);
    }
  });

  it('explains live code-analysis progress without exposing parser internals by default', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        indexActivityLoader: async () => runningIndexActivity,
        indexOverviewLoader: async () => publishedIndexOverview,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Projekt verwalten' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Code-Analyse' }));
    expect(await screen.findByText('Schritt 4 von 6: Zusammenhänge erkennen')).toBeTruthy();
    expect(
      screen.getByText('Die zuletzt fertige Analyse bleibt währenddessen nutzbar.'),
    ).toBeTruthy();
    expect(
      screen.getByRole('progressbar', { name: 'Fortschritt der Code-Analyse' }),
    ).toHaveProperty('value', 3);
    expect(screen.getByRole('heading', { name: 'Wie gut A^3 dein Projekt kennt' })).toBeTruthy();
    expect(screen.getAllByText(/80,00\s%/)).toHaveLength(1);
    const issues = screen.getByText('Hinweise zu 1 Dateien').closest('details');
    expect(issues?.open).toBe(false);
    await fireEvent.click(screen.getByText('Hinweise zu 1 Dateien'));
    expect(screen.getByText('src/lib.rs')).toBeTruthy();
    const diagnostic = screen.getByText('syntax error', { exact: false });
    expect(diagnostic.textContent).toContain('Fehler:');
    expect(screen.queryByText(/Bytes 8/)).toBeNull();
  });

  it('shows authoritative Stale and NeedsReview Module Card counts with causes', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleCardFreshnessLoader: async () => moduleCardFreshness,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Mapping' }));
    expect(await screen.findByRole('heading', { name: 'Module-Card-Aktualität' })).toBeTruthy();
    expect(screen.getByText('Stale')).toBeTruthy();
    expect(screen.getByText('NeedsReview')).toBeTruthy();
    expect(screen.getByText(/Direkte Evidenz geändert · 2/)).toBeTruthy();
    expect(screen.getByText(/Direkte Abhängigkeit geändert · 1/)).toBeTruthy();
  });

  it('runs Project Map search only on submit and exposes bounded provenance and evidence', async () => {
    const projectMapSearchLoader = vi.fn(async () => projectMapSearch);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectMapSearchLoader,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    const input = await screen.findByRole('searchbox', {
      name: 'Pfad, Symbol oder Signatur suchen',
    });
    expect(projectMapSearchLoader).not.toHaveBeenCalled();
    await fireEvent.input(input, { target: { value: 'launch parser' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Suchen' }));

    await waitFor(() => expect(projectMapSearchLoader).toHaveBeenCalledTimes(1));
    expect(projectMapSearchLoader).toHaveBeenCalledWith({ query: 'launch parser' });
    expect(await screen.findByText('crate::launch')).toBeTruthy();
    expect(screen.getByText('Exact')).toBeTruthy();
    expect(screen.getByText('Lexical')).toBeTruthy();
    expect(screen.getByText('exakter qualifizierter Name')).toBeTruthy();
    expect(screen.getByText(/weitere Kandidaten sichtbar aus/)).toBeTruthy();
    expect(screen.getByText(/Semantische Ähnlichkeit.*niemals ein Beweis/)).toBeTruthy();

    const evidence = screen.getByText('Evidence anzeigen').parentElement;
    if (!(evidence instanceof HTMLDetailsElement)) {
      throw new Error('Project Map search Evidence details are missing.');
    }
    await fireEvent.click(screen.getByText('Evidence anzeigen'));
    expect(evidence.open).toBe(true);
    expect(screen.getByText('c'.repeat(64))).toBeTruthy();
    expect(screen.getByText('Bytes 0–11')).toBeTruthy();
  });

  it('switches explicitly to a durable Task Lens and keeps semantic candidates and hypotheses unproven', async () => {
    const taskLensTasksLoader = vi.fn(async () => taskLensTasks);
    const taskLensTaskLoader = vi.fn(async () => taskLensTask);
    const taskLensCompiler = vi.fn(async () => taskLensCompilation);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
        taskLensCompiler,
        taskLensTaskLoader,
        taskLensTasksLoader,
      },
    });

    await screen.findByRole('searchbox', { name: 'Pfad, Symbol oder Signatur suchen' });
    expect(taskLensTasksLoader).not.toHaveBeenCalled();
    expect(taskLensTaskLoader).not.toHaveBeenCalled();
    expect(taskLensCompiler).not.toHaveBeenCalled();

    const searchMode = screen.getByRole('button', { name: 'Suche' });
    const taskLensMode = screen.getByRole('button', { name: 'Task Lens' });
    expect(screen.getByRole('group', { name: 'Project-Map-Ansicht' })).toBeTruthy();
    expect(searchMode.getAttribute('aria-pressed')).toBe('true');
    expect(taskLensMode.getAttribute('aria-pressed')).toBe('false');
    await fireEvent.click(taskLensMode);
    expect(taskLensMode.getAttribute('aria-pressed')).toBe('true');
    expect(searchMode.getAttribute('aria-pressed')).toBe('false');
    await waitFor(() => expect(taskLensTasksLoader).toHaveBeenCalledTimes(1));
    expect(screen.getByText(/WebView kann weder Seeds noch Projektpfade erfinden/)).toBeTruthy();

    await fireEvent.change(screen.getByLabelText('Goal Contract'), {
      target: { value: taskLensTaskId },
    });
    await waitFor(() =>
      expect(taskLensTaskLoader).toHaveBeenCalledWith({ taskId: taskLensTaskId }),
    );
    expect(taskLensCompiler).not.toHaveBeenCalled();

    await fireEvent.change(screen.getByLabelText('Aktueller Fokus-Schritt'), {
      target: { value: taskLensStepId },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Task Lens aktualisieren' }));
    await waitFor(() =>
      expect(taskLensCompiler).toHaveBeenCalledWith({
        stepId: taskLensStepId,
        taskId: taskLensTaskId,
      }),
    );

    expect(await screen.findByText('Semantic · nur Kandidat')).toBeTruthy();
    expect(screen.getByText('kein Beweis')).toBeTruthy();
    expect(screen.getByText(/2 stale Claims wurden vollständig ausgeschlossen/)).toBeTruthy();
    expect(screen.getByText('Hypothese · unbewiesen')).toBeTruthy();
    expect(
      screen
        .getByText('Hypothese · unbewiesen')
        .closest('li')
        ?.classList.contains('task-lens-hypothesis'),
    ).toBe(true);

    const fileEvidence = screen.getAllByText('Evidence anzeigen')[1]?.parentElement;
    if (!(fileEvidence instanceof HTMLDetailsElement)) {
      throw new Error('Task Lens file Evidence details are missing.');
    }
    await fireEvent.click(screen.getAllByText('Evidence anzeigen')[1]);
    expect(fileEvidence.open).toBe(true);
    expect(screen.getAllByText('c'.repeat(64)).length).toBeGreaterThan(0);

    const hypothesisEvidence = screen.getAllByText('Evidence / Beweisstatus')[1]?.parentElement;
    if (!(hypothesisEvidence instanceof HTMLDetailsElement)) {
      throw new Error('Task Lens hypothesis proof status is missing.');
    }
    await fireEvent.click(screen.getAllByText('Evidence / Beweisstatus')[1]);
    expect(hypothesisEvidence.open).toBe(true);
    expect(screen.getByText(/Keine beweisende Evidence vorhanden/)).toBeTruthy();
  });

  it('navigates the bounded published repository tree one directory at a time', async () => {
    const repositoryTreeLoader = vi.fn(async (query: { directoryPathHex: string | null }) =>
      query.directoryPathHex === null ? repositoryTreeRoot : repositoryTreeSrc,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
        repositoryTreeLoader,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    expect(await screen.findByRole('heading', { name: 'Repository-Baum' })).toBeTruthy();
    expect(await screen.findByText('README.md')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Verzeichnis src öffnen' }));

    expect(await screen.findByText('lib.rs')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'src' }).getAttribute('aria-current')).toBe('page');
    expect(repositoryTreeLoader).toHaveBeenCalledWith({
      afterNameHex: null,
      directoryPathHex: '737263',
      limit: 50,
    });
  });

  it('keeps only one bounded repository page while navigating forward and backward', async () => {
    const cursor = '524541444d452e6d64';
    const rootResult = repositoryTreeRoot.result;
    if (rootResult.status !== 'available')
      throw new Error('repository root fixture is unavailable');
    const firstPage: RepositoryTreeResponseV1 = {
      ...repositoryTreeRoot,
      result: {
        ...rootResult,
        page: {
          ...rootResult.page,
          entries: [rootResult.page.entries[0]!],
          nextAfterNameHex: cursor,
        },
      },
    };
    const secondPage: RepositoryTreeResponseV1 = {
      ...repositoryTreeRoot,
      result: {
        ...rootResult,
        page: {
          ...rootResult.page,
          entries: [rootResult.page.entries[1]!],
          nextAfterNameHex: null,
        },
      },
    };
    const repositoryTreeLoader = vi.fn(async (query: { afterNameHex: string | null }) =>
      query.afterNameHex === null ? firstPage : secondPage,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
        repositoryTreeLoader,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    expect(await screen.findByText('README.md')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Nächste Seite' }));
    expect(await screen.findByRole('button', { name: 'Verzeichnis src öffnen' })).toBeTruthy();
    expect(screen.queryByText('README.md')).toBeNull();
    expect(screen.getByText(/Seite 2 · höchstens 50 Einträge im DOM/u)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Vorherige Seite' }));
    expect(await screen.findByText('README.md')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Verzeichnis src öffnen' })).toBeNull();
    expect(repositoryTreeLoader).toHaveBeenCalledWith({
      afterNameHex: cursor,
      directoryPathHex: null,
      limit: 50,
    });
  });

  it('navigates only direct primary modules while exposing graph communities as a count', async () => {
    const moduleTreeLoader = vi.fn(async (query: { parentModuleId: string | null }) =>
      query.parentModuleId === null ? moduleTreeRoot : moduleTreeRepository,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleTreeLoader,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    expect(await screen.findByRole('heading', { name: 'Modulbaum' })).toBeTruthy();
    expect(await screen.findByRole('button', { name: 'Modul Repository öffnen' })).toBeTruthy();
    expect(screen.getByText('Graph-Communities')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Graph-Community/ })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Modul Repository öffnen' }));

    expect(await screen.findByText('tools')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Repository' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(moduleTreeLoader).toHaveBeenCalledWith({
      afterModuleId: null,
      limit: 50,
      parentModuleId: 'a'.repeat(64),
    });
  });

  it('keeps only one bounded module page while retaining validated cursor history', async () => {
    const cursor = 'a'.repeat(64);
    const rootResult = moduleTreeRoot.result;
    const repositoryResult = moduleTreeRepository.result;
    if (rootResult.status !== 'available' || repositoryResult.status !== 'available') {
      throw new Error('module tree fixture is unavailable');
    }
    const firstPage: ModuleTreeResponseV1 = {
      ...moduleTreeRoot,
      result: {
        ...rootResult,
        page: { ...rootResult.page, nextAfterModuleId: cursor },
      },
    };
    const secondPage: ModuleTreeResponseV1 = {
      ...moduleTreeRepository,
      result: {
        ...repositoryResult,
        page: {
          ...repositoryResult.page,
          parentModuleId: null,
        },
      },
    };
    const moduleTreeLoader = vi.fn(async (query: { afterModuleId: string | null }) =>
      query.afterModuleId === null ? firstPage : secondPage,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleTreeLoader,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    expect(await screen.findByRole('button', { name: 'Modul Repository öffnen' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Nächste Seite' }));
    expect(await screen.findByText('tools')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Modul Repository öffnen' })).toBeNull();
    expect(screen.getByText(/Seite 2 · höchstens 50 Module im DOM/u)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Vorherige Seite' }));
    expect(await screen.findByRole('button', { name: 'Modul Repository öffnen' })).toBeTruthy();
    expect(screen.queryByText('tools')).toBeNull();
    expect(moduleTreeLoader).toHaveBeenCalledWith({
      afterModuleId: cursor,
      limit: 50,
      parentModuleId: null,
    });
  });

  it('loads a bounded module dependency graph only after selection and exposes exact evidence', async () => {
    const moduleDependencyGraphLoader = vi.fn(async () => moduleDependencyGraph);
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleDependencyGraphLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    expect(moduleDependencyGraphLoader).not.toHaveBeenCalled();
    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Abhängigkeiten anzeigen' }));

    await waitFor(() => expect(moduleDependencyGraphLoader).toHaveBeenCalledTimes(1));
    expect(moduleDependencyGraphLoader).toHaveBeenCalledWith({
      centerModuleId: 'a'.repeat(64),
      nodeLimit: 50,
    });
    expect(await screen.findByText('beobachtete Belege', { exact: false })).toBeTruthy();
    expect(screen.getByText(/1 inspizierte Kanten besitzen keinen eindeutig/)).toBeTruthy();

    await fireEvent.click(
      screen.getByRole('button', {
        name: 'Evidence für Repository baut tools anzeigen',
      }),
    );
    expect(
      await screen.findByRole('heading', { name: 'Repräsentative Graph-Evidence' }),
    ).toBeTruthy();
    expect(screen.getByText('src/lib.rs')).toBeTruthy();
    expect(screen.getByText('c'.repeat(64))).toBeTruthy();
    expect(screen.getByText(/Bytes 8–16/)).toBeTruthy();
  });

  it('releases graph selection and evidence when the active project changes', async () => {
    const nextProject = {
      ...projectSummary,
      repositoryId: '9'.repeat(64),
      worktreeId: '8'.repeat(64),
      worktreeRootDisplay: 'D:\\next-worktree',
    };
    const nextStatus: ProjectStatusResponseV1 = {
      ...activeProjectStatus,
      result: { ...activeProjectResult, project: nextProject, projectId: '7'.repeat(64) },
    };
    const projectStatusLoader = vi
      .fn<() => Promise<ProjectStatusResponseV1>>()
      .mockResolvedValueOnce(activeProjectStatus)
      .mockResolvedValue(nextStatus);
    const moduleDependencyGraphLoader = vi.fn(async () => moduleDependencyGraph);
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleDependencyGraphLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectOpener: async () => ({
          protocolVersion: 1,
          result: { project: nextProject, status: 'opened' },
        }),
        projectStatusLoader,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Abhängigkeiten anzeigen' }));
    await fireEvent.click(
      await screen.findByRole('button', {
        name: 'Evidence für Repository baut tools anzeigen',
      }),
    );
    expect(screen.getByRole('heading', { name: 'Repräsentative Graph-Evidence' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Projekt hinzufügen' }));
    expect((await screen.findAllByText('D:\\next-worktree')).length).toBeGreaterThanOrEqual(2);
    expect(screen.queryByRole('heading', { name: 'Repräsentative Graph-Evidence' })).toBeNull();
    expect(screen.getByText(/Wähle im Modulbaum „Abhängigkeiten anzeigen“/u)).toBeTruthy();
    expect(moduleDependencyGraphLoader).toHaveBeenCalledTimes(1);
  });

  it('loads a Module Card only after selection and never presents a stale Fact as current', async () => {
    let resolveReload: ((response: ModuleCardDetailResponseV1) => void) | undefined;
    const pendingReload = new Promise<ModuleCardDetailResponseV1>((resolve) => {
      resolveReload = resolve;
    });
    const moduleCardDetailLoader = vi
      .fn<() => Promise<ModuleCardDetailResponseV1>>()
      .mockResolvedValueOnce(staleModuleCard)
      .mockReturnValueOnce(pendingReload);
    const moduleCardEvidenceLoader = vi.fn(async () => staleModuleCardEvidence);
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleCardDetailLoader,
        moduleCardEvidenceLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    expect(moduleCardDetailLoader).not.toHaveBeenCalled();
    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Module Card' }));

    await waitFor(() => expect(moduleCardDetailLoader).toHaveBeenCalledTimes(1));
    expect(moduleCardDetailLoader).toHaveBeenCalledWith({ moduleId: 'a'.repeat(64) });
    expect(await screen.findByText('Stale — keine aktuelle Faktenquelle')).toBeTruthy();
    expect(
      screen.getByRole('heading', { name: 'Confidence, Coverage und Freshness' }),
    ).toBeTruthy();
    expect(screen.getByText(/1 von 12 Feldern/)).toBeTruthy();
    expect(screen.getByText(/1 von 8 Muss-Feldern/)).toBeTruthy();
    expect(screen.getByText(/Numerische Einschätzung/)).toBeTruthy();
    await fireEvent.click(screen.getByText('Feldabdeckung im Detail'));
    expect(screen.getByRole('heading', { name: 'Fehlende Muss-Felder' })).toBeTruthy();
    expect(screen.getByText('Titel')).toBeTruthy();
    expect(screen.getByText('Fact')).toBeTruthy();
    expect(screen.getByText('exports main')).toBeTruthy();
    expect(screen.getByText(/Ein als „Fact“ klassifizierter/)).toBeTruthy();
    expect(screen.getByText('1 Claim-Evidence-ID(s)')).toBeTruthy();
    expect(moduleCardEvidenceLoader).not.toHaveBeenCalled();

    await fireEvent.click(
      screen.getByRole('button', {
        name: /Evidence f+ für „exports main“ untersuchen/,
      }),
    );
    await waitFor(() => expect(moduleCardEvidenceLoader).toHaveBeenCalledTimes(1));
    expect(moduleCardEvidenceLoader).toHaveBeenCalledWith({
      cardId: 'e'.repeat(64),
      currentIndexRunId: '6'.repeat(64),
      currentSnapshotId: '4'.repeat(64),
      evidenceId: 'f'.repeat(64),
      moduleId: 'a'.repeat(64),
      sourceIndexRunId: '5'.repeat(64),
      sourceSnapshotId: '3'.repeat(64),
    });
    expect(await screen.findByText('Evidence Stale — nur historische Provenienz')).toBeTruthy();
    expect(screen.getByText('Card-Zustand:', { exact: false })).toBeTruthy();
    expect(screen.getByText('src/lib.rs')).toBeTruthy();

    const cardPanel = screen.getByRole('heading', { name: 'Module Card' }).parentElement
      ?.parentElement;
    const refreshButton = cardPanel?.querySelector('button');
    if (!(refreshButton instanceof HTMLButtonElement)) {
      throw new Error('Module Card refresh button is missing.');
    }
    await fireEvent.click(refreshButton);
    await waitFor(() => expect(moduleCardDetailLoader).toHaveBeenCalledTimes(2));
    expect(screen.queryByText('exports main')).toBeNull();
    expect(screen.queryByText('Evidence Stale — nur historische Provenienz')).toBeNull();
    expect(screen.getByText(/wird atomar gelesen/)).toBeTruthy();

    resolveReload?.(staleModuleCard);
    expect(await screen.findByText('exports main')).toBeTruthy();
  });

  it('rejects an Evidence hook after publication or Card selection changed', async () => {
    const moduleCardEvidenceLoader = vi.fn(async (): Promise<ModuleCardEvidenceResponseV1> => ({
      protocolVersion: 1,
      result: { status: 'selectionChanged' },
    }));
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleCardDetailLoader: async () => staleModuleCard,
        moduleCardEvidenceLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Module Card' }));
    await fireEvent.click(
      await screen.findByRole('button', {
        name: /Evidence f+ für „exports main“ untersuchen/,
      }),
    );

    expect(
      await screen.findByText(/Publikation oder neueste Card haben sich geändert/),
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Module Card neu laden' })).toBeTruthy();
  });

  it('loads runtime roots only after selection and traces a publication-bound evidence path', async () => {
    const moduleRuntimeMapLoader = vi.fn(async () => moduleRuntimeMap);
    const moduleRuntimeFlowLoader = vi.fn(async () => moduleRuntimeFlow);
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleRuntimeFlowLoader,
        moduleRuntimeMapLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    expect(moduleRuntimeMapLoader).not.toHaveBeenCalled();
    expect(moduleRuntimeFlowLoader).not.toHaveBeenCalled();

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Entry Points & Tests' }));
    await waitFor(() => expect(moduleRuntimeMapLoader).toHaveBeenCalledTimes(1));
    expect(moduleRuntimeMapLoader).toHaveBeenCalledWith({
      entrypointLimit: 20,
      moduleId: 'a'.repeat(64),
      testLimit: 20,
    });
    expect(await screen.findByText(/Strukturelle Beobachtung/)).toBeTruthy();
    expect(screen.getByText('main')).toBeTruthy();
    expect(moduleRuntimeFlowLoader).not.toHaveBeenCalled();

    await fireEvent.click(
      screen.getByRole('button', { name: 'Aufrufpfad für Entry Point main anzeigen' }),
    );
    await waitFor(() => expect(moduleRuntimeFlowLoader).toHaveBeenCalledTimes(1));
    expect(moduleRuntimeFlowLoader).toHaveBeenCalledWith({
      expectedIndexRunId: '6'.repeat(64),
      expectedSnapshotId: '4'.repeat(64),
      kind: 'entrypointCalls',
      moduleId: 'a'.repeat(64),
      resultLimit: 20,
      rootSymbolId: 'd'.repeat(64),
    });
    expect(await screen.findByText('run')).toBeTruthy();
    expect(screen.getByText(/Schritt 1: beobachteter Aufruf/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Kanten-Evidence' }));
    expect(await screen.findByRole('heading', { name: 'Graph-Kanten-Evidence' })).toBeTruthy();
    expect(screen.getByText('c'.repeat(64))).toBeTruthy();
    expect(moduleRuntimeMapLoader).toHaveBeenCalledTimes(1);
    expect(moduleRuntimeFlowLoader).toHaveBeenCalledTimes(1);
  });

  it('hides stale runtime roots and evidence after a publication switch', async () => {
    const moduleRuntimeMapLoader = vi.fn(async () => moduleRuntimeMap);
    const moduleRuntimeFlowLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'publicationChanged' as const },
    }));
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleRuntimeFlowLoader,
        moduleRuntimeMapLoader,
        moduleTreeLoader: async () => moduleTreeRoot,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Explorer' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Entry Points & Tests' }));
    await fireEvent.click(
      await screen.findByRole('button', { name: 'Aufrufpfad für Entry Point main anzeigen' }),
    );

    expect(await screen.findByText(/Root-Liste ist nicht mehr verifizierbar/)).toBeTruthy();
    expect(
      screen.queryByRole('button', { name: 'Aufrufpfad für Entry Point main anzeigen' }),
    ).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Symbol-Evidence' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Roots neu laden' }));
    await waitFor(() => expect(moduleRuntimeMapLoader).toHaveBeenCalledTimes(2));
    expect(
      await screen.findByRole('button', { name: 'Aufrufpfad für Entry Point main anzeigen' }),
    ).toBeTruthy();
  });

  it('shows verified model and budgets without starting Deep Map until the explicit click', async () => {
    const deepMapStarter = vi.fn<
      (budget: {
        tokenLimit: number;
        timeLimitMillis: number;
        toolCallLimit: number;
      }) => Promise<DeepMapControlResponseV1>
    >(async () => ({ accepted: true, protocolVersion: 1 }));
    render(App, {
      props: {
        deepMapStarter,
        deepMapStatusLoader: async () => idleDeepMapStatus,
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Mapping' }));
    expect(await screen.findByText('ollama / mapper:latest')).toBeTruthy();
    expect(screen.getByText('Bereit für einen bewussten Start')).toBeTruthy();
    expect(deepMapStarter).not.toHaveBeenCalled();
    expect((screen.getByLabelText('Tokenbudget') as HTMLInputElement).valueAsNumber).toBe(32_000);
    expect(
      (screen.getByLabelText('Zeitbudget in Millisekunden') as HTMLInputElement).valueAsNumber,
    ).toBe(120_000);
    expect(
      (screen.getByLabelText('Read-only-Werkzeugaufrufe') as HTMLInputElement).valueAsNumber,
    ).toBe(64);

    await fireEvent.click(screen.getByRole('button', { name: 'Deep Map bewusst starten' }));

    await waitFor(() => {
      expect(deepMapStarter).toHaveBeenCalledWith({
        tokenLimit: 32_000,
        timeLimitMillis: 120_000,
        toolCallLimit: 64,
      });
    });
  });

  it('explains a failed Deep Map run with a concrete safe recovery step', async () => {
    render(App, {
      props: {
        deepMapStatusLoader: async () => timedOutDeepMapStatus,
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Mapping' }));
    const alert = (await screen.findByText('Die Modellantwort hat zu lange gedauert')).closest(
      '[role="alert"]',
    );
    expect(alert).not.toBeNull();
    expect(alert?.textContent).toContain('Die Modellantwort hat zu lange gedauert');
    expect(alert?.textContent).toContain('kleineres beziehungsweise schnelleres Modell');
    expect(
      (screen.getByRole('button', { name: 'Deep Map bewusst starten' }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
    expect((screen.getByRole('button', { name: 'Pausieren' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect((screen.getByRole('button', { name: 'Fortsetzen' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect((screen.getByRole('button', { name: 'Abbrechen' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('uses the configured provider in Deep Map connection recovery guidance', async () => {
    render(App, {
      props: {
        deepMapStatusLoader: async () => unavailableGeminiDeepMapStatus,
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Mapping' }));
    const alert = (await screen.findByText('Das Mapping-Modell ist nicht erreichbar')).closest(
      '[role="alert"]',
    );
    expect(alert?.textContent).toContain('automatische zweite Verbindungsversuch');
    expect(alert?.textContent).toContain('Google-Gemini-Verbindung');
    expect(alert?.textContent).not.toContain('Ollama');
  });

  it('keeps unavailable app information terse in Settings and supports retry', async () => {
    const healthLoader = vi
      .fn<() => Promise<HealthResponseV1>>()
      .mockRejectedValueOnce(new Error('sensitive internal detail'))
      .mockResolvedValueOnce(health);

    render(App, {
      props: {
        healthLoader,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    await fireEvent.click(screen.getByRole('link', { name: 'Settings' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Info' }));
    const heading = await screen.findByRole('heading', { name: 'Info' });
    const page = heading.closest('section');
    expect(page).not.toBeNull();
    expect(await within(page!).findByText('Informationen nicht verfügbar')).toBeTruthy();
    expect(page?.textContent).not.toContain('sensitive internal detail');

    await fireEvent.click(within(page!).getByRole('button', { name: 'Erneut laden' }));

    await waitFor(() => {
      expect(page?.textContent).toContain('0.1.0');
    });
    expect(healthLoader).toHaveBeenCalledTimes(2);
  });

  it('opens a project after explicit interaction and keeps technical details in one dialog', async () => {
    const projectOpener = vi.fn(async () => openedProject);
    const projectStatusLoader = vi
      .fn<() => Promise<ProjectStatusResponseV1>>()
      .mockResolvedValueOnce(noProjectStatus)
      .mockResolvedValueOnce(activeProjectStatus)
      .mockResolvedValueOnce(queuedRebuildStatus);
    const projectRebuilder = vi.fn<() => Promise<RebuildProjectIndexResponseV1>>(async () => ({
      protocolVersion: 1,
      state: 'queued',
    }));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectRebuilder,
        projectStatusLoader,
      },
    });

    expect(projectOpener).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Projekt hinzufügen' }));

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Aktives Projekt' })).toBeTruthy();
      expect(screen.getAllByText('C:\\worktree')).toHaveLength(2);
    });
    expect(projectOpener).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByText('Projekt verwalten'));
    const dialog = screen.getByRole('dialog', { name: 'worktree' });
    expect(within(dialog).getByRole('heading', { name: 'Analyse ist bereit' })).toBeTruthy();
    expect(within(dialog).queryByText('Wartung')).toBeNull();
    const technicalDetails = within(dialog).getByText('Technische Details').closest('details');
    expect(technicalDetails?.open).toBe(false);
    await fireEvent.click(within(dialog).getByText('Technische Details'));
    expect(within(dialog).getByText(/Generation 2/)).toBeTruthy();
    expect(within(dialog).getByText('4 KB')).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Optionen' }));
    expect(
      within(dialog).getByText(
        /wenn Suche oder Projektkarte veraltet wirken.*Projektdaten bleiben unverändert/s,
      ),
    ).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Analyse neu erstellen' }));
    await waitFor(() => expect(screen.getByText('Die neue Analyse startet gleich')).toBeTruthy());
    expect(projectRebuilder).toHaveBeenCalledTimes(1);
  });

  it('does not expose project-open adapter details in the UI error', async () => {
    const projectOpener = vi
      .fn<() => Promise<OpenProjectResponseV1>>()
      .mockRejectedValue(new Error('C:\\secret\\repository contains invalid config'));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projekt hinzufügen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('erreichbarer Git-Worktree-Root');
    expect(alert.textContent).not.toContain('secret');
  });

  it('shows concrete recovery for a selected path that became unavailable', async () => {
    const projectOpener = vi.fn<() => Promise<OpenProjectResponseV1>>().mockRejectedValue({
      code: 'projectSelectionUnavailable',
      message: 'C:\\secret\\repository disappeared',
      protocolVersion: 1,
    });
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projekt hinzufügen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('Prüfe Laufwerk und Zugriffsrechte');
    expect(alert.textContent).toContain('wähle ihn erneut');
    expect(alert.textContent).not.toContain('secret');
  });

  it('presents Projects as the searchable project catalog', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Deine Projekte' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Projekt hinzufügen' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Gespeicherte Worktrees' })).toBeTruthy();
    expect(screen.getByRole('search')).toBeTruthy();
  });

  it('restores the last project before reading active status', async () => {
    const order: string[] = [];
    const projectRestorer = vi.fn(async (): Promise<ProjectActivationResponseV1> => {
      order.push('restore');
      return {
        protocolVersion: 1,
        result: { project: projectSummary, projectId: '3'.repeat(64), status: 'activated' },
      };
    });
    const projectStatusLoader = vi.fn(async () => {
      order.push('status');
      return activeProjectStatus;
    });
    render(App, {
      props: {
        healthLoader: async () => health,
        projectRestorer,
        projectStatusLoader,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Aktives Projekt' })).toBeTruthy();
    expect(order.slice(0, 2)).toEqual(['restore', 'status']);
    expect(projectRestorer).toHaveBeenCalledTimes(1);
  });

  it('keeps the catalog usable and redacts details when startup restoration fails', async () => {
    const projectRestorer = vi.fn<() => Promise<ProjectActivationResponseV1>>().mockRejectedValue({
      code: 'projectSelectionUnavailable',
      message: 'C:\\secret\\missing-worktree disappeared',
      protocolVersion: 1,
    });
    const projectCatalogLoader = vi.fn(async (): Promise<ProjectCatalogResponseV1> => ({
      nextCursor: null,
      previousCursor: null,
      projects: [{ project: projectSummary, projectId: '3'.repeat(64) }],
      protocolVersion: 1,
    }));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectCatalogLoader,
        projectRestorer,
        projectStatusLoader: async () => noProjectStatus,
      },
    });

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Projekt konnte nicht automatisch geöffnet werden');
    expect(alert.textContent).toContain('Prüfe Laufwerk und Zugriffsrechte');
    expect(alert.textContent).toContain('füge seinen neuen Root erneut hinzu');
    expect(alert.textContent).not.toContain('secret');
    expect(await screen.findByText('worktree')).toBeTruthy();
    expect(projectCatalogLoader).toHaveBeenCalled();
  });

  it('searches, pages, activates, and confirms catalog removal', async () => {
    const nextProject: ProjectSummaryV1 = {
      ...projectSummary,
      repositoryId: '8'.repeat(64),
      worktreeId: '9'.repeat(64),
      worktreeRootDisplay: 'D:\\clients\\next-worktree',
    };
    const initialPage: ProjectCatalogResponseV1 = {
      nextCursor: '0000000000000019',
      previousCursor: null,
      projects: [
        { project: projectSummary, projectId: '3'.repeat(64) },
        { project: nextProject, projectId: '7'.repeat(64) },
      ],
      protocolVersion: 1,
    };
    const projectCatalogLoader = vi.fn(
      async (query: ProjectCatalogQueryV1): Promise<ProjectCatalogResponseV1> => {
        if (query.direction === 'next') {
          return {
            nextCursor: null,
            previousCursor: '0000000000000020',
            projects: [{ project: nextProject, projectId: '7'.repeat(64) }],
            protocolVersion: 1,
          };
        }
        if (query.search === 'next') {
          return { ...initialPage, nextCursor: null, projects: [initialPage.projects[1]!] };
        }
        return initialPage;
      },
    );
    const switchedStatus: ProjectStatusResponseV1 = {
      ...activeProjectStatus,
      result: { ...activeProjectResult, project: nextProject, projectId: '7'.repeat(64) },
    };
    const projectStatusLoader = vi
      .fn<() => Promise<ProjectStatusResponseV1>>()
      .mockResolvedValueOnce(activeProjectStatus)
      .mockResolvedValue(switchedStatus);
    const projectCatalogActivator = vi.fn(async (): Promise<ProjectActivationResponseV1> => ({
      protocolVersion: 1,
      result: { project: nextProject, projectId: '7'.repeat(64), status: 'activated' },
    }));
    const projectCatalogRemover = vi.fn(async () => removedProject);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectCatalogActivator,
        projectCatalogLoader,
        projectCatalogRemover,
        projectStatusLoader,
      },
    });

    await screen.findByText('next-worktree');
    await fireEvent.click(screen.getByRole('button', { name: 'Weiter' }));
    await waitFor(() =>
      expect(projectCatalogLoader).toHaveBeenCalledWith({
        cursor: '0000000000000019',
        direction: 'next',
        search: null,
      }),
    );
    expect(screen.getByText('Seite 2')).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('Projekte durchsuchen'), {
      target: { value: 'next' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Projekte suchen' }));
    await waitFor(() =>
      expect(projectCatalogLoader).toHaveBeenCalledWith({
        cursor: null,
        direction: 'initial',
        search: 'next',
      }),
    );

    const projectRow = screen.getByText('next-worktree').closest('li');
    expect(projectRow).not.toBeNull();
    await fireEvent.click(within(projectRow!).getByRole('button', { name: 'Aktivieren' }));
    await waitFor(() =>
      expect(projectCatalogActivator).toHaveBeenCalledWith(nextProject.worktreeId),
    );
    expect((await screen.findAllByText('D:\\clients\\next-worktree')).length).toBeGreaterThan(0);

    const refreshedRow = screen.getByText('next-worktree').closest('li');
    expect(refreshedRow).not.toBeNull();
    await fireEvent.click(
      within(refreshedRow!).getByRole('button', { name: 'Nur aus A^3 entfernen' }),
    );
    expect(projectCatalogRemover).not.toHaveBeenCalled();
    const dialog = screen.getByRole('dialog', { name: 'Projekt nur aus A^3 entfernen?' });
    expect(dialog.textContent).toContain('knowledge.db');
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Entfernen bestätigen' }));
    await waitFor(() => expect(projectCatalogRemover).toHaveBeenCalledWith(nextProject.worktreeId));
  });

  it('requires explicit confirmation and explains non-destructive project removal', async () => {
    const projectRemover = vi.fn(async () => removedProject);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectRemover,
        projectStatusLoader: async () => activeProjectStatus,
      },
    });

    await fireEvent.click(await screen.findByText('Projekt verwalten'));
    await fireEvent.click(screen.getByRole('button', { name: 'Optionen' }));
    const removeButton = await screen.findByRole('button', { name: 'Aus A^3 entfernen' });
    expect(screen.getByText(/Der Ordner und alle Dateien.*bleiben unverändert/s)).toBeTruthy();
    await fireEvent.click(removeButton);
    expect(projectRemover).not.toHaveBeenCalled();
    expect(screen.getByRole('heading', { name: 'Projekt aus A^3 entfernen?' })).toBeTruthy();
    expect(screen.getByText('Der Projektordner und alle Dateien bleiben erhalten.')).toBeTruthy();
    expect(screen.getByText('Lokale A^3-Projektdaten werden nicht gelöscht.')).toBeTruthy();
    expect(screen.getAllByRole('dialog')).toHaveLength(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Aus A^3 entfernen' }));

    await waitFor(() => {
      expect(screen.getByText(/Worktree aus der A\^3-Projektliste entfernt/)).toBeTruthy();
      expect(screen.queryByRole('heading', { name: 'Zuletzt verwendet' })).toBeNull();
    });
    expect(projectRemover).toHaveBeenCalledTimes(1);
  });
});

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MapWorkspace from './MapWorkspace.svelte';
import type { DeepMapStatusResponseV1 } from './deep-map';
import type { ModuleCardDetailResponseV1 } from './module-card-detail';
import type { ModuleCardEvidenceResponseV1 } from './module-card-evidence';
import type { ModuleRuntimeMapResponseV1 } from './module-runtime';
import type { ProjectMapSceneResponseV1 } from './project-map-scene';
import type { ProjectMapSourcePreviewResponseV1 } from './project-map-source-preview';

const id = (digit: string): string => digit.repeat(64);

const overviewScene: ProjectMapSceneResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    scene: {
      focusModuleId: null,
      indexRunId: id('1'),
      inspectedEdgeCount: '1',
      modules: [
        {
          cardBinding: {
            cardId: id('4'),
            sourceIndexRunId: id('1'),
            sourceSnapshotId: id('2'),
          },
          cardCoverageBasisPoints: 7_500,
          centralSymbolCount: '2',
          displayName: 'a3-application',
          entrypointCount: '1',
          fileCount: '8',
          kind: 'manifestBoundary',
          manifestCount: '1',
          mappingStatus: 'current',
          moduleId: id('a'),
          parentModuleId: null,
          rank: 1,
          representativeEvidenceId: id('f'),
          symbolCount: '20',
          testCount: '4',
        },
        {
          cardBinding: null,
          cardCoverageBasisPoints: null,
          centralSymbolCount: '1',
          displayName: 'desktop',
          entrypointCount: '1',
          fileCount: '4',
          kind: 'pathBoundary',
          manifestCount: '0',
          mappingStatus: 'unmapped',
          moduleId: id('b'),
          parentModuleId: null,
          rank: 2,
          representativeEvidenceId: null,
          symbolCount: '10',
          testCount: '2',
        },
      ],
      modulesTruncated: false,
      observedRelationGroupCount: '1',
      policyVersion: 'v1',
      primaryModuleCount: '2',
      relations: [
        {
          evidenceId: id('e'),
          observedEvidenceCount: '3',
          relation: 'calls',
          sourceModuleId: id('a'),
          targetModuleId: id('b'),
        },
      ],
      relationsTruncated: false,
      snapshotId: id('2'),
      sourceEdgesTruncated: false,
      unmappedEdgeCount: '0',
    },
  },
};

const currentCard: ModuleCardDetailResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    detail: {
      cardId: id('4'),
      confidenceBasisPoints: 8_600,
      coverage: {
        basisPoints: 833,
        coveredFieldCount: 1,
        must: {
          basisPoints: 1_250,
          coveredFieldCount: 1,
          missingFields: [
            'title',
            'paths',
            'responsibilities',
            'publicSurface',
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
      currentIndexRunId: id('1'),
      currentSnapshotId: id('2'),
      fields: [
        {
          evidenceIds: [id('f')],
          kind: 'purpose',
          values: [
            {
              claim: {
                claimId: id('5'),
                confidenceBasisPoints: 8_600,
                evidenceIds: [id('f')],
                kind: 'fact',
                state: 'current',
              },
              value: 'Orchestriert die deterministischen Anwendungsfälle.',
            },
          ],
        },
      ],
      lifecycle: { status: 'current' },
      mapperProfileVersion: 1,
      moduleId: id('a'),
      schemaVersion: 1,
      sourceIndexRunId: id('1'),
      sourceSnapshotId: id('2'),
    },
  },
};

const currentEvidence: ModuleCardEvidenceResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    detail: {
      cardId: id('4'),
      cardLifecycle: { status: 'current' },
      currentIndexRunId: id('1'),
      currentSnapshotId: id('2'),
      evidenceId: id('f'),
      freshness: 'current',
      moduleId: id('a'),
      payload: {
        kind: 'file',
        revision: { contentHash: id('6'), pathHex: '7372632f6c69622e7273' },
      },
      sourceIndexRunId: id('1'),
      sourceSnapshotId: id('2'),
    },
  },
};

const sourcePreview: ProjectMapSourcePreviewResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    preview: {
      highlight: null,
      language: 'rust',
      lineCount: 2,
      pathDisplay: 'src/lib.rs',
      startLine: 10,
      text: 'pub struct RunDeepMap;\nimpl RunDeepMap {}',
      truncatedAfter: true,
      truncatedBefore: true,
    },
  },
};

const runtime: ModuleRuntimeMapResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    map: {
      entrypoints: {
        projectionTruncated: false,
        roots: [],
        storedCount: '0',
        visibleTruncated: false,
      },
      indexRunId: id('1'),
      moduleId: id('a'),
      snapshotId: id('2'),
      tests: { projectionTruncated: false, roots: [], storedCount: '0', visibleTruncated: false },
    },
  },
};

const deepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    configuration: {
      defaultBudget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      maximumBudget: { tokenLimit: 1_000_000, timeLimitMillis: 86_400_000, toolCallLimit: 4_096 },
      minimumBudget: { tokenLimit: 1, timeLimitMillis: 1, toolCallLimit: 1 },
      model: {
        contextTokens: 32_000,
        modelId: 'mapper',
        outputTokens: 4_096,
        profileId: id('7'),
        profileVersion: 1,
        providerId: 'local',
      },
    },
    activity: {
      budget: null,
      confirmedSteps: '0',
      currentModuleId: null,
      events: [],
      failure: null,
      phase: null,
      progress: null,
      publicationSummary: null,
      safeAction: null,
      state: 'idle',
      stepPosition: null,
      targetKind: null,
      totalSteps: '0',
    },
  },
};

function renderWorkspace(overrides: Record<string, unknown> = {}) {
  const baseScene = (
    overviewScene.result as Extract<ProjectMapSceneResponseV1['result'], { status: 'available' }>
  ).scene;
  const sceneLoader = vi.fn(async ({ focusModuleId }: { focusModuleId: string | null }) => {
    if (focusModuleId === null) return overviewScene;
    return {
      ...overviewScene,
      result: {
        status: 'available',
        scene: { ...baseScene, focusModuleId },
      },
    } as ProjectMapSceneResponseV1;
  });
  const props = {
    cardLoader: vi.fn(async () => currentCard),
    deepMapStatusLoader: vi.fn(async () => deepMapStatus),
    evidenceLoader: vi.fn(async () => currentEvidence),
    projectKey: id('9'),
    runtimeLoader: vi.fn(async () => runtime),
    sceneLoader,
    searchLoader: vi.fn(),
    sourcePreviewLoader: vi.fn(async () => sourcePreview),
    ...overrides,
  };
  return { ...render(MapWorkspace, { props }), props, sceneLoader };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('U11 Map workspace', () => {
  it('opens as one map-first surface without legacy tabs and defers module reads', async () => {
    const { props, sceneLoader } = renderWorkspace();
    expect(await screen.findByRole('heading', { name: 'Code Atlas' })).toBeTruthy();
    expect(await screen.findByRole('button', { name: /a3-application, Current/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Recherche' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Explorer' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Modul' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Mapping' })).toBeNull();
    expect(sceneLoader).toHaveBeenCalledWith({ focusModuleId: null });
    expect(props.cardLoader).not.toHaveBeenCalled();
    expect(props.runtimeLoader).not.toHaveBeenCalled();
  });

  it('loads progressive module detail and opens only a revalidated bounded source preview', async () => {
    const { props, sceneLoader } = renderWorkspace();
    await fireEvent.click(await screen.findByRole('button', { name: /a3-application, Current/ }));
    await waitFor(() => expect(props.cardLoader).toHaveBeenCalledWith({ moduleId: id('a') }));
    expect(props.runtimeLoader).toHaveBeenCalledWith({
      entrypointLimit: 20,
      moduleId: id('a'),
      testLimit: 20,
    });
    expect(sceneLoader).toHaveBeenCalledWith({ focusModuleId: id('a') });
    await fireEvent.click(await screen.findByRole('button', { name: 'Evidence öffnen' }));
    await fireEvent.click(
      await screen.findByRole('button', { name: 'Begrenzten Codeausschnitt anzeigen' }),
    );
    expect(await screen.findByText('pub struct RunDeepMap;', { exact: false })).toBeTruthy();
    expect(props.sourcePreviewLoader).toHaveBeenCalledTimes(1);
  });

  it('searches only after submit and starts the Standard preset with its hard budget', async () => {
    const searchLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { search: { hits: [], query: 'runner' }, status: 'available' as const },
    })) as never;
    const starter = vi.fn(async () => ({ accepted: true as const, protocolVersion: 1 as const }));
    renderWorkspace({ deepMapStarter: starter, searchLoader });
    const input = await screen.findByRole('searchbox', { name: 'Code durchsuchen' });
    await fireEvent.input(input, { target: { value: 'runner' } });
    expect(searchLoader).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Suchen' }));
    await waitFor(() => expect(searchLoader).toHaveBeenCalledWith({ query: 'runner' }));

    await fireEvent.click(screen.getByRole('button', { name: /Deep Map/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Deep Map starten' }));
    await waitFor(() =>
      expect(starter).toHaveBeenCalledWith({
        tokenLimit: 32_000,
        timeLimitMillis: 120_000,
        toolCallLimit: 64,
      }),
    );
  });
});

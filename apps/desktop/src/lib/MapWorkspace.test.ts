import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MapWorkspace from './MapWorkspace.svelte';
import type { DeepMapStatusResponseV1 } from './deep-map';
import type {
  ProjectMapAtlasNodeV1,
  ProjectMapAtlasSceneResponseV1,
  ProjectMapAtlasSceneV1,
  ProjectMapEntityContextResponseV1,
  ProjectMapEntitySelectionV1,
  ProjectMapInventoryPageResponseV1,
} from './project-map-atlas';
import type { ProjectMapSourcePreviewResponseV1 } from './project-map-source-preview';

const id = (digit: string): string => digit.repeat(64);
const moduleSelection: ProjectMapEntitySelectionV1 = { kind: 'module', moduleId: id('a') };
const fileSelection: ProjectMapEntitySelectionV1 = {
  evidenceId: id('f'),
  kind: 'file',
  moduleId: id('a'),
  ordinal: 1,
};

const moduleNode: ProjectMapAtlasNodeV1 = {
  claimBadgeCount: 1,
  currentRiskCount: '2',
  detail: '8 Dateien · 20 Symbole',
  dimmed: false,
  displayName: 'a3-application',
  evidenceId: null,
  fileCount: '8',
  kind: 'manifestModule',
  mappingStatus: 'current',
  memberCount: '0',
  nodeId: id('a'),
  parentNodeId: null,
  purpose: 'Orchestriert die deterministischen Anwendungsfälle.',
  rank: 1,
  selection: moduleSelection,
  symbolCount: '20',
  volume: '8',
};
const fileNode: ProjectMapAtlasNodeV1 = {
  ...moduleNode,
  claimBadgeCount: 0,
  currentRiskCount: '0',
  detail: 'RunDeepMap · ExploreProjectMapAtlas',
  displayName: 'lib.rs',
  evidenceId: id('f'),
  fileCount: '1',
  kind: 'file',
  mappingStatus: null,
  nodeId: id('b'),
  parentNodeId: id('a'),
  selection: fileSelection,
  symbolCount: '12',
  volume: '12',
};

function scene(selection: ProjectMapEntitySelectionV1 | null): ProjectMapAtlasSceneV1 {
  const focused = selection !== null;
  return {
    boundariesTruncated: false,
    boundaryCount: '0',
    breadcrumb: focused
      ? [
          { label: 'Projekt', selection: null },
          { label: 'a3-application', selection: moduleSelection },
        ]
      : [{ label: 'Projekt', selection: null }],
    indexRunId: id('1'),
    inspectedEdgeCount: '0',
    level: focused ? 'module' : 'project',
    nodeCount: '1',
    nodes: [focused ? fileNode : moduleNode],
    nodesTruncated: false,
    policyVersion: 1,
    relationCount: '0',
    relations: [],
    relationsTruncated: false,
    selection,
    snapshotId: id('2'),
    sourceEdgesTruncated: false,
    unresolvedCount: '0',
  };
}

const sourcePreview: ProjectMapSourcePreviewResponseV1 = {
  protocolVersion: 1,
  result: {
    preview: {
      highlight: { endColumn: 18, endLine: 10, startColumn: 0, startLine: 10 },
      language: 'rust',
      lineCount: 2,
      pathDisplay: 'crates/a3-application/src/lib.rs',
      startLine: 9,
      text: 'use crate::atlas;\npub struct RunDeepMap;',
      truncatedAfter: true,
      truncatedBefore: true,
    },
    status: 'available',
  },
};

const deepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
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
    status: 'available',
  },
};

function renderWorkspace(deepMapStatusResponse: DeepMapStatusResponseV1 = deepMapStatus) {
  const atlasSceneLoader = vi.fn(async (selection: ProjectMapEntitySelectionV1 | null) => ({
    protocolVersion: 1 as const,
    result: { scene: scene(selection), status: 'available' as const },
  })) as unknown as (
    selection: ProjectMapEntitySelectionV1 | null,
  ) => Promise<ProjectMapAtlasSceneResponseV1>;
  const contextLoader = vi.fn(async (selection: ProjectMapEntitySelectionV1) => {
    const entity = selection.kind === 'module' ? moduleNode : fileNode;
    return {
      protocolVersion: 1 as const,
      result: {
        context: {
          architectureRelationCount: '0',
          architectureRelations: [],
          boundaryCount: '0',
          boundaryNodes: [],
          boundaryRelations: [],
          claims: [],
          documentRelationCount: '0',
          entity,
          indexRunId: id('1'),
          relatedNodes: [],
          relationCounts: [],
          snapshotId: id('2'),
          sourceEdgesTruncated: false,
        },
        status: 'available' as const,
      },
    };
  }) as unknown as (
    selection: ProjectMapEntitySelectionV1,
  ) => Promise<ProjectMapEntityContextResponseV1>;
  const inventoryLoader = vi.fn(async () => ({
    protocolVersion: 1 as const,
    result: {
      page: {
        indexRunId: id('1'),
        items: [fileNode],
        nextCursor: null,
        pageNumber: 1,
        pageSize: 50 as const,
        previousCursor: null,
        selection: moduleSelection,
        snapshotId: id('2'),
        totalCount: '1',
        view: 'files' as const,
      },
      status: 'available' as const,
    },
  })) as unknown as () => Promise<ProjectMapInventoryPageResponseV1>;
  const sourcePreviewLoader = vi.fn(async () => sourcePreview);
  const searchLoader = vi.fn(async () => ({
    protocolVersion: 1 as const,
    result: {
      search: {
        fusionPolicyVersion: 1 as const,
        hits: [],
        indexRunId: id('1'),
        query: 'runner',
        snapshotId: id('2'),
        truncated: false,
      },
      status: 'available' as const,
    },
  }));
  const starter = vi.fn(async () => ({ accepted: true as const, protocolVersion: 1 as const }));
  const props = {
    atlasSceneLoader,
    contextLoader,
    deepMapStarter: starter,
    deepMapStatusLoader: vi.fn(async () => deepMapStatusResponse),
    inventoryLoader,
    publicationKey: id('2'),
    projectKey: id('9'),
    searchLoader,
    sourcePreviewLoader,
  };
  return { ...render(MapWorkspace, { props }), props };
}

afterEach(() => vi.restoreAllMocks());

describe('U12 progressive Code Atlas workspace', () => {
  it('opens full-surface at project level and defers all entity reads', async () => {
    const { props } = renderWorkspace();
    expect(await screen.findByRole('heading', { name: 'Code Atlas' })).toBeTruthy();
    expect(await screen.findByRole('button', { name: /a3-application, Package/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Recherche' })).toBeNull();
    expect(screen.getByText('Fläche 1:8 begrenzt')).toBeTruthy();
    expect(screen.queryByRole('complementary', { name: 'Code Inspector' })).toBeNull();
    expect(props.atlasSceneLoader).toHaveBeenCalledWith(null);
    expect(props.contextLoader).not.toHaveBeenCalled();
  });

  it('reloads the Atlas when a newer atomic index publication becomes visible', async () => {
    const view = renderWorkspace();
    await screen.findByRole('button', { name: /a3-application, Package/ });
    expect(view.props.atlasSceneLoader).toHaveBeenCalledTimes(1);

    await view.rerender({ ...view.props, publicationKey: id('3') });

    await waitFor(() => expect(view.props.atlasSceneLoader).toHaveBeenCalledTimes(2));
    expect(view.props.atlasSceneLoader).toHaveBeenLastCalledWith(null);
  });

  it('separates selection from semantic opening and previews only typed index Evidence', async () => {
    const { props } = renderWorkspace();
    await fireEvent.click(await screen.findByRole('button', { name: /a3-application, Package/ }));
    await waitFor(() => expect(props.contextLoader).toHaveBeenCalledWith(moduleSelection));
    expect(props.atlasSceneLoader).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: 'Modul öffnen' }));
    expect(await screen.findByRole('button', { name: /lib.rs, Datei/ })).toBeTruthy();
    expect(props.atlasSceneLoader).toHaveBeenLastCalledWith(moduleSelection);
    await fireEvent.click(screen.getByRole('button', { name: /lib.rs, Datei/ }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Code anzeigen' }));
    expect(await screen.findByText('pub struct RunDeepMap;', { exact: false })).toBeTruthy();
    expect(props.sourcePreviewLoader).toHaveBeenCalledWith({
      evidence: fileSelection,
      kind: 'index',
    });
  });

  it('retries a failed semantic zoom at the same entity instead of returning to project level', async () => {
    const { props } = renderWorkspace();
    await screen.findByRole('button', { name: /a3-application, Package/ });
    vi.mocked(props.atlasSceneLoader).mockRejectedValueOnce(new Error('transient read failure'));

    await fireEvent.click(screen.getByRole('button', { name: /a3-application, Package/ }));
    await waitFor(() => expect(props.contextLoader).toHaveBeenCalledWith(moduleSelection));
    await fireEvent.click(screen.getByRole('button', { name: 'Modul öffnen' }));
    expect(await screen.findByText('Der Atlas konnte nicht sicher geladen werden.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Erneut laden' }));
    expect(await screen.findByRole('button', { name: /lib.rs, Datei/ })).toBeTruthy();
    expect(props.atlasSceneLoader).toHaveBeenLastCalledWith(moduleSelection);
  });

  it('can recover to the project overview when a focused Atlas scene remains invalid', async () => {
    const { props } = renderWorkspace();
    await screen.findByRole('button', { name: /a3-application, Package/ });
    vi.mocked(props.atlasSceneLoader).mockImplementation(async (selection) => {
      if (selection !== null) {
        throw new Error('invalid focused projection');
      }
      return {
        protocolVersion: 1,
        result: { scene: scene(null), status: 'available' },
      };
    });

    await fireEvent.click(screen.getByRole('button', { name: /a3-application, Package/ }));
    await waitFor(() => expect(props.contextLoader).toHaveBeenCalledWith(moduleSelection));
    await fireEvent.click(screen.getByRole('button', { name: 'Modul öffnen' }));
    expect(await screen.findByText('Der Atlas konnte nicht sicher geladen werden.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Zur Projektübersicht' }));
    expect(await screen.findByRole('button', { name: /a3-application, Package/ })).toBeTruthy();
    expect(props.atlasSceneLoader).toHaveBeenLastCalledWith(null);
  });

  it('returns through the breadcrumb after a successful semantic zoom', async () => {
    const { props } = renderWorkspace();
    await fireEvent.click(await screen.findByRole('button', { name: /a3-application, Package/ }));
    await waitFor(() => expect(props.contextLoader).toHaveBeenCalledWith(moduleSelection));
    await fireEvent.click(screen.getByRole('button', { name: 'Modul öffnen' }));
    await screen.findByRole('button', { name: /lib.rs, Datei/ });

    await fireEvent.click(screen.getByRole('button', { name: 'Projekt' }));
    expect(await screen.findByRole('button', { name: /a3-application, Package/ })).toBeTruthy();
    expect(props.atlasSceneLoader).toHaveBeenLastCalledWith(null);
  });

  it('resizes the open Inspector by mouse or keyboard within the desktop split view', async () => {
    const { props } = renderWorkspace();
    await fireEvent.click(await screen.findByRole('button', { name: /a3-application, Package/ }));
    await waitFor(() => expect(props.contextLoader).toHaveBeenCalledWith(moduleSelection));

    const separator = screen.getByRole('separator', { name: 'Breite des Inspectors ändern' });
    expect(separator.getAttribute('aria-valuenow')).toBe('380');

    await fireEvent.keyDown(separator, { key: 'ArrowLeft' });
    expect(separator.getAttribute('aria-valuenow')).toBe('404');
    await fireEvent.keyDown(separator, { key: 'ArrowRight' });
    expect(separator.getAttribute('aria-valuenow')).toBe('380');

    const workspace = separator.closest('main');
    expect(workspace).not.toBeNull();
    vi.spyOn(workspace!, 'getBoundingClientRect').mockReturnValue({
      bottom: 700,
      height: 700,
      left: 0,
      right: 1_000,
      toJSON: () => ({}),
      top: 0,
      width: 1_000,
      x: 0,
      y: 0,
    });
    await fireEvent.pointerDown(separator, { button: 0, clientX: 500, pointerId: 1 });
    await waitFor(() => expect(separator.getAttribute('aria-valuenow')).toBe('500'));
    await fireEvent.pointerMove(separator, { clientX: 400, pointerId: 1 });
    await waitFor(() => expect(separator.getAttribute('aria-valuenow')).toBe('600'));
    await fireEvent.pointerUp(separator, { clientX: 400, pointerId: 1 });
  });

  it('submits search explicitly and starts the fixed Standard Deep Map preset', async () => {
    const { props } = renderWorkspace();
    const input = await screen.findByRole('searchbox', { name: 'Code durchsuchen' });
    await fireEvent.input(input, { target: { value: 'runner' } });
    expect(props.searchLoader).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Suchen' }));
    await waitFor(() => expect(props.searchLoader).toHaveBeenCalledWith({ query: 'runner' }));
    await fireEvent.click(screen.getByRole('button', { name: /Deep Map/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Deep Map starten' }));
    await waitFor(() =>
      expect(props.deepMapStarter).toHaveBeenCalledWith({
        tokenLimit: 32_000,
        timeLimitMillis: 120_000,
        toolCallLimit: 64,
      }),
    );
  });

  it('opens a safe detailed OpenAI failure when the failed Deep Map status is clicked', async () => {
    if (deepMapStatus.result.status !== 'available') {
      throw new Error('Deep Map test fixture must be available');
    }
    const availableStatus = deepMapStatus.result;
    const failedStatus: DeepMapStatusResponseV1 = {
      ...deepMapStatus,
      result: {
        ...availableStatus,
        activity: {
          ...availableStatus.activity,
          budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
          failure: 'modelRejected',
          state: 'failed',
        },
        configuration: {
          ...availableStatus.configuration,
          model: {
            ...availableStatus.configuration.model,
            modelId: 'gpt-5.4',
            providerId: 'openai',
          },
        },
      },
    };
    renderWorkspace(failedStatus);

    const summary = await screen.findByRole('button', { name: /Deep Map.*Fehlgeschlagen/ });
    expect(screen.queryByText('OpenAI hat die strukturierte Anfrage abgelehnt')).toBeNull();
    await fireEvent.click(summary);

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('OpenAI hat die strukturierte Anfrage abgelehnt');
    expect(alert.textContent).toContain('gpt-5.4');
    expect(alert.textContent).toContain('Mapping-Capability erneut');
    expect(alert.textContent).toContain('modelRejected');
    expect(alert.textContent).not.toContain('provider response');
  });
});

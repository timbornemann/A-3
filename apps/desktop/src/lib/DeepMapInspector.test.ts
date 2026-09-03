import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import DeepMapInspector from './DeepMapInspector.svelte';
import type { ModuleCardDetailResponseV1 } from './module-card-detail';

const id = (digit: string): string => digit.repeat(64);
const runSelection = 'a'.repeat(96);
const moduleSelection = 'b'.repeat(96);

describe('Deep Map live information system', () => {
  it('shows the understandable plan, published Card and exact Atlas impact inline', async () => {
    const onshowinatlas = vi.fn();
    const card: ModuleCardDetailResponseV1 = {
      protocolVersion: 1,
      result: {
        detail: {
          cardId: id('1'),
          confidenceBasisPoints: 9_000,
          coverage: {
            basisPoints: 10_000,
            coveredFieldCount: 1,
            must: {
              basisPoints: 10_000,
              coveredFieldCount: 1,
              missingFields: [],
              totalFieldCount: 1,
            },
            should: {
              basisPoints: 10_000,
              coveredFieldCount: 0,
              missingFields: [],
              totalFieldCount: 0,
            },
            totalFieldCount: 1,
          },
          currentIndexRunId: id('2'),
          currentSnapshotId: id('3'),
          fields: [
            {
              evidenceIds: [id('4')],
              kind: 'purpose',
              values: [
                {
                  claim: {
                    claimId: id('5'),
                    confidenceBasisPoints: 9_000,
                    evidenceIds: [id('4')],
                    kind: 'fact',
                    state: 'current',
                  },
                  value: 'Orchestriert sichere Deep-Map-Läufe.',
                },
              ],
            },
          ],
          lifecycle: { status: 'current' },
          mapperProfileVersion: 1,
          moduleId: id('6'),
          schemaVersion: 1,
          sourceIndexRunId: id('2'),
          sourceSnapshotId: id('3'),
        },
        status: 'available',
      },
    };
    let moduleReadsInFlight = 0;
    let maximumModuleReadsInFlight = 0;
    const exclusiveModuleRead = async <T>(value: T): Promise<T> => {
      moduleReadsInFlight += 1;
      maximumModuleReadsInFlight = Math.max(maximumModuleReadsInFlight, moduleReadsInFlight);
      await new Promise((resolve) => window.setTimeout(resolve, 0));
      moduleReadsInFlight -= 1;
      return value;
    };

    render(DeepMapInspector, {
      props: {
        atlasImpactLoader: vi.fn(() =>
          exclusiveModuleRead({
            protocolVersion: 1 as const,
            result: {
              items: [{ confirmedClaimCount: '1', kind: 'file' as const, label: 'src/lib.rs' }],
              nextCursor: null,
              status: 'available' as const,
              summary: {
                fileCount: '1',
                purpose: 'Orchestriert sichere Deep-Map-Läufe.',
                relationCount: '0',
                riskCount: '0',
                symbolCount: '0',
              },
            },
          }),
        ),
        cardLoader: vi.fn(() => exclusiveModuleRead(card)),
        dashboardLoader: vi.fn(async () => ({
          confirmedSteps: '1',
          currentActivity: null,
          detailsIncomplete: false,
          failure: null,
          freshness: 'current' as const,
          historicalPlanLimited: false,
          phases: [
            { phase: 'planning' as const, state: 'completed' as const },
            { phase: 'exploring' as const, state: 'completed' as const },
            { phase: 'creatingCards' as const, state: 'completed' as const },
            { phase: 'verifying' as const, state: 'completed' as const },
            { phase: 'updatingAtlas' as const, state: 'completed' as const },
          ],
          protocolVersion: 1 as const,
          runSelection,
          startedAtUnixMillis: '1000',
          state: 'completed' as const,
          totalSteps: '1',
          updatedAtUnixMillis: '1200',
        })),
        entriesLoader: vi.fn(async () => ({
          entries: [
            {
              action: 'publishCards' as const,
              confirmed: true,
              failure: null,
              occurredAtUnixMillis: '1200',
              phase: 'publishing' as const,
              result: 'published' as const,
              selection: 'c'.repeat(48),
              sequence: '2',
              state: 'succeeded' as const,
              stepPosition: null,
              targetKind: 'project' as const,
              totalSteps: null,
            },
          ],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        modulesLoader: vi.fn(async () => ({
          modules: [
            {
              cardAvailable: true,
              confirmedSteps: '1',
              displayName: 'a3-application',
              plannedSteps: '1',
              selection: moduleSelection,
              state: 'published' as const,
            },
          ],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        onclose: vi.fn(),
        onshowinatlas,
        open: true,
        runsLoader: vi.fn(async () => ({
          nextCursor: null,
          protocolVersion: 1 as const,
          runs: [
            {
              confirmedSteps: '1',
              detailsIncomplete: false,
              failure: null,
              mode: 'standard' as const,
              selection: runSelection,
              startedAtUnixMillis: '1000',
              state: 'succeeded' as const,
              totalSteps: '1',
              updatedAtUnixMillis: '1200',
            },
          ],
        })),
        sourcePreviewLoader: vi.fn(),
        stepsLoader: vi.fn(() =>
          exclusiveModuleRead({
            historicalDetailsLimited: false,
            nextCursor: null,
            protocolVersion: 1 as const,
            steps: [
              {
                cardFields: ['purpose' as const, 'dependencies' as const],
                position: '1',
                selectionReason: 'centralSymbol' as const,
                state: 'confirmed' as const,
                targetKind: 'symbol' as const,
                targetLabel: 'RunDeepMap',
              },
            ],
          }),
        ),
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: /a3-application/ }));
    expect(await screen.findByText('RunDeepMap')).toBeTruthy();
    await waitFor(() =>
      expect(screen.getAllByText('Orchestriert sichere Deep-Map-Läufe.')).toHaveLength(2),
    );
    expect(screen.getByText('Belegt')).toBeTruthy();
    expect(screen.getByText(/1\s+Quelle/)).toBeTruthy();
    expect(screen.getByText('src/lib.rs')).toBeTruthy();
    expect(screen.queryByText(/9000|gpt|openai|Token/)).toBeNull();
    expect(maximumModuleReadsInFlight).toBe(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Im Atlas zeigen' }));
    await waitFor(() => expect(onshowinatlas).toHaveBeenCalledWith(runSelection, moduleSelection));
  });

  it('loads dashboard, modules and history without overlapping project reads', async () => {
    let readsInFlight = 0;
    let maximumReadsInFlight = 0;
    const exclusiveRead = async <T>(value: T): Promise<T> => {
      readsInFlight += 1;
      maximumReadsInFlight = Math.max(maximumReadsInFlight, readsInFlight);
      await new Promise((resolve) => window.setTimeout(resolve, 0));
      readsInFlight -= 1;
      return value;
    };

    render(DeepMapInspector, {
      props: {
        dashboardLoader: vi.fn(() =>
          exclusiveRead({
            confirmedSteps: '0',
            currentActivity: null,
            detailsIncomplete: false,
            failure: null,
            freshness: 'current' as const,
            historicalPlanLimited: false,
            phases: [
              { phase: 'planning' as const, state: 'stopped' as const },
              { phase: 'exploring' as const, state: 'pending' as const },
              { phase: 'creatingCards' as const, state: 'pending' as const },
              { phase: 'verifying' as const, state: 'pending' as const },
              { phase: 'updatingAtlas' as const, state: 'pending' as const },
            ],
            protocolVersion: 1 as const,
            runSelection,
            startedAtUnixMillis: '1000',
            state: 'interrupted' as const,
            totalSteps: '1',
            updatedAtUnixMillis: '1200',
          }),
        ),
        entriesLoader: vi.fn(() =>
          exclusiveRead({
            entries: [],
            nextCursor: null,
            protocolVersion: 1 as const,
          }),
        ),
        modulesLoader: vi.fn(() =>
          exclusiveRead({
            modules: [
              {
                cardAvailable: false,
                confirmedSteps: '0',
                displayName: 'Repository',
                plannedSteps: '1',
                selection: moduleSelection,
                state: 'incomplete' as const,
              },
            ],
            nextCursor: null,
            protocolVersion: 1 as const,
          }),
        ),
        onclose: vi.fn(),
        open: true,
        runsLoader: vi.fn(async () => ({
          nextCursor: null,
          protocolVersion: 1 as const,
          runs: [
            {
              confirmedSteps: '0',
              detailsIncomplete: false,
              failure: null,
              mode: 'standard' as const,
              selection: runSelection,
              startedAtUnixMillis: '1000',
              state: 'interrupted' as const,
              totalSteps: '1',
              updatedAtUnixMillis: '1200',
            },
          ],
        })),
      },
    });

    expect(await screen.findByRole('button', { name: /Repository/ })).toBeTruthy();
    expect(screen.queryByText(/konnte nicht geladen werden/)).toBeNull();
    expect(maximumReadsInFlight).toBe(1);
  });

  it('reloads the run list when its initial request failed', async () => {
    const runsLoader = vi
      .fn()
      .mockRejectedValueOnce(new Error('temporary read conflict'))
      .mockResolvedValueOnce({ nextCursor: null, protocolVersion: 1 as const, runs: [] });

    render(DeepMapInspector, {
      props: {
        onclose: vi.fn(),
        open: true,
        runsLoader,
      },
    });

    await fireEvent.click(
      await screen.findByRole('button', {
        name: 'Erneut laden',
      }),
    );

    expect(await screen.findByText('Noch keine Deep Map vorhanden')).toBeTruthy();
    expect(runsLoader).toHaveBeenCalledTimes(2);
  });

  it('keeps following an active run when the first dashboard read is temporarily unavailable', async () => {
    const dashboardLoader = vi
      .fn()
      .mockRejectedValueOnce(new Error('write transaction in progress'))
      .mockResolvedValueOnce({
        confirmedSteps: '1',
        currentActivity: {
          action: 'inspect' as const,
          cardFields: ['purpose' as const],
          moduleName: 'a3-application',
          phase: 'exploring' as const,
          selectionReason: 'centralSymbol' as const,
          targetKind: 'symbol' as const,
          targetLabel: 'RunDeepMap',
        },
        detailsIncomplete: false,
        failure: null,
        freshness: 'current' as const,
        historicalPlanLimited: false,
        phases: [
          { phase: 'planning' as const, state: 'completed' as const },
          { phase: 'exploring' as const, state: 'active' as const },
          { phase: 'creatingCards' as const, state: 'pending' as const },
          { phase: 'verifying' as const, state: 'pending' as const },
          { phase: 'updatingAtlas' as const, state: 'pending' as const },
        ],
        protocolVersion: 1 as const,
        runSelection,
        startedAtUnixMillis: '1000',
        state: 'running' as const,
        totalSteps: '2',
        updatedAtUnixMillis: '1200',
      });

    const view = render(DeepMapInspector, {
      props: {
        dashboardLoader,
        entriesLoader: vi.fn(async () => ({
          entries: [],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        modulesLoader: vi.fn(async () => ({
          modules: [],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        onclose: vi.fn(),
        open: true,
        runsLoader: vi.fn(async () => ({
          nextCursor: null,
          protocolVersion: 1 as const,
          runs: [
            {
              confirmedSteps: '0',
              detailsIncomplete: false,
              failure: null,
              mode: 'standard' as const,
              selection: runSelection,
              startedAtUnixMillis: '1000',
              state: 'running' as const,
              totalSteps: '2',
              updatedAtUnixMillis: '1000',
            },
          ],
        })),
      },
    });

    expect(
      await screen.findByText('Der aktuelle Deep-Map-Stand konnte nicht geladen werden.'),
    ).toBeTruthy();
    expect(await screen.findByText('RunDeepMap', {}, { timeout: 2_500 })).toBeTruthy();
    expect(dashboardLoader).toHaveBeenCalledTimes(2);
    expect(
      screen.queryByText('Der aktuelle Deep-Map-Stand konnte nicht geladen werden.'),
    ).toBeNull();
    view.unmount();
  });

  it('selects a newly started run and follows its live activity through completion', async () => {
    const oldRunSelection = 'c'.repeat(96);
    const liveRunSelection = 'd'.repeat(96);
    let liveRunAvailable = false;
    let liveDashboardReads = 0;
    const oldRun = {
      confirmedSteps: '2',
      detailsIncomplete: false,
      failure: null,
      mode: 'standard' as const,
      selection: oldRunSelection,
      startedAtUnixMillis: '1000',
      state: 'succeeded' as const,
      totalSteps: '2',
      updatedAtUnixMillis: '1200',
    };
    const liveRun = {
      confirmedSteps: '0',
      detailsIncomplete: false,
      failure: null,
      mode: 'standard' as const,
      selection: liveRunSelection,
      startedAtUnixMillis: '2000',
      state: 'running' as const,
      totalSteps: '2',
      updatedAtUnixMillis: '2100',
    };
    const runsLoader = vi.fn(async () => ({
      nextCursor: null,
      protocolVersion: 1 as const,
      runs: liveRunAvailable ? [liveRun, oldRun] : [oldRun],
    }));
    const dashboardLoader = vi.fn(async (selection: string) => {
      if (selection === oldRunSelection) {
        return {
          confirmedSteps: '2',
          currentActivity: null,
          detailsIncomplete: false,
          failure: null,
          freshness: 'current' as const,
          historicalPlanLimited: false,
          phases: [
            { phase: 'planning' as const, state: 'completed' as const },
            { phase: 'exploring' as const, state: 'completed' as const },
            { phase: 'creatingCards' as const, state: 'completed' as const },
            { phase: 'verifying' as const, state: 'completed' as const },
            { phase: 'updatingAtlas' as const, state: 'completed' as const },
          ],
          protocolVersion: 1 as const,
          runSelection: oldRunSelection,
          startedAtUnixMillis: '1000',
          state: 'completed' as const,
          totalSteps: '2',
          updatedAtUnixMillis: '1200',
        };
      }
      liveDashboardReads += 1;
      const completed = liveDashboardReads > 1;
      return {
        confirmedSteps: completed ? '2' : '0',
        currentActivity: completed
          ? null
          : {
              action: 'inspect' as const,
              cardFields: ['purpose' as const, 'dataFlows' as const],
              moduleName: 'taskflow',
              phase: 'exploring' as const,
              selectionReason: 'entrypoint' as const,
              targetKind: 'symbol' as const,
              targetLabel: 'run_task',
            },
        detailsIncomplete: false,
        failure: null,
        freshness: 'current' as const,
        historicalPlanLimited: false,
        phases: [
          { phase: 'planning' as const, state: 'completed' as const },
          {
            phase: 'exploring' as const,
            state: completed ? ('completed' as const) : ('active' as const),
          },
          {
            phase: 'creatingCards' as const,
            state: completed ? ('completed' as const) : ('pending' as const),
          },
          {
            phase: 'verifying' as const,
            state: completed ? ('completed' as const) : ('pending' as const),
          },
          {
            phase: 'updatingAtlas' as const,
            state: completed ? ('completed' as const) : ('pending' as const),
          },
        ],
        protocolVersion: 1 as const,
        runSelection: liveRunSelection,
        startedAtUnixMillis: '2000',
        state: completed ? ('completed' as const) : ('running' as const),
        totalSteps: '2',
        updatedAtUnixMillis: completed ? '3000' : '2100',
      };
    });
    const modulesLoader = vi.fn(async () => ({
      modules: [],
      nextCursor: null,
      protocolVersion: 1 as const,
    }));
    const entriesLoader = vi.fn(async () => ({
      entries: [],
      nextCursor: null,
      protocolVersion: 1 as const,
    }));
    const view = render(DeepMapInspector, {
      props: {
        dashboardLoader,
        entriesLoader,
        modulesLoader,
        onclose: vi.fn(),
        open: true,
        runStartedEpoch: 0,
        runsLoader,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Abgeschlossen' })).toBeTruthy();

    liveRunAvailable = true;
    await view.rerender({
      dashboardLoader,
      entriesLoader,
      modulesLoader,
      onclose: vi.fn(),
      open: true,
      runStartedEpoch: 1,
      runsLoader,
    });

    expect(await screen.findByRole('heading', { name: 'Läuft' })).toBeTruthy();
    expect(screen.getByText('run_task')).toBeTruthy();
    expect(
      screen.getByText('A^3 liest die vorhandenen Strukturhinweise zu diesem Ziel.'),
    ).toBeTruthy();
    expect((screen.getByRole('combobox') as HTMLSelectElement).selectedIndex).toBe(0);
    await waitFor(
      () => expect(screen.getByRole('heading', { name: 'Abgeschlossen' })).toBeTruthy(),
      { timeout: 2_500 },
    );
    expect(
      (screen.getByRole('combobox') as HTMLSelectElement).selectedOptions[0]?.textContent,
    ).toContain('Abgeschlossen');
    expect(dashboardLoader).toHaveBeenLastCalledWith(liveRunSelection);
    expect(runsLoader).toHaveBeenCalledTimes(2);
  });

  it('refreshes the visible plan step while an expanded module is being explored', async () => {
    let stepReads = 0;
    const stepsLoader = vi.fn(async () => {
      stepReads += 1;
      return {
        historicalDetailsLimited: false,
        nextCursor: null,
        protocolVersion: 1 as const,
        steps: [
          {
            cardFields: ['purpose' as const],
            position: '1',
            selectionReason: 'centralSymbol' as const,
            state: stepReads === 1 ? ('exploring' as const) : ('confirmed' as const),
            targetKind: 'symbol' as const,
            targetLabel: 'DeepMapRunner',
          },
        ],
      };
    });
    const view = render(DeepMapInspector, {
      props: {
        dashboardLoader: vi.fn(async () => ({
          confirmedSteps: '0',
          currentActivity: {
            action: 'inspect' as const,
            cardFields: ['purpose' as const],
            moduleName: 'a3-application',
            phase: 'exploring' as const,
            selectionReason: 'centralSymbol' as const,
            targetKind: 'symbol' as const,
            targetLabel: 'DeepMapRunner',
          },
          detailsIncomplete: false,
          failure: null,
          freshness: 'current' as const,
          historicalPlanLimited: false,
          phases: [
            { phase: 'planning' as const, state: 'completed' as const },
            { phase: 'exploring' as const, state: 'active' as const },
            { phase: 'creatingCards' as const, state: 'pending' as const },
            { phase: 'verifying' as const, state: 'pending' as const },
            { phase: 'updatingAtlas' as const, state: 'pending' as const },
          ],
          protocolVersion: 1 as const,
          runSelection,
          startedAtUnixMillis: '1000',
          state: 'running' as const,
          totalSteps: '1',
          updatedAtUnixMillis: '1200',
        })),
        entriesLoader: vi.fn(async () => ({
          entries: [],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        modulesLoader: vi.fn(async () => ({
          modules: [
            {
              cardAvailable: false,
              confirmedSteps: stepReads === 0 ? '0' : '1',
              displayName: 'a3-application',
              plannedSteps: '1',
              selection: moduleSelection,
              state: 'exploring' as const,
            },
          ],
          nextCursor: null,
          protocolVersion: 1 as const,
        })),
        onclose: vi.fn(),
        open: true,
        runsLoader: vi.fn(async () => ({
          nextCursor: null,
          protocolVersion: 1 as const,
          runs: [
            {
              confirmedSteps: '0',
              detailsIncomplete: false,
              failure: null,
              mode: 'standard' as const,
              selection: runSelection,
              startedAtUnixMillis: '1000',
              state: 'running' as const,
              totalSteps: '1',
              updatedAtUnixMillis: '1200',
            },
          ],
        })),
        stepsLoader,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: /a3-application/ }));
    expect(await screen.findByText('Wird erkundet')).toBeTruthy();
    await waitFor(() => expect(stepsLoader).toHaveBeenCalledTimes(2), { timeout: 2_500 });
    expect(screen.getByText('Bestätigt')).toBeTruthy();
    view.unmount();
  });
});

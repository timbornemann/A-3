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

    render(DeepMapInspector, {
      props: {
        atlasImpactLoader: vi.fn(async () => ({
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
        })),
        cardLoader: vi.fn(async () => card),
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
        stepsLoader: vi.fn(async () => ({
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
        })),
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: /a3-application/ }));
    expect(await screen.findByText('RunDeepMap')).toBeTruthy();
    expect(await screen.findAllByText('Orchestriert sichere Deep-Map-Läufe.')).toHaveLength(2);
    expect(screen.getByText('Belegt')).toBeTruthy();
    expect(screen.getByText(/1\s+Quelle/)).toBeTruthy();
    expect(screen.getByText('src/lib.rs')).toBeTruthy();
    expect(screen.queryByText(/9000|gpt|openai|Token/)).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Im Atlas zeigen' }));
    await waitFor(() => expect(onshowinatlas).toHaveBeenCalledWith(runSelection, moduleSelection));
  });
});

import { describe, expect, it, vi } from 'vitest';
import {
  parseDashboard,
  parseImpact,
  parseModules,
  parseSteps,
  queryDeepMapModuleSteps,
} from './deep-map-dashboard';

const runSelection = 'a'.repeat(96);
const moduleSelection = 'b'.repeat(96);

const phases = [
  { phase: 'planning', state: 'completed' },
  { phase: 'exploring', state: 'active' },
  { phase: 'creatingCards', state: 'pending' },
  { phase: 'verifying', state: 'pending' },
  { phase: 'updatingAtlas', state: 'pending' },
] as const;

describe('Deep Map dashboard V1 boundary', () => {
  it('accepts the five product phases and rejects technical metadata', () => {
    const payload = {
      confirmedSteps: '1',
      currentActivity: {
        action: 'inspect',
        cardFields: ['purpose', 'dependencies'],
        moduleName: 'a3-application',
        phase: 'exploring',
        selectionReason: 'centralSymbol',
        targetKind: 'symbol',
        targetLabel: 'RunDeepMap',
      },
      detailsIncomplete: false,
      failure: null,
      freshness: 'current',
      historicalPlanLimited: false,
      phases,
      protocolVersion: 1,
      runSelection,
      startedAtUnixMillis: '1000',
      state: 'running',
      totalSteps: '3',
      updatedAtUnixMillis: '1200',
    };

    expect(parseDashboard(payload, runSelection).currentActivity?.targetLabel).toBe('RunDeepMap');
    expect(() => parseDashboard({ ...payload, tokenBudget: 32_000 }, runSelection)).toThrow();
    expect(() =>
      parseDashboard({ ...payload, phases: [...phases].reverse() }, runSelection),
    ).toThrow();
  });

  it('enforces the fixed module, step and Atlas page bounds', () => {
    const module = {
      cardAvailable: false,
      confirmedSteps: '0',
      displayName: 'Modul',
      plannedSteps: '1',
      selection: moduleSelection,
      state: 'planned',
    } as const;
    expect(() =>
      parseModules({ protocolVersion: 1, modules: Array(21).fill(module), nextCursor: null }),
    ).toThrow();

    const step = {
      cardFields: ['purpose'],
      position: '1',
      selectionReason: 'manifest',
      state: 'planned',
      targetKind: 'manifest',
      targetLabel: 'Cargo.toml',
    } as const;
    expect(() =>
      parseSteps({
        protocolVersion: 1,
        steps: Array(51).fill(step),
        nextCursor: null,
        historicalDetailsLimited: false,
      }),
    ).toThrow();

    const item = { confirmedClaimCount: '1', kind: 'file', label: 'src/lib.rs' } as const;
    expect(() =>
      parseImpact({
        protocolVersion: 1,
        result: {
          items: Array(51).fill(item),
          nextCursor: null,
          status: 'available',
          summary: {
            fileCount: '51',
            purpose: null,
            relationCount: '0',
            riskCount: '0',
            symbolCount: '0',
          },
        },
      }),
    ).toThrow();
  });

  it('sends only project-bound opaque selections and cursors', async () => {
    const cursor = 'c'.repeat(48);
    const invoke = vi.fn(async () => ({
      historicalDetailsLimited: false,
      nextCursor: null,
      protocolVersion: 1,
      steps: [],
    }));

    await queryDeepMapModuleSteps(runSelection, moduleSelection, cursor, invoke);

    expect(invoke).toHaveBeenCalledWith('query_deep_map_module_steps', {
      request: { cursor, moduleSelection, protocolVersion: 1, runSelection },
    });
  });
});

import { describe, expect, it, vi } from 'vitest';
import {
  cancelDeepMap,
  parseDeepMapStatusResponseV3,
  pauseDeepMap,
  queryDeepMap,
  queryDeepMapEntries,
  queryDeepMapEntryDetail,
  queryDeepMapRuns,
  resumeDeepMap,
  startDeepMap,
} from './deep-map';

const model = {
  profileId: 'a'.repeat(64),
  profileVersion: 1,
  providerId: 'openai',
  modelId: 'gpt-5.4-mini',
  contextTokens: 128_000,
  outputTokens: 16_384,
};
const progress = {
  action: 'verifyEvidence',
  confirmedSteps: '3',
  phase: 'verifying',
  totalSteps: '6',
};
const readyStatus = {
  protocolVersion: 1,
  result: { lifecycle: { state: 'ready' }, model, status: 'available' },
};
const runSelection = 'a'.repeat(96);
const entrySelection = 'b'.repeat(48);
const runCursor = 'c'.repeat(112);
const run = {
  confirmedSteps: '2',
  detailsIncomplete: false,
  failure: null,
  mode: 'standard',
  selection: runSelection,
  startedAtUnixMillis: '1000',
  state: 'running',
  totalSteps: '6',
  updatedAtUnixMillis: '1200',
};
const entry = {
  action: 'verifyEvidence',
  confirmed: true,
  failure: null,
  occurredAtUnixMillis: '1100',
  phase: 'verifying',
  result: 'confirmed',
  selection: entrySelection,
  sequence: '2',
  state: 'running',
  stepPosition: '2',
  targetKind: 'module',
  totalSteps: '6',
};

describe('Deep Map V2/V3 boundary', () => {
  it('reads the compact status without an event feed or caller budgets', async () => {
    const invokeMock = vi.fn(async () => readyStatus);
    await expect(queryDeepMap(invokeMock)).resolves.toEqual(readyStatus);
    expect(invokeMock).toHaveBeenCalledWith('query_deep_map', {
      request: { protocolVersion: 1 },
    });
  });

  it('starts only one of the three closed modes', async () => {
    const invokeMock = vi.fn(async () => ({ outcome: 'queued', protocolVersion: 1 }));
    await expect(startDeepMap('standard', invokeMock)).resolves.toEqual({
      outcome: 'queued',
      protocolVersion: 1,
    });
    expect(invokeMock).toHaveBeenCalledWith('start_deep_map', {
      request: { mode: 'standard', protocolVersion: 1 },
    });
  });

  it('keeps pause, resume and cancel on strict acknowledgements', async () => {
    const invokeMock = vi.fn(async (command: string, arguments_?: unknown) => {
      void command;
      void arguments_;
      return {
        accepted: true,
        protocolVersion: 1,
      };
    });
    await pauseDeepMap(invokeMock);
    await resumeDeepMap(invokeMock);
    await cancelDeepMap(invokeMock);
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      'pause_deep_map',
      'resume_deep_map',
      'cancel_deep_map',
    ]);
  });

  it('parses current and distinct safe failure states', () => {
    expect(
      parseDeepMapStatusResponseV3({
        protocolVersion: 1,
        result: {
          lifecycle: { cardCount: '4', detailsAvailable: false, state: 'current' },
          model,
          status: 'available',
        },
      }).result,
    ).toMatchObject({ lifecycle: { state: 'current' } });
    expect(
      parseDeepMapStatusResponseV3({
        protocolVersion: 1,
        result: {
          lifecycle: {
            detailsIncomplete: false,
            failure: 'publicationStorage',
            progress,
            state: 'failed',
          },
          model,
          status: 'available',
        },
      }).result,
    ).toMatchObject({ lifecycle: { failure: 'publicationStorage', state: 'failed' } });
  });

  it('rejects unknown status fields and contradictory progress', () => {
    expect(() =>
      parseDeepMapStatusResponseV3({ ...readyStatus, rawProviderPayload: 'secret' }),
    ).toThrow(/field/i);
    expect(() =>
      parseDeepMapStatusResponseV3({
        protocolVersion: 1,
        result: {
          lifecycle: {
            detailsIncomplete: false,
            progress: { ...progress, confirmedSteps: '7' },
            state: 'running',
          },
          model,
          status: 'available',
        },
      }),
    ).toThrow(/progress/i);
  });

  it('uses bounded opaque run and entry pagination contracts', async () => {
    const invokeMock = vi
      .fn()
      .mockResolvedValueOnce({ nextCursor: runCursor, protocolVersion: 1, runs: [run] })
      .mockResolvedValueOnce({ entries: [entry], nextCursor: null, protocolVersion: 1 });
    await expect(queryDeepMapRuns(null, invokeMock)).resolves.toMatchObject({ runs: [run] });
    await expect(queryDeepMapEntries(runSelection, null, invokeMock)).resolves.toMatchObject({
      entries: [entry],
    });
  });

  it('loads only safe detail metadata and rejects invented selections', async () => {
    const detail = {
      durationMillis: '100',
      entry,
      indexReference: '123456abcdef',
      modelId: model.modelId,
      nextAction: null,
      planStopReason: 'coveragePlanned',
      profileId: model.profileId,
      profileVersion: 1,
      protocolVersion: 1,
      providerId: 'openai',
      publicationResult: null,
      run,
      snapshotReference: 'abcdef123456',
      step: {
        confirmed: true,
        coverageFieldCount: 3,
        evidenceRequirement: 'fieldEvidence',
        informationGainBasisPoints: 7500,
        reservedTimeMillis: '750',
        reservedTokens: 512,
        reservedToolCalls: 1,
        seedReason: 'centralSymbol',
        targetKind: 'module',
        verificationMethod: 'publishedIndexEvidence',
      },
      timeBudgetMillis: '120000',
      tokenBudget: 32000,
      toolCallBudget: 64,
    };
    const invokeMock = vi.fn(async () => detail);
    await expect(
      queryDeepMapEntryDetail(runSelection, entrySelection, invokeMock),
    ).resolves.toEqual(detail);
    await expect(queryDeepMapEntryDetail('invented', entrySelection, invokeMock)).rejects.toThrow(
      /selection/i,
    );
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it('rejects oversized pages before they reach the inspector DOM', async () => {
    const invokeMock = vi.fn(async () => ({
      entries: Array.from({ length: 51 }, (_, index) => ({
        ...entry,
        sequence: String(index + 1),
        selection: index.toString(16).padStart(48, '0'),
      })),
      nextCursor: null,
      protocolVersion: 1,
    }));
    await expect(queryDeepMapEntries(runSelection, null, invokeMock)).rejects.toThrow(/page/i);
  });
});

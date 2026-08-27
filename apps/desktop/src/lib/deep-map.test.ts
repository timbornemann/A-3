import { describe, expect, it, vi } from 'vitest';

import {
  cancelDeepMap,
  parseDeepMapStatusResponseV1,
  pauseDeepMap,
  queryDeepMap,
  resumeDeepMap,
  startDeepMap,
} from './deep-map';

const emptyActivityV2 = {
  currentModuleId: null,
  events: [],
  phase: null,
  publicationSummary: null,
  safeAction: null,
  stepPosition: null,
  targetKind: null,
} as const;

function availableResponse(): unknown {
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      configuration: {
        model: {
          profileId: '11'.repeat(32),
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
        ...emptyActivityV2,
        state: 'idle',
        budget: null,
        progress: null,
        failure: null,
        confirmedSteps: '0',
        totalSteps: '0',
      },
    },
  };
}

describe('Deep Map protocol', () => {
  it('queries status without exposing a path, profile choice, or job id', async () => {
    const invokeMock = vi.fn().mockResolvedValue(availableResponse());
    await expect(queryDeepMap(invokeMock)).resolves.toMatchObject({
      result: { status: 'available' },
    });
    expect(invokeMock).toHaveBeenCalledWith('query_deep_map', {
      request: { protocolVersion: 1 },
    });
  });

  it('passes only a validated hard budget to explicit start', async () => {
    const invokeMock = vi.fn().mockResolvedValue({ accepted: true, protocolVersion: 1 });
    const budget = { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 };
    await expect(startDeepMap(budget, invokeMock)).resolves.toEqual({
      accepted: true,
      protocolVersion: 1,
    });
    expect(invokeMock).toHaveBeenCalledWith('start_deep_map', {
      request: { budget, protocolVersion: 1 },
    });
  });

  it('keeps pause, resume, and cancel pathless', async () => {
    const invokeMock = vi.fn().mockResolvedValue({ accepted: true, protocolVersion: 1 });
    await pauseDeepMap(invokeMock);
    await resumeDeepMap(invokeMock);
    await cancelDeepMap(invokeMock);
    expect(invokeMock.mock.calls).toEqual([
      ['pause_deep_map', { request: { protocolVersion: 1 } }],
      ['resume_deep_map', { request: { protocolVersion: 1 } }],
      ['cancel_deep_map', { request: { protocolVersion: 1 } }],
    ]);
  });

  it('rejects unknown fields and contradictory paused state', () => {
    const extra = availableResponse() as Record<string, unknown>;
    expect(() => parseDeepMapStatusResponseV1({ ...extra, rawEndpoint: 'secret' })).toThrow();

    const paused = availableResponse() as {
      result: { activity: Record<string, unknown> };
    };
    paused.result.activity = {
      ...emptyActivityV2,
      state: 'paused',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: null,
      failure: null,
      confirmedSteps: '4',
      totalSteps: '4',
    };
    expect(() => parseDeepMapStatusResponseV1(paused)).toThrow();
  });

  it('accepts only a known content-free failure on failed activity', () => {
    const failed = availableResponse() as {
      result: { activity: Record<string, unknown> };
    };
    failed.result.activity = {
      ...emptyActivityV2,
      state: 'failed',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: null,
      failure: 'modelTimedOut',
      confirmedSteps: '0',
      totalSteps: '0',
    };
    expect(parseDeepMapStatusResponseV1(failed).result).toMatchObject({
      activity: { failure: 'modelTimedOut', state: 'failed' },
    });

    failed.result.activity.failure = 'rawProviderError';
    expect(() => parseDeepMapStatusResponseV1(failed)).toThrow();
  });

  it('rejects oversized or non-monotonic live event feeds', () => {
    const oversized = availableResponse() as {
      result: { activity: Record<string, unknown> };
    };
    oversized.result.activity = {
      ...emptyActivityV2,
      state: 'running',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: { completed: '1', total: '40' },
      failure: null,
      confirmedSteps: '1',
      totalSteps: '40',
      events: Array.from({ length: 33 }, (_, index) => ({
        confirmed: true,
        currentModuleId: 'aa'.repeat(32),
        phase: 'exploring',
        safeAction: 'inspect',
        sequence: String(index + 1),
        stepPosition: String(index + 1),
        targetKind: 'module',
        totalSteps: '40',
      })),
    };
    expect(() => parseDeepMapStatusResponseV1(oversized)).toThrow(/activity|event/i);

    const nonMonotonic = structuredClone(oversized);
    (nonMonotonic.result.activity.events as Array<Record<string, unknown>>).splice(0, 2);
    (nonMonotonic.result.activity.events as Array<Record<string, unknown>>)[1].sequence = '2';
    expect(() => parseDeepMapStatusResponseV1(nonMonotonic)).toThrow(/activity|event/i);
  });
});

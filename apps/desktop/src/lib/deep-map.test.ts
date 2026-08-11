import { describe, expect, it, vi } from 'vitest';

import {
  cancelDeepMap,
  parseDeepMapStatusResponseV1,
  pauseDeepMap,
  queryDeepMap,
  resumeDeepMap,
  startDeepMap,
} from './deep-map';

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
        state: 'idle',
        budget: null,
        progress: null,
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
      state: 'paused',
      budget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      progress: null,
      confirmedSteps: '4',
      totalSteps: '4',
    };
    expect(() => parseDeepMapStatusResponseV1(paused)).toThrow();
  });
});

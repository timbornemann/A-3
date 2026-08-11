import { describe, expect, it, vi } from 'vitest';
import { parseIndexActivityResponseV1, queryIndexActivity } from './index-activity';

const running = {
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
} as const;

describe('index activity V1 boundary', () => {
  it('invokes only the pathless lightweight query', async () => {
    const invoke = vi.fn(async () => running);

    await expect(queryIndexActivity(invoke)).resolves.toEqual(running);
    expect(invoke).toHaveBeenCalledWith('query_index_activity', {
      request: { protocolVersion: 1 },
    });
  });

  it('accepts the six deterministic phases and completed publish state', () => {
    expect(parseIndexActivityResponseV1(running)).toEqual(running);
    expect(
      parseIndexActivityResponseV1({
        protocolVersion: 1,
        result: {
          activity: {
            completedPhases: 6,
            phase: 'publish',
            state: 'succeeded',
            totalPhases: 6,
          },
          status: 'active',
        },
      }),
    ).toBeTruthy();
  });

  it('rejects unknown fields, contradictory phases, and false completion', () => {
    expect(() =>
      parseIndexActivityResponseV1({
        ...running,
        result: { ...running.result, repositoryPath: 'C:\\secret' },
      }),
    ).toThrow();
    expect(() =>
      parseIndexActivityResponseV1({
        protocolVersion: 1,
        result: {
          activity: { completedPhases: 4, phase: 'parse', state: 'running', totalPhases: 6 },
          status: 'active',
        },
      }),
    ).toThrow();
    expect(() =>
      parseIndexActivityResponseV1({
        protocolVersion: 1,
        result: {
          activity: { completedPhases: 5, phase: 'publish', state: 'succeeded', totalPhases: 6 },
          status: 'active',
        },
      }),
    ).toThrow();
  });
});

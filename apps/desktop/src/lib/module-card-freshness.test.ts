import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleCardFreshnessResponseV1,
  queryModuleCardFreshness,
  type ModuleCardFreshnessResponseV1,
} from './module-card-freshness';

const available: ModuleCardFreshnessResponseV1 = {
  protocolVersion: 1,
  result: {
    freshness: {
      counts: {
        needsReviewCount: '1',
        publishedCount: '7',
        staleCount: '2',
        totalCount: '10',
      },
      indexRunId: '11'.repeat(32),
      reasons: [
        { count: '2', reason: 'evidenceChanged', status: 'stale' },
        { count: '1', reason: 'directDependencyChanged', status: 'needsReview' },
      ],
      snapshotId: '22'.repeat(32),
    },
    status: 'available',
  },
};

describe('Module Card freshness protocol', () => {
  it('invokes the pathless command with only the protocol version', async () => {
    const invoke = vi.fn().mockResolvedValue(available);

    await expect(queryModuleCardFreshness(invoke)).resolves.toEqual(available);
    expect(invoke).toHaveBeenCalledWith('query_module_card_freshness', {
      request: { protocolVersion: 1 },
    });
  });

  it('rejects unknown fields and contradictory aggregate counts', () => {
    expect(() =>
      parseModuleCardFreshnessResponseV1({ ...available, authoritativePath: 'C:\\private' }),
    ).toThrow();
    expect(() =>
      parseModuleCardFreshnessResponseV1({
        ...available,
        result: {
          ...available.result,
          freshness: {
            ...('freshness' in available.result ? available.result.freshness : {}),
            counts: {
              needsReviewCount: '1',
              publishedCount: '7',
              staleCount: '2',
              totalCount: '11',
            },
          },
        },
      }),
    ).toThrow('contradictory counts');
  });

  it('rejects illegal status/reason pairs and non-canonical ordering', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const freshness = available.result.freshness;
    expect(() =>
      parseModuleCardFreshnessResponseV1({
        ...available,
        result: {
          freshness: {
            ...freshness,
            reasons: [
              { count: '1', reason: 'directDependencyChanged', status: 'stale' },
              { count: '2', reason: 'evidenceChanged', status: 'stale' },
            ],
          },
          status: 'available',
        },
      }),
    ).toThrow();
    expect(() =>
      parseModuleCardFreshnessResponseV1({
        ...available,
        result: {
          freshness: {
            ...freshness,
            reasons: [...freshness.reasons].reverse(),
          },
          status: 'available',
        },
      }),
    ).toThrow('unordered');
  });
});

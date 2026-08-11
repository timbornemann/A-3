import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleCardDetailResponseV1,
  queryModuleCardDetail,
  type ModuleCardDetailResponseV1,
} from './module-card-detail';

const currentRunId = '11'.repeat(32);
const currentSnapshotId = '22'.repeat(32);
const sourceRunId = '33'.repeat(32);
const sourceSnapshotId = '44'.repeat(32);
const moduleId = '55'.repeat(32);
const cardId = '66'.repeat(32);
const evidenceId = '77'.repeat(32);
const claimId = '88'.repeat(32);

const available: ModuleCardDetailResponseV1 = {
  protocolVersion: 1,
  result: {
    detail: {
      cardId,
      confidenceBasisPoints: 8_000,
      currentIndexRunId: currentRunId,
      currentSnapshotId,
      fields: [
        {
          evidenceIds: [evidenceId],
          kind: 'publicSurface',
          values: [
            {
              claim: {
                claimId,
                confidenceBasisPoints: 7_000,
                evidenceIds: [evidenceId],
                kind: 'fact',
                state: 'stale',
              },
              value: 'exports main',
            },
          ],
        },
      ],
      lifecycle: {
        invalidatedByIndexRunId: currentRunId,
        reason: 'evidenceChanged',
        status: 'stale',
      },
      mapperProfileVersion: 1,
      moduleId,
      schemaVersion: 1,
      sourceIndexRunId: sourceRunId,
      sourceSnapshotId,
    },
    status: 'available',
  },
};

describe('Module Card detail V1 boundary', () => {
  it('invokes only the selected stable module token and preserves stale Fact classification', async () => {
    const invoke = vi.fn().mockResolvedValue(available);

    await expect(queryModuleCardDetail({ moduleId }, invoke)).resolves.toEqual(available);
    expect(invoke).toHaveBeenCalledWith('query_module_card_detail', {
      request: { moduleId, protocolVersion: 1 },
    });
    const parsed = parseModuleCardDetailResponseV1(available);
    expect(parsed.result.status).toBe('available');
    if (parsed.result.status === 'available') {
      expect(parsed.result.detail.fields[0]?.values[0]?.claim).toMatchObject({
        kind: 'fact',
        state: 'stale',
      });
    }
  });

  it('rejects malformed queries and responses for another module', async () => {
    await expect(queryModuleCardDetail({ moduleId: 'AA'.repeat(32) }, vi.fn())).rejects.toThrow(
      /query/,
    );
    await expect(
      queryModuleCardDetail(
        { moduleId },
        vi.fn().mockResolvedValue({
          ...available,
          result: {
            ...available.result,
            detail: {
              ...('detail' in available.result ? available.result.detail : {}),
              moduleId: '99'.repeat(32),
            },
          },
        }),
      ),
    ).rejects.toThrow(/selected module/);
    expect(() =>
      parseModuleCardDetailResponseV1({ ...available, databasePath: 'C:\\private' }),
    ).toThrow(/schema/);
  });

  it('rejects stale Cards whose Fact still appears current', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const detail = available.result.detail;
    expect(() =>
      parseModuleCardDetailResponseV1({
        ...available,
        result: {
          detail: {
            ...detail,
            fields: [
              {
                ...detail.fields[0],
                values: [
                  {
                    ...detail.fields[0]!.values[0],
                    claim: { ...detail.fields[0]!.values[0]!.claim, state: 'current' },
                  },
                ],
              },
            ],
          },
          status: 'available',
        },
      }),
    ).toThrow(/freshness/);
  });

  it('rejects claim evidence outside its field and unordered schema fields', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const detail = available.result.detail;
    expect(() =>
      parseModuleCardDetailResponseV1({
        ...available,
        result: {
          detail: {
            ...detail,
            fields: [
              {
                ...detail.fields[0],
                values: [
                  {
                    ...detail.fields[0]!.values[0],
                    claim: {
                      ...detail.fields[0]!.values[0]!.claim,
                      evidenceIds: ['99'.repeat(32)],
                    },
                  },
                ],
              },
            ],
          },
          status: 'available',
        },
      }),
    ).toThrow(/field evidence/);
    expect(() =>
      parseModuleCardDetailResponseV1({
        ...available,
        result: {
          detail: {
            ...detail,
            fields: [
              { ...detail.fields[0], kind: 'risks' },
              { ...detail.fields[0], kind: 'purpose' },
            ],
          },
          status: 'available',
        },
      }),
    ).toThrow(/unordered/);
  });

  it('accepts precise unavailable states without invented Card content', () => {
    for (const status of [
      'noProject',
      'noPublishedIndex',
      'projectionUnavailable',
      'moduleUnavailable',
      'cardUnavailable',
    ]) {
      expect(parseModuleCardDetailResponseV1({ protocolVersion: 1, result: { status } })).toEqual({
        protocolVersion: 1,
        result: { status },
      });
    }
  });
});

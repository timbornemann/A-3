import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleCardEvidenceResponseV1,
  queryModuleCardEvidence,
  type ModuleCardEvidenceQueryV1,
} from './module-card-evidence';

const query: ModuleCardEvidenceQueryV1 = {
  cardId: '55'.repeat(32),
  currentIndexRunId: '11'.repeat(32),
  currentSnapshotId: '22'.repeat(32),
  evidenceId: '77'.repeat(32),
  moduleId: '66'.repeat(32),
  sourceIndexRunId: '33'.repeat(32),
  sourceSnapshotId: '44'.repeat(32),
};

const revision = { contentHash: '99'.repeat(32), pathHex: '7372632f6c69622e7273' };

function available(payload: Record<string, unknown>, freshness: 'current' | 'stale' = 'current') {
  return {
    protocolVersion: 1,
    result: {
      detail: {
        ...query,
        cardLifecycle:
          freshness === 'stale'
            ? {
                invalidatedByIndexRunId: query.currentIndexRunId,
                reason: 'evidenceChanged',
                status: 'stale',
              }
            : { status: 'current' },
        freshness,
        payload,
      },
      status: 'available',
    },
  };
}

describe('Module Card Evidence Inspector V1 boundary', () => {
  it('sends only visible Card anchors and parses current file Evidence', async () => {
    const invoke = vi.fn(async () => available({ kind: 'file', revision }));
    const response = await queryModuleCardEvidence(query, invoke);

    expect(invoke).toHaveBeenCalledWith('query_module_card_evidence', {
      request: { ...query, protocolVersion: 1 },
    });
    expect(response.result.status).toBe('available');
  });

  it('accepts symbol and stale graph payloads while keeping freshness independent', () => {
    const symbol = parseModuleCardEvidenceResponseV1(
      available({ kind: 'symbol', revision, symbolId: '88'.repeat(32) }),
    );
    expect(symbol.result.status).toBe('available');

    const graph = parseModuleCardEvidenceResponseV1(
      available(
        {
          edge: {
            confidenceBasisPoints: 8_000,
            contentHash: revision.contentHash,
            evidenceId: query.evidenceId,
            pathHex: revision.pathHex,
            provider: 'treeSitter',
            range: {
              end: { column: 12, row: 1 },
              endByte: 20,
              start: { column: 2, row: 1 },
              startByte: 10,
            },
            resolution: 'adapterLocalSymbol',
            source: { kind: 'file', pathHex: revision.pathHex },
            target: { kind: 'symbol', symbolId: '88'.repeat(32) },
          },
          kind: 'graphEdge',
          relation: 'calls',
        },
        'stale',
      ),
    );
    expect(graph.result.status).toBe('available');
    if (graph.result.status === 'available') {
      expect(graph.result.detail.freshness).toBe('stale');
      expect(graph.result.detail.cardLifecycle.status).toBe('stale');
    }
  });

  it('rejects fabricated fields, mismatched graph IDs, and stale Evidence on a current Card', async () => {
    expect(() =>
      parseModuleCardEvidenceResponseV1({
        ...available({ kind: 'file', revision }),
        source: 'raw',
      }),
    ).toThrow(/schema/);
    expect(() =>
      parseModuleCardEvidenceResponseV1(
        available({
          edge: {
            confidenceBasisPoints: 8_000,
            contentHash: revision.contentHash,
            evidenceId: 'aa'.repeat(32),
            pathHex: revision.pathHex,
            provider: 'manifest',
            range: {
              end: { column: 1, row: 1 },
              endByte: 1,
              start: { column: 0, row: 1 },
              startByte: 0,
            },
            resolution: 'adapterFile',
            source: { kind: 'file', pathHex: revision.pathHex },
            target: { kind: 'file', pathHex: revision.pathHex },
          },
          kind: 'graphEdge',
          relation: 'calls',
        }),
      ),
    ).toThrow(/identity/);
    const invalidFreshness = available({ kind: 'file', revision }, 'stale');
    invalidFreshness.result.detail.cardLifecycle = { status: 'current' };
    expect(() => parseModuleCardEvidenceResponseV1(invalidFreshness)).toThrow(/stale Evidence/);

    await expect(
      queryModuleCardEvidence({ ...query, sourceIndexRunId: query.currentIndexRunId }, vi.fn()),
    ).rejects.toThrow(/query/);
    await expect(
      queryModuleCardEvidence(
        query,
        vi.fn(async () =>
          available({ kind: 'file', revision, extra: true } as Record<string, unknown>),
        ),
      ),
    ).rejects.toThrow(/payload/);
  });
});

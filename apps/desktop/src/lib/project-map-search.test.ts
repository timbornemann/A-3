import { describe, expect, it, vi } from 'vitest';
import { parseProjectMapSearchResponseV1, queryProjectMapSearch } from './project-map-search';

const id = (character: string) => character.repeat(64);

function availableResponse() {
  return {
    protocolVersion: 1,
    result: {
      search: {
        fusionPolicyVersion: 1,
        hits: [
          {
            finalScore: 52_478,
            priority: 'exact',
            rank: 1,
            sources: [
              {
                channel: 'exact',
                explanation: 'qualifiedNameExact',
                normalizedScoreBasisPoints: 10_000,
              },
              {
                channel: 'lexical',
                explanation: 'symbolName',
                nativeScore: 80_000,
                normalizedScoreBasisPoints: 8_000,
              },
            ],
            target: {
              evidence: {
                contentHash: id('c'),
                declarationRange: {
                  end: { column: 11, row: 0 },
                  endByte: 11,
                  start: { column: 0, row: 0 },
                  startByte: 0,
                },
                pathDisplay: 'src/lib.rs',
                pathHex: '7372632f6c69622e7273',
              },
              kind: 'symbol',
              name: 'launch',
              qualifiedName: 'crate::launch',
              signature: 'fn launch()',
              symbolId: id('d'),
              symbolKind: 'function',
            },
          },
        ],
        indexRunId: id('a'),
        query: 'launch parser',
        snapshotId: id('b'),
        truncated: true,
      },
      status: 'available',
    },
  };
}

describe('Project Map search V1 boundary', () => {
  it('accepts complete current evidence and auditable exact-plus-lexical provenance', () => {
    const response = parseProjectMapSearchResponseV1(availableResponse());

    expect(response.result.status).toBe('available');
    if (response.result.status === 'available') {
      expect(response.result.search.hits[0].sources.map((source) => source.channel)).toEqual([
        'exact',
        'lexical',
      ]);
      expect(response.result.search.truncated).toBe(true);
    }
  });

  it('rejects an unexplained score, reordered provenance, and unknown fields', () => {
    const wrongScore = structuredClone(availableResponse());
    wrongScore.result.search.hits[0].finalScore += 1;
    expect(() => parseProjectMapSearchResponseV1(wrongScore)).toThrow(/fusion score/i);

    const reordered = structuredClone(availableResponse());
    reordered.result.search.hits[0].sources.reverse();
    expect(() => parseProjectMapSearchResponseV1(reordered)).toThrow(/provenance/i);

    const unknown = { ...availableResponse(), source: 'database' };
    expect(() => parseProjectMapSearchResponseV1(unknown)).toThrow(/does not match V1/i);
  });

  it('rejects semantic proof shapes and inconsistent target evidence', () => {
    const semantic = structuredClone(availableResponse());
    semantic.result.search.hits[0].sources = [
      {
        channel: 'semantic',
        explanation: 'similarity',
        nativeScore: 90_000,
        normalizedScoreBasisPoints: 9_000,
      },
    ] as never;
    expect(() => parseProjectMapSearchResponseV1(semantic)).toThrow(/source explanation/i);

    const missingRange = structuredClone(availableResponse());
    missingRange.result.search.hits[0].target.evidence.declarationRange = null as never;
    expect(() => parseProjectMapSearchResponseV1(missingRange)).toThrow(/evidence target/i);
  });

  it('trims once, invokes the pathless command, and binds the response to the query', async () => {
    const invoke = vi.fn(async () => availableResponse());

    await expect(queryProjectMapSearch({ query: '  launch parser  ' }, invoke)).resolves.toEqual(
      parseProjectMapSearchResponseV1(availableResponse()),
    );
    expect(invoke).toHaveBeenCalledWith('query_project_map_search', {
      request: { protocolVersion: 1, query: 'launch parser' },
    });

    const stale = availableResponse();
    stale.result.search.query = 'another query';
    await expect(
      queryProjectMapSearch({ query: 'launch parser' }, async () => stale),
    ).rejects.toThrow(/does not match its query/i);
  });

  it('rejects empty, short, multiline, and oversized queries before invocation', async () => {
    const invoke = vi.fn();
    for (const query of ['', 'ab', 'parser\nsource', 'x'.repeat(4_097)]) {
      await expect(queryProjectMapSearch({ query }, invoke)).rejects.toThrow(/query/i);
    }
    expect(invoke).not.toHaveBeenCalled();
  });
});

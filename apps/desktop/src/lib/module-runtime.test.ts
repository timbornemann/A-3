import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleRuntimeFlowResponseV1,
  parseModuleRuntimeMapResponseV1,
  queryModuleRuntimeFlow,
  queryModuleRuntimeMap,
  type ModuleRuntimeFlowResponseV1,
  type ModuleRuntimeMapResponseV1,
  type ModuleRuntimeSymbolV1,
} from './module-runtime';

const runId = '11'.repeat(32);
const snapshotId = '22'.repeat(32);
const moduleId = '33'.repeat(32);
const rootId = '44'.repeat(32);
const targetId = '55'.repeat(32);
const evidenceId = '66'.repeat(32);
const contentHash = '77'.repeat(32);
const pathHex = '7372632f6d61696e2e7273';

const range = {
  end: { column: 4, row: 0 },
  endByte: 4,
  start: { column: 0, row: 0 },
  startByte: 0,
};

function symbol(symbolId: string, name: string): ModuleRuntimeSymbolV1 {
  return {
    contentHash,
    evidenceId,
    name,
    pathHex,
    selectionRange: range,
    symbolId,
    symbolKind: 'function',
  };
}

const availableMap = {
  protocolVersion: 1,
  result: {
    map: {
      entrypoints: {
        projectionTruncated: true,
        roots: [{ kind: 'entrypoint', rank: 1, symbol: symbol(rootId, 'main') }],
        storedCount: '1',
        visibleTruncated: true,
      },
      indexRunId: runId,
      moduleId,
      snapshotId,
      tests: {
        projectionTruncated: false,
        roots: [],
        storedCount: '0',
        visibleTruncated: false,
      },
    },
    status: 'available',
  },
} satisfies ModuleRuntimeMapResponseV1;

const edge = {
  evidence: {
    confidenceBasisPoints: 10_000,
    contentHash,
    evidenceId,
    pathHex,
    provider: 'treeSitter' as const,
    range,
    resolution: 'adapterLocalSymbol' as const,
    source: { kind: 'symbol' as const, symbolId: rootId },
    target: { kind: 'symbol' as const, symbolId: targetId },
  },
  relation: 'calls' as const,
};

const availableFlow = {
  protocolVersion: 1,
  result: {
    flow: {
      hits: [{ path: [edge], target: { kind: 'symbol', symbol: symbol(targetId, 'run') } }],
      indexRunId: runId,
      kind: 'entrypointCalls',
      moduleId,
      rootSymbolId: rootId,
      snapshotId,
      truncated: false,
    },
    status: 'available',
  },
} satisfies ModuleRuntimeFlowResponseV1;

describe('module runtime V1 boundary', () => {
  it('sends only bounded map fields and parses exact current root evidence', async () => {
    const invoke = vi.fn().mockResolvedValue(availableMap);

    await expect(
      queryModuleRuntimeMap({ entrypointLimit: 20, moduleId, testLimit: 40 }, invoke),
    ).resolves.toEqual(availableMap);
    expect(invoke).toHaveBeenCalledWith('query_module_runtime_map', {
      request: { entrypointLimit: 20, moduleId, protocolVersion: 1, testLimit: 40 },
    });
    const parsed = parseModuleRuntimeMapResponseV1(availableMap);
    expect(parsed.result.status).toBe('available');
    if (parsed.result.status === 'available') {
      expect(parsed.result.map.entrypoints.roots[0]?.symbol.pathHex).toBe(pathHex);
    }
  });

  it('rejects malformed map queries and contradictory prefix metadata', async () => {
    await expect(
      queryModuleRuntimeMap({ entrypointLimit: 0, moduleId, testLimit: 20 }, vi.fn()),
    ).rejects.toThrow(/query/);
    await expect(
      queryModuleRuntimeMap(
        { entrypointLimit: 20, moduleId: 'aa'.repeat(32).toUpperCase(), testLimit: 20 },
        vi.fn(),
      ),
    ).rejects.toThrow(/query/);
    expect(() =>
      parseModuleRuntimeMapResponseV1({ ...availableMap, repositoryPath: 'C:\\private' }),
    ).toThrow(/response/);
    expect(() =>
      parseModuleRuntimeMapResponseV1({
        ...availableMap,
        result: {
          ...availableMap.result,
          map: {
            ...availableMap.result.map,
            entrypoints: {
              ...availableMap.result.map.entrypoints,
              projectionTruncated: false,
              visibleTruncated: true,
            },
          },
        },
      }),
    ).toThrow(/bounds/);
    expect(() =>
      parseModuleRuntimeMapResponseV1({
        ...availableMap,
        result: {
          ...availableMap.result,
          map: {
            ...availableMap.result.map,
            entrypoints: {
              ...availableMap.result.map.entrypoints,
              roots: [{ ...availableMap.result.map.entrypoints.roots[0], kind: 'test' }],
            },
          },
        },
      }),
    ).toThrow(/root/);
    await expect(
      queryModuleRuntimeMap(
        { entrypointLimit: 1, moduleId, testLimit: 40 },
        vi.fn().mockResolvedValue({
          ...availableMap,
          result: {
            ...availableMap.result,
            map: {
              ...availableMap.result.map,
              entrypoints: {
                ...availableMap.result.map.entrypoints,
                roots: [
                  availableMap.result.map.entrypoints.roots[0],
                  {
                    ...availableMap.result.map.entrypoints.roots[0],
                    rank: 2,
                    symbol: {
                      ...availableMap.result.map.entrypoints.roots[0].symbol,
                      symbolId: 'cd'.repeat(32),
                    },
                  },
                ],
                storedCount: '2',
              },
            },
          },
        }),
      ),
    ).rejects.toThrow(/bounds/);
  });

  it('binds a flow request to visible publication and role tokens', async () => {
    const invoke = vi.fn().mockResolvedValue(availableFlow);
    const query = {
      expectedIndexRunId: runId,
      expectedSnapshotId: snapshotId,
      kind: 'entrypointCalls' as const,
      moduleId,
      resultLimit: 20,
      rootSymbolId: rootId,
    };

    await expect(queryModuleRuntimeFlow(query, invoke)).resolves.toEqual(availableFlow);
    expect(invoke).toHaveBeenCalledWith('query_module_runtime_flow', {
      request: { ...query, protocolVersion: 1 },
    });
  });

  it('rejects wrong relations, disconnected paths, and mismatched targets', () => {
    expect(() =>
      parseModuleRuntimeFlowResponseV1({
        ...availableFlow,
        result: {
          ...availableFlow.result,
          flow: {
            ...availableFlow.result.flow,
            hits: [
              { ...availableFlow.result.flow.hits[0], path: [{ ...edge, relation: 'tests' }] },
            ],
          },
        },
      }),
    ).toThrow(/relation/);
    expect(() =>
      parseModuleRuntimeFlowResponseV1({
        ...availableFlow,
        result: {
          ...availableFlow.result,
          flow: {
            ...availableFlow.result.flow,
            hits: [
              {
                ...availableFlow.result.flow.hits[0],
                path: [
                  {
                    ...edge,
                    evidence: {
                      ...edge.evidence,
                      source: { kind: 'symbol', symbolId: '88'.repeat(32) },
                    },
                  },
                ],
              },
            ],
          },
        },
      }),
    ).toThrow(/disconnected/);
    expect(() =>
      parseModuleRuntimeFlowResponseV1({
        ...availableFlow,
        result: {
          ...availableFlow.result,
          flow: {
            ...availableFlow.result.flow,
            hits: [
              {
                ...availableFlow.result.flow.hits[0],
                target: { kind: 'symbol', symbol: symbol('99'.repeat(32), 'other') },
              },
            ],
          },
        },
      }),
    ).toThrow(/target/);
  });

  it('accepts precise unavailable states without invented flow evidence', () => {
    for (const status of [
      'noProject',
      'noPublishedIndex',
      'projectionUnavailable',
      'publicationChanged',
      'moduleUnavailable',
      'rootUnavailable',
    ]) {
      expect(parseModuleRuntimeFlowResponseV1({ protocolVersion: 1, result: { status } })).toEqual({
        protocolVersion: 1,
        result: { status },
      });
    }
  });
});

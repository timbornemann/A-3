import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleDependencyGraphResponseV1,
  queryModuleDependencyGraph,
  type ModuleDependencyGraphResponseV1,
} from './module-dependency-graph';

const center = '33'.repeat(32);
const neighbor = '44'.repeat(32);
const node = (moduleId: string, name: string) => ({
  kind: 'pathBoundary' as const,
  moduleId,
  name,
  nameTruncated: false,
  representativeEvidence: {
    contentHash: '55'.repeat(32),
    evidenceId: '66'.repeat(32),
    pathHex: name === 'src' ? '7372632f6c69622e7273' : '746f6f6c732f6c69622e7273',
  },
  rootPathHex: name === 'src' ? '737263' : '746f6f6c73',
});

const edge = {
  observedEvidenceCount: '2',
  relation: 'imports' as const,
  representativeEvidence: {
    confidenceBasisPoints: 10_000,
    contentHash: '55'.repeat(32),
    evidenceId: '77'.repeat(32),
    pathHex: '7372632f6c69622e7273',
    provider: 'treeSitter' as const,
    range: {
      end: { column: 8, row: 1 },
      endByte: 16,
      start: { column: 0, row: 1 },
      startByte: 8,
    },
    resolution: 'adapterFile' as const,
    source: { kind: 'symbol' as const, symbolId: '88'.repeat(32) },
    target: { kind: 'file' as const, pathHex: '746f6f6c732f6c69622e7273' },
  },
  sourceModuleId: center,
  targetModuleId: neighbor,
};

const available: ModuleDependencyGraphResponseV1 = {
  protocolVersion: 1,
  result: {
    graph: {
      centerModuleId: center,
      edges: [edge],
      edgesTruncated: false,
      indexRunId: '11'.repeat(32),
      inspectedEdgeCount: '3',
      nodes: [node(center, 'src'), node(neighbor, 'tools')],
      nodesTruncated: false,
      observedEdgeGroupCount: '1',
      observedNeighborCount: '1',
      snapshotId: '22'.repeat(32),
      sourceEdgesTruncated: false,
      unmappedEdgeCount: '0',
    },
    status: 'available',
  },
};

describe('Module dependency graph protocol', () => {
  it('invokes only the bounded stable-ID command', async () => {
    const invoke = vi.fn().mockResolvedValue(available);
    await expect(
      queryModuleDependencyGraph({ centerModuleId: center, nodeLimit: 50 }, invoke),
    ).resolves.toEqual(available);
    expect(invoke).toHaveBeenCalledWith('query_module_dependency_graph', {
      request: { centerModuleId: center, nodeLimit: 50, protocolVersion: 1 },
    });
  });

  it('rejects unknown fields, uppercase IDs, and unsafe limits', async () => {
    expect(() =>
      parseModuleDependencyGraphResponseV1({ ...available, repositoryPath: 'C:\\private' }),
    ).toThrow('schema');
    await expect(
      queryModuleDependencyGraph({ centerModuleId: 'AA'.repeat(32), nodeLimit: 50 }, vi.fn()),
    ).rejects.toThrow('query');
    await expect(
      queryModuleDependencyGraph({ centerModuleId: center, nodeLimit: 101 }, vi.fn()),
    ).rejects.toThrow('query');
  });

  it('rejects missing center, unordered nodes, and edges outside the selected neighborhood', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const graph = available.result.graph;
    for (const changed of [
      { ...graph, centerModuleId: '99'.repeat(32) },
      { ...graph, nodes: [...graph.nodes].reverse() },
      { ...graph, edges: [{ ...edge, sourceModuleId: '99'.repeat(32) }] },
    ]) {
      expect(() =>
        parseModuleDependencyGraphResponseV1({
          ...available,
          result: { graph: changed, status: 'available' },
        }),
      ).toThrow();
    }
  });

  it('requires exact node, edge-group, scan, and unmapped truncation semantics', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const graph = available.result.graph;
    for (const changed of [
      { ...graph, nodesTruncated: true },
      { ...graph, edgesTruncated: true },
      { ...graph, sourceEdgesTruncated: true },
      { ...graph, unmappedEdgeCount: '4' },
      { ...graph, observedNeighborCount: '4', nodesTruncated: true },
      { ...graph, observedEdgeGroupCount: '4', edgesTruncated: true },
      { ...graph, inspectedEdgeCount: '4097' },
    ]) {
      expect(() =>
        parseModuleDependencyGraphResponseV1({
          ...available,
          result: { graph: changed, status: 'available' },
        }),
      ).toThrow();
    }
  });

  it('validates exact evidence IDs, endpoints, ranges, confidence, and counts', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const graph = available.result.graph;
    for (const changedEdge of [
      { ...edge, observedEvidenceCount: '0' },
      { ...edge, observedEvidenceCount: '4' },
      {
        ...edge,
        representativeEvidence: { ...edge.representativeEvidence, evidenceId: 'AA'.repeat(32) },
      },
      {
        ...edge,
        representativeEvidence: {
          ...edge.representativeEvidence,
          confidenceBasisPoints: 10_001,
        },
      },
      {
        ...edge,
        representativeEvidence: {
          ...edge.representativeEvidence,
          range: { ...edge.representativeEvidence.range, endByte: 1 },
        },
      },
    ]) {
      expect(() =>
        parseModuleDependencyGraphResponseV1({
          ...available,
          result: { graph: { ...graph, edges: [changedEdge] }, status: 'available' },
        }),
      ).toThrow();
    }
  });
});

import { describe, expect, it, vi } from 'vitest';
import {
  parseAtlasSceneResponse,
  parseFlowResponse,
  parseInventoryResponse,
  queryProjectMapAtlasScene,
} from './project-map-atlas';

const id = (digit: string): string => digit.repeat(64);
const selection = { kind: 'module' as const, moduleId: id('a') };
const node = {
  claimBadgeCount: 0,
  currentRiskCount: '0',
  detail: '1 Datei · 2 Symbole',
  dimmed: false,
  displayName: 'application',
  evidenceId: null,
  fileCount: '1',
  kind: 'manifestModule',
  mappingStatus: 'current',
  memberCount: '0',
  nodeId: id('b'),
  parentNodeId: null,
  purpose: null,
  rank: 1,
  selection,
  symbolCount: '2',
  volume: '1',
};

function response() {
  return {
    protocolVersion: 1,
    result: {
      scene: {
        boundariesTruncated: false,
        boundaryCount: '0',
        breadcrumb: [{ label: 'Projekt', selection: null }],
        indexRunId: id('1'),
        inspectedEdgeCount: '0',
        level: 'project',
        nodeCount: '1',
        nodes: [node],
        nodesTruncated: false,
        policyVersion: 1,
        relationCount: '0',
        relations: [],
        relationsTruncated: false,
        selection: null,
        snapshotId: id('2'),
        sourceEdgesTruncated: false,
        unresolvedCount: '0',
      },
      status: 'available',
    },
  };
}

describe('progressive Atlas V1 decoder', () => {
  it('sends only a typed Core-issued selection and no limits or paths', async () => {
    const invoke = vi.fn().mockResolvedValue(response());
    await queryProjectMapAtlasScene(selection, invoke);
    expect(invoke).toHaveBeenCalledWith('query_project_map_atlas_scene', {
      request: { protocolVersion: 1, selection },
    });
  });

  it('rejects unknown fields, duplicate IDs, contradictory counts, and invalid breadcrumbs', () => {
    const unknown = response();
    Object.assign(unknown.result.scene, { limit: 500 });
    expect(() => parseAtlasSceneResponse(unknown)).toThrow();

    const duplicate = response();
    duplicate.result.scene.nodes.push({ ...node, rank: 2 });
    duplicate.result.scene.nodeCount = '2';
    expect(() => parseAtlasSceneResponse(duplicate)).toThrow();

    const contradictory = response();
    contradictory.result.scene.nodeCount = '2';
    expect(() => parseAtlasSceneResponse(contradictory)).toThrow();

    const breadcrumb = response();
    breadcrumb.result.scene.breadcrumb = [];
    expect(() => parseAtlasSceneResponse(breadcrumb)).toThrow();
  });

  it('requires bounded exact claim badges on architecture routes', () => {
    const routed = response();
    routed.result.scene.nodes.push({ ...node, nodeId: id('c'), rank: 2 });
    routed.result.scene.nodeCount = '2';
    routed.result.scene.relationCount = '1';
    Object.assign(routed.result.scene, {
      relations: [
        {
          claimBadgeCount: 2,
          confidenceBasisPoints: 9_000,
          evidence: {
            edgeSequence: '1',
            evidenceId: id('e'),
            kind: 'relation',
            moduleId: id('a'),
          },
          evidenceCount: '1',
          provider: 'treeSitter',
          relation: 'imports',
          sourceNodeId: id('b'),
          targetNodeId: id('c'),
          uncertainty: null,
        },
      ],
    });
    expect(parseAtlasSceneResponse(routed).result.status).toBe('available');
    delete (routed.result.scene.relations[0] as Record<string, unknown>).claimBadgeCount;
    expect(() => parseAtlasSceneResponse(routed)).toThrow();
  });

  it('rejects oversized inventory pages and flow paths beyond two hops', () => {
    const inventory = {
      protocolVersion: 1,
      result: {
        page: {
          indexRunId: id('1'),
          items: Array.from({ length: 51 }, (_, index) => ({
            ...node,
            nodeId: index.toString(16).padStart(64, '0'),
            rank: index + 1,
          })),
          nextCursor: null,
          pageNumber: 1,
          pageSize: 50,
          previousCursor: null,
          selection,
          snapshotId: id('2'),
          totalCount: '51',
          view: 'files',
        },
        status: 'available',
      },
    };
    expect(() => parseInventoryResponse(inventory)).toThrow();

    const evidence = { evidenceId: id('e'), kind: 'file', moduleId: id('a'), ordinal: 1 };
    const step = { evidence, relation: 'calls', sourceNodeId: id('b'), targetNodeId: id('c') };
    const flow = {
      protocolVersion: 1,
      result: {
        flow: {
          indexRunId: id('1'),
          inspectedEdgeCount: '3',
          nodes: [{ ...node, nodeId: id('c'), selection: evidence }],
          preset: 'callees',
          root: node,
          snapshotId: id('2'),
          sourceEdgesTruncated: false,
          targetCount: '1',
          targets: [{ depth: 3, nodeId: id('c'), path: [step, step, step] }],
          targetsTruncated: false,
        },
        status: 'available',
      },
    };
    expect(() => parseFlowResponse(flow)).toThrow();
  });
});

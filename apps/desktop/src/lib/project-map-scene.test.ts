import { describe, expect, it, vi } from 'vitest';
import {
  parseProjectMapSceneResponseV1,
  queryProjectMapScene,
  type ProjectMapSceneModuleV1,
} from './project-map-scene';

const id = (digit: string): string => digit.repeat(64);

function module(moduleId: string, rank: number) {
  return {
    cardBinding: null,
    cardCoverageBasisPoints: null,
    centralSymbolCount: '3',
    displayName: `module-${rank}`,
    entrypointCount: '1',
    fileCount: '2',
    kind: 'pathBoundary',
    manifestCount: '0',
    mappingStatus: 'unmapped',
    moduleId,
    parentModuleId: null,
    rank,
    representativeEvidenceId: id('a'),
    symbolCount: '5',
    testCount: '1',
  };
}

function response() {
  return {
    protocolVersion: 1,
    result: {
      scene: {
        focusModuleId: null,
        indexRunId: id('1'),
        inspectedEdgeCount: '2',
        modules: [module(id('3'), 1), module(id('4'), 2)],
        modulesTruncated: false,
        observedRelationGroupCount: '1',
        policyVersion: 'v1',
        primaryModuleCount: '2',
        relations: [
          {
            evidenceId: null,
            observedEvidenceCount: '2',
            relation: 'imports',
            sourceModuleId: id('3'),
            targetModuleId: id('4'),
          },
        ],
        relationsTruncated: false,
        snapshotId: id('2'),
        sourceEdgesTruncated: false,
        unmappedEdgeCount: '0',
      },
      status: 'available',
    },
  };
}

describe('Project Map scene V1', () => {
  it('submits only protocol version and optional focus', async () => {
    const invoke = vi.fn().mockResolvedValue(response());
    await queryProjectMapScene({ focusModuleId: null }, invoke);
    expect(invoke).toHaveBeenCalledWith('query_project_map_scene', {
      request: { focusModuleId: null, protocolVersion: 1 },
    });
  });

  it('rejects unknown fields and contradictory relation endpoints', () => {
    const unknown = response();
    Object.assign(unknown.result.scene, { limit: 500 });
    expect(() => parseProjectMapSceneResponseV1(unknown)).toThrow();

    const outside = response();
    outside.result.scene.relations[0]!.targetModuleId = id('5');
    expect(() => parseProjectMapSceneResponseV1(outside)).toThrow();
  });

  it('rejects oversized scenes and mixed mapping state', () => {
    const oversized = response();
    oversized.result.scene.modules = Array.from({ length: 65 }, (_, index) =>
      module(index.toString(16).padStart(64, '0'), index + 1),
    );
    oversized.result.scene.primaryModuleCount = '65';
    expect(() => parseProjectMapSceneResponseV1(oversized)).toThrow();

    const mixed = response();
    (mixed.result.scene.modules[0] as ProjectMapSceneModuleV1).cardCoverageBasisPoints = 5_000;
    expect(() => parseProjectMapSceneResponseV1(mixed)).toThrow();
  });
});

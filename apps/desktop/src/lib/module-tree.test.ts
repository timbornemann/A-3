import { describe, expect, it, vi } from 'vitest';
import {
  parseModuleTreeResponseV1,
  queryModuleTree,
  type ModuleTreeResponseV1,
} from './module-tree';

const manifestEntry = {
  boundaryEvidence: {
    manifestRevision: {
      contentHash: '55'.repeat(32),
      pathHex: '436172676f2e746f6d6c',
    },
    representativeRevision: {
      contentHash: '66'.repeat(32),
      pathHex: '7372632f6c69622e7273',
    },
  },
  centralSymbols: { count: '1', truncated: false },
  childState: 'hasChildren' as const,
  entrypoints: { count: '1', truncated: false },
  fileCount: '1',
  kind: 'manifestBoundary' as const,
  manifestCount: '1',
  moduleId: '33'.repeat(32),
  name: 'Repository',
  nameTruncated: false,
  rootPathHex: null,
  symbolCount: '2',
  tests: { count: '0', truncated: false },
};

const pathEntry = {
  boundaryEvidence: {
    manifestRevision: null,
    representativeRevision: {
      contentHash: '77'.repeat(32),
      pathHex: '746f6f6c732f6d61696e2e7273',
    },
  },
  centralSymbols: { count: '1', truncated: true },
  childState: 'leaf' as const,
  entrypoints: { count: '0', truncated: false },
  fileCount: '1',
  kind: 'pathBoundary' as const,
  manifestCount: '0',
  moduleId: '44'.repeat(32),
  name: 'tools',
  nameTruncated: false,
  rootPathHex: '746f6f6c73',
  symbolCount: '1',
  tests: { count: '0', truncated: false },
};

const available: ModuleTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      entries: [manifestEntry, pathEntry],
      graphCommunityCount: '1',
      indexRunId: '11'.repeat(32),
      nextAfterModuleId: pathEntry.moduleId,
      parentModuleId: null,
      primaryModuleCount: '3',
      snapshotId: '22'.repeat(32),
    },
    status: 'available',
  },
};

describe('Module tree protocol', () => {
  it('invokes the bounded active-project command with stable IDs only', async () => {
    const invoke = vi.fn().mockResolvedValue(available);
    const query = {
      afterModuleId: '22'.repeat(32),
      limit: 50,
      parentModuleId: '11'.repeat(32),
    };

    await expect(queryModuleTree(query, invoke)).resolves.toEqual(available);
    expect(invoke).toHaveBeenCalledWith('query_module_tree', {
      request: { ...query, protocolVersion: 1 },
    });
  });

  it('rejects unknown fields, non-canonical IDs, and unsafe limits', async () => {
    expect(() =>
      parseModuleTreeResponseV1({ ...available, repositoryPath: 'C:\\private' }),
    ).toThrow('schema');
    await expect(
      queryModuleTree({ afterModuleId: null, limit: 50, parentModuleId: 'AA'.repeat(32) }, vi.fn()),
    ).rejects.toThrow('query');
    await expect(
      queryModuleTree({ afterModuleId: null, limit: 101, parentModuleId: null }, vi.fn()),
    ).rejects.toThrow('query');
  });

  it('rejects unordered, duplicate, parent-loop, and cursor-inconsistent nodes', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const page = available.result.page;
    for (const changedPage of [
      { ...page, entries: [...page.entries].reverse() },
      { ...page, entries: [page.entries[0], page.entries[0]] },
      { ...page, nextAfterModuleId: '99'.repeat(32) },
      { ...page, parentModuleId: manifestEntry.moduleId },
    ]) {
      expect(() =>
        parseModuleTreeResponseV1({
          ...available,
          result: { page: changedPage, status: 'available' },
        }),
      ).toThrow();
    }
  });

  it('requires exact manifest, representative, and bounded feature evidence', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const page = available.result.page;
    for (const changedEntry of [
      { ...manifestEntry, manifestCount: '0' },
      {
        ...manifestEntry,
        boundaryEvidence: { ...manifestEntry.boundaryEvidence, representativeRevision: null },
      },
      {
        ...pathEntry,
        boundaryEvidence: {
          ...pathEntry.boundaryEvidence,
          manifestRevision: manifestEntry.boundaryEvidence.manifestRevision,
        },
      },
      { ...pathEntry, centralSymbols: { count: '0', truncated: true } },
      { ...pathEntry, tests: { count: '2', truncated: false } },
    ]) {
      expect(() =>
        parseModuleTreeResponseV1({
          ...available,
          result: {
            page: { ...page, entries: [changedEntry], nextAfterModuleId: null },
            status: 'available',
          },
        }),
      ).toThrow();
    }
  });

  it('keeps projection absence distinct from an empty current projection', () => {
    expect(
      parseModuleTreeResponseV1({
        protocolVersion: 1,
        result: { status: 'projectionUnavailable' },
      }).result.status,
    ).toBe('projectionUnavailable');

    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    expect(
      parseModuleTreeResponseV1({
        ...available,
        result: {
          page: {
            ...available.result.page,
            entries: [],
            nextAfterModuleId: null,
            primaryModuleCount: '0',
          },
          status: 'available',
        },
      }).result.status,
    ).toBe('available');
  });
});

import { describe, expect, it, vi } from 'vitest';
import {
  parseRepositoryTreeResponseV1,
  queryRepositoryTree,
  type RepositoryTreeResponseV1,
} from './repository-tree';

const available: RepositoryTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      directoryPathHex: '737263',
      entries: [
        {
          contentHash: '33'.repeat(32),
          descendantFileCount: '1',
          kind: 'file',
          name: 'lib.rs',
          nameTruncated: false,
          pathHex: '7372632f6c69622e7273',
        },
        {
          contentHash: null,
          descendantFileCount: '2',
          kind: 'directory',
          name: 'nested',
          nameTruncated: false,
          pathHex: '7372632f6e6573746564',
        },
      ],
      indexRunId: '11'.repeat(32),
      nextAfterNameHex: '6e6573746564',
      snapshotId: '22'.repeat(32),
    },
    status: 'available',
  },
};

describe('Repository tree protocol', () => {
  it('invokes the bounded indexed command with no filesystem path capability', async () => {
    const invoke = vi.fn().mockResolvedValue(available);
    const query = { afterNameHex: null, directoryPathHex: '737263', limit: 50 };

    await expect(queryRepositoryTree(query, invoke)).resolves.toEqual(available);
    expect(invoke).toHaveBeenCalledWith('query_repository_tree', {
      request: { ...query, protocolVersion: 1 },
    });
  });

  it('rejects unknown fields, non-canonical tokens, and unsafe limits', async () => {
    expect(() =>
      parseRepositoryTreeResponseV1({ ...available, authoritativePath: 'C:\\private' }),
    ).toThrow();
    await expect(
      queryRepositoryTree(
        { afterNameHex: null, directoryPathHex: 'C:/private', limit: 50 },
        vi.fn(),
      ),
    ).rejects.toThrow('query');
    await expect(
      queryRepositoryTree({ afterNameHex: null, directoryPathHex: null, limit: 101 }, vi.fn()),
    ).rejects.toThrow('query');
  });

  it('rejects indirect, unordered, and cursor-inconsistent children', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const page = available.result.page;
    expect(() =>
      parseRepositoryTreeResponseV1({
        ...available,
        result: {
          page: {
            ...page,
            entries: [
              {
                ...page.entries[0],
                pathHex: '7372632f6e65737465642f6c69622e7273',
              },
            ],
            nextAfterNameHex: null,
          },
          status: 'available',
        },
      }),
    ).toThrow('direct child');
    expect(() =>
      parseRepositoryTreeResponseV1({
        ...available,
        result: {
          page: { ...page, entries: [...page.entries].reverse() },
          status: 'available',
        },
      }),
    ).toThrow('unordered');
    expect(() =>
      parseRepositoryTreeResponseV1({
        ...available,
        result: {
          page: { ...page, nextAfterNameHex: '6f74686572' },
          status: 'available',
        },
      }),
    ).toThrow('next cursor');
  });

  it('requires exact file evidence and lossless unsigned counts', () => {
    if (available.result.status !== 'available') throw new Error('fixture is unavailable');
    const page = available.result.page;
    expect(() =>
      parseRepositoryTreeResponseV1({
        ...available,
        result: {
          page: {
            ...page,
            entries: [{ ...page.entries[0], contentHash: null }],
            nextAfterNameHex: null,
          },
          status: 'available',
        },
      }),
    ).toThrow('evidence');
    expect(() =>
      parseRepositoryTreeResponseV1({
        ...available,
        result: {
          page: {
            ...page,
            entries: [{ ...page.entries[1], descendantFileCount: '01' }],
            nextAfterNameHex: null,
          },
          status: 'available',
        },
      }),
    ).toThrow('entry');
  });
});

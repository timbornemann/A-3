import { describe, expect, it, vi } from 'vitest';
import { parseProjectStatusResponseV1, queryProjectStatus } from './project-status';

const project = {
  head: { kind: 'born', objectId: 'a'.repeat(40), reference: 'refs/heads/main' },
  repositoryId: '1'.repeat(64),
  worktreeId: '2'.repeat(64),
  worktreeRootDisplay: '/worktree',
};

const active = {
  protocolVersion: 1,
  result: {
    index: {
      latestAttemptSnapshotId: '4'.repeat(64),
      latestSnapshot: { generation: '7', snapshotId: '4'.repeat(64) },
      publishedSnapshotId: '4'.repeat(64),
      state: 'published',
    },
    project,
    projectId: '3'.repeat(64),
    rebuildState: 'idle',
    status: 'active',
    storageBytes: '4096',
  },
};

describe('project status protocol', () => {
  it('sends only the protocol version and validates an active projection', async () => {
    const invoke = vi.fn(async () => active);

    await expect(queryProjectStatus(invoke)).resolves.toEqual(active);
    expect(invoke).toHaveBeenCalledWith('query_project_status', {
      request: { protocolVersion: 1 },
    });
  });

  it('accepts the exact no-project state', () => {
    expect(
      parseProjectStatusResponseV1({ protocolVersion: 1, result: { status: 'noProject' } }),
    ).toEqual({ protocolVersion: 1, result: { status: 'noProject' } });
  });

  it('rejects unknown fields and inconsistent or lossy index metadata', () => {
    expect(() =>
      parseProjectStatusResponseV1({ ...active, authoritativePath: '/private' }),
    ).toThrow();
    expect(() =>
      parseProjectStatusResponseV1({
        ...active,
        result: { ...active.result, storageBytes: '18446744073709551616' },
      }),
    ).toThrow();
    expect(() =>
      parseProjectStatusResponseV1({
        ...active,
        result: {
          ...active.result,
          index: { ...active.result.index, latestAttemptSnapshotId: null },
        },
      }),
    ).toThrow();
    expect(() =>
      parseProjectStatusResponseV1({
        ...active,
        result: {
          ...active.result,
          index: {
            ...active.result.index,
            latestSnapshot: {
              ...active.result.index.latestSnapshot,
              generation: '9223372036854775808',
            },
          },
        },
      }),
    ).toThrow();
  });
});

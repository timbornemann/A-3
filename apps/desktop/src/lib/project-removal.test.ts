import { describe, expect, it, vi } from 'vitest';
import { parseRemoveProjectResponseV1, removeProject } from './project-removal';

describe('project removal protocol', () => {
  it('sends no project identity or path and requires the retention guarantee', async () => {
    const invoke = vi.fn(async () => ({
      protocolVersion: 1,
      result: { retainedPrivateStorage: true, status: 'removed' },
    }));

    await expect(removeProject(invoke)).resolves.toEqual({
      protocolVersion: 1,
      result: { retainedPrivateStorage: true, status: 'removed' },
    });
    expect(invoke).toHaveBeenCalledWith('remove_project', {
      request: { protocolVersion: 1 },
    });
  });

  it('rejects destructive or extended result shapes', () => {
    expect(() =>
      parseRemoveProjectResponseV1({
        protocolVersion: 1,
        result: { retainedPrivateStorage: false, status: 'removed' },
      }),
    ).toThrow();
    expect(() =>
      parseRemoveProjectResponseV1({
        protocolVersion: 1,
        result: {
          deletedRepository: true,
          retainedPrivateStorage: true,
          status: 'removed',
        },
      }),
    ).toThrow();
  });
});

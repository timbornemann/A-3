import { describe, expect, it, vi } from 'vitest';
import { parseRebuildProjectIndexResponseV1, rebuildProjectIndex } from './project-rebuild';

describe('project rebuild protocol', () => {
  it('sends no project identity or path and accepts only a queued acknowledgement', async () => {
    const invoke = vi.fn(async () => ({ protocolVersion: 1, state: 'queued' }));

    await expect(rebuildProjectIndex(invoke)).resolves.toEqual({
      protocolVersion: 1,
      state: 'queued',
    });
    expect(invoke).toHaveBeenCalledWith('rebuild_project_index', {
      request: { protocolVersion: 1 },
    });
  });

  it('rejects unknown fields and unacknowledged states', () => {
    expect(() =>
      parseRebuildProjectIndexResponseV1({
        protocolVersion: 1,
        state: 'queued',
        worktreeId: '2'.repeat(64),
      }),
    ).toThrow();
    expect(() =>
      parseRebuildProjectIndexResponseV1({ protocolVersion: 1, state: 'running' }),
    ).toThrow();
  });
});

import { describe, expect, it, vi } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import {
  listRecentProjects,
  parseRecentProjectsResponseV1,
  type RecentProjectsResponseV1,
} from './recent-projects';

const response: RecentProjectsResponseV1 = {
  projects: [
    {
      project: {
        head: { kind: 'unborn', reference: 'refs/heads/main' },
        repositoryId: '1'.repeat(64),
        worktreeId: '2'.repeat(64),
        worktreeRootDisplay: '/worktree',
      },
      projectId: '3'.repeat(64),
    },
  ],
  protocolVersion: CURRENT_PROTOCOL_VERSION,
};

describe('recent projects IPC client', () => {
  it('queries a fixed server-side bound without sending a path or limit', async () => {
    const invokeCommand = vi.fn(async () => response);

    await expect(listRecentProjects(invokeCommand)).resolves.toEqual(response);
    expect(invokeCommand).toHaveBeenCalledWith('list_recent_projects', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('rejects an unbounded response', () => {
    expect(() =>
      parseRecentProjectsResponseV1({
        ...response,
        projects: Array.from({ length: 11 }, () => response.projects[0]),
      }),
    ).toThrowError('invalid bounded list');
  });

  it('rejects malformed catalog identities and unknown fields', () => {
    expect(() =>
      parseRecentProjectsResponseV1({
        ...response,
        projects: [{ ...response.projects[0], projectId: 'invalid' }],
      }),
    ).toThrowError('invalid catalog identity');
    expect(() => parseRecentProjectsResponseV1({ ...response, rawPath: '/private' })).toThrowError(
      'does not match the V1 schema',
    );
  });
});

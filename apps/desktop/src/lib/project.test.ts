import { describe, expect, it, vi } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import { openProject, parseOpenProjectResponseV1, type OpenProjectResponseV1 } from './project';

const openedResponse: OpenProjectResponseV1 = {
  protocolVersion: CURRENT_PROTOCOL_VERSION,
  result: {
    project: {
      head: { kind: 'unborn', reference: 'refs/heads/main' },
      repositoryId: '1'.repeat(64),
      worktreeId: '2'.repeat(64),
      worktreeRootDisplay: 'C:\\worktree',
    },
    status: 'opened',
  },
};

describe('project IPC client', () => {
  it('sends no WebView-supplied path and validates the opened project', async () => {
    const invokeCommand = vi.fn(async () => openedResponse);

    await expect(openProject(invokeCommand)).resolves.toEqual(openedResponse);
    expect(invokeCommand).toHaveBeenCalledWith('open_project', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('accepts the mutually exclusive cancellation result', () => {
    expect(
      parseOpenProjectResponseV1({
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        result: { status: 'cancelled' },
      }),
    ).toEqual({
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: 'cancelled' },
    });
  });

  it('rejects malformed identity digests from the untrusted boundary', () => {
    expect(() =>
      parseOpenProjectResponseV1({
        ...openedResponse,
        result: {
          ...openedResponse.result,
          project: {
            ...('project' in openedResponse.result ? openedResponse.result.project : {}),
            repositoryId: 'not-an-id',
          },
        },
      }),
    ).toThrowError('invalid project identity');
  });

  it('rejects extra fields even when the remaining shape is valid', () => {
    expect(() =>
      parseOpenProjectResponseV1({
        ...openedResponse,
        selectedPath: 'untrusted',
      }),
    ).toThrowError('does not match the V1 schema');
  });
});

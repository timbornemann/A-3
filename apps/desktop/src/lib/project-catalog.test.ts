import { describe, expect, it, vi } from 'vitest';
import type { InvokeCommand } from './health';
import {
  activateCatalogProject,
  parseProjectActivationResponseV1,
  parseProjectCatalogResponseV1,
  queryProjectCatalog,
  removeCatalogProject,
  restoreLastProject,
} from './project-catalog';

const project = {
  head: { kind: 'unborn' as const, reference: 'refs/heads/main' },
  repositoryId: '1'.repeat(64),
  worktreeId: '2'.repeat(64),
  worktreeRootDisplay: 'C:\\workspace',
};

describe('project catalog V1 client', () => {
  it('sends only bounded search and opaque cursor fields', async () => {
    const response = {
      nextCursor: '0000000000000019',
      previousCursor: null,
      projects: [{ project, projectId: '3'.repeat(64) }],
      protocolVersion: 1 as const,
    };
    const invokeCommand = vi.fn<InvokeCommand>(async () => response);

    await expect(
      queryProjectCatalog(
        { cursor: null, direction: 'initial', search: 'workspace' },
        invokeCommand,
      ),
    ).resolves.toEqual(response);
    expect(invokeCommand).toHaveBeenCalledWith('query_project_catalog', {
      request: {
        cursor: null,
        direction: 'initial',
        protocolVersion: 1,
        search: 'workspace',
      },
    });
    expect(JSON.stringify(invokeCommand.mock.calls[0])).not.toContain('selectedPath');
  });

  it('rejects oversized pages, raw adapter fields, and malformed cursors', () => {
    const entry = { project, projectId: '3'.repeat(64) };
    expect(() =>
      parseProjectCatalogResponseV1({
        nextCursor: null,
        previousCursor: null,
        projects: Array.from({ length: 26 }, () => entry),
        protocolVersion: 1,
      }),
    ).toThrowError(/schema/u);
    expect(() =>
      parseProjectCatalogResponseV1({
        nextCursor: null,
        previousCursor: null,
        projects: [{ ...entry, rawPath: 'C:\\secret' }],
        protocolVersion: 1,
      }),
    ).toThrowError(/entry/u);
    expect(() =>
      parseProjectCatalogResponseV1({
        nextCursor: 'not-a-cursor',
        previousCursor: null,
        projects: [],
        protocolVersion: 1,
      }),
    ).toThrowError(/schema/u);
  });

  it('activates, restores, and removes by worktree ID without a path parameter', async () => {
    const activation = {
      protocolVersion: 1 as const,
      result: { project, projectId: '3'.repeat(64), status: 'activated' as const },
    };
    const removed = {
      protocolVersion: 1 as const,
      result: { retainedPrivateStorage: true as const, status: 'removed' as const },
    };
    const invokeCommand = vi.fn<InvokeCommand>(async (command) =>
      command === 'remove_catalog_project' ? removed : activation,
    );

    await expect(activateCatalogProject(project.worktreeId, invokeCommand)).resolves.toEqual(
      activation,
    );
    await expect(restoreLastProject(invokeCommand)).resolves.toEqual(activation);
    await expect(removeCatalogProject(project.worktreeId, invokeCommand)).resolves.toEqual(removed);
    expect(invokeCommand.mock.calls).toEqual([
      [
        'activate_catalog_project',
        { request: { protocolVersion: 1, worktreeId: project.worktreeId } },
      ],
      ['restore_last_project', { request: { protocolVersion: 1 } }],
      [
        'remove_catalog_project',
        { request: { protocolVersion: 1, worktreeId: project.worktreeId } },
      ],
    ]);
    expect(JSON.stringify(invokeCommand.mock.calls)).not.toContain('C:\\workspace');
  });

  it('keeps no-saved-project restoration distinct from a failed activation', () => {
    expect(
      parseProjectActivationResponseV1({
        protocolVersion: 1,
        result: { status: 'noSavedProject' },
      }).result,
    ).toEqual({ status: 'noSavedProject' });
    expect(() =>
      parseProjectActivationResponseV1({
        protocolVersion: 1,
        result: { fallbackProject: project, status: 'noSavedProject' },
      }),
    ).toThrowError(/result/u);
  });
});

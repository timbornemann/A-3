import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import { parseProjectSummaryV1, type ProjectSummaryV1 } from './project';
import { parseRemoveProjectResponseV1, type RemoveProjectResponseV1 } from './project-removal';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const CURSOR_PATTERN = /^[0-9a-f]{16}$/;
const MAX_PROJECTS_PER_PAGE = 25;
const MAX_SEARCH_LENGTH = 128;

export type ProjectCatalogDirectionV1 = 'initial' | 'next' | 'previous';

export interface ProjectCatalogQueryV1 {
  cursor: string | null;
  direction: ProjectCatalogDirectionV1;
  search: string | null;
}

export interface ProjectCatalogEntryV1 {
  project: ProjectSummaryV1;
  projectId: string;
}

export interface ProjectCatalogResponseV1 {
  nextCursor: string | null;
  previousCursor: string | null;
  projects: ProjectCatalogEntryV1[];
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ProjectActivationResultV1 =
  | { status: 'noSavedProject' }
  | { project: ProjectSummaryV1; projectId: string; status: 'activated' };

export interface ProjectActivationResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectActivationResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectCatalog(
  query: ProjectCatalogQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectCatalogResponseV1> {
  validateProjectCatalogQuery(query);
  const payload = await invokeCommand('query_project_catalog', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION, ...query },
  });
  return parseProjectCatalogResponseV1(payload);
}

export async function activateCatalogProject(
  worktreeId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectActivationResponseV1> {
  assertStableId(worktreeId);
  const payload = await invokeCommand('activate_catalog_project', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION, worktreeId },
  });
  return parseProjectActivationResponseV1(payload);
}

export async function restoreLastProject(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectActivationResponseV1> {
  const payload = await invokeCommand('restore_last_project', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseProjectActivationResponseV1(payload);
}

export async function removeCatalogProject(
  worktreeId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<RemoveProjectResponseV1> {
  assertStableId(worktreeId);
  const payload = await invokeCommand('remove_catalog_project', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION, worktreeId },
  });
  return parseRemoveProjectResponseV1(payload);
}

export function parseProjectCatalogResponseV1(payload: unknown): ProjectCatalogResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['nextCursor', 'previousCursor', 'projects', 'protocolVersion']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !Array.isArray(payload.projects) ||
    payload.projects.length > MAX_PROJECTS_PER_PAGE ||
    !isCursor(payload.previousCursor) ||
    !isCursor(payload.nextCursor)
  ) {
    throw new Error('Project catalog response does not match the V1 schema.');
  }
  return {
    nextCursor: payload.nextCursor,
    previousCursor: payload.previousCursor,
    projects: payload.projects.map(parseCatalogEntry),
    protocolVersion: payload.protocolVersion,
  };
}

export function parseProjectActivationResponseV1(payload: unknown): ProjectActivationResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !isRecord(payload.result) ||
    typeof payload.result.status !== 'string'
  ) {
    throw new Error('Project activation response does not match the V1 schema.');
  }
  if (payload.result.status === 'noSavedProject' && hasExactKeys(payload.result, ['status'])) {
    return { protocolVersion: payload.protocolVersion, result: { status: 'noSavedProject' } };
  }
  if (
    payload.result.status === 'activated' &&
    hasExactKeys(payload.result, ['project', 'projectId', 'status']) &&
    typeof payload.result.projectId === 'string' &&
    STABLE_ID_PATTERN.test(payload.result.projectId)
  ) {
    return {
      protocolVersion: payload.protocolVersion,
      result: {
        project: parseProjectSummaryV1(payload.result.project),
        projectId: payload.result.projectId,
        status: 'activated',
      },
    };
  }
  throw new Error('Project activation response contains an invalid result.');
}

export function validateProjectCatalogQuery(query: ProjectCatalogQueryV1): void {
  const validSearch =
    query.search === null ||
    (query.search.length <= MAX_SEARCH_LENGTH &&
      !Array.from(query.search).some((character) => {
        const point = character.codePointAt(0);
        return point !== undefined && (point <= 31 || point === 127);
      }));
  const validCursor = isCursor(query.cursor);
  const paired =
    (query.direction === 'initial' && query.cursor === null) ||
    (query.direction !== 'initial' && query.cursor !== null);
  if (!validSearch || !validCursor || !paired) {
    throw new Error('Project catalog query is outside the V1 bounds.');
  }
}

function parseCatalogEntry(value: unknown): ProjectCatalogEntryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['project', 'projectId']) ||
    typeof value.projectId !== 'string' ||
    !STABLE_ID_PATTERN.test(value.projectId)
  ) {
    throw new Error('Project catalog entry is invalid.');
  }
  return { project: parseProjectSummaryV1(value.project), projectId: value.projectId };
}

function assertStableId(value: string): void {
  if (!STABLE_ID_PATTERN.test(value)) throw new Error('Project catalog identity is invalid.');
}

function isCursor(value: unknown): value is string | null {
  return value === null || (typeof value === 'string' && CURSOR_PATTERN.test(value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const keys = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return (
    keys.length === sortedExpected.length &&
    keys.every((key, index) => key === sortedExpected[index])
  );
}

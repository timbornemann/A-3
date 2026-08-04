import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import { parseProjectSummaryV1, type ProjectSummaryV1 } from './project';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const MAX_RECENT_PROJECTS = 10;

export interface ListRecentProjectsRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface RecentProjectSummaryV1 {
  project: ProjectSummaryV1;
  projectId: string;
}

export interface RecentProjectsResponseV1 {
  projects: RecentProjectSummaryV1[];
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function listRecentProjects(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<RecentProjectsResponseV1> {
  const request: ListRecentProjectsRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('list_recent_projects', { request });
  return parseRecentProjectsResponseV1(payload);
}

export function parseRecentProjectsResponseV1(payload: unknown): RecentProjectsResponseV1 {
  if (!isRecord(payload) || !hasExactKeys(payload, ['projects', 'protocolVersion'])) {
    throw new Error('Recent projects response does not match the V1 schema.');
  }
  if (
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !Array.isArray(payload.projects) ||
    payload.projects.length > MAX_RECENT_PROJECTS
  ) {
    throw new Error('Recent projects response contains an invalid bounded list.');
  }

  return {
    projects: payload.projects.map(parseRecentProject),
    protocolVersion: payload.protocolVersion,
  };
}

function parseRecentProject(value: unknown): RecentProjectSummaryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['project', 'projectId']) ||
    typeof value.projectId !== 'string' ||
    !STABLE_ID_PATTERN.test(value.projectId)
  ) {
    throw new Error('Recent projects response contains an invalid catalog identity.');
  }
  return {
    project: parseProjectSummaryV1(value.project),
    projectId: value.projectId,
  };
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

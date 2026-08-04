import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const OBJECT_ID_PATTERN = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/;
const MAX_REFERENCE_LENGTH = 1_024;
const MAX_PATH_DISPLAY_LENGTH = 32_768;

export interface OpenProjectRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type GitHeadV1 =
  | { kind: 'born'; objectId: string; reference: string | null }
  | { kind: 'unborn'; reference: string };

export interface ProjectSummaryV1 {
  head: GitHeadV1;
  repositoryId: string;
  worktreeId: string;
  worktreeRootDisplay: string;
}

export type OpenProjectResultV1 =
  { status: 'cancelled' } | { project: ProjectSummaryV1; status: 'opened' };

export interface OpenProjectResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: OpenProjectResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function openProject(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<OpenProjectResponseV1> {
  const request: OpenProjectRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('open_project', { request });
  return parseOpenProjectResponseV1(payload);
}

export function parseOpenProjectResponseV1(payload: unknown): OpenProjectResponseV1 {
  if (!isRecord(payload) || !hasExactKeys(payload, ['protocolVersion', 'result'])) {
    throw new Error('Project response does not match the V1 schema.');
  }
  if (payload.protocolVersion !== CURRENT_PROTOCOL_VERSION) {
    throw new Error('Project response uses an unsupported protocol version.');
  }

  return {
    protocolVersion: payload.protocolVersion,
    result: parseResult(payload.result),
  };
}

function parseResult(value: unknown): OpenProjectResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Project response contains an invalid result.');
  }
  if (value.status === 'cancelled' && hasExactKeys(value, ['status'])) {
    return { status: 'cancelled' };
  }
  if (value.status === 'opened' && hasExactKeys(value, ['project', 'status'])) {
    return { project: parseProject(value.project), status: 'opened' };
  }
  throw new Error('Project response contains an invalid result.');
}

function parseProject(value: unknown): ProjectSummaryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['head', 'repositoryId', 'worktreeId', 'worktreeRootDisplay']) ||
    typeof value.repositoryId !== 'string' ||
    !STABLE_ID_PATTERN.test(value.repositoryId) ||
    typeof value.worktreeId !== 'string' ||
    !STABLE_ID_PATTERN.test(value.worktreeId) ||
    typeof value.worktreeRootDisplay !== 'string' ||
    value.worktreeRootDisplay.length === 0 ||
    value.worktreeRootDisplay.length > MAX_PATH_DISPLAY_LENGTH
  ) {
    throw new Error('Project response contains an invalid project identity.');
  }

  return {
    head: parseHead(value.head),
    repositoryId: value.repositoryId,
    worktreeId: value.worktreeId,
    worktreeRootDisplay: value.worktreeRootDisplay,
  };
}

function parseHead(value: unknown): GitHeadV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') {
    throw new Error('Project response contains an invalid HEAD state.');
  }
  if (
    value.kind === 'born' &&
    hasExactKeys(value, ['kind', 'objectId', 'reference']) &&
    typeof value.objectId === 'string' &&
    OBJECT_ID_PATTERN.test(value.objectId) &&
    (value.reference === null || isReference(value.reference))
  ) {
    return { kind: 'born', objectId: value.objectId, reference: value.reference };
  }
  if (
    value.kind === 'unborn' &&
    hasExactKeys(value, ['kind', 'reference']) &&
    isReference(value.reference)
  ) {
    return { kind: 'unborn', reference: value.reference };
  }
  throw new Error('Project response contains an invalid HEAD state.');
}

function isReference(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.startsWith('refs/') &&
    value.length <= MAX_REFERENCE_LENGTH &&
    !Array.from(value).some((character) => {
      const codePoint = character.codePointAt(0);
      return codePoint !== undefined && (codePoint <= 31 || codePoint === 127);
    })
  );
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

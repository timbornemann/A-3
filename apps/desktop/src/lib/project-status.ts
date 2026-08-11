import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import { parseProjectSummaryV1, type ProjectSummaryV1 } from './project';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const GENERATION_PATTERN = /^[1-9][0-9]{0,18}$/;
const MAX_GENERATION = 9_223_372_036_854_775_807n;
const BYTE_COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAX_BYTE_COUNT = 18_446_744_073_709_551_615n;

export interface QueryProjectStatusRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type IndexStateV1 = 'notStarted' | 'building' | 'published' | 'failed' | 'cancelled';
export type RebuildStateV1 = 'idle' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

export interface ProjectSnapshotV1 {
  generation: string;
  snapshotId: string;
}

export interface ProjectIndexStatusV1 {
  latestAttemptSnapshotId: string | null;
  latestSnapshot: ProjectSnapshotV1 | null;
  publishedSnapshotId: string | null;
  state: IndexStateV1;
}

export type ProjectStatusResultV1 =
  | { status: 'noProject' }
  | {
      index: ProjectIndexStatusV1;
      project: ProjectSummaryV1;
      projectId: string;
      rebuildState: RebuildStateV1;
      status: 'active';
      storageBytes: string | null;
    };

export interface ProjectStatusResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectStatusResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectStatus(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectStatusResponseV1> {
  const request: QueryProjectStatusRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  const payload = await invokeCommand('query_project_status', { request });
  return parseProjectStatusResponseV1(payload);
}

export function parseProjectStatusResponseV1(payload: unknown): ProjectStatusResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Project status response does not match the V1 schema.');
  }

  return {
    protocolVersion: payload.protocolVersion,
    result: parseResult(payload.result),
  };
}

function parseResult(value: unknown): ProjectStatusResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Project status response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (
    value.status === 'active' &&
    hasExactKeys(value, [
      'index',
      'project',
      'projectId',
      'rebuildState',
      'status',
      'storageBytes',
    ]) &&
    isStableId(value.projectId) &&
    isStorageBytes(value.storageBytes) &&
    isRebuildState(value.rebuildState)
  ) {
    return {
      index: parseIndexStatus(value.index),
      project: parseProjectSummaryV1(value.project),
      projectId: value.projectId,
      rebuildState: value.rebuildState,
      status: 'active',
      storageBytes: value.storageBytes,
    };
  }
  throw new Error('Project status response contains an invalid result.');
}

function parseIndexStatus(value: unknown): ProjectIndexStatusV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'latestAttemptSnapshotId',
      'latestSnapshot',
      'publishedSnapshotId',
      'state',
    ]) ||
    !isIndexState(value.state) ||
    !isOptionalStableId(value.latestAttemptSnapshotId) ||
    !isOptionalStableId(value.publishedSnapshotId)
  ) {
    throw new Error('Project status response contains an invalid index state.');
  }

  const latestSnapshot = value.latestSnapshot === null ? null : parseSnapshot(value.latestSnapshot);
  const notStarted = value.state === 'notStarted';
  if (
    notStarted !== (value.latestAttemptSnapshotId === null) ||
    (value.latestAttemptSnapshotId !== null && latestSnapshot === null) ||
    (value.publishedSnapshotId !== null && latestSnapshot === null)
  ) {
    throw new Error('Project status response contains inconsistent index metadata.');
  }

  return {
    latestAttemptSnapshotId: value.latestAttemptSnapshotId,
    latestSnapshot,
    publishedSnapshotId: value.publishedSnapshotId,
    state: value.state,
  };
}

function parseSnapshot(value: unknown): ProjectSnapshotV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['generation', 'snapshotId']) ||
    !isStableId(value.snapshotId) ||
    typeof value.generation !== 'string' ||
    !GENERATION_PATTERN.test(value.generation) ||
    BigInt(value.generation) > MAX_GENERATION
  ) {
    throw new Error('Project status response contains an invalid snapshot.');
  }
  return { generation: value.generation, snapshotId: value.snapshotId };
}

function isIndexState(value: unknown): value is IndexStateV1 {
  return (
    value === 'notStarted' ||
    value === 'building' ||
    value === 'published' ||
    value === 'failed' ||
    value === 'cancelled'
  );
}

function isRebuildState(value: unknown): value is RebuildStateV1 {
  return (
    value === 'idle' ||
    value === 'queued' ||
    value === 'running' ||
    value === 'succeeded' ||
    value === 'failed' ||
    value === 'cancelled'
  );
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isOptionalStableId(value: unknown): value is string | null {
  return value === null || isStableId(value);
}

function isStorageBytes(value: unknown): value is string | null {
  return (
    value === null ||
    (typeof value === 'string' && BYTE_COUNT_PATTERN.test(value) && BigInt(value) <= MAX_BYTE_COUNT)
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

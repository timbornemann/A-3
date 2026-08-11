import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

export interface QueryIndexActivityRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type IndexActivityStateV1 =
  'idle' | 'queued' | 'running' | 'cancelling' | 'succeeded' | 'failed' | 'cancelled';
export type IndexPhaseV1 = 'discover' | 'hash' | 'parse' | 'link' | 'rank' | 'publish';

export interface IndexActivityV1 {
  completedPhases: number;
  phase: IndexPhaseV1 | null;
  state: IndexActivityStateV1;
  totalPhases: 6;
}

export type IndexActivityResultV1 =
  { status: 'noProject' } | { activity: IndexActivityV1; status: 'active' };

export interface IndexActivityResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: IndexActivityResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryIndexActivity(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<IndexActivityResponseV1> {
  const request: QueryIndexActivityRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  const payload = await invokeCommand('query_index_activity', { request });
  return parseIndexActivityResponseV1(payload);
}

export function parseIndexActivityResponseV1(payload: unknown): IndexActivityResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Index activity response does not match the V1 schema.');
  }

  return {
    protocolVersion: payload.protocolVersion,
    result: parseResult(payload.result),
  };
}

function parseResult(value: unknown): IndexActivityResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Index activity response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status === 'active' && hasExactKeys(value, ['activity', 'status'])) {
    return { activity: parseActivity(value.activity), status: 'active' };
  }
  throw new Error('Index activity response contains an invalid result.');
}

function parseActivity(value: unknown): IndexActivityV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['completedPhases', 'phase', 'state', 'totalPhases']) ||
    !isActivityState(value.state) ||
    !Number.isInteger(value.completedPhases) ||
    typeof value.completedPhases !== 'number' ||
    value.completedPhases < 0 ||
    value.completedPhases > 6 ||
    value.totalPhases !== 6 ||
    (value.phase !== null && !isIndexPhase(value.phase))
  ) {
    throw new Error('Index activity response contains invalid progress.');
  }

  if (value.state === 'idle') {
    if (value.phase !== null || value.completedPhases !== 0) {
      throw new Error('Idle index activity contains contradictory progress.');
    }
  } else {
    if (value.phase === null || !phaseMatchesProgress(value.phase, value.completedPhases)) {
      throw new Error('Index activity phase does not match its progress.');
    }
    if (value.state === 'queued' && (value.phase !== 'discover' || value.completedPhases !== 0)) {
      throw new Error('Queued index activity must begin with discovery.');
    }
    if (value.state === 'succeeded' && value.completedPhases !== 6) {
      throw new Error('Succeeded index activity must be complete.');
    }
  }

  return {
    completedPhases: value.completedPhases,
    phase: value.phase,
    state: value.state,
    totalPhases: value.totalPhases,
  };
}

function phaseMatchesProgress(phase: IndexPhaseV1, completed: number): boolean {
  const expected: Record<IndexPhaseV1, number> = {
    discover: 0,
    hash: 1,
    parse: 2,
    link: 3,
    rank: 4,
    publish: 5,
  };
  return completed === expected[phase] || (phase === 'publish' && completed === 6);
}

function isActivityState(value: unknown): value is IndexActivityStateV1 {
  return (
    value === 'idle' ||
    value === 'queued' ||
    value === 'running' ||
    value === 'cancelling' ||
    value === 'succeeded' ||
    value === 'failed' ||
    value === 'cancelled'
  );
}

function isIndexPhase(value: unknown): value is IndexPhaseV1 {
  return (
    value === 'discover' ||
    value === 'hash' ||
    value === 'parse' ||
    value === 'link' ||
    value === 'rank' ||
    value === 'publish'
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

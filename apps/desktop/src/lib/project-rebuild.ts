import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

export interface RebuildProjectIndexRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface RebuildProjectIndexResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  state: 'queued';
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function rebuildProjectIndex(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<RebuildProjectIndexResponseV1> {
  const request: RebuildProjectIndexRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  const payload = await invokeCommand('rebuild_project_index', { request });
  return parseRebuildProjectIndexResponseV1(payload);
}

export function parseRebuildProjectIndexResponseV1(
  payload: unknown,
): RebuildProjectIndexResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'state']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    payload.state !== 'queued'
  ) {
    throw new Error('Project rebuild response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, state: payload.state };
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

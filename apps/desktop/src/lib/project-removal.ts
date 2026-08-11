import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

export interface RemoveProjectRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface RemoveProjectResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: {
    retainedPrivateStorage: true;
    status: 'removed';
  };
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function removeProject(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<RemoveProjectResponseV1> {
  const request: RemoveProjectRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  const payload = await invokeCommand('remove_project', { request });
  return parseRemoveProjectResponseV1(payload);
}

export function parseRemoveProjectResponseV1(payload: unknown): RemoveProjectResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !isRecord(payload.result) ||
    !hasExactKeys(payload.result, ['retainedPrivateStorage', 'status']) ||
    payload.result.status !== 'removed' ||
    payload.result.retainedPrivateStorage !== true
  ) {
    throw new Error('Project removal response does not match the V1 schema.');
  }
  return {
    protocolVersion: payload.protocolVersion,
    result: {
      retainedPrivateStorage: true,
      status: 'removed',
    },
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

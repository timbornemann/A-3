import { invoke as tauriInvoke } from '@tauri-apps/api/core';

export const CURRENT_PROTOCOL_VERSION = 1 as const;

export type PlatformV1 = 'windows' | 'linux' | 'macOs' | 'unsupported';

export interface HealthRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface HealthResponseV1 {
  applicationVersion: string;
  platform: PlatformV1;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  status: 'ready';
}

export type InvokeCommand = (
  command: string,
  arguments_: Record<string, unknown>,
) => Promise<unknown>;

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryHealth(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<HealthResponseV1> {
  const request: HealthRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_health', { request });

  return parseHealthResponseV1(payload);
}

export function parseHealthResponseV1(payload: unknown): HealthResponseV1 {
  if (!isRecord(payload) || !hasExactHealthKeys(payload)) {
    throw new Error('Health response does not match the V1 schema.');
  }

  if (payload.protocolVersion !== CURRENT_PROTOCOL_VERSION) {
    throw new Error('Health response uses an unsupported protocol version.');
  }

  if (
    typeof payload.applicationVersion !== 'string' ||
    !/^[A-Za-z0-9.+-]{1,64}$/.test(payload.applicationVersion)
  ) {
    throw new Error('Health response contains an invalid application version.');
  }

  if (!isPlatformV1(payload.platform)) {
    throw new Error('Health response contains an invalid platform.');
  }

  if (payload.status !== 'ready') {
    throw new Error('Health response contains an invalid status.');
  }

  return {
    applicationVersion: payload.applicationVersion,
    platform: payload.platform,
    protocolVersion: payload.protocolVersion,
    status: payload.status,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactHealthKeys(value: Record<string, unknown>): boolean {
  const keys = Object.keys(value).sort();
  const expectedKeys = ['applicationVersion', 'platform', 'protocolVersion', 'status'];

  return (
    keys.length === expectedKeys.length && keys.every((key, index) => key === expectedKeys[index])
  );
}

function isPlatformV1(value: unknown): value is PlatformV1 {
  return value === 'windows' || value === 'linux' || value === 'macOs' || value === 'unsupported';
}

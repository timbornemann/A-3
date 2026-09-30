import { invoke } from '@tauri-apps/api/core';

export interface MachineRecoveryScope {
  ledgerRevision: number;
  ledgerStoreVersion: string;
  scope: string;
  resourceKind: 'file' | 'process' | 'http';
  target: string;
}
export type MachineRecoveryResult =
  | { status: 'unavailable' | 'activityChanged' | 'queued' }
  | { status: 'available'; recovery: MachineRecoveryScope };

const id = /^[0-9a-f]{64}$/u;
const revision = /^[1-9][0-9]{0,18}$/u;
const maxRevision = 9_223_372_036_854_775_807n;
const utf8 = new TextEncoder();

export function parseMachineRecovery(payload: unknown): MachineRecoveryResult {
  if (
    !record(payload) ||
    !keys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== 1 ||
    !record(payload.result)
  )
    throw invalid();
  const result = payload.result;
  if (
    ['unavailable', 'activityChanged', 'queued'].includes(String(result.status)) &&
    keys(result, ['status'])
  )
    return result as MachineRecoveryResult;
  if (
    result.status !== 'available' ||
    !keys(result, ['status', 'recovery']) ||
    !record(result.recovery)
  )
    throw invalid();
  const scope = result.recovery;
  if (
    !keys(scope, ['ledgerRevision', 'ledgerStoreVersion', 'scope', 'resourceKind', 'target']) ||
    typeof scope.ledgerRevision !== 'number' ||
    !Number.isInteger(scope.ledgerRevision) ||
    scope.ledgerRevision < 1 ||
    scope.ledgerRevision > 4_294_967_295 ||
    typeof scope.ledgerStoreVersion !== 'string' ||
    !revision.test(scope.ledgerStoreVersion) ||
    BigInt(scope.ledgerStoreVersion) > maxRevision ||
    typeof scope.scope !== 'string' ||
    !id.test(scope.scope) ||
    !['file', 'process', 'http'].includes(String(scope.resourceKind)) ||
    typeof scope.target !== 'string' ||
    scope.target.length === 0 ||
    utf8.encode(scope.target).length > 4096 ||
    Array.from(scope.target).some(
      (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    )
  )
    throw invalid();
  return { status: 'available', recovery: scope as unknown as MachineRecoveryScope };
}

export async function queryMachineRecovery(taskId: string): Promise<MachineRecoveryResult> {
  if (!id.test(taskId)) throw invalid();
  return parseMachineRecovery(
    await invoke('query_agent_machine_recovery', { request: { protocolVersion: 1, taskId } }),
  );
}
export async function recoverMachineEffect(
  taskId: string,
  scope: MachineRecoveryScope,
): Promise<MachineRecoveryResult> {
  if (!id.test(taskId)) throw invalid();
  return parseMachineRecovery(
    await invoke('recover_agent_machine_effect', {
      request: {
        protocolVersion: 1,
        taskId,
        expectedLedgerRevision: scope.ledgerRevision,
        expectedLedgerStoreVersion: scope.ledgerStoreVersion,
        expectedScope: scope.scope,
        action:
          scope.resourceKind === 'file' ? 'observeFileAndReplan' : 'acknowledgeUnknownAndReplan',
      },
    }),
  );
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function keys(value: Record<string, unknown>, names: string[]): boolean {
  return (
    Object.keys(value).length === names.length && names.every((name) => Object.hasOwn(value, name))
  );
}
function invalid(): Error {
  return new Error('Machine recovery does not match V1.');
}

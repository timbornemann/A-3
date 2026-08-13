import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { AgentControllerStateV1 } from './agent-activity';

const CURRENT_PROTOCOL_VERSION = 1 as const;
const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/u;
const POSITIVE_DECIMAL_PATTERN = /^[1-9][0-9]*$/u;
const MAX_U32 = 4_294_967_295;
const MAX_U64 = 18_446_744_073_709_551_615n;

type InvokeCommand = (command: string, arguments_: Record<string, unknown>) => Promise<unknown>;

export type AgentTaskControlActionV1 = 'cancel' | 'pause' | 'replan' | 'resume';
export type AgentTaskControlOutcomeV1 = 'cancelled' | 'replanRequired' | 'resumed';
export type AgentTaskControlAcceptedOutcomeV1 = 'cancelRequested' | 'pauseRequested';
export type AgentTaskRuntimeStartV1 = 'failed' | 'queued' | 'unavailable';
export type AgentTaskRuntimeStateV1 = 'cancelling' | 'pausing' | 'queued' | 'running';

export interface AgentTaskRuntimeV1 {
  canPause: boolean;
  controllerState: AgentControllerStateV1;
  ledgerRevision: number;
  ledgerStoreVersion: string;
  runtimeState: AgentTaskRuntimeStateV1;
}

export interface AgentTaskRecoveryV1 {
  canResume: boolean;
  interruptedToolAttempts: number;
  ledgerRevision: number;
  ledgerStoreVersion: string;
  mutationReconciliationRequired: boolean;
  mutationReplanRequired: boolean;
  publishedSnapshotId: string;
  runSnapshotId: string;
  snapshotChanged: boolean;
  staleEvidenceCount: number;
  state: AgentControllerStateV1;
}

export type AgentTaskRecoveryResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | { currentRevision: number; ledgerRevision: number; status: 'goalRevisionMismatch' }
  | { status: 'activityChanged' }
  | { status: 'runUnavailable' }
  | { state: AgentControllerStateV1; status: 'runNotControllable' }
  | { runtime: AgentTaskRuntimeV1; status: 'runtimeOwned' }
  | { recovery: AgentTaskRecoveryV1; status: 'paused' }
  | { recovery: AgentTaskRecoveryV1; status: 'available' };

export interface AgentTaskRecoveryResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentTaskRecoveryResultV1;
}

export type AgentTaskControlResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | { currentRevision: number; ledgerRevision: number; status: 'goalRevisionMismatch' }
  | { status: 'activityChanged' }
  | { status: 'runUnavailable' }
  | { state: AgentControllerStateV1; status: 'runNotControllable' }
  | { status: 'mutationReconciliationRequired' }
  | { status: 'resumeRequiresReplan' }
  | { outcome: AgentTaskControlAcceptedOutcomeV1; status: 'accepted' }
  | {
      interruptedToolAttempts: number;
      ledgerStoreVersion: string;
      outcome: AgentTaskControlOutcomeV1;
      reopenedStepCount: number;
      runtimeStart: AgentTaskRuntimeStartV1 | null;
      state: AgentControllerStateV1;
      status: 'applied';
    };

export interface AgentTaskControlResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentTaskControlResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentTaskRecovery(
  taskId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentTaskRecoveryResponseV1> {
  if (!isStableId(taskId)) throw new Error('Agent task recovery identity does not match V1.');
  const request = { protocolVersion: CURRENT_PROTOCOL_VERSION, taskId };
  return parseAgentTaskRecoveryResponseV1(
    await invokeCommand('query_agent_task_recovery', { request }),
  );
}

export async function controlAgentTaskRun(
  taskId: string,
  expectedLedgerRevision: number,
  expectedLedgerStoreVersion: string,
  action: AgentTaskControlActionV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentTaskControlResponseV1> {
  if (
    !isStableId(taskId) ||
    !isPositiveU32(expectedLedgerRevision) ||
    !isPositiveDecimal(expectedLedgerStoreVersion) ||
    !isControlAction(action)
  ) {
    throw new Error('Agent task control request does not match V1.');
  }
  const request = {
    action,
    expectedLedgerRevision,
    expectedLedgerStoreVersion,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    taskId,
  };
  return parseAgentTaskControlResponseV1(
    await invokeCommand('control_agent_task_run', { request }),
  );
}

export function parseAgentTaskRecoveryResponseV1(payload: unknown): AgentTaskRecoveryResponseV1 {
  const result = parseEnvelope(payload);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseRecoveryResult(result) };
}

export function parseAgentTaskControlResponseV1(payload: unknown): AgentTaskControlResponseV1 {
  const result = parseEnvelope(payload);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseControlResult(result) };
}

function parseEnvelope(payload: unknown): unknown {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Agent task control response does not match V1.');
  }
  return payload.result;
}

function parseRecoveryResult(value: unknown): AgentTaskRecoveryResultV1 {
  const common = parseCommonResult(value);
  if (common !== null) return common;
  if (
    isRecord(value) &&
    value.status === 'runtimeOwned' &&
    hasExactKeys(value, ['runtime', 'status'])
  ) {
    return { runtime: parseRuntime(value.runtime), status: 'runtimeOwned' };
  }
  if (
    isRecord(value) &&
    (value.status === 'available' || value.status === 'paused') &&
    hasExactKeys(value, ['recovery', 'status'])
  ) {
    return { recovery: parseRecovery(value.recovery), status: value.status };
  }
  throw new Error('Agent task recovery result uses an unsupported state.');
}

function parseControlResult(value: unknown): AgentTaskControlResultV1 {
  const common = parseCommonResult(value);
  if (common !== null) return common;
  if (
    isRecord(value) &&
    value.status === 'accepted' &&
    hasExactKeys(value, ['outcome', 'status']) &&
    isAcceptedControlOutcome(value.outcome)
  ) {
    return { outcome: value.outcome, status: 'accepted' };
  }
  if (
    isRecord(value) &&
    (value.status === 'mutationReconciliationRequired' ||
      value.status === 'resumeRequiresReplan') &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status };
  }
  if (
    isRecord(value) &&
    value.status === 'applied' &&
    hasExactKeys(value, [
      'interruptedToolAttempts',
      'ledgerStoreVersion',
      'outcome',
      'reopenedStepCount',
      'runtimeStart',
      'state',
      'status',
    ]) &&
    isU32(value.interruptedToolAttempts) &&
    isPositiveDecimal(value.ledgerStoreVersion) &&
    isControlOutcome(value.outcome) &&
    isU32(value.reopenedStepCount) &&
    (value.runtimeStart === null || isRuntimeStart(value.runtimeStart)) &&
    isControllerState(value.state) &&
    ((value.outcome === 'cancelled' &&
      value.state === 'cancelled' &&
      value.runtimeStart === null) ||
      (value.outcome !== 'cancelled' &&
        !isTerminalState(value.state) &&
        value.runtimeStart !== null))
  ) {
    return value as AgentTaskControlResultV1;
  }
  throw new Error('Agent task control result uses an unsupported state.');
}

function parseCommonResult(value: unknown): CommonAgentTaskResultV1 | null {
  if (!isRecord(value) || typeof value.status !== 'string') return null;
  if (
    [
      'noProject',
      'taskNotFound',
      'ledgerUnavailable',
      'activityChanged',
      'runUnavailable',
    ].includes(value.status) &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status } as CommonAgentTaskResultV1;
  }
  if (
    value.status === 'goalRevisionMismatch' &&
    hasExactKeys(value, ['currentRevision', 'ledgerRevision', 'status']) &&
    isPositiveU32(value.currentRevision) &&
    isPositiveU32(value.ledgerRevision) &&
    value.currentRevision !== value.ledgerRevision
  ) {
    return {
      currentRevision: value.currentRevision,
      ledgerRevision: value.ledgerRevision,
      status: 'goalRevisionMismatch',
    };
  }
  if (
    value.status === 'runNotControllable' &&
    hasExactKeys(value, ['state', 'status']) &&
    isControllerState(value.state)
  ) {
    return { state: value.state, status: 'runNotControllable' };
  }
  return null;
}

type CommonAgentTaskResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | { currentRevision: number; ledgerRevision: number; status: 'goalRevisionMismatch' }
  | { status: 'activityChanged' }
  | { status: 'runUnavailable' }
  | { state: AgentControllerStateV1; status: 'runNotControllable' };

function parseRuntime(value: unknown): AgentTaskRuntimeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'canPause',
      'controllerState',
      'ledgerRevision',
      'ledgerStoreVersion',
      'runtimeState',
    ]) ||
    typeof value.canPause !== 'boolean' ||
    !isControllerState(value.controllerState) ||
    isTerminalState(value.controllerState) ||
    !isPositiveU32(value.ledgerRevision) ||
    !isPositiveDecimal(value.ledgerStoreVersion) ||
    !isRuntimeState(value.runtimeState) ||
    value.canPause !== (value.runtimeState === 'running')
  ) {
    throw new Error('Agent task runtime projection is invalid.');
  }
  return value as unknown as AgentTaskRuntimeV1;
}

function parseRecovery(value: unknown): AgentTaskRecoveryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'canResume',
      'interruptedToolAttempts',
      'ledgerRevision',
      'ledgerStoreVersion',
      'mutationReconciliationRequired',
      'mutationReplanRequired',
      'publishedSnapshotId',
      'runSnapshotId',
      'snapshotChanged',
      'staleEvidenceCount',
      'state',
    ]) ||
    typeof value.canResume !== 'boolean' ||
    !isU32(value.interruptedToolAttempts) ||
    !isPositiveU32(value.ledgerRevision) ||
    !isPositiveDecimal(value.ledgerStoreVersion) ||
    typeof value.mutationReconciliationRequired !== 'boolean' ||
    typeof value.mutationReplanRequired !== 'boolean' ||
    !isStableId(value.publishedSnapshotId) ||
    !isStableId(value.runSnapshotId) ||
    typeof value.snapshotChanged !== 'boolean' ||
    value.snapshotChanged !== (value.publishedSnapshotId !== value.runSnapshotId) ||
    !isU32(value.staleEvidenceCount) ||
    !isControllerState(value.state) ||
    isTerminalState(value.state) ||
    value.canResume !==
      (value.staleEvidenceCount === 0 &&
        !value.mutationReconciliationRequired &&
        !value.mutationReplanRequired) ||
    (value.mutationReconciliationRequired && value.mutationReplanRequired)
  ) {
    throw new Error('Agent task recovery projection is invalid.');
  }
  return value as unknown as AgentTaskRecoveryV1;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isPositiveDecimal(value: unknown): value is string {
  return (
    typeof value === 'string' && POSITIVE_DECIMAL_PATTERN.test(value) && BigInt(value) <= MAX_U64
  );
}

function isPositiveU32(value: unknown): value is number {
  return isU32(value) && value > 0;
}

function isU32(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= MAX_U32;
}

function isControllerState(value: unknown): value is AgentControllerStateV1 {
  return [
    'intake',
    'localize',
    'plan',
    'execute',
    'verify',
    'replan',
    'awaitApproval',
    'done',
    'failed',
    'cancelled',
  ].includes(value as string);
}

function isTerminalState(value: AgentControllerStateV1): boolean {
  return value === 'done' || value === 'failed' || value === 'cancelled';
}

function isControlAction(value: unknown): value is AgentTaskControlActionV1 {
  return value === 'cancel' || value === 'pause' || value === 'replan' || value === 'resume';
}

function isControlOutcome(value: unknown): value is AgentTaskControlOutcomeV1 {
  return value === 'cancelled' || value === 'replanRequired' || value === 'resumed';
}

function isAcceptedControlOutcome(value: unknown): value is AgentTaskControlAcceptedOutcomeV1 {
  return value === 'cancelRequested' || value === 'pauseRequested';
}

function isRuntimeStart(value: unknown): value is AgentTaskRuntimeStartV1 {
  return value === 'failed' || value === 'queued' || value === 'unavailable';
}

function isRuntimeState(value: unknown): value is AgentTaskRuntimeStateV1 {
  return value === 'cancelling' || value === 'pausing' || value === 'queued' || value === 'running';
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

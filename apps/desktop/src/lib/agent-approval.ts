import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { AgentControllerStateV1 } from './agent-activity';
import type { AgentInspectionPathV1 } from './agent-inspection';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID = /^[0-9a-f]{64}$/u;
const POSITIVE_DECIMAL = /^[1-9][0-9]{0,19}$/u;
const DECIMAL = /^(?:0|[1-9][0-9]{0,19})$/u;
const HEX_32 = /^[0-9a-f]{64}$/u;
const MAX_U64 = 18_446_744_073_709_551_615n;
const MAX_U32 = 4_294_967_295;
const MAX_FILES = 64;
const MAX_ARGUMENTS = 256;
const MAX_ARGUMENT_BYTES = 4 * 1024;
const MAX_TOTAL_ARGV_BYTES = 64 * 1024;
const MAX_ENVIRONMENT_NAMES = 64;
const MAX_ENVIRONMENT_NAME_BYTES = 128;
const MAX_TIMEOUT_MILLIS = 30n * 60n * 1000n;
const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;
const ENVIRONMENT_NAME = /^[A-Z_][A-Z0-9_]*$/u;
const utf8 = new TextEncoder();

export type AgentApprovalControlActionV1 = 'allowOnce' | 'deny' | 'continue' | 'revoke';
export type AgentApprovalStatusV1 =
  'pending' | 'active' | 'consumed' | 'revoked' | 'expired' | 'denied';
export type AgentApprovalActionClassV1 =
  | 'read'
  | 'derive'
  | 'write'
  | 'executeSafe'
  | 'executeOpen'
  | 'network'
  | 'destructive'
  | 'publish'
  | 'outsideRoot';
export type AgentApprovalRiskV1 = 'low' | 'moderate' | 'high' | 'critical';
export type AgentApprovalStepStatusV1 =
  | 'pending'
  | 'ready'
  | 'inProgress'
  | 'blocked'
  | 'awaitingApproval'
  | 'verifying'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'stale';

export interface AgentApprovalFileV1 {
  operation: 'add' | 'update' | 'move' | 'delete';
  sourcePath: AgentInspectionPathV1 | null;
  targetPath: AgentInspectionPathV1 | null;
}

export type AgentApprovalActionV1 =
  | { kind: 'patch'; patch: { rationale: string; files: AgentApprovalFileV1[] } }
  | {
      kind: 'process';
      process: {
        processKind: 'test' | 'build' | 'diagnostic' | 'lint' | 'format' | 'command';
        executable: string;
        arguments: string[];
        workingDirectory: { kind: 'root' } | { kind: 'subtree'; path: AgentInspectionPathV1 };
        environmentAllowlist: string[];
        timeoutMillis: string;
        stdoutLimit: number;
        stderrLimit: number;
        executionMode: 'knownSafe' | 'open' | 'shell';
        planBinding: { kind: 'unbound' } | { kind: 'validated'; stepId: string };
        network: { kind: 'denied' } | { kind: 'requested'; scopeDigest: string };
        specificationId: string;
      };
    };

export interface AgentApprovalV1 {
  approvalRevision: string;
  ledgerRevision: number;
  ledgerStoreVersion: string;
  controllerState: AgentControllerStateV1;
  stepStatus: AgentApprovalStepStatusV1;
  stepId: string;
  snapshotId: string;
  scopeDigest: string;
  actionClass: AgentApprovalActionClassV1;
  risk: AgentApprovalRiskV1;
  reason: 'systemPolicy' | 'workspacePolicy';
  requestedAtUnixMillis: string;
  expiresAtUnixMillis: string;
  status: AgentApprovalStatusV1;
  action: AgentApprovalActionV1;
  canAllowOnce: boolean;
  canDeny: boolean;
  canContinue: boolean;
  canRevoke: boolean;
}

export type AgentApprovalResultV1 =
  | {
      status:
        'noProject' | 'taskNotFound' | 'ledgerUnavailable' | 'activityChanged' | 'unavailable';
    }
  | { status: 'goalRevisionMismatch'; currentRevision: number; ledgerRevision: number }
  | { status: 'available'; approval: AgentApprovalV1 };

export interface AgentApprovalResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentApprovalResultV1;
}

export type AgentApprovalControlResultV1 =
  | {
      status:
        | 'noProject'
        | 'taskNotFound'
        | 'ledgerUnavailable'
        | 'goalRevisionMismatch'
        | 'activityChanged'
        | 'unavailable';
    }
  | {
      status: 'applied';
      outcome: 'grantStored' | 'denied' | 'revoked' | 'continueRequested';
      approvalRevision: string;
      ledgerStoreVersion: string;
      runtimeStart: 'unavailable' | 'queued' | 'failed' | null;
    };

export interface AgentApprovalControlResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentApprovalControlResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentApproval(
  taskId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentApprovalResponseV1> {
  if (!STABLE_ID.test(taskId)) throw new Error('Agent approval task identity does not match V1.');
  return parseAgentApprovalResponseV1(
    await invokeCommand('query_agent_approval', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, taskId },
    }),
  );
}

export async function controlAgentApproval(
  taskId: string,
  approval: AgentApprovalV1,
  action: AgentApprovalControlActionV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentApprovalControlResponseV1> {
  if (!STABLE_ID.test(taskId) || !isControlAction(action)) {
    throw new Error('Agent approval control does not match V1.');
  }
  const request = {
    action,
    expectedApprovalRevision: approval.approvalRevision,
    expectedLedgerRevision: approval.ledgerRevision,
    expectedLedgerStoreVersion: approval.ledgerStoreVersion,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    taskId,
  };
  return parseAgentApprovalControlResponseV1(
    await invokeCommand('control_agent_approval', { request }),
  );
}

export function parseAgentApprovalResponseV1(payload: unknown): AgentApprovalResponseV1 {
  const result = parseEnvelope(payload);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseQueryResult(result) };
}

export function parseAgentApprovalControlResponseV1(
  payload: unknown,
): AgentApprovalControlResponseV1 {
  const result = parseEnvelope(payload);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseControlResult(result) };
}

function parseEnvelope(payload: unknown): unknown {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Agent approval response does not match V1.');
  }
  return payload.result;
}

function parseQueryResult(value: unknown): AgentApprovalResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') throw invalid();
  if (
    ['noProject', 'taskNotFound', 'ledgerUnavailable', 'activityChanged', 'unavailable'].includes(
      value.status,
    )
  ) {
    if (!hasExactKeys(value, ['status'])) throw invalid();
    return value as AgentApprovalResultV1;
  }
  if (value.status === 'goalRevisionMismatch') {
    if (
      !hasExactKeys(value, ['currentRevision', 'ledgerRevision', 'status']) ||
      !isPositiveU32(value.currentRevision) ||
      !isPositiveU32(value.ledgerRevision)
    )
      throw invalid();
    return {
      currentRevision: value.currentRevision,
      ledgerRevision: value.ledgerRevision,
      status: value.status,
    };
  }
  if (value.status === 'available' && hasExactKeys(value, ['approval', 'status'])) {
    return { approval: parseApproval(value.approval), status: 'available' };
  }
  throw invalid();
}

function parseControlResult(value: unknown): AgentApprovalControlResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') throw invalid();
  if (
    [
      'noProject',
      'taskNotFound',
      'ledgerUnavailable',
      'goalRevisionMismatch',
      'activityChanged',
      'unavailable',
    ].includes(value.status)
  ) {
    if (!hasExactKeys(value, ['status'])) throw invalid();
    return value as AgentApprovalControlResultV1;
  }
  if (
    value.status === 'applied' &&
    hasExactKeys(value, [
      'approvalRevision',
      'ledgerStoreVersion',
      'outcome',
      'runtimeStart',
      'status',
    ]) &&
    isPositiveDecimal(value.approvalRevision) &&
    isPositiveDecimal(value.ledgerStoreVersion) &&
    ['grantStored', 'denied', 'revoked', 'continueRequested'].includes(String(value.outcome)) &&
    (value.runtimeStart === null ||
      ['unavailable', 'queued', 'failed'].includes(String(value.runtimeStart)))
  ) {
    if ((value.outcome === 'continueRequested') !== (value.runtimeStart !== null)) throw invalid();
    return value as AgentApprovalControlResultV1;
  }
  throw invalid();
}

function parseApproval(value: unknown): AgentApprovalV1 {
  const keys = [
    'action',
    'actionClass',
    'approvalRevision',
    'canAllowOnce',
    'canContinue',
    'canDeny',
    'canRevoke',
    'controllerState',
    'expiresAtUnixMillis',
    'ledgerRevision',
    'ledgerStoreVersion',
    'reason',
    'requestedAtUnixMillis',
    'risk',
    'scopeDigest',
    'snapshotId',
    'status',
    'stepId',
    'stepStatus',
  ];
  if (
    !isRecord(value) ||
    !hasExactKeys(value, keys) ||
    !isPositiveDecimal(value.approvalRevision) ||
    !isPositiveU32(value.ledgerRevision) ||
    !isPositiveDecimal(value.ledgerStoreVersion) ||
    !isControllerState(value.controllerState) ||
    !isStepStatus(value.stepStatus) ||
    !STABLE_ID.test(String(value.stepId)) ||
    !STABLE_ID.test(String(value.snapshotId)) ||
    !HEX_32.test(String(value.scopeDigest)) ||
    !isActionClass(value.actionClass) ||
    !['low', 'moderate', 'high', 'critical'].includes(String(value.risk)) ||
    !['systemPolicy', 'workspacePolicy'].includes(String(value.reason)) ||
    !isDecimal(value.requestedAtUnixMillis) ||
    !isDecimal(value.expiresAtUnixMillis) ||
    !['pending', 'active', 'consumed', 'revoked', 'expired', 'denied'].includes(
      String(value.status),
    ) ||
    ![value.canAllowOnce, value.canDeny, value.canContinue, value.canRevoke].every(
      (item) => typeof item === 'boolean',
    )
  )
    throw invalid();
  if (BigInt(value.requestedAtUnixMillis) >= BigInt(value.expiresAtUnixMillis)) throw invalid();
  const status = String(value.status);
  const pendingControls = value.canAllowOnce === true && value.canDeny === true;
  const activeControls = value.canContinue === true && value.canRevoke === true;
  if (
    (status === 'pending' && (!pendingControls || activeControls)) ||
    (status === 'active' && (pendingControls || !activeControls)) ||
    (!['pending', 'active'].includes(status) && (pendingControls || activeControls))
  )
    throw invalid();
  const action = parseAction(value.action);
  if (
    action.kind === 'process' &&
    action.process.planBinding.kind === 'validated' &&
    action.process.planBinding.stepId !== value.stepId
  )
    throw invalid();
  return { ...(value as Omit<AgentApprovalV1, 'action'>), action };
}

function parseAction(value: unknown): AgentApprovalActionV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') throw invalid();
  if (
    value.kind === 'patch' &&
    hasExactKeys(value, ['kind', 'patch']) &&
    isRecord(value.patch) &&
    hasExactKeys(value.patch, ['files', 'rationale']) &&
    isBoundedText(value.patch.rationale, 4 * 1024) &&
    Array.isArray(value.patch.files) &&
    value.patch.files.length > 0 &&
    value.patch.files.length <= MAX_FILES
  ) {
    return {
      kind: 'patch',
      patch: { rationale: value.patch.rationale, files: value.patch.files.map(parseFile) },
    };
  }
  if (value.kind === 'process' && hasExactKeys(value, ['kind', 'process'])) {
    return { kind: 'process', process: parseProcess(value.process) };
  }
  throw invalid();
}

function parseFile(value: unknown): AgentApprovalFileV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['operation', 'sourcePath', 'targetPath']) ||
    !['add', 'update', 'move', 'delete'].includes(String(value.operation))
  )
    throw invalid();
  const sourcePath = value.sourcePath === null ? null : parsePath(value.sourcePath);
  const targetPath = value.targetPath === null ? null : parsePath(value.targetPath);
  const shape =
    value.operation === 'add'
      ? sourcePath === null && targetPath !== null
      : value.operation === 'delete'
        ? sourcePath !== null && targetPath === null
        : sourcePath !== null && targetPath !== null;
  if (!shape) throw invalid();
  if (
    value.operation === 'update' &&
    sourcePath !== null &&
    targetPath !== null &&
    sourcePath.pathHex !== targetPath.pathHex
  )
    throw invalid();
  return { operation: value.operation as AgentApprovalFileV1['operation'], sourcePath, targetPath };
}

function parseProcess(
  value: unknown,
): Extract<AgentApprovalActionV1, { kind: 'process' }>['process'] {
  const keys = [
    'arguments',
    'environmentAllowlist',
    'executable',
    'executionMode',
    'network',
    'planBinding',
    'processKind',
    'specificationId',
    'stderrLimit',
    'stdoutLimit',
    'timeoutMillis',
    'workingDirectory',
  ];
  if (
    !isRecord(value) ||
    !hasExactKeys(value, keys) ||
    !isProcessExecutable(value.executable) ||
    !Array.isArray(value.arguments) ||
    value.arguments.length > MAX_ARGUMENTS ||
    !value.arguments.every(isProcessArgument) ||
    !Array.isArray(value.environmentAllowlist) ||
    value.environmentAllowlist.length > MAX_ENVIRONMENT_NAMES ||
    !value.environmentAllowlist.every(isEnvironmentName) ||
    !isPositiveDecimal(value.timeoutMillis) ||
    BigInt(value.timeoutMillis) > MAX_TIMEOUT_MILLIS ||
    !isPositiveBoundedOutput(value.stdoutLimit) ||
    !isPositiveBoundedOutput(value.stderrLimit) ||
    !['test', 'build', 'diagnostic', 'lint', 'format', 'command'].includes(
      String(value.processKind),
    ) ||
    !['knownSafe', 'open', 'shell'].includes(String(value.executionMode)) ||
    !STABLE_ID.test(String(value.specificationId))
  )
    throw invalid();
  const argvBytes = value.arguments.reduce(
    (total, argument) => total + utf8.encode(argument).length + 1,
    utf8.encode(value.executable).length,
  );
  if (argvBytes > MAX_TOTAL_ARGV_BYTES) throw invalid();
  if (new Set(value.environmentAllowlist).size !== value.environmentAllowlist.length)
    throw invalid();
  return {
    ...(value as Omit<
      Extract<AgentApprovalActionV1, { kind: 'process' }>['process'],
      'workingDirectory' | 'planBinding' | 'network'
    >),
    workingDirectory: parseWorkingDirectory(value.workingDirectory),
    planBinding: parsePlanBinding(value.planBinding),
    network: parseNetwork(value.network),
  };
}

function parsePath(value: unknown): AgentInspectionPathV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['displayPath', 'pathHex']) ||
    typeof value.displayPath !== 'string' ||
    !/^(?:[0-9a-f]{2})+$/u.test(String(value.pathHex)) ||
    utf8.encode(value.displayPath).length > 524_288
  )
    throw invalid();
  return { displayPath: value.displayPath, pathHex: String(value.pathHex) };
}

function parseWorkingDirectory(
  value: unknown,
): Extract<AgentApprovalActionV1, { kind: 'process' }>['process']['workingDirectory'] {
  if (!isRecord(value) || typeof value.kind !== 'string') throw invalid();
  if (value.kind === 'root' && hasExactKeys(value, ['kind'])) return { kind: 'root' };
  if (value.kind === 'subtree' && hasExactKeys(value, ['kind', 'path']))
    return { kind: 'subtree', path: parsePath(value.path) };
  throw invalid();
}

function parsePlanBinding(
  value: unknown,
): Extract<AgentApprovalActionV1, { kind: 'process' }>['process']['planBinding'] {
  if (!isRecord(value) || typeof value.kind !== 'string') throw invalid();
  if (value.kind === 'unbound' && hasExactKeys(value, ['kind'])) return { kind: 'unbound' };
  if (
    value.kind === 'validated' &&
    hasExactKeys(value, ['kind', 'stepId']) &&
    STABLE_ID.test(String(value.stepId))
  )
    return { kind: 'validated', stepId: String(value.stepId) };
  throw invalid();
}

function parseNetwork(
  value: unknown,
): Extract<AgentApprovalActionV1, { kind: 'process' }>['process']['network'] {
  if (!isRecord(value) || typeof value.kind !== 'string') throw invalid();
  if (value.kind === 'denied' && hasExactKeys(value, ['kind'])) return { kind: 'denied' };
  if (
    value.kind === 'requested' &&
    hasExactKeys(value, ['kind', 'scopeDigest']) &&
    STABLE_ID.test(String(value.scopeDigest))
  )
    return { kind: 'requested', scopeDigest: String(value.scopeDigest) };
  throw invalid();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}
function isPositiveDecimal(value: unknown): value is string {
  return typeof value === 'string' && POSITIVE_DECIMAL.test(value) && BigInt(value) <= MAX_U64;
}
function isDecimal(value: unknown): value is string {
  return typeof value === 'string' && DECIMAL.test(value) && BigInt(value) <= MAX_U64;
}
function isU32(value: unknown): value is number {
  return Number.isInteger(value) && Number(value) >= 0 && Number(value) <= MAX_U32;
}
function isPositiveU32(value: unknown): value is number {
  return isU32(value) && value > 0;
}
function isBoundedText(value: unknown, max: number): value is string {
  return typeof value === 'string' && value.length > 0 && utf8.encode(value).length <= max;
}
function isProcessExecutable(value: unknown): value is string {
  return (
    isBoundedText(value, 4 * 1024) &&
    ![...value].some((character) => character === '\0' || /\p{Cc}/u.test(character))
  );
}
function isProcessArgument(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    utf8.encode(value).length <= MAX_ARGUMENT_BYTES &&
    !value.includes('\0')
  );
}
function isEnvironmentName(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    utf8.encode(value).length <= MAX_ENVIRONMENT_NAME_BYTES &&
    ENVIRONMENT_NAME.test(value)
  );
}
function isPositiveBoundedOutput(value: unknown): value is number {
  return isU32(value) && value > 0 && value <= MAX_OUTPUT_BYTES;
}
function isControlAction(value: unknown): value is AgentApprovalControlActionV1 {
  return ['allowOnce', 'deny', 'continue', 'revoke'].includes(String(value));
}
function isActionClass(value: unknown): value is AgentApprovalActionClassV1 {
  return [
    'read',
    'derive',
    'write',
    'executeSafe',
    'executeOpen',
    'network',
    'destructive',
    'publish',
    'outsideRoot',
  ].includes(String(value));
}
function isStepStatus(value: unknown): value is AgentApprovalStepStatusV1 {
  return [
    'pending',
    'ready',
    'inProgress',
    'blocked',
    'awaitingApproval',
    'verifying',
    'completed',
    'failed',
    'cancelled',
    'stale',
  ].includes(String(value));
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
  ].includes(String(value));
}
function invalid(): Error {
  return new Error('Agent approval response does not match V1.');
}

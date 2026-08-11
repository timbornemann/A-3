import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const POSITIVE_DECIMAL_PATTERN = /^[1-9][0-9]{0,19}$/;
const DECIMAL_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAX_U64 = 18_446_744_073_709_551_615n;
const MAX_PERSISTED_MILLIS = 9_223_372_036_854_775_807n;
const MAX_U32 = 4_294_967_295;
const MAX_EVENTS = 64;
const MAX_BLOCKERS = 256;
const MAX_BLOCKER_BYTES = 4 * 1_024;
const utf8 = new TextEncoder();

export type AgentControllerStateV1 =
  | 'intake'
  | 'localize'
  | 'plan'
  | 'execute'
  | 'verify'
  | 'replan'
  | 'awaitApproval'
  | 'done'
  | 'failed'
  | 'cancelled';

export type AgentActivityOutcomeV1 = 'succeeded' | 'failed' | 'cancelled' | 'denied';
export type AgentActivityCodeV1 =
  | 'none'
  | 'userRequest'
  | 'controllerDecision'
  | 'policyDecision'
  | 'timeout'
  | 'cancellation'
  | 'invalidModelOutput'
  | 'toolFailure'
  | 'verificationFailure'
  | 'stateRecovered';
export type AgentSelectedActionV1 =
  'search' | 'inspect' | 'updateLedger' | 'finish' | 'applyPatch' | 'run';

export interface AgentActivityTurnV1 {
  outputTokens: number;
  promptTokens: number;
  repairUsed: boolean;
  selectedAction: AgentSelectedActionV1 | null;
}

export type AgentActivityEventKindV1 =
  | { kind: 'runStarted' }
  | { from: AgentControllerStateV1; kind: 'stateTransition'; to: AgentControllerStateV1 }
  | { kind: 'contextCompiled' }
  | { kind: 'modelInteraction'; turn: AgentActivityTurnV1 | null }
  | { kind: 'toolAction' }
  | { fromRevision: number; kind: 'ledgerUpdated'; toRevision: number }
  | { kind: 'verificationRecorded' }
  | { kind: 'approvalRecorded' }
  | { kind: 'diagnostic' };

export interface AgentActivityEventV1 {
  code: AgentActivityCodeV1;
  event: AgentActivityEventKindV1;
  occurredAtUnixMillis: string;
  outcome: AgentActivityOutcomeV1 | null;
  sequence: string;
  snapshotId: string;
}

export interface AgentActivityBudgetV1 {
  actionLimit: number;
  durationLimitMillis: string;
  outputTokenLimit: string;
  promptTokenLimit: string;
  repairLimit: number;
  turnLimit: number;
}

export interface AgentActivityUsageV1 {
  actionCount: number;
  elapsedAtLastEventMillis: string;
  outputTokens: string;
  promptTokens: string;
  repairCount: number;
  turnCount: number;
}

export interface AgentActivityBlockerV1 {
  reason: string;
  status: 'blocked' | 'awaitingApproval';
  stepId: string;
}

export interface AgentActivityRunV1 {
  attemptNumber: number;
  budget: AgentActivityBudgetV1;
  createdAtUnixMillis: string;
  currentSnapshotId: string;
  earlierEventsOmitted: boolean;
  ledgerRevision: number;
  ledgerRevisionMatchesCurrent: boolean;
  runId: string;
  state: AgentControllerStateV1;
  stepId: string;
  terminal: boolean;
  timeline: AgentActivityEventV1[];
  updatedAtUnixMillis: string;
  usage: AgentActivityUsageV1;
}

export interface AgentActivityV1 {
  blockers: AgentActivityBlockerV1[];
  currentLedgerRevision: number;
  ledgerStoreVersion: string;
  run: AgentActivityRunV1 | null;
}

export type AgentActivityResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | { currentRevision: number; ledgerRevision: number; status: 'goalRevisionMismatch' }
  | { status: 'activityChanged' }
  | { activity: AgentActivityV1; status: 'available' };

export interface AgentActivityResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentActivityResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentActivity(
  taskId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentActivityResponseV1> {
  if (!isStableId(taskId)) throw new Error('Agent activity task identity does not match V1.');
  const request = { protocolVersion: CURRENT_PROTOCOL_VERSION, taskId };
  return parseAgentActivityResponseV1(await invokeCommand('query_agent_activity', { request }));
}

export function parseAgentActivityResponseV1(payload: unknown): AgentActivityResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Agent activity response does not match V1.');
  }
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseResult(payload.result) };
}

function parseResult(value: unknown): AgentActivityResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  if (
    ['noProject', 'taskNotFound', 'ledgerUnavailable', 'activityChanged'].includes(value.status) &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status } as AgentActivityResultV1;
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
  if (value.status === 'available' && hasExactKeys(value, ['activity', 'status'])) {
    return { activity: parseActivity(value.activity), status: 'available' };
  }
  return invalidResult();
}

function parseActivity(value: unknown): AgentActivityV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['blockers', 'currentLedgerRevision', 'ledgerStoreVersion', 'run']) ||
    !isPositiveU32(value.currentLedgerRevision) ||
    !isPositiveDecimal(value.ledgerStoreVersion) ||
    !Array.isArray(value.blockers) ||
    value.blockers.length > MAX_BLOCKERS
  ) {
    throw new Error('Agent activity ledger projection is invalid.');
  }
  const blockers = value.blockers.map(parseBlocker);
  if (new Set(blockers.map((blocker) => blocker.stepId)).size !== blockers.length) {
    throw new Error('Agent activity blockers contain duplicate steps.');
  }
  const run = value.run === null ? null : parseRun(value.run, value.currentLedgerRevision);
  return {
    blockers,
    currentLedgerRevision: value.currentLedgerRevision,
    ledgerStoreVersion: value.ledgerStoreVersion,
    run,
  };
}

function parseBlocker(value: unknown): AgentActivityBlockerV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['reason', 'status', 'stepId']) ||
    !isStableId(value.stepId) ||
    (value.status !== 'blocked' && value.status !== 'awaitingApproval') ||
    !isBoundedText(value.reason, MAX_BLOCKER_BYTES)
  ) {
    throw new Error('Agent activity blocker is invalid.');
  }
  return { reason: value.reason, status: value.status, stepId: value.stepId };
}

function parseRun(value: unknown, currentLedgerRevision: number): AgentActivityRunV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'attemptNumber',
      'budget',
      'createdAtUnixMillis',
      'currentSnapshotId',
      'earlierEventsOmitted',
      'ledgerRevision',
      'ledgerRevisionMatchesCurrent',
      'runId',
      'state',
      'stepId',
      'terminal',
      'timeline',
      'updatedAtUnixMillis',
      'usage',
    ]) ||
    !isStableId(value.runId) ||
    !isStableId(value.stepId) ||
    !isStableId(value.currentSnapshotId) ||
    !isPositiveU32(value.attemptNumber) ||
    !isPositiveU32(value.ledgerRevision) ||
    value.ledgerRevision > currentLedgerRevision ||
    typeof value.ledgerRevisionMatchesCurrent !== 'boolean' ||
    value.ledgerRevisionMatchesCurrent !== (value.ledgerRevision === currentLedgerRevision) ||
    !isControllerState(value.state) ||
    typeof value.terminal !== 'boolean' ||
    value.terminal !== isTerminalState(value.state) ||
    typeof value.earlierEventsOmitted !== 'boolean' ||
    !Array.isArray(value.timeline) ||
    value.timeline.length === 0 ||
    value.timeline.length > MAX_EVENTS
  ) {
    throw new Error('Agent activity run projection is invalid.');
  }
  const createdAtUnixMillis = parsePersistedMillis(value.createdAtUnixMillis);
  const updatedAtUnixMillis = parsePersistedMillis(value.updatedAtUnixMillis);
  if (BigInt(updatedAtUnixMillis) < BigInt(createdAtUnixMillis)) {
    throw new Error('Agent activity run timestamps regressed.');
  }
  const timeline = value.timeline.map(parseEvent);
  validateTimeline(timeline, value.earlierEventsOmitted, value.currentSnapshotId);
  return {
    attemptNumber: value.attemptNumber,
    budget: parseBudget(value.budget),
    createdAtUnixMillis,
    currentSnapshotId: value.currentSnapshotId,
    earlierEventsOmitted: value.earlierEventsOmitted,
    ledgerRevision: value.ledgerRevision,
    ledgerRevisionMatchesCurrent: value.ledgerRevisionMatchesCurrent,
    runId: value.runId,
    state: value.state,
    stepId: value.stepId,
    terminal: value.terminal,
    timeline,
    updatedAtUnixMillis,
    usage: parseUsage(value.usage),
  };
}

function parseBudget(value: unknown): AgentActivityBudgetV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'actionLimit',
      'durationLimitMillis',
      'outputTokenLimit',
      'promptTokenLimit',
      'repairLimit',
      'turnLimit',
    ]) ||
    !isPositiveU32(value.actionLimit) ||
    !isPositiveU32(value.repairLimit) ||
    !isPositiveU32(value.turnLimit) ||
    !isPositiveDecimal(value.durationLimitMillis) ||
    !isPositiveDecimal(value.outputTokenLimit) ||
    !isPositiveDecimal(value.promptTokenLimit)
  ) {
    throw new Error('Agent activity budget is invalid.');
  }
  return value as unknown as AgentActivityBudgetV1;
}

function parseUsage(value: unknown): AgentActivityUsageV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'actionCount',
      'elapsedAtLastEventMillis',
      'outputTokens',
      'promptTokens',
      'repairCount',
      'turnCount',
    ]) ||
    !isU32(value.actionCount) ||
    !isU32(value.repairCount) ||
    !isU32(value.turnCount) ||
    value.actionCount > value.turnCount ||
    value.repairCount > value.turnCount ||
    !isDecimal(value.elapsedAtLastEventMillis) ||
    !isDecimal(value.outputTokens) ||
    !isDecimal(value.promptTokens)
  ) {
    throw new Error('Agent activity usage is invalid.');
  }
  return value as unknown as AgentActivityUsageV1;
}

function parseEvent(value: unknown): AgentActivityEventV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'code',
      'event',
      'occurredAtUnixMillis',
      'outcome',
      'sequence',
      'snapshotId',
    ]) ||
    !isActivityCode(value.code) ||
    (value.outcome !== null && !isActivityOutcome(value.outcome)) ||
    !isPositiveDecimal(value.sequence) ||
    !isStableId(value.snapshotId)
  ) {
    throw new Error('Agent activity timeline event is invalid.');
  }
  return {
    code: value.code,
    event: parseEventKind(value.event),
    occurredAtUnixMillis: parsePersistedMillis(value.occurredAtUnixMillis),
    outcome: value.outcome,
    sequence: value.sequence,
    snapshotId: value.snapshotId,
  };
}

function parseEventKind(value: unknown): AgentActivityEventKindV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') throw new Error('Invalid event kind.');
  if (
    [
      'runStarted',
      'contextCompiled',
      'toolAction',
      'verificationRecorded',
      'approvalRecorded',
      'diagnostic',
    ].includes(value.kind) &&
    hasExactKeys(value, ['kind'])
  ) {
    return { kind: value.kind } as AgentActivityEventKindV1;
  }
  if (
    value.kind === 'stateTransition' &&
    hasExactKeys(value, ['from', 'kind', 'to']) &&
    isControllerState(value.from) &&
    isControllerState(value.to) &&
    value.from !== value.to
  ) {
    return { from: value.from, kind: 'stateTransition', to: value.to };
  }
  if (
    value.kind === 'ledgerUpdated' &&
    hasExactKeys(value, ['fromRevision', 'kind', 'toRevision']) &&
    isPositiveU32(value.fromRevision) &&
    value.fromRevision < MAX_U32 &&
    value.toRevision === value.fromRevision + 1
  ) {
    return {
      fromRevision: value.fromRevision,
      kind: 'ledgerUpdated',
      toRevision: value.toRevision,
    };
  }
  if (value.kind === 'modelInteraction' && hasExactKeys(value, ['kind', 'turn'])) {
    return {
      kind: 'modelInteraction',
      turn: value.turn === null ? null : parseTurn(value.turn),
    };
  }
  throw new Error('Agent activity event kind is invalid.');
}

function parseTurn(value: unknown): AgentActivityTurnV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['outputTokens', 'promptTokens', 'repairUsed', 'selectedAction']) ||
    !isU32(value.outputTokens) ||
    !isU32(value.promptTokens) ||
    typeof value.repairUsed !== 'boolean' ||
    (value.selectedAction !== null && !isSelectedAction(value.selectedAction))
  ) {
    throw new Error('Agent activity model-turn charge is invalid.');
  }
  return {
    outputTokens: value.outputTokens,
    promptTokens: value.promptTokens,
    repairUsed: value.repairUsed,
    selectedAction: value.selectedAction,
  };
}

function validateTimeline(
  events: AgentActivityEventV1[],
  earlierEventsOmitted: boolean,
  currentSnapshotId: string,
): void {
  if (
    (!earlierEventsOmitted && events[0].sequence !== '1') ||
    events.at(-1)?.snapshotId !== currentSnapshotId
  ) {
    throw new Error('Agent activity timeline does not match its run anchors.');
  }
  for (let index = 1; index < events.length; index += 1) {
    if (BigInt(events[index].sequence) !== BigInt(events[index - 1].sequence) + 1n) {
      throw new Error('Agent activity timeline is not contiguous.');
    }
    if (
      BigInt(events[index].occurredAtUnixMillis) < BigInt(events[index - 1].occurredAtUnixMillis)
    ) {
      throw new Error('Agent activity timeline timestamps regressed.');
    }
  }
}

function parsePersistedMillis(value: unknown): string {
  if (
    typeof value !== 'string' ||
    !DECIMAL_PATTERN.test(value) ||
    BigInt(value) > MAX_PERSISTED_MILLIS
  ) {
    throw new Error('Agent activity timestamp is invalid.');
  }
  return value;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isPositiveDecimal(value: unknown): value is string {
  return (
    typeof value === 'string' && POSITIVE_DECIMAL_PATTERN.test(value) && BigInt(value) <= MAX_U64
  );
}

function isDecimal(value: unknown): value is string {
  return typeof value === 'string' && DECIMAL_PATTERN.test(value) && BigInt(value) <= MAX_U64;
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

function isActivityCode(value: unknown): value is AgentActivityCodeV1 {
  return [
    'none',
    'userRequest',
    'controllerDecision',
    'policyDecision',
    'timeout',
    'cancellation',
    'invalidModelOutput',
    'toolFailure',
    'verificationFailure',
    'stateRecovered',
  ].includes(value as string);
}

function isActivityOutcome(value: unknown): value is AgentActivityOutcomeV1 {
  return ['succeeded', 'failed', 'cancelled', 'denied'].includes(value as string);
}

function isSelectedAction(value: unknown): value is AgentSelectedActionV1 {
  return ['search', 'inspect', 'updateLedger', 'finish', 'applyPatch', 'run'].includes(
    value as string,
  );
}

function isBoundedText(value: unknown, maximumBytes: number): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value === value.replace(/\r\n?/gu, '\n').trim() &&
    utf8.encode(value).length <= maximumBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return code !== undefined && ((code < 32 && code !== 9 && code !== 10) || code === 127);
    })
  );
}

function invalidResult(): never {
  throw new Error('Agent activity result uses an unsupported state.');
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

import { invoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const PROFILE_ID = /^[0-9a-f]{64}$/;
const PROVIDER_ID = /^[A-Za-z0-9._-]{1,128}$/;
const MODEL_ID = /^[A-Za-z0-9._/@:-]{1,512}$/;
const RUN_SELECTION = /^[0-9a-f]{96}$/;
const ENTRY_SELECTION = /^[0-9a-f]{48}$/;
const RUN_CURSOR = /^[0-9a-f]{112}$/;
const U64_DECIMAL = /^(0|[1-9][0-9]{0,19})$/;
const U64_MAX = BigInt('18446744073709551615');

export type DeepMapModeV2 = 'fast' | 'standard' | 'thorough';
export type DeepMapPhaseV2 = 'planning' | 'exploring' | 'claiming' | 'verifying' | 'publishing';
export type DeepMapTargetKindV2 = 'project' | 'module' | 'manifest' | 'symbol';
export type DeepMapSafeActionV2 =
  | 'buildPlan'
  | 'inspect'
  | 'search'
  | 'propose'
  | 'generateClaims'
  | 'verifyEvidence'
  | 'publishCards';
export type DeepMapFailureV3 =
  | 'noPublishedIndex'
  | 'staleIndex'
  | 'planning'
  | 'modelUnavailable'
  | 'modelRejected'
  | 'modelTimeout'
  | 'invalidModelResponse'
  | 'read'
  | 'verification'
  | 'publicationRejected'
  | 'publicationStorage'
  | 'publicationTimeout'
  | 'publicationProgress'
  | 'invalidCheckpoint'
  | 'progressUnavailable'
  | 'interrupted';
export type DeepMapRunStateV1 =
  | 'queued'
  | 'running'
  | 'pausing'
  | 'paused'
  | 'cancelling'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'interrupted';
export type DeepMapEventResultV1 =
  | 'pending'
  | 'confirmed'
  | 'alreadyCurrent'
  | 'published'
  | 'paused'
  | 'resumed'
  | 'cancelled'
  | 'failed'
  | 'interrupted';

export interface DeepMapModelV1 {
  profileId: string;
  profileVersion: number;
  providerId: string;
  modelId: string;
  contextTokens: number;
  outputTokens: number;
}

export interface DeepMapCompactProgressV3 {
  confirmedSteps: string;
  totalSteps: string;
  phase: DeepMapPhaseV2 | null;
  action: DeepMapSafeActionV2 | null;
}

type ActiveLifecycleStateV3 =
  'queued' | 'running' | 'pausing' | 'paused' | 'cancelling' | 'succeeded' | 'cancelled';
export type DeepMapLifecycleV3 =
  | { state: 'ready' }
  | { state: 'current'; cardCount: string; detailsAvailable: boolean }
  | {
      state: ActiveLifecycleStateV3;
      progress: DeepMapCompactProgressV3;
      detailsIncomplete: boolean;
    }
  | {
      state: 'failed';
      progress: DeepMapCompactProgressV3;
      failure: DeepMapFailureV3;
      detailsIncomplete: boolean;
    };

export type DeepMapStatusResponseV3 = {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result:
    | { status: 'noProject' }
    | { status: 'unavailable' }
    | { status: 'available'; model: DeepMapModelV1; lifecycle: DeepMapLifecycleV3 };
};

export interface DeepMapStartResponseV2 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  outcome: 'queued' | 'alreadyCurrent';
}

export interface DeepMapControlResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  accepted: true;
}

export interface DeepMapRunV1 {
  selection: string;
  mode: DeepMapModeV2;
  state: DeepMapRunStateV1;
  startedAtUnixMillis: string;
  updatedAtUnixMillis: string;
  confirmedSteps: string;
  totalSteps: string;
  failure: DeepMapFailureV3 | null;
  detailsIncomplete: boolean;
}

export interface DeepMapRunPageResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  runs: DeepMapRunV1[];
  nextCursor: string | null;
}

export interface DeepMapEntryV1 {
  selection: string;
  sequence: string;
  state: DeepMapRunStateV1;
  occurredAtUnixMillis: string;
  phase: DeepMapPhaseV2 | null;
  action: DeepMapSafeActionV2 | null;
  targetKind: DeepMapTargetKindV2 | null;
  stepPosition: string | null;
  totalSteps: string | null;
  confirmed: boolean;
  result: DeepMapEventResultV1;
  failure: DeepMapFailureV3 | null;
}

export interface DeepMapEntryPageResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  entries: DeepMapEntryV1[];
  nextCursor: string | null;
}

export interface DeepMapStepDetailV1 {
  targetKind: DeepMapTargetKindV2;
  seedReason:
    'manifest' | 'entrypoint' | 'centralSymbol' | 'testRoot' | 'graphCommunity' | 'uncoveredModule';
  reservedTokens: number;
  reservedTimeMillis: string;
  reservedToolCalls: number;
  informationGainBasisPoints: number;
  coverageFieldCount: number;
  evidenceRequirement: 'fieldEvidence';
  verificationMethod: 'publishedIndexEvidence';
  confirmed: boolean;
}

export interface DeepMapEntryDetailResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  run: DeepMapRunV1;
  entry: DeepMapEntryV1;
  durationMillis: string;
  providerId: string;
  modelId: string;
  profileId: string;
  profileVersion: number;
  tokenBudget: number;
  timeBudgetMillis: string;
  toolCallBudget: number;
  indexReference: string;
  snapshotReference: string;
  nextAction: string | null;
  planStopReason:
    'coveragePlanned' | 'budgetExhausted' | 'belowGainThreshold' | 'noEligibleSeed' | null;
  publicationResult: 'published' | 'alreadyCurrent' | null;
  step: DeepMapStepDetailV1 | null;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  invoke<unknown>(command, arguments_);

export async function queryDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapStatusResponseV3> {
  return parseDeepMapStatusResponseV3(
    await invokeCommand('query_deep_map', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
}

export async function startDeepMap(
  mode: DeepMapModeV2,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapStartResponseV2> {
  const object = record(
    await invokeCommand('start_deep_map', {
      request: { mode, protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
    ['outcome', 'protocolVersion'],
  );
  if (
    object.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !oneOf(object.outcome, ['queued', 'alreadyCurrent'] as const)
  ) {
    throw new Error('Invalid Deep Map start response');
  }
  return object as unknown as DeepMapStartResponseV2;
}

export async function pauseDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('pause_deep_map', invokeCommand);
}

export async function resumeDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('resume_deep_map', invokeCommand);
}

export async function cancelDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('cancel_deep_map', invokeCommand);
}

export async function queryDeepMapRuns(
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapRunPageResponseV1> {
  if (cursor !== null && !RUN_CURSOR.test(cursor)) throw new Error('Invalid Deep Map run cursor');
  return parseRunPage(
    await invokeCommand('query_deep_map_runs', {
      request: { cursor, protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
}

export async function queryDeepMapEntries(
  runSelection: string,
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapEntryPageResponseV1> {
  selection(runSelection, RUN_SELECTION, 'run');
  if (cursor !== null) selection(cursor, ENTRY_SELECTION, 'entry cursor');
  return parseEntryPage(
    await invokeCommand('query_deep_map_entries', {
      request: { cursor, protocolVersion: CURRENT_PROTOCOL_VERSION, runSelection },
    }),
  );
}

export async function queryDeepMapEntryDetail(
  runSelection: string,
  entrySelection: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapEntryDetailResponseV1> {
  selection(runSelection, RUN_SELECTION, 'run');
  selection(entrySelection, ENTRY_SELECTION, 'entry');
  return parseEntryDetail(
    await invokeCommand('query_deep_map_entry_detail', {
      request: { entrySelection, protocolVersion: CURRENT_PROTOCOL_VERSION, runSelection },
    }),
  );
}

async function control(
  command: string,
  invokeCommand: InvokeCommand,
): Promise<DeepMapControlResponseV1> {
  const object = record(
    await invokeCommand(command, {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
    ['accepted', 'protocolVersion'],
  );
  if (object.protocolVersion !== CURRENT_PROTOCOL_VERSION || object.accepted !== true) {
    throw new Error('Invalid Deep Map control response');
  }
  return { accepted: true, protocolVersion: CURRENT_PROTOCOL_VERSION };
}

export function parseDeepMapStatusResponseV3(value: unknown): DeepMapStatusResponseV3 {
  const root = record(value, ['protocolVersion', 'result']);
  version(root.protocolVersion);
  const result = record(root.result);
  if (result.status === 'noProject' || result.status === 'unavailable') {
    exactKeys(result, ['status']);
    return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: { status: result.status } };
  }
  if (result.status !== 'available') throw new Error('Invalid Deep Map availability state');
  exactKeys(result, ['lifecycle', 'model', 'status']);
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      status: 'available',
      model: parseModel(result.model),
      lifecycle: parseLifecycle(result.lifecycle),
    },
  };
}

function parseLifecycle(value: unknown): DeepMapLifecycleV3 {
  const object = record(value);
  if (object.state === 'ready') {
    exactKeys(object, ['state']);
    return { state: 'ready' };
  }
  if (object.state === 'current') {
    exactKeys(object, ['cardCount', 'detailsAvailable', 'state']);
    decimal(object.cardCount);
    if (typeof object.detailsAvailable !== 'boolean') throw new Error('Invalid current state');
    return object as unknown as DeepMapLifecycleV3;
  }
  const active = [
    'queued',
    'running',
    'pausing',
    'paused',
    'cancelling',
    'succeeded',
    'cancelled',
  ] as const;
  if (oneOf(object.state, active)) {
    exactKeys(object, ['detailsIncomplete', 'progress', 'state']);
    if (typeof object.detailsIncomplete !== 'boolean') throw new Error('Invalid journal state');
    return { ...object, progress: parseProgress(object.progress) } as DeepMapLifecycleV3;
  }
  if (object.state === 'failed') {
    exactKeys(object, ['detailsIncomplete', 'failure', 'progress', 'state']);
    if (typeof object.detailsIncomplete !== 'boolean') throw new Error('Invalid journal state');
    return {
      state: 'failed',
      progress: parseProgress(object.progress),
      failure: failure(object.failure),
      detailsIncomplete: object.detailsIncomplete,
    };
  }
  throw new Error('Invalid Deep Map lifecycle state');
}

function parseProgress(value: unknown): DeepMapCompactProgressV3 {
  const object = record(value, ['action', 'confirmedSteps', 'phase', 'totalSteps']);
  const confirmed = decimal(object.confirmedSteps);
  const total = decimal(object.totalSteps);
  if (confirmed > total) throw new Error('Invalid Deep Map progress');
  return {
    confirmedSteps: object.confirmedSteps as string,
    totalSteps: object.totalSteps as string,
    phase: nullableOneOf(object.phase, PHASES),
    action: nullableOneOf(object.action, ACTIONS),
  };
}

function parseModel(value: unknown): DeepMapModelV1 {
  const object = record(value, [
    'contextTokens',
    'modelId',
    'outputTokens',
    'profileId',
    'profileVersion',
    'providerId',
  ]);
  if (
    typeof object.profileId !== 'string' ||
    !PROFILE_ID.test(object.profileId) ||
    !integer(object.profileVersion, 1, 65_535) ||
    typeof object.providerId !== 'string' ||
    !PROVIDER_ID.test(object.providerId) ||
    typeof object.modelId !== 'string' ||
    !MODEL_ID.test(object.modelId) ||
    !integer(object.contextTokens, 1_024, 1_048_576) ||
    !integer(object.outputTokens, 1, 262_144) ||
    object.outputTokens > object.contextTokens
  ) {
    throw new Error('Invalid verified Deep Map model');
  }
  return object as unknown as DeepMapModelV1;
}

function parseRunPage(value: unknown): DeepMapRunPageResponseV1 {
  const object = record(value, ['nextCursor', 'protocolVersion', 'runs']);
  version(object.protocolVersion);
  if (!Array.isArray(object.runs) || object.runs.length > 20) {
    throw new Error('Invalid Deep Map run page');
  }
  const runs = object.runs.map(parseRun);
  for (let index = 1; index < runs.length; index += 1) {
    if (BigInt(runs[index - 1].updatedAtUnixMillis) < BigInt(runs[index].updatedAtUnixMillis)) {
      throw new Error('Invalid Deep Map run ordering');
    }
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    runs,
    nextCursor: nullableSelection(object.nextCursor, RUN_CURSOR, 'run cursor'),
  };
}

function parseRun(value: unknown): DeepMapRunV1 {
  const object = record(value, [
    'confirmedSteps',
    'detailsIncomplete',
    'failure',
    'mode',
    'selection',
    'startedAtUnixMillis',
    'state',
    'totalSteps',
    'updatedAtUnixMillis',
  ]);
  const state = requiredOneOf(object.state, RUN_STATES, 'run state');
  const confirmed = decimal(object.confirmedSteps);
  const total = decimal(object.totalSteps);
  const started = decimal(object.startedAtUnixMillis);
  const updated = decimal(object.updatedAtUnixMillis);
  const diagnosis = object.failure === null ? null : failure(object.failure);
  if (
    confirmed > total ||
    started > updated ||
    typeof object.detailsIncomplete !== 'boolean' ||
    (state === 'failed') !== (diagnosis !== null)
  ) {
    throw new Error('Contradictory Deep Map run');
  }
  return {
    selection: selection(object.selection, RUN_SELECTION, 'run'),
    mode: requiredOneOf(object.mode, MODES, 'mode'),
    state,
    startedAtUnixMillis: object.startedAtUnixMillis as string,
    updatedAtUnixMillis: object.updatedAtUnixMillis as string,
    confirmedSteps: object.confirmedSteps as string,
    totalSteps: object.totalSteps as string,
    failure: diagnosis,
    detailsIncomplete: object.detailsIncomplete,
  };
}

function parseEntryPage(value: unknown): DeepMapEntryPageResponseV1 {
  const object = record(value, ['entries', 'nextCursor', 'protocolVersion']);
  version(object.protocolVersion);
  if (!Array.isArray(object.entries) || object.entries.length > 50) {
    throw new Error('Invalid Deep Map entry page');
  }
  const entries = object.entries.map(parseEntry);
  for (let index = 1; index < entries.length; index += 1) {
    if (BigInt(entries[index - 1].sequence) >= BigInt(entries[index].sequence)) {
      throw new Error('Invalid Deep Map entry ordering');
    }
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    entries,
    nextCursor: nullableSelection(object.nextCursor, ENTRY_SELECTION, 'entry cursor'),
  };
}

function parseEntry(value: unknown): DeepMapEntryV1 {
  const object = record(value, [
    'action',
    'confirmed',
    'failure',
    'occurredAtUnixMillis',
    'phase',
    'result',
    'selection',
    'sequence',
    'state',
    'stepPosition',
    'targetKind',
    'totalSteps',
  ]);
  const state = requiredOneOf(object.state, RUN_STATES, 'entry state');
  const result = requiredOneOf(object.result, EVENT_RESULTS, 'entry result');
  const diagnosis = object.failure === null ? null : failure(object.failure);
  decimal(object.sequence);
  decimal(object.occurredAtUnixMillis);
  const stepPosition = object.stepPosition === null ? null : decimal(object.stepPosition);
  const totalSteps = object.totalSteps === null ? null : decimal(object.totalSteps);
  if (
    typeof object.confirmed !== 'boolean' ||
    (stepPosition === null) !== (totalSteps === null) ||
    (stepPosition !== null &&
      totalSteps !== null &&
      (stepPosition === BigInt(0) || stepPosition > totalSteps)) ||
    (state === 'failed') !== (diagnosis !== null) ||
    (result === 'failed') !== (diagnosis !== null)
  ) {
    throw new Error('Contradictory Deep Map entry');
  }
  return {
    selection: selection(object.selection, ENTRY_SELECTION, 'entry'),
    sequence: object.sequence as string,
    state,
    occurredAtUnixMillis: object.occurredAtUnixMillis as string,
    phase: nullableOneOf(object.phase, PHASES),
    action: nullableOneOf(object.action, ACTIONS),
    targetKind: nullableOneOf(object.targetKind, TARGET_KINDS),
    stepPosition: object.stepPosition as string | null,
    totalSteps: object.totalSteps as string | null,
    confirmed: object.confirmed,
    result,
    failure: diagnosis,
  };
}

function parseEntryDetail(value: unknown): DeepMapEntryDetailResponseV1 {
  const object = record(value, [
    'durationMillis',
    'entry',
    'indexReference',
    'modelId',
    'nextAction',
    'planStopReason',
    'profileId',
    'profileVersion',
    'protocolVersion',
    'providerId',
    'publicationResult',
    'run',
    'snapshotReference',
    'step',
    'timeBudgetMillis',
    'tokenBudget',
    'toolCallBudget',
  ]);
  version(object.protocolVersion);
  decimal(object.durationMillis);
  decimal(object.timeBudgetMillis);
  if (
    typeof object.providerId !== 'string' ||
    !PROVIDER_ID.test(object.providerId) ||
    typeof object.modelId !== 'string' ||
    !MODEL_ID.test(object.modelId) ||
    typeof object.profileId !== 'string' ||
    !PROFILE_ID.test(object.profileId) ||
    !integer(object.profileVersion, 1, 65_535) ||
    !integer(object.tokenBudget, 1, 1_000_000) ||
    !integer(object.toolCallBudget, 1, 4_096) ||
    typeof object.indexReference !== 'string' ||
    !/^[0-9a-f]{12}$/.test(object.indexReference) ||
    typeof object.snapshotReference !== 'string' ||
    !/^[0-9a-f]{12}$/.test(object.snapshotReference) ||
    (object.nextAction !== null &&
      (typeof object.nextAction !== 'string' || object.nextAction.length > 512))
  ) {
    throw new Error('Invalid Deep Map entry detail');
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    run: parseRun(object.run),
    entry: parseEntry(object.entry),
    durationMillis: object.durationMillis as string,
    providerId: object.providerId,
    modelId: object.modelId,
    profileId: object.profileId,
    profileVersion: object.profileVersion,
    tokenBudget: object.tokenBudget,
    timeBudgetMillis: object.timeBudgetMillis as string,
    toolCallBudget: object.toolCallBudget,
    indexReference: object.indexReference,
    snapshotReference: object.snapshotReference,
    nextAction: object.nextAction as string | null,
    planStopReason: nullableOneOf(object.planStopReason, PLAN_STOP_REASONS),
    publicationResult: nullableOneOf(object.publicationResult, PUBLICATION_RESULTS),
    step: object.step === null ? null : parseStepDetail(object.step),
  };
}

function parseStepDetail(value: unknown): DeepMapStepDetailV1 {
  const object = record(value, [
    'confirmed',
    'coverageFieldCount',
    'evidenceRequirement',
    'informationGainBasisPoints',
    'reservedTimeMillis',
    'reservedTokens',
    'reservedToolCalls',
    'seedReason',
    'targetKind',
    'verificationMethod',
  ]);
  decimal(object.reservedTimeMillis);
  if (
    !integer(object.reservedTokens, 0, 1_000_000) ||
    !integer(object.reservedToolCalls, 0, 4_096) ||
    !integer(object.informationGainBasisPoints, 0, 10_000) ||
    !integer(object.coverageFieldCount, 1, 65_535) ||
    object.evidenceRequirement !== 'fieldEvidence' ||
    object.verificationMethod !== 'publishedIndexEvidence' ||
    typeof object.confirmed !== 'boolean'
  ) {
    throw new Error('Invalid Deep Map step detail');
  }
  return {
    targetKind: requiredOneOf(object.targetKind, TARGET_KINDS, 'target kind'),
    seedReason: requiredOneOf(object.seedReason, SEED_REASONS, 'seed reason'),
    reservedTokens: object.reservedTokens,
    reservedTimeMillis: object.reservedTimeMillis as string,
    reservedToolCalls: object.reservedToolCalls,
    informationGainBasisPoints: object.informationGainBasisPoints,
    coverageFieldCount: object.coverageFieldCount,
    evidenceRequirement: 'fieldEvidence',
    verificationMethod: 'publishedIndexEvidence',
    confirmed: object.confirmed,
  };
}

const MODES = ['fast', 'standard', 'thorough'] as const;
const PHASES = ['planning', 'exploring', 'claiming', 'verifying', 'publishing'] as const;
const TARGET_KINDS = ['project', 'module', 'manifest', 'symbol'] as const;
const ACTIONS = [
  'buildPlan',
  'inspect',
  'search',
  'propose',
  'generateClaims',
  'verifyEvidence',
  'publishCards',
] as const;
const FAILURES = [
  'noPublishedIndex',
  'staleIndex',
  'planning',
  'modelUnavailable',
  'modelRejected',
  'modelTimeout',
  'invalidModelResponse',
  'read',
  'verification',
  'publicationRejected',
  'publicationStorage',
  'publicationTimeout',
  'publicationProgress',
  'invalidCheckpoint',
  'progressUnavailable',
  'interrupted',
] as const;
const RUN_STATES = [
  'queued',
  'running',
  'pausing',
  'paused',
  'cancelling',
  'succeeded',
  'failed',
  'cancelled',
  'interrupted',
] as const;
const EVENT_RESULTS = [
  'pending',
  'confirmed',
  'alreadyCurrent',
  'published',
  'paused',
  'resumed',
  'cancelled',
  'failed',
  'interrupted',
] as const;
const SEED_REASONS = [
  'manifest',
  'entrypoint',
  'centralSymbol',
  'testRoot',
  'graphCommunity',
  'uncoveredModule',
] as const;
const PLAN_STOP_REASONS = [
  'coveragePlanned',
  'budgetExhausted',
  'belowGainThreshold',
  'noEligibleSeed',
] as const;
const PUBLICATION_RESULTS = ['published', 'alreadyCurrent'] as const;

function failure(value: unknown): DeepMapFailureV3 {
  return requiredOneOf(value, FAILURES, 'failure');
}

function nullableSelection(value: unknown, pattern: RegExp, name: string): string | null {
  return value === null ? null : selection(value, pattern, name);
}

function selection(value: unknown, pattern: RegExp, name: string): string {
  if (typeof value !== 'string' || !pattern.test(value)) {
    throw new Error(`Invalid Deep Map ${name} selection`);
  }
  return value;
}

function decimal(value: unknown): bigint {
  if (typeof value !== 'string' || !U64_DECIMAL.test(value)) {
    throw new Error('Invalid lossless Deep Map count');
  }
  const parsed = BigInt(value);
  if (parsed > U64_MAX) throw new Error('Deep Map count exceeds u64');
  return parsed;
}

function requiredOneOf<const T extends readonly string[]>(
  value: unknown,
  allowed: T,
  name: string,
): T[number] {
  if (!oneOf(value, allowed)) throw new Error(`Invalid Deep Map ${name}`);
  return value;
}

function nullableOneOf<const T extends readonly string[]>(
  value: unknown,
  allowed: T,
): T[number] | null {
  if (value === null) return null;
  return requiredOneOf(value, allowed, 'enum value');
}

function oneOf<const T extends readonly string[]>(value: unknown, allowed: T): value is T[number] {
  return typeof value === 'string' && allowed.includes(value as T[number]);
}

function integer(value: unknown, minimum: number, maximum: number): value is number {
  return (
    Number.isSafeInteger(value) && (value as number) >= minimum && (value as number) <= maximum
  );
}

function version(value: unknown): void {
  if (value !== CURRENT_PROTOCOL_VERSION) throw new Error('Unsupported Deep Map protocol version');
}

function record(value: unknown, keys?: string[]): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Invalid Deep Map response object');
  }
  const object = value as Record<string, unknown>;
  if (keys !== undefined) exactKeys(object, keys);
  return object;
}

function exactKeys(value: Record<string, unknown>, expected: string[]): void {
  const actual = Object.keys(value).sort();
  const sorted = [...expected].sort();
  if (actual.length !== sorted.length || actual.some((key, index) => key !== sorted[index])) {
    throw new Error('Unexpected Deep Map response field');
  }
}

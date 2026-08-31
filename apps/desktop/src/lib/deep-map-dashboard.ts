import { invoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { DeepMapFailureV3, DeepMapSafeActionV2, DeepMapTargetKindV2 } from './deep-map';

const RUN_SELECTION = /^[0-9a-f]{96}$/;
const MODULE_SELECTION = /^[0-9a-f]{96}$/;
const NUMBER_CURSOR = /^[0-9a-f]{48}$/;
const U64 = /^(0|[1-9][0-9]{0,19})$/;
const PHASES = ['planning', 'exploring', 'creatingCards', 'verifying', 'updatingAtlas'] as const;
const PHASE_STATES = ['pending', 'active', 'completed', 'stopped'] as const;
const DASHBOARD_STATES = [
  'queued',
  'running',
  'pausing',
  'paused',
  'cancelling',
  'completed',
  'alreadyCurrent',
  'cancelled',
  'failed',
  'interrupted',
] as const;
const MODULE_STATES = ['planned', 'exploring', 'verifying', 'published', 'incomplete'] as const;
const STEP_STATES = ['planned', 'exploring', 'confirmed'] as const;
const REASONS = [
  'manifest',
  'entrypoint',
  'centralSymbol',
  'testRoot',
  'graphCommunity',
  'uncoveredModule',
] as const;
const FIELDS = [
  'title',
  'paths',
  'purpose',
  'responsibilities',
  'publicSurface',
  'entrypoints',
  'dependencies',
  'dataFlows',
  'invariants',
  'tests',
  'risks',
  'openQuestions',
] as const;
const ACTIONS = [
  'buildPlan',
  'inspect',
  'search',
  'propose',
  'generateClaims',
  'verifyEvidence',
  'publishCards',
] as const;
const TARGETS = ['project', 'module', 'manifest', 'symbol'] as const;
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

export type DeepMapDashboardPhaseV1 = (typeof PHASES)[number];
export type DeepMapDashboardPhaseStateV1 = (typeof PHASE_STATES)[number];
export type DeepMapDashboardStateV1 = (typeof DASHBOARD_STATES)[number];
export type DeepMapModuleStateV1 = (typeof MODULE_STATES)[number];
export type DeepMapPlanStepStateV1 = (typeof STEP_STATES)[number];
export type DeepMapSelectionReasonV1 = (typeof REASONS)[number];
export type DeepMapCardFieldV1 = (typeof FIELDS)[number];

export interface DeepMapDashboardPhaseProgressV1 {
  phase: DeepMapDashboardPhaseV1;
  state: DeepMapDashboardPhaseStateV1;
}

export interface DeepMapCurrentActivityV1 {
  phase: DeepMapDashboardPhaseV1;
  action: DeepMapSafeActionV2 | null;
  targetKind: DeepMapTargetKindV2 | null;
  moduleName: string | null;
  targetLabel: string | null;
  selectionReason: DeepMapSelectionReasonV1 | null;
  cardFields: DeepMapCardFieldV1[];
}

export interface DeepMapDashboardFailureV1 {
  cause: DeepMapFailureV3;
  confirmedWorkRetained: boolean;
  diagnosticCode: string | null;
}

export interface DeepMapRunDashboardResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  runSelection: string;
  state: DeepMapDashboardStateV1;
  freshness: 'current' | 'historical';
  phases: DeepMapDashboardPhaseProgressV1[];
  confirmedSteps: string;
  totalSteps: string;
  startedAtUnixMillis: string;
  updatedAtUnixMillis: string;
  currentActivity: DeepMapCurrentActivityV1 | null;
  failure: DeepMapDashboardFailureV1 | null;
  detailsIncomplete: boolean;
  historicalPlanLimited: boolean;
}

export interface DeepMapRunModuleV1 {
  selection: string;
  displayName: string;
  state: DeepMapModuleStateV1;
  plannedSteps: string;
  confirmedSteps: string;
  cardAvailable: boolean;
}

export interface DeepMapRunModulesResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  modules: DeepMapRunModuleV1[];
  nextCursor: string | null;
}

export interface DeepMapModuleStepV1 {
  position: string;
  targetKind: DeepMapTargetKindV2;
  targetLabel: string | null;
  selectionReason: DeepMapSelectionReasonV1;
  cardFields: DeepMapCardFieldV1[] | null;
  state: DeepMapPlanStepStateV1;
}

export interface DeepMapModuleStepsResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  steps: DeepMapModuleStepV1[];
  nextCursor: string | null;
  historicalDetailsLimited: boolean;
}

export interface DeepMapAtlasImpactItemV1 {
  kind: 'file' | 'symbol' | 'relation';
  label: string;
  confirmedClaimCount: string;
}

export interface DeepMapAtlasImpactSummaryV1 {
  purpose: string | null;
  riskCount: string;
  fileCount: string;
  symbolCount: string;
  relationCount: string;
}

export type DeepMapAtlasImpactResultV1 =
  | { status: 'historical' }
  | { status: 'cardUnavailable' }
  | {
      status: 'available';
      summary: DeepMapAtlasImpactSummaryV1;
      items: DeepMapAtlasImpactItemV1[];
      nextCursor: string | null;
    };

export interface DeepMapAtlasImpactResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: DeepMapAtlasImpactResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  invoke<unknown>(command, arguments_);

export async function queryDeepMapRunDashboard(
  runSelection: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapRunDashboardResponseV1> {
  assertToken(runSelection, RUN_SELECTION, 'run selection');
  return parseDashboard(
    await invokeCommand('query_deep_map_run_dashboard', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, runSelection },
    }),
    runSelection,
  );
}

export async function queryDeepMapRunModules(
  runSelection: string,
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapRunModulesResponseV1> {
  assertToken(runSelection, RUN_SELECTION, 'run selection');
  if (cursor !== null) assertToken(cursor, MODULE_SELECTION, 'module cursor');
  return parseModules(
    await invokeCommand('query_deep_map_run_modules', {
      request: { cursor, protocolVersion: CURRENT_PROTOCOL_VERSION, runSelection },
    }),
  );
}

export async function queryDeepMapModuleSteps(
  runSelection: string,
  moduleSelection: string,
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapModuleStepsResponseV1> {
  assertToken(runSelection, RUN_SELECTION, 'run selection');
  assertToken(moduleSelection, MODULE_SELECTION, 'module selection');
  if (cursor !== null) assertToken(cursor, NUMBER_CURSOR, 'step cursor');
  return parseSteps(
    await invokeCommand('query_deep_map_module_steps', {
      request: {
        cursor,
        moduleSelection,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        runSelection,
      },
    }),
  );
}

export async function queryDeepMapAtlasImpact(
  runSelection: string,
  moduleSelection: string,
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapAtlasImpactResponseV1> {
  assertToken(runSelection, RUN_SELECTION, 'run selection');
  assertToken(moduleSelection, MODULE_SELECTION, 'module selection');
  if (cursor !== null) assertToken(cursor, NUMBER_CURSOR, 'impact cursor');
  return parseImpact(
    await invokeCommand('query_deep_map_atlas_impact', {
      request: {
        cursor,
        moduleSelection,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        runSelection,
      },
    }),
  );
}

export function parseDashboard(
  payload: unknown,
  expectedRunSelection?: string,
): DeepMapRunDashboardResponseV1 {
  const value = exact(payload, [
    'protocolVersion',
    'runSelection',
    'state',
    'freshness',
    'phases',
    'confirmedSteps',
    'totalSteps',
    'startedAtUnixMillis',
    'updatedAtUnixMillis',
    'currentActivity',
    'failure',
    'detailsIncomplete',
    'historicalPlanLimited',
  ]);
  if (
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !isOneOf(value.state, DASHBOARD_STATES) ||
    !isOneOf(value.freshness, ['current', 'historical'] as const) ||
    !isU64(value.confirmedSteps) ||
    !isU64(value.totalSteps) ||
    !isU64(value.startedAtUnixMillis) ||
    !isU64(value.updatedAtUnixMillis) ||
    typeof value.detailsIncomplete !== 'boolean' ||
    typeof value.historicalPlanLimited !== 'boolean' ||
    typeof value.runSelection !== 'string' ||
    !RUN_SELECTION.test(value.runSelection) ||
    (expectedRunSelection !== undefined && value.runSelection !== expectedRunSelection) ||
    !Array.isArray(value.phases) ||
    value.phases.length !== PHASES.length
  ) {
    throw new Error('Deep Map dashboard response does not match V1.');
  }
  const phases = value.phases.map((item, index) => {
    const phase = exact(item, ['phase', 'state']);
    if (phase.phase !== PHASES[index] || !isOneOf(phase.state, PHASE_STATES)) {
      throw new Error('Deep Map dashboard phases are invalid.');
    }
    return phase as unknown as DeepMapDashboardPhaseProgressV1;
  });
  const currentActivity =
    value.currentActivity === null ? null : parseCurrentActivity(value.currentActivity);
  const failure = value.failure === null ? null : parseFailure(value.failure);
  if ((value.state === 'failed') !== (failure !== null)) {
    throw new Error('Deep Map dashboard failure contradicts its state.');
  }
  return {
    ...(value as unknown as DeepMapRunDashboardResponseV1),
    phases,
    currentActivity,
    failure,
  };
}

export function parseModules(payload: unknown): DeepMapRunModulesResponseV1 {
  const value = exact(payload, ['protocolVersion', 'modules', 'nextCursor']);
  if (
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !Array.isArray(value.modules) ||
    value.modules.length > 20 ||
    !isNullableToken(value.nextCursor, MODULE_SELECTION)
  ) {
    throw new Error('Deep Map modules response does not match V1.');
  }
  const modules = value.modules.map((item) => {
    const module = exact(item, [
      'selection',
      'displayName',
      'state',
      'plannedSteps',
      'confirmedSteps',
      'cardAvailable',
    ]);
    if (
      typeof module.selection !== 'string' ||
      !MODULE_SELECTION.test(module.selection) ||
      !isText(module.displayName) ||
      !isOneOf(module.state, MODULE_STATES) ||
      !isU64(module.plannedSteps) ||
      !isU64(module.confirmedSteps) ||
      typeof module.cardAvailable !== 'boolean'
    ) {
      throw new Error('Deep Map module summary is invalid.');
    }
    return module as unknown as DeepMapRunModuleV1;
  });
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    modules,
    nextCursor: value.nextCursor as string | null,
  };
}

export function parseSteps(payload: unknown): DeepMapModuleStepsResponseV1 {
  const value = exact(payload, [
    'protocolVersion',
    'steps',
    'nextCursor',
    'historicalDetailsLimited',
  ]);
  if (
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !Array.isArray(value.steps) ||
    value.steps.length > 50 ||
    !isNullableToken(value.nextCursor, NUMBER_CURSOR) ||
    typeof value.historicalDetailsLimited !== 'boolean'
  ) {
    throw new Error('Deep Map steps response does not match V1.');
  }
  const steps = value.steps.map((item) => {
    const step = exact(item, [
      'position',
      'targetKind',
      'targetLabel',
      'selectionReason',
      'cardFields',
      'state',
    ]);
    if (
      !isU64(step.position) ||
      step.position === '0' ||
      !isOneOf(step.targetKind, TARGETS) ||
      !isNullableText(step.targetLabel) ||
      !isOneOf(step.selectionReason, REASONS) ||
      !isOneOf(step.state, STEP_STATES)
    ) {
      throw new Error('Deep Map plan step is invalid.');
    }
    const cardFields = parseFields(step.cardFields, true);
    return { ...(step as unknown as DeepMapModuleStepV1), cardFields };
  });
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    steps,
    nextCursor: value.nextCursor as string | null,
    historicalDetailsLimited: value.historicalDetailsLimited,
  };
}

export function parseImpact(payload: unknown): DeepMapAtlasImpactResponseV1 {
  const value = exact(payload, ['protocolVersion', 'result']);
  if (value.protocolVersion !== CURRENT_PROTOCOL_VERSION) {
    throw new Error('Deep Map Atlas impact response has an invalid version.');
  }
  const result = exactWithStatus(value.result);
  if (
    (result.status === 'historical' || result.status === 'cardUnavailable') &&
    Object.keys(result).length === 1
  ) {
    return {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: result.status },
    };
  }
  if (result.status !== 'available') {
    throw new Error('Deep Map Atlas impact result is invalid.');
  }
  const available = exact(result, ['status', 'summary', 'items', 'nextCursor']);
  const summary = exact(available.summary, [
    'purpose',
    'riskCount',
    'fileCount',
    'symbolCount',
    'relationCount',
  ]);
  if (
    !isNullableText(summary.purpose) ||
    !isU64(summary.riskCount) ||
    !isU64(summary.fileCount) ||
    !isU64(summary.symbolCount) ||
    !isU64(summary.relationCount) ||
    !Array.isArray(available.items) ||
    available.items.length > 50 ||
    !isNullableToken(available.nextCursor, NUMBER_CURSOR)
  ) {
    throw new Error('Deep Map Atlas impact data is invalid.');
  }
  const items = available.items.map((item) => {
    const impact = exact(item, ['kind', 'label', 'confirmedClaimCount']);
    if (
      !isOneOf(impact.kind, ['file', 'symbol', 'relation'] as const) ||
      !isText(impact.label) ||
      !isU64(impact.confirmedClaimCount)
    ) {
      throw new Error('Deep Map Atlas impact item is invalid.');
    }
    return impact as unknown as DeepMapAtlasImpactItemV1;
  });
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      status: 'available',
      summary: summary as unknown as DeepMapAtlasImpactSummaryV1,
      items,
      nextCursor: available.nextCursor as string | null,
    },
  };
}

function parseCurrentActivity(value: unknown): DeepMapCurrentActivityV1 {
  const activity = exact(value, [
    'phase',
    'action',
    'targetKind',
    'moduleName',
    'targetLabel',
    'selectionReason',
    'cardFields',
  ]);
  if (
    !isOneOf(activity.phase, PHASES) ||
    !(activity.action === null || isOneOf(activity.action, ACTIONS)) ||
    !(activity.targetKind === null || isOneOf(activity.targetKind, TARGETS)) ||
    !isNullableText(activity.moduleName) ||
    !isNullableText(activity.targetLabel) ||
    !(activity.selectionReason === null || isOneOf(activity.selectionReason, REASONS))
  ) {
    throw new Error('Deep Map current activity is invalid.');
  }
  return {
    ...(activity as unknown as DeepMapCurrentActivityV1),
    cardFields: parseFields(activity.cardFields, false) ?? [],
  };
}

function parseFailure(value: unknown): DeepMapDashboardFailureV1 {
  const failure = exact(value, ['cause', 'confirmedWorkRetained', 'diagnosticCode']);
  if (
    !isOneOf(failure.cause, FAILURES) ||
    typeof failure.confirmedWorkRetained !== 'boolean' ||
    !(
      failure.diagnosticCode === null ||
      (typeof failure.diagnosticCode === 'string' &&
        /^DM-[A-Z-]{2,32}$/.test(failure.diagnosticCode))
    )
  ) {
    throw new Error('Deep Map dashboard failure is invalid.');
  }
  return failure as unknown as DeepMapDashboardFailureV1;
}

function parseFields(value: unknown, nullable: boolean): DeepMapCardFieldV1[] | null {
  if (nullable && value === null) return null;
  if (!Array.isArray(value) || value.length > FIELDS.length) {
    throw new Error('Deep Map Card fields are invalid.');
  }
  const fields = value as unknown[];
  if (
    fields.some((field) => !isOneOf(field, FIELDS)) ||
    fields.some(
      (field, index) =>
        index > 0 &&
        FIELDS.indexOf(field as DeepMapCardFieldV1) <=
          FIELDS.indexOf(fields[index - 1] as DeepMapCardFieldV1),
    )
  ) {
    throw new Error('Deep Map Card fields are duplicated or unordered.');
  }
  return fields as DeepMapCardFieldV1[];
}

function exact(value: unknown, keys: string[]): Record<string, unknown> {
  if (
    value === null ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).length !== keys.length ||
    keys.some((key) => !Object.prototype.hasOwnProperty.call(value, key))
  ) {
    throw new Error('Deep Map dashboard object has unexpected fields.');
  }
  return value as Record<string, unknown>;
}

function exactWithStatus(value: unknown): Record<string, unknown> & { status: unknown } {
  if (
    value === null ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    !Object.prototype.hasOwnProperty.call(value, 'status')
  ) {
    throw new Error('Deep Map dashboard result has no status.');
  }
  return value as Record<string, unknown> & { status: unknown };
}

function isOneOf<const T extends readonly string[]>(value: unknown, values: T): value is T[number] {
  return typeof value === 'string' && values.includes(value as T[number]);
}

function isU64(value: unknown): value is string {
  return typeof value === 'string' && U64.test(value);
}

function isText(value: unknown): value is string {
  return (
    typeof value === 'string' && value.length > 0 && value.length <= 4_096 && !value.includes('\0')
  );
}

function isNullableText(value: unknown): value is string | null {
  return value === null || isText(value);
}

function isNullableToken(value: unknown, pattern: RegExp): value is string | null {
  return value === null || (typeof value === 'string' && pattern.test(value));
}

function assertToken(value: string, pattern: RegExp, label: string): void {
  if (!pattern.test(value)) throw new Error(`Invalid Deep Map ${label}.`);
}

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import {
  parseProjectMapSourcePreviewV1,
  type ProjectMapSourcePreviewV1,
} from './project-map-source-preview';

const STABLE_ID = /^[0-9a-f]{64}$/u;
const DECIMAL = /^(?:0|[1-9][0-9]{0,18})$/u;
const PHASES = [
  'preparing',
  'locating',
  'deciding',
  'reading',
  'evaluating',
  'answeringOrPlanning',
  'completed',
] as const;
const STATES = ['running', 'completed', 'awaitingContinuation', 'failed', 'cancelled'] as const;
const MODES = ['ask', 'plan', 'agent'] as const;
const DEPTHS = ['standard', 'thorough'] as const;
const FINDING_KINDS = ['observation', 'hypothesis', 'conclusion'] as const;
const COMPLETENESS = ['complete', 'limited', 'notApplicable'] as const;
const SOURCE_KINDS = ['file', 'symbol', 'relationship', 'verifiedClaim'] as const;
const REASONS = [
  'exactNameOrPath',
  'indexedText',
  'relationship',
  'test',
  'verifiedModuleKnowledge',
  'semanticCandidate',
  'sourceText',
] as const;

export type AgentAskResearchPhaseV1 = (typeof PHASES)[number];
export type AgentAskResearchStateV1 = (typeof STATES)[number];
export type AgentAskResearchCompletenessV1 = (typeof COMPLETENESS)[number];
export type AgentAskResearchSourceKindV1 = (typeof SOURCE_KINDS)[number];
export type AgentAskResearchSelectionReasonV1 = (typeof REASONS)[number];
export type AgentWorkTraceModeV1 = (typeof MODES)[number];
export type AgentWorkTraceDepthV1 = (typeof DEPTHS)[number];
export type AgentWorkTraceFindingKindV1 = (typeof FINDING_KINDS)[number];

export interface AgentWorkTraceNoteV1 {
  finding: string;
  findingKind: AgentWorkTraceFindingKindV1;
  gap: string;
  goal: string;
  nextStep: string;
  sourceRefs: string[];
}

export interface AgentAskResearchStepV1 {
  action: string;
  completeness: AgentAskResearchCompletenessV1;
  occurredAtUnixMillis: string;
  phase: AgentAskResearchPhaseV1;
  query: string | null;
  state: AgentAskResearchStateV1;
  note: AgentWorkTraceNoteV1 | null;
}
export interface AgentAskResearchTurnV1 {
  action: string;
  citedSourceCount: number;
  phase: AgentAskResearchPhaseV1;
  sourceCount: number;
  stale: boolean;
  startedAtUnixMillis: string;
  state: AgentAskResearchStateV1;
  userSequence: string;
  mode: AgentWorkTraceModeV1;
  depth: AgentWorkTraceDepthV1;
  legacy: boolean;
}
export interface AgentAskResearchDetailV1 {
  citedSourceCount: number;
  sourceCount: number;
  stale: boolean;
  steps: AgentAskResearchStepV1[];
  userSequence: string;
  mode: AgentWorkTraceModeV1;
  depth: AgentWorkTraceDepthV1;
  legacy: boolean;
}
export interface AgentAskResearchSourceV1 {
  endLine: number | null;
  kind: AgentAskResearchSourceKindV1;
  path: string;
  reason: AgentAskResearchSelectionReasonV1;
  sourceRef: string;
  startLine: number | null;
  symbol: string | null;
  usedForAnswer: boolean;
}
export interface AgentWorkTraceSourceV2 extends AgentAskResearchSourceV1 {
  referenceLabel: string;
}
export interface AgentWorkTracePresentationV1 {
  additionalSourcesExpanded?: boolean;
  autoCollapseDone?: boolean;
  disclosureOverride?: boolean;
  detail: AgentAskResearchDetailV1;
  expanded: boolean;
  loadState: 'available';
  preview: ProjectMapSourcePreviewV1 | null;
  previewState: 'idle' | 'loading' | 'stale' | 'error';
  selectedSource: string | null;
  sourceLoadState: 'loading' | 'available' | 'updating' | 'error';
  sources: AgentWorkTraceSourceV2[];
  visibleStepCount: number;
}

export type AgentAskResearchTurnsResponseV1 = {
  protocolVersion: 1;
  result:
    { status: 'noProject' | 'notFound' } | { status: 'available'; turns: AgentAskResearchTurnV1[] };
};
export type AgentAskResearchDetailResponseV1 = {
  protocolVersion: 1;
  result:
    | { status: 'noProject' | 'notFound' | 'notRecorded' }
    | { status: 'available'; detail: AgentAskResearchDetailV1 };
};
export type AgentAskResearchSourcesResponseV1 = {
  protocolVersion: 1;
  result:
    | { status: 'noProject' | 'notFound' }
    | { status: 'available'; sources: AgentAskResearchSourceV1[]; nextCursor: string | null };
};
export type AgentAskResearchSourcePreviewResponseV1 = {
  protocolVersion: 1;
  result:
    | { status: 'noProject' | 'notFound' | 'stale' }
    | { status: 'available'; preview: ProjectMapSourcePreviewV1 };
};
export type AgentWorkTraceProjectionResponseV1 = {
  protocolVersion: 1;
  result:
    | { status: 'noProject' | 'notFound' | 'notRecorded' | 'updating' }
    | {
        status: 'available';
        detail: AgentAskResearchDetailV1;
        projectionRef: string;
        sources: AgentWorkTraceSourceV2[];
        nextCursor: string | null;
      };
};
export type AgentWorkTraceSourcesResponseV2 = {
  protocolVersion: 1;
  result:
    | { status: 'noProject' | 'notFound' | 'projectionChanged' }
    | { status: 'available'; sources: AgentWorkTraceSourceV2[]; nextCursor: string | null };
};

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentAskResearchTurns(
  sessionId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentAskResearchTurnsResponseV1> {
  stable(sessionId);
  return parseTurns(
    await invokeCommand('query_agent_work_trace_turns', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId },
    }),
  );
}

export async function queryAgentAskResearchDetail(
  sessionId: string,
  userSequence: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentAskResearchDetailResponseV1> {
  stable(sessionId);
  decimal(userSequence, false);
  return parseDetail(
    await invokeCommand('query_agent_work_trace_detail_v2', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId, userSequence },
    }),
  );
}

export async function queryAgentAskResearchSources(
  sessionId: string,
  userSequence: string,
  cursor: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentAskResearchSourcesResponseV1> {
  stable(sessionId);
  decimal(userSequence, false);
  if (cursor !== null) stable(cursor);
  return parseSources(
    await invokeCommand('query_agent_work_trace_sources', {
      request: { cursor, protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId, userSequence },
    }),
  );
}

export async function queryAgentAskResearchSourcePreview(
  sessionId: string,
  userSequence: string,
  sourceRef: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentAskResearchSourcePreviewResponseV1> {
  stable(sessionId);
  decimal(userSequence, false);
  stable(sourceRef);
  return parsePreview(
    await invokeCommand('query_agent_work_trace_source_preview', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId, sourceRef, userSequence },
    }),
  );
}

export async function queryAgentWorkTraceProjection(
  sessionId: string,
  userSequence: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentWorkTraceProjectionResponseV1> {
  stable(sessionId);
  decimal(userSequence, false);
  return parseProjection(
    await invokeCommand('query_agent_work_trace_projection', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId, userSequence },
    }),
  );
}

export async function queryAgentWorkTraceSourcesV2(
  sessionId: string,
  userSequence: string,
  projectionRef: string,
  cursor: string | null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentWorkTraceSourcesResponseV2> {
  stable(sessionId);
  decimal(userSequence, false);
  stable(projectionRef);
  if (cursor !== null) stable(cursor);
  return parseSourcesV2(
    await invokeCommand('query_agent_work_trace_sources_v2', {
      request: {
        cursor,
        projectionRef,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        sessionId,
        userSequence,
      },
    }),
  );
}

function parseTurns(payload: unknown): AgentAskResearchTurnsResponseV1 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (status === 'noProject' || status === 'notFound') {
    exact(result, ['status']);
    return root as AgentAskResearchTurnsResponseV1;
  }
  exact(result, ['status', 'turns']);
  if (status !== 'available' || !Array.isArray(result.turns) || result.turns.length > 32) invalid();
  return { protocolVersion: 1, result: { status: 'available', turns: result.turns.map(turn) } };
}

function parseDetail(payload: unknown): AgentAskResearchDetailResponseV1 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (status === 'noProject' || status === 'notFound' || status === 'notRecorded') {
    exact(result, ['status']);
    return root as AgentAskResearchDetailResponseV1;
  }
  exact(result, ['detail', 'status']);
  if (status !== 'available') invalid();
  const value = record(result.detail);
  exact(value, [
    'citedSourceCount',
    'depth',
    'legacy',
    'mode',
    'sourceCount',
    'stale',
    'steps',
    'userSequence',
  ]);
  decimal(value.userSequence, false);
  if (
    !count(value.sourceCount, 200) ||
    !count(value.citedSourceCount, 200) ||
    typeof value.stale !== 'boolean' ||
    typeof value.legacy !== 'boolean' ||
    !MODES.includes(value.mode as never) ||
    !DEPTHS.includes(value.depth as never) ||
    !Array.isArray(value.steps) ||
    value.steps.length > 64
  )
    invalid();
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      detail: {
        citedSourceCount: value.citedSourceCount,
        sourceCount: value.sourceCount,
        stale: value.stale,
        legacy: value.legacy,
        mode: value.mode,
        depth: value.depth,
        steps: value.steps.map(step),
        userSequence: value.userSequence,
      } as AgentAskResearchDetailV1,
    },
  };
}

function parseSources(payload: unknown): AgentAskResearchSourcesResponseV1 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (status === 'noProject' || status === 'notFound') {
    exact(result, ['status']);
    return root as AgentAskResearchSourcesResponseV1;
  }
  exact(result, ['nextCursor', 'sources', 'status']);
  if (
    status !== 'available' ||
    !Array.isArray(result.sources) ||
    result.sources.length > 50 ||
    (result.nextCursor !== null &&
      (typeof result.nextCursor !== 'string' || !STABLE_ID.test(result.nextCursor)))
  )
    invalid();
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      nextCursor: result.nextCursor as string | null,
      sources: result.sources.map(source),
    },
  };
}

function parsePreview(payload: unknown): AgentAskResearchSourcePreviewResponseV1 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (status === 'noProject' || status === 'notFound' || status === 'stale') {
    exact(result, ['status']);
    return root as AgentAskResearchSourcePreviewResponseV1;
  }
  exact(result, ['preview', 'status']);
  if (status !== 'available') invalid();
  return {
    protocolVersion: 1,
    result: { status: 'available', preview: parseProjectMapSourcePreviewV1(result.preview) },
  };
}

function parseProjection(payload: unknown): AgentWorkTraceProjectionResponseV1 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (
    status === 'noProject' ||
    status === 'notFound' ||
    status === 'notRecorded' ||
    status === 'updating'
  ) {
    exact(result, ['status']);
    return root as AgentWorkTraceProjectionResponseV1;
  }
  exact(result, ['detail', 'nextCursor', 'projectionRef', 'sources', 'status']);
  if (
    status !== 'available' ||
    !Array.isArray(result.sources) ||
    result.sources.length > 50 ||
    (result.nextCursor !== null &&
      (typeof result.nextCursor !== 'string' || !STABLE_ID.test(result.nextCursor)))
  )
    invalid();
  stable(result.projectionRef);
  const detailResponse = parseDetail({
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { detail: result.detail, status: 'available' },
  });
  if (detailResponse.result.status !== 'available') invalid();
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      detail: detailResponse.result.detail,
      projectionRef: result.projectionRef,
      sources: result.sources.map(sourceV2),
      nextCursor: result.nextCursor as string | null,
    },
  };
}

function parseSourcesV2(payload: unknown): AgentWorkTraceSourcesResponseV2 {
  const root = response(payload);
  const result = record(root.result);
  const status = result.status;
  if (status === 'noProject' || status === 'notFound' || status === 'projectionChanged') {
    exact(result, ['status']);
    return root as AgentWorkTraceSourcesResponseV2;
  }
  exact(result, ['nextCursor', 'sources', 'status']);
  if (
    status !== 'available' ||
    !Array.isArray(result.sources) ||
    result.sources.length > 50 ||
    (result.nextCursor !== null &&
      (typeof result.nextCursor !== 'string' || !STABLE_ID.test(result.nextCursor)))
  )
    invalid();
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      nextCursor: result.nextCursor as string | null,
      sources: result.sources.map(sourceV2),
    },
  };
}

function turn(payload: unknown): AgentAskResearchTurnV1 {
  const value = record(payload);
  exact(value, [
    'action',
    'citedSourceCount',
    'phase',
    'sourceCount',
    'stale',
    'startedAtUnixMillis',
    'state',
    'userSequence',
    'mode',
    'depth',
    'legacy',
  ]);
  text(value.action, 512);
  decimal(value.startedAtUnixMillis, true);
  decimal(value.userSequence, false);
  if (
    !PHASES.includes(value.phase as never) ||
    !STATES.includes(value.state as never) ||
    !count(value.sourceCount, 200) ||
    !count(value.citedSourceCount, 200) ||
    typeof value.stale !== 'boolean' ||
    typeof value.legacy !== 'boolean' ||
    !MODES.includes(value.mode as never) ||
    !DEPTHS.includes(value.depth as never)
  )
    invalid();
  return value as unknown as AgentAskResearchTurnV1;
}
function step(payload: unknown): AgentAskResearchStepV1 {
  const value = record(payload);
  exact(value, [
    'action',
    'completeness',
    'note',
    'occurredAtUnixMillis',
    'phase',
    'query',
    'state',
  ]);
  text(value.action, 512);
  if (value.query !== null) text(value.query, 4096);
  decimal(value.occurredAtUnixMillis, true);
  if (
    !PHASES.includes(value.phase as never) ||
    !STATES.includes(value.state as never) ||
    !COMPLETENESS.includes(value.completeness as never)
  )
    invalid();
  return {
    ...(value as unknown as Omit<AgentAskResearchStepV1, 'note'>),
    note: value.note === null ? null : note(value.note),
  };
}

function note(payload: unknown): AgentWorkTraceNoteV1 {
  const value = record(payload);
  exact(value, ['finding', 'findingKind', 'gap', 'goal', 'nextStep', 'sourceRefs']);
  text(value.goal, 1024);
  text(value.finding, 4096);
  text(value.gap, 1024);
  text(value.nextStep, 1024);
  if (
    !FINDING_KINDS.includes(value.findingKind as never) ||
    !Array.isArray(value.sourceRefs) ||
    value.sourceRefs.length > 32
  )
    invalid();
  for (const sourceRef of value.sourceRefs) stable(sourceRef);
  return value as unknown as AgentWorkTraceNoteV1;
}
function source(payload: unknown): AgentAskResearchSourceV1 {
  const value = record(payload);
  exact(value, [
    'endLine',
    'kind',
    'path',
    'reason',
    'sourceRef',
    'startLine',
    'symbol',
    'usedForAnswer',
  ]);
  stable(value.sourceRef);
  text(value.path, 4096);
  if (value.symbol !== null) text(value.symbol, 512);
  if (
    !SOURCE_KINDS.includes(value.kind as never) ||
    !REASONS.includes(value.reason as never) ||
    typeof value.usedForAnswer !== 'boolean' ||
    !nullableLine(value.startLine) ||
    !nullableLine(value.endLine) ||
    (value.startLine === null) !== (value.endLine === null)
  )
    invalid();
  return value as unknown as AgentAskResearchSourceV1;
}
function sourceV2(payload: unknown): AgentWorkTraceSourceV2 {
  const value = record(payload);
  exact(value, [
    'endLine',
    'kind',
    'path',
    'reason',
    'referenceLabel',
    'sourceRef',
    'startLine',
    'symbol',
    'usedForAnswer',
  ]);
  const legacy = source({
    endLine: value.endLine,
    kind: value.kind,
    path: value.path,
    reason: value.reason,
    sourceRef: value.sourceRef,
    startLine: value.startLine,
    symbol: value.symbol,
    usedForAnswer: value.usedForAnswer,
  });
  if (
    typeof value.referenceLabel !== 'string' ||
    !/^S(?:[1-9]|[1-9][0-9]|1[0-9][0-9]|200)$/u.test(value.referenceLabel)
  )
    invalid();
  return { ...legacy, referenceLabel: value.referenceLabel };
}
function response(payload: unknown): Record<string, unknown> {
  const value = record(payload);
  exact(value, ['protocolVersion', 'result']);
  if (value.protocolVersion !== CURRENT_PROTOCOL_VERSION) invalid();
  return value;
}
function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) invalid();
  return value as Record<string, unknown>;
}
function exact(value: Record<string, unknown>, keys: string[]): void {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index]))
    invalid();
}
function stable(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !STABLE_ID.test(value)) invalid();
}
function decimal(value: unknown, zero: boolean): asserts value is string {
  if (typeof value !== 'string' || !DECIMAL.test(value) || (!zero && value === '0')) invalid();
}
function text(value: unknown, max: number): asserts value is string {
  if (
    typeof value !== 'string' ||
    value.trim().length === 0 ||
    new TextEncoder().encode(value).length > max
  )
    invalid();
}
function count(value: unknown, max: number): value is number {
  return Number.isInteger(value) && Number(value) >= 0 && Number(value) <= max;
}
function nullableLine(value: unknown): boolean {
  return (
    value === null ||
    (Number.isInteger(value) && Number(value) >= 1 && Number(value) <= 4_294_967_295)
  );
}
function invalid(): never {
  throw new Error('Ask research response does not match V1.');
}

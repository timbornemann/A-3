import { invoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import {
  parseProjectMapSourcePreviewResponseV1,
  type ProjectMapSourcePreviewV1,
} from './project-map-source-preview';
import {
  parseIndexEvidence,
  type ProjectMapEntitySelectionV1,
  type ProjectMapIndexEvidenceSelectionV1,
} from './project-map-atlas';

export const FLOW_STEP_KINDS = [
  'call',
  'process',
  'assign',
  'condition',
  'branch',
  'loop',
  'return',
  'throw',
  'break',
  'continue',
  'await',
  'handler',
  'deferred',
  'unknown',
] as const;
export const FLOW_VALUE_KINDS = [
  'parameter',
  'local',
  'external',
  'callResult',
  'merge',
  'scriptArgument',
] as const;
export type FlowStepKind = (typeof FLOW_STEP_KINDS)[number];
export type FlowValueKind = (typeof FLOW_VALUE_KINDS)[number];
export type FlowDirection = 'origins' | 'uses';
export interface FlowSelection {
  runId: string;
  root: string;
  callPath: number[];
}
export interface FlowSource {
  path: string;
  line: number;
  preview: ProjectMapIndexEvidenceSelectionV1 | null;
  mapSelection: ProjectMapEntitySelectionV1 | null;
}
export interface FlowEntry {
  selection: FlowSelection;
  name: string;
  category: 'function' | 'test' | 'entrypoint' | 'script';
  source: FlowSource;
}
export interface FlowStep {
  processMode: 'wait' | 'spawn' | 'compileOnly' | null;
  valuesTruncated: boolean;
  id: number;
  parent: number | null;
  kind: FlowStepKind;
  name: string | null;
  line: number;
  target: FlowSelection | null;
  inputs: number[];
  outputs: number[];
}
export interface FlowValue {
  id: number;
  name: string;
  kind: FlowValueKind;
  line: number;
}
export interface FlowGap {
  kind: 'unsupported' | 'dynamic' | 'limit' | 'parseError';
  line: number;
}
export interface FlowView {
  selection: FlowSelection;
  name: string;
  source: FlowSource;
  breadcrumbs: FlowEntry[];
  steps: FlowStep[];
  values: FlowValue[];
  stepTotal: number;
  valueTotal: number;
  gaps: FlowGap[];
  gapsTruncated: boolean;
}
export interface FlowTrace {
  direction: FlowDirection;
  nodes: {
    selection: FlowSelection;
    value: FlowValue;
    functionName: string;
    path: string;
    unknown: boolean;
  }[];
  truncated: boolean;
}
export type FlowQuery =
  | { kind: 'source'; selection: FlowSelection; step: number }
  | { kind: 'catalog'; term: string; offset: number }
  | { kind: 'inspect'; selection: FlowSelection; stepOffset: number; valueOffset: number }
  | { kind: 'trace'; selection: FlowSelection; value: number; direction: FlowDirection };
export type FlowResult =
  | { status: 'source'; preview: ProjectMapSourcePreviewV1 }
  | { status: 'noProject' | 'noPublishedIndex' | 'selectionChanged' }
  | { status: 'catalog'; page: { entries: FlowEntry[]; hasMore: boolean } }
  | { status: 'flow'; flow: FlowView }
  | { status: 'trace'; trace: FlowTrace };
export interface FlowResponse {
  protocolVersion: 1;
  result: FlowResult;
}
export async function queryFunctionFlows(
  query: FlowQuery,
  command: InvokeCommand = (name, args) => invoke<unknown>(name, args),
): Promise<FlowResponse> {
  validateQuery(query);
  return parseFlowResponse(
    await command('query_function_flows', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, query },
    }),
  );
}
export function parseFlowResponse(payload: unknown): FlowResponse {
  const response = record(payload, ['protocolVersion', 'result']);
  if (response.protocolVersion !== 1) invalid();
  const result = record(response.result);
  switch (result.status) {
    case 'source': {
      record(result, ['status', 'preview']);
      const parsed = parseProjectMapSourcePreviewResponseV1({
        protocolVersion: 1,
        result: { status: 'available', preview: result.preview },
      });
      if (parsed.result.status !== 'available') return invalid();
      return { protocolVersion: 1, result: { status: 'source', preview: parsed.result.preview } };
    }
    case 'noProject':
    case 'noPublishedIndex':
    case 'selectionChanged':
      record(result, ['status']);
      return { protocolVersion: 1, result: { status: result.status } };
    case 'catalog': {
      record(result, ['status', 'page']);
      const p = record(result.page, ['entries', 'hasMore']);
      const entries = list(p.entries, entry);
      if (
        new Set(entries.map((e) => e.selection.root)).size !== entries.length ||
        entries.some(
          (e) =>
            e.selection.callPath.length !== 0 || e.selection.runId !== entries[0].selection.runId,
        )
      )
        invalid();
      return {
        protocolVersion: 1,
        result: {
          status: 'catalog',
          page: { entries, hasMore: bool(p.hasMore) },
        },
      };
    }
    case 'flow':
      record(result, ['status', 'flow']);
      return { protocolVersion: 1, result: { status: 'flow', flow: view(result.flow) } };
    case 'trace': {
      record(result, ['status', 'trace']);
      const t = record(result.trace, ['direction', 'nodes', 'truncated']);
      const nodes = list(t.nodes, (raw) => {
        const n = record(raw, ['selection', 'value', 'functionName', 'path', 'unknown']);
        return {
          selection: selection(n.selection),
          value: value(n.value),
          functionName: text(n.functionName, 1024),
          path: text(n.path, 131072),
          unknown: bool(n.unknown),
        };
      });
      if (
        nodes.some(
          (n) =>
            n.selection.root !== nodes[0].selection.root ||
            n.selection.runId !== nodes[0].selection.runId,
        ) ||
        new Set(nodes.map((n) => `${n.selection.callPath.join('.')}:${n.value.id}`)).size !==
          nodes.length
      )
        invalid();
      return {
        protocolVersion: 1,
        result: {
          status: 'trace',
          trace: {
            direction: choice(t.direction, ['origins', 'uses'] as const),
            nodes,
            truncated: bool(t.truncated),
          },
        },
      };
    }
    default:
      return invalid();
  }
}
function view(raw: unknown): FlowView {
  const f = record(raw, [
    'selection',
    'name',
    'source',
    'breadcrumbs',
    'steps',
    'values',
    'stepTotal',
    'valueTotal',
    'gaps',
    'gapsTruncated',
  ]);
  const selected = selection(f.selection);
  const steps = list(f.steps, (raw) => {
    const s = record(raw, [
      'id',
      'parent',
      'kind',
      'name',
      'line',
      'target',
      'inputs',
      'outputs',
      'processMode',
      'valuesTruncated',
    ]);
    const id = integer(s.id, 1, 4096);
    const target = optional(s.target, selection);
    const kind = choice(s.kind, FLOW_STEP_KINDS);
    const processMode = optional(s.processMode, (m) =>
      choice(m, ['wait', 'spawn', 'compileOnly'] as const),
    );
    if (
      (kind === 'process') !== (processMode !== null) ||
      (target && (processMode === 'compileOnly' || !['call', 'process'].includes(kind)))
    )
      invalid();
    if (
      target &&
      (target.runId !== selected.runId ||
        target.root !== selected.root ||
        target.callPath.join('.') !== [...selected.callPath, id].join('.'))
    )
      invalid();
    return {
      id,
      processMode,
      valuesTruncated: bool(s.valuesTruncated),
      parent: optional(s.parent, (p) => integer(p, 1, id - 1)),
      kind,
      name: optional(s.name, (n) => text(n, 1024)),
      line: integer(s.line, 1, 4294967295),
      target,
      inputs: list(s.inputs, (i) => integer(i, 1, 4096)),
      outputs: list(s.outputs, (i) => integer(i, 1, 4096)),
    };
  });
  const values = list(f.values, value);
  const stepTotal = integer(f.stepTotal, 0, 4096);
  const valueTotal = integer(f.valueTotal, 0, 4096);
  const breadcrumbs = list(f.breadcrumbs, entry, 8);
  if (
    steps.length > stepTotal ||
    values.length > valueTotal ||
    steps.some((s, i) => i > 0 && s.id <= steps[i - 1].id) ||
    values.some((v, i) => i > 0 && v.id <= values[i - 1].id) ||
    breadcrumbs.length !== selected.callPath.length + 1 ||
    breadcrumbs.some(
      (b, i) =>
        b.selection.runId !== selected.runId ||
        b.selection.root !== selected.root ||
        b.selection.callPath.join('.') !== selected.callPath.slice(0, i).join('.'),
    )
  )
    invalid();
  if (
    new Set(steps.map((s) => s.id)).size !== steps.length ||
    new Set(values.map((v) => v.id)).size !== values.length
  )
    invalid();
  return {
    selection: selected,
    name: text(f.name, 1024),
    source: source(f.source),
    breadcrumbs,
    steps,
    values,
    stepTotal,
    valueTotal,
    gaps: list(f.gaps, (raw) => {
      const g = record(raw, ['kind', 'line']);
      return {
        kind: choice(g.kind, ['unsupported', 'dynamic', 'limit', 'parseError'] as const),
        line: integer(g.line, 1, 4294967295),
      };
    }),
    gapsTruncated: bool(f.gapsTruncated),
  };
}
function value(raw: unknown): FlowValue {
  const v = record(raw, ['id', 'name', 'kind', 'line']);
  return {
    id: integer(v.id, 1, 4096),
    name: text(v.name, 1024),
    kind: choice(v.kind, FLOW_VALUE_KINDS),
    line: integer(v.line, 1, 4294967295),
  };
}
function entry(raw: unknown): FlowEntry {
  const e = record(raw, ['selection', 'name', 'category', 'source']);
  return {
    selection: selection(e.selection),
    name: text(e.name, 1024),
    category: choice(e.category, ['function', 'test', 'entrypoint', 'script'] as const),
    source: source(e.source),
  };
}
function source(raw: unknown): FlowSource {
  const s = record(raw, ['path', 'line', 'preview', 'mapSelection']);
  const map = optional(s.mapSelection, parseIndexEvidence);
  if (map !== null && map.kind !== 'symbol') invalid();
  return {
    path: text(s.path, 131072),
    line: integer(s.line, 1, 4294967295),
    preview: optional(s.preview, parseIndexEvidence),
    mapSelection: map,
  };
}
function selection(raw: unknown): FlowSelection {
  const s = record(raw, ['runId', 'root', 'callPath']);
  return {
    runId: stableId(s.runId),
    root: stableId(s.root),
    callPath: list(s.callPath, (i) => integer(i, 1, 4096), 7),
  };
}
function validateQuery(q: FlowQuery): void {
  if (q.kind === 'catalog') {
    text(q.term, 512, true);
    integer(q.offset, 0, 1000000);
    if (q.offset % 50 !== 0) invalid();
    return;
  }
  selection(q.selection);
  if (q.kind === 'source') {
    integer(q.step, 1, 4096);
    return;
  }
  if (q.kind === 'inspect') {
    for (const page of [q.stepOffset, q.valueOffset]) {
      integer(page, 0, 4050);
      if (page % 50 !== 0) invalid();
    }
  } else {
    integer(q.value, 1, 4096);
    choice(q.direction, ['origins', 'uses'] as const);
  }
}
function record(v: unknown, keys?: string[]): Record<string, unknown> {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return invalid();
  const r = v as Record<string, unknown>;
  if (keys && (Object.keys(r).length !== keys.length || keys.some((k) => !Object.hasOwn(r, k))))
    invalid();
  return r;
}
function list<T>(v: unknown, parse: (v: unknown) => T, max = 50): T[] {
  if (!Array.isArray(v) || v.length > max) return invalid();
  return v.map(parse);
}
function optional<T>(v: unknown, parse: (v: unknown) => T): T | null {
  return v === null ? null : parse(v);
}
function integer(v: unknown, min: number, max: number): number {
  if (typeof v !== 'number' || !Number.isInteger(v) || v < min || v > max) return invalid();
  return v;
}
function text(v: unknown, max: number, empty = false): string {
  if (
    typeof v !== 'string' ||
    (!empty && v.length === 0) ||
    new TextEncoder().encode(v).length > max ||
    Array.from(v).some((character) => character.charCodeAt(0) < 32 && !'\t\n\r'.includes(character))
  )
    return invalid();
  return v;
}
function stableId(v: unknown): string {
  if (typeof v !== 'string' || !/^[0-9a-f]{64}$/.test(v)) return invalid();
  return v;
}
function bool(v: unknown): boolean {
  if (typeof v !== 'boolean') return invalid();
  return v;
}
function choice<T extends string>(v: unknown, values: readonly T[]): T {
  if (typeof v !== 'string' || !values.includes(v as T)) return invalid();
  return v as T;
}
function invalid(): never {
  throw new Error('Die Ablaufdaten entsprechen nicht dem sicheren V1-Format.');
}

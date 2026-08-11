import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { parseModuleCardLifecycleV1, type ModuleCardLifecycleV1 } from './module-card-detail';
import {
  isRepositoryPathHex,
  parseModuleDependencyEdgeEvidenceV1,
  type ModuleDependencyEdgeEvidenceV1,
} from './module-dependency-graph';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;

export interface ModuleCardEvidenceQueryV1 {
  cardId: string;
  currentIndexRunId: string;
  currentSnapshotId: string;
  evidenceId: string;
  moduleId: string;
  sourceIndexRunId: string;
  sourceSnapshotId: string;
}

export interface QueryModuleCardEvidenceRequestV1 extends ModuleCardEvidenceQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleCardEvidenceFreshnessV1 = 'current' | 'stale';

export type ModuleCardEvidenceRelationV1 =
  | 'contains'
  | 'defines'
  | 'imports'
  | 'exports'
  | 'calls'
  | 'implements'
  | 'extends'
  | 'reads'
  | 'writes'
  | 'configures'
  | 'tests'
  | 'builds'
  | 'documents';

export interface ModuleCardEvidenceRevisionV1 {
  contentHash: string;
  pathHex: string;
}

export type ModuleCardEvidencePayloadV1 =
  | { kind: 'file'; revision: ModuleCardEvidenceRevisionV1 }
  | { kind: 'symbol'; revision: ModuleCardEvidenceRevisionV1; symbolId: string }
  | {
      edge: ModuleDependencyEdgeEvidenceV1;
      kind: 'graphEdge';
      relation: ModuleCardEvidenceRelationV1;
    };

export interface ModuleCardEvidenceV1 extends ModuleCardEvidenceQueryV1 {
  cardLifecycle: ModuleCardLifecycleV1;
  freshness: ModuleCardEvidenceFreshnessV1;
  payload: ModuleCardEvidencePayloadV1;
}

export type ModuleCardEvidenceResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'moduleUnavailable' }
  | { status: 'cardUnavailable' }
  | { status: 'selectionChanged' }
  | { status: 'evidenceUnavailable' }
  | { detail: ModuleCardEvidenceV1; status: 'available' };

export interface ModuleCardEvidenceResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleCardEvidenceResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleCardEvidence(
  query: ModuleCardEvidenceQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleCardEvidenceResponseV1> {
  if (!isQuery(query)) {
    throw new Error('Module Card Evidence query does not match the V1 schema.');
  }
  const request: QueryModuleCardEvidenceRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_card_evidence', { request });
  const response = parseModuleCardEvidenceResponseV1(payload);
  if (
    response.result.status === 'available' &&
    !detailMatchesQuery(response.result.detail, query)
  ) {
    throw new Error('Module Card Evidence response does not match the selected Card hook.');
  }
  return response;
}

export function parseModuleCardEvidenceResponseV1(payload: unknown): ModuleCardEvidenceResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Module Card Evidence response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ModuleCardEvidenceResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module Card Evidence response contains an invalid result.');
  }
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'moduleUnavailable',
    'cardUnavailable',
    'selectionChanged',
    'evidenceUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['detail', 'status'])) {
    return { detail: parseDetail(value.detail), status: 'available' };
  }
  throw new Error('Module Card Evidence response contains an invalid result.');
}

function parseDetail(value: unknown): ModuleCardEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'cardId',
      'cardLifecycle',
      'currentIndexRunId',
      'currentSnapshotId',
      'evidenceId',
      'freshness',
      'moduleId',
      'payload',
      'sourceIndexRunId',
      'sourceSnapshotId',
    ]) ||
    !isStableId(value.cardId) ||
    !isStableId(value.currentIndexRunId) ||
    !isStableId(value.currentSnapshotId) ||
    !isStableId(value.evidenceId) ||
    !isStableId(value.moduleId) ||
    !isStableId(value.sourceIndexRunId) ||
    !isStableId(value.sourceSnapshotId) ||
    (value.sourceIndexRunId === value.currentIndexRunId &&
      value.sourceSnapshotId !== value.currentSnapshotId) ||
    (value.freshness !== 'current' && value.freshness !== 'stale')
  ) {
    throw new Error('Module Card Evidence response contains an invalid envelope.');
  }
  const cardLifecycle = parseModuleCardLifecycleV1(value.cardLifecycle);
  if (value.freshness === 'stale' && cardLifecycle.status !== 'stale') {
    throw new Error('Current or NeedsReview Card cannot expose stale Evidence.');
  }
  return {
    cardId: value.cardId,
    cardLifecycle,
    currentIndexRunId: value.currentIndexRunId,
    currentSnapshotId: value.currentSnapshotId,
    evidenceId: value.evidenceId,
    freshness: value.freshness,
    moduleId: value.moduleId,
    payload: parsePayload(value.payload, value.evidenceId),
    sourceIndexRunId: value.sourceIndexRunId,
    sourceSnapshotId: value.sourceSnapshotId,
  };
}

function parsePayload(value: unknown, evidenceId: string): ModuleCardEvidencePayloadV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') {
    throw new Error('Module Card Evidence response contains an invalid payload.');
  }
  if (value.kind === 'file' && hasExactKeys(value, ['kind', 'revision'])) {
    return { kind: 'file', revision: parseRevision(value.revision) };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, ['kind', 'revision', 'symbolId']) &&
    isStableId(value.symbolId)
  ) {
    return { kind: 'symbol', revision: parseRevision(value.revision), symbolId: value.symbolId };
  }
  if (
    value.kind === 'graphEdge' &&
    hasExactKeys(value, ['edge', 'kind', 'relation']) &&
    isRelation(value.relation)
  ) {
    const edge = parseModuleDependencyEdgeEvidenceV1(value.edge);
    if (edge.evidenceId !== evidenceId) {
      throw new Error('Graph-edge payload does not match the selected Evidence identity.');
    }
    return { edge, kind: 'graphEdge', relation: value.relation };
  }
  throw new Error('Module Card Evidence response contains an invalid payload.');
}

function isRelation(value: unknown): value is ModuleCardEvidenceRelationV1 {
  return (
    typeof value === 'string' &&
    [
      'contains',
      'defines',
      'imports',
      'exports',
      'calls',
      'implements',
      'extends',
      'reads',
      'writes',
      'configures',
      'tests',
      'builds',
      'documents',
    ].includes(value)
  );
}

function parseRevision(value: unknown): ModuleCardEvidenceRevisionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['contentHash', 'pathHex']) ||
    !isStableId(value.contentHash) ||
    !isRepositoryPathHex(value.pathHex)
  ) {
    throw new Error('Module Card Evidence response contains an invalid file revision.');
  }
  return { contentHash: value.contentHash, pathHex: value.pathHex };
}

function isQuery(query: ModuleCardEvidenceQueryV1): boolean {
  return (
    hasExactKeys(query as unknown as Record<string, unknown>, [
      'cardId',
      'currentIndexRunId',
      'currentSnapshotId',
      'evidenceId',
      'moduleId',
      'sourceIndexRunId',
      'sourceSnapshotId',
    ]) &&
    isStableId(query.cardId) &&
    isStableId(query.currentIndexRunId) &&
    isStableId(query.currentSnapshotId) &&
    isStableId(query.evidenceId) &&
    isStableId(query.moduleId) &&
    isStableId(query.sourceIndexRunId) &&
    isStableId(query.sourceSnapshotId) &&
    (query.sourceIndexRunId !== query.currentIndexRunId ||
      query.sourceSnapshotId === query.currentSnapshotId)
  );
}

function detailMatchesQuery(
  detail: ModuleCardEvidenceV1,
  query: ModuleCardEvidenceQueryV1,
): boolean {
  return (
    detail.cardId === query.cardId &&
    detail.currentIndexRunId === query.currentIndexRunId &&
    detail.currentSnapshotId === query.currentSnapshotId &&
    detail.evidenceId === query.evidenceId &&
    detail.moduleId === query.moduleId &&
    detail.sourceIndexRunId === query.sourceIndexRunId &&
    detail.sourceSnapshotId === query.sourceSnapshotId
  );
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return (
    actual.length === sortedExpected.length &&
    actual.every((key, index) => key === sortedExpected[index])
  );
}

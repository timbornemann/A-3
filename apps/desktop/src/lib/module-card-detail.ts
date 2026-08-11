import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModuleCardFreshnessReasonV1 } from './module-card-freshness';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const MAX_FIELDS = 12;
const MAX_CARD_BYTES = 65_536;
const MAX_CARD_EVIDENCE = 512;
const MAX_CLAIM_EVIDENCE = 16;

export interface ModuleCardDetailQueryV1 {
  moduleId: string;
}

export interface QueryModuleCardDetailRequestV1 extends ModuleCardDetailQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleCardClaimKindV1 = 'fact' | 'observation' | 'hypothesis';
export type ModuleCardClaimStateV1 = 'current' | 'stale' | 'needsReview';
export type ModuleCardFieldKindV1 =
  | 'title'
  | 'paths'
  | 'purpose'
  | 'responsibilities'
  | 'publicSurface'
  | 'entrypoints'
  | 'dependencies'
  | 'dataFlows'
  | 'invariants'
  | 'tests'
  | 'risks'
  | 'openQuestions';

export type ModuleCardLifecycleV1 =
  | { status: 'current' }
  | {
      invalidatedByIndexRunId: string;
      reason: ModuleCardFreshnessReasonV1;
      status: 'stale';
    }
  | {
      invalidatedByIndexRunId: string;
      reason: 'directDependencyChanged';
      status: 'needsReview';
    };

export interface ModuleCardClaimV1 {
  claimId: string;
  confidenceBasisPoints: number;
  evidenceIds: string[];
  kind: ModuleCardClaimKindV1;
  state: ModuleCardClaimStateV1;
}

export interface ModuleCardValueV1 {
  claim: ModuleCardClaimV1;
  value: string;
}

export interface ModuleCardDetailFieldV1 {
  evidenceIds: string[];
  kind: ModuleCardFieldKindV1;
  values: ModuleCardValueV1[];
}

export interface ModuleCardDetailV1 {
  cardId: string;
  confidenceBasisPoints: number;
  currentIndexRunId: string;
  currentSnapshotId: string;
  fields: ModuleCardDetailFieldV1[];
  lifecycle: ModuleCardLifecycleV1;
  mapperProfileVersion: 1;
  moduleId: string;
  schemaVersion: 1;
  sourceIndexRunId: string;
  sourceSnapshotId: string;
}

export type ModuleCardDetailResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'moduleUnavailable' }
  | { status: 'cardUnavailable' }
  | { detail: ModuleCardDetailV1; status: 'available' };

export interface ModuleCardDetailResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleCardDetailResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleCardDetail(
  query: ModuleCardDetailQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleCardDetailResponseV1> {
  if (
    !isStableId(query.moduleId) ||
    !hasExactKeys(query as unknown as Record<string, unknown>, ['moduleId'])
  ) {
    throw new Error('Module Card detail query does not match the V1 schema.');
  }
  const request: QueryModuleCardDetailRequestV1 = {
    moduleId: query.moduleId,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_card_detail', { request });
  const response = parseModuleCardDetailResponseV1(payload);
  if (
    response.result.status === 'available' &&
    response.result.detail.moduleId !== query.moduleId
  ) {
    throw new Error('Module Card detail response does not match the selected module.');
  }
  return response;
}

export function parseModuleCardDetailResponseV1(payload: unknown): ModuleCardDetailResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Module Card detail response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ModuleCardDetailResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module Card detail response contains an invalid result.');
  }
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'moduleUnavailable',
    'cardUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['detail', 'status'])) {
    return { detail: parseDetail(value.detail), status: 'available' };
  }
  throw new Error('Module Card detail response contains an invalid result.');
}

function parseDetail(value: unknown): ModuleCardDetailV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'cardId',
      'confidenceBasisPoints',
      'currentIndexRunId',
      'currentSnapshotId',
      'fields',
      'lifecycle',
      'mapperProfileVersion',
      'moduleId',
      'schemaVersion',
      'sourceIndexRunId',
      'sourceSnapshotId',
    ]) ||
    !isStableId(value.cardId) ||
    !isStableId(value.currentIndexRunId) ||
    !isStableId(value.currentSnapshotId) ||
    !isStableId(value.sourceIndexRunId) ||
    !isStableId(value.sourceSnapshotId) ||
    !isStableId(value.moduleId) ||
    value.schemaVersion !== 1 ||
    value.mapperProfileVersion !== 1 ||
    !isConfidence(value.confidenceBasisPoints) ||
    !Array.isArray(value.fields) ||
    value.fields.length === 0 ||
    value.fields.length > MAX_FIELDS ||
    (value.sourceIndexRunId === value.currentIndexRunId &&
      value.sourceSnapshotId !== value.currentSnapshotId)
  ) {
    throw new Error('Module Card detail response contains an invalid Card envelope.');
  }
  const lifecycle = parseLifecycle(value.lifecycle);
  const fields = value.fields.map((field) => parseField(field, lifecycle.status));
  if (!isCanonicalFieldOrder(fields)) {
    throw new Error('Module Card detail fields are duplicated or unordered.');
  }
  const cardEvidence = new Set(fields.flatMap((field) => field.evidenceIds));
  const claimIds = fields.flatMap((field) => field.values.map((item) => item.claim.claimId));
  const documentBytes = fields.reduce(
    (total, field) =>
      total + field.values.reduce((fieldTotal, item) => fieldTotal + utf8Bytes(item.value), 0),
    0,
  );
  if (
    cardEvidence.size > MAX_CARD_EVIDENCE ||
    new Set(claimIds).size !== claimIds.length ||
    documentBytes > MAX_CARD_BYTES
  ) {
    throw new Error('Module Card detail response exceeds Card-wide bounds.');
  }
  return {
    cardId: value.cardId,
    confidenceBasisPoints: value.confidenceBasisPoints,
    currentIndexRunId: value.currentIndexRunId,
    currentSnapshotId: value.currentSnapshotId,
    fields,
    lifecycle,
    mapperProfileVersion: 1,
    moduleId: value.moduleId,
    schemaVersion: 1,
    sourceIndexRunId: value.sourceIndexRunId,
    sourceSnapshotId: value.sourceSnapshotId,
  };
}

function parseLifecycle(value: unknown): ModuleCardLifecycleV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module Card detail response contains an invalid lifecycle.');
  }
  if (value.status === 'current' && hasExactKeys(value, ['status'])) return { status: 'current' };
  if (
    value.status === 'stale' &&
    hasExactKeys(value, ['invalidatedByIndexRunId', 'reason', 'status']) &&
    isStableId(value.invalidatedByIndexRunId) &&
    isStaleReason(value.reason)
  ) {
    return {
      invalidatedByIndexRunId: value.invalidatedByIndexRunId,
      reason: value.reason,
      status: 'stale',
    };
  }
  if (
    value.status === 'needsReview' &&
    hasExactKeys(value, ['invalidatedByIndexRunId', 'reason', 'status']) &&
    isStableId(value.invalidatedByIndexRunId) &&
    value.reason === 'directDependencyChanged'
  ) {
    return {
      invalidatedByIndexRunId: value.invalidatedByIndexRunId,
      reason: value.reason,
      status: 'needsReview',
    };
  }
  throw new Error('Module Card detail response contains an invalid lifecycle.');
}

function parseField(
  value: unknown,
  expectedState: ModuleCardClaimStateV1,
): ModuleCardDetailFieldV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['evidenceIds', 'kind', 'values']) ||
    !isFieldKind(value.kind) ||
    !Array.isArray(value.evidenceIds) ||
    !Array.isArray(value.values)
  ) {
    throw new Error('Module Card detail response contains an invalid field.');
  }
  const spec = FIELD_SPECS[value.kind];
  const evidenceIds = parseEvidenceIds(value.evidenceIds, MAX_CARD_EVIDENCE);
  if (
    evidenceIds.length === 0 ||
    value.values.length === 0 ||
    value.values.length > spec.maxItems
  ) {
    throw new Error('Module Card detail response contains an invalid field bound.');
  }
  const fieldEvidence = new Set(evidenceIds);
  const values = value.values.map((item) =>
    parseValue(item, spec.maxItemBytes, expectedState, fieldEvidence),
  );
  if (new Set(values.map((item) => item.value)).size !== values.length) {
    throw new Error('Module Card detail response contains duplicate field values.');
  }
  return { evidenceIds, kind: value.kind, values };
}

function parseValue(
  value: unknown,
  maxBytes: number,
  expectedState: ModuleCardClaimStateV1,
  fieldEvidence: Set<string>,
): ModuleCardValueV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['claim', 'value']) ||
    typeof value.value !== 'string' ||
    value.value.trim().length === 0 ||
    utf8Bytes(value.value) > maxBytes ||
    containsControl(value.value)
  ) {
    throw new Error('Module Card detail response contains an invalid field value.');
  }
  const claim = parseClaim(value.claim);
  if (
    claim.state !== expectedState ||
    claim.evidenceIds.some((evidenceId) => !fieldEvidence.has(evidenceId))
  ) {
    throw new Error('Module Card detail claim contradicts Card freshness or field evidence.');
  }
  return { claim, value: value.value };
}

function parseClaim(value: unknown): ModuleCardClaimV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['claimId', 'confidenceBasisPoints', 'evidenceIds', 'kind', 'state']) ||
    !isStableId(value.claimId) ||
    !isClaimKind(value.kind) ||
    !isClaimState(value.state) ||
    !isConfidence(value.confidenceBasisPoints) ||
    !Array.isArray(value.evidenceIds)
  ) {
    throw new Error('Module Card detail response contains an invalid claim.');
  }
  const evidenceIds = parseEvidenceIds(value.evidenceIds, MAX_CLAIM_EVIDENCE);
  if (evidenceIds.length === 0 && value.kind !== 'hypothesis') {
    throw new Error('Module Card detail response contains an unsupported evidence-free claim.');
  }
  return {
    claimId: value.claimId,
    confidenceBasisPoints: value.confidenceBasisPoints,
    evidenceIds,
    kind: value.kind,
    state: value.state,
  };
}

function parseEvidenceIds(value: unknown[], maximum: number): string[] {
  if (
    value.length > maximum ||
    value.some((item) => !isStableId(item)) ||
    value.some((item, index) => index > 0 && String(value[index - 1]) >= String(item))
  ) {
    throw new Error('Module Card detail response contains invalid evidence identities.');
  }
  return value as string[];
}

const FIELD_ORDER: ModuleCardFieldKindV1[] = [
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
];

const FIELD_SPECS: Record<ModuleCardFieldKindV1, { maxItemBytes: number; maxItems: number }> = {
  title: { maxItemBytes: 256, maxItems: 1 },
  paths: { maxItemBytes: 1_024, maxItems: 32 },
  purpose: { maxItemBytes: 2_048, maxItems: 8 },
  responsibilities: { maxItemBytes: 2_048, maxItems: 32 },
  publicSurface: { maxItemBytes: 2_048, maxItems: 64 },
  entrypoints: { maxItemBytes: 2_048, maxItems: 64 },
  dependencies: { maxItemBytes: 2_048, maxItems: 128 },
  dataFlows: { maxItemBytes: 2_048, maxItems: 32 },
  invariants: { maxItemBytes: 2_048, maxItems: 32 },
  tests: { maxItemBytes: 2_048, maxItems: 128 },
  risks: { maxItemBytes: 2_048, maxItems: 32 },
  openQuestions: { maxItemBytes: 2_048, maxItems: 32 },
};

function isCanonicalFieldOrder(fields: ModuleCardDetailFieldV1[]): boolean {
  return fields.every(
    (field, index) =>
      index === 0 || FIELD_ORDER.indexOf(fields[index - 1]!.kind) < FIELD_ORDER.indexOf(field.kind),
  );
}

function isFieldKind(value: unknown): value is ModuleCardFieldKindV1 {
  return typeof value === 'string' && FIELD_ORDER.includes(value as ModuleCardFieldKindV1);
}

function isClaimKind(value: unknown): value is ModuleCardClaimKindV1 {
  return value === 'fact' || value === 'observation' || value === 'hypothesis';
}

function isClaimState(value: unknown): value is ModuleCardClaimStateV1 {
  return value === 'current' || value === 'stale' || value === 'needsReview';
}

function isStaleReason(
  value: unknown,
): value is Exclude<ModuleCardFreshnessReasonV1, 'directDependencyChanged'> {
  return (
    value === 'evidenceChanged' ||
    value === 'moduleRemoved' ||
    value === 'parserVersionChanged' ||
    value === 'mapperVersionChanged'
  );
}

function isConfidence(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= 10_000;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).length;
}

function containsControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return (
      codePoint !== undefined && (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f))
    );
  });
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

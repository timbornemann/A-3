import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAX_U64 = BigInt('18446744073709551615');
const ZERO = BigInt(0);
const MAX_REASONS = 5;

export interface QueryModuleCardFreshnessRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleCardFreshnessStatusV1 = 'stale' | 'needsReview';
export type ModuleCardFreshnessReasonV1 =
  | 'evidenceChanged'
  | 'moduleRemoved'
  | 'parserVersionChanged'
  | 'mapperVersionChanged'
  | 'directDependencyChanged';

export interface ModuleCardFreshnessReasonCountV1 {
  count: string;
  reason: ModuleCardFreshnessReasonV1;
  status: ModuleCardFreshnessStatusV1;
}

export interface ModuleCardFreshnessCountsV1 {
  needsReviewCount: string;
  publishedCount: string;
  staleCount: string;
  totalCount: string;
}

export interface ModuleCardFreshnessV1 {
  counts: ModuleCardFreshnessCountsV1;
  indexRunId: string;
  reasons: ModuleCardFreshnessReasonCountV1[];
  snapshotId: string;
}

export type ModuleCardFreshnessResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { freshness: ModuleCardFreshnessV1; status: 'available' };

export interface ModuleCardFreshnessResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleCardFreshnessResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleCardFreshness(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleCardFreshnessResponseV1> {
  const request: QueryModuleCardFreshnessRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_card_freshness', { request });
  return parseModuleCardFreshnessResponseV1(payload);
}

export function parseModuleCardFreshnessResponseV1(
  payload: unknown,
): ModuleCardFreshnessResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Module Card freshness response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ModuleCardFreshnessResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module Card freshness response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status === 'noPublishedIndex' && hasExactKeys(value, ['status'])) {
    return { status: 'noPublishedIndex' };
  }
  if (value.status === 'available' && hasExactKeys(value, ['freshness', 'status'])) {
    return { freshness: parseFreshness(value.freshness), status: 'available' };
  }
  throw new Error('Module Card freshness response contains an invalid result.');
}

function parseFreshness(value: unknown): ModuleCardFreshnessV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['counts', 'indexRunId', 'reasons', 'snapshotId']) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !Array.isArray(value.reasons) ||
    value.reasons.length > MAX_REASONS
  ) {
    throw new Error('Module Card freshness response contains an invalid projection.');
  }
  const counts = parseCounts(value.counts);
  const reasons = value.reasons.map(parseReasonCount);
  if (!isCanonicalReasonOrder(reasons)) {
    throw new Error('Module Card freshness reasons are duplicated or unordered.');
  }
  const staleFromReasons = sumReasons(reasons, 'stale');
  const needsReviewFromReasons = sumReasons(reasons, 'needsReview');
  const published = BigInt(counts.publishedCount);
  const stale = BigInt(counts.staleCount);
  const needsReview = BigInt(counts.needsReviewCount);
  const total = published + stale + needsReview;
  if (
    staleFromReasons !== stale ||
    needsReviewFromReasons !== needsReview ||
    total > MAX_U64 ||
    total !== BigInt(counts.totalCount)
  ) {
    throw new Error('Module Card freshness response contains contradictory counts.');
  }
  return {
    counts,
    indexRunId: value.indexRunId,
    reasons,
    snapshotId: value.snapshotId,
  };
}

function parseCounts(value: unknown): ModuleCardFreshnessCountsV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['needsReviewCount', 'publishedCount', 'staleCount', 'totalCount']) ||
    !isCount(value.needsReviewCount) ||
    !isCount(value.publishedCount) ||
    !isCount(value.staleCount) ||
    !isCount(value.totalCount)
  ) {
    throw new Error('Module Card freshness response contains invalid counts.');
  }
  return {
    needsReviewCount: value.needsReviewCount,
    publishedCount: value.publishedCount,
    staleCount: value.staleCount,
    totalCount: value.totalCount,
  };
}

function parseReasonCount(value: unknown): ModuleCardFreshnessReasonCountV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['count', 'reason', 'status']) ||
    !isCount(value.count) ||
    BigInt(value.count) === ZERO ||
    !isStatus(value.status) ||
    !isReason(value.reason) ||
    !isStatusReasonPair(value.status, value.reason)
  ) {
    throw new Error('Module Card freshness response contains an invalid reason count.');
  }
  return { count: value.count, reason: value.reason, status: value.status };
}

function isStatusReasonPair(
  status: ModuleCardFreshnessStatusV1,
  reason: ModuleCardFreshnessReasonV1,
): boolean {
  return status === 'needsReview'
    ? reason === 'directDependencyChanged'
    : reason !== 'directDependencyChanged';
}

function isCanonicalReasonOrder(reasons: ModuleCardFreshnessReasonCountV1[]): boolean {
  return reasons.every((reason, index) => {
    if (index === 0) return true;
    const previous = reasons[index - 1];
    return previous !== undefined && reasonRank(previous) < reasonRank(reason);
  });
}

function reasonRank(reason: ModuleCardFreshnessReasonCountV1): number {
  const statusRank = reason.status === 'stale' ? 0 : 1;
  const reasons: ModuleCardFreshnessReasonV1[] = [
    'evidenceChanged',
    'moduleRemoved',
    'parserVersionChanged',
    'mapperVersionChanged',
    'directDependencyChanged',
  ];
  return statusRank * MAX_REASONS + reasons.indexOf(reason.reason);
}

function sumReasons(
  reasons: ModuleCardFreshnessReasonCountV1[],
  status: ModuleCardFreshnessStatusV1,
): bigint {
  return reasons
    .filter((reason) => reason.status === status)
    .reduce((total, reason) => total + BigInt(reason.count), ZERO);
}

function isCount(value: unknown): value is string {
  return typeof value === 'string' && COUNT_PATTERN.test(value) && BigInt(value) <= MAX_U64;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isStatus(value: unknown): value is ModuleCardFreshnessStatusV1 {
  return value === 'stale' || value === 'needsReview';
}

function isReason(value: unknown): value is ModuleCardFreshnessReasonV1 {
  return (
    value === 'evidenceChanged' ||
    value === 'moduleRemoved' ||
    value === 'parserVersionChanged' ||
    value === 'mapperVersionChanged' ||
    value === 'directDependencyChanged'
  );
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

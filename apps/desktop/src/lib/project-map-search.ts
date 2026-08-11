import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModuleDependencySourceRangeV1 } from './module-dependency-graph';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_QUERY_BYTES = 4_096;
const MAX_QUERY_TOKENS = 32;
const MAX_HITS = 20;
const MAX_PATH_BYTES = 131_072;
const MAX_SYMBOL_TEXT_BYTES = 16_384;
const MAX_SCORE = 100_000;
const MAX_TOKEN_COST = 65_535;
const utf8 = new TextEncoder();

export interface ProjectMapSearchQueryV1 {
  query: string;
}

export interface QueryProjectMapSearchRequestV1 extends ProjectMapSearchQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ProjectMapSearchChannelV1 = 'exact' | 'lexical';
export type ProjectMapSearchPriorityV1 = 'exact' | 'evidence';
export type ProjectMapExactExplanationV1 =
  | 'normalizedPathExact'
  | 'qualifiedNameExact'
  | 'symbolNameExact'
  | 'signatureExact'
  | 'qualifiedNamePrefix'
  | 'symbolNamePrefix'
  | 'signaturePrefix'
  | 'manifestRole'
  | 'entrypointRole'
  | 'testRole';
export type ProjectMapLexicalExplanationV1 = 'path' | 'qualifiedName' | 'symbolName' | 'signature';
export type ProjectMapSearchSymbolKindV1 =
  | 'module'
  | 'namespace'
  | 'function'
  | 'method'
  | 'struct'
  | 'enum'
  | 'trait'
  | 'interface'
  | 'class'
  | 'implementation'
  | 'typeAlias'
  | 'constant'
  | 'static'
  | 'variable'
  | 'field'
  | 'variant'
  | 'parameter';

export type ProjectMapSearchSourceV1 =
  | {
      channel: 'exact';
      explanation: ProjectMapExactExplanationV1;
      normalizedScoreBasisPoints: number;
    }
  | {
      channel: 'lexical';
      explanation: ProjectMapLexicalExplanationV1;
      nativeScore: number;
      normalizedScoreBasisPoints: number;
    };

export interface ProjectMapSearchEvidenceV1 {
  contentHash: string;
  declarationRange: ModuleDependencySourceRangeV1 | null;
  pathDisplay: string;
  pathHex: string;
}

export type ProjectMapSearchTargetV1 =
  | { evidence: ProjectMapSearchEvidenceV1; kind: 'file' }
  | {
      evidence: ProjectMapSearchEvidenceV1;
      kind: 'symbol';
      name: string;
      qualifiedName: string;
      signature: string | null;
      symbolId: string;
      symbolKind: ProjectMapSearchSymbolKindV1;
    };

export interface ProjectMapSearchHitV1 {
  finalScore: number;
  priority: ProjectMapSearchPriorityV1;
  rank: number;
  sources: ProjectMapSearchSourceV1[];
  target: ProjectMapSearchTargetV1;
}

export interface ProjectMapSearchV1 {
  fusionPolicyVersion: 1;
  hits: ProjectMapSearchHitV1[];
  indexRunId: string;
  query: string;
  snapshotId: string;
  truncated: boolean;
}

export type ProjectMapSearchResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { channel: ProjectMapSearchChannelV1; status: 'projectionUnavailable' }
  | { search: ProjectMapSearchV1; status: 'available' };

export interface ProjectMapSearchResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectMapSearchResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectMapSearch(
  query: ProjectMapSearchQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectMapSearchResponseV1> {
  if (!isSearchQuery(query.query)) throw new Error('Project Map search query does not match V1.');
  const request: QueryProjectMapSearchRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    query: query.query.trim(),
  };
  const payload = await invokeCommand('query_project_map_search', { request });
  const response = parseProjectMapSearchResponseV1(payload);
  if (response.result.status === 'available' && response.result.search.query !== request.query) {
    throw new Error('Project Map search response does not match its query.');
  }
  return response;
}

export function parseProjectMapSearchResponseV1(payload: unknown): ProjectMapSearchResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Project Map search response does not match V1.');
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: parseResult(payload.result),
  };
}

function parseResult(value: unknown): ProjectMapSearchResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  if (
    (value.status === 'noProject' || value.status === 'noPublishedIndex') &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status };
  }
  if (
    value.status === 'projectionUnavailable' &&
    hasExactKeys(value, ['channel', 'status']) &&
    (value.channel === 'exact' || value.channel === 'lexical')
  ) {
    return { channel: value.channel, status: 'projectionUnavailable' };
  }
  if (value.status === 'available' && hasExactKeys(value, ['search', 'status'])) {
    return { search: parseSearch(value.search), status: 'available' };
  }
  return invalidResult();
}

function invalidResult(): never {
  throw new Error('Project Map search response contains an invalid result.');
}

function parseSearch(value: unknown): ProjectMapSearchV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'fusionPolicyVersion',
      'hits',
      'indexRunId',
      'query',
      'snapshotId',
      'truncated',
    ]) ||
    value.fusionPolicyVersion !== 1 ||
    !Array.isArray(value.hits) ||
    value.hits.length > MAX_HITS ||
    !isStableId(value.indexRunId) ||
    !isSearchQuery(value.query) ||
    value.query !== value.query.trim() ||
    !isStableId(value.snapshotId) ||
    typeof value.truncated !== 'boolean'
  ) {
    throw new Error('Project Map search response contains an invalid search projection.');
  }
  const hits = value.hits.map(parseHit);
  const targetIds = hits.map(targetIdentity);
  if (
    new Set(targetIds).size !== hits.length ||
    hits.some((hit, index) => hit.rank !== index + 1) ||
    hits.some((hit, index) => index > 0 && compareHits(hits[index - 1], hit) > 0)
  ) {
    throw new Error('Project Map search hits violate deterministic ordering or identity.');
  }
  return {
    fusionPolicyVersion: 1,
    hits,
    indexRunId: value.indexRunId,
    query: value.query,
    snapshotId: value.snapshotId,
    truncated: value.truncated,
  };
}

function parseHit(value: unknown): ProjectMapSearchHitV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['finalScore', 'priority', 'rank', 'sources', 'target']) ||
    !isIntegerBetween(value.rank, 1, MAX_HITS) ||
    !isIntegerBetween(value.finalScore, 0, MAX_SCORE) ||
    (value.priority !== 'exact' && value.priority !== 'evidence') ||
    !Array.isArray(value.sources) ||
    value.sources.length === 0 ||
    value.sources.length > 2
  ) {
    throw new Error('Project Map search response contains an invalid hit.');
  }
  const sources = value.sources.map(parseSource);
  const channels = sources.map((source) => source.channel);
  const expectedPriority = channels.includes('exact') ? 'exact' : 'evidence';
  if (
    new Set(channels).size !== channels.length ||
    channels.join(',') !== [...channels].sort(channelOrder).join(',') ||
    value.priority !== expectedPriority
  ) {
    throw new Error('Project Map search response contains inconsistent provenance.');
  }
  const target = parseTarget(value.target);
  if (value.finalScore !== expectedFinalScore(sources, target)) {
    throw new Error('Project Map search response contains an inconsistent fusion score.');
  }
  return {
    finalScore: value.finalScore,
    priority: value.priority,
    rank: value.rank,
    sources,
    target,
  };
}

function parseSource(value: unknown): ProjectMapSearchSourceV1 {
  if (!isRecord(value) || typeof value.channel !== 'string') return invalidSource();
  if (
    value.channel === 'exact' &&
    hasExactKeys(value, ['channel', 'explanation', 'normalizedScoreBasisPoints']) &&
    isExactExplanation(value.explanation) &&
    value.normalizedScoreBasisPoints === exactScore(value.explanation)
  ) {
    return {
      channel: 'exact',
      explanation: value.explanation,
      normalizedScoreBasisPoints: value.normalizedScoreBasisPoints,
    };
  }
  if (
    value.channel === 'lexical' &&
    hasExactKeys(value, ['channel', 'explanation', 'nativeScore', 'normalizedScoreBasisPoints']) &&
    isLexicalExplanation(value.explanation) &&
    isIntegerBetween(value.nativeScore, 1, MAX_SCORE) &&
    value.normalizedScoreBasisPoints === Math.floor(value.nativeScore / 10)
  ) {
    return {
      channel: 'lexical',
      explanation: value.explanation,
      nativeScore: value.nativeScore,
      normalizedScoreBasisPoints: value.normalizedScoreBasisPoints,
    };
  }
  return invalidSource();
}

function invalidSource(): never {
  throw new Error('Project Map search response contains an invalid source explanation.');
}

function parseTarget(value: unknown): ProjectMapSearchTargetV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalidTarget();
  if (value.kind === 'file' && hasExactKeys(value, ['evidence', 'kind'])) {
    const evidence = parseEvidence(value.evidence);
    if (evidence.declarationRange !== null) return invalidTarget();
    return { evidence, kind: 'file' };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, [
      'evidence',
      'kind',
      'name',
      'qualifiedName',
      'signature',
      'symbolId',
      'symbolKind',
    ]) &&
    isStableId(value.symbolId) &&
    isSymbolKind(value.symbolKind) &&
    isBoundedText(value.name, MAX_SYMBOL_TEXT_BYTES, false) &&
    isBoundedText(value.qualifiedName, MAX_SYMBOL_TEXT_BYTES, false) &&
    (value.signature === null || isBoundedText(value.signature, MAX_SYMBOL_TEXT_BYTES, true))
  ) {
    const evidence = parseEvidence(value.evidence);
    if (evidence.declarationRange === null) return invalidTarget();
    return {
      evidence,
      kind: 'symbol',
      name: value.name,
      qualifiedName: value.qualifiedName,
      signature: value.signature,
      symbolId: value.symbolId,
      symbolKind: value.symbolKind,
    };
  }
  return invalidTarget();
}

function invalidTarget(): never {
  throw new Error('Project Map search response contains an invalid evidence target.');
}

function parseEvidence(value: unknown): ProjectMapSearchEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['contentHash', 'declarationRange', 'pathDisplay', 'pathHex']) ||
    !isStableId(value.contentHash) ||
    !isPathHex(value.pathHex) ||
    typeof value.pathDisplay !== 'string' ||
    value.pathDisplay.length === 0 ||
    containsUnsupportedControl(value.pathDisplay)
  ) {
    throw new Error('Project Map search response contains invalid evidence metadata.');
  }
  return {
    contentHash: value.contentHash,
    declarationRange:
      value.declarationRange === null ? null : parseSourceRange(value.declarationRange),
    pathDisplay: value.pathDisplay,
    pathHex: value.pathHex,
  };
}

function parseSourceRange(value: unknown): ModuleDependencySourceRangeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['end', 'endByte', 'start', 'startByte']) ||
    !isIntegerBetween(value.startByte, 0, 4_294_967_295) ||
    !isIntegerBetween(value.endByte, 1, 4_294_967_295) ||
    value.startByte >= value.endByte
  ) {
    throw new Error('Project Map search response contains an invalid declaration range.');
  }
  const start = parsePosition(value.start);
  const end = parsePosition(value.end);
  if (end.row < start.row || (end.row === start.row && end.column < start.column)) {
    throw new Error('Project Map search response contains a reversed declaration range.');
  }
  return { end, endByte: value.endByte, start, startByte: value.startByte };
}

function parsePosition(value: unknown): { column: number; row: number } {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['column', 'row']) ||
    !isIntegerBetween(value.row, 0, 4_294_967_295) ||
    !isIntegerBetween(value.column, 0, 4_294_967_295)
  ) {
    throw new Error('Project Map search response contains an invalid source position.');
  }
  return { column: value.column, row: value.row };
}

function expectedFinalScore(
  sources: ProjectMapSearchSourceV1[],
  target: ProjectMapSearchTargetV1,
): number {
  const source = Math.max(...sources.map((item) => item.normalizedScoreBasisPoints)) * 3;
  const freshness = 10_000;
  const tokenCost = targetTokenCost(target);
  const tokenPenalty = Math.floor(((tokenCost - 1) * 10_000) / (MAX_TOKEN_COST - 1));
  const tokenEfficiency = 10_000 - tokenPenalty;
  const corroboration = sources.length > 1 ? 2_500 : 0;
  return Math.min(MAX_SCORE, source + freshness + tokenEfficiency + corroboration);
}

function targetTokenCost(target: ProjectMapSearchTargetV1): number {
  const pathBytes = target.evidence.pathHex.length / 2;
  if (target.kind === 'file') return Math.min(MAX_TOKEN_COST, pathBytes + 96);
  const range = target.evidence.declarationRange;
  if (range === null) return MAX_TOKEN_COST;
  const bytes =
    pathBytes +
    utf8.encode(target.name).length +
    utf8.encode(target.qualifiedName).length +
    (target.signature === null ? 0 : utf8.encode(target.signature).length) +
    (range.endByte - range.startByte) +
    96;
  return Math.max(1, Math.min(MAX_TOKEN_COST, bytes));
}

function compareHits(left: ProjectMapSearchHitV1, right: ProjectMapSearchHitV1): number {
  const priority = priorityOrder(left.priority) - priorityOrder(right.priority);
  if (priority !== 0) return priority;
  if (left.finalScore !== right.finalScore) return right.finalScore - left.finalScore;
  return compareHexIdentity(targetIdentity(left), targetIdentity(right));
}

function targetIdentity(hit: ProjectMapSearchHitV1): string {
  return hit.target.kind === 'file'
    ? `0:${hit.target.evidence.pathHex}`
    : `1:${hit.target.symbolId}`;
}

function compareHexIdentity(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function priorityOrder(value: ProjectMapSearchPriorityV1): number {
  return value === 'exact' ? 0 : 1;
}

function channelOrder(left: ProjectMapSearchChannelV1, right: ProjectMapSearchChannelV1): number {
  return (left === 'exact' ? 0 : 1) - (right === 'exact' ? 0 : 1);
}

function exactScore(value: ProjectMapExactExplanationV1): number {
  switch (value) {
    case 'normalizedPathExact':
    case 'qualifiedNameExact':
      return 10_000;
    case 'symbolNameExact':
      return 9_500;
    case 'signatureExact':
    case 'manifestRole':
    case 'entrypointRole':
    case 'testRole':
      return 9_000;
    case 'qualifiedNamePrefix':
      return 8_000;
    case 'symbolNamePrefix':
      return 7_500;
    case 'signaturePrefix':
      return 7_000;
  }
}

function isSearchQuery(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const normalized = value.trim();
  if (
    normalized.length === 0 ||
    utf8.encode(normalized).length > MAX_QUERY_BYTES ||
    containsAnyControl(normalized)
  ) {
    return false;
  }
  const tokens = normalized.match(/[\p{L}\p{N}_]+/gu) ?? [];
  const searchable = tokens.filter((token) => Array.from(token).length >= 3);
  return searchable.length >= 1 && searchable.length <= MAX_QUERY_TOKENS;
}

function isExactExplanation(value: unknown): value is ProjectMapExactExplanationV1 {
  return [
    'normalizedPathExact',
    'qualifiedNameExact',
    'symbolNameExact',
    'signatureExact',
    'qualifiedNamePrefix',
    'symbolNamePrefix',
    'signaturePrefix',
    'manifestRole',
    'entrypointRole',
    'testRole',
  ].includes(value as ProjectMapExactExplanationV1);
}

function isLexicalExplanation(value: unknown): value is ProjectMapLexicalExplanationV1 {
  return ['path', 'qualifiedName', 'symbolName', 'signature'].includes(
    value as ProjectMapLexicalExplanationV1,
  );
}

function isSymbolKind(value: unknown): value is ProjectMapSearchSymbolKindV1 {
  return [
    'module',
    'namespace',
    'function',
    'method',
    'struct',
    'enum',
    'trait',
    'interface',
    'class',
    'implementation',
    'typeAlias',
    'constant',
    'static',
    'variable',
    'field',
    'variant',
    'parameter',
  ].includes(value as ProjectMapSearchSymbolKindV1);
}

function isBoundedText(value: unknown, maxBytes: number, allowLayout: boolean): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    utf8.encode(value).length <= maxBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return (
        code !== undefined &&
        ((code <= 31 && !(allowLayout && (code === 9 || code === 10 || code === 13))) ||
          (code >= 127 && code <= 159))
      );
    })
  );
}

function isPathHex(value: unknown): value is string {
  return typeof value === 'string' && value.length <= MAX_PATH_BYTES * 2 && HEX_PATTERN.test(value);
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isIntegerBetween(value: unknown, minimum: number, maximum: number): value is number {
  return (
    typeof value === 'number' && Number.isInteger(value) && value >= minimum && value <= maximum
  );
}

function containsUnsupportedControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.codePointAt(0);
    return (
      code !== undefined &&
      ((code <= 31 && code !== 9 && code !== 10) || (code >= 127 && code <= 159))
    );
  });
}

function containsAnyControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.codePointAt(0);
    return code !== undefined && (code <= 31 || (code >= 127 && code <= 159));
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const keys = Object.keys(value).sort();
  const expectedKeys = [...expected].sort();
  return (
    keys.length === expectedKeys.length && keys.every((key, index) => key === expectedKeys[index])
  );
}

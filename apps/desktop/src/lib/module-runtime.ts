import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type {
  ModuleDependencyEdgeEvidenceV1,
  ModuleDependencyEndpointV1,
  ModuleDependencyProviderV1,
  ModuleDependencyRelationV1,
  ModuleDependencyResolutionV1,
  ModuleDependencySourcePositionV1,
  ModuleDependencySourceRangeV1,
} from './module-dependency-graph';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,2})$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_U32 = 4_294_967_295;
const MAX_PATH_BYTES = 131_072;
const MAX_SYMBOL_NAME_BYTES = 1_024;
const MAX_ROOTS = 256;
const MAX_FLOW_HITS = 100;

export interface ModuleRuntimeMapQueryV1 {
  entrypointLimit: number;
  moduleId: string;
  testLimit: number;
}

export interface QueryModuleRuntimeMapRequestV1 extends ModuleRuntimeMapQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleRuntimeRootKindV1 = 'entrypoint' | 'test';

export type ModuleRuntimeSymbolKindV1 =
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

export interface ModuleRuntimeSymbolV1 {
  contentHash: string;
  evidenceId: string;
  name: string;
  pathHex: string;
  selectionRange: ModuleDependencySourceRangeV1;
  symbolId: string;
  symbolKind: ModuleRuntimeSymbolKindV1;
}

export interface ModuleRuntimeRootV1 {
  kind: ModuleRuntimeRootKindV1;
  rank: number;
  symbol: ModuleRuntimeSymbolV1;
}

export interface ModuleRuntimeRootSetV1 {
  projectionTruncated: boolean;
  roots: ModuleRuntimeRootV1[];
  storedCount: string;
  visibleTruncated: boolean;
}

export interface ModuleRuntimeMapV1 {
  entrypoints: ModuleRuntimeRootSetV1;
  indexRunId: string;
  moduleId: string;
  snapshotId: string;
  tests: ModuleRuntimeRootSetV1;
}

export type ModuleRuntimeMapResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'moduleUnavailable' }
  | { map: ModuleRuntimeMapV1; status: 'available' };

export interface ModuleRuntimeMapResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleRuntimeMapResultV1;
}

export type ModuleRuntimeFlowKindV1 = 'entrypointCalls' | 'testTargets';

export interface ModuleRuntimeFlowQueryV1 {
  expectedIndexRunId: string;
  expectedSnapshotId: string;
  kind: ModuleRuntimeFlowKindV1;
  moduleId: string;
  resultLimit: number;
  rootSymbolId: string;
}

export interface QueryModuleRuntimeFlowRequestV1 extends ModuleRuntimeFlowQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleRuntimeFlowTargetV1 =
  | { contentHash: string; evidenceId: string; kind: 'file'; pathHex: string }
  | { kind: 'symbol'; symbol: ModuleRuntimeSymbolV1 };

export interface ModuleRuntimeFlowEdgeV1 {
  evidence: ModuleDependencyEdgeEvidenceV1;
  relation: Extract<ModuleDependencyRelationV1, 'calls' | 'tests'>;
}

export interface ModuleRuntimeFlowHitV1 {
  path: ModuleRuntimeFlowEdgeV1[];
  target: ModuleRuntimeFlowTargetV1;
}

export interface ModuleRuntimeFlowV1 {
  hits: ModuleRuntimeFlowHitV1[];
  indexRunId: string;
  kind: ModuleRuntimeFlowKindV1;
  moduleId: string;
  rootSymbolId: string;
  snapshotId: string;
  truncated: boolean;
}

export type ModuleRuntimeFlowResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'publicationChanged' }
  | { status: 'moduleUnavailable' }
  | { status: 'rootUnavailable' }
  | { flow: ModuleRuntimeFlowV1; status: 'available' };

export interface ModuleRuntimeFlowResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleRuntimeFlowResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleRuntimeMap(
  query: ModuleRuntimeMapQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleRuntimeMapResponseV1> {
  if (!isMapQuery(query)) throw new Error('Module runtime-map query does not match V1.');
  const request: QueryModuleRuntimeMapRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_runtime_map', { request });
  const response = parseModuleRuntimeMapResponseV1(payload);
  if (
    response.result.status === 'available' &&
    (response.result.map.moduleId !== query.moduleId ||
      response.result.map.entrypoints.roots.length > query.entrypointLimit ||
      response.result.map.tests.roots.length > query.testLimit)
  ) {
    throw new Error('Module runtime-map response does not match the selected module or bounds.');
  }
  return response;
}

export async function queryModuleRuntimeFlow(
  query: ModuleRuntimeFlowQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleRuntimeFlowResponseV1> {
  if (!isFlowQuery(query)) throw new Error('Module runtime-flow query does not match V1.');
  const request: QueryModuleRuntimeFlowRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_runtime_flow', { request });
  const response = parseModuleRuntimeFlowResponseV1(payload);
  if (
    response.result.status === 'available' &&
    (response.result.flow.indexRunId !== query.expectedIndexRunId ||
      response.result.flow.snapshotId !== query.expectedSnapshotId ||
      response.result.flow.moduleId !== query.moduleId ||
      response.result.flow.rootSymbolId !== query.rootSymbolId ||
      response.result.flow.kind !== query.kind ||
      response.result.flow.hits.length > query.resultLimit)
  ) {
    throw new Error('Module runtime-flow response does not match its visible seed.');
  }
  return response;
}

export function parseModuleRuntimeMapResponseV1(payload: unknown): ModuleRuntimeMapResponseV1 {
  const result = parseEnvelope(payload, parseMapResult, 'runtime-map');
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result };
}

export function parseModuleRuntimeFlowResponseV1(payload: unknown): ModuleRuntimeFlowResponseV1 {
  const result = parseEnvelope(payload, parseFlowResult, 'runtime-flow');
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result };
}

function parseEnvelope<T>(payload: unknown, parseResult: (value: unknown) => T, label: string): T {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error(`Module ${label} response does not match V1.`);
  }
  return parseResult(payload.result);
}

function parseMapResult(value: unknown): ModuleRuntimeMapResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidMapResult();
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'moduleUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['map', 'status'])) {
    return { map: parseMap(value.map), status: 'available' };
  }
  return invalidMapResult();
}

function invalidMapResult(): never {
  throw new Error('Module runtime-map response contains an invalid result.');
}

function parseMap(value: unknown): ModuleRuntimeMapV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['entrypoints', 'indexRunId', 'moduleId', 'snapshotId', 'tests']) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isStableId(value.moduleId)
  ) {
    throw new Error('Module runtime-map response contains an invalid map.');
  }
  return {
    entrypoints: parseRootSet(value.entrypoints, 'entrypoint'),
    indexRunId: value.indexRunId,
    moduleId: value.moduleId,
    snapshotId: value.snapshotId,
    tests: parseRootSet(value.tests, 'test'),
  };
}

function parseRootSet(value: unknown, kind: ModuleRuntimeRootKindV1): ModuleRuntimeRootSetV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['projectionTruncated', 'roots', 'storedCount', 'visibleTruncated']) ||
    !Array.isArray(value.roots) ||
    value.roots.length > MAX_ROOTS ||
    !isRootCount(value.storedCount) ||
    typeof value.projectionTruncated !== 'boolean' ||
    typeof value.visibleTruncated !== 'boolean'
  ) {
    throw new Error('Module runtime-map response contains an invalid root set.');
  }
  const roots = value.roots.map((root) => parseRoot(root, kind));
  const storedCount = Number(value.storedCount);
  const ids = new Set(roots.map((root) => root.symbol.symbolId));
  const expectedVisibleTruncation = roots.length < storedCount || value.projectionTruncated;
  if (
    roots.length > storedCount ||
    ids.size !== roots.length ||
    roots.some((root, index) => root.rank !== index + 1) ||
    (value.projectionTruncated && storedCount === 0) ||
    value.visibleTruncated !== expectedVisibleTruncation
  ) {
    throw new Error('Module runtime-map root set contradicts its bounds.');
  }
  return {
    projectionTruncated: value.projectionTruncated,
    roots,
    storedCount: value.storedCount,
    visibleTruncated: value.visibleTruncated,
  };
}

function parseRoot(value: unknown, kind: ModuleRuntimeRootKindV1): ModuleRuntimeRootV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['kind', 'rank', 'symbol']) ||
    value.kind !== kind ||
    !Number.isInteger(value.rank) ||
    typeof value.rank !== 'number' ||
    value.rank < 1 ||
    value.rank > MAX_ROOTS
  ) {
    throw new Error('Module runtime-map response contains an invalid root.');
  }
  return { kind, rank: value.rank, symbol: parseSymbol(value.symbol) };
}

function parseSymbol(value: unknown): ModuleRuntimeSymbolV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'contentHash',
      'evidenceId',
      'name',
      'pathHex',
      'selectionRange',
      'symbolId',
      'symbolKind',
    ]) ||
    !isStableId(value.symbolId) ||
    !isSymbolKind(value.symbolKind) ||
    typeof value.name !== 'string' ||
    new TextEncoder().encode(value.name).length === 0 ||
    new TextEncoder().encode(value.name).length > MAX_SYMBOL_NAME_BYTES ||
    containsControl(value.name) ||
    !isStableId(value.evidenceId) ||
    !isRepositoryPathHex(value.pathHex) ||
    !isStableId(value.contentHash)
  ) {
    throw new Error('Module runtime response contains an invalid symbol.');
  }
  return {
    contentHash: value.contentHash,
    evidenceId: value.evidenceId,
    name: value.name,
    pathHex: value.pathHex,
    selectionRange: parseRange(value.selectionRange),
    symbolId: value.symbolId,
    symbolKind: value.symbolKind,
  };
}

function parseFlowResult(value: unknown): ModuleRuntimeFlowResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidFlowResult();
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'publicationChanged',
    'moduleUnavailable',
    'rootUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['flow', 'status'])) {
    return { flow: parseFlow(value.flow), status: 'available' };
  }
  return invalidFlowResult();
}

function invalidFlowResult(): never {
  throw new Error('Module runtime-flow response contains an invalid result.');
}

function parseFlow(value: unknown): ModuleRuntimeFlowV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'hits',
      'indexRunId',
      'kind',
      'moduleId',
      'rootSymbolId',
      'snapshotId',
      'truncated',
    ]) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isStableId(value.moduleId) ||
    !isStableId(value.rootSymbolId) ||
    !isFlowKind(value.kind) ||
    !Array.isArray(value.hits) ||
    value.hits.length > MAX_FLOW_HITS ||
    typeof value.truncated !== 'boolean'
  ) {
    throw new Error('Module runtime-flow response contains an invalid flow.');
  }
  const rootSymbolId = value.rootSymbolId;
  const kind = value.kind;
  const hits = value.hits.map((hit) => parseFlowHit(hit, rootSymbolId, kind));
  const targets = new Set(hits.map((hit) => endpointKey(targetEndpoint(hit.target))));
  if (targets.size !== hits.length) {
    throw new Error('Module runtime-flow response repeats a target.');
  }
  return {
    hits,
    indexRunId: value.indexRunId,
    kind,
    moduleId: value.moduleId,
    rootSymbolId,
    snapshotId: value.snapshotId,
    truncated: value.truncated,
  };
}

function parseFlowHit(
  value: unknown,
  rootSymbolId: string,
  kind: ModuleRuntimeFlowKindV1,
): ModuleRuntimeFlowHitV1 {
  if (!isRecord(value) || !hasExactKeys(value, ['path', 'target']) || !Array.isArray(value.path)) {
    throw new Error('Module runtime-flow response contains an invalid hit.');
  }
  const maximumDepth = kind === 'entrypointCalls' ? 2 : 1;
  const relation = kind === 'entrypointCalls' ? 'calls' : 'tests';
  if (value.path.length === 0 || value.path.length > maximumDepth) {
    throw new Error('Module runtime-flow response contains an invalid path depth.');
  }
  const path = value.path.map((edge) => parseFlowEdge(edge, relation));
  const target = parseFlowTarget(value.target);
  let current: ModuleDependencyEndpointV1 = { kind: 'symbol', symbolId: rootSymbolId };
  const visited = new Set([endpointKey(current)]);
  for (const edge of path) {
    if (!endpointsEqual(edge.evidence.source, current)) {
      throw new Error('Module runtime-flow response contains a disconnected path.');
    }
    current = edge.evidence.target;
    if (visited.has(endpointKey(current))) {
      throw new Error('Module runtime-flow response contains a cycle.');
    }
    visited.add(endpointKey(current));
  }
  if (!endpointsEqual(current, targetEndpoint(target))) {
    throw new Error('Module runtime-flow target does not match its evidence path.');
  }
  return { path, target };
}

function parseFlowTarget(value: unknown): ModuleRuntimeFlowTargetV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') {
    throw new Error('Module runtime-flow response contains an invalid target.');
  }
  if (
    value.kind === 'file' &&
    hasExactKeys(value, ['contentHash', 'evidenceId', 'kind', 'pathHex']) &&
    isStableId(value.evidenceId) &&
    isRepositoryPathHex(value.pathHex) &&
    isStableId(value.contentHash)
  ) {
    return {
      contentHash: value.contentHash,
      evidenceId: value.evidenceId,
      kind: 'file',
      pathHex: value.pathHex,
    };
  }
  if (value.kind === 'symbol' && hasExactKeys(value, ['kind', 'symbol'])) {
    return { kind: 'symbol', symbol: parseSymbol(value.symbol) };
  }
  throw new Error('Module runtime-flow response contains an invalid target.');
}

function parseFlowEdge(
  value: unknown,
  expectedRelation: 'calls' | 'tests',
): ModuleRuntimeFlowEdgeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['evidence', 'relation']) ||
    value.relation !== expectedRelation
  ) {
    throw new Error('Module runtime-flow response contains an invalid relation.');
  }
  return { evidence: parseEdgeEvidence(value.evidence), relation: expectedRelation };
}

function parseEdgeEvidence(value: unknown): ModuleDependencyEdgeEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'confidenceBasisPoints',
      'contentHash',
      'evidenceId',
      'pathHex',
      'provider',
      'range',
      'resolution',
      'source',
      'target',
    ]) ||
    !isStableId(value.evidenceId) ||
    !isRepositoryPathHex(value.pathHex) ||
    !isStableId(value.contentHash) ||
    !isProvider(value.provider) ||
    !isResolution(value.resolution) ||
    !Number.isInteger(value.confidenceBasisPoints) ||
    typeof value.confidenceBasisPoints !== 'number' ||
    value.confidenceBasisPoints < 0 ||
    value.confidenceBasisPoints > 10_000
  ) {
    throw new Error('Module runtime-flow response contains invalid edge evidence.');
  }
  const source = parseEndpoint(value.source);
  const target = parseEndpoint(value.target);
  if (endpointsEqual(source, target)) {
    throw new Error('Module runtime-flow response contains a self edge.');
  }
  return {
    confidenceBasisPoints: value.confidenceBasisPoints,
    contentHash: value.contentHash,
    evidenceId: value.evidenceId,
    pathHex: value.pathHex,
    provider: value.provider,
    range: parseRange(value.range),
    resolution: value.resolution,
    source,
    target,
  };
}

function parseEndpoint(value: unknown): ModuleDependencyEndpointV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') {
    throw new Error('Module runtime-flow response contains an invalid endpoint.');
  }
  if (
    value.kind === 'file' &&
    hasExactKeys(value, ['kind', 'pathHex']) &&
    isRepositoryPathHex(value.pathHex)
  ) {
    return { kind: 'file', pathHex: value.pathHex };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, ['kind', 'symbolId']) &&
    isStableId(value.symbolId)
  ) {
    return { kind: 'symbol', symbolId: value.symbolId };
  }
  throw new Error('Module runtime-flow response contains an invalid endpoint.');
}

function parseRange(value: unknown): ModuleDependencySourceRangeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['end', 'endByte', 'start', 'startByte']) ||
    !isU32(value.startByte) ||
    !isU32(value.endByte) ||
    value.startByte > value.endByte
  ) {
    throw new Error('Module runtime response contains an invalid source range.');
  }
  const start = parsePosition(value.start);
  const end = parsePosition(value.end);
  if (start.row > end.row || (start.row === end.row && start.column > end.column)) {
    throw new Error('Module runtime response contains an inverted source range.');
  }
  return { end, endByte: value.endByte, start, startByte: value.startByte };
}

function parsePosition(value: unknown): ModuleDependencySourcePositionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['column', 'row']) ||
    !isU32(value.column) ||
    !isU32(value.row)
  ) {
    throw new Error('Module runtime response contains an invalid source position.');
  }
  return { column: value.column, row: value.row };
}

function targetEndpoint(target: ModuleRuntimeFlowTargetV1): ModuleDependencyEndpointV1 {
  return target.kind === 'file'
    ? { kind: 'file', pathHex: target.pathHex }
    : { kind: 'symbol', symbolId: target.symbol.symbolId };
}

function endpointsEqual(
  left: ModuleDependencyEndpointV1,
  right: ModuleDependencyEndpointV1,
): boolean {
  return endpointKey(left) === endpointKey(right);
}

function endpointKey(endpoint: ModuleDependencyEndpointV1): string {
  return endpoint.kind === 'file' ? `f:${endpoint.pathHex}` : `s:${endpoint.symbolId}`;
}

function isMapQuery(value: ModuleRuntimeMapQueryV1): boolean {
  return (
    hasExactKeys(value as unknown as Record<string, unknown>, [
      'entrypointLimit',
      'moduleId',
      'testLimit',
    ]) &&
    isStableId(value.moduleId) &&
    isRootLimit(value.entrypointLimit) &&
    isRootLimit(value.testLimit)
  );
}

function isFlowQuery(value: ModuleRuntimeFlowQueryV1): boolean {
  return (
    hasExactKeys(value as unknown as Record<string, unknown>, [
      'expectedIndexRunId',
      'expectedSnapshotId',
      'kind',
      'moduleId',
      'resultLimit',
      'rootSymbolId',
    ]) &&
    isStableId(value.expectedIndexRunId) &&
    isStableId(value.expectedSnapshotId) &&
    isStableId(value.moduleId) &&
    isStableId(value.rootSymbolId) &&
    isFlowKind(value.kind) &&
    Number.isInteger(value.resultLimit) &&
    value.resultLimit >= 1 &&
    value.resultLimit <= MAX_FLOW_HITS
  );
}

const SYMBOL_KINDS = new Set<ModuleRuntimeSymbolKindV1>([
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
]);

function isSymbolKind(value: unknown): value is ModuleRuntimeSymbolKindV1 {
  return typeof value === 'string' && SYMBOL_KINDS.has(value as ModuleRuntimeSymbolKindV1);
}

function isFlowKind(value: unknown): value is ModuleRuntimeFlowKindV1 {
  return value === 'entrypointCalls' || value === 'testTargets';
}

function isProvider(value: unknown): value is ModuleDependencyProviderV1 {
  return value === 'treeSitter' || value === 'manifest' || value === 'languageHeuristic';
}

function isResolution(value: unknown): value is ModuleDependencyResolutionV1 {
  return (
    value === 'adapterLocalSymbol' ||
    value === 'adapterFile' ||
    value === 'exactModuleReference' ||
    value === 'uniqueFileLocalName' ||
    value === 'uniqueQualifiedName'
  );
}

function isRootLimit(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 1 && value <= MAX_ROOTS;
}

function isRootCount(value: unknown): value is string {
  return typeof value === 'string' && COUNT_PATTERN.test(value) && Number(value) <= MAX_ROOTS;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isU32(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= MAX_U32;
}

function isRepositoryPathHex(value: unknown): value is string {
  if (
    typeof value !== 'string' ||
    value.length > MAX_PATH_BYTES * 2 ||
    !HEX_PATTERN.test(value) ||
    containsHexByte(value, '00')
  )
    return false;
  const segments = splitHexPath(value);
  return (
    segments.length > 0 &&
    segments.every((segment) => segment.length > 0 && segment !== '2e' && segment !== '2e2e')
  );
}

function splitHexPath(value: string): string[] {
  const segments: string[] = [];
  let start = 0;
  for (let index = 0; index < value.length; index += 2) {
    if (value.slice(index, index + 2) === '2f') {
      segments.push(value.slice(start, index));
      start = index + 2;
    }
  }
  segments.push(value.slice(start));
  return segments;
}

function containsHexByte(value: string, target: string): boolean {
  for (let index = 0; index < value.length; index += 2) {
    if (value.slice(index, index + 2) === target) return true;
  }
  return false;
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

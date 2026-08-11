import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModuleTreeEntryKindV1 } from './module-tree';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_U64 = BigInt('18446744073709551615');
const MAX_U32 = 4_294_967_295;
const MAX_PATH_BYTES = 131_072;
const MAX_DISPLAY_CHARACTERS = 256;
const MAX_NODES = 100;
const MAX_EDGES = 256;
const MAX_INSPECTED_EDGES = BigInt(4_096);

export interface ModuleDependencyGraphQueryV1 {
  centerModuleId: string;
  nodeLimit: number;
}

export interface QueryModuleDependencyGraphRequestV1 extends ModuleDependencyGraphQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleDependencyRelationV1 =
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

export type ModuleDependencyProviderV1 = 'treeSitter' | 'manifest' | 'languageHeuristic';

export type ModuleDependencyResolutionV1 =
  | 'adapterLocalSymbol'
  | 'adapterFile'
  | 'exactModuleReference'
  | 'uniqueFileLocalName'
  | 'uniqueQualifiedName';

export type ModuleDependencyEndpointV1 =
  { kind: 'file'; pathHex: string } | { kind: 'symbol'; symbolId: string };

export interface ModuleDependencySourcePositionV1 {
  column: number;
  row: number;
}

export interface ModuleDependencySourceRangeV1 {
  end: ModuleDependencySourcePositionV1;
  endByte: number;
  start: ModuleDependencySourcePositionV1;
  startByte: number;
}

export interface ModuleDependencyNodeEvidenceV1 {
  contentHash: string;
  evidenceId: string;
  pathHex: string;
}

export interface ModuleDependencyNodeV1 {
  kind: ModuleTreeEntryKindV1;
  moduleId: string;
  name: string;
  nameTruncated: boolean;
  representativeEvidence: ModuleDependencyNodeEvidenceV1 | null;
  rootPathHex: string | null;
}

export interface ModuleDependencyEdgeEvidenceV1 {
  confidenceBasisPoints: number;
  contentHash: string;
  evidenceId: string;
  pathHex: string;
  provider: ModuleDependencyProviderV1;
  range: ModuleDependencySourceRangeV1;
  resolution: ModuleDependencyResolutionV1;
  source: ModuleDependencyEndpointV1;
  target: ModuleDependencyEndpointV1;
}

export interface ModuleDependencyEdgeV1 {
  observedEvidenceCount: string;
  relation: ModuleDependencyRelationV1;
  representativeEvidence: ModuleDependencyEdgeEvidenceV1;
  sourceModuleId: string;
  targetModuleId: string;
}

export interface ModuleDependencyGraphV1 {
  centerModuleId: string;
  edges: ModuleDependencyEdgeV1[];
  edgesTruncated: boolean;
  indexRunId: string;
  inspectedEdgeCount: string;
  nodes: ModuleDependencyNodeV1[];
  nodesTruncated: boolean;
  observedEdgeGroupCount: string;
  observedNeighborCount: string;
  snapshotId: string;
  sourceEdgesTruncated: boolean;
  unmappedEdgeCount: string;
}

export type ModuleDependencyGraphResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'centerUnavailable' }
  | { graph: ModuleDependencyGraphV1; status: 'available' };

export interface ModuleDependencyGraphResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleDependencyGraphResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleDependencyGraph(
  query: ModuleDependencyGraphQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleDependencyGraphResponseV1> {
  if (!isQuery(query)) {
    throw new Error('Module dependency query does not match the V1 schema.');
  }
  const request: QueryModuleDependencyGraphRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_dependency_graph', { request });
  return parseModuleDependencyGraphResponseV1(payload);
}

export function parseModuleDependencyGraphResponseV1(
  payload: unknown,
): ModuleDependencyGraphResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Module dependency response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ModuleDependencyGraphResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module dependency response contains an invalid result.');
  }
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'centerUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['graph', 'status'])) {
    return { graph: parseGraph(value.graph), status: 'available' };
  }
  throw new Error('Module dependency response contains an invalid result.');
}

function parseGraph(value: unknown): ModuleDependencyGraphV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'centerModuleId',
      'edges',
      'edgesTruncated',
      'indexRunId',
      'inspectedEdgeCount',
      'nodes',
      'nodesTruncated',
      'observedEdgeGroupCount',
      'observedNeighborCount',
      'snapshotId',
      'sourceEdgesTruncated',
      'unmappedEdgeCount',
    ]) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isStableId(value.centerModuleId) ||
    !Array.isArray(value.nodes) ||
    value.nodes.length === 0 ||
    value.nodes.length > MAX_NODES ||
    !Array.isArray(value.edges) ||
    value.edges.length > MAX_EDGES ||
    !isCount(value.observedNeighborCount) ||
    typeof value.nodesTruncated !== 'boolean' ||
    !isCount(value.observedEdgeGroupCount) ||
    typeof value.edgesTruncated !== 'boolean' ||
    !isCount(value.inspectedEdgeCount) ||
    typeof value.sourceEdgesTruncated !== 'boolean' ||
    !isCount(value.unmappedEdgeCount)
  ) {
    throw new Error('Module dependency response contains an invalid graph.');
  }
  const nodes = value.nodes.map(parseNode);
  if (
    nodes.some((node, index) => index > 0 && nodes[index - 1]!.moduleId >= node.moduleId) ||
    nodes.filter((node) => node.moduleId === value.centerModuleId).length !== 1
  ) {
    throw new Error('Module dependency graph has invalid node identity or ordering.');
  }
  const visibleNeighborCount = BigInt(nodes.length - 1);
  const observedNeighborCount = BigInt(value.observedNeighborCount);
  if (
    observedNeighborCount < visibleNeighborCount ||
    value.nodesTruncated !== observedNeighborCount > visibleNeighborCount
  ) {
    throw new Error('Module dependency graph has contradictory node bounds.');
  }
  const nodeIds = new Set(nodes.map((node) => node.moduleId));
  const edges = value.edges.map(parseEdge);
  if (
    edges.some((edge, index) => {
      const key = edgeKey(edge);
      return (
        !nodeIds.has(edge.sourceModuleId) ||
        !nodeIds.has(edge.targetModuleId) ||
        (edge.sourceModuleId !== value.centerModuleId &&
          edge.targetModuleId !== value.centerModuleId) ||
        (index > 0 && edgeKey(edges[index - 1]!) >= key)
      );
    })
  ) {
    throw new Error('Module dependency graph has invalid edge endpoints or ordering.');
  }
  const observedEdgeGroupCount = BigInt(value.observedEdgeGroupCount);
  const inspectedEdgeCount = BigInt(value.inspectedEdgeCount);
  const unmappedEdgeCount = BigInt(value.unmappedEdgeCount);
  const visibleEvidenceCount = edges.reduce(
    (count, edge) => count + BigInt(edge.observedEvidenceCount),
    BigInt(0),
  );
  if (
    observedEdgeGroupCount < BigInt(edges.length) ||
    value.edgesTruncated !== observedEdgeGroupCount > BigInt(edges.length) ||
    inspectedEdgeCount > MAX_INSPECTED_EDGES ||
    observedNeighborCount > inspectedEdgeCount ||
    observedEdgeGroupCount > inspectedEdgeCount ||
    visibleEvidenceCount > inspectedEdgeCount ||
    (value.sourceEdgesTruncated && inspectedEdgeCount !== MAX_INSPECTED_EDGES) ||
    unmappedEdgeCount > inspectedEdgeCount
  ) {
    throw new Error('Module dependency graph has contradictory evidence bounds.');
  }
  return {
    centerModuleId: value.centerModuleId,
    edges,
    edgesTruncated: value.edgesTruncated,
    indexRunId: value.indexRunId,
    inspectedEdgeCount: value.inspectedEdgeCount,
    nodes,
    nodesTruncated: value.nodesTruncated,
    observedEdgeGroupCount: value.observedEdgeGroupCount,
    observedNeighborCount: value.observedNeighborCount,
    snapshotId: value.snapshotId,
    sourceEdgesTruncated: value.sourceEdgesTruncated,
    unmappedEdgeCount: value.unmappedEdgeCount,
  };
}

function parseNode(value: unknown): ModuleDependencyNodeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'kind',
      'moduleId',
      'name',
      'nameTruncated',
      'representativeEvidence',
      'rootPathHex',
    ]) ||
    !isStableId(value.moduleId) ||
    !isNodeKind(value.kind) ||
    !(value.rootPathHex === null || isRepositoryPathHex(value.rootPathHex)) ||
    typeof value.name !== 'string' ||
    value.name.length === 0 ||
    Array.from(value.name).length > MAX_DISPLAY_CHARACTERS ||
    containsControl(value.name) ||
    typeof value.nameTruncated !== 'boolean' ||
    (value.nameTruncated && Array.from(value.name).length !== MAX_DISPLAY_CHARACTERS)
  ) {
    throw new Error('Module dependency response contains an invalid node.');
  }
  return {
    kind: value.kind,
    moduleId: value.moduleId,
    name: value.name,
    nameTruncated: value.nameTruncated,
    representativeEvidence: parseOptionalNodeEvidence(value.representativeEvidence),
    rootPathHex: value.rootPathHex,
  };
}

function parseOptionalNodeEvidence(value: unknown): ModuleDependencyNodeEvidenceV1 | null {
  if (value === null) return null;
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['contentHash', 'evidenceId', 'pathHex']) ||
    !isStableId(value.contentHash) ||
    !isStableId(value.evidenceId) ||
    !isRepositoryPathHex(value.pathHex)
  ) {
    throw new Error('Module dependency response contains invalid node evidence.');
  }
  return { contentHash: value.contentHash, evidenceId: value.evidenceId, pathHex: value.pathHex };
}

function parseEdge(value: unknown): ModuleDependencyEdgeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'observedEvidenceCount',
      'relation',
      'representativeEvidence',
      'sourceModuleId',
      'targetModuleId',
    ]) ||
    !isStableId(value.sourceModuleId) ||
    !isStableId(value.targetModuleId) ||
    value.sourceModuleId === value.targetModuleId ||
    !isRelation(value.relation) ||
    !isPositiveCount(value.observedEvidenceCount)
  ) {
    throw new Error('Module dependency response contains an invalid edge.');
  }
  return {
    observedEvidenceCount: value.observedEvidenceCount,
    relation: value.relation,
    representativeEvidence: parseModuleDependencyEdgeEvidenceV1(value.representativeEvidence),
    sourceModuleId: value.sourceModuleId,
    targetModuleId: value.targetModuleId,
  };
}

export function parseModuleDependencyEdgeEvidenceV1(
  value: unknown,
): ModuleDependencyEdgeEvidenceV1 {
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
    !isConfidenceBasisPoints(value.confidenceBasisPoints)
  ) {
    throw new Error('Module dependency response contains invalid edge evidence.');
  }
  return {
    confidenceBasisPoints: value.confidenceBasisPoints,
    contentHash: value.contentHash,
    evidenceId: value.evidenceId,
    pathHex: value.pathHex,
    provider: value.provider,
    range: parseRange(value.range),
    resolution: value.resolution,
    source: parseEndpoint(value.source),
    target: parseEndpoint(value.target),
  };
}

function parseEndpoint(value: unknown): ModuleDependencyEndpointV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') {
    throw new Error('Module dependency response contains an invalid edge endpoint.');
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
  throw new Error('Module dependency response contains an invalid edge endpoint.');
}

function parseRange(value: unknown): ModuleDependencySourceRangeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['end', 'endByte', 'start', 'startByte']) ||
    !isU32(value.startByte) ||
    !isU32(value.endByte) ||
    value.startByte > value.endByte
  ) {
    throw new Error('Module dependency response contains an invalid source range.');
  }
  const start = parsePosition(value.start);
  const end = parsePosition(value.end);
  if (start.row > end.row || (start.row === end.row && start.column > end.column)) {
    throw new Error('Module dependency response contains an inverted source range.');
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
    throw new Error('Module dependency response contains an invalid source position.');
  }
  return { column: value.column, row: value.row };
}

const RELATIONS: ModuleDependencyRelationV1[] = [
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
];

function edgeKey(edge: ModuleDependencyEdgeV1): string {
  const relationIndex = RELATIONS.indexOf(edge.relation).toString().padStart(2, '0');
  return `${edge.sourceModuleId}:${edge.targetModuleId}:${relationIndex}`;
}

function isQuery(value: ModuleDependencyGraphQueryV1): boolean {
  return (
    hasExactKeys(value as unknown as Record<string, unknown>, ['centerModuleId', 'nodeLimit']) &&
    isStableId(value.centerModuleId) &&
    Number.isInteger(value.nodeLimit) &&
    value.nodeLimit >= 1 &&
    value.nodeLimit <= MAX_NODES
  );
}

function isNodeKind(value: unknown): value is ModuleTreeEntryKindV1 {
  return value === 'manifestBoundary' || value === 'pathBoundary';
}

function isRelation(value: unknown): value is ModuleDependencyRelationV1 {
  return typeof value === 'string' && RELATIONS.includes(value as ModuleDependencyRelationV1);
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

function isCount(value: unknown): value is string {
  return typeof value === 'string' && COUNT_PATTERN.test(value) && BigInt(value) <= MAX_U64;
}

function isPositiveCount(value: unknown): value is string {
  return isCount(value) && value !== '0';
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isU32(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= MAX_U32;
}

function isConfidenceBasisPoints(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= 10_000;
}

export function isRepositoryPathHex(value: unknown): value is string {
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

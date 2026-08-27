import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const ID = /^[0-9a-f]{64}$/;
const COUNT = /^(0|[1-9][0-9]*)$/;
const CURSOR = /^[0-9a-f]{80}$/;
const MAX_TEXT = 4_096;

export type ProjectMapEntitySelectionV1 =
  | { kind: 'module'; moduleId: string }
  | { evidenceId: string; kind: 'file'; moduleId: string; ordinal: number }
  | { evidenceId: string; kind: 'symbol'; moduleId: string; symbolId: string };

export type ProjectMapIndexEvidenceSelectionV1 =
  | { evidenceId: string; kind: 'file'; moduleId: string; ordinal: number }
  | { evidenceId: string; kind: 'symbol'; moduleId: string; symbolId: string }
  | { edgeSequence: string; evidenceId: string; kind: 'relation'; moduleId: string }
  | {
      candidateSequence: string;
      evidenceId: string;
      kind: 'unresolvedRelation';
      moduleId: string;
    };

export type ProjectMapAtlasLevelV1 = 'project' | 'module' | 'file' | 'symbol';
export type ProjectMapAtlasNodeKindV1 =
  | 'manifestModule'
  | 'pathModule'
  | 'file'
  | 'namespace'
  | 'type'
  | 'callable'
  | 'member'
  | 'boundary';
export type ProjectMapRelationKindV1 =
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
export type ProjectMapFlowPresetV1 = 'callers' | 'callees' | 'tests' | 'dataAccess';
export type ProjectMapInventoryViewV1 = 'files' | 'symbols' | 'members';
export type ProjectMapMappingStatusV1 = 'current' | 'stale' | 'needsReview' | 'unmapped';

export interface ProjectMapAtlasNodeV1 {
  claimBadgeCount: number;
  currentRiskCount: string;
  detail: string | null;
  dimmed: boolean;
  displayName: string;
  evidenceId: string | null;
  fileCount: string;
  kind: ProjectMapAtlasNodeKindV1;
  mappingStatus: ProjectMapMappingStatusV1 | null;
  memberCount: string;
  nodeId: string;
  parentNodeId: string | null;
  purpose: string | null;
  rank: number;
  selection: ProjectMapEntitySelectionV1 | null;
  symbolCount: string;
  volume: string;
}

export interface ProjectMapAtlasRelationV1 {
  claimBadgeCount: number;
  confidenceBasisPoints: number;
  evidence: ProjectMapIndexEvidenceSelectionV1 | null;
  evidenceCount: string;
  provider: 'treeSitter' | 'manifest' | 'languageHeuristic';
  relation: ProjectMapRelationKindV1;
  sourceNodeId: string;
  targetNodeId: string;
  uncertainty:
    | 'external'
    | 'noDeterministicMatch'
    | 'ambiguousMatch'
    | 'dynamicReference'
    | 'missingFile'
    | null;
}

export interface ProjectMapAtlasBreadcrumbV1 {
  label: string;
  selection: ProjectMapEntitySelectionV1 | null;
}

export interface ProjectMapAtlasSceneV1 {
  boundariesTruncated: boolean;
  boundaryCount: string;
  breadcrumb: ProjectMapAtlasBreadcrumbV1[];
  indexRunId: string;
  inspectedEdgeCount: string;
  level: ProjectMapAtlasLevelV1;
  nodeCount: string;
  nodes: ProjectMapAtlasNodeV1[];
  nodesTruncated: boolean;
  policyVersion: 1;
  relationCount: string;
  relations: ProjectMapAtlasRelationV1[];
  relationsTruncated: boolean;
  selection: ProjectMapEntitySelectionV1 | null;
  snapshotId: string;
  sourceEdgesTruncated: boolean;
  unresolvedCount: string;
}

export interface ProjectMapEntityContextV1 {
  architectureRelationCount: string;
  architectureRelations: ProjectMapAtlasRelationV1[];
  boundaryCount: string;
  boundaryNodes: ProjectMapAtlasNodeV1[];
  boundaryRelations: ProjectMapAtlasRelationV1[];
  claims: { cardId: string; claimId: string; confidenceBasisPoints: number }[];
  documentRelationCount: string;
  entity: ProjectMapAtlasNodeV1;
  indexRunId: string;
  relatedNodes: ProjectMapAtlasNodeV1[];
  relationCounts: { incoming: string; outgoing: string; relation: ProjectMapRelationKindV1 }[];
  snapshotId: string;
  sourceEdgesTruncated: boolean;
}

export interface ProjectMapInventoryPageV1 {
  indexRunId: string;
  items: ProjectMapAtlasNodeV1[];
  nextCursor: string | null;
  pageNumber: number;
  pageSize: 50;
  previousCursor: string | null;
  selection: ProjectMapEntitySelectionV1;
  snapshotId: string;
  totalCount: string;
  view: ProjectMapInventoryViewV1;
}

export interface ProjectMapFlowSceneV1 {
  indexRunId: string;
  inspectedEdgeCount: string;
  nodes: ProjectMapAtlasNodeV1[];
  preset: ProjectMapFlowPresetV1;
  root: ProjectMapAtlasNodeV1;
  snapshotId: string;
  sourceEdgesTruncated: boolean;
  targetCount: string;
  targets: {
    depth: number;
    nodeId: string;
    path: {
      evidence: ProjectMapIndexEvidenceSelectionV1;
      relation: ProjectMapRelationKindV1;
      sourceNodeId: string;
      targetNodeId: string;
    }[];
  }[];
  targetsTruncated: boolean;
}

type Availability<T, K extends string> =
  | { status: 'noProject' | 'noPublishedIndex' | 'projectionUnavailable' | 'selectionChanged' }
  | ({ status: 'available' } & Record<K, T>);
export interface AtlasResponse<T, K extends string> {
  protocolVersion: 1;
  result: Availability<T, K>;
}
export type ProjectMapAtlasSceneResponseV1 = AtlasResponse<ProjectMapAtlasSceneV1, 'scene'>;
export type ProjectMapEntityContextResponseV1 = AtlasResponse<ProjectMapEntityContextV1, 'context'>;
export type ProjectMapInventoryPageResponseV1 = AtlasResponse<ProjectMapInventoryPageV1, 'page'>;
export type ProjectMapFlowSceneResponseV1 = AtlasResponse<ProjectMapFlowSceneV1, 'flow'>;

const invokeTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectMapAtlasScene(
  selection: ProjectMapEntitySelectionV1 | null,
  invoke: InvokeCommand = invokeTauri,
): Promise<ProjectMapAtlasSceneResponseV1> {
  if (selection !== null) parseSelection(selection);
  return parseAtlasSceneResponse(
    await invoke('query_project_map_atlas_scene', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, selection },
    }),
  );
}

export async function queryProjectMapEntityContext(
  selection: ProjectMapEntitySelectionV1,
  invoke: InvokeCommand = invokeTauri,
): Promise<ProjectMapEntityContextResponseV1> {
  parseSelection(selection);
  return parseEntityContextResponse(
    await invoke('query_project_map_entity_context', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, selection },
    }),
  );
}

export async function queryProjectMapInventoryPage(
  selection: ProjectMapEntitySelectionV1,
  view: ProjectMapInventoryViewV1,
  cursor: string | null,
  invoke: InvokeCommand = invokeTauri,
): Promise<ProjectMapInventoryPageResponseV1> {
  parseSelection(selection);
  if (!['files', 'symbols', 'members'].includes(view) || (cursor !== null && !CURSOR.test(cursor)))
    throw new Error('Invalid Atlas inventory query.');
  return parseInventoryResponse(
    await invoke('query_project_map_inventory_page', {
      request: { cursor, protocolVersion: CURRENT_PROTOCOL_VERSION, selection, view },
    }),
  );
}

export async function queryProjectMapFlowScene(
  selection: ProjectMapEntitySelectionV1,
  preset: ProjectMapFlowPresetV1,
  invoke: InvokeCommand = invokeTauri,
): Promise<ProjectMapFlowSceneResponseV1> {
  parseSelection(selection);
  if (!['callers', 'callees', 'tests', 'dataAccess'].includes(preset))
    throw new Error('Invalid Atlas flow query.');
  return parseFlowResponse(
    await invoke('query_project_map_flow_scene', {
      request: { preset, protocolVersion: CURRENT_PROTOCOL_VERSION, selection },
    }),
  );
}

export function parseAtlasSceneResponse(value: unknown): ProjectMapAtlasSceneResponseV1 {
  return parseResponse(value, 'scene', parseScene);
}
export function parseEntityContextResponse(value: unknown): ProjectMapEntityContextResponseV1 {
  return parseResponse(value, 'context', parseContext);
}
export function parseInventoryResponse(value: unknown): ProjectMapInventoryPageResponseV1 {
  return parseResponse(value, 'page', parseInventory);
}
export function parseFlowResponse(value: unknown): ProjectMapFlowSceneResponseV1 {
  return parseResponse(value, 'flow', parseFlow);
}

function parseResponse<T, K extends string>(
  value: unknown,
  field: K,
  parser: (payload: unknown) => T,
): AtlasResponse<T, K> {
  const record = exact(value, ['protocolVersion', 'result']);
  if (record.protocolVersion !== CURRENT_PROTOCOL_VERSION) invalid();
  const result = record.result;
  if (!isRecord(result) || typeof result.status !== 'string') invalid();
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'selectionChanged',
  ] as const) {
    if (result.status === status && hasKeys(result, ['status']))
      return { protocolVersion: 1, result: { status } };
  }
  if (result.status !== 'available' || !hasKeys(result, [field, 'status'])) invalid();
  return {
    protocolVersion: 1,
    result: { [field]: parser(result[field]), status: 'available' } as Availability<T, K>,
  };
}

function parseScene(value: unknown): ProjectMapAtlasSceneV1 {
  const r = exact(value, [
    'boundariesTruncated',
    'boundaryCount',
    'breadcrumb',
    'indexRunId',
    'inspectedEdgeCount',
    'level',
    'nodeCount',
    'nodes',
    'nodesTruncated',
    'policyVersion',
    'relationCount',
    'relations',
    'relationsTruncated',
    'selection',
    'snapshotId',
    'sourceEdgesTruncated',
    'unresolvedCount',
  ]);
  const level = oneOf(r.level, ['project', 'module', 'file', 'symbol'] as const);
  const nodeLimit = { project: 80, module: 48, file: 64, symbol: 48 }[level];
  const nodes = array(r.nodes, nodeLimit, parseNode);
  const relations = array(r.relations, 128, parseRelation);
  const breadcrumb = array(r.breadcrumb, 4, (entry) => {
    const b = exact(entry, ['label', 'selection']);
    return { label: text(b.label, 1_024), selection: nullable(b.selection, parseSelection) };
  });
  const ids = new Set(nodes.map((node) => node.nodeId));
  if (ids.size !== nodes.length || nodes.some((node, index) => node.rank !== index + 1)) invalid();
  if (relations.some((edge) => !ids.has(edge.sourceNodeId) || !ids.has(edge.targetNodeId)))
    invalid();
  const boundaryRendered = nodes.filter((node) => node.kind === 'boundary').length;
  const entityRendered = nodes.length - boundaryRendered;
  const nodeCount = count(r.nodeCount);
  const boundaryCount = count(r.boundaryCount);
  const relationCount = count(r.relationCount);
  const unresolvedCount = count(r.unresolvedCount);
  if (
    nodeCount < BigInt(entityRendered) ||
    boundaryCount < BigInt(boundaryRendered) ||
    relationCount < BigInt(relations.length) ||
    unresolvedCount > boundaryCount ||
    Boolean(r.nodesTruncated) !== nodeCount > BigInt(entityRendered) ||
    Boolean(r.boundariesTruncated) !== boundaryCount > BigInt(boundaryRendered) ||
    Boolean(r.relationsTruncated) !== relationCount > BigInt(relations.length) ||
    breadcrumb.length === 0 ||
    (level === 'project') !== (r.selection === null) ||
    r.policyVersion !== 1
  )
    invalid();
  bools(r, ['boundariesTruncated', 'nodesTruncated', 'relationsTruncated', 'sourceEdgesTruncated']);
  return {
    boundariesTruncated: r.boundariesTruncated as boolean,
    boundaryCount: r.boundaryCount as string,
    breadcrumb,
    indexRunId: stableId(r.indexRunId),
    inspectedEdgeCount: countString(r.inspectedEdgeCount),
    level,
    nodeCount: r.nodeCount as string,
    nodes,
    nodesTruncated: r.nodesTruncated as boolean,
    policyVersion: 1,
    relationCount: r.relationCount as string,
    relations,
    relationsTruncated: r.relationsTruncated as boolean,
    selection: nullable(r.selection, parseSelection),
    snapshotId: stableId(r.snapshotId),
    sourceEdgesTruncated: r.sourceEdgesTruncated as boolean,
    unresolvedCount: r.unresolvedCount as string,
  };
}

function parseContext(value: unknown): ProjectMapEntityContextV1 {
  const r = exact(value, [
    'architectureRelationCount',
    'architectureRelations',
    'boundaryCount',
    'boundaryNodes',
    'boundaryRelations',
    'claims',
    'documentRelationCount',
    'entity',
    'indexRunId',
    'relatedNodes',
    'relationCounts',
    'snapshotId',
    'sourceEdgesTruncated',
  ]);
  const relatedNodes = array(r.relatedNodes, 32, parseNode);
  const architectureRelations = array(r.architectureRelations, 32, parseRelation);
  const boundaryNodes = array(r.boundaryNodes, 16, parseNode);
  const boundaryRelations = array(r.boundaryRelations, 16, parseRelation);
  const relationCounts = array(r.relationCounts, 13, (value) => {
    const item = exact(value, ['incoming', 'outgoing', 'relation']);
    return {
      incoming: countString(item.incoming),
      outgoing: countString(item.outgoing),
      relation: relation(item.relation),
    };
  });
  const claims = array(r.claims, 64, (value) => {
    const claim = exact(value, ['cardId', 'claimId', 'confidenceBasisPoints']);
    return {
      cardId: stableId(claim.cardId),
      claimId: stableId(claim.claimId),
      confidenceBasisPoints: integer(claim.confidenceBasisPoints, 0, 10_000),
    };
  });
  if (
    count(r.architectureRelationCount) < BigInt(architectureRelations.length) ||
    count(r.boundaryCount) < BigInt(boundaryNodes.length)
  )
    invalid();
  if (typeof r.sourceEdgesTruncated !== 'boolean') invalid();
  return {
    architectureRelationCount: r.architectureRelationCount as string,
    architectureRelations,
    boundaryCount: r.boundaryCount as string,
    boundaryNodes,
    boundaryRelations,
    claims,
    documentRelationCount: countString(r.documentRelationCount),
    entity: parseNode(r.entity),
    indexRunId: stableId(r.indexRunId),
    relatedNodes,
    relationCounts,
    snapshotId: stableId(r.snapshotId),
    sourceEdgesTruncated: r.sourceEdgesTruncated,
  };
}

function parseInventory(value: unknown): ProjectMapInventoryPageV1 {
  const r = exact(value, [
    'indexRunId',
    'items',
    'nextCursor',
    'pageNumber',
    'pageSize',
    'previousCursor',
    'selection',
    'snapshotId',
    'totalCount',
    'view',
  ]);
  const items = array(r.items, 50, parseNode);
  if (r.pageSize !== 50 || count(r.totalCount) < BigInt(items.length)) invalid();
  return {
    indexRunId: stableId(r.indexRunId),
    items,
    nextCursor: cursor(r.nextCursor),
    pageNumber: integer(r.pageNumber, 1, 4_294_967_295),
    pageSize: 50,
    previousCursor: cursor(r.previousCursor),
    selection: parseSelection(r.selection),
    snapshotId: stableId(r.snapshotId),
    totalCount: r.totalCount as string,
    view: oneOf(r.view, ['files', 'symbols', 'members'] as const),
  };
}

function parseFlow(value: unknown): ProjectMapFlowSceneV1 {
  const r = exact(value, [
    'indexRunId',
    'inspectedEdgeCount',
    'nodes',
    'preset',
    'root',
    'snapshotId',
    'sourceEdgesTruncated',
    'targetCount',
    'targets',
    'targetsTruncated',
  ]);
  const nodes = array(r.nodes, 31, parseNode);
  const ids = new Set(nodes.map((node) => node.nodeId));
  const targets = array(r.targets, 31, (value) => {
    const target = exact(value, ['depth', 'nodeId', 'path']);
    const path = array(target.path, 2, (value) => {
      const step = exact(value, ['evidence', 'relation', 'sourceNodeId', 'targetNodeId']);
      return {
        evidence: parseIndexEvidence(step.evidence),
        relation: relation(step.relation),
        sourceNodeId: stableId(step.sourceNodeId),
        targetNodeId: stableId(step.targetNodeId),
      };
    });
    const depth = integer(target.depth, 1, 2);
    if (path.length !== depth || !ids.has(stableId(target.nodeId))) invalid();
    return { depth, nodeId: target.nodeId as string, path };
  });
  if (
    typeof r.targetsTruncated !== 'boolean' ||
    typeof r.sourceEdgesTruncated !== 'boolean' ||
    count(r.targetCount) < BigInt(targets.length)
  )
    invalid();
  return {
    indexRunId: stableId(r.indexRunId),
    inspectedEdgeCount: countString(r.inspectedEdgeCount),
    nodes,
    preset: oneOf(r.preset, ['callers', 'callees', 'tests', 'dataAccess'] as const),
    root: parseNode(r.root),
    snapshotId: stableId(r.snapshotId),
    sourceEdgesTruncated: r.sourceEdgesTruncated,
    targetCount: r.targetCount as string,
    targets,
    targetsTruncated: r.targetsTruncated,
  };
}

function parseNode(value: unknown): ProjectMapAtlasNodeV1 {
  const r = exact(value, [
    'claimBadgeCount',
    'currentRiskCount',
    'detail',
    'dimmed',
    'displayName',
    'evidenceId',
    'fileCount',
    'kind',
    'mappingStatus',
    'memberCount',
    'nodeId',
    'parentNodeId',
    'purpose',
    'rank',
    'selection',
    'symbolCount',
    'volume',
  ]);
  if (typeof r.dimmed !== 'boolean') invalid();
  return {
    claimBadgeCount: integer(r.claimBadgeCount, 0, 65_535),
    currentRiskCount: countString(r.currentRiskCount),
    detail: nullable(r.detail, (v) => text(v, MAX_TEXT)),
    dimmed: r.dimmed,
    displayName: text(r.displayName, 1_024),
    evidenceId: nullable(r.evidenceId, stableId),
    fileCount: countString(r.fileCount),
    kind: oneOf(r.kind, [
      'manifestModule',
      'pathModule',
      'file',
      'namespace',
      'type',
      'callable',
      'member',
      'boundary',
    ] as const),
    mappingStatus: nullable(r.mappingStatus, (v) =>
      oneOf(v, ['current', 'stale', 'needsReview', 'unmapped'] as const),
    ),
    memberCount: countString(r.memberCount),
    nodeId: stableId(r.nodeId),
    parentNodeId: nullable(r.parentNodeId, stableId),
    purpose: nullable(r.purpose, (v) => text(v, 160)),
    rank: integer(r.rank, 1, 65_535),
    selection: nullable(r.selection, parseSelection),
    symbolCount: countString(r.symbolCount),
    volume: positiveCountString(r.volume),
  };
}

function parseRelation(value: unknown): ProjectMapAtlasRelationV1 {
  const r = exact(value, [
    'claimBadgeCount',
    'confidenceBasisPoints',
    'evidence',
    'evidenceCount',
    'provider',
    'relation',
    'sourceNodeId',
    'targetNodeId',
    'uncertainty',
  ]);
  return {
    claimBadgeCount: integer(r.claimBadgeCount, 0, 65_535),
    confidenceBasisPoints: integer(r.confidenceBasisPoints, 0, 10_000),
    evidence: nullable(r.evidence, parseIndexEvidence),
    evidenceCount: positiveCountString(r.evidenceCount),
    provider: oneOf(r.provider, ['treeSitter', 'manifest', 'languageHeuristic'] as const),
    relation: relation(r.relation),
    sourceNodeId: stableId(r.sourceNodeId),
    targetNodeId: stableId(r.targetNodeId),
    uncertainty: nullable(r.uncertainty, (v) =>
      oneOf(v, [
        'external',
        'noDeterministicMatch',
        'ambiguousMatch',
        'dynamicReference',
        'missingFile',
      ] as const),
    ),
  };
}

function parseSelection(value: unknown): ProjectMapEntitySelectionV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') invalid();
  if (value.kind === 'module' && hasKeys(value, ['kind', 'moduleId']))
    return { kind: 'module', moduleId: stableId(value.moduleId) };
  if (value.kind === 'file' && hasKeys(value, ['evidenceId', 'kind', 'moduleId', 'ordinal']))
    return {
      evidenceId: stableId(value.evidenceId),
      kind: 'file',
      moduleId: stableId(value.moduleId),
      ordinal: integer(value.ordinal, 1, 250_000),
    };
  if (value.kind === 'symbol' && hasKeys(value, ['evidenceId', 'kind', 'moduleId', 'symbolId']))
    return {
      evidenceId: stableId(value.evidenceId),
      kind: 'symbol',
      moduleId: stableId(value.moduleId),
      symbolId: stableId(value.symbolId),
    };
  return invalid();
}

export function parseIndexEvidence(value: unknown): ProjectMapIndexEvidenceSelectionV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') invalid();
  if (value.kind === 'file')
    return parseSelection(value) as Extract<ProjectMapIndexEvidenceSelectionV1, { kind: 'file' }>;
  if (value.kind === 'symbol')
    return parseSelection(value) as Extract<ProjectMapIndexEvidenceSelectionV1, { kind: 'symbol' }>;
  if (
    value.kind === 'relation' &&
    hasKeys(value, ['edgeSequence', 'evidenceId', 'kind', 'moduleId'])
  )
    return {
      edgeSequence: positiveCountString(value.edgeSequence),
      evidenceId: stableId(value.evidenceId),
      kind: 'relation',
      moduleId: stableId(value.moduleId),
    };
  if (
    value.kind === 'unresolvedRelation' &&
    hasKeys(value, ['candidateSequence', 'evidenceId', 'kind', 'moduleId'])
  )
    return {
      candidateSequence: positiveCountString(value.candidateSequence),
      evidenceId: stableId(value.evidenceId),
      kind: 'unresolvedRelation',
      moduleId: stableId(value.moduleId),
    };
  return invalid();
}

function relation(value: unknown): ProjectMapRelationKindV1 {
  return oneOf(value, [
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
  ] as const);
}
function exact(value: unknown, keys: string[]): Record<string, unknown> {
  if (!isRecord(value) || !hasKeys(value, keys)) invalid();
  return value;
}
function hasKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}
function array<T>(value: unknown, maximum: number, parser: (item: unknown) => T): T[] {
  if (!Array.isArray(value) || value.length > maximum) invalid();
  return value.map(parser);
}
function nullable<T>(value: unknown, parser: (item: unknown) => T): T | null {
  return value === null ? null : parser(value);
}
function stableId(value: unknown): string {
  if (typeof value !== 'string' || !ID.test(value)) invalid();
  return value;
}
function count(value: unknown): bigint {
  if (typeof value !== 'string' || !COUNT.test(value)) invalid();
  return BigInt(value);
}
function countString(value: unknown): string {
  count(value);
  return value as string;
}
function positiveCountString(value: unknown): string {
  if (count(value) === 0n) invalid();
  return value as string;
}
function cursor(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value !== 'string' || !CURSOR.test(value)) invalid();
  return value;
}
function text(value: unknown, maximum: number): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > maximum ||
    containsControl(value)
  )
    invalid();
  return value;
}
function containsControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const point = character.codePointAt(0);
    return (
      point !== undefined &&
      ((point <= 0x1f && point !== 0x09 && point !== 0x0a && point !== 0x0d) ||
        (point >= 0x7f && point <= 0x9f))
    );
  });
}
function integer(value: unknown, minimum: number, maximum: number): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < minimum || value > maximum)
    invalid();
  return value;
}
function oneOf<T extends readonly string[]>(value: unknown, values: T): T[number] {
  if (typeof value !== 'string' || !values.includes(value)) invalid();
  return value as T[number];
}
function bools(value: Record<string, unknown>, keys: string[]): void {
  if (keys.some((key) => typeof value[key] !== 'boolean')) invalid();
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function invalid(): never {
  throw new Error('Progressive Project Map Atlas payload is invalid.');
}

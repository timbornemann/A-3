import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModuleDependencyRelationV1 } from './module-dependency-graph';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const MAX_U64 = BigInt('18446744073709551615');
const MAX_OVERVIEW_MODULES = 64;
const MAX_FOCUS_MODULES = 32;
const MAX_RELATIONS = 128;
const MAX_INSPECTED_EDGES = BigInt(4_096);
const MAX_DISPLAY_CHARACTERS = 256;

export interface ProjectMapSceneQueryV1 {
  focusModuleId: string | null;
}

export interface QueryProjectMapSceneRequestV1 extends ProjectMapSceneQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ProjectMapMappingStatusV1 = 'current' | 'stale' | 'needsReview' | 'unmapped';
export type ProjectMapSceneModuleKindV1 = 'manifestBoundary' | 'pathBoundary';

export interface ProjectMapSceneCardBindingV1 {
  cardId: string;
  sourceIndexRunId: string;
  sourceSnapshotId: string;
}

export interface ProjectMapSceneModuleV1 {
  cardBinding: ProjectMapSceneCardBindingV1 | null;
  cardCoverageBasisPoints: number | null;
  centralSymbolCount: string;
  displayName: string;
  entrypointCount: string;
  fileCount: string;
  kind: ProjectMapSceneModuleKindV1;
  manifestCount: string;
  mappingStatus: ProjectMapMappingStatusV1;
  moduleId: string;
  parentModuleId: string | null;
  rank: number;
  representativeEvidenceId: string | null;
  symbolCount: string;
  testCount: string;
}

export interface ProjectMapSceneRelationV1 {
  evidenceId: string | null;
  observedEvidenceCount: string;
  relation: ModuleDependencyRelationV1;
  sourceModuleId: string;
  targetModuleId: string;
}

export interface ProjectMapSceneV1 {
  focusModuleId: string | null;
  indexRunId: string;
  inspectedEdgeCount: string;
  modules: ProjectMapSceneModuleV1[];
  modulesTruncated: boolean;
  observedRelationGroupCount: string;
  policyVersion: 'v1';
  primaryModuleCount: string;
  relations: ProjectMapSceneRelationV1[];
  relationsTruncated: boolean;
  snapshotId: string;
  sourceEdgesTruncated: boolean;
  unmappedEdgeCount: string;
}

export type ProjectMapSceneResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'focusUnavailable' }
  | { scene: ProjectMapSceneV1; status: 'available' };

export interface ProjectMapSceneResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectMapSceneResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectMapScene(
  query: ProjectMapSceneQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectMapSceneResponseV1> {
  if (!(query.focusModuleId === null || isStableId(query.focusModuleId))) {
    throw new Error('Project Map scene query does not match V1.');
  }
  const request: QueryProjectMapSceneRequestV1 = {
    focusModuleId: query.focusModuleId,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_project_map_scene', { request });
  const response = parseProjectMapSceneResponseV1(payload);
  if (
    response.result.status === 'available' &&
    response.result.scene.focusModuleId !== query.focusModuleId
  ) {
    throw new Error('Project Map scene response does not match its focus.');
  }
  return response;
}

export function parseProjectMapSceneResponseV1(payload: unknown): ProjectMapSceneResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Project Map scene response does not match V1.');
  }
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ProjectMapSceneResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'focusUnavailable',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['scene', 'status'])) {
    return { scene: parseScene(value.scene), status: 'available' };
  }
  return invalidResult();
}

function invalidResult(): never {
  throw new Error('Project Map scene response contains an invalid result.');
}

function parseScene(value: unknown): ProjectMapSceneV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'focusModuleId',
      'indexRunId',
      'inspectedEdgeCount',
      'modules',
      'modulesTruncated',
      'observedRelationGroupCount',
      'policyVersion',
      'primaryModuleCount',
      'relations',
      'relationsTruncated',
      'snapshotId',
      'sourceEdgesTruncated',
      'unmappedEdgeCount',
    ]) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !(value.focusModuleId === null || isStableId(value.focusModuleId)) ||
    value.policyVersion !== 'v1' ||
    !isCount(value.primaryModuleCount) ||
    !Array.isArray(value.modules) ||
    value.modules.length >
      (value.focusModuleId === null ? MAX_OVERVIEW_MODULES : MAX_FOCUS_MODULES) ||
    typeof value.modulesTruncated !== 'boolean' ||
    !isCount(value.observedRelationGroupCount) ||
    !Array.isArray(value.relations) ||
    value.relations.length > MAX_RELATIONS ||
    typeof value.relationsTruncated !== 'boolean' ||
    !isCount(value.inspectedEdgeCount) ||
    !isCount(value.unmappedEdgeCount) ||
    typeof value.sourceEdgesTruncated !== 'boolean'
  ) {
    throw new Error('Project Map scene response contains an invalid scene.');
  }
  const modules = value.modules.map(parseModule);
  const moduleIds = new Set(modules.map((module) => module.moduleId));
  const primaryCount = BigInt(value.primaryModuleCount);
  if (
    moduleIds.size !== modules.length ||
    modules.some((module, index) => module.rank !== index + 1) ||
    modules.some(
      (module) => module.parentModuleId !== null && !moduleIds.has(module.parentModuleId),
    ) ||
    primaryCount < BigInt(modules.length) ||
    value.modulesTruncated !== primaryCount > BigInt(modules.length) ||
    (value.focusModuleId !== null && !moduleIds.has(value.focusModuleId))
  ) {
    throw new Error('Project Map scene contains contradictory module bounds.');
  }
  const relations = value.relations.map(parseRelation);
  const relationKeys = new Set(
    relations.map(
      (relation) => `${relation.sourceModuleId}:${relation.targetModuleId}:${relation.relation}`,
    ),
  );
  const observedGroups = BigInt(value.observedRelationGroupCount);
  const inspectedEdges = BigInt(value.inspectedEdgeCount);
  const unmappedEdges = BigInt(value.unmappedEdgeCount);
  if (
    relationKeys.size !== relations.length ||
    relations.some(
      (relation) =>
        !moduleIds.has(relation.sourceModuleId) || !moduleIds.has(relation.targetModuleId),
    ) ||
    observedGroups < BigInt(relations.length) ||
    value.relationsTruncated !== observedGroups > BigInt(relations.length) ||
    inspectedEdges > MAX_INSPECTED_EDGES ||
    unmappedEdges > inspectedEdges ||
    (value.sourceEdgesTruncated && inspectedEdges !== MAX_INSPECTED_EDGES)
  ) {
    throw new Error('Project Map scene contains contradictory relation bounds.');
  }
  return {
    focusModuleId: value.focusModuleId,
    indexRunId: value.indexRunId,
    inspectedEdgeCount: value.inspectedEdgeCount,
    modules,
    modulesTruncated: value.modulesTruncated,
    observedRelationGroupCount: value.observedRelationGroupCount,
    policyVersion: 'v1',
    primaryModuleCount: value.primaryModuleCount,
    relations,
    relationsTruncated: value.relationsTruncated,
    snapshotId: value.snapshotId,
    sourceEdgesTruncated: value.sourceEdgesTruncated,
    unmappedEdgeCount: value.unmappedEdgeCount,
  };
}

function parseModule(value: unknown): ProjectMapSceneModuleV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'cardBinding',
      'cardCoverageBasisPoints',
      'centralSymbolCount',
      'displayName',
      'entrypointCount',
      'fileCount',
      'kind',
      'manifestCount',
      'mappingStatus',
      'moduleId',
      'parentModuleId',
      'rank',
      'representativeEvidenceId',
      'symbolCount',
      'testCount',
    ]) ||
    !isStableId(value.moduleId) ||
    !(value.parentModuleId === null || isStableId(value.parentModuleId)) ||
    (value.kind !== 'manifestBoundary' && value.kind !== 'pathBoundary') ||
    typeof value.displayName !== 'string' ||
    value.displayName.length === 0 ||
    Array.from(value.displayName).length > MAX_DISPLAY_CHARACTERS ||
    containsControl(value.displayName) ||
    !Number.isInteger(value.rank) ||
    typeof value.rank !== 'number' ||
    value.rank < 1 ||
    !isCount(value.manifestCount) ||
    !isCount(value.fileCount) ||
    !isCount(value.symbolCount) ||
    !isCount(value.centralSymbolCount) ||
    !isCount(value.entrypointCount) ||
    !isCount(value.testCount) ||
    !isMappingStatus(value.mappingStatus) ||
    !(value.cardCoverageBasisPoints === null || isBasisPoints(value.cardCoverageBasisPoints)) ||
    !(value.representativeEvidenceId === null || isStableId(value.representativeEvidenceId))
  ) {
    throw new Error('Project Map scene contains an invalid module.');
  }
  const fileCount = BigInt(value.fileCount);
  const symbolCount = BigInt(value.symbolCount);
  if (
    fileCount > symbolCount ||
    BigInt(value.centralSymbolCount) > symbolCount ||
    BigInt(value.entrypointCount) > symbolCount ||
    BigInt(value.testCount) > symbolCount
  ) {
    throw new Error('Project Map scene module counts are contradictory.');
  }
  const cardBinding = value.cardBinding === null ? null : parseCardBinding(value.cardBinding);
  if (
    (value.mappingStatus === 'unmapped') !== (cardBinding === null) ||
    (value.cardCoverageBasisPoints === null) !== (cardBinding === null)
  ) {
    throw new Error('Project Map scene module has contradictory mapping state.');
  }
  return {
    cardBinding,
    cardCoverageBasisPoints: value.cardCoverageBasisPoints,
    centralSymbolCount: value.centralSymbolCount,
    displayName: value.displayName,
    entrypointCount: value.entrypointCount,
    fileCount: value.fileCount,
    kind: value.kind,
    manifestCount: value.manifestCount,
    mappingStatus: value.mappingStatus,
    moduleId: value.moduleId,
    parentModuleId: value.parentModuleId,
    rank: value.rank,
    representativeEvidenceId: value.representativeEvidenceId,
    symbolCount: value.symbolCount,
    testCount: value.testCount,
  };
}

function parseCardBinding(value: unknown): ProjectMapSceneCardBindingV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['cardId', 'sourceIndexRunId', 'sourceSnapshotId']) ||
    !isStableId(value.cardId) ||
    !isStableId(value.sourceIndexRunId) ||
    !isStableId(value.sourceSnapshotId)
  ) {
    throw new Error('Project Map scene contains an invalid Card binding.');
  }
  return {
    cardId: value.cardId,
    sourceIndexRunId: value.sourceIndexRunId,
    sourceSnapshotId: value.sourceSnapshotId,
  };
}

function parseRelation(value: unknown): ProjectMapSceneRelationV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'evidenceId',
      'observedEvidenceCount',
      'relation',
      'sourceModuleId',
      'targetModuleId',
    ]) ||
    !isStableId(value.sourceModuleId) ||
    !isStableId(value.targetModuleId) ||
    value.sourceModuleId === value.targetModuleId ||
    !isRelation(value.relation) ||
    !isPositiveCount(value.observedEvidenceCount) ||
    !(value.evidenceId === null || isStableId(value.evidenceId))
  ) {
    throw new Error('Project Map scene contains an invalid relation.');
  }
  return {
    evidenceId: value.evidenceId,
    observedEvidenceCount: value.observedEvidenceCount,
    relation: value.relation,
    sourceModuleId: value.sourceModuleId,
    targetModuleId: value.targetModuleId,
  };
}

function isRelation(value: unknown): value is ModuleDependencyRelationV1 {
  return (
    typeof value === 'string' &&
    [
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

function isMappingStatus(value: unknown): value is ProjectMapMappingStatusV1 {
  return (
    value === 'current' || value === 'stale' || value === 'needsReview' || value === 'unmapped'
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

function isBasisPoints(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 10_000;
}

function containsControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const point = character.codePointAt(0);
    return point !== undefined && (point <= 0x1f || (point >= 0x7f && point <= 0x9f));
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

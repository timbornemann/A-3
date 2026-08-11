import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_U64 = BigInt('18446744073709551615');
const MAX_PATH_BYTES = 131_072;
const MAX_ENTRIES = 100;
const MAX_DISPLAY_CHARACTERS = 256;

export interface ModuleTreeQueryV1 {
  afterModuleId: string | null;
  limit: number;
  parentModuleId: string | null;
}

export interface QueryModuleTreeRequestV1 extends ModuleTreeQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type ModuleTreeEntryKindV1 = 'manifestBoundary' | 'pathBoundary';
export type ModuleTreeChildStateV1 = 'leaf' | 'hasChildren';

export interface ModuleTreeRevisionV1 {
  contentHash: string;
  pathHex: string;
}

export interface ModuleTreeBoundaryEvidenceV1 {
  manifestRevision: ModuleTreeRevisionV1 | null;
  representativeRevision: ModuleTreeRevisionV1 | null;
}

export interface ModuleTreeFeatureCountV1 {
  count: string;
  truncated: boolean;
}

export interface ModuleTreeEntryV1 {
  boundaryEvidence: ModuleTreeBoundaryEvidenceV1;
  centralSymbols: ModuleTreeFeatureCountV1;
  childState: ModuleTreeChildStateV1;
  entrypoints: ModuleTreeFeatureCountV1;
  fileCount: string;
  kind: ModuleTreeEntryKindV1;
  manifestCount: string;
  moduleId: string;
  name: string;
  nameTruncated: boolean;
  rootPathHex: string | null;
  symbolCount: string;
  tests: ModuleTreeFeatureCountV1;
}

export interface ModuleTreePageV1 {
  entries: ModuleTreeEntryV1[];
  graphCommunityCount: string;
  indexRunId: string;
  nextAfterModuleId: string | null;
  parentModuleId: string | null;
  primaryModuleCount: string;
  snapshotId: string;
}

export type ModuleTreeResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { page: ModuleTreePageV1; status: 'available' };

export interface ModuleTreeResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ModuleTreeResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryModuleTree(
  query: ModuleTreeQueryV1 = {
    afterModuleId: null,
    limit: 50,
    parentModuleId: null,
  },
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ModuleTreeResponseV1> {
  if (!isQuery(query)) {
    throw new Error('Module tree query does not match the V1 schema.');
  }
  const request: QueryModuleTreeRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_module_tree', { request });
  return parseModuleTreeResponseV1(payload);
}

export function parseModuleTreeResponseV1(payload: unknown): ModuleTreeResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Module tree response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ModuleTreeResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Module tree response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status === 'noPublishedIndex' && hasExactKeys(value, ['status'])) {
    return { status: 'noPublishedIndex' };
  }
  if (value.status === 'projectionUnavailable' && hasExactKeys(value, ['status'])) {
    return { status: 'projectionUnavailable' };
  }
  if (value.status === 'available' && hasExactKeys(value, ['page', 'status'])) {
    return { page: parsePage(value.page), status: 'available' };
  }
  throw new Error('Module tree response contains an invalid result.');
}

function parsePage(value: unknown): ModuleTreePageV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'entries',
      'graphCommunityCount',
      'indexRunId',
      'nextAfterModuleId',
      'parentModuleId',
      'primaryModuleCount',
      'snapshotId',
    ]) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isOptionalStableId(value.parentModuleId) ||
    !isCount(value.primaryModuleCount) ||
    !isCount(value.graphCommunityCount) ||
    !Array.isArray(value.entries) ||
    value.entries.length > MAX_ENTRIES ||
    !isOptionalStableId(value.nextAfterModuleId)
  ) {
    throw new Error('Module tree response contains an invalid page.');
  }
  const entries = value.entries.map(parseEntry);
  if (entries.some((entry, index) => index > 0 && entries[index - 1]!.moduleId >= entry.moduleId)) {
    throw new Error('Module tree entries are duplicated or unordered.');
  }
  if (
    value.nextAfterModuleId !== null &&
    (entries.length === 0 || entries[entries.length - 1]!.moduleId !== value.nextAfterModuleId)
  ) {
    throw new Error('Module tree response contains an invalid next cursor.');
  }
  if (
    BigInt(value.primaryModuleCount) < BigInt(entries.length) ||
    entries.some((entry) => entry.moduleId === value.parentModuleId)
  ) {
    throw new Error('Module tree response contains contradictory page counts or hierarchy.');
  }
  return {
    entries,
    graphCommunityCount: value.graphCommunityCount,
    indexRunId: value.indexRunId,
    nextAfterModuleId: value.nextAfterModuleId,
    parentModuleId: value.parentModuleId,
    primaryModuleCount: value.primaryModuleCount,
    snapshotId: value.snapshotId,
  };
}

function parseEntry(value: unknown): ModuleTreeEntryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'boundaryEvidence',
      'centralSymbols',
      'childState',
      'entrypoints',
      'fileCount',
      'kind',
      'manifestCount',
      'moduleId',
      'name',
      'nameTruncated',
      'rootPathHex',
      'symbolCount',
      'tests',
    ]) ||
    !isStableId(value.moduleId) ||
    !isEntryKind(value.kind) ||
    !isOptionalRepositoryPathHex(value.rootPathHex) ||
    typeof value.name !== 'string' ||
    value.name.length === 0 ||
    Array.from(value.name).length > MAX_DISPLAY_CHARACTERS ||
    containsControl(value.name) ||
    typeof value.nameTruncated !== 'boolean' ||
    (value.nameTruncated && Array.from(value.name).length !== MAX_DISPLAY_CHARACTERS) ||
    !isCount(value.manifestCount) ||
    !isCount(value.fileCount) ||
    !isCount(value.symbolCount) ||
    !isChildState(value.childState)
  ) {
    throw new Error('Module tree response contains an invalid entry.');
  }
  const boundaryEvidence = parseBoundaryEvidence(value.boundaryEvidence);
  const centralSymbols = parseFeatureCount(value.centralSymbols);
  const entrypoints = parseFeatureCount(value.entrypoints);
  const tests = parseFeatureCount(value.tests);
  const symbolCount = BigInt(value.symbolCount);
  const manifestShape =
    value.kind === 'manifestBoundary'
      ? BigInt(value.manifestCount) > BigInt(0) && boundaryEvidence.manifestRevision !== null
      : value.manifestCount === '0' && boundaryEvidence.manifestRevision === null;
  const representativeShape =
    (symbolCount === BigInt(0)) === (boundaryEvidence.representativeRevision === null);
  if (
    !manifestShape ||
    !representativeShape ||
    BigInt(value.fileCount) > symbolCount ||
    [centralSymbols, entrypoints, tests].some((feature) => BigInt(feature.count) > symbolCount)
  ) {
    throw new Error('Module tree response contains contradictory entry evidence.');
  }
  return {
    boundaryEvidence,
    centralSymbols,
    childState: value.childState,
    entrypoints,
    fileCount: value.fileCount,
    kind: value.kind,
    manifestCount: value.manifestCount,
    moduleId: value.moduleId,
    name: value.name,
    nameTruncated: value.nameTruncated,
    rootPathHex: value.rootPathHex,
    symbolCount: value.symbolCount,
    tests,
  };
}

function parseBoundaryEvidence(value: unknown): ModuleTreeBoundaryEvidenceV1 {
  if (!isRecord(value) || !hasExactKeys(value, ['manifestRevision', 'representativeRevision'])) {
    throw new Error('Module tree response contains invalid boundary evidence.');
  }
  return {
    manifestRevision: parseOptionalRevision(value.manifestRevision),
    representativeRevision: parseOptionalRevision(value.representativeRevision),
  };
}

function parseOptionalRevision(value: unknown): ModuleTreeRevisionV1 | null {
  if (value === null) return null;
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['contentHash', 'pathHex']) ||
    !isStableId(value.contentHash) ||
    !isRepositoryPathHex(value.pathHex)
  ) {
    throw new Error('Module tree response contains an invalid file revision.');
  }
  return { contentHash: value.contentHash, pathHex: value.pathHex };
}

function parseFeatureCount(value: unknown): ModuleTreeFeatureCountV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['count', 'truncated']) ||
    !isCount(value.count) ||
    typeof value.truncated !== 'boolean' ||
    (value.truncated && value.count === '0')
  ) {
    throw new Error('Module tree response contains an invalid feature count.');
  }
  return { count: value.count, truncated: value.truncated };
}

function isQuery(value: ModuleTreeQueryV1): boolean {
  return (
    hasExactKeys(value as unknown as Record<string, unknown>, [
      'afterModuleId',
      'limit',
      'parentModuleId',
    ]) &&
    isOptionalStableId(value.parentModuleId) &&
    isOptionalStableId(value.afterModuleId) &&
    Number.isInteger(value.limit) &&
    value.limit >= 1 &&
    value.limit <= MAX_ENTRIES
  );
}

function isOptionalRepositoryPathHex(value: unknown): value is string | null {
  return value === null || isRepositoryPathHex(value);
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

function isCount(value: unknown): value is string {
  return typeof value === 'string' && COUNT_PATTERN.test(value) && BigInt(value) <= MAX_U64;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isOptionalStableId(value: unknown): value is string | null {
  return value === null || isStableId(value);
}

function isEntryKind(value: unknown): value is ModuleTreeEntryKindV1 {
  return value === 'manifestBoundary' || value === 'pathBoundary';
}

function isChildState(value: unknown): value is ModuleTreeChildStateV1 {
  return value === 'leaf' || value === 'hasChildren';
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

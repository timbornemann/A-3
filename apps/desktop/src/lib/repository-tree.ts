import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_U64 = BigInt('18446744073709551615');
const MAX_PATH_BYTES = 131_072;
const MAX_CHILD_NAME_BYTES = 4_096;
const MAX_ENTRIES = 100;
const MAX_DISPLAY_CHARACTERS = 256;

export interface RepositoryTreeQueryV1 {
  afterNameHex: string | null;
  directoryPathHex: string | null;
  limit: number;
}

export interface QueryRepositoryTreeRequestV1 extends RepositoryTreeQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type RepositoryTreeEntryKindV1 = 'directory' | 'file';

export interface RepositoryTreeEntryV1 {
  contentHash: string | null;
  descendantFileCount: string;
  kind: RepositoryTreeEntryKindV1;
  name: string;
  nameTruncated: boolean;
  pathHex: string;
}

export interface RepositoryTreePageV1 {
  directoryPathHex: string | null;
  entries: RepositoryTreeEntryV1[];
  indexRunId: string;
  nextAfterNameHex: string | null;
  snapshotId: string;
}

export type RepositoryTreeResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { page: RepositoryTreePageV1; status: 'available' };

export interface RepositoryTreeResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: RepositoryTreeResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryRepositoryTree(
  query: RepositoryTreeQueryV1 = {
    afterNameHex: null,
    directoryPathHex: null,
    limit: 50,
  },
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<RepositoryTreeResponseV1> {
  if (!isQuery(query)) {
    throw new Error('Repository tree query does not match the V1 schema.');
  }
  const request: QueryRepositoryTreeRequestV1 = {
    ...query,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  const payload = await invokeCommand('query_repository_tree', { request });
  return parseRepositoryTreeResponseV1(payload);
}

export function parseRepositoryTreeResponseV1(payload: unknown): RepositoryTreeResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Repository tree response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): RepositoryTreeResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Repository tree response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status === 'noPublishedIndex' && hasExactKeys(value, ['status'])) {
    return { status: 'noPublishedIndex' };
  }
  if (value.status === 'available' && hasExactKeys(value, ['page', 'status'])) {
    return { page: parsePage(value.page), status: 'available' };
  }
  throw new Error('Repository tree response contains an invalid result.');
}

function parsePage(value: unknown): RepositoryTreePageV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'directoryPathHex',
      'entries',
      'indexRunId',
      'nextAfterNameHex',
      'snapshotId',
    ]) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isOptionalRepositoryPathHex(value.directoryPathHex) ||
    !Array.isArray(value.entries) ||
    value.entries.length > MAX_ENTRIES ||
    !isOptionalChildNameHex(value.nextAfterNameHex)
  ) {
    throw new Error('Repository tree response contains an invalid page.');
  }
  const directoryPathHex = value.directoryPathHex as string | null;
  const entries = value.entries.map((entry) => parseEntry(entry, directoryPathHex));
  const names = entries.map((entry) => directChildNameHex(entry.pathHex, directoryPathHex));
  if (names.some((name, index) => index > 0 && names[index - 1]! >= name)) {
    throw new Error('Repository tree entries are duplicated or unordered.');
  }
  if (
    value.nextAfterNameHex !== null &&
    (names.length === 0 || names[names.length - 1] !== value.nextAfterNameHex)
  ) {
    throw new Error('Repository tree response contains an invalid next cursor.');
  }
  return {
    directoryPathHex,
    entries,
    indexRunId: value.indexRunId,
    nextAfterNameHex: value.nextAfterNameHex,
    snapshotId: value.snapshotId,
  };
}

function parseEntry(value: unknown, directoryPathHex: string | null): RepositoryTreeEntryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'contentHash',
      'descendantFileCount',
      'kind',
      'name',
      'nameTruncated',
      'pathHex',
    ]) ||
    !isEntryKind(value.kind) ||
    !isRepositoryPathHex(value.pathHex) ||
    typeof value.name !== 'string' ||
    value.name.length === 0 ||
    Array.from(value.name).length > MAX_DISPLAY_CHARACTERS ||
    containsControl(value.name) ||
    typeof value.nameTruncated !== 'boolean' ||
    (value.nameTruncated && Array.from(value.name).length !== MAX_DISPLAY_CHARACTERS) ||
    !isCount(value.descendantFileCount) ||
    BigInt(value.descendantFileCount) === BigInt(0)
  ) {
    throw new Error('Repository tree response contains an invalid entry.');
  }
  directChildNameHex(value.pathHex, directoryPathHex);
  const fileShape =
    value.kind === 'file' && value.descendantFileCount === '1' && isStableId(value.contentHash);
  const directoryShape = value.kind === 'directory' && value.contentHash === null;
  if (!fileShape && !directoryShape) {
    throw new Error('Repository tree response contains contradictory entry evidence.');
  }
  const contentHash = value.contentHash as string | null;
  return {
    contentHash,
    descendantFileCount: value.descendantFileCount,
    kind: value.kind,
    name: value.name,
    nameTruncated: value.nameTruncated,
    pathHex: value.pathHex,
  };
}

function directChildNameHex(pathHex: string, directoryPathHex: string | null): string {
  const prefix = directoryPathHex === null ? '' : `${directoryPathHex}2f`;
  if (!pathHex.startsWith(prefix)) {
    throw new Error('Repository tree entry is outside the requested directory.');
  }
  const childNameHex = pathHex.slice(prefix.length);
  if (!isChildNameHex(childNameHex)) {
    throw new Error('Repository tree entry is not a direct child.');
  }
  return childNameHex;
}

function isQuery(value: RepositoryTreeQueryV1): boolean {
  return (
    hasExactKeys(value as unknown as Record<string, unknown>, [
      'afterNameHex',
      'directoryPathHex',
      'limit',
    ]) &&
    isOptionalRepositoryPathHex(value.directoryPathHex) &&
    isOptionalChildNameHex(value.afterNameHex) &&
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
    !isBoundedHex(value, MAX_PATH_BYTES) ||
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

function isOptionalChildNameHex(value: unknown): value is string | null {
  return value === null || (typeof value === 'string' && isChildNameHex(value));
}

function isChildNameHex(value: string): boolean {
  return (
    isBoundedHex(value, MAX_CHILD_NAME_BYTES) &&
    value !== '2e' &&
    value !== '2e2e' &&
    !containsHexByte(value, '00') &&
    !containsHexByte(value, '2f')
  );
}

function isBoundedHex(value: string, maxBytes: number): boolean {
  return value.length <= maxBytes * 2 && HEX_PATTERN.test(value);
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

function isEntryKind(value: unknown): value is RepositoryTreeEntryKindV1 {
  return value === 'directory' || value === 'file';
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

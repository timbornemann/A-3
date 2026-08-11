import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const COUNT_PATTERN = /^(?:0|[1-9][0-9]{0,19})$/;
const ZERO = BigInt(0);
const MAX_U64 = BigInt('18446744073709551615');
const MAX_PATH_DISPLAY_CHARS = 512;
const MAX_DIAGNOSTIC_FILES = 64;
const MAX_DIAGNOSTICS_PER_FILE = 8;
const MAX_DIAGNOSTIC_MESSAGE_BYTES = 1_024;
const MAX_U32 = 4_294_967_295;

export interface QueryIndexOverviewRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type IndexLanguageV1 = 'generic' | 'rust' | 'typeScriptJavaScript' | 'python';
export type IndexDiagnosticCodeV1 =
  'syntaxError' | 'missingSyntax' | 'invalidEncoding' | 'unsupportedSyntax' | 'outputTruncated';
export type IndexDiagnosticSeverityV1 = 'error' | 'warning' | 'information';

export interface IndexDiagnosticV1 {
  code: IndexDiagnosticCodeV1;
  endByte: number;
  message: string;
  severity: IndexDiagnosticSeverityV1;
  startByte: number;
}

export interface IndexFileDiagnosticsV1 {
  coverageBasisPoints: number | null;
  diagnosticCount: string;
  diagnostics: IndexDiagnosticV1[];
  diagnosticsTruncated: boolean;
  language: IndexLanguageV1;
  pathDisplay: string;
  pathDisplayTruncated: boolean;
}

export interface IndexOverviewCountsV1 {
  diagnosticCount: string;
  diagnosticFileCount: string;
  fileCount: string;
  parsedFileCount: string;
  symbolCount: string;
}

export interface IndexOverviewV1 {
  counts: IndexOverviewCountsV1;
  coverageBasisPoints: number | null;
  diagnosticFiles: IndexFileDiagnosticsV1[];
  diagnosticFilesTruncated: boolean;
  snapshotId: string;
}

export type IndexOverviewResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { overview: IndexOverviewV1; status: 'published' };

export interface IndexOverviewResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: IndexOverviewResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryIndexOverview(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<IndexOverviewResponseV1> {
  const request: QueryIndexOverviewRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  const payload = await invokeCommand('query_index_overview', { request });
  return parseIndexOverviewResponseV1(payload);
}

export function parseIndexOverviewResponseV1(payload: unknown): IndexOverviewResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Index overview response does not match the V1 schema.');
  }
  return { protocolVersion: payload.protocolVersion, result: parseResult(payload.result) };
}

function parseResult(value: unknown): IndexOverviewResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Index overview response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status === 'noPublishedIndex' && hasExactKeys(value, ['status'])) {
    return { status: 'noPublishedIndex' };
  }
  if (value.status === 'published' && hasExactKeys(value, ['overview', 'status'])) {
    return { overview: parseOverview(value.overview), status: 'published' };
  }
  throw new Error('Index overview response contains an invalid result.');
}

function parseOverview(value: unknown): IndexOverviewV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'counts',
      'coverageBasisPoints',
      'diagnosticFiles',
      'diagnosticFilesTruncated',
      'snapshotId',
    ]) ||
    !isStableId(value.snapshotId) ||
    !isCoverage(value.coverageBasisPoints) ||
    !Array.isArray(value.diagnosticFiles) ||
    value.diagnosticFiles.length > MAX_DIAGNOSTIC_FILES ||
    typeof value.diagnosticFilesTruncated !== 'boolean'
  ) {
    throw new Error('Index overview response contains an invalid publication.');
  }
  const counts = parseCounts(value.counts);
  const diagnosticFiles = value.diagnosticFiles.map(parseDiagnosticFile);
  const fileCount = BigInt(counts.fileCount);
  const parsedFileCount = BigInt(counts.parsedFileCount);
  const diagnosticFileCount = BigInt(counts.diagnosticFileCount);
  const diagnosticCount = BigInt(counts.diagnosticCount);
  const visibleDiagnosticCount = diagnosticFiles.reduce(
    (total, file) => total + BigInt(file.diagnosticCount),
    ZERO,
  );
  if (
    parsedFileCount > fileCount ||
    diagnosticFileCount > fileCount ||
    diagnosticFileCount < BigInt(diagnosticFiles.length) ||
    value.diagnosticFilesTruncated !== diagnosticFileCount > BigInt(diagnosticFiles.length) ||
    (parsedFileCount === ZERO) !== (value.coverageBasisPoints === null) ||
    diagnosticCount < visibleDiagnosticCount ||
    (!value.diagnosticFilesTruncated && diagnosticCount !== visibleDiagnosticCount)
  ) {
    throw new Error('Index overview response contains contradictory aggregate values.');
  }
  return {
    counts,
    coverageBasisPoints: value.coverageBasisPoints,
    diagnosticFiles,
    diagnosticFilesTruncated: value.diagnosticFilesTruncated,
    snapshotId: value.snapshotId,
  };
}

function parseCounts(value: unknown): IndexOverviewCountsV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'diagnosticCount',
      'diagnosticFileCount',
      'fileCount',
      'parsedFileCount',
      'symbolCount',
    ]) ||
    !isCount(value.diagnosticCount) ||
    !isCount(value.diagnosticFileCount) ||
    !isCount(value.fileCount) ||
    !isCount(value.parsedFileCount) ||
    !isCount(value.symbolCount)
  ) {
    throw new Error('Index overview response contains invalid counts.');
  }
  return {
    diagnosticCount: value.diagnosticCount,
    diagnosticFileCount: value.diagnosticFileCount,
    fileCount: value.fileCount,
    parsedFileCount: value.parsedFileCount,
    symbolCount: value.symbolCount,
  };
}

function parseDiagnosticFile(value: unknown): IndexFileDiagnosticsV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'coverageBasisPoints',
      'diagnosticCount',
      'diagnostics',
      'diagnosticsTruncated',
      'language',
      'pathDisplay',
      'pathDisplayTruncated',
    ]) ||
    !isCoverage(value.coverageBasisPoints) ||
    !isCount(value.diagnosticCount) ||
    BigInt(value.diagnosticCount) === ZERO ||
    !Array.isArray(value.diagnostics) ||
    value.diagnostics.length > MAX_DIAGNOSTICS_PER_FILE ||
    typeof value.diagnosticsTruncated !== 'boolean' ||
    !isLanguage(value.language) ||
    typeof value.pathDisplay !== 'string' ||
    value.pathDisplay.length === 0 ||
    Array.from(value.pathDisplay).length > MAX_PATH_DISPLAY_CHARS ||
    containsControl(value.pathDisplay) ||
    typeof value.pathDisplayTruncated !== 'boolean'
  ) {
    throw new Error('Index overview response contains invalid file diagnostics.');
  }
  const diagnostics = value.diagnostics.map(parseDiagnostic);
  const diagnosticCount = BigInt(value.diagnosticCount);
  if (
    diagnosticCount < BigInt(diagnostics.length) ||
    value.diagnosticsTruncated !== diagnosticCount > BigInt(diagnostics.length) ||
    (value.language === 'generic') !== (value.coverageBasisPoints === null)
  ) {
    throw new Error('Index overview response contains contradictory file diagnostics.');
  }
  return {
    coverageBasisPoints: value.coverageBasisPoints,
    diagnosticCount: value.diagnosticCount,
    diagnostics,
    diagnosticsTruncated: value.diagnosticsTruncated,
    language: value.language,
    pathDisplay: value.pathDisplay,
    pathDisplayTruncated: value.pathDisplayTruncated,
  };
}

function parseDiagnostic(value: unknown): IndexDiagnosticV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['code', 'endByte', 'message', 'severity', 'startByte']) ||
    !isDiagnosticCode(value.code) ||
    !isDiagnosticSeverity(value.severity) ||
    typeof value.message !== 'string' ||
    new TextEncoder().encode(value.message).length === 0 ||
    new TextEncoder().encode(value.message).length > MAX_DIAGNOSTIC_MESSAGE_BYTES ||
    containsControl(value.message) ||
    !isU32(value.startByte) ||
    !isU32(value.endByte) ||
    value.endByte < value.startByte
  ) {
    throw new Error('Index overview response contains an invalid diagnostic.');
  }
  return {
    code: value.code,
    endByte: value.endByte,
    message: value.message,
    severity: value.severity,
    startByte: value.startByte,
  };
}

function isCoverage(value: unknown): value is number | null {
  return (
    value === null ||
    (Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= 10_000)
  );
}

function isCount(value: unknown): value is string {
  return typeof value === 'string' && COUNT_PATTERN.test(value) && BigInt(value) <= MAX_U64;
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isU32(value: unknown): value is number {
  return Number.isInteger(value) && typeof value === 'number' && value >= 0 && value <= MAX_U32;
}

function isLanguage(value: unknown): value is IndexLanguageV1 {
  return (
    value === 'generic' ||
    value === 'rust' ||
    value === 'typeScriptJavaScript' ||
    value === 'python'
  );
}

function isDiagnosticCode(value: unknown): value is IndexDiagnosticCodeV1 {
  return (
    value === 'syntaxError' ||
    value === 'missingSyntax' ||
    value === 'invalidEncoding' ||
    value === 'unsupportedSyntax' ||
    value === 'outputTruncated'
  );
}

function isDiagnosticSeverity(value: unknown): value is IndexDiagnosticSeverityV1 {
  return value === 'error' || value === 'warning' || value === 'information';
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
  const keys = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return (
    keys.length === sortedExpected.length &&
    keys.every((key, index) => key === sortedExpected[index])
  );
}

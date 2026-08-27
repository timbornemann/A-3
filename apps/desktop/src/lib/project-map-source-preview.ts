import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModuleCardEvidenceQueryV1 } from './module-card-evidence';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const MAX_LINES = 64;
const MAX_BYTES = 16 * 1_024;
const MAX_PATH_CHARACTERS = 512;
const MAX_U32 = 4_294_967_295;
const utf8 = new TextEncoder();

export type ProjectMapSourceLanguageV1 = 'generic' | 'rust' | 'typeScriptJavaScript' | 'python';

export interface ProjectMapSourceHighlightV1 {
  endColumn: number;
  endLine: number;
  startColumn: number;
  startLine: number;
}

export interface ProjectMapSourcePreviewV1 {
  highlight: ProjectMapSourceHighlightV1 | null;
  language: ProjectMapSourceLanguageV1;
  lineCount: number;
  pathDisplay: string;
  startLine: number;
  text: string;
  truncatedAfter: boolean;
  truncatedBefore: boolean;
}

export type ProjectMapSourcePreviewResultV1 =
  | { status: 'noProject' }
  | { status: 'noPublishedIndex' }
  | { status: 'projectionUnavailable' }
  | { status: 'moduleUnavailable' }
  | { status: 'cardUnavailable' }
  | { status: 'selectionChanged' }
  | { status: 'evidenceUnavailable' }
  | { status: 'staleEvidence' }
  | { preview: ProjectMapSourcePreviewV1; status: 'available' };

export interface ProjectMapSourcePreviewResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectMapSourcePreviewResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectMapSourcePreview(
  query: ModuleCardEvidenceQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectMapSourcePreviewResponseV1> {
  if (!isEvidenceQuery(query))
    throw new Error('Project Map source-preview query does not match V1.');
  const request = { ...query, protocolVersion: CURRENT_PROTOCOL_VERSION };
  return parseProjectMapSourcePreviewResponseV1(
    await invokeCommand('query_project_map_source_preview', { request }),
  );
}

export function parseProjectMapSourcePreviewResponseV1(
  payload: unknown,
): ProjectMapSourcePreviewResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Project Map source-preview response does not match V1.');
  }
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: parseResult(payload.result) };
}

function parseResult(value: unknown): ProjectMapSourcePreviewResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  for (const status of [
    'noProject',
    'noPublishedIndex',
    'projectionUnavailable',
    'moduleUnavailable',
    'cardUnavailable',
    'selectionChanged',
    'evidenceUnavailable',
    'staleEvidence',
  ] as const) {
    if (value.status === status && hasExactKeys(value, ['status'])) return { status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['preview', 'status'])) {
    return { preview: parsePreview(value.preview), status: 'available' };
  }
  return invalidResult();
}

function invalidResult(): never {
  throw new Error('Project Map source-preview response contains an invalid result.');
}

function parsePreview(value: unknown): ProjectMapSourcePreviewV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'highlight',
      'language',
      'lineCount',
      'pathDisplay',
      'startLine',
      'text',
      'truncatedAfter',
      'truncatedBefore',
    ]) ||
    !isLanguage(value.language) ||
    typeof value.pathDisplay !== 'string' ||
    value.pathDisplay.length === 0 ||
    Array.from(value.pathDisplay).length > MAX_PATH_CHARACTERS ||
    containsControl(value.pathDisplay) ||
    !isIntegerBetween(value.startLine, 1, MAX_U32) ||
    !isIntegerBetween(value.lineCount, 0, MAX_LINES) ||
    typeof value.text !== 'string' ||
    utf8.encode(value.text).length > MAX_BYTES ||
    sourceLineCount(value.text) !== value.lineCount ||
    typeof value.truncatedBefore !== 'boolean' ||
    typeof value.truncatedAfter !== 'boolean' ||
    value.truncatedBefore !== value.startLine > 1
  ) {
    throw new Error('Project Map source-preview response contains an invalid preview.');
  }
  const highlight = value.highlight === null ? null : parseHighlight(value.highlight);
  const pageEnd =
    value.lineCount === 0 ? value.startLine - 1 : value.startLine + value.lineCount - 1;
  if (
    highlight !== null &&
    (highlight.startLine < value.startLine || highlight.endLine > pageEnd)
  ) {
    throw new Error('Project Map source-preview highlight is outside the page.');
  }
  return {
    highlight,
    language: value.language,
    lineCount: value.lineCount,
    pathDisplay: value.pathDisplay,
    startLine: value.startLine,
    text: value.text,
    truncatedAfter: value.truncatedAfter,
    truncatedBefore: value.truncatedBefore,
  };
}

function parseHighlight(value: unknown): ProjectMapSourceHighlightV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['endColumn', 'endLine', 'startColumn', 'startLine']) ||
    !isIntegerBetween(value.startLine, 1, MAX_U32) ||
    !isIntegerBetween(value.endLine, 1, MAX_U32) ||
    !isIntegerBetween(value.startColumn, 0, MAX_U32) ||
    !isIntegerBetween(value.endColumn, 0, MAX_U32) ||
    value.endLine < value.startLine
  ) {
    throw new Error('Project Map source-preview response contains an invalid highlight.');
  }
  return {
    endColumn: value.endColumn,
    endLine: value.endLine,
    startColumn: value.startColumn,
    startLine: value.startLine,
  };
}

function sourceLineCount(value: string): number {
  if (value.length === 0) return 0;
  const newlines = Array.from(value).filter((character) => character === '\n').length;
  return newlines + (value.endsWith('\n') ? 0 : 1);
}

function isEvidenceQuery(value: ModuleCardEvidenceQueryV1): boolean {
  return (
    isStableId(value.cardId) &&
    isStableId(value.currentIndexRunId) &&
    isStableId(value.currentSnapshotId) &&
    isStableId(value.evidenceId) &&
    isStableId(value.moduleId) &&
    isStableId(value.sourceIndexRunId) &&
    isStableId(value.sourceSnapshotId) &&
    (value.sourceIndexRunId !== value.currentIndexRunId ||
      value.sourceSnapshotId === value.currentSnapshotId)
  );
}

function isLanguage(value: unknown): value is ProjectMapSourceLanguageV1 {
  return (
    value === 'generic' ||
    value === 'rust' ||
    value === 'typeScriptJavaScript' ||
    value === 'python'
  );
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isIntegerBetween(value: unknown, minimum: number, maximum: number): value is number {
  return (
    typeof value === 'number' && Number.isInteger(value) && value >= minimum && value <= maximum
  );
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

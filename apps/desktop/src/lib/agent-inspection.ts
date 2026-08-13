import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID = /^[0-9a-f]{64}$/;
const DECIMAL = /^(?:0|[1-9][0-9]{0,19})$/;
const POSITIVE_DECIMAL = /^[1-9][0-9]{0,19}$/;
const HEX = /^(?:[0-9a-f]{2})+$/;
const MAX_U64 = 18_446_744_073_709_551_615n;
const MAX_U32 = 4_294_967_295;
const MAX_PATCH_FILES = 64;
const MAX_PATCH_RETAINED_BYTES = 64 * 1024;
const MAX_PATCH_CONTENT_BYTES = 16 * 1024;
const MAX_DIFF_ROWS = 128 * 1024;
const MAX_DIFF_EVIDENCE_PATHS = 128;
const MAX_PROCESS_ROWS = 32;
const MAX_LOG_PAGE = 16 * 1024;
const MAX_TEST_CASES = 100;
const MAX_STEPS = 256;
const MAX_CRITERIA = 64;
const MAX_EVIDENCE_PER_ATTEMPT = 64;
const MAX_PATH_BYTES = 131_072;
const MAX_PATH_DISPLAY_BYTES = MAX_PATH_BYTES * 4;
const MAX_CRITERION_TEXT_BYTES = 4 * 1024;
const MAX_STEP_TEXT_BYTES = 8 * 1024;
const MAX_TEST_CASE_NAME_BYTES = 1024;
const utf8 = new TextEncoder();

export type AgentInspectionStreamV1 = 'stdout' | 'stderr';
export type AgentDiffLineEndingV1 = 'lf' | 'crlf' | 'cr' | 'none';
export type AgentChangeAttributionV1 =
  'proposedAgent' | 'appliedAgent' | 'external' | 'unattributed';
export type AgentVerificationMethodV1 =
  'command' | 'test' | 'diffInvariant' | 'diagnostic' | 'userConfirm';
export type AgentVerificationStepStatusV1 =
  | 'pending'
  | 'ready'
  | 'inProgress'
  | 'blocked'
  | 'awaitingApproval'
  | 'verifying'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'stale';

export interface AgentInspectionPathV1 {
  displayPath: string;
  pathHex: string;
}

export interface AgentDiffLineV1 {
  ending: AgentDiffLineEndingV1;
  text: string;
}

export type AgentDiffRowV1 =
  | { afterLine: number; beforeLine: number; kind: 'context'; line: AgentDiffLineV1 }
  | { beforeLine: number; kind: 'removed'; line: AgentDiffLineV1 }
  | { afterLine: number; kind: 'added'; line: AgentDiffLineV1 };

export interface AgentDiffHunkV1 {
  afterCount: number;
  afterStart: number;
  beforeCount: number;
  beforeStart: number;
  rows: AgentDiffRowV1[];
}

export interface AgentDiffContentV1 {
  contentHash: string;
  contentTruncated: boolean;
  encoding: 'utf8' | 'utf8Bom';
  lineEndings: 'none' | 'lf' | 'crlf' | 'cr' | 'mixed';
  retainedBytes: string;
  totalBytes: string;
}

export interface AgentDiffFileV1 {
  addedLines: number;
  after: AgentDiffContentV1 | null;
  attribution: AgentChangeAttributionV1;
  before: AgentDiffContentV1 | null;
  contentTruncated: boolean;
  hunks: AgentDiffHunkV1[];
  operation: 'add' | 'update' | 'move' | 'delete';
  removedLines: number;
  sourcePath: AgentInspectionPathV1 | null;
  targetPath: AgentInspectionPathV1 | null;
}

export interface AgentPatchInspectionV1 {
  files: AgentDiffFileV1[];
  inspectionId: string;
  retainedBytes: string;
  runId: string;
  snapshotId: string;
  stepId: string;
  verificationSpecId: string;
}

export type AgentProcessRedactionV1 = 'invalidUtf8' | 'secretCandidate' | 'unsafeControl';

export interface AgentProcessStreamV1 {
  digest: string;
  observedBytes: string;
  redaction: AgentProcessRedactionV1 | null;
  retainedBytes: string;
  retainedLimit: number;
  sourceTruncated: boolean;
}

export type AgentProcessTerminationV1 =
  | { code: number | null; kind: 'exited'; success: boolean }
  | { kind: 'timedOut' }
  | { kind: 'cancelled' };

export interface AgentProcessInspectionV1 {
  durationMillis: string;
  inspectionId: string;
  kind: 'test' | 'build' | 'diagnostic' | 'lint' | 'format' | 'command';
  runId: string;
  snapshotId: string;
  stderr: AgentProcessStreamV1;
  stdout: AgentProcessStreamV1;
  stepId: string;
  termination: AgentProcessTerminationV1;
  verificationSpecId: string;
}

export type AgentVerificationEvaluationV1 =
  { status: 'passed' } | { reason: string; status: 'failed' };
export type AgentEvidenceFreshnessV1 =
  { status: 'fresh' } | { reason: 'snapshotChanged' | 'dependencyChanged'; status: 'stale' };

export interface AgentVerificationProcessStreamV1 {
  digest: string;
  observedBytes: string;
  redaction: AgentProcessRedactionV1 | null;
  retainedLimit: number;
  sourceTruncated: boolean;
}

export interface AgentVerificationCommandV1 {
  commandId: string;
  durationMillis: string;
  stderr: AgentVerificationProcessStreamV1;
  stdout: AgentVerificationProcessStreamV1;
  termination: AgentProcessTerminationV1;
}

export interface AgentTestCaseV1 {
  name: string;
  outcome: 'passed' | 'failed' | 'ignored';
}

export type AgentVerificationEvidenceDetailV1 =
  | { command: AgentVerificationCommandV1; kind: 'command' }
  | {
      cases: AgentTestCaseV1[];
      casesTruncated: boolean;
      command: AgentVerificationCommandV1;
      failed: string;
      ignored: string;
      kind: 'test';
      passed: string;
    }
  | {
      baseSnapshotId: string;
      changedPaths: AgentInspectionPathV1[];
      complete: boolean;
      kind: 'diff';
      snapshotId: string;
      source: 'patchChangeSet' | 'publishedIndexes';
    }
  | { command: AgentVerificationCommandV1; errors: number; kind: 'diagnostic'; warnings: number }
  | { confirmedAtUnixMillis: string; kind: 'userConfirmation'; scopeId: string };

export interface AgentVerificationEvidenceV1 {
  detail: AgentVerificationEvidenceDetailV1;
  evaluation: AgentVerificationEvaluationV1;
  evidenceId: string;
  freshness: AgentEvidenceFreshnessV1;
  method: AgentVerificationMethodV1;
  runId: string;
  snapshotId: string;
}

export type AgentStepVerificationOutcomeV1 =
  { status: 'passed' } | { status: 'failed'; summary: string };

export interface AgentVerificationAttemptV1 {
  evidence: AgentVerificationEvidenceV1[];
  number: number;
  outcome: AgentStepVerificationOutcomeV1;
}

export type AgentStepStaleCauseV1 =
  { evidenceIds: string[]; kind: 'verificationEvidence' } | { kind: 'dependency'; stepId: string };

export interface AgentVerificationStepV1 {
  attempts: AgentVerificationAttemptV1[];
  intendedOutcome: string;
  method: AgentVerificationMethodV1;
  staleCause: AgentStepStaleCauseV1 | null;
  status: AgentVerificationStepStatusV1;
  stepId: string;
  verificationSpecId: string;
}

export interface AgentCriterionProofV1 {
  evidenceIds: string[];
  stepId: string;
}

export interface AgentCriterionInspectionV1 {
  criterionId: string;
  proofState: 'proven' | 'pending' | 'failed' | 'stale' | 'missing';
  proofs: AgentCriterionProofV1[];
  requirement: 'must' | 'should';
  statement: string;
}

export interface AgentVerificationInspectionV1 {
  criteria: AgentCriterionInspectionV1[];
  goalRevision: number;
  ledgerRevision: number;
  ledgerStoreVersion: string;
  publishedSnapshotId: string;
  steps: AgentVerificationStepV1[];
}

export interface AgentInspectionV1 {
  inspectionRevision: string | null;
  patch: AgentPatchInspectionV1 | null;
  processes: AgentProcessInspectionV1[];
  verification: AgentVerificationInspectionV1;
}

export type AgentInspectionResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | { status: 'goalRevisionMismatch' }
  | { status: 'inspectionChanged' }
  | { inspection: AgentInspectionV1; status: 'available' };

export interface AgentInspectionResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentInspectionResultV1;
}

export interface AgentInspectionLogPageV1 {
  nextOffset: number | null;
  offset: number;
  pageTruncated: boolean;
  redaction: AgentProcessRedactionV1 | null;
  sourceTruncated: boolean;
  text: string;
}

export type AgentInspectionLogResultV1 =
  | { status: 'noProject' }
  | { status: 'unavailable' }
  | { status: 'inspectionChanged' }
  | { page: AgentInspectionLogPageV1; status: 'available' };

export interface AgentInspectionLogResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentInspectionLogResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentInspection(
  taskId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentInspectionResponseV1> {
  if (!isStableId(taskId)) throw new Error('Agent inspection task identity does not match V1.');
  return parseAgentInspectionResponseV1(
    await invokeCommand('query_agent_inspection', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, taskId },
    }),
  );
}

export async function queryAgentInspectionLog(
  taskId: string,
  inspectionRevision: string,
  inspectionId: string,
  stream: AgentInspectionStreamV1,
  offset: number,
  limit = 8 * 1024,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentInspectionLogResponseV1> {
  if (
    !isStableId(taskId) ||
    !isPositiveDecimal(inspectionRevision) ||
    !isStableId(inspectionId) ||
    (stream !== 'stdout' && stream !== 'stderr') ||
    !isU32(offset) ||
    !isU32(limit) ||
    limit < 4 ||
    limit > MAX_LOG_PAGE
  ) {
    throw new Error('Agent inspection log selection does not match V1.');
  }
  const response = parseAgentInspectionLogResponseV1(
    await invokeCommand('query_agent_inspection_log', {
      request: {
        inspectionId,
        inspectionRevision,
        limit,
        offset,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        stream,
        taskId,
      },
    }),
  );
  if (response.result.status === 'available') {
    const page = response.result.page;
    const pageBytes = utf8.encode(page.text).length;
    if (
      page.offset !== offset ||
      pageBytes > limit ||
      (page.nextOffset !== null && page.nextOffset !== offset + pageBytes)
    ) {
      throw new Error('Agent inspection log response does not match the requested cursor.');
    }
  }
  return response;
}

export function parseAgentInspectionResponseV1(payload: unknown): AgentInspectionResponseV1 {
  const envelope = parseEnvelope(payload, 'Agent inspection');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: parseInspectionResult(envelope.result),
  };
}

export function parseAgentInspectionLogResponseV1(payload: unknown): AgentInspectionLogResponseV1 {
  const envelope = parseEnvelope(payload, 'Agent inspection log');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: parseLogResult(envelope.result),
  };
}

function parseEnvelope(
  value: unknown,
  label: string,
): { protocolVersion: typeof CURRENT_PROTOCOL_VERSION; result: unknown } {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['protocolVersion', 'result']) ||
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error(`${label} response does not match V1.`);
  }
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: value.result };
}

function parseInspectionResult(value: unknown): AgentInspectionResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalid('result');
  if (
    [
      'noProject',
      'taskNotFound',
      'ledgerUnavailable',
      'goalRevisionMismatch',
      'inspectionChanged',
    ].includes(value.status) &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status } as AgentInspectionResultV1;
  }
  if (value.status === 'available' && hasExactKeys(value, ['inspection', 'status'])) {
    return { inspection: parseInspection(value.inspection), status: 'available' };
  }
  return invalid('result');
}

function parseInspection(value: unknown): AgentInspectionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['inspectionRevision', 'patch', 'processes', 'verification']) ||
    (value.inspectionRevision !== null && !isPositiveDecimal(value.inspectionRevision)) ||
    !Array.isArray(value.processes) ||
    value.processes.length > MAX_PROCESS_ROWS
  ) {
    return invalid('projection');
  }
  const patch = value.patch === null ? null : parsePatch(value.patch);
  const processes = value.processes.map(parseProcess);
  if ((patch !== null || processes.length > 0) !== (value.inspectionRevision !== null)) {
    return invalid('volatile revision');
  }
  const ids = [patch?.inspectionId, ...processes.map((process) => process.inspectionId)].filter(
    (item): item is string => item !== undefined,
  );
  if (new Set(ids).size !== ids.length) return invalid('volatile identities');
  const verification = parseVerification(value.verification);
  validateVolatileAnchors(patch, processes, verification);
  return {
    inspectionRevision: value.inspectionRevision,
    patch,
    processes,
    verification,
  };
}

function validateVolatileAnchors(
  patch: AgentPatchInspectionV1 | null,
  processes: AgentProcessInspectionV1[],
  verification: AgentVerificationInspectionV1,
): void {
  const specifications = new Map(
    verification.steps.map((step) => [step.stepId, step.verificationSpecId]),
  );
  for (const record of [...(patch === null ? [] : [patch]), ...processes]) {
    if (
      record.snapshotId !== verification.publishedSnapshotId ||
      specifications.get(record.stepId) !== record.verificationSpecId
    ) {
      return invalid('volatile anchor');
    }
  }
}

function parsePatch(value: unknown): AgentPatchInspectionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'files',
      'inspectionId',
      'retainedBytes',
      'runId',
      'snapshotId',
      'stepId',
      'verificationSpecId',
    ]) ||
    ![
      value.inspectionId,
      value.runId,
      value.snapshotId,
      value.stepId,
      value.verificationSpecId,
    ].every(isStableId) ||
    !isDecimal(value.retainedBytes) ||
    BigInt(value.retainedBytes) > BigInt(MAX_PATCH_RETAINED_BYTES) ||
    !Array.isArray(value.files) ||
    value.files.length === 0 ||
    value.files.length > MAX_PATCH_FILES
  ) {
    return invalid('patch');
  }
  const files = value.files.map(parseFile);
  const sourcePaths = files.flatMap((file) =>
    file.sourcePath === null ? [] : [file.sourcePath.pathHex],
  );
  const targetPaths = files.flatMap((file) =>
    file.targetPath === null ? [] : [file.targetPath.pathHex],
  );
  if (
    new Set(sourcePaths).size !== sourcePaths.length ||
    new Set(targetPaths).size !== targetPaths.length ||
    files.reduce((count, file) => count + file.hunks.length, 0) > MAX_DIFF_ROWS ||
    files.reduce(
      (count, file) => count + file.hunks.reduce((rows, hunk) => rows + hunk.rows.length, 0),
      0,
    ) > MAX_DIFF_ROWS
  ) {
    return invalid('patch bounds');
  }
  const retained = files.reduce(
    (sum, file) =>
      sum + BigInt(file.before?.retainedBytes ?? '0') + BigInt(file.after?.retainedBytes ?? '0'),
    0n,
  );
  if (retained !== BigInt(value.retainedBytes)) return invalid('patch retained bytes');
  return { ...value, files } as AgentPatchInspectionV1;
}

function parseFile(value: unknown): AgentDiffFileV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'addedLines',
      'after',
      'attribution',
      'before',
      'contentTruncated',
      'hunks',
      'operation',
      'removedLines',
      'sourcePath',
      'targetPath',
    ]) ||
    !['add', 'update', 'move', 'delete'].includes(value.operation as string) ||
    !['proposedAgent', 'appliedAgent', 'external', 'unattributed'].includes(
      value.attribution as string,
    ) ||
    !isU32(value.addedLines) ||
    !isU32(value.removedLines) ||
    typeof value.contentTruncated !== 'boolean' ||
    !Array.isArray(value.hunks)
  ) {
    return invalid('diff file');
  }
  const sourcePath = value.sourcePath === null ? null : parsePath(value.sourcePath);
  const targetPath = value.targetPath === null ? null : parsePath(value.targetPath);
  const before = value.before === null ? null : parseContent(value.before);
  const after = value.after === null ? null : parseContent(value.after);
  const shape = `${sourcePath === null ? '0' : '1'}${targetPath === null ? '0' : '1'}${before === null ? '0' : '1'}${after === null ? '0' : '1'}`;
  const expectedShape = { add: '0101', update: '1111', move: '1111', delete: '1010' }[
    value.operation as AgentDiffFileV1['operation']
  ];
  if (
    shape !== expectedShape ||
    (value.operation === 'update' && sourcePath?.pathHex !== targetPath?.pathHex) ||
    (value.operation === 'move' && sourcePath?.pathHex === targetPath?.pathHex) ||
    value.contentTruncated !==
      (before?.contentTruncated === true || after?.contentTruncated === true)
  ) {
    return invalid('diff file shape');
  }
  const hunks = value.hunks.map(parseHunk);
  const rows = hunks.flatMap((hunk) => hunk.rows);
  if (
    !hunksAreOrdered(hunks) ||
    rows.filter((row) => row.kind === 'added').length !== value.addedLines ||
    rows.filter((row) => row.kind === 'removed').length !== value.removedLines
  ) {
    return invalid('diff counts');
  }
  return {
    addedLines: value.addedLines,
    after,
    attribution: value.attribution as AgentChangeAttributionV1,
    before,
    contentTruncated: value.contentTruncated,
    hunks,
    operation: value.operation as AgentDiffFileV1['operation'],
    removedLines: value.removedLines,
    sourcePath,
    targetPath,
  };
}

function hunksAreOrdered(hunks: AgentDiffHunkV1[]): boolean {
  return hunks.every((hunk, index) => {
    if (index === 0) return true;
    const prior = hunks[index - 1];
    return (
      hunk.beforeStart >= prior.beforeStart + prior.beforeCount &&
      hunk.afterStart >= prior.afterStart + prior.afterCount
    );
  });
}

function parsePath(value: unknown): AgentInspectionPathV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['displayPath', 'pathHex']) ||
    !isBoundedDisplayText(value.displayPath, MAX_PATH_DISPLAY_BYTES) ||
    !isRepositoryPathHex(value.pathHex)
  ) {
    return invalid('path');
  }
  return { displayPath: value.displayPath, pathHex: value.pathHex };
}

function parseContent(value: unknown): AgentDiffContentV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'contentHash',
      'contentTruncated',
      'encoding',
      'lineEndings',
      'retainedBytes',
      'totalBytes',
    ]) ||
    !isStableId(value.contentHash) ||
    (value.encoding !== 'utf8' && value.encoding !== 'utf8Bom') ||
    !['none', 'lf', 'crlf', 'cr', 'mixed'].includes(value.lineEndings as string) ||
    !isDecimal(value.retainedBytes) ||
    !isDecimal(value.totalBytes) ||
    BigInt(value.retainedBytes) > BigInt(MAX_PATCH_CONTENT_BYTES) ||
    BigInt(value.retainedBytes) > BigInt(value.totalBytes) ||
    typeof value.contentTruncated !== 'boolean' ||
    value.contentTruncated !== BigInt(value.retainedBytes) < BigInt(value.totalBytes)
  ) {
    return invalid('diff content');
  }
  return value as unknown as AgentDiffContentV1;
}

function parseHunk(value: unknown): AgentDiffHunkV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['afterCount', 'afterStart', 'beforeCount', 'beforeStart', 'rows']) ||
    !isU32(value.afterCount) ||
    !isPositiveU32(value.afterStart) ||
    !isU32(value.beforeCount) ||
    !isPositiveU32(value.beforeStart) ||
    !Array.isArray(value.rows) ||
    value.rows.length === 0
  ) {
    return invalid('hunk');
  }
  const rows = value.rows.map(parseRow);
  if (
    rows.filter((row) => row.kind !== 'added').length !== value.beforeCount ||
    rows.filter((row) => row.kind !== 'removed').length !== value.afterCount ||
    !hasSequentialHunkCoordinates(rows, value.beforeStart, value.afterStart)
  ) {
    return invalid('hunk coordinates');
  }
  return {
    afterCount: value.afterCount,
    afterStart: value.afterStart,
    beforeCount: value.beforeCount,
    beforeStart: value.beforeStart,
    rows,
  };
}

function hasSequentialHunkCoordinates(
  rows: AgentDiffRowV1[],
  beforeStart: number,
  afterStart: number,
): boolean {
  let beforeLine = beforeStart;
  let afterLine = afterStart;
  for (const row of rows) {
    if (row.kind === 'context') {
      if (row.beforeLine !== beforeLine || row.afterLine !== afterLine) return false;
      beforeLine += 1;
      afterLine += 1;
    } else if (row.kind === 'removed') {
      if (row.beforeLine !== beforeLine) return false;
      beforeLine += 1;
    } else {
      if (row.afterLine !== afterLine) return false;
      afterLine += 1;
    }
    if (beforeLine > MAX_U32 || afterLine > MAX_U32) return false;
  }
  return true;
}

function parseRow(value: unknown): AgentDiffRowV1 {
  if (!isRecord(value) || !isRecord(value.line)) return invalid('diff row');
  const line = parseLine(value.line);
  if (
    value.kind === 'context' &&
    hasExactKeys(value, ['afterLine', 'beforeLine', 'kind', 'line']) &&
    isPositiveU32(value.afterLine) &&
    isPositiveU32(value.beforeLine)
  ) {
    return { afterLine: value.afterLine, beforeLine: value.beforeLine, kind: 'context', line };
  }
  if (
    value.kind === 'removed' &&
    hasExactKeys(value, ['beforeLine', 'kind', 'line']) &&
    isPositiveU32(value.beforeLine)
  ) {
    return { beforeLine: value.beforeLine, kind: 'removed', line };
  }
  if (
    value.kind === 'added' &&
    hasExactKeys(value, ['afterLine', 'kind', 'line']) &&
    isPositiveU32(value.afterLine)
  ) {
    return { afterLine: value.afterLine, kind: 'added', line };
  }
  return invalid('diff row');
}

function parseLine(value: Record<string, unknown>): AgentDiffLineV1 {
  if (
    !hasExactKeys(value, ['ending', 'text']) ||
    !isBoundedContentText(value.text, MAX_PATCH_CONTENT_BYTES) ||
    !['lf', 'crlf', 'cr', 'none'].includes(value.ending as string)
  ) {
    return invalid('diff line');
  }
  return { ending: value.ending as AgentDiffLineEndingV1, text: value.text };
}

function parseProcess(value: unknown): AgentProcessInspectionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'durationMillis',
      'inspectionId',
      'kind',
      'runId',
      'snapshotId',
      'stderr',
      'stdout',
      'stepId',
      'termination',
      'verificationSpecId',
    ]) ||
    ![
      value.inspectionId,
      value.runId,
      value.snapshotId,
      value.stepId,
      value.verificationSpecId,
    ].every(isStableId) ||
    !['test', 'build', 'diagnostic', 'lint', 'format', 'command'].includes(value.kind as string) ||
    !isDecimal(value.durationMillis)
  ) {
    return invalid('process');
  }
  return {
    ...value,
    stderr: parseProcessStream(value.stderr),
    stdout: parseProcessStream(value.stdout),
    termination: parseTermination(value.termination),
  } as AgentProcessInspectionV1;
}

function parseProcessStream(value: unknown): AgentProcessStreamV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'digest',
      'observedBytes',
      'redaction',
      'retainedBytes',
      'retainedLimit',
      'sourceTruncated',
    ]) ||
    !isStableId(value.digest) ||
    !isDecimal(value.observedBytes) ||
    !isDecimal(value.retainedBytes) ||
    !isPositiveU32(value.retainedLimit) ||
    BigInt(value.retainedBytes) > BigInt(value.retainedLimit) ||
    BigInt(value.retainedBytes) > BigInt(value.observedBytes) ||
    typeof value.sourceTruncated !== 'boolean' ||
    !isRedactionOrNull(value.redaction) ||
    (value.redaction !== null && value.retainedBytes !== '0') ||
    (value.redaction === null &&
      value.sourceTruncated !== BigInt(value.observedBytes) > BigInt(value.retainedBytes)) ||
    (value.sourceTruncated && BigInt(value.observedBytes) <= BigInt(value.retainedBytes))
  ) {
    return invalid('process stream');
  }
  return value as unknown as AgentProcessStreamV1;
}

function parseTermination(value: unknown): AgentProcessTerminationV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalid('termination');
  if (
    value.kind === 'exited' &&
    hasExactKeys(value, ['code', 'kind', 'success']) &&
    (value.code === null ||
      (typeof value.code === 'number' &&
        Number.isInteger(value.code) &&
        value.code >= -2_147_483_648 &&
        value.code <= 2_147_483_647)) &&
    typeof value.success === 'boolean' &&
    value.success === (value.code === 0)
  ) {
    return { code: value.code, kind: 'exited', success: value.success };
  }
  if ((value.kind === 'timedOut' || value.kind === 'cancelled') && hasExactKeys(value, ['kind'])) {
    return { kind: value.kind };
  }
  return invalid('termination');
}

function parseVerification(value: unknown): AgentVerificationInspectionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'criteria',
      'goalRevision',
      'ledgerRevision',
      'ledgerStoreVersion',
      'publishedSnapshotId',
      'steps',
    ]) ||
    !isPositiveU32(value.goalRevision) ||
    !isPositiveU32(value.ledgerRevision) ||
    !isPositiveDecimal(value.ledgerStoreVersion) ||
    !isStableId(value.publishedSnapshotId) ||
    !Array.isArray(value.criteria) ||
    value.criteria.length === 0 ||
    value.criteria.length > MAX_CRITERIA ||
    !Array.isArray(value.steps) ||
    value.steps.length > MAX_STEPS
  ) {
    return invalid('verification');
  }
  const steps = value.steps.map(parseStep);
  const criteria = value.criteria.map(parseCriterion);
  if (
    new Set(steps.map((step) => step.stepId)).size !== steps.length ||
    new Set(steps.map((step) => step.verificationSpecId)).size !== steps.length ||
    new Set(criteria.map((criterion) => criterion.criterionId)).size !== criteria.length ||
    !criteria.some((criterion) => criterion.requirement === 'must')
  ) {
    return invalid('verification identities');
  }
  validateStaleCauses(steps);
  validateCriterionProofs(criteria, steps);
  return {
    criteria,
    goalRevision: value.goalRevision,
    ledgerRevision: value.ledgerRevision,
    ledgerStoreVersion: value.ledgerStoreVersion,
    publishedSnapshotId: value.publishedSnapshotId,
    steps,
  };
}

function parseStep(value: unknown): AgentVerificationStepV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'attempts',
      'intendedOutcome',
      'method',
      'staleCause',
      'status',
      'stepId',
      'verificationSpecId',
    ]) ||
    !isStableId(value.stepId) ||
    !isStableId(value.verificationSpecId) ||
    !isBoundedDomainText(value.intendedOutcome, MAX_STEP_TEXT_BYTES) ||
    !isMethod(value.method) ||
    !isStepStatus(value.status) ||
    !Array.isArray(value.attempts)
  ) {
    return invalid('verification step');
  }
  const attempts = value.attempts.map(parseAttempt);
  if (
    attempts.some((attempt, index) => index > 0 && attempts[index - 1].number >= attempt.number)
  ) {
    return invalid('attempt ordering');
  }
  const staleCause = value.staleCause === null ? null : parseStaleCause(value.staleCause);
  if (
    (value.status === 'stale') !== (staleCause !== null) ||
    ((value.status === 'completed' || value.status === 'stale') &&
      attempts.at(-1)?.outcome.status !== 'passed')
  ) {
    return invalid('step state');
  }
  return {
    attempts,
    intendedOutcome: value.intendedOutcome,
    method: value.method,
    staleCause,
    status: value.status,
    stepId: value.stepId,
    verificationSpecId: value.verificationSpecId,
  };
}

function validateStaleCauses(steps: AgentVerificationStepV1[]): void {
  const stepIds = new Set(steps.map((step) => step.stepId));
  for (const step of steps) {
    if (step.staleCause?.kind === 'dependency') {
      if (step.staleCause.stepId === step.stepId || !stepIds.has(step.staleCause.stepId)) {
        return invalid('stale dependency');
      }
    } else if (step.staleCause?.kind === 'verificationEvidence') {
      const latestIds = new Set(
        step.attempts.at(-1)?.evidence.map((item) => item.evidenceId) ?? [],
      );
      if (step.staleCause.evidenceIds.some((id) => !latestIds.has(id))) {
        return invalid('stale evidence');
      }
    }
  }
}

function parseAttempt(value: unknown): AgentVerificationAttemptV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['evidence', 'number', 'outcome']) ||
    !isPositiveU32(value.number) ||
    !Array.isArray(value.evidence) ||
    value.evidence.length === 0 ||
    value.evidence.length > MAX_EVIDENCE_PER_ATTEMPT
  ) {
    return invalid('verification attempt');
  }
  const evidence = value.evidence.map(parseEvidence);
  if (new Set(evidence.map((item) => item.evidenceId)).size !== evidence.length) {
    return invalid('attempt evidence identities');
  }
  return { evidence, number: value.number, outcome: parseAttemptOutcome(value.outcome) };
}

function parseAttemptOutcome(value: unknown): AgentStepVerificationOutcomeV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalid('attempt outcome');
  if (value.status === 'passed' && hasExactKeys(value, ['status'])) return { status: 'passed' };
  if (
    value.status === 'failed' &&
    hasExactKeys(value, ['status', 'summary']) &&
    isBoundedDomainText(value.summary, MAX_STEP_TEXT_BYTES)
  ) {
    return { status: 'failed', summary: value.summary };
  }
  return invalid('attempt outcome');
}

function parseStaleCause(value: unknown): AgentStepStaleCauseV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalid('stale cause');
  if (
    value.kind === 'verificationEvidence' &&
    hasExactKeys(value, ['evidenceIds', 'kind']) &&
    isUniqueStableIds(value.evidenceIds, true)
  ) {
    return { evidenceIds: value.evidenceIds, kind: 'verificationEvidence' };
  }
  if (
    value.kind === 'dependency' &&
    hasExactKeys(value, ['kind', 'stepId']) &&
    isStableId(value.stepId)
  ) {
    return { kind: 'dependency', stepId: value.stepId };
  }
  return invalid('stale cause');
}

function parseEvidence(value: unknown): AgentVerificationEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'detail',
      'evaluation',
      'evidenceId',
      'freshness',
      'method',
      'runId',
      'snapshotId',
    ]) ||
    !isStableId(value.evidenceId) ||
    !isStableId(value.runId) ||
    !isStableId(value.snapshotId) ||
    !isMethod(value.method)
  ) {
    return invalid('verification evidence');
  }
  const detail = parseEvidenceDetail(value.detail);
  const expectedKind = {
    command: 'command',
    test: 'test',
    diffInvariant: 'diff',
    diagnostic: 'diagnostic',
    userConfirm: 'userConfirmation',
  }[value.method];
  if (
    detail.kind !== expectedKind ||
    (detail.kind === 'diff' && detail.snapshotId !== value.snapshotId)
  ) {
    return invalid('evidence kind');
  }
  return {
    detail,
    evaluation: parseEvaluation(value.evaluation),
    evidenceId: value.evidenceId,
    freshness: parseFreshness(value.freshness),
    method: value.method,
    runId: value.runId,
    snapshotId: value.snapshotId,
  };
}

function parseEvaluation(value: unknown): AgentVerificationEvaluationV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalid('evaluation');
  if (value.status === 'passed' && hasExactKeys(value, ['status'])) return { status: 'passed' };
  if (
    value.status === 'failed' &&
    hasExactKeys(value, ['reason', 'status']) &&
    [
      'legacySpecification',
      'specificationMismatch',
      'evidenceKindMismatch',
      'commandMismatch',
      'processUnsuccessful',
      'missingStructuredTestCases',
      'tooFewPassingTestCases',
      'selectedTestCaseFailed',
      'incompleteChangeSet',
      'diffInvariantMismatch',
      'errorDiagnosticsPresent',
      'warningDiagnosticsPresent',
      'confirmationScopeMismatch',
    ].includes(value.reason as string)
  ) {
    return { reason: value.reason as string, status: 'failed' };
  }
  return invalid('evaluation');
}

function parseFreshness(value: unknown): AgentEvidenceFreshnessV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalid('freshness');
  if (value.status === 'fresh' && hasExactKeys(value, ['status'])) return { status: 'fresh' };
  if (
    value.status === 'stale' &&
    hasExactKeys(value, ['reason', 'status']) &&
    (value.reason === 'snapshotChanged' || value.reason === 'dependencyChanged')
  ) {
    return { reason: value.reason, status: 'stale' };
  }
  return invalid('freshness');
}

function parseEvidenceDetail(value: unknown): AgentVerificationEvidenceDetailV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalid('evidence detail');
  if (value.kind === 'command' && hasExactKeys(value, ['command', 'kind'])) {
    return { command: parseVerificationCommand(value.command), kind: 'command' };
  }
  if (
    value.kind === 'test' &&
    hasExactKeys(value, [
      'cases',
      'casesTruncated',
      'command',
      'failed',
      'ignored',
      'kind',
      'passed',
    ]) &&
    isDecimal(value.failed) &&
    isDecimal(value.ignored) &&
    isDecimal(value.passed) &&
    typeof value.casesTruncated === 'boolean' &&
    Array.isArray(value.cases) &&
    value.cases.length <= MAX_TEST_CASES
  ) {
    const cases = value.cases.map(parseTestCase);
    const count = BigInt(value.failed) + BigInt(value.ignored) + BigInt(value.passed);
    const visible = {
      failed: BigInt(cases.filter((testCase) => testCase.outcome === 'failed').length),
      ignored: BigInt(cases.filter((testCase) => testCase.outcome === 'ignored').length),
      passed: BigInt(cases.filter((testCase) => testCase.outcome === 'passed').length),
    };
    if (
      !isStrictlySortedUtf8(cases.map((testCase) => testCase.name)) ||
      visible.failed > BigInt(value.failed) ||
      visible.ignored > BigInt(value.ignored) ||
      visible.passed > BigInt(value.passed) ||
      (value.casesTruncated
        ? count <= BigInt(cases.length)
        : count !== BigInt(cases.length) ||
          visible.failed !== BigInt(value.failed) ||
          visible.ignored !== BigInt(value.ignored) ||
          visible.passed !== BigInt(value.passed))
    ) {
      return invalid('test case truncation');
    }
    return {
      cases,
      casesTruncated: value.casesTruncated,
      command: parseVerificationCommand(value.command),
      failed: value.failed,
      ignored: value.ignored,
      kind: 'test',
      passed: value.passed,
    };
  }
  if (
    value.kind === 'diff' &&
    hasExactKeys(value, [
      'baseSnapshotId',
      'changedPaths',
      'complete',
      'kind',
      'snapshotId',
      'source',
    ]) &&
    isStableId(value.baseSnapshotId) &&
    isStableId(value.snapshotId) &&
    (value.source === 'patchChangeSet' || value.source === 'publishedIndexes') &&
    typeof value.complete === 'boolean' &&
    Array.isArray(value.changedPaths) &&
    value.changedPaths.length <= MAX_DIFF_EVIDENCE_PATHS
  ) {
    const changedPaths = value.changedPaths.map(parsePath);
    if (
      value.baseSnapshotId === value.snapshotId ||
      !isStrictlySorted(changedPaths.map((path) => path.pathHex))
    ) {
      return invalid('diff paths');
    }
    return {
      baseSnapshotId: value.baseSnapshotId,
      changedPaths,
      complete: value.complete,
      kind: 'diff',
      snapshotId: value.snapshotId,
      source: value.source,
    };
  }
  if (
    value.kind === 'diagnostic' &&
    hasExactKeys(value, ['command', 'errors', 'kind', 'warnings']) &&
    isU32(value.errors) &&
    isU32(value.warnings)
  ) {
    return {
      command: parseVerificationCommand(value.command),
      errors: value.errors,
      kind: 'diagnostic',
      warnings: value.warnings,
    };
  }
  if (
    value.kind === 'userConfirmation' &&
    hasExactKeys(value, ['confirmedAtUnixMillis', 'kind', 'scopeId']) &&
    isDecimal(value.confirmedAtUnixMillis) &&
    isStableId(value.scopeId)
  ) {
    return {
      confirmedAtUnixMillis: value.confirmedAtUnixMillis,
      kind: 'userConfirmation',
      scopeId: value.scopeId,
    };
  }
  return invalid('evidence detail');
}

function parseVerificationCommand(value: unknown): AgentVerificationCommandV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['commandId', 'durationMillis', 'stderr', 'stdout', 'termination']) ||
    !isStableId(value.commandId) ||
    !isDecimal(value.durationMillis)
  ) {
    return invalid('verification command');
  }
  return {
    commandId: value.commandId,
    durationMillis: value.durationMillis,
    stderr: parseVerificationStream(value.stderr),
    stdout: parseVerificationStream(value.stdout),
    termination: parseTermination(value.termination),
  };
}

function parseVerificationStream(value: unknown): AgentVerificationProcessStreamV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'digest',
      'observedBytes',
      'redaction',
      'retainedLimit',
      'sourceTruncated',
    ]) ||
    !isStableId(value.digest) ||
    !isDecimal(value.observedBytes) ||
    !isPositiveU32(value.retainedLimit) ||
    typeof value.sourceTruncated !== 'boolean' ||
    !isRedactionOrNull(value.redaction)
  ) {
    return invalid('verification stream');
  }
  return value as unknown as AgentVerificationProcessStreamV1;
}

function parseTestCase(value: unknown): AgentTestCaseV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['name', 'outcome']) ||
    !isBoundedDomainText(value.name, MAX_TEST_CASE_NAME_BYTES) ||
    !['passed', 'failed', 'ignored'].includes(value.outcome as string)
  ) {
    return invalid('test case');
  }
  return { name: value.name, outcome: value.outcome as AgentTestCaseV1['outcome'] };
}

function parseCriterion(value: unknown): AgentCriterionInspectionV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['criterionId', 'proofState', 'proofs', 'requirement', 'statement']) ||
    !isStableId(value.criterionId) ||
    !isBoundedDomainText(value.statement, MAX_CRITERION_TEXT_BYTES) ||
    (value.requirement !== 'must' && value.requirement !== 'should') ||
    !['proven', 'pending', 'failed', 'stale', 'missing'].includes(value.proofState as string) ||
    !Array.isArray(value.proofs)
  ) {
    return invalid('criterion');
  }
  const proofs = value.proofs.map(parseProof);
  if (
    (value.proofState === 'proven') !== proofs.length > 0 ||
    new Set(proofs.map((proof) => proof.stepId)).size !== proofs.length
  ) {
    return invalid('criterion proofs');
  }
  return {
    criterionId: value.criterionId,
    proofState: value.proofState as AgentCriterionInspectionV1['proofState'],
    proofs,
    requirement: value.requirement,
    statement: value.statement,
  };
}

function parseProof(value: unknown): AgentCriterionProofV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['evidenceIds', 'stepId']) ||
    !isStableId(value.stepId) ||
    !isUniqueStableIds(value.evidenceIds, true)
  ) {
    return invalid('criterion proof');
  }
  return { evidenceIds: value.evidenceIds, stepId: value.stepId };
}

function validateCriterionProofs(
  criteria: AgentCriterionInspectionV1[],
  steps: AgentVerificationStepV1[],
): void {
  const byId = new Map(steps.map((step) => [step.stepId, step]));
  for (const criterion of criteria) {
    for (const proof of criterion.proofs) {
      const step = byId.get(proof.stepId);
      const attempt = step?.attempts.at(-1);
      if (step?.status !== 'completed' || attempt?.outcome.status !== 'passed') {
        return invalid('criterion proof step');
      }
      const evidenceById = new Map(attempt.evidence.map((item) => [item.evidenceId, item]));
      if (
        proof.evidenceIds.length !== attempt.evidence.length ||
        proof.evidenceIds.some((id, index) => {
          const evidence = evidenceById.get(id);
          return (
            attempt.evidence[index]?.evidenceId !== id ||
            evidence?.evaluation.status !== 'passed' ||
            evidence.freshness.status !== 'fresh'
          );
        })
      ) {
        return invalid('criterion proof evidence');
      }
    }
  }
}

function parseLogResult(value: unknown): AgentInspectionLogResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalid('log result');
  if (
    ['noProject', 'unavailable', 'inspectionChanged'].includes(value.status) &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status } as AgentInspectionLogResultV1;
  }
  if (value.status === 'available' && hasExactKeys(value, ['page', 'status'])) {
    return { page: parseLogPage(value.page), status: 'available' };
  }
  return invalid('log result');
}

function parseLogPage(value: unknown): AgentInspectionLogPageV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'nextOffset',
      'offset',
      'pageTruncated',
      'redaction',
      'sourceTruncated',
      'text',
    ]) ||
    !isBoundedContentText(value.text, MAX_LOG_PAGE) ||
    !isU32(value.offset) ||
    (value.nextOffset !== null && !isU32(value.nextOffset)) ||
    typeof value.pageTruncated !== 'boolean' ||
    typeof value.sourceTruncated !== 'boolean' ||
    !isRedactionOrNull(value.redaction) ||
    value.pageTruncated !== (value.nextOffset !== null) ||
    (value.nextOffset !== null && value.nextOffset <= value.offset) ||
    (value.redaction !== null &&
      (value.text !== '' || value.nextOffset !== null || value.pageTruncated))
  ) {
    return invalid('log page');
  }
  return value as unknown as AgentInspectionLogPageV1;
}

function isRepositoryPathHex(value: unknown): value is string {
  if (typeof value !== 'string' || !HEX.test(value) || value.length > MAX_PATH_BYTES * 2)
    return false;
  const bytes = decodeHex(value);
  if (bytes[0] === 47 || bytes.at(-1) === 47 || bytes.includes(0)) return false;
  return bytes
    .reduce<number[][]>(
      (segments, byte) => {
        if (byte === 47) segments.push([]);
        else segments.at(-1)?.push(byte);
        return segments;
      },
      [[]],
    )
    .every(
      (segment) =>
        segment.length > 0 &&
        !(segment.length === 1 && segment[0] === 46) &&
        !(segment.length === 2 && segment[0] === 46 && segment[1] === 46),
    );
}

function decodeHex(value: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index < value.length; index += 2) {
    bytes.push(Number.parseInt(value.slice(index, index + 2), 16));
  }
  return bytes;
}

function isRedactionOrNull(value: unknown): value is AgentProcessRedactionV1 | null {
  return (
    value === null || ['invalidUtf8', 'secretCandidate', 'unsafeControl'].includes(value as string)
  );
}

function isMethod(value: unknown): value is AgentVerificationMethodV1 {
  return ['command', 'test', 'diffInvariant', 'diagnostic', 'userConfirm'].includes(
    value as string,
  );
}

function isStepStatus(value: unknown): value is AgentVerificationStepStatusV1 {
  return [
    'pending',
    'ready',
    'inProgress',
    'blocked',
    'awaitingApproval',
    'verifying',
    'completed',
    'failed',
    'cancelled',
    'stale',
  ].includes(value as string);
}

function isUniqueStableIds(value: unknown, nonempty: boolean): value is string[] {
  return (
    Array.isArray(value) &&
    (!nonempty || value.length > 0) &&
    value.every(isStableId) &&
    new Set(value).size === value.length
  );
}

function isStrictlySorted(values: string[]): boolean {
  return values.every((value, index) => index === 0 || values[index - 1] < value);
}

function isStrictlySortedUtf8(values: string[]): boolean {
  return values.every(
    (value, index) =>
      index === 0 || compareBytes(utf8.encode(values[index - 1]), utf8.encode(value)),
  );
}

function compareBytes(left: Uint8Array, right: Uint8Array): boolean {
  const shared = Math.min(left.length, right.length);
  for (let index = 0; index < shared; index += 1) {
    if (left[index] !== right[index]) return left[index] < right[index];
  }
  return left.length < right.length;
}

function isBoundedDomainText(value: unknown, maximumBytes: number): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value === value.trim() &&
    !value.includes('\r') &&
    utf8.encode(value).length <= maximumBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return code !== undefined && isControlCodePoint(code) && code !== 9 && code !== 10;
    })
  );
}

function isBoundedDisplayText(value: unknown, maximumBytes: number): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    utf8.encode(value).length <= maximumBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return code !== undefined && isControlCodePoint(code);
    })
  );
}

function isBoundedContentText(value: unknown, maximumBytes: number): value is string {
  return (
    typeof value === 'string' &&
    utf8.encode(value).length <= maximumBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return (
        code !== undefined && isControlCodePoint(code) && code !== 9 && code !== 10 && code !== 13
      );
    })
  );
}

function isControlCodePoint(value: number): boolean {
  return value < 32 || (value >= 127 && value <= 159);
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID.test(value);
}

function isDecimal(value: unknown): value is string {
  return typeof value === 'string' && DECIMAL.test(value) && BigInt(value) <= MAX_U64;
}

function isPositiveDecimal(value: unknown): value is string {
  return typeof value === 'string' && POSITIVE_DECIMAL.test(value) && BigInt(value) <= MAX_U64;
}

function isPositiveU32(value: unknown): value is number {
  return isU32(value) && value > 0;
}

function isU32(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= MAX_U32;
}

function invalid(part: string): never {
  throw new Error(`Agent inspection ${part} is invalid.`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const keys = Object.keys(value).sort();
  const sorted = [...expected].sort();
  return keys.length === sorted.length && keys.every((key, index) => key === sorted[index]);
}

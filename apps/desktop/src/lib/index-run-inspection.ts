import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

export type IndexRunStateV1 =
  'queued' | 'running' | 'cancelling' | 'succeeded' | 'failed' | 'cancelled' | 'interrupted';
export type IndexRunTriggerV1 =
  'initialObservation' | 'fileChanges' | 'recoveryRescan' | 'manualRetry';
export type IndexRunPhaseV1 = 'discover' | 'hash' | 'parse' | 'link' | 'rank' | 'publish';
export type IndexRunPhaseStateV1 = 'pending' | 'running' | 'succeeded' | 'failed' | 'cancelled';
export type IndexRunFileFilterV1 =
  | 'all'
  | 'new'
  | 'changed'
  | 'unchanged'
  | 'deleted'
  | 'hashed'
  | 'hashReused'
  | 'structural'
  | 'parseReused'
  | 'generic'
  | 'failed';
export type IndexRunControlActionV1 = 'cancel' | 'retry';
export type IndexRunFailureCodeV1 =
  | 'discovery'
  | 'sourceUnavailable'
  | 'revisionChanged'
  | 'parse'
  | 'link'
  | 'rank'
  | 'publish'
  | 'resourceLimit'
  | 'timeout'
  | 'progressUnavailable'
  | 'journalIncomplete'
  | 'workerUnavailable'
  | 'interrupted';

export interface IndexRunFailureV1 {
  code: IndexRunFailureCodeV1;
  explanation: string;
  recovery: string;
}
export interface IndexRunPathV1 {
  display: string;
  truncated: boolean;
}
export interface IndexRunCountsV1 {
  discovered: string;
  pending: string;
  newFiles: string;
  changed: string;
  unchanged: string;
  deleted: string;
  hashed: string;
  hashReused: string;
  structural: string;
  parseReused: string;
  generic: string;
  failed: string;
}
export interface IndexRunPhaseProgressV1 {
  phase: IndexRunPhaseV1;
  state: IndexRunPhaseStateV1;
  startedAtUnixMillis: string | null;
  endedAtUnixMillis: string | null;
  completed: string | null;
  total: string | null;
}
export interface IndexRunEventV1 {
  revision: string;
  occurredAtUnixMillis: string;
  kind: string;
  phase: IndexRunPhaseV1 | null;
  file: IndexRunPathV1 | null;
  failure: IndexRunFailureV1 | null;
}
export interface IndexRunDetailV1 {
  runRef: string;
  revision: string;
  state: IndexRunStateV1;
  trigger: IndexRunTriggerV1;
  startedAtUnixMillis: string;
  endedAtUnixMillis: string | null;
  lastActivityAtUnixMillis: string;
  durationMillis: string;
  currentPhase: IndexRunPhaseV1 | null;
  currentFile: IndexRunPathV1 | null;
  detailsIncomplete: boolean;
  previousPublicationAvailable: boolean;
  counts: IndexRunCountsV1;
  phases: IndexRunPhaseProgressV1[];
  events: IndexRunEventV1[];
  failure: IndexRunFailureV1 | null;
}
export type IndexRunInspectionResultV1 =
  | { status: 'noProject' }
  | { status: 'noRuns' }
  | {
      status: 'available';
      current: IndexRunDetailV1;
      previous: IndexRunDetailV1 | null;
      serverTimeUnixMillis: string;
      stallThresholdSeconds: number;
    };
export interface IndexRunInspectionResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: IndexRunInspectionResultV1;
}

export interface IndexRunFileV1 {
  path: IndexRunPathV1;
  change: 'pending' | 'new' | 'changed' | 'unchanged' | 'deleted';
  hash: 'pending' | 'hashed' | 'reused' | 'notApplicable';
  parse: 'structural' | 'reused' | 'generic' | 'failed' | 'notApplicable' | null;
  failures: IndexRunFailureV1[];
  failuresTruncated: boolean;
}
export type IndexRunFilesResultV1 =
  | { status: 'noProject' | 'notFound' | 'staleRevision' }
  | {
      status: 'page';
      runRef: string;
      revision: string;
      files: IndexRunFileV1[];
      total: string;
      previousCursor: string | null;
      nextCursor: string | null;
    };
export interface IndexRunFilesResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: IndexRunFilesResultV1;
}
export type IndexRunControlResultV1 =
  'accepted' | 'noProject' | 'notFound' | 'staleRevision' | 'invalidState' | 'busy' | 'unavailable';
export interface IndexRunControlResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: { status: IndexRunControlResultV1 };
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);
const PHASES: IndexRunPhaseV1[] = ['discover', 'hash', 'parse', 'link', 'rank', 'publish'];
const STATES: IndexRunStateV1[] = [
  'queued',
  'running',
  'cancelling',
  'succeeded',
  'failed',
  'cancelled',
  'interrupted',
];
const TRIGGERS: IndexRunTriggerV1[] = [
  'initialObservation',
  'fileChanges',
  'recoveryRescan',
  'manualRetry',
];
const FILTERS: IndexRunFileFilterV1[] = [
  'all',
  'new',
  'changed',
  'unchanged',
  'deleted',
  'hashed',
  'hashReused',
  'structural',
  'parseReused',
  'generic',
  'failed',
];
const FAILURE_CODES: IndexRunFailureCodeV1[] = [
  'discovery',
  'sourceUnavailable',
  'revisionChanged',
  'parse',
  'link',
  'rank',
  'publish',
  'resourceLimit',
  'timeout',
  'progressUnavailable',
  'journalIncomplete',
  'workerUnavailable',
  'interrupted',
];

export async function queryIndexRunInspection(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<IndexRunInspectionResponseV1> {
  return parseIndexRunInspectionResponseV1(
    await invokeCommand('query_index_run_inspection', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
}

export async function queryIndexRunFiles(
  run: IndexRunDetailV1,
  search: string,
  filter: IndexRunFileFilterV1,
  cursor: string | null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<IndexRunFilesResponseV1> {
  if (
    !FILTERS.includes(filter) ||
    new TextEncoder().encode(search).length > 256 ||
    hasControl(search)
  ) {
    throw new Error('Index run file query is outside the supported bounds.');
  }
  return parseIndexRunFilesResponseV1(
    await invokeCommand('query_index_run_files', {
      request: {
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        runRef: run.runRef,
        revision: run.revision,
        search: search.trim() || null,
        filter,
        cursor,
      },
    }),
  );
}

export async function controlIndexRun(
  run: IndexRunDetailV1,
  action: IndexRunControlActionV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<IndexRunControlResponseV1> {
  const payload = await invokeCommand('control_index_run', {
    request: {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      runRef: run.runRef,
      revision: run.revision,
      action,
    },
  });
  return parseControlResponse(payload);
}

export function parseIndexRunInspectionResponseV1(payload: unknown): IndexRunInspectionResponseV1 {
  const root = exactRecord(payload, ['protocolVersion', 'result'], 'Index run response');
  if (root.protocolVersion !== CURRENT_PROTOCOL_VERSION) fail('Index run protocol version');
  const result = exactRecord(root.result, undefined, 'Index run result');
  if (result.status === 'noProject' || result.status === 'noRuns') {
    exactKeys(result, ['status'], 'Index run result');
    return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: { status: result.status } };
  }
  if (result.status !== 'available') fail('Index run result');
  exactKeys(
    result,
    ['current', 'previous', 'serverTimeUnixMillis', 'stallThresholdSeconds', 'status'],
    'Index run result',
  );
  const serverTimeUnixMillis = uintString(result.serverTimeUnixMillis, 'server time');
  if (result.stallThresholdSeconds !== 60) fail('stall threshold');
  const current = parseRun(result.current);
  const previous = result.previous === null ? null : parseRun(result.previous);
  if (
    previous !== null &&
    (['succeeded', 'failed', 'cancelled', 'interrupted'].includes(current.state) ||
      !['succeeded', 'failed', 'cancelled', 'interrupted'].includes(previous.state) ||
      current.runRef === previous.runRef)
  )
    fail('retained run relationship');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      status: 'available',
      current,
      previous,
      serverTimeUnixMillis,
      stallThresholdSeconds: 60,
    },
  };
}

export function parseIndexRunFilesResponseV1(payload: unknown): IndexRunFilesResponseV1 {
  const root = exactRecord(payload, ['protocolVersion', 'result'], 'Index run files response');
  if (root.protocolVersion !== CURRENT_PROTOCOL_VERSION) fail('Index run files protocol version');
  const result = exactRecord(root.result, undefined, 'Index run files result');
  if (
    result.status === 'noProject' ||
    result.status === 'notFound' ||
    result.status === 'staleRevision'
  ) {
    exactKeys(result, ['status'], 'Index run files result');
    return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: { status: result.status } };
  }
  if (result.status !== 'page') fail('Index run files result');
  exactKeys(
    result,
    ['files', 'nextCursor', 'previousCursor', 'revision', 'runRef', 'status', 'total'],
    'Index run files page',
  );
  const files = array(result.files, 'files').map(parseFile);
  if (files.length > 100) fail('file page size');
  const total = uintString(result.total, 'file total');
  if (BigInt(total) < BigInt(files.length)) fail('file total');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      status: 'page',
      runRef: traceRef(result.runRef),
      revision: positiveUintString(result.revision, 'revision'),
      files,
      total,
      previousCursor: nullableBoundedString(result.previousCursor, 256, 'previous cursor'),
      nextCursor: nullableBoundedString(result.nextCursor, 256, 'next cursor'),
    },
  };
}

function parseControlResponse(payload: unknown): IndexRunControlResponseV1 {
  const root = exactRecord(payload, ['protocolVersion', 'result'], 'Index run control response');
  if (root.protocolVersion !== CURRENT_PROTOCOL_VERSION) fail('Index run control protocol version');
  const result = exactRecord(root.result, ['status'], 'Index run control result');
  const allowed: IndexRunControlResultV1[] = [
    'accepted',
    'noProject',
    'notFound',
    'staleRevision',
    'invalidState',
    'busy',
    'unavailable',
  ];
  if (
    typeof result.status !== 'string' ||
    !allowed.includes(result.status as IndexRunControlResultV1)
  )
    fail('Index run control result');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { status: result.status as IndexRunControlResultV1 },
  };
}

function parseRun(value: unknown): IndexRunDetailV1 {
  const run = exactRecord(
    value,
    [
      'counts',
      'currentFile',
      'currentPhase',
      'detailsIncomplete',
      'durationMillis',
      'endedAtUnixMillis',
      'events',
      'failure',
      'lastActivityAtUnixMillis',
      'phases',
      'previousPublicationAvailable',
      'revision',
      'runRef',
      'startedAtUnixMillis',
      'state',
      'trigger',
    ],
    'Index run',
  );
  if (
    !STATES.includes(run.state as IndexRunStateV1) ||
    !TRIGGERS.includes(run.trigger as IndexRunTriggerV1)
  )
    fail('Index run state');
  const phases = array(run.phases, 'phases').map(parsePhase);
  if (phases.length !== 6 || phases.some((phase, index) => phase.phase !== PHASES[index]))
    fail('phase order');
  const ended = nullableUintString(run.endedAtUnixMillis, 'end time');
  const terminal = ['succeeded', 'failed', 'cancelled', 'interrupted'].includes(
    run.state as string,
  );
  if (terminal !== (ended !== null)) fail('Index run timing');
  const events = array(run.events, 'events').map(parseEvent);
  if (events.length > 128) fail('event limit');
  return {
    runRef: traceRef(run.runRef),
    revision: positiveUintString(run.revision, 'revision'),
    state: run.state as IndexRunStateV1,
    trigger: run.trigger as IndexRunTriggerV1,
    startedAtUnixMillis: uintString(run.startedAtUnixMillis, 'start time'),
    endedAtUnixMillis: ended,
    lastActivityAtUnixMillis: uintString(run.lastActivityAtUnixMillis, 'last activity'),
    durationMillis: uintString(run.durationMillis, 'duration'),
    currentPhase: run.currentPhase === null ? null : phase(run.currentPhase),
    currentFile: run.currentFile === null ? null : parsePath(run.currentFile),
    detailsIncomplete: boolean(run.detailsIncomplete, 'details incomplete'),
    previousPublicationAvailable: boolean(run.previousPublicationAvailable, 'previous publication'),
    counts: parseCounts(run.counts),
    phases,
    events,
    failure: run.failure === null ? null : parseFailure(run.failure),
  };
}

function parseCounts(value: unknown): IndexRunCountsV1 {
  const counts = exactRecord(
    value,
    [
      'changed',
      'deleted',
      'discovered',
      'failed',
      'generic',
      'hashReused',
      'hashed',
      'newFiles',
      'pending',
      'parseReused',
      'structural',
      'unchanged',
    ],
    'counts',
  );
  const parsed = Object.fromEntries(
    Object.entries(counts).map(([key, item]) => [key, uintString(item, `count ${key}`)]),
  );
  const discovered = BigInt(parsed.discovered);
  if (
    BigInt(parsed.pending) +
      BigInt(parsed.newFiles) +
      BigInt(parsed.changed) +
      BigInt(parsed.unchanged) !==
    discovered
  )
    fail('contradictory file counters');
  if (
    BigInt(parsed.pending) + BigInt(parsed.hashed) + BigInt(parsed.hashReused) !== discovered ||
    BigInt(parsed.structural) +
      BigInt(parsed.parseReused) +
      BigInt(parsed.generic) +
      BigInt(parsed.failed) >
      discovered
  )
    fail('contradictory work counters');
  return parsed as unknown as IndexRunCountsV1;
}

function parsePhase(value: unknown): IndexRunPhaseProgressV1 {
  const row = exactRecord(
    value,
    ['completed', 'endedAtUnixMillis', 'phase', 'startedAtUnixMillis', 'state', 'total'],
    'phase',
  );
  const states: IndexRunPhaseStateV1[] = ['pending', 'running', 'succeeded', 'failed', 'cancelled'];
  if (!states.includes(row.state as IndexRunPhaseStateV1)) fail('phase state');
  const completed = nullableUintString(row.completed, 'phase completed');
  const total = nullableUintString(row.total, 'phase total');
  if (
    (completed === null) !== (total === null) ||
    (completed !== null && BigInt(completed) > BigInt(total!))
  )
    fail('phase progress');
  return {
    phase: phase(row.phase),
    state: row.state as IndexRunPhaseStateV1,
    startedAtUnixMillis: nullableUintString(row.startedAtUnixMillis, 'phase start'),
    endedAtUnixMillis: nullableUintString(row.endedAtUnixMillis, 'phase end'),
    completed,
    total,
  };
}

function parseEvent(value: unknown): IndexRunEventV1 {
  const event = exactRecord(
    value,
    ['failure', 'file', 'kind', 'occurredAtUnixMillis', 'phase', 'revision'],
    'event',
  );
  const kinds = [
    'queued',
    'started',
    'phaseStarted',
    'progress',
    'fileObserved',
    'diagnostic',
    'cancellationRequested',
    'succeeded',
    'failed',
    'cancelled',
    'interrupted',
  ];
  if (typeof event.kind !== 'string' || !kinds.includes(event.kind)) fail('event kind');
  return {
    revision: positiveUintString(event.revision, 'event revision'),
    occurredAtUnixMillis: uintString(event.occurredAtUnixMillis, 'event time'),
    kind: event.kind,
    phase: event.phase === null ? null : phase(event.phase),
    file: event.file === null ? null : parsePath(event.file),
    failure: event.failure === null ? null : parseFailure(event.failure),
  };
}

function parseFile(value: unknown): IndexRunFileV1 {
  const file = exactRecord(
    value,
    ['change', 'failures', 'failuresTruncated', 'hash', 'parse', 'path'],
    'file',
  );
  if (
    !['pending', 'new', 'changed', 'unchanged', 'deleted'].includes(file.change as string) ||
    !['pending', 'hashed', 'reused', 'notApplicable'].includes(file.hash as string) ||
    (file.parse !== null &&
      !['structural', 'reused', 'generic', 'failed', 'notApplicable'].includes(
        file.parse as string,
      ))
  )
    fail('file outcome');
  if (
    (file.change === 'pending' && (file.hash !== 'pending' || file.parse !== null)) ||
    (file.change === 'deleted' &&
      (file.hash !== 'notApplicable' || file.parse !== 'notApplicable')) ||
    (!['pending', 'deleted'].includes(file.change as string) &&
      (!['hashed', 'reused'].includes(file.hash as string) || file.parse === 'notApplicable'))
  )
    fail('contradictory file outcome');
  const failures = array(file.failures, 'file failures').map(parseFailure);
  if (failures.length > 8) fail('file failure limit');
  return {
    path: parsePath(file.path),
    change: file.change as IndexRunFileV1['change'],
    hash: file.hash as IndexRunFileV1['hash'],
    parse: file.parse as IndexRunFileV1['parse'],
    failures,
    failuresTruncated: boolean(file.failuresTruncated, 'failures truncated'),
  };
}

function parseFailure(value: unknown): IndexRunFailureV1 {
  const failure = exactRecord(value, ['code', 'explanation', 'recovery'], 'failure');
  if (!FAILURE_CODES.includes(failure.code as IndexRunFailureCodeV1)) fail('failure code');
  return {
    code: failure.code as IndexRunFailureCodeV1,
    explanation: boundedNonEmptyText(failure.explanation, 512, 'failure explanation'),
    recovery: boundedNonEmptyText(failure.recovery, 512, 'failure recovery'),
  };
}
function parsePath(value: unknown): IndexRunPathV1 {
  const path = exactRecord(value, ['display', 'truncated'], 'path');
  return {
    display: boundedNonEmptyText(path.display, 512, 'path'),
    truncated: boolean(path.truncated, 'path truncation'),
  };
}
function phase(value: unknown): IndexRunPhaseV1 {
  if (typeof value !== 'string' || !PHASES.includes(value as IndexRunPhaseV1)) fail('phase');
  return value as IndexRunPhaseV1;
}
function traceRef(value: unknown): string {
  if (typeof value !== 'string' || !/^[0-9a-f]{64}$/.test(value)) fail('run reference');
  return value;
}
function uintString(value: unknown, label: string): string {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) fail(label);
  return value;
}
function positiveUintString(value: unknown, label: string): string {
  const parsed = uintString(value, label);
  if (parsed === '0') fail(label);
  return parsed;
}
function nullableUintString(value: unknown, label: string): string | null {
  return value === null ? null : uintString(value, label);
}
function nullableBoundedString(value: unknown, max: number, label: string): string | null {
  return value === null ? null : boundedText(value, max, label);
}
function boundedText(value: unknown, max: number, label: string): string {
  if (typeof value !== 'string' || value.length > max || hasControl(value)) fail(label);
  return value;
}
function boundedNonEmptyText(value: unknown, max: number, label: string): string {
  const parsed = boundedText(value, max, label);
  if (parsed.length === 0) fail(label);
  return parsed;
}
function boolean(value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') fail(label);
  return value;
}
function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) fail(label);
  return value;
}
function hasControl(value: string): boolean {
  return [...value].some((character) => {
    const codePoint = character.codePointAt(0) ?? 0;
    return codePoint <= 31 || (codePoint >= 127 && codePoint <= 159);
  });
}
function exactRecord(
  value: unknown,
  keys: string[] | undefined,
  label: string,
): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) fail(label);
  const record = value as Record<string, unknown>;
  if (keys) exactKeys(record, keys, label);
  return record;
}
function exactKeys(value: Record<string, unknown>, expected: string[], label: string): void {
  const keys = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (keys.length !== wanted.length || keys.some((key, index) => key !== wanted[index]))
    fail(label);
}
function fail(label: string): never {
  throw new Error(`${label} does not match the V1 schema.`);
}

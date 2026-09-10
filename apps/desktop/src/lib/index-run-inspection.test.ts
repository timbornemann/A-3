import { describe, expect, it, vi } from 'vitest';
import {
  controlIndexRun,
  parseIndexRunFilesResponseV1,
  parseIndexRunInspectionResponseV1,
  queryIndexRunFiles,
  type IndexRunDetailV1,
} from './index-run-inspection';

const run: IndexRunDetailV1 = {
  runRef: 'a'.repeat(64),
  revision: '7',
  state: 'running',
  trigger: 'fileChanges',
  startedAtUnixMillis: '1000',
  endedAtUnixMillis: null,
  lastActivityAtUnixMillis: '2000',
  durationMillis: '1000',
  currentPhase: 'parse',
  currentFile: { display: 'src/main.rs', truncated: false },
  detailsIncomplete: false,
  previousPublicationAvailable: true,
  counts: {
    discovered: '3',
    pending: '0',
    newFiles: '1',
    changed: '1',
    unchanged: '1',
    deleted: '0',
    hashed: '2',
    hashReused: '1',
    structural: '1',
    parseReused: '1',
    generic: '0',
    failed: '0',
  },
  phases: ['discover', 'hash', 'parse', 'link', 'rank', 'publish'].map((phase, index) => ({
    phase: phase as IndexRunDetailV1['phases'][number]['phase'],
    state: index < 2 ? 'succeeded' : index === 2 ? 'running' : 'pending',
    startedAtUnixMillis: index <= 2 ? String(1000 + index) : null,
    endedAtUnixMillis: index < 2 ? String(1001 + index) : null,
    completed: index === 2 ? '1' : null,
    total: index === 2 ? '3' : null,
  })) as IndexRunDetailV1['phases'],
  events: [],
  failure: null,
};

const response = {
  protocolVersion: 1,
  result: {
    status: 'available',
    current: run,
    previous: null,
    serverTimeUnixMillis: '3000',
    stallThresholdSeconds: 60,
  },
} as const;

describe('Fast-Index run inspection V1 boundary', () => {
  it('accepts the exact six-phase projection', () => {
    expect(parseIndexRunInspectionResponseV1(response)).toEqual(response);
  });

  it('rejects unknown fields, unsafe paths, and contradictory counters', () => {
    expect(() => parseIndexRunInspectionResponseV1({ ...response, secret: 'x' })).toThrow();
    expect(() =>
      parseIndexRunInspectionResponseV1({
        ...response,
        result: {
          ...response.result,
          current: { ...run, currentFile: { display: 'src/\u0000.rs', truncated: false } },
        },
      }),
    ).toThrow();
    expect(() =>
      parseIndexRunInspectionResponseV1({
        ...response,
        result: {
          ...response.result,
          current: {
            ...run,
            failure: { code: 'rawAdapterError', explanation: 'x', recovery: 'y' },
          },
        },
      }),
    ).toThrow();
    expect(() =>
      parseIndexRunInspectionResponseV1({
        ...response,
        result: {
          ...response.result,
          current: { ...run, counts: { ...run.counts, discovered: '9' } },
        },
      }),
    ).toThrow();
  });

  it('enforces the fixed file-page bound and strict row shape', () => {
    const file = {
      path: { display: 'src/main.rs', truncated: false },
      change: 'changed',
      hash: 'hashed',
      parse: 'structural',
      failures: [],
      failuresTruncated: false,
    };
    const page = {
      protocolVersion: 1,
      result: {
        status: 'page',
        runRef: run.runRef,
        revision: '7',
        files: [file],
        total: '1',
        previousCursor: null,
        nextCursor: null,
      },
    };
    expect(parseIndexRunFilesResponseV1(page)).toEqual(page);
    expect(() =>
      parseIndexRunFilesResponseV1({
        ...page,
        result: { ...page.result, files: Array(101).fill(file), total: '101' },
      }),
    ).toThrow();
    expect(() =>
      parseIndexRunFilesResponseV1({
        ...page,
        result: { ...page.result, files: [{ ...file, absolutePath: 'C:\\secret' }] },
      }),
    ).toThrow();
  });

  it('sends only revision-bound actions and bounded file selectors', async () => {
    const invoke = vi.fn(async (command: string) =>
      command === 'control_index_run'
        ? { protocolVersion: 1, result: { status: 'accepted' } }
        : {
            protocolVersion: 1,
            result: {
              status: 'page',
              runRef: run.runRef,
              revision: run.revision,
              files: [],
              total: '0',
              previousCursor: null,
              nextCursor: null,
            },
          },
    );
    await controlIndexRun(run, 'cancel', invoke);
    await queryIndexRunFiles(run, ' src ', 'changed', null, invoke);
    expect(invoke).toHaveBeenNthCalledWith(1, 'control_index_run', {
      request: { protocolVersion: 1, runRef: run.runRef, revision: '7', action: 'cancel' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'query_index_run_files', {
      request: {
        protocolVersion: 1,
        runRef: run.runRef,
        revision: '7',
        search: 'src',
        filter: 'changed',
        cursor: null,
      },
    });
  });
});

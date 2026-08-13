import { describe, expect, it, vi } from 'vitest';
import {
  parseAgentInspectionLogResponseV1,
  parseAgentInspectionResponseV1,
  queryAgentInspectionLog,
} from './agent-inspection';

const id = (value: string): string => value.repeat(64);
const pathHex = (value: string): string =>
  Array.from(new TextEncoder().encode(value), (byte) => byte.toString(16).padStart(2, '0')).join(
    '',
  );

function freshEvidence() {
  return {
    detail: {
      confirmedAtUnixMillis: '1786000000000',
      kind: 'userConfirmation',
      scopeId: id('8'),
    },
    evaluation: { status: 'passed' },
    evidenceId: id('7'),
    freshness: { status: 'fresh' },
    method: 'userConfirm',
    runId: id('2'),
    snapshotId: id('5'),
  };
}

function response() {
  const evidence = freshEvidence();
  return {
    protocolVersion: 1,
    result: {
      inspection: {
        inspectionRevision: '3',
        patch: {
          files: [
            {
              addedLines: 1,
              after: {
                contentHash: id('a'),
                contentTruncated: false,
                encoding: 'utf8',
                lineEndings: 'lf',
                retainedBytes: '8',
                totalBytes: '8',
              },
              attribution: 'proposedAgent',
              before: {
                contentHash: id('b'),
                contentTruncated: false,
                encoding: 'utf8',
                lineEndings: 'lf',
                retainedBytes: '8',
                totalBytes: '8',
              },
              contentTruncated: false,
              hunks: [
                {
                  afterCount: 2,
                  afterStart: 1,
                  beforeCount: 2,
                  beforeStart: 1,
                  rows: [
                    {
                      afterLine: 1,
                      beforeLine: 1,
                      kind: 'context',
                      line: { ending: 'lf', text: 'same' },
                    },
                    {
                      beforeLine: 2,
                      kind: 'removed',
                      line: { ending: 'lf', text: 'old' },
                    },
                    {
                      afterLine: 2,
                      kind: 'added',
                      line: { ending: 'lf', text: 'new' },
                    },
                  ],
                },
              ],
              operation: 'update',
              removedLines: 1,
              sourcePath: { displayPath: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
              targetPath: { displayPath: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
            },
          ],
          inspectionId: id('1'),
          retainedBytes: '16',
          runId: id('2'),
          snapshotId: id('5'),
          stepId: id('3'),
          verificationSpecId: id('4'),
        },
        processes: [
          {
            durationMillis: '9',
            inspectionId: id('6'),
            kind: 'test',
            runId: id('2'),
            snapshotId: id('5'),
            stderr: {
              digest: id('d'),
              observedBytes: '0',
              redaction: null,
              retainedBytes: '0',
              retainedLimit: 1024,
              sourceTruncated: false,
            },
            stdout: {
              digest: id('c'),
              observedBytes: '12',
              redaction: null,
              retainedBytes: '8',
              retainedLimit: 8,
              sourceTruncated: true,
            },
            stepId: id('3'),
            termination: { code: 0, kind: 'exited', success: true },
            verificationSpecId: id('4'),
          },
        ],
        verification: {
          criteria: [
            {
              criterionId: id('9'),
              proofState: 'proven',
              proofs: [{ evidenceIds: [id('7')], stepId: id('3') }],
              requirement: 'must',
              statement: 'the exact scope is proven',
            },
          ],
          goalRevision: 1,
          ledgerRevision: 2,
          ledgerStoreVersion: '7',
          publishedSnapshotId: id('5'),
          steps: [
            {
              attempts: [{ evidence: [evidence], number: 1, outcome: { status: 'passed' } }],
              intendedOutcome: 'verify exact scope',
              method: 'userConfirm',
              staleCause: null,
              status: 'completed',
              stepId: id('3'),
              verificationSpecId: id('4'),
            },
          ],
        },
      },
      status: 'available',
    },
  };
}

describe('agent inspection V1', () => {
  it('accepts one shared diff row model for unified and side-by-side rendering', () => {
    const parsed = parseAgentInspectionResponseV1(response());
    if (parsed.result.status !== 'available') throw new Error('available fixture required');
    const file = parsed.result.inspection.patch?.files[0];
    expect(file?.hunks[0].rows.map((row) => row.kind)).toEqual(['context', 'removed', 'added']);
    expect(file?.addedLines).toBe(1);
    expect(file?.removedLines).toBe(1);
  });

  it('rejects inconsistent hunk line coordinates instead of rendering a false exact diff', () => {
    const payload = response();
    payload.result.inspection.patch.files[0].hunks[0].rows[2] = {
      afterLine: 3,
      kind: 'added',
      line: { ending: 'lf', text: 'new' },
    } as never;
    expect(() => parseAgentInspectionResponseV1(payload)).toThrow(/hunk coordinates/u);
  });

  it('rejects a proven Must criterion when its referenced artifact becomes stale', () => {
    const payload = response();
    payload.result.inspection.verification.steps[0].attempts[0].evidence[0].freshness = {
      reason: 'snapshotChanged',
      status: 'stale',
    } as never;
    expect(() => parseAgentInspectionResponseV1(payload)).toThrow(/proof evidence/u);
  });

  it('requires a Done proof to name every artifact from the passed verification attempt', () => {
    const payload = response();
    payload.result.inspection.verification.steps[0].attempts[0].evidence.push({
      ...freshEvidence(),
      evidenceId: id('e'),
    } as never);

    expect(() => parseAgentInspectionResponseV1(payload)).toThrow(/proof evidence/u);
  });

  it('accepts strictly increasing verification attempt numbers with earlier non-verification gaps', () => {
    const payload = response();
    payload.result.inspection.verification.steps[0].attempts[0].number = 2;

    const parsed = parseAgentInspectionResponseV1(payload);
    expect(
      parsed.result.status === 'available' &&
        parsed.result.inspection.verification.steps[0].attempts[0].number,
    ).toBe(2);
  });

  it('rejects volatile data outside the current published step and snapshot anchors', () => {
    const payload = response();
    payload.result.inspection.patch.snapshotId = id('e');

    expect(() => parseAgentInspectionResponseV1(payload)).toThrow(/volatile anchor/u);
  });

  it('accepts all 128 domain-bounded changed paths in exact diff evidence', () => {
    const payload = response();
    const changedPaths = Array.from({ length: 128 }, (_, index) => {
      const displayPath = `src/${index.toString().padStart(3, '0')}.rs`;
      return { displayPath, pathHex: pathHex(displayPath) };
    });
    const evidence = payload.result.inspection.verification.steps[0].attempts[0].evidence[0];
    evidence.method = 'diffInvariant';
    evidence.detail = {
      baseSnapshotId: id('b'),
      changedPaths,
      complete: true,
      kind: 'diff',
      snapshotId: id('5'),
      source: 'publishedIndexes',
    } as never;
    payload.result.inspection.verification.steps[0].method = 'diffInvariant';

    const parsed = parseAgentInspectionResponseV1(payload);
    expect(
      parsed.result.status === 'available' &&
        parsed.result.inspection.verification.steps[0].attempts[0].evidence[0].detail.kind ===
          'diff' &&
        parsed.result.inspection.verification.steps[0].attempts[0].evidence[0].detail.changedPaths
          .length,
    ).toBe(128);
  });

  it('keeps retained-page and source truncation independent', () => {
    const retained = parseAgentInspectionLogResponseV1({
      protocolVersion: 1,
      result: {
        page: {
          nextOffset: 4,
          offset: 0,
          pageTruncated: true,
          redaction: null,
          sourceTruncated: false,
          text: 'test',
        },
        status: 'available',
      },
    });
    const overflow = parseAgentInspectionLogResponseV1({
      protocolVersion: 1,
      result: {
        page: {
          nextOffset: null,
          offset: 4,
          pageTruncated: false,
          redaction: null,
          sourceTruncated: true,
          text: 'done',
        },
        status: 'available',
      },
    });
    expect(retained.result.status === 'available' && retained.result.page.nextOffset).toBe(4);
    expect(overflow.result.status === 'available' && overflow.result.page.sourceTruncated).toBe(
      true,
    );
  });

  it('never accepts text in a redacted log page', () => {
    expect(() =>
      parseAgentInspectionLogResponseV1({
        protocolVersion: 1,
        result: {
          page: {
            nextOffset: null,
            offset: 0,
            pageTruncated: false,
            redaction: 'secretCandidate',
            sourceTruncated: false,
            text: 'token=secret',
          },
          status: 'available',
        },
      }),
    ).toThrow(/log page/u);
  });

  it('sends only task-bound Core-emitted selectors for a log page', async () => {
    const invoke = vi.fn(async () => ({ protocolVersion: 1, result: { status: 'unavailable' } }));
    await queryAgentInspectionLog(id('0'), '3', id('6'), 'stdout', 4, 8192, invoke);
    expect(invoke).toHaveBeenCalledWith('query_agent_inspection_log', {
      request: {
        inspectionId: id('6'),
        inspectionRevision: '3',
        limit: 8192,
        offset: 4,
        protocolVersion: 1,
        stream: 'stdout',
        taskId: id('0'),
      },
    });
  });

  it('rejects a log page that does not match the requested cursor', async () => {
    const invoke = vi.fn(async () => ({
      protocolVersion: 1,
      result: {
        page: {
          nextOffset: null,
          offset: 0,
          pageTruncated: false,
          redaction: null,
          sourceTruncated: false,
          text: 'wrong page',
        },
        status: 'available',
      },
    }));

    await expect(
      queryAgentInspectionLog(id('0'), '3', id('6'), 'stdout', 4, 8192, invoke),
    ).rejects.toThrow(/requested cursor/u);
  });

  it('rejects page limits that cannot advance across one UTF-8 scalar', async () => {
    const invoke = vi.fn();

    await expect(
      queryAgentInspectionLog(id('0'), '3', id('6'), 'stdout', 0, 3, invoke),
    ).rejects.toThrow(/selection does not match/u);
    expect(invoke).not.toHaveBeenCalled();
  });
});

import type { AgentInspectionResponseV1 } from './agent-inspection';
const id = (value: string): string => value.repeat(64);
const stepId = id('3');
const evidenceId = id('7');

export function inspection(): AgentInspectionResponseV1 {
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
                  afterCount: 1,
                  afterStart: 1,
                  beforeCount: 1,
                  beforeStart: 1,
                  rows: [
                    { beforeLine: 1, kind: 'removed', line: { ending: 'lf', text: 'old' } },
                    { afterLine: 1, kind: 'added', line: { ending: 'lf', text: 'new' } },
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
          stepId,
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
              retainedLimit: 8,
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
            stepId,
            termination: { code: 0, kind: 'exited', success: true },
            verificationSpecId: id('4'),
          },
        ],
        verification: {
          criteria: [
            {
              criterionId: id('9'),
              proofState: 'proven',
              proofs: [{ evidenceIds: [evidenceId], stepId }],
              requirement: 'must',
              statement: 'Der genaue Scope ist bewiesen.',
            },
          ],
          goalRevision: 1,
          ledgerRevision: 2,
          ledgerStoreVersion: '7',
          publishedSnapshotId: id('5'),
          steps: [
            {
              attempts: [
                {
                  evidence: [
                    {
                      detail: {
                        confirmedAtUnixMillis: '1786000000000',
                        kind: 'userConfirmation',
                        scopeId: id('8'),
                      },
                      evaluation: { status: 'passed' },
                      evidenceId,
                      freshness: { status: 'fresh' },
                      method: 'userConfirm',
                      runId: id('2'),
                      snapshotId: id('5'),
                    },
                  ],
                  number: 1,
                  outcome: { status: 'passed' },
                },
              ],
              intendedOutcome: 'Exakten Scope verifizieren',
              method: 'userConfirm',
              staleCause: null,
              status: 'completed',
              stepId,
              verificationSpecId: id('4'),
            },
          ],
        },
      },
      status: 'available',
    },
  };
}

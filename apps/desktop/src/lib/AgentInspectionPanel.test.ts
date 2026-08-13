import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AgentInspectionLogResponseV1, AgentInspectionResponseV1 } from './agent-inspection';
import AgentInspectionPanel from './AgentInspectionPanel.svelte';

const id = (value: string): string => value.repeat(64);
const taskId = id('0');
const stepId = id('3');
const evidenceId = id('7');

function inspection(): AgentInspectionResponseV1 {
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

describe('AgentInspectionPanel', () => {
  it('renders exact paths and the same hunk in unified and side-by-side layouts', async () => {
    const loader = vi.fn(async () => inspection());
    render(AgentInspectionPanel, { loader, taskId });

    expect(await screen.findByText('src/lib.rs')).toBeTruthy();
    expect(screen.getByText('Vom Agenten vorgeschlagen')).toBeTruthy();
    expect(screen.getByRole('table', { name: 'Unified Diff' })).toBeTruthy();
    expect(screen.getByText('old')).toBeTruthy();
    expect(screen.getByText('new')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Side-by-side' }));
    expect(screen.getByRole('table', { name: 'Side-by-side Diff' })).toBeTruthy();
    expect(screen.getByText('old')).toBeTruthy();
    expect(screen.getByText('new')).toBeTruthy();
    expect(loader).toHaveBeenCalledWith(taskId);
  });

  it.each([
    ['appliedAgent' as const, 'Vom Agenten angewendet'],
    ['external' as const, 'Extern beobachtet'],
    ['unattributed' as const, 'Urheber nicht zuverlässig bestimmt'],
  ])('labels %s provenance without inventing a user attribution', async (attribution, label) => {
    const current = inspection();
    if (current.result.status !== 'available' || current.result.inspection.patch === null) {
      throw new Error('patch fixture required');
    }
    current.result.inspection.patch.files[0].attribution = attribution;

    render(AgentInspectionPanel, { loader: async () => current, taskId });

    expect(await screen.findByText(label)).toBeTruthy();
  });

  it('loads bounded logs only on demand and continues from the Core-provided offset', async () => {
    const pages: AgentInspectionLogResponseV1[] = [
      {
        protocolVersion: 1,
        result: {
          page: {
            nextOffset: 4,
            offset: 0,
            pageTruncated: true,
            redaction: null,
            sourceTruncated: true,
            text: 'part',
          },
          status: 'available',
        },
      },
      {
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
      },
    ];
    const logLoader = vi
      .fn<
        (
          selectedTaskId: string,
          revision: string,
          inspectionId: string,
          stream: 'stdout' | 'stderr',
          offset: number,
        ) => Promise<AgentInspectionLogResponseV1>
      >()
      .mockResolvedValueOnce(pages[0])
      .mockResolvedValueOnce(pages[1]);
    render(AgentInspectionPanel, { loader: async () => inspection(), logLoader, taskId });

    const load = await screen.findByRole('button', { name: 'stdout-Log gezielt laden' });
    expect(logLoader).not.toHaveBeenCalled();
    expect(screen.getByText(/dauerhaft verworfen/u)).toBeTruthy();
    await fireEvent.click(load);
    expect(await screen.findByText('part')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Nächste stdout-Logseite laden' }));
    expect(await screen.findByText('partdone')).toBeTruthy();
    expect(logLoader.mock.calls.map((call) => call[4])).toEqual([0, 4]);
  });

  it('shows exact Must proofs and never hides stale verification', async () => {
    const current = inspection();
    render(AgentInspectionPanel, { loader: async () => current, taskId });

    expect(await screen.findByText(/Done · alle Muss-Kriterien/u)).toBeTruthy();
    expect(screen.getAllByText(stepId)).not.toHaveLength(0);
    expect(screen.getAllByText(evidenceId)).not.toHaveLength(0);

    if (current.result.status !== 'available') throw new Error('available fixture required');
    current.result.inspection.verification.criteria[0] = {
      ...current.result.inspection.verification.criteria[0],
      proofState: 'stale',
      proofs: [],
    };
    current.result.inspection.verification.steps[0] = {
      ...current.result.inspection.verification.steps[0],
      staleCause: { evidenceIds: [evidenceId], kind: 'verificationEvidence' },
      status: 'stale',
    };
    await fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
    await waitFor(() => expect(screen.getAllByText('Veraltet')).not.toHaveLength(0));
    expect(screen.getByText(/Verification ist stale/u)).toBeTruthy();
    expect(screen.getByText(/Done ist nicht belegt/u)).toBeTruthy();
  });

  it('reveals exact changed paths retained by diff evidence', async () => {
    const current = inspection();
    if (current.result.status !== 'available') throw new Error('available fixture required');
    const step = current.result.inspection.verification.steps[0];
    step.method = 'diffInvariant';
    step.attempts[0].evidence[0] = {
      ...step.attempts[0].evidence[0],
      detail: {
        baseSnapshotId: id('b'),
        changedPaths: [{ displayPath: 'src/lib.rs', pathHex: '7372632f6c69622e7273' }],
        complete: true,
        kind: 'diff',
        snapshotId: id('5'),
        source: 'patchChangeSet',
      },
      method: 'diffInvariant',
    };

    render(AgentInspectionPanel, { loader: async () => current, taskId });

    expect(await screen.findByText('1 tatsächlich geänderte Pfade')).toBeTruthy();
    await fireEvent.click(screen.getByText('Exakte geänderte Pfade'));
    expect(screen.getAllByText('7372632f6c69622e7273')).not.toHaveLength(0);
  });
});

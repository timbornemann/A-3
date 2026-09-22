import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AgentInspectionLogResponseV1, AgentInspectionResponseV1 } from './agent-inspection';
import AgentInspectionPanel from './AgentInspectionPanel.svelte';
import { inspection } from './agent-inspection.fixture';

const id = (value: string): string => value.repeat(64);
const taskId = id('0');
const stepId = id('3');
const evidenceId = id('7');

describe('AgentInspectionPanel', () => {
  it('retains the diff and chosen layout across delayed refreshes and read conflicts', async () => {
    let finish: ((response: AgentInspectionResponseV1) => void) | undefined;
    const loader = vi
      .fn()
      .mockResolvedValueOnce(inspection())
      .mockImplementation(
        () =>
          new Promise<AgentInspectionResponseV1>((resolve) => {
            finish = resolve;
          }),
      );
    const { rerender } = render(AgentInspectionPanel, { taskId, loader, refreshKey: 0 });
    await screen.findByText('src/lib.rs');
    await fireEvent.click(screen.getByRole('button', { name: 'Nebeneinander' }));
    const table = screen.getByRole('table', { name: 'Side-by-side Diff' });
    await rerender({ refreshKey: 1 });
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(2));
    expect(screen.getByRole('table', { name: 'Side-by-side Diff' })).toBe(table);
    finish?.({ protocolVersion: 1, result: { status: 'inspectionChanged' } });
    await screen.findByText(/Letzter bestätigter Stand/);
    expect(screen.getByRole('table', { name: 'Side-by-side Diff' })).toBe(table);
    await rerender({ refreshKey: 2 });
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(3));
    finish?.(inspection());
    await screen.findByText('Zuletzt bestätigte Änderungen und Nachweise.');
    expect(screen.getByRole('table', { name: 'Side-by-side Diff' })).toBe(table);
  });

  it('discards a delayed inspection from a previously selected task', async () => {
    let finish: ((response: AgentInspectionResponseV1) => void) | undefined;
    const loader = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<AgentInspectionResponseV1>((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue({ protocolVersion: 1, result: { status: 'taskNotFound' } });
    const { rerender } = render(AgentInspectionPanel, { taskId, loader });
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(1));
    await rerender({ taskId: id('f') });
    await screen.findByText('Für diesen Task ist keine Inspektion verfügbar.');
    finish?.(inspection());
    await waitFor(() => expect(screen.queryByText('src/lib.rs')).toBeNull());
  });

  it('renders exact paths and the same hunk in unified and side-by-side layouts', async () => {
    const loader = vi.fn(async () => inspection());
    render(AgentInspectionPanel, { loader, taskId });

    expect(await screen.findByText('src/lib.rs')).toBeTruthy();
    expect(screen.getByText('Vom Agenten vorgeschlagen')).toBeTruthy();
    expect(screen.getByRole('table', { name: 'Unified Diff' })).toBeTruthy();
    expect(screen.getByText('old')).toBeTruthy();
    expect(screen.getByText('new')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Nebeneinander' }));
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

    expect(await screen.findByText(/Abschluss belegt · alle Muss-Kriterien/u)).toBeTruthy();
    expect(screen.getByText('Nachweise anzeigen · 1').closest('details')?.open).toBe(false);
    expect(screen.getByText('Technischer Prüfstand').closest('details')?.open).toBe(false);
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
    expect(screen.getByText(/Prüfung ist veraltet/u)).toBeTruthy();
    expect(screen.getByText(/Abschluss noch nicht belegt/u)).toBeTruthy();
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

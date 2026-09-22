import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AgentApprovalControlResponseV1, AgentApprovalResponseV1 } from './agent-approval';
import { patchApprovalResponse } from './agent-approval.fixture';
import AgentApprovalCenter from './AgentApprovalCenter.svelte';
import projections from './agent-execution-contract.fixture.json';
import { parseAgentApprovalResponseV1 } from './agent-approval';

const taskId = '1'.repeat(64);

describe('AgentApprovalCenter', () => {
  it('renders the real process approval with exact arguments and readable execution details', async () => {
    const response = parseAgentApprovalResponseV1(
      projections.filter((projection) => projection.kind === 'approval')[1].response,
    );
    render(AgentApprovalCenter, { taskId, loader: async () => response });
    await screen.findByRole('heading', { name: 'Dieser Befehl wird ausgeführt' });
    expect(screen.getByText('"python"')).toBeTruthy();
    expect(screen.getByText('"unittest"')).toBeTruthy();
    expect(screen.getByText('120 Sekunden')).toBeTruthy();
    expect(screen.getByText('Bekannter Befehl')).toBeTruthy();
    expect(screen.getByText('An den aktuellen Arbeitsschritt gebunden')).toBeTruthy();
    expect(screen.getByText('Nicht angefordert')).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: 'Entscheidung bestätigen' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it('preserves a choice for the same approval, but closes controls when a refresh conflicts', async () => {
    const loader = vi.fn().mockResolvedValue(patchApprovalResponse());
    const controller = vi.fn();
    const { rerender } = render(AgentApprovalCenter, { taskId, loader, controller, refreshKey: 0 });
    const allow = (await screen.findByRole('radio', {
      name: 'Diese Aktion einmal erlauben',
    })) as HTMLInputElement;
    await fireEvent.click(allow);
    await rerender({ refreshKey: 1 });
    await waitFor(() => expect(loader).toHaveBeenCalledTimes(2));
    expect(allow.checked).toBe(true);
    loader.mockResolvedValueOnce({ protocolVersion: 1, result: { status: 'activityChanged' } });
    await rerender({ refreshKey: 2 });
    await screen.findByText(/Entscheidungen sind bis zum aktuellen Stand gesperrt/);
    expect(allow.disabled).toBe(true);
    expect(allow.checked).toBe(false);
    expect(controller).not.toHaveBeenCalled();
    await rerender({ refreshKey: 3 });
    await waitFor(() => expect(allow.disabled).toBe(false));
    expect(allow.checked).toBe(false);
  });

  it('ignores a decision response after its owning task was replaced', async () => {
    let finish: ((response: AgentApprovalControlResponseV1) => void) | undefined;
    const controller = vi.fn(
      () =>
        new Promise<AgentApprovalControlResponseV1>((resolve) => {
          finish = resolve;
        }),
    );
    const onChanged = vi.fn();
    const loader = vi
      .fn<() => Promise<AgentApprovalResponseV1>>()
      .mockResolvedValue(patchApprovalResponse());
    const { rerender } = render(AgentApprovalCenter, { taskId, loader, controller, onChanged });
    await fireEvent.click(
      await screen.findByRole('radio', { name: 'Diese Aktion einmal erlauben' }),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Entscheidung bestätigen' }));
    loader.mockResolvedValue({ protocolVersion: 1, result: { status: 'unavailable' } });
    await rerender({ taskId: '2'.repeat(64) });
    await screen.findByText('Für diese Aufgabe ist gerade keine Freigabe erforderlich.');
    finish?.({
      protocolVersion: 1,
      result: {
        approvalRevision: '4',
        ledgerStoreVersion: '7',
        outcome: 'grantStored',
        runtimeStart: null,
        status: 'applied',
      },
    });
    await waitFor(() => expect(onChanged).not.toHaveBeenCalled());
    expect(screen.queryByText(/Die einmalige Freigabe wurde gespeichert/)).toBeNull();
  });

  it('shows scope and risk without preselecting or enabling confirmation', async () => {
    render(AgentApprovalCenter, {
      taskId,
      loader: vi.fn().mockResolvedValue(patchApprovalResponse()),
    });
    expect(await screen.findByText('Den eng begrenzten Fehler beheben.')).toBeTruthy();
    expect(screen.getByText('src/lib.rs → src/lib.rs')).toBeTruthy();
    expect(screen.getByText('Moderat')).toBeTruthy();
    const options = screen.getAllByRole('radio') as HTMLInputElement[];
    expect(options).toHaveLength(2);
    expect(options.every((option) => !option.checked)).toBe(true);
    expect(
      (
        screen.getByRole('button', {
          name: 'Entscheidung bestätigen',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    const audit = screen.getByText('Technische Freigabedetails').closest('details');
    expect(audit?.open).toBe(false);
    expect(
      screen.getByText('Nur diese Aktion, die angezeigten Ziele und den aktuellen Lauf. Einmalig.'),
    ).toBeTruthy();
    expect(screen.getByText('Gültig bis')).toBeTruthy();
    await fireEvent.click(screen.getByText('Technische Freigabedetails'));
    expect(screen.getByText('a'.repeat(64))).toBeTruthy();
  });

  it('requires an explicit neutral choice before storing a grant', async () => {
    const controller = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: {
        approvalRevision: '4',
        ledgerStoreVersion: '7',
        outcome: 'grantStored',
        runtimeStart: null,
        status: 'applied',
      },
    } satisfies AgentApprovalControlResponseV1);
    const loader = vi
      .fn()
      .mockResolvedValueOnce(patchApprovalResponse())
      .mockResolvedValue(patchApprovalResponse('active'));
    render(AgentApprovalCenter, { taskId, loader, controller });
    await fireEvent.click(
      await screen.findByRole('radio', {
        name: 'Diese Aktion einmal erlauben',
      }),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Entscheidung bestätigen' }));
    await waitFor(() => expect(controller).toHaveBeenCalledTimes(1));
    expect(controller.mock.calls[0]?.[2]).toBe('allowOnce');
    expect(
      await screen.findByText(
        'Die Freigabe ist gespeichert. Erst „Agent fortsetzen“ startet die Aktion. Bis dahin kannst du die Freigabe widerrufen.',
      ),
    ).toBeTruthy();
  });
});

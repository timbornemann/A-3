import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { AgentApprovalControlResponseV1 } from './agent-approval';
import { patchApprovalResponse } from './agent-approval.fixture';
import AgentApprovalCenter from './AgentApprovalCenter.svelte';

const taskId = '1'.repeat(64);

describe('AgentApprovalCenter', () => {
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
          name: 'Ausgewählte Entscheidung bestätigen',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
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
        name: 'Einmalig für genau diese Aktion und diesen Scope erlauben',
      }),
    );
    await fireEvent.click(
      screen.getByRole('button', { name: 'Ausgewählte Entscheidung bestätigen' }),
    );
    await waitFor(() => expect(controller).toHaveBeenCalledTimes(1));
    expect(controller.mock.calls[0]?.[2]).toBe('allowOnce');
    expect(
      await screen.findByText(
        'Die Freigabe ist gespeichert, aber noch ungenutzt. Erst „Agent fortsetzen“ startet einen neuen vom Scheduler verwalteten Versuch.',
      ),
    ).toBeTruthy();
  });
});

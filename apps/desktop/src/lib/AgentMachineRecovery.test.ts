import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AgentMachineRecovery from './AgentMachineRecovery.svelte';
import type { MachineRecoveryResult, MachineRecoveryScope } from './agent-machine-recovery';
afterEach(cleanup);
const scope: MachineRecoveryScope = {
  ledgerRevision: 2,
  ledgerStoreVersion: '3',
  scope: 'b'.repeat(64),
  resourceKind: 'file',
  target: 'D:\\scratch\\file.txt',
};
describe('Scoped machine recovery', () => {
  it('requires a human click on the current displayed scope', async () => {
    const recoverer = vi.fn().mockResolvedValue({ status: 'queued' });
    const onqueued = vi.fn();
    render(AgentMachineRecovery, {
      taskId: 'a'.repeat(64),
      refreshKey: 1,
      loader: async () => ({ status: 'available', recovery: scope }),
      recoverer,
      onqueued,
    });
    const button = await screen.findByRole('button', { name: 'Dateizustand lesen und neu planen' });
    expect(recoverer).not.toHaveBeenCalled();
    await fireEvent.click(button);
    await waitFor(() => expect(onqueued).toHaveBeenCalledOnce());
    expect(recoverer).toHaveBeenCalledExactlyOnceWith('a'.repeat(64), scope);
    expect(screen.queryByRole('button')).toBeNull();
  });
  it('ignores an old query and old completion after changing tasks', async () => {
    let finishOld: (value: MachineRecoveryResult) => void = () => {};
    const old = new Promise<MachineRecoveryResult>((resolve) => {
      finishOld = resolve;
    });
    const loader = vi
      .fn()
      .mockReturnValueOnce(old)
      .mockResolvedValue({
        status: 'available',
        recovery: { ...scope, target: 'D:\\scratch\\current.txt' },
      });
    const recoverer = vi.fn();
    const { rerender } = render(AgentMachineRecovery, {
      taskId: 'a'.repeat(64),
      refreshKey: 1,
      loader,
      recoverer,
    });
    await waitFor(() => expect(loader).toHaveBeenCalledOnce());
    await rerender({ taskId: 'c'.repeat(64), refreshKey: 2 });
    await screen.findByText('D:\\scratch\\current.txt');
    finishOld({ status: 'available', recovery: scope });
    await waitFor(() => expect(screen.queryByText(scope.target)).toBeNull());
    expect(recoverer).not.toHaveBeenCalled();
  });
});

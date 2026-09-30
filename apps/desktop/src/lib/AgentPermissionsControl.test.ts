import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AgentPermissionsControl from './AgentPermissionsControl.svelte';
import type { AgentPermissions } from './agent-permissions';

afterEach(cleanup);
const initial: AgentPermissions = { protocolVersion: 1, revision: '1', mode: 'askPermissions' };
describe('permission mode selection', () => {
  it('activates on selection and ignores stale settings events', async () => {
    let receive: (settings: AgentPermissions) => void = () => {};
    const updater = vi.fn().mockResolvedValue({ ...initial, revision: '2', mode: 'fullMachine' });
    const unsubscribe = vi.fn();
    const { unmount } = render(AgentPermissionsControl, {
      loader: async () => initial,
      updater,
      subscriber: async (callback) => {
        receive = callback;
        return unsubscribe;
      },
    });
    const full = screen.getByRole('button', { name: 'Full machine' });
    const ask = screen.getByRole('button', { name: 'Ask permissions' });
    await waitFor(() => expect(ask.getAttribute('aria-pressed')).toBe('true'));
    await fireEvent.click(full);
    await waitFor(() => expect(full.getAttribute('aria-pressed')).toBe('true'));
    expect(updater).toHaveBeenCalledExactlyOnceWith(initial, 'fullMachine');
    receive(initial);
    await waitFor(() => expect(full.getAttribute('aria-pressed')).toBe('true'));
    receive({ ...initial, revision: '3' });
    await waitFor(() => expect(ask.getAttribute('aria-pressed')).toBe('true'));
    unmount();
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
  it('reloads competing changes without granting optimistic authority', async () => {
    const loader = vi
      .fn()
      .mockResolvedValueOnce(initial)
      .mockResolvedValue({ ...initial, revision: '3' });
    const updater = vi.fn().mockRejectedValue(new Error('Conflict'));
    render(AgentPermissionsControl, { loader, updater, subscriber: async () => () => {} });
    const full = screen.getByRole('button', { name: 'Full machine' });
    await waitFor(() => expect((full as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(full);
    await screen.findByRole('alert');
    expect(full.getAttribute('aria-pressed')).toBe('false');
    expect(loader).toHaveBeenCalledTimes(2);
  });
});

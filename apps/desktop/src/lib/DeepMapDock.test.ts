import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import DeepMapDock from './DeepMapDock.svelte';
import type { DeepMapStatusResponseV3 } from './deep-map';

const model = {
  contextTokens: 32_000,
  modelId: 'mapper',
  outputTokens: 4_096,
  profileId: '7'.repeat(64),
  profileVersion: 1,
  providerId: 'local',
};

const readyStatus: DeepMapStatusResponseV3 = {
  protocolVersion: 1,
  result: {
    lifecycle: { state: 'ready' },
    model,
    status: 'available',
  },
};

const queuedStatus: DeepMapStatusResponseV3 = {
  protocolVersion: 1,
  result: {
    lifecycle: {
      detailsIncomplete: false,
      progress: { action: 'buildPlan', confirmedSteps: '0', phase: 'planning', totalSteps: '3' },
      state: 'queued',
    },
    model,
    status: 'available',
  },
};

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolvePromise: ((value: T) => void) | null = null;
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve;
  });
  return {
    promise,
    resolve: (value) => resolvePromise?.(value),
  };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Deep Map compact live status', () => {
  it('waits for one status read before scheduling the next poll', async () => {
    vi.useFakeTimers();
    const firstRead = deferred<DeepMapStatusResponseV3>();
    const secondRead = deferred<DeepMapStatusResponseV3>();
    const statusLoader = vi
      .fn<() => Promise<DeepMapStatusResponseV3>>()
      .mockImplementationOnce(() => firstRead.promise)
      .mockImplementationOnce(() => secondRead.promise);
    const view = render(DeepMapDock, { props: { statusLoader } });

    await vi.advanceTimersByTimeAsync(6_000);
    expect(statusLoader).toHaveBeenCalledTimes(1);

    firstRead.resolve(readyStatus);
    await vi.advanceTimersByTimeAsync(1_499);
    expect(statusLoader).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(statusLoader).toHaveBeenCalledTimes(2);

    view.unmount();
    secondRead.resolve(readyStatus);
  });

  it('opens the new run only after its queued status has been read', async () => {
    const queuedRead = deferred<DeepMapStatusResponseV3>();
    const statusLoader = vi
      .fn<() => Promise<DeepMapStatusResponseV3>>()
      .mockResolvedValueOnce(readyStatus)
      .mockImplementationOnce(() => queuedRead.promise);
    const starter = vi.fn(async () => ({
      outcome: 'queued' as const,
      protocolVersion: 1 as const,
    }));
    const onrunstarted = vi.fn();
    const view = render(DeepMapDock, { props: { onrunstarted, starter, statusLoader } });

    const start = await screen.findByRole<HTMLButtonElement>('button', { name: 'Start' });
    await waitFor(() => expect(start.disabled).toBe(false));
    await fireEvent.click(start);
    await waitFor(() => expect(starter).toHaveBeenCalledWith('standard'));
    expect(onrunstarted).not.toHaveBeenCalled();

    queuedRead.resolve(queuedStatus);
    await waitFor(() => expect(onrunstarted).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('status').textContent).toContain('0/3 · Plan erstellen');
    view.unmount();
  });
});

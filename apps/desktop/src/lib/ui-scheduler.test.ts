import { describe, expect, it, vi } from 'vitest';
import { UiScheduler, type AnimationFrameClock } from './ui-scheduler';

function controlledClock(): AnimationFrameClock & {
  flush(): void;
  pending(): number;
} {
  let nextId = 1;
  const callbacks = new Map<number, FrameRequestCallback>();
  return {
    cancel(frameId) {
      callbacks.delete(frameId);
    },
    flush() {
      const queued = [...callbacks.values()];
      callbacks.clear();
      for (const callback of queued) callback(0);
    },
    pending() {
      return callbacks.size;
    },
    request(callback) {
      const frameId = nextId;
      nextId += 1;
      callbacks.set(frameId, callback);
      return frameId;
    },
  };
}

function deferred(): { promise: Promise<void>; resolve(): void } {
  let settle: (() => void) | undefined;
  const promise = new Promise<void>((resolve) => {
    settle = resolve;
  });
  return {
    promise,
    resolve() {
      settle?.();
    },
  };
}

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe('UiScheduler', () => {
  it('commits only the latest value per key inside one animation frame', () => {
    const clock = controlledClock();
    const scheduler = new UiScheduler(clock);
    const commits: number[] = [];

    for (let value = 0; value < 10_000; value += 1) {
      scheduler.queueCommit('index-activity', scheduler.generation, () => commits.push(value));
    }

    expect(scheduler.pendingCommitCount).toBe(1);
    expect(clock.pending()).toBe(1);
    clock.flush();
    expect(commits).toEqual([9_999]);
  });

  it('allows one active poll and collapses a burst into one queued rerun', async () => {
    const clock = controlledClock();
    const scheduler = new UiScheduler(clock);
    const first = deferred();
    const second = deferred();
    const task = vi
      .fn<(generation: number) => Promise<void>>()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    scheduler.poll('activity', task);
    for (let index = 0; index < 1_000; index += 1) scheduler.poll('activity', task);
    expect(task).toHaveBeenCalledTimes(1);

    first.resolve();
    await flushPromises();
    expect(task).toHaveBeenCalledTimes(2);

    second.resolve();
    await flushPromises();
    expect(task).toHaveBeenCalledTimes(2);
  });

  it('drops old render buffers and releases project and app ownership exactly once', async () => {
    const clock = controlledClock();
    const scheduler = new UiScheduler(clock);
    const oldCommit = vi.fn();
    const projectCleanup = vi.fn();
    const appCleanup = vi.fn();
    const oldPoll = deferred();
    const oldGeneration = scheduler.generation;
    scheduler.ownProjectCleanup(projectCleanup);
    scheduler.ownAppCleanup(appCleanup);
    scheduler.queueCommit('graph', oldGeneration, oldCommit);
    scheduler.poll('activity', () => oldPoll.promise);

    expect(scheduler.beginProject('b'.repeat(64))).toBe(true);
    expect(scheduler.pendingCommitCount).toBe(0);
    expect(clock.pending()).toBe(0);
    expect(projectCleanup).toHaveBeenCalledTimes(1);
    expect(scheduler.isCurrent(oldGeneration)).toBe(false);

    oldPoll.resolve();
    await flushPromises();
    clock.flush();
    expect(oldCommit).not.toHaveBeenCalled();

    scheduler.dispose();
    scheduler.dispose();
    expect(projectCleanup).toHaveBeenCalledTimes(1);
    expect(appCleanup).toHaveBeenCalledTimes(1);
  });
});

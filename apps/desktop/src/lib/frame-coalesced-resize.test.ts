import { describe, expect, it, vi } from 'vitest';
import { createFrameCoalescedResize } from './frame-coalesced-resize';

describe('createFrameCoalescedResize', () => {
  it('commits only the latest measurement outside the observer delivery cycle', () => {
    const scheduled: FrameRequestCallback[] = [];
    const commit = vi.fn();
    const requestFrame = vi.fn((callback: FrameRequestCallback) => {
      scheduled.push(callback);
      return 7;
    });
    const resize = createFrameCoalescedResize(commit, requestFrame, vi.fn());

    resize.schedule({ height: 400, width: 600 });
    resize.schedule({ height: 420, width: 640 });

    expect(commit).not.toHaveBeenCalled();
    expect(requestFrame).toHaveBeenCalledOnce();
    expect(scheduled).toHaveLength(1);
    scheduled[0]?.(0);
    expect(commit).toHaveBeenCalledOnce();
    expect(commit).toHaveBeenCalledWith({ height: 420, width: 640 });
  });

  it('cancels a pending layout write when its component is destroyed', () => {
    const commit = vi.fn();
    const cancelFrame = vi.fn();
    const resize = createFrameCoalescedResize(commit, () => 11, cancelFrame);

    resize.schedule({ height: 400, width: 600 });
    resize.dispose();
    resize.schedule({ height: 500, width: 700 });

    expect(cancelFrame).toHaveBeenCalledWith(11);
    expect(commit).not.toHaveBeenCalled();
  });
});

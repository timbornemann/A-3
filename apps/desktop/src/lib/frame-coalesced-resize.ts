export interface ElementSize {
  height: number;
  width: number;
}

export interface FrameCoalescedResize {
  dispose: () => void;
  schedule: (size: ElementSize) => void;
}

/**
 * Moves layout-affecting state writes out of the ResizeObserver delivery cycle and keeps only the
 * latest measurement. This prevents a resize-triggered render from recursively exhausting the
 * browser's current observer notification loop.
 */
export function createFrameCoalescedResize(
  commit: (size: ElementSize) => void,
  requestFrame: (callback: FrameRequestCallback) => number = window.requestAnimationFrame.bind(
    window,
  ),
  cancelFrame: (handle: number) => void = window.cancelAnimationFrame.bind(window),
): FrameCoalescedResize {
  let disposed = false;
  let frame: number | null = null;
  let pending: ElementSize | null = null;

  return {
    dispose: () => {
      disposed = true;
      pending = null;
      if (frame !== null) cancelFrame(frame);
      frame = null;
    },
    schedule: (size) => {
      if (disposed) return;
      pending = size;
      if (frame !== null) return;
      frame = requestFrame(() => {
        frame = null;
        const next = pending;
        pending = null;
        if (!disposed && next !== null) commit(next);
      });
    },
  };
}

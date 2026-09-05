import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import FlowWorkspace from './FlowWorkspace.svelte';
import type { FlowQuery, FlowResponse } from './function-flow';
import { entry, flow } from './function-flow.test-fixtures';
afterEach(cleanup);
function loader() {
  return vi.fn(async (q: FlowQuery): Promise<FlowResponse> => ({
    protocolVersion: 1,
    result:
      q.kind === 'source'
        ? { status: 'selectionChanged' }
        : q.kind === 'catalog'
          ? { status: 'catalog', page: { entries: [entry], hasMore: false } }
          : q.kind === 'inspect'
            ? { status: 'flow', flow: { ...flow, selection: q.selection } }
            : { status: 'trace', trace: { direction: q.direction, nodes: [], truncated: false } },
  }));
}
describe('FlowWorkspace', () => {
  it('opens a map selection directly and resolves source by occurrence only', async () => {
    const load = loader();
    render(FlowWorkspace, {
      projectKey: 'worktree',
      publicationKey: 'snapshot',
      loader: load,
      initialSelection: flow.selection,
    });
    await screen.findByRole('heading', { name: 'Werte verstehen' });
    expect(load.mock.calls[0][0]).toEqual({
      kind: 'inspect',
      selection: flow.selection,
      stepOffset: 0,
      valueOffset: 0,
    });
    await fireEvent.click(screen.getByRole('button', { name: /Funktion aufrufen: B/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quelle dieses Schritts' }));
    expect(load).toHaveBeenLastCalledWith({ kind: 'source', selection: flow.selection, step: 1 });
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Werte verstehen' })).toBeNull(),
    );
  });
  it('explores actual call occurrences without executing code', async () => {
    const load = loader();
    render(FlowWorkspace, { projectKey: 'worktree', publicationKey: 'snapshot', loader: load });
    await fireEvent.click(await screen.findByRole('button', { name: /A.*Ablauf öffnen/ }));
    await screen.findByRole('heading', { name: 'Werte verstehen' });
    expect(screen.getByText(/keine Aufzeichnung/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Herkunft von input Version 1' }));
    await screen.findByRole('heading', { name: 'Woher kommt dieser Wert?' });
    expect(load).toHaveBeenLastCalledWith({
      kind: 'trace',
      selection: flow.selection,
      value: 1,
      direction: 'origins',
    });
    expect(load.mock.calls.every(([q]) => ['catalog', 'inspect', 'trace'].includes(q.kind))).toBe(
      true,
    );
  });
  it('invalidates visible evidence as soon as indexing starts', async () => {
    const load = loader();
    const view = render(FlowWorkspace, {
      projectKey: 'one',
      publicationKey: 'snapshot',
      loader: load,
    });
    await fireEvent.click(await screen.findByRole('button', { name: /A.*Ablauf öffnen/ }));
    await screen.findByRole('heading', { name: 'Werte verstehen' });
    await view.rerender({
      projectKey: 'one',
      publicationKey: 'snapshot',
      indexBusy: true,
      loader: load,
    });
    expect(screen.queryByRole('heading', { name: 'Werte verstehen' })).toBeNull();
    expect(screen.getByRole('heading', { name: 'Der Code wird neu eingelesen' })).toBeTruthy();
  });
  it('drops late results from another project', async () => {
    let finish: ((r: FlowResponse) => void) | undefined;
    const load = vi.fn(
      () =>
        new Promise<FlowResponse>((resolve) => {
          finish = resolve;
        }),
    );
    const view = render(FlowWorkspace, {
      projectKey: 'one',
      publicationKey: 'snapshot',
      loader: load,
    });
    await waitFor(() => expect(load).toHaveBeenCalledTimes(1));
    await view.rerender({ projectKey: null, publicationKey: null, loader: load });
    finish?.({
      protocolVersion: 1,
      result: { status: 'catalog', page: { entries: [entry], hasMore: false } },
    });
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: /A.*Ablauf öffnen/ })).toBeNull(),
    );
  });
});

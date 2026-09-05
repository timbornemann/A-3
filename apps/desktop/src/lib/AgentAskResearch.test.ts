import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AgentAskResearch from './AgentAskResearch.svelte';
import type { AgentAskResearchDetailV1, AgentWorkTracePresentationV1 } from './agent-ask-research';

const id = (digit: string): string => digit.repeat(64);
const emptySources = vi.fn(async () => ({
  protocolVersion: 1 as const,
  result: { nextCursor: null, sources: [], status: 'available' as const },
}));

const detailResponse = (steps: AgentAskResearchDetailV1['steps']) => ({
  protocolVersion: 1 as const,
  result: {
    detail: {
      citedSourceCount: 0,
      depth: 'standard' as const,
      legacy: false,
      mode: 'ask' as const,
      sourceCount: 3,
      stale: false,
      steps,
      userSequence: '1',
    },
    status: 'available' as const,
  },
});

const step = (
  action: string,
  occurredAtUnixMillis: string,
  phase: AgentAskResearchDetailV1['steps'][number]['phase'] = 'locating',
  state: AgentAskResearchDetailV1['steps'][number]['state'] = 'running',
): AgentAskResearchDetailV1['steps'][number] => ({
  action,
  completeness: 'notApplicable',
  occurredAtUnixMillis,
  note: null,
  phase,
  query: null,
  state,
});

const timelineAction = (text: string): HTMLElement => {
  const element = screen.getAllByText(text).find((candidate) => candidate.closest('li') !== null);
  if (!element) throw new Error(`Missing timeline action: ${text}`);
  return element;
};

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('AgentAskResearch', () => {
  it.each(['projection', 'legacy'] as const)(
    'advances a full 64-event %s window through completion without replacing retained rows',
    async (api) => {
      vi.useFakeTimers();
      const all = Array.from({ length: 130 }, (_, index) =>
        step(`Schritt ${index + 1}`, String(100 + index), 'reading'),
      );
      all[129] = step('Recherche fertig', '229', 'completed', 'completed');
      let current = all.slice(0, 64);
      const detailLoader = vi.fn(async () => detailResponse(current));
      const projectionLoader = vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          status: 'available' as const,
          detail: detailResponse(current).result.detail,
          nextCursor: null,
          projectionRef: id('4'),
          sources: [],
        },
      }));
      const view = render(AgentAskResearch, {
        ...(api === 'projection'
          ? { projectionLoader }
          : { detailLoader, sourcesLoader: emptySources }),
        live: true,
        refreshKey: '1',
        sessionId: id('1'),
        userSequence: '1',
      });
      await vi.advanceTimersByTimeAsync(1000);
      const retained = timelineAction('Schritt 40').closest('li');
      expect(view.container.querySelectorAll('.research-steps li')).toHaveLength(64);
      current = all.slice(36, 100);
      await view.rerender({ refreshKey: '2' });
      await vi.advanceTimersByTimeAsync(1000);
      expect(timelineAction('Schritt 40').closest('li')).toBe(retained);
      expect(timelineAction('Schritt 100')).toBeTruthy();
      expect(screen.queryByText('Schritt 1')).toBeNull();
      current = all.slice(0, 64); // stale full window must not undo progress
      await view.rerender({ refreshKey: '3' });
      await vi.advanceTimersByTimeAsync(1000);
      expect(timelineAction('Schritt 100')).toBeTruthy();
      current = all.slice(66, 130);
      await view.rerender({ refreshKey: '4' });
      await vi.advanceTimersByTimeAsync(1000);
      expect(timelineAction('Recherche fertig')).toBeTruthy();
      expect(view.container.querySelectorAll('.research-steps li')).toHaveLength(64);
      view.unmount();
    },
  );

  it('accepts a wholly later full window after missed polls but rejects an older one', async () => {
    vi.useFakeTimers();
    const windows = [100, 300].map((start) =>
      Array.from({ length: 64 }, (_, index) =>
        step(`Ereignis ${start + index}`, String(start + index), 'reading'),
      ),
    );
    let current = windows[0];
    const view = render(AgentAskResearch, {
      detailLoader: vi.fn(async () => detailResponse(current)),
      sourcesLoader: emptySources,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    await vi.advanceTimersByTimeAsync(1000);
    current = windows[1];
    await view.rerender({ refreshKey: '2' });
    await vi.advanceTimersByTimeAsync(1000);
    expect(timelineAction('Ereignis 363')).toBeTruthy();
    current = windows[0];
    await view.rerender({ refreshKey: '3' });
    await vi.advanceTimersByTimeAsync(1000);
    expect(timelineAction('Ereignis 363')).toBeTruthy();
    expect(screen.queryByText('Ereignis 100')).toBeNull();
    view.unmount();
  });

  it('does not start fallback reads after an unmounted projection fails', async () => {
    let fail: ((reason: Error) => void) | undefined;
    const projectionLoader = vi.fn(
      () =>
        new Promise<never>((_resolve, reject) => {
          fail = reject;
        }),
    );
    const detailLoader = vi.fn(async () => detailResponse([]));
    const view = render(AgentAskResearch, {
      projectionLoader,
      detailLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    await tick();
    view.unmount();
    fail?.(new Error('late failure'));
    await tick();
    expect(detailLoader).not.toHaveBeenCalled();
  });
  it('retains focused S-source controls and selection when opaque references rotate', async () => {
    let revision = '2';
    const source = () => ({
      referenceLabel: 'S1',
      sourceRef: id(revision),
      path: 'src/manager.py',
      startLine: 18,
      endLine: 32,
      kind: 'file' as const,
      reason: 'sourceText' as const,
      symbol: null,
      usedForAnswer: true,
    });
    const projectionLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        status: 'available' as const,
        projectionRef: id('4'),
        nextCursor: null,
        sources: [source()],
        detail: {
          ...detailResponse([step('Prüfen', '100', 'reading')]).result.detail,
          sourceCount: 1,
          citedSourceCount: 1,
        },
      },
    }));
    const previewLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'stale' as const },
    }));
    const view = render(AgentAskResearch, {
      projectionLoader,
      previewLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    const button = await screen.findByRole('button', { name: /Quelle S1: src\/manager.py/ });
    button.focus();
    await fireEvent.click(button);
    revision = '3';
    await view.rerender({ refreshKey: '2' });
    await waitFor(() => expect(projectionLoader).toHaveBeenCalledTimes(2));
    expect(screen.getByRole('button', { name: /Quelle S1: src\/manager.py/ })).toBe(button);
    expect(document.activeElement).toBe(button);
    expect(button.getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(button);
    expect(previewLoader).toHaveBeenLastCalledWith(id('1'), '1', id('3'));
  });

  it('does not reload itself or postpone reveals when identical polls arrive rapidly', async () => {
    vi.useFakeTimers();
    const response = detailResponse([
      step('Start', '100', 'preparing'),
      step('Suche', '101'),
      step('Entscheiden', '102', 'deciding'),
      step('Lesen', '103', 'reading'),
      step('Auswerten', '104', 'evaluating'),
    ]);
    const detailLoader = vi.fn(async () => structuredClone(response));
    const view = render(AgentAskResearch, {
      detailLoader,
      sourcesLoader: emptySources,
      live: true,
      refreshKey: '0',
      sessionId: id('1'),
      userSequence: '1',
    });
    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(detailLoader).toHaveBeenCalledOnce();
    const first = view.container.querySelector('.research-steps li');
    for (let poll = 1; poll <= 18; poll += 1) {
      await vi.advanceTimersByTimeAsync(50);
      await view.rerender({ refreshKey: String(poll) });
    }
    expect(view.container.querySelectorAll('.research-steps li')).toHaveLength(5);
    expect(view.container.querySelector('.research-steps li')).toBe(first);
    expect(detailLoader).toHaveBeenCalledTimes(19);
    await vi.advanceTimersByTimeAsync(900);
    expect(detailLoader).toHaveBeenCalledTimes(19);
  });

  it('discards an in-flight projection after unmount without publishing or restarting', async () => {
    let finish: ((value: ReturnType<typeof detailResponse>) => void) | undefined;
    const detailLoader = vi.fn(
      () =>
        new Promise<ReturnType<typeof detailResponse>>((resolve) => {
          finish = resolve;
        }),
    );
    const onpresentationchange = vi.fn();
    const view = render(AgentAskResearch, {
      detailLoader,
      sourcesLoader: emptySources,
      onpresentationchange,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    await tick();
    view.unmount();
    finish?.(detailResponse([step('Verspätet', '100', 'preparing')]));
    await tick();
    expect(onpresentationchange).not.toHaveBeenCalled();
    expect(detailLoader).toHaveBeenCalledOnce();
  });

  it('keeps the understandable trace and citations under a completed answer', async () => {
    const previewLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        preview: {
          highlight: { endColumn: 8, endLine: 201, startColumn: 3, startLine: 201 },
          language: 'rust' as const,
          lineCount: 1,
          pathDisplay: 'src/late.rs',
          startLine: 201,
          text: '// TODO\n',
          truncatedAfter: false,
          truncatedBefore: true,
        },
        status: 'available' as const,
      },
    }));
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          detail: {
            citedSourceCount: 1,
            depth: 'standard' as const,
            legacy: false,
            mode: 'ask' as const,
            sourceCount: 1,
            stale: false,
            steps: [
              {
                action: 'Aktuelle indexierte Dateien nach konkretem Text durchsuchen',
                completeness: 'limited' as const,
                occurredAtUnixMillis: '100',
                note: null,
                phase: 'reading' as const,
                query: 'TODO',
                state: 'running' as const,
              },
              {
                action: 'Antwort und verwendete Quellen veröffentlicht',
                completeness: 'notApplicable' as const,
                occurredAtUnixMillis: '101',
                note: null,
                phase: 'completed' as const,
                query: null,
                state: 'completed' as const,
              },
            ],
            userSequence: '1',
          },
          status: 'available' as const,
        },
      })),
      previewLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          nextCursor: null,
          sources: [
            {
              endLine: 201,
              kind: 'symbol' as const,
              path: 'src/late.rs',
              reason: 'sourceText' as const,
              sourceRef: id('2'),
              startLine: 201,
              symbol: null,
              usedForAnswer: true,
            },
          ],
          status: 'available' as const,
        },
      })),
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText('Quellen')).toBeTruthy();
    expect(await screen.findByText('Für die Antwort verwendet')).toBeTruthy();
    expect(screen.getByText(/feste Sicherheits- oder Ressourcengrenze/)).toBeTruthy();
    expect(screen.queryByText(/provider|token|snapshot/i)).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: /src\/late\.rs:201/ }));
    expect(await screen.findByText('// TODO')).toBeTruthy();
    expect(previewLoader).toHaveBeenCalledWith(id('1'), '1', id('2'));
  });

  it('keeps known source counts and shows retry instead of a false empty state', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          detail: {
            ...detailResponse([step('Antwort veröffentlicht', '101', 'completed', 'completed')])
              .result.detail,
            citedSourceCount: 3,
            sourceCount: 12,
          },
          status: 'available' as const,
        },
      })),
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: vi.fn(async () => {
        throw new Error('temporary source page failure');
      }),
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText(/12 Quellen wurden gefunden/)).toBeTruthy();
    expect(screen.queryByText(/Noch keine zitierbare/)).toBeNull();
  });

  it('publishes paged sources atomically and retains the prior projection on revision change', async () => {
    let reads = 0;
    const stableDetail = {
      ...detailResponse([step('Stabiler Recherchepfad', '100', 'reading')]).result.detail,
      sourceCount: 1,
    };
    const nextDetail = {
      ...detailResponse([
        step('Stabiler Recherchepfad', '100', 'reading'),
        step('Noch nicht atomarer Folgeschritt', '101', 'evaluating'),
      ]).result.detail,
      sourceCount: 2,
    };
    const projectionLoader = vi.fn(async () => {
      reads += 1;
      return {
        protocolVersion: 1 as const,
        result: {
          detail: reads === 1 ? stableDetail : nextDetail,
          nextCursor: reads === 1 ? null : id('8'),
          projectionRef: reads === 1 ? id('6') : id('7'),
          sources: [
            {
              endLine: 12,
              kind: 'file' as const,
              path: reads === 1 ? 'src/stable.rs' : 'src/transient.rs',
              reason: 'sourceText' as const,
              referenceLabel: 'S1',
              sourceRef: reads === 1 ? id('2') : id('3'),
              startLine: 10,
              symbol: null,
              usedForAnswer: false,
            },
          ],
          status: 'available' as const,
        },
      };
    });
    let releasePage = (): void => {
      throw new Error('source page was not requested');
    };
    let pageFinished = false;
    const sourcesV2Loader = vi.fn(async () => {
      await new Promise<void>((resolve) => {
        releasePage = resolve;
      });
      pageFinished = true;
      return {
        protocolVersion: 1 as const,
        result: { status: 'projectionChanged' as const },
      };
    });
    const props = {
      projectionLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesV2Loader,
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    await fireEvent.click(await screen.findByRole('button', { name: /Zusätzlich gefunden/u }));
    const retainedStep = timelineAction('Stabiler Recherchepfad').closest('li');
    expect(screen.getByRole('button', { name: /stable\.rs/u })).toBeTruthy();

    await view.rerender({ ...props, refreshKey: '2' });
    await waitFor(() => expect(sourcesV2Loader).toHaveBeenCalledOnce());
    expect(timelineAction('Stabiler Recherchepfad').closest('li')).toBe(retainedStep);
    expect(screen.queryByText('Noch nicht atomarer Folgeschritt')).toBeNull();
    expect(screen.queryByRole('button', { name: /transient\.rs/u })).toBeNull();

    releasePage();
    await waitFor(() => expect(pageFinished).toBe(true));
    expect(timelineAction('Stabiler Recherchepfad').closest('li')).toBe(retainedStep);
    expect(screen.getByRole('button', { name: /stable\.rs/u })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /transient\.rs/u })).toBeNull();
  });

  it('does not starve a coherent projection when another live refresh arrives in flight', async () => {
    let releaseFirst = (): void => {
      throw new Error('first projection was not requested');
    };
    let releaseSecond = (): void => {
      throw new Error('second projection was not requested');
    };
    let reads = 0;
    const projectionLoader = vi.fn(async () => {
      reads += 1;
      const currentRead = reads;
      await new Promise<void>((resolve) => {
        if (currentRead === 1) releaseFirst = resolve;
        else releaseSecond = resolve;
      });
      const steps = [
        step('Erster konsistenter Stand', '100', 'preparing'),
        ...(currentRead > 1 ? [step('Nachfolgender Stand', '101', 'deciding')] : []),
      ];
      return {
        protocolVersion: 1 as const,
        result: {
          detail: { ...detailResponse(steps).result.detail, sourceCount: 0 },
          nextCursor: null,
          projectionRef: id(currentRead === 1 ? '6' : '7'),
          sources: [],
          status: 'available' as const,
        },
      };
    });
    const props = {
      projectionLoader,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await waitFor(() => expect(projectionLoader).toHaveBeenCalledTimes(1));

    await view.rerender({ ...props, refreshKey: '2' });
    releaseFirst();

    expect(await screen.findByText('Erster konsistenter Stand')).toBeTruthy();
    await waitFor(() => expect(projectionLoader).toHaveBeenCalledTimes(2));
    expect(screen.queryByText('Nachfolgender Stand')).toBeNull();

    releaseSecond();
    await waitFor(() => expect(screen.getByText('Nachfolgender Stand')).toBeTruthy());
  });

  it('never stretches the timeline to a retained viewport height', async () => {
    const { container } = render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([step('Natürlich hoher Schritt', '100', 'preparing')]),
      ),
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await screen.findByText('Natürlich hoher Schritt');
    const timeline = container.querySelector<HTMLOListElement>('.research-steps');
    expect(timeline).not.toBeNull();
    expect(timeline?.style.minHeight).toBe('');
    expect(timeline?.style.height).toBe('');
  });

  it('keeps additional sources behind one compact disclosure', async () => {
    const response = detailResponse([
      step('Antwort veröffentlicht', '101', 'completed', 'completed'),
    ]);
    response.result.detail.citedSourceCount = 1;
    response.result.detail.sourceCount = 3;
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => response),
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          nextCursor: null,
          sources: [
            {
              endLine: 12,
              kind: 'symbol' as const,
              path: 'src/used.rs',
              reason: 'sourceText' as const,
              sourceRef: id('2'),
              startLine: 10,
              symbol: 'used',
              usedForAnswer: true,
            },
            {
              endLine: 22,
              kind: 'file' as const,
              path: 'src/additional-one.rs',
              reason: 'indexedText' as const,
              sourceRef: id('3'),
              startLine: 20,
              symbol: null,
              usedForAnswer: false,
            },
            {
              endLine: null,
              kind: 'file' as const,
              path: 'src/additional-two.rs',
              reason: 'relationship' as const,
              sourceRef: id('4'),
              startLine: null,
              symbol: null,
              usedForAnswer: false,
            },
          ],
          status: 'available' as const,
        },
      })),
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    const toggle = await screen.findByRole('button', { name: /Zusätzlich gefunden/u });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('button', { name: /additional-one\.rs/u })).toBeNull();

    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByRole('button', { name: /additional-one\.rs/u })).toBeTruthy();
    expect(screen.getByRole('button', { name: /additional-two\.rs/u })).toBeTruthy();
  });

  it('restores disclosure, source selection, and preview when a turn changes placement', async () => {
    let retained: AgentWorkTracePresentationV1 | null = null;
    const detailLoader = vi.fn(async () =>
      detailResponse([step('Antwort veröffentlicht', '101', 'completed', 'completed')]),
    );
    const sourcesLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        nextCursor: null,
        sources: [
          {
            endLine: 20,
            kind: 'file' as const,
            path: 'src/storage.py',
            reason: 'sourceText' as const,
            sourceRef: id('2'),
            startLine: 18,
            symbol: null,
            usedForAnswer: true,
          },
        ],
        status: 'available' as const,
      },
    }));
    const previewLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        preview: {
          highlight: null,
          language: 'python' as const,
          lineCount: 3,
          pathDisplay: 'src/storage.py',
          startLine: 18,
          text: 'def load_tasks():\n    return []\n',
          truncatedAfter: false,
          truncatedBefore: false,
        },
        status: 'available' as const,
      },
    }));
    const first = render(AgentAskResearch, {
      detailLoader,
      onpresentationchange: (presentation) => (retained = presentation),
      previewLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader,
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    await fireEvent.click(await screen.findByRole('button', { name: /src\/storage\.py:18/ }));
    expect(await screen.findByText(/def load_tasks/)).toBeTruthy();
    expect(retained).not.toBeNull();
    expect((retained as AgentWorkTracePresentationV1 | null)?.expanded).toBe(true);
    first.unmount();

    const restored = render(AgentAskResearch, {
      detailLoader,
      presentation: retained,
      previewLoader,
      refreshKey: '2',
      sessionId: id('1'),
      sourcesLoader,
      userSequence: '1',
    });
    await tick();
    expect(restored.container.querySelector('details')?.open).toBe(true);
    expect(await screen.findByText(/def load_tasks/)).toBeTruthy();
    expect(
      screen.getByRole('button', { name: /src\/storage\.py:18/ }).getAttribute('aria-pressed'),
    ).toBe('true');
  });

  it('groups repeated preparation events and numbers decision rounds', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([
          step('Projektstand wird gebunden', '100', 'preparing'),
          step('Projektstand ist gebunden', '101', 'preparing'),
          step('Task Lens startet', '102', 'locating'),
          step('Task Lens ist fertig', '103', 'locating'),
          step('Erste Entscheidung', '104', 'deciding'),
          step('Erste Quelle lesen', '105', 'reading'),
          step('Zweite Entscheidung', '106', 'deciding'),
          step('Antwort formulieren', '107', 'answeringOrPlanning'),
        ]),
      ),
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(screen.queryByText('Projektstand wird gebunden', { selector: 'p' })).toBeNull();
    expect(screen.getByText('Projektstand ist gebunden')).toBeTruthy();
    expect(screen.queryByText('Task Lens startet', { selector: 'p' })).toBeNull();
    expect(screen.getByText('Task Lens ist fertig')).toBeTruthy();
    expect(screen.getByText('Recherche-Runde 1')).toBeTruthy();
    expect(screen.getByText('Recherche-Runde 2')).toBeTruthy();
    expect(screen.getByText('Abschluss')).toBeTruthy();
  });

  it('explains traces that predate V30', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { status: 'notRecorded' as const },
      })),
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText(/noch nicht aufgezeichnet/)).toBeTruthy();
  });

  it('treats a not-yet-created live trace as preparation instead of old history', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { status: 'notRecorded' as const },
      })),
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });

    expect(await screen.findByText(/Rechercheweg wird vorbereitet/)).toBeTruthy();
    expect(screen.queryByText(/älteren Antwort/)).toBeNull();
  });

  it('falls back to the compatible trace API when the coherent projection is unavailable', async () => {
    const detailLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'notRecorded' as const },
    }));
    const projectionLoader = vi.fn(async () => {
      throw new Error('query_agent_work_trace_projection is not available');
    });
    const sourcesLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { nextCursor: null, sources: [], status: 'available' as const },
    }));
    render(AgentAskResearch, {
      detailLoader,
      live: true,
      projectionLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader,
      userSequence: '1',
    });

    expect(await screen.findByText(/Rechercheweg wird vorbereitet/)).toBeTruthy();
    expect(screen.queryByText(/konnten gerade nicht geladen/)).toBeNull();
    expect(projectionLoader).toHaveBeenCalledOnce();
    expect(detailLoader).toHaveBeenCalledOnce();
    expect(sourcesLoader).not.toHaveBeenCalled();
  });

  it('loads an existing trace through the compatible API after a projection failure', async () => {
    const detailLoader = vi.fn(async () =>
      detailResponse([step('Projektstand wird gebunden', '100', 'preparing')]),
    );
    const projectionLoader = vi.fn(async () => {
      throw new Error('temporary projection read failure');
    });
    const sourcesLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        nextCursor: null,
        sources: [
          {
            endLine: 20,
            kind: 'file' as const,
            path: 'src/storage.py',
            reason: 'sourceText' as const,
            sourceRef: id('2'),
            startLine: 18,
            symbol: null,
            usedForAnswer: false,
          },
        ],
        status: 'available' as const,
      },
    }));
    render(AgentAskResearch, {
      detailLoader,
      live: true,
      projectionLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader,
      userSequence: '1',
    });

    const additionalSources = await screen.findByRole('button', { name: /Zusätzlich gefunden/u });
    await fireEvent.click(additionalSources);
    expect(timelineAction('Projektstand wird gebunden')).toBeTruthy();
    expect(screen.getByRole('button', { name: /Quelle S1: src\/storage\.py:18–20/u })).toBeTruthy();
    expect(screen.queryByText(/konnten gerade nicht geladen/)).toBeNull();
  });

  it('reveals a live batch step by step and advances the active marker', async () => {
    vi.useFakeTimers();
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([
          step('Projektstand binden', '100', 'preparing'),
          step('Task Lens auswerten', '101'),
          step('Nächsten Schritt wählen', '102', 'deciding'),
          step('Quellen prüfen', '103', 'reading'),
          step('Antwort belegen', '104', 'answeringOrPlanning'),
        ]),
      ),
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(
      screen.getByText('Projektstand binden').closest('li')?.getAttribute('aria-current'),
    ).toBe('step');
    expect(screen.queryByText('Task Lens auswerten')).toBeNull();

    await vi.advanceTimersByTimeAsync(180);
    await tick();
    expect(
      screen.getByText('Projektstand binden').closest('li')?.getAttribute('data-step-state'),
    ).toBe('completed');
    expect(
      screen.getByText('Task Lens auswerten').closest('li')?.getAttribute('aria-current'),
    ).toBe('step');

    await vi.advanceTimersByTimeAsync(540);
    await tick();
    expect(timelineAction('Antwort belegen').closest('li')?.getAttribute('data-step-state')).toBe(
      'active',
    );
    expect(screen.getAllByText('In Arbeit')).toHaveLength(1);
    expect(screen.queryByText('Aktueller Stand')).toBeNull();
    expect(screen.getByText('Recherche-Runde 1')).toBeTruthy();
    expect(document.querySelectorAll('[aria-live="polite"]')).toHaveLength(1);
  });

  it('appends a newly polled step without replacing the visible timeline', async () => {
    vi.useFakeTimers();
    let reads = 0;
    const detailLoader = vi.fn(async () => {
      reads += 1;
      return detailResponse([
        step('Bereits sichtbarer Schritt', '100', 'preparing'),
        ...(reads > 1 ? [step('Neu eingetroffener Schritt', '101', 'locating')] : []),
      ]);
    });
    const props = {
      detailLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(
      timelineAction('Bereits sichtbarer Schritt').closest('li')?.getAttribute('aria-current'),
    ).toBe('step');

    await view.rerender({ ...props, refreshKey: '2' });
    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(
      timelineAction('Bereits sichtbarer Schritt').closest('li')?.getAttribute('aria-current'),
    ).toBe('step');

    await vi.advanceTimersByTimeAsync(180);
    await tick();
    expect(
      timelineAction('Bereits sichtbarer Schritt').closest('li')?.getAttribute('data-step-state'),
    ).toBe('completed');
    expect(
      timelineAction('Neu eingetroffener Schritt').closest('li')?.getAttribute('aria-current'),
    ).toBe('step');
  });

  it('keeps the last complete timeline when a live poll regresses temporarily', async () => {
    vi.useFakeTimers();
    let reads = 0;
    const stableSteps = [
      step('Projektstand binden', '100', 'preparing'),
      step('Suchraum lokalisieren', '101', 'locating'),
      step('Nächsten Schritt wählen', '102', 'deciding'),
      step('Quellen prüfen', '103', 'reading'),
      step('Befunde auswerten', '104', 'evaluating'),
    ];
    const detailLoader = vi.fn(async () => {
      reads += 1;
      return detailResponse(
        reads === 1 ? stableSteps : [step('Unvollständiger Zwischenstand', '200', 'preparing')],
      );
    });
    const props = {
      detailLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await vi.advanceTimersByTimeAsync(900);
    await tick();
    const retainedStep = timelineAction('Quellen prüfen').closest('li');
    expect(timelineAction('Befunde auswerten')).toBeTruthy();

    await view.rerender({ ...props, refreshKey: '2' });
    await vi.advanceTimersByTimeAsync(0);
    await tick();

    expect(screen.queryByText('Unvollständiger Zwischenstand')).toBeNull();
    expect(timelineAction('Quellen prüfen').closest('li')).toBe(retainedStep);
    expect(timelineAction('Befunde auswerten')).toBeTruthy();
    expect(screen.queryByText(/neuesten Recherchestand abgeglichen/u)).toBeNull();
  });

  it('does not hide a loaded timeline behind a transient not-recorded response', async () => {
    let reads = 0;
    const detailLoader = vi.fn(async () => {
      reads += 1;
      return reads === 1
        ? detailResponse([step('Sichtbarer Recherchefortschritt', '100', 'reading')])
        : {
            protocolVersion: 1 as const,
            result: { status: 'notRecorded' as const },
          };
    });
    const props = {
      detailLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await screen.findByText('Sichtbarer Recherchefortschritt');
    const retainedStep = timelineAction('Sichtbarer Recherchefortschritt').closest('li');

    await view.rerender({ ...props, refreshKey: '2' });
    await waitFor(() => expect(reads).toBe(2));

    expect(timelineAction('Sichtbarer Recherchefortschritt').closest('li')).toBe(retainedStep);
    expect(screen.queryByText(/damals noch nicht aufgezeichnet/u)).toBeNull();
  });

  it('shows the terminal checkpoint before collapsing once', async () => {
    vi.useFakeTimers();
    const view = render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([
          step('Evidence auswählen', '100'),
          step('Antwort veröffentlicht', '101', 'completed', 'completed'),
        ]),
      ),
      recentlyCompleted: true,
      refreshKey: '2',
      responseVisible: true,
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });
    const disclosure = view.container.querySelector('details');
    expect(disclosure?.open).toBe(true);

    await vi.advanceTimersByTimeAsync(180);
    await tick();
    expect(screen.getByText('Abgeschlossen')).toBeTruthy();
    expect(disclosure?.open).toBe(true);

    await vi.advanceTimersByTimeAsync(700);
    await tick();
    expect(disclosure?.open).toBe(false);

    await fireEvent.click(screen.getByText('Recherche & Quellen'));
    expect(disclosure?.open).toBe(true);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(disclosure?.open).toBe(true);
  });

  it.each([
    ['failed', 'Fehlgeschlagen'],
    ['cancelled', 'Abgebrochen'],
  ] as const)('keeps sources visible for a %s terminal state', async (state, label) => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([
          step('Evidence auswählen', '100'),
          step(`Recherche ${state}`, '101', 'answeringOrPlanning', state),
        ]),
      ),
      refreshKey: '2',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText(label)).toBeTruthy();
    expect(screen.queryByText('Aktueller Stand')).toBeNull();
    expect(
      timelineAction(`Recherche ${state}`).closest('li')?.getAttribute('data-step-state'),
    ).toBe(state);
  });

  it('shows public work notes and keeps continuation-ready research open', async () => {
    const oncontinue = vi.fn();
    const noted = step('Direkte Aufrufer prüfen', '100', 'evaluating', 'awaitingContinuation');
    noted.note = {
      finding: 'Die indirekten Aufrufstellen sind noch nicht vollständig belegt.',
      findingKind: 'hypothesis',
      gap: 'Aufrufer in weiteren Modulen',
      goal: 'Aufgabenerzeugung nachvollziehen',
      nextStep: 'Mit neuem Budget die Beziehungen weiter verfolgen',
      sourceRefs: [],
    };
    const view = render(AgentAskResearch, {
      detailLoader: vi.fn(async () => detailResponse([noted])),
      oncontinue,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText('Aufgabenerzeugung nachvollziehen')).toBeTruthy();
    expect(screen.getByText(/Hypothese/)).toBeTruthy();
    expect(screen.getByText('Aufrufer in weiteren Modulen')).toBeTruthy();
    expect(view.container.querySelector('details')?.open).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Recherche fortsetzen' }));
    expect(oncontinue).toHaveBeenCalledOnce();
  });

  it('clears a pending reveal when the selected turn changes', async () => {
    vi.useFakeTimers();
    const detailLoader = vi.fn(async (_sessionId: string, sequence: string) =>
      detailResponse(
        sequence === '1'
          ? [
              step('Alter Schritt eins', '100', 'preparing'),
              step('Alter Schritt zwei', '101', 'locating'),
              step('Alter Schritt drei', '102', 'reading'),
            ]
          : [step('Neuer Schritt', '200', 'preparing')],
      ),
    );
    const props = {
      detailLoader,
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    };
    const view = render(AgentAskResearch, props);
    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(screen.getByText('Alter Schritt eins')).toBeTruthy();

    await view.rerender({ ...props, refreshKey: '2', userSequence: '2' });
    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(timelineAction('Neuer Schritt')).toBeTruthy();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(screen.queryByText('Alter Schritt zwei')).toBeNull();
    expect(screen.queryByText('Alter Schritt drei')).toBeNull();
  });

  it('skips stagger and motion when reduced motion is requested', async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({
        addEventListener: vi.fn(),
        matches: true,
        removeEventListener: vi.fn(),
      })),
    );
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () =>
        detailResponse([
          step('Projektstand binden', '100', 'preparing'),
          step('Task Lens auswerten', '101'),
          step('Antwort belegen', '102', 'answeringOrPlanning'),
        ]),
      ),
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: emptySources,
      userSequence: '1',
    });

    await vi.advanceTimersByTimeAsync(0);
    await tick();
    expect(screen.getByText('Projektstand binden')).toBeTruthy();
    expect(screen.getByText('Task Lens auswerten')).toBeTruthy();
    expect(timelineAction('Antwort belegen')).toBeTruthy();
    expect(timelineAction('Antwort belegen').closest('li')?.classList.contains('animate')).toBe(
      false,
    );
  });
});

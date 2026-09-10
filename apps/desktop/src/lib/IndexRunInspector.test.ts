import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import IndexRunInspector from './IndexRunInspector.svelte';
import type { IndexRunDetailV1 } from './index-run-inspection';

const mocks = vi.hoisted(() => ({
  control: vi.fn(),
  files: vi.fn(),
  inspection: vi.fn(),
}));

vi.mock('./index-run-inspection', async (importOriginal) => {
  const original = await importOriginal<typeof import('./index-run-inspection')>();
  return {
    ...original,
    controlIndexRun: mocks.control,
    queryIndexRunFiles: mocks.files,
    queryIndexRunInspection: mocks.inspection,
  };
});

const phases = ['discover', 'hash', 'parse', 'link', 'rank', 'publish'] as const;
const active: IndexRunDetailV1 = {
  runRef: 'a'.repeat(64),
  revision: '9',
  state: 'running',
  trigger: 'fileChanges',
  startedAtUnixMillis: '1000',
  endedAtUnixMillis: null,
  lastActivityAtUnixMillis: '1000',
  durationMillis: '61000',
  currentPhase: 'parse',
  currentFile: { display: 'src/main.rs', truncated: false },
  detailsIncomplete: false,
  previousPublicationAvailable: true,
  counts: {
    discovered: '2',
    pending: '0',
    newFiles: '1',
    changed: '0',
    unchanged: '1',
    deleted: '1',
    hashed: '1',
    hashReused: '1',
    structural: '1',
    parseReused: '0',
    generic: '0',
    failed: '0',
  },
  phases: phases.map((phase, index) => ({
    phase,
    state: index < 2 ? 'succeeded' : index === 2 ? 'running' : 'pending',
    startedAtUnixMillis: index <= 2 ? '1000' : null,
    endedAtUnixMillis: index < 2 ? '1001' : null,
    completed: index === 2 ? '1' : null,
    total: index === 2 ? '2' : null,
  })) as IndexRunDetailV1['phases'],
  events: [
    {
      revision: '9',
      occurredAtUnixMillis: '1000',
      kind: 'fileObserved',
      phase: 'parse',
      file: { display: 'src/main.rs', truncated: false },
      failure: null,
    },
  ],
  failure: null,
};
const previous: IndexRunDetailV1 = {
  ...active,
  runRef: 'b'.repeat(64),
  revision: '4',
  state: 'failed',
  endedAtUnixMillis: '900',
  currentFile: null,
  failure: {
    code: 'publish',
    explanation: 'Der Index konnte nicht veröffentlicht werden.',
    recovery: 'Erneut versuchen.',
  },
};

afterEach(() => vi.clearAllMocks());

describe('IndexRunInspector', () => {
  it('shows six phases, stall recovery, previous run, file paging, and revision-bound cancel', async () => {
    mocks.inspection.mockResolvedValue({
      protocolVersion: 1,
      result: {
        status: 'available',
        current: active,
        previous,
        serverTimeUnixMillis: '62000',
        stallThresholdSeconds: 60,
      },
    });
    mocks.files.mockResolvedValueOnce({
      protocolVersion: 1,
      result: {
        status: 'page',
        runRef: active.runRef,
        revision: active.revision,
        files: [
          {
            path: { display: 'src/main.rs', truncated: false },
            change: 'new',
            hash: 'hashed',
            parse: 'structural',
            failures: [],
            failuresTruncated: false,
          },
        ],
        total: '101',
        previousCursor: null,
        nextCursor: 'cursor-next',
      },
    });
    mocks.files.mockResolvedValueOnce({
      protocolVersion: 1,
      result: {
        status: 'page',
        runRef: active.runRef,
        revision: active.revision,
        files: [
          {
            path: { display: 'src/last.rs', truncated: false },
            change: 'unchanged',
            hash: 'reused',
            parse: 'reused',
            failures: [],
            failuresTruncated: false,
          },
        ],
        total: '101',
        previousCursor: 'cursor-back',
        nextCursor: null,
      },
    });
    mocks.control.mockResolvedValue({ protocolVersion: 1, result: { status: 'accepted' } });
    const onClose = vi.fn();
    const view = render(IndexRunInspector, { onClose });

    expect(await screen.findByRole('heading', { name: 'Indexlauf-Details' })).toBeTruthy();
    expect(document.activeElement).toBe(
      screen.getByRole('button', { name: 'Indexlauf-Details schließen' }),
    );
    expect(await screen.findByText('Seit mindestens 60 Sekunden kein Fortschritt.')).toBeTruthy();
    for (const label of [
      'Quellcode finden',
      'Änderungen erkennen',
      'Struktur analysieren',
      'Beziehungen verknüpfen',
      'Signale gewichten',
      'Index veröffentlichen',
    ])
      expect(screen.getByText(label)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Dateien' }));
    expect(await screen.findByText('101 passende Dateien')).toBeTruthy();
    expect(mocks.files).toHaveBeenCalledWith(active, '', 'all', null);
    await fireEvent.click(screen.getByRole('button', { name: 'Weiter' }));
    await waitFor(() =>
      expect(mocks.files).toHaveBeenLastCalledWith(active, '', 'all', 'cursor-next'),
    );
    expect(await screen.findByText('src/last.rs')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Übersicht' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Lauf abbrechen' })[0]);
    await waitFor(() => expect(mocks.control).toHaveBeenCalledWith(active, 'cancel'));

    await fireEvent.click(screen.getByRole('button', { name: 'Vorheriger Lauf' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Fehler & Ereignisse' }));
    expect(await screen.findByText('Der Index konnte nicht veröffentlicht werden.')).toBeTruthy();
    expect(screen.getByText('Erneut versuchen.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Neu versuchen' }));
    await waitFor(() => expect(mocks.control).toHaveBeenLastCalledWith(previous, 'retry'));
    view.unmount();
  });
});

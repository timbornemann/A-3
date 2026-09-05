import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AgentWorkspace from './AgentWorkspace.svelte';
import * as sessionApi from './agent-session';
import type { AgentActivityResponseV1 } from './agent-activity';
import type {
  AgentSessionControlActionV1,
  AgentSessionResponseV1,
  AgentSessionsResponseV1,
  AgentSlashCommandsResponseV1,
} from './agent-session';
import type { TaskLensTaskResponseV1 } from './task-lens';

// This suite verifies conversation/poll ownership, not Mermaid's layout in jsdom.
// Renderer and sanitizer contracts live in AgentDiagrams and agent-diagram-rendering tests.
const diagramRenderer = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn(async () => ({
    svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>Start</text></svg>',
  })),
}));
vi.mock('mermaid', () => ({ default: diagramRenderer }));

const sessionId = 'a'.repeat(64);

const noSessions = (): AgentSessionsResponseV1 => ({
  protocolVersion: 1,
  result: { nextCursor: null, sessions: [], status: 'available' },
});

const reviewedPlan = (): AgentSessionResponseV1 => ({
  protocolVersion: 1,
  result: {
    session: {
      activeTaskId: null,
      entries: [
        {
          createdAtUnixMillis: '100',
          kind: 'userMessage',
          planRevision: null,
          sequence: '1',
          text: 'Überarbeite den Agent Workspace',
        },
        {
          createdAtUnixMillis: '101',
          kind: 'plan',
          planRevision: 1,
          sequence: '2',
          text: 'Ein exakter Implementierungsplan',
        },
      ],
      hasOlderEntries: false,
      summary: {
        currentPlanRevision: 1,
        mode: 'plan',
        revision: '2',
        sessionId,
        state: 'awaitingPlanReview',
        title: 'Agent Workspace überarbeiten',
        updatedAtUnixMillis: '101',
      },
    },
    status: 'available',
  },
});

const askSession = (state: 'running' | 'completed'): AgentSessionResponseV1 => ({
  protocolVersion: 1,
  result: {
    session: {
      activeTaskId: null,
      entries: [
        {
          createdAtUnixMillis: '100',
          kind: 'userMessage',
          planRevision: null,
          sequence: '1',
          text: 'Was macht A^3?',
        },
        ...(state === 'completed'
          ? [
              {
                createdAtUnixMillis: '101',
                kind: 'finalReport' as const,
                planRevision: null,
                sequence: '2',
                text: 'A^3 ist ein evidenzgebundener Coding-Agent.',
              },
            ]
          : []),
      ],
      hasOlderEntries: false,
      summary: {
        currentPlanRevision: null,
        mode: 'ask',
        revision: state === 'completed' ? '2' : '1',
        sessionId,
        state,
        title: 'Was macht A^3?',
        updatedAtUnixMillis: state === 'completed' ? '101' : '100',
      },
    },
    status: 'available',
  },
});

const activeAgentSession = (): AgentSessionResponseV1 => ({
  protocolVersion: 1,
  result: {
    session: {
      activeTaskId: 'b'.repeat(64),
      entries: [
        {
          createdAtUnixMillis: '100',
          kind: 'userMessage',
          planRevision: null,
          sequence: '1',
          text: 'Setze die geprüfte Änderung um',
        },
      ],
      hasOlderEntries: false,
      summary: {
        currentPlanRevision: null,
        mode: 'agent',
        revision: '1',
        sessionId,
        state: 'running',
        title: 'Geprüfte Änderung umsetzen',
        updatedAtUnixMillis: '100',
      },
    },
    status: 'available',
  },
});

const activeAgentActivity = (): AgentActivityResponseV1 => ({
  protocolVersion: 1,
  result: {
    activity: {
      blockers: [],
      currentLedgerRevision: 1,
      ledgerStoreVersion: '1',
      run: {
        attemptNumber: 1,
        budget: {
          actionLimit: 8,
          durationLimitMillis: '60000',
          outputTokenLimit: '2000',
          promptTokenLimit: '8000',
          repairLimit: 1,
          turnLimit: 8,
        },
        createdAtUnixMillis: '100',
        currentSnapshotId: 'c'.repeat(64),
        earlierEventsOmitted: false,
        ledgerRevision: 1,
        ledgerRevisionMatchesCurrent: true,
        runId: 'd'.repeat(64),
        state: 'execute',
        stepId: 'e'.repeat(64),
        terminal: false,
        timeline: [
          {
            code: 'controllerDecision',
            event: { kind: 'runStarted' },
            occurredAtUnixMillis: '100',
            outcome: 'succeeded',
            sequence: '1',
            snapshotId: 'c'.repeat(64),
          },
          {
            code: 'policyDecision',
            event: { kind: 'toolAction' },
            occurredAtUnixMillis: '101',
            outcome: null,
            sequence: '2',
            snapshotId: 'c'.repeat(64),
          },
        ],
        updatedAtUnixMillis: '101',
        usage: {
          actionCount: 1,
          elapsedAtLastEventMillis: '1',
          outputTokens: '10',
          promptTokens: '20',
          repairCount: 0,
          turnCount: 1,
        },
      },
    },
    status: 'available',
  },
});

const adaptiveWorkPlan = (): TaskLensTaskResponseV1 => ({
  protocolVersion: 1,
  result: {
    ledgerRevision: 2,
    ledgerStoreVersion: '4',
    status: 'available',
    steps: [
      { intendedOutcome: 'API-Vertrag definieren', status: 'completed', stepId: 'f'.repeat(64) },
      {
        intendedOutcome: 'Serializer ergänzen und Adapter anbinden',
        status: 'inProgress',
        stepId: 'e'.repeat(64),
      },
      { intendedOutcome: 'Integrationstests ausführen', status: 'pending', stepId: '9'.repeat(64) },
    ],
    task: {
      goalRevision: 1,
      objective: 'API sicher implementieren',
      taskId: 'b'.repeat(64),
    },
  },
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('AgentWorkspace', () => {
  it('renames through a native dialog with the visible session revision and trimmed title', async () => {
    let response = askSession('completed');
    const sessionController = vi.fn(
      async (_id: string, _revision: string, action: AgentSessionControlActionV1) => {
        if (response.result.status !== 'available' || action.kind !== 'rename')
          throw new Error('rename fixture required');
        response = structuredClone(response);
        if (response.result.status !== 'available') throw new Error('available fixture required');
        response.result.session.summary.title = action.title;
        response.result.session.summary.revision = '3';
        return response;
      },
    );
    render(AgentWorkspace, {
      activeProject: true,
      sessionController,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          status: 'available',
          sessions: response.result.status === 'available' ? [response.result.session.summary] : [],
        },
      }),
    });
    const trigger = await screen.findByRole('button', { name: 'Session-Aktionen' });
    await fireEvent.click(trigger);
    await fireEvent.click(screen.getByRole('button', { name: 'Umbenennen' }));
    const dialog = screen.getByRole('dialog', { name: 'Chat umbenennen' });
    expect(dialog.tagName).toBe('DIALOG');
    expect(dialog.hasAttribute('open')).toBe(true);
    const input = within(dialog).getByRole('textbox', { name: 'Name' });
    expect((input as HTMLInputElement).value).toBe('Was macht A^3?');
    await fireEvent.input(input, { target: { value: '  ' } });
    expect(
      within(dialog).getByRole<HTMLButtonElement>('button', { name: 'Speichern' }).disabled,
    ).toBe(true);
    await fireEvent.input(input, { target: { value: '  Architektur verstehen  ' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() =>
      expect(sessionController).toHaveBeenCalledWith(sessionId, '2', {
        kind: 'rename',
        title: 'Architektur verstehen',
      }),
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('heading', { name: 'Architektur verstehen' })).toBeTruthy();
    expect(document.activeElement).toBe(trigger);
  });

  it.each(['button', 'native cancel'] as const)(
    'cancels rename by %s without changing the session',
    async (method) => {
      const response = askSession('completed');
      if (response.result.status !== 'available') throw new Error('available fixture required');
      const sessionController = vi.fn(async () => response);
      const summary = response.result.session.summary;
      render(AgentWorkspace, {
        activeProject: true,
        sessionController,
        sessionLoader: async () => response,
        sessionsLoader: async () => ({
          protocolVersion: 1,
          result: { nextCursor: null, status: 'available', sessions: [summary] },
        }),
      });
      const trigger = await screen.findByRole('button', { name: 'Session-Aktionen' });
      await fireEvent.click(trigger);
      await fireEvent.click(screen.getByRole('button', { name: 'Umbenennen' }));
      const dialog = screen.getByRole('dialog', { name: 'Chat umbenennen' });
      const focusTrigger = trigger.focus.bind(trigger);
      vi.spyOn(trigger, 'focus').mockImplementation(() => {
        expect(document.querySelector('dialog[open]')).toBeNull();
        focusTrigger();
      });
      await fireEvent.input(within(dialog).getByRole('textbox', { name: 'Name' }), {
        target: { value: 'Verwerfen' },
      });
      if (method === 'button')
        await fireEvent.click(within(dialog).getByRole('button', { name: 'Abbrechen' }));
      else await fireEvent(dialog, new Event('cancel', { cancelable: true }));
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(sessionController).not.toHaveBeenCalled();
      expect(screen.getByRole('heading', { name: 'Was macht A^3?' })).toBeTruthy();
      expect(document.activeElement).toBe(trigger);
    },
  );

  it('keeps history focus reachable across close, open and Escape', async () => {
    render(AgentWorkspace, { activeProject: true, sessionsLoader: async () => noSessions() });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: 'Verlauf schließen' }));
    const trigger = screen.getByRole('button', { name: 'Verlauf öffnen' });
    expect(document.activeElement).toBe(trigger);
    expect(screen.queryByRole('complementary', { name: 'Unterhaltungen' })).toBeNull();
    const history = document.getElementById(trigger.getAttribute('aria-controls')!);
    expect(history?.inert).toBe(true);
    await fireEvent.click(trigger);
    const close = screen.getByRole('button', { name: 'Verlauf schließen' });
    expect(document.activeElement).toBe(close);
    expect(history?.inert).toBe(false);
    await fireEvent.keyDown(close, { key: 'Escape' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Verlauf öffnen' }));
    expect(history?.getAttribute('aria-hidden')).toBe('true');
  });

  it('ignores Escape from another route while the history drawer stays open', async () => {
    render(AgentWorkspace, { activeProject: true, sessionsLoader: async () => noSessions() });
    await screen.findByText('Woran möchtest du arbeiten?');
    const outside = document.createElement('button');
    outside.textContent = 'Andere Ansicht';
    document.body.append(outside);
    try {
      outside.focus();
      await fireEvent.keyDown(outside, { key: 'Escape' });
      await fireEvent.keyDown(window, { key: 'Escape' });
      expect(screen.getByRole('complementary', { name: 'Unterhaltungen' })).toBeTruthy();
      expect(document.activeElement).toBe(outside);
      const close = screen.getByRole('button', { name: 'Verlauf schließen' });
      close.focus();
      await fireEvent.keyDown(close, { key: 'Escape' });
      expect(screen.queryByRole('complementary', { name: 'Unterhaltungen' })).toBeNull();
    } finally {
      outside.remove();
    }
  });

  it('allows only one rename mutation and keeps the pending modal open', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = response.result.session.summary;
    let resolveRename: (value: AgentSessionResponseV1) => void = () => {};
    const sessionController = vi.fn(
      () =>
        new Promise<AgentSessionResponseV1>((resolve) => {
          resolveRename = resolve;
        }),
    );
    render(AgentWorkspace, {
      activeProject: true,
      sessionController,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { nextCursor: null, status: 'available', sessions: [summary] },
      }),
    });
    await fireEvent.click(await screen.findByRole('button', { name: 'Session-Aktionen' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Umbenennen' }));
    const dialog = screen.getByRole('dialog', { name: 'Chat umbenennen' });
    const input = within(dialog).getByRole('textbox', { name: 'Name' });
    await fireEvent.input(input, { target: { value: 'Neuer Titel' } });
    const form = input.closest('form')!;
    await fireEvent.submit(form);
    await fireEvent.submit(form);
    expect(sessionController).toHaveBeenCalledTimes(1);
    expect(
      within(dialog).getByRole<HTMLButtonElement>('button', { name: 'Wird gespeichert …' })
        .disabled,
    ).toBe(true);
    expect(
      within(dialog).getByRole<HTMLButtonElement>('button', { name: 'Abbrechen' }).disabled,
    ).toBe(true);
    expect(
      within(dialog).getByRole<HTMLButtonElement>('button', { name: 'Umbenennen schließen' })
        .disabled,
    ).toBe(true);
    const cancel = new Event('cancel', { cancelable: true });
    await fireEvent(dialog, cancel);
    expect(cancel.defaultPrevented).toBe(true);
    expect(screen.getByRole('dialog')).toBe(dialog);
    resolveRename(response);
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('uses bounded keyboard resizing for both workspace separators', async () => {
    vi.spyOn(sessionApi, 'updateAgentWorkspaceLayout').mockImplementation(
      async (current, layout) => ({ ...current, ...layout }),
    );
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = response.result.session.summary;
    render(AgentWorkspace, {
      activeProject: true,
      activityLoader: async () => activeAgentActivity(),
      workPlanLoader: async () => adaptiveWorkPlan(),
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { nextCursor: null, status: 'available', sessions: [summary] },
      }),
    });
    const history = await screen.findByRole('separator', { name: 'Verlaufbreite ändern' });
    const inspector = await screen.findByRole('separator', { name: 'Inspectorbreite ändern' });
    for (const [separator, min, max, arrow] of [
      [history, '220', '360', 'ArrowRight'],
      [inspector, '320', '640', 'ArrowLeft'],
    ] as const) {
      expect(separator.getAttribute('tabindex')).toBe('0');
      expect(separator.getAttribute('aria-orientation')).toBe('vertical');
      await fireEvent.keyDown(separator, { key: 'Home' });
      expect(separator.getAttribute('aria-valuenow')).toBe(min);
      await fireEvent.keyDown(separator, { key: arrow });
      expect(separator.getAttribute('aria-valuenow')).toBe(String(Number(min) + 16));
      await fireEvent.keyDown(separator, { key: 'End' });
      await fireEvent.keyDown(separator, { key: arrow });
      expect(separator.getAttribute('aria-valuenow')).toBe(max);
    }
  });

  it('ends pointer resizing on cancellation and releases all listeners on unmount', async () => {
    const persist = vi
      .spyOn(sessionApi, 'updateAgentWorkspaceLayout')
      .mockImplementation(async (current, layout) => ({ ...current, ...layout }));
    const added = vi.spyOn(window, 'addEventListener');
    const removed = vi.spyOn(window, 'removeEventListener');
    const view = render(AgentWorkspace, {
      activeProject: true,
      sessionsLoader: async () => noSessions(),
    });
    const separator = await screen.findByRole('separator', { name: 'Verlaufbreite ändern' });
    await fireEvent.pointerDown(separator, { button: 0, clientX: 100 });
    await fireEvent.pointerMove(window, { clientX: 132 });
    expect(separator.getAttribute('aria-valuenow')).toBe('296');
    await fireEvent.pointerCancel(window);
    expect(persist).toHaveBeenCalledTimes(1);
    await fireEvent.pointerMove(window, { clientX: 190 });
    await fireEvent.pointerUp(window);
    expect(separator.getAttribute('aria-valuenow')).toBe('296');
    expect(persist).toHaveBeenCalledTimes(1);
    await fireEvent.pointerDown(separator, { button: 0, clientX: 132 });
    const owned = added.mock.calls.filter(([event]) =>
      ['pointermove', 'pointerup', 'pointercancel'].includes(event),
    );
    view.unmount();
    for (const [event, listener] of owned) expect(removed).toHaveBeenCalledWith(event, listener);
    await fireEvent.pointerMove(window, { clientX: 180 });
    await fireEvent.pointerUp(window);
    expect(persist).toHaveBeenCalledTimes(1);
  });

  it('keeps every Core boundary closed without an active project', () => {
    const sessionsLoader = vi.fn<() => Promise<AgentSessionsResponseV1>>();

    render(AgentWorkspace, { activeProject: false, sessionsLoader });

    expect(screen.getByText('Öffne zuerst ein Projekt')).toBeTruthy();
    expect(sessionsLoader).not.toHaveBeenCalled();
  });

  it('starts new work in Agent mode and exposes the three capability presets', async () => {
    render(AgentWorkspace, {
      activeProject: true,
      sessionsLoader: vi.fn(async () => noSessions()),
    });

    await screen.findByText('Woran möchtest du arbeiten?');
    expect(
      screen
        .getByRole('button', { name: /Agent\s*Änderungen ausführen/u })
        .getAttribute('aria-pressed'),
    ).toBe('true');
    expect(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u })).toBeTruthy();
    expect(screen.getByRole('button', { name: /Plan\s*Gemeinsam ausarbeiten/u })).toBeTruthy();
  });

  it('sends the selected mode, visible message, and per-message research depth', async () => {
    const messageSubmitter = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'noProject' as const },
    }));
    render(AgentWorkspace, {
      activeProject: true,
      messageSubmitter,
      sessionsLoader: vi.fn(async () => noSessions()),
    });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u }));
    await fireEvent.click(screen.getByRole('button', { name: 'Gründlich' }));
    await fireEvent.input(screen.getByLabelText('Nachricht an A^3'), {
      target: { value: 'Wie funktioniert der Index?' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht senden' }));

    await waitFor(() =>
      expect(messageSubmitter).toHaveBeenCalledWith({
        message: 'Wie funktioniert der Index?',
        mode: 'ask',
        researchDepth: 'thorough',
      }),
    );
  });

  it('allows choosing the depth for the next message while the current Ask turn is running', async () => {
    const running = askSession('running');
    if (running.result.status !== 'available') throw new Error('fixture must be available');
    const runningSession = running.result.session;
    render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 60_000,
      sessionLoader: vi.fn(async () => running),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [runningSession.summary],
          status: 'available',
        },
      })),
    });

    await screen.findByText('Was macht A^3?');
    const thorough = screen.getByRole('button', { name: 'Gründlich' });
    expect((thorough as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(thorough);
    expect(thorough.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Standard' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    expect(screen.queryByRole('complementary', { name: 'Agentenlauf' })).toBeNull();
  });

  it('shows and controls the durable FIFO above the composer', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.queuePaused = true;
    response.result.session.queueRevision = '7';
    response.result.session.modeOptions = [
      { mode: 'ask', requiresPlanReview: false, selectable: true },
      { mode: 'plan', requiresPlanReview: false, selectable: true },
      { mode: 'agent', requiresPlanReview: true, selectable: true },
    ];
    response.result.session.queuedMessages = [
      {
        enqueuedAtUnixMillis: '102',
        position: 1,
        preview: 'Erste vorgemerkte Frage',
        queueReference: 'c'.repeat(64),
        targetMode: 'ask',
      },
      {
        enqueuedAtUnixMillis: '103',
        position: 2,
        preview: 'Anschließenden Plan erstellen',
        queueReference: 'd'.repeat(64),
        targetMode: 'plan',
      },
    ];
    const sessionQueueController = vi.fn(async () => response);
    const sessionSummary = response.result.session.summary;
    render(AgentWorkspace, {
      activeProject: true,
      sessionLoader: vi.fn(async () => response),
      sessionQueueController,
      sessionsLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { nextCursor: null, sessions: [sessionSummary], status: 'available' as const },
      })),
    });

    expect(await screen.findByText('2 vorgemerkt')).toBeTruthy();
    expect(screen.getByText('Erste vorgemerkte Frage')).toBeTruthy();
    await fireEvent.click(
      screen.getByRole('button', { name: 'Vorgemerkte Nachricht 1 entfernen' }),
    );
    await waitFor(() =>
      expect(sessionQueueController).toHaveBeenCalledWith(sessionId, '7', {
        kind: 'remove',
        queueReference: 'c'.repeat(64),
      }),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Mit Warteschlange fortfahren' }));
    await waitFor(() =>
      expect(sessionQueueController).toHaveBeenCalledWith(sessionId, '7', { kind: 'resume' }),
    );
  });

  it('honors the Core-owned selectable modes and plan-review marker', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.modeOptions = [
      { mode: 'ask', requiresPlanReview: false, selectable: true },
      { mode: 'plan', requiresPlanReview: false, selectable: false },
      { mode: 'agent', requiresPlanReview: true, selectable: true },
    ];
    const sessionSummary = response.result.session.summary;
    render(AgentWorkspace, {
      activeProject: true,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { nextCursor: null, sessions: [sessionSummary], status: 'available' as const },
      })),
    });

    await screen.findByText('Was macht A^3?');
    const plan = screen.getByRole('button', { name: /Plan\s*Gemeinsam ausarbeiten/u });
    const agent = screen.getByRole('button', { name: /Agent\s*Änderungen ausführen/u });
    await waitFor(() => expect((plan as HTMLButtonElement).disabled).toBe(true));
    await fireEvent.click(agent);
    expect(agent.textContent).toContain('Nach Planfreigabe');
  });

  it('keeps the header menu keyboard reachable and returns focus on Escape', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const sessionSummary = response.result.session.summary;
    render(AgentWorkspace, {
      activeProject: true,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { nextCursor: null, sessions: [sessionSummary], status: 'available' as const },
      })),
    });

    const trigger = await screen.findByRole('button', { name: 'Session-Aktionen' });
    await fireEvent.click(trigger);
    expect(await screen.findByRole('button', { name: 'Umbenennen' })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Umbenennen' }));

    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('button', { name: 'Umbenennen' })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('selects a mode-compatible slash command and locks the Core-owned depth', async () => {
    const messageSubmitter = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'noProject' as const },
    }));
    const slashCommandsLoader = vi.fn(async () => ({
      catalogVersion: 1 as const,
      commands: [
        {
          available: true,
          depth: 'standard' as const,
          description: 'Erstellt belegte Diagramme.',
          implicitPrimary: null,
          name: '/diagram',
          requiresSubject: true,
          role: 'primary' as const,
          title: 'Diagramm',
        },
        {
          available: true,
          depth: 'thorough' as const,
          description: 'Prüft Sicherheitsgrenzen.',
          implicitPrimary: '/review',
          name: '/security',
          requiresSubject: false,
          role: 'lens' as const,
          title: 'Security',
        },
      ],
      protocolVersion: 1 as const,
    }));
    render(AgentWorkspace, {
      activeProject: true,
      messageSubmitter,
      sessionsLoader: vi.fn(async () => noSessions()),
      slashCommandsLoader,
    });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u }));
    const composer = screen.getByLabelText('Nachricht an A^3');
    await fireEvent.input(composer, { target: { value: '/' } });
    await fireEvent.click(await screen.findByRole('option', { name: /\/diagram/u }));
    await fireEvent.input(composer, { target: { value: '/diagram Agentenablauf' } });

    expect(screen.getByText('Standard · automatisch')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Gründlich' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht senden' }));
    await waitFor(() =>
      expect(messageSubmitter).toHaveBeenCalledWith({
        message: '/diagram Agentenablauf',
        mode: 'ask',
        researchDepth: 'command',
      }),
    );
  });

  it('navigates the command palette with the keyboard', async () => {
    const slashCommandsLoader = vi.fn(async (): Promise<AgentSlashCommandsResponseV1> => ({
      catalogVersion: 1,
      commands: [
        {
          available: true,
          depth: 'standard',
          description: 'Erstellt belegte Diagramme.',
          implicitPrimary: null,
          name: '/diagram',
          requiresSubject: true,
          role: 'primary',
          title: 'Diagramm',
        },
        {
          available: true,
          depth: 'thorough',
          description: 'Prüft streng.',
          implicitPrimary: null,
          name: '/review',
          requiresSubject: false,
          role: 'primary',
          title: 'Review',
        },
      ],
      protocolVersion: 1,
    }));
    render(AgentWorkspace, {
      activeProject: true,
      sessionsLoader: vi.fn(async () => noSessions()),
      slashCommandsLoader,
    });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u }));
    const composer = screen.getByLabelText('Nachricht an A^3');
    await fireEvent.input(composer, { target: { value: '/' } });
    await screen.findByRole('option', { name: /\/diagram/u });

    await fireEvent.keyDown(composer, { key: 'ArrowDown' });
    await fireEvent.keyDown(composer, { key: 'Enter' });

    expect((composer as HTMLTextAreaElement).value).toBe('/review ');
    expect(screen.getByText('Gründlich · automatisch')).toBeTruthy();
  });

  it('fails closed without retry loops and can reload the command catalog', async () => {
    const slashCommandsLoader = vi
      .fn<() => Promise<AgentSlashCommandsResponseV1>>()
      .mockRejectedValueOnce(new Error('catalog unavailable'))
      .mockResolvedValue({
        catalogVersion: 1,
        commands: [
          {
            available: true,
            depth: 'standard',
            description: 'Erstellt belegte Diagramme.',
            implicitPrimary: null,
            name: '/diagram',
            requiresSubject: true,
            role: 'primary',
            title: 'Diagramm',
          },
        ],
        protocolVersion: 1,
      });
    render(AgentWorkspace, {
      activeProject: true,
      sessionsLoader: vi.fn(async () => noSessions()),
      slashCommandsLoader,
    });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u }));
    await fireEvent.input(screen.getByLabelText('Nachricht an A^3'), {
      target: { value: '/diagram Ablauf' },
    });

    expect(await screen.findByText('Die Commands konnten nicht geladen werden.')).toBeTruthy();
    expect(slashCommandsLoader).toHaveBeenCalledTimes(1);
    const send = screen.getByRole('button', { name: 'Nachricht senden' }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    await fireEvent.click(screen.getByRole('button', { name: 'Erneut laden' }));
    await waitFor(() => expect(slashCommandsLoader).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(send.disabled).toBe(false));
  });

  it('keeps an escaped leading slash as ordinary message text after reload', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.entries[0] = {
      ...response.result.session.entries[0],
      command: null,
      text: '/review ist hier normaler Text.',
    };
    const sessionSummary = response.result.session.summary;
    const slashCommandsLoader = vi.fn(async (): Promise<AgentSlashCommandsResponseV1> => ({
      catalogVersion: 1,
      commands: [
        {
          available: true,
          depth: 'thorough',
          description: 'Prüft streng.',
          implicitPrimary: null,
          name: '/review',
          requiresSubject: false,
          role: 'primary',
          title: 'Review',
        },
      ],
      protocolVersion: 1,
    }));
    render(AgentWorkspace, {
      activeProject: true,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          nextCursor: null,
          sessions: [sessionSummary],
          status: 'available' as const,
        },
      })),
      slashCommandsLoader,
    });

    const message = await screen.findByText('/review ist hier normaler Text.');
    expect(message.tagName).toBe('P');
    expect(screen.queryByLabelText('Slash Commands')).toBeNull();
  });

  it('treats a leading lens as implicit review and never suggests another primary command', async () => {
    const slashCommandsLoader = vi.fn(async () => ({
      catalogVersion: 1 as const,
      commands: [
        {
          available: true,
          depth: 'standard' as const,
          description: 'Erstellt belegte Diagramme.',
          implicitPrimary: null,
          name: '/diagram',
          requiresSubject: true,
          role: 'primary' as const,
          title: 'Diagramm',
        },
        {
          available: true,
          depth: 'thorough' as const,
          description: 'Prüft streng.',
          implicitPrimary: null,
          name: '/review',
          requiresSubject: false,
          role: 'primary' as const,
          title: 'Review',
        },
        {
          available: true,
          depth: 'thorough' as const,
          description: 'Prüft Sicherheitsgrenzen.',
          implicitPrimary: '/review',
          name: '/security',
          requiresSubject: false,
          role: 'lens' as const,
          title: 'Security',
        },
        {
          available: true,
          depth: 'thorough' as const,
          description: 'Prüft Laufzeit und Ressourcen.',
          implicitPrimary: '/review',
          name: '/performance',
          requiresSubject: false,
          role: 'lens' as const,
          title: 'Performance',
        },
      ],
      protocolVersion: 1 as const,
    }));
    render(AgentWorkspace, {
      activeProject: true,
      sessionsLoader: vi.fn(async () => noSessions()),
      slashCommandsLoader,
    });
    await screen.findByText('Woran möchtest du arbeiten?');
    await fireEvent.click(screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u }));
    const composer = screen.getByLabelText('Nachricht an A^3');
    await fireEvent.input(composer, { target: { value: '/security ' } });

    expect(await screen.findByRole('option', { name: /\/performance/u })).toBeTruthy();
    expect(screen.queryByRole('option', { name: /\/diagram/u })).toBeNull();
    expect(screen.queryByRole('option', { name: /\/review/u })).toBeNull();

    await fireEvent.input(composer, { target: { value: '/security /diagram auth' } });
    expect(
      screen.getByText(/Eine allein verwendete Linse nutzt automatisch \/review/u),
    ).toBeTruthy();
  });

  it('implements only the exact visible plan revision', async () => {
    const response = reviewedPlan();
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const session = response.result.session;
    const planStarter = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        outcome: 'started' as const,
        session,
        status: 'available' as const,
      },
    }));
    render(AgentWorkspace, {
      activeProject: true,
      planStarter,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [
            response.result.status === 'available'
              ? response.result.session.summary
              : (() => {
                  throw new Error('fixture must be available');
                })(),
          ],
          status: 'available',
        },
      })),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Plan umsetzen' }));
    await waitFor(() => expect(planStarter).toHaveBeenCalledWith(sessionId, '2', 1));
  });

  it('selects Plan for the next message without mutating the completed Ask work item', async () => {
    const response = reviewedPlan();
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.summary.mode = 'ask';
    response.result.session.summary.state = 'completed';
    response.result.session.summary.currentPlanRevision = null;
    const sessionSummary = response.result.session.summary;
    const messageSubmitter = vi.fn(async () => reviewedPlan());
    render(AgentWorkspace, {
      activeProject: true,
      messageSubmitter,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [sessionSummary],
          status: 'available',
        },
      })),
    });

    await fireEvent.click(
      await screen.findByRole('button', { name: /Plan\s*Gemeinsam ausarbeiten/u }),
    );
    await fireEvent.input(screen.getByLabelText('Nachricht an A^3'), {
      target: { value: 'Plane die nächste Änderung' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht senden' }));

    await waitFor(() =>
      expect(messageSubmitter).toHaveBeenCalledWith({
        expectedSessionRevision: '2',
        message: 'Plane die nächste Änderung',
        mode: 'plan',
        researchDepth: 'standard',
        sessionId,
      }),
    );
  });

  it('shows a compact continuation instead of repeating the original question', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.entries[0].text =
      'Recherche fortsetzen. Ursprüngliche Frage:\nUntersuche den REST-API Server und router.py.';
    const summary = response.result.session.summary;
    render(AgentWorkspace, {
      activeProject: true,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: { nextCursor: null, sessions: [summary], status: 'available' },
      })),
    });
    expect(await screen.findByText('Recherche fortsetzen')).toBeTruthy();
    expect(screen.queryByText(/Ursprüngliche Frage:/u)).toBeNull();
    expect(screen.queryByText(/Untersuche den REST-API Server/u)).toBeNull();
  });

  it('keeps polling a running Ask session after a transient read failure', async () => {
    const running = askSession('running');
    const completed = askSession('completed');
    if (running.result.status !== 'available') throw new Error('fixture must be available');
    const runningSummary = running.result.session.summary;
    let detailReads = 0;
    const sessionLoader = vi.fn(async () => {
      detailReads += 1;
      if (detailReads === 2) throw new Error('transient read failure');
      return detailReads >= 3 ? completed : running;
    });
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 5,
      sessionLoader,
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [runningSummary],
          status: 'available',
        },
      })),
    });

    await waitFor(() => expect(screen.getAllByText('A^3 arbeitet').length).toBeGreaterThan(0));
    const liveResearch = container.querySelector('.messages details.ask-research');
    expect(liveResearch).not.toBeNull();
    await screen.findByText('A^3 ist ein evidenzgebundener Coding-Agent.');
    await waitFor(() => {
      const researchSummaries = screen.getAllByText('Recherche & Quellen');
      expect(researchSummaries).toHaveLength(1);
      expect(researchSummaries.some((summary) => summary.closest('details')?.open === true)).toBe(
        true,
      );
    });
    expect(container.querySelector('.messages details.ask-research')).toBe(liveResearch);
    expect(detailReads).toBeGreaterThanOrEqual(3);
  });

  it('keeps the latest research turn mounted when a session poll regresses', async () => {
    const running = askSession('running');
    if (running.result.status !== 'available') throw new Error('fixture must be available');
    const runningSummary = running.result.session.summary;
    const regressive = structuredClone(running);
    if (regressive.result.status !== 'available') throw new Error('fixture must be available');
    regressive.result.session.entries = [];
    regressive.result.session.summary.revision = '2';
    let reads = 0;
    const sessionLoader = vi.fn(async () => {
      reads += 1;
      return reads === 1 ? running : regressive;
    });
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 5,
      sessionLoader,
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [runningSummary],
          status: 'available',
        },
      })),
    });

    await screen.findByText('Was macht A^3?');
    const research = container.querySelector('.messages details.ask-research');
    expect(research).not.toBeNull();
    await waitFor(() => expect(reads).toBeGreaterThanOrEqual(2));
    expect(container.querySelector('.messages .user-message')?.textContent).toContain(
      'Was macht A^3?',
    );
    expect(container.querySelector('.messages details.ask-research')).toBe(research);
  });

  it('coalesces live follow and respects manual browsing, layout clamping and cleanup', async () => {
    let observerCount = 0;
    let notifyResize = () => {};
    const disconnect = vi.fn();
    class ResizeObserverMock {
      constructor(callback: ResizeObserverCallback) {
        notifyResize = () => callback([], this);
        observerCount += 1;
      }

      observe(): void {}
      disconnect = disconnect;
      unobserve(): void {}
    }
    vi.stubGlobal('ResizeObserver', ResizeObserverMock);

    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const session = response.result.session;
    const { container, unmount } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 60_000,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [session.summary],
          status: 'available',
        },
      })),
    });

    await screen.findByText('A^3 ist ein evidenzgebundener Coding-Agent.');
    const viewport = container.querySelector<HTMLDivElement>('.message-scroll');
    if (!viewport) throw new Error('scroll fixture was not initialized');
    let scrollHeight = 1_000;
    Object.defineProperty(viewport, 'clientHeight', { configurable: true, value: 300 });
    Object.defineProperty(viewport, 'scrollHeight', {
      configurable: true,
      get: () => scrollHeight,
    });

    notifyResize();
    await waitFor(() => expect(viewport.scrollTop).toBe(700));
    scrollHeight = 1_200;
    for (let count = 0; count < 30; count += 1) notifyResize();
    expect(viewport.scrollTop).toBe(700); // no write in observer delivery
    await waitFor(() => expect(viewport.scrollTop).toBe(900));

    await fireEvent.pointerDown(viewport);
    viewport.scrollTop = 420;
    await fireEvent.scroll(viewport);
    scrollHeight = 1_300;
    notifyResize();
    await new Promise((resolve) => window.setTimeout(resolve, 25));
    expect(viewport.scrollTop).toBe(420);
    // Shrinking content can clamp to the end; it must not reattach the reader.
    scrollHeight = 600;
    viewport.scrollTop = 300;
    await fireEvent.scroll(viewport);
    scrollHeight = 1_400;
    notifyResize();
    await new Promise((resolve) => window.setTimeout(resolve, 25));
    expect(viewport.scrollTop).toBe(300);
    await fireEvent.click(screen.getByRole('button', { name: /Zum neuesten Schritt/ }));
    await waitFor(() => expect(viewport.scrollTop).toBe(1_100));
    await fireEvent.wheel(viewport, { deltaY: -100 });
    viewport.scrollTop = 900;
    await fireEvent.scroll(viewport);
    await fireEvent.wheel(viewport, { deltaY: 100 });
    viewport.scrollTop = 1_100;
    await fireEvent.scroll(viewport);
    scrollHeight = 1_500;
    notifyResize();
    await waitFor(() => expect(viewport.scrollTop).toBe(1_200));
    expect(observerCount).toBe(1);
    unmount();
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it('keeps the same research element and disclosure when the next user turn starts', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const session = response.result.session;
    const next = structuredClone(session);
    next.summary.revision = '3';
    next.summary.state = 'running';
    next.entries.push({ ...next.entries[0], sequence: '3', text: 'Und die Tests?' });
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 60_000,
      sessionLoader: vi.fn(async () => response),
      messageSubmitter: vi.fn(async (): Promise<AgentSessionResponseV1> => ({
        protocolVersion: 1,
        result: { status: 'available', session: next },
      })),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: { nextCursor: null, sessions: [session.summary], status: 'available' },
      })),
    });
    await screen.findByText('A^3 ist ein evidenzgebundener Coding-Agent.');
    const research = container.querySelector<HTMLDetailsElement>('.ask-research');
    if (!research) throw new Error('missing research');
    await fireEvent.click(research.querySelector('summary') as HTMLElement);
    expect(research.open).toBe(true);
    await fireEvent.input(screen.getByLabelText('Nachricht an A^3'), {
      target: { value: 'Und die Tests?' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht senden' }));
    await screen.findByText('Und die Tests?');
    await waitFor(() => expect(container.querySelectorAll('.ask-research')).toHaveLength(2));
    expect(container.querySelector('.ask-research')).toBe(research);
    expect(research.open).toBe(true);
  });

  it('keeps one polling owner across fresh running-session projections', async () => {
    const response = askSession('running');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const summary = response.result.session.summary;
    const scheduleTimer = vi.spyOn(window, 'setTimeout');
    const clearTimer = vi.spyOn(window, 'clearTimeout');
    const sessionLoader = vi.fn(async () => structuredClone(response));
    const view = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 37,
      sessionLoader,
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: { nextCursor: null, sessions: [summary], status: 'available' },
      })),
    });
    await waitFor(() => expect(sessionLoader.mock.calls.length).toBeGreaterThanOrEqual(4));
    const ownedTimers = new Set<unknown>();
    scheduleTimer.mock.calls.forEach(([, timeout], index) => {
      const result = scheduleTimer.mock.results[index];
      if (timeout === 37 && result?.type === 'return') ownedTimers.add(result.value);
    });
    expect(ownedTimers.size).toBeGreaterThanOrEqual(3);
    expect(
      clearTimer.mock.calls.filter(([timer]) => timer !== undefined && ownedTimers.has(timer)),
    ).toHaveLength(0);
    view.unmount();
    expect(
      clearTimer.mock.calls.some(([timer]) => timer !== undefined && ownedTimers.has(timer)),
    ).toBe(true);
  });

  it('polls only the latest research turn, not historical traces with fresh parent objects', async () => {
    const warn = vi.spyOn(console, 'warn');
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.entries.push({
      ...response.result.session.entries[0],
      sequence: '3',
      text: 'Weiter recherchieren',
    });
    response.result.session.summary.state = 'running';
    const summary = response.result.session.summary;
    const sessionLoader = vi.fn(async () => structuredClone(response));
    const researchProjectionLoader = vi.fn(async (_session: string, userSequence: string) => ({
      protocolVersion: 1 as const,
      result: {
        status: 'available' as const,
        projectionRef: 'c'.repeat(128),
        nextCursor: null,
        sources: [],
        detail: {
          userSequence,
          citedSourceCount: 0,
          sourceCount: 0,
          depth: 'standard' as const,
          legacy: false,
          mode: 'ask' as const,
          stale: false,
          steps: [],
        },
      },
    }));
    const view = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 20,
      sessionLoader,
      researchProjectionLoader,
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      })),
    });
    await waitFor(() => expect(sessionLoader.mock.calls.length).toBeGreaterThanOrEqual(5));
    expect(researchProjectionLoader.mock.calls.filter(([, turn]) => turn === '1')).toHaveLength(1);
    expect(
      researchProjectionLoader.mock.calls.filter(([, turn]) => turn === '3').length,
    ).toBeGreaterThanOrEqual(4);
    expect(
      warn.mock.calls
        .flat()
        .some((message) => String(message).includes('state_proxy_equality_mismatch')),
    ).toBe(false);
    view.unmount();
  });

  it('does not remount a completed diagram while the following Ask turn is polled', async () => {
    diagramRenderer.render.mockClear();
    const diagramSummary = {
      artifactRef: 'f'.repeat(128),
      description: 'Bereits vollständig gerenderter Ablauf',
      kind: 'flowchart' as const,
      stale: false,
      title: 'Vorheriger Ablauf',
      userSequence: '1',
    };
    const response: AgentSessionResponseV1 = {
      protocolVersion: 1,
      result: {
        session: {
          activeTaskId: null,
          entries: [
            {
              createdAtUnixMillis: '100',
              diagrams: [diagramSummary],
              kind: 'userMessage',
              planRevision: null,
              sequence: '1',
              text: '/diagram Zeige den Ablauf',
            },
            {
              createdAtUnixMillis: '101',
              kind: 'finalReport',
              planRevision: null,
              sequence: '2',
              text: 'Der belegte Ablauf.',
            },
            {
              createdAtUnixMillis: '102',
              kind: 'userMessage',
              planRevision: null,
              sequence: '3',
              text: 'Erkläre den nächsten Teil.',
            },
          ],
          hasOlderEntries: false,
          summary: {
            currentPlanRevision: null,
            mode: 'ask',
            revision: '3',
            sessionId,
            state: 'running',
            title: 'Ablauf erklären',
            updatedAtUnixMillis: '102',
          },
        },
        status: 'available',
      },
    };
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        artifact: {
          mermaid: 'flowchart TD\n  n0["Start"]\n',
          summary: diagramSummary,
        },
        kind: 'available' as const,
      },
    }));
    const sessionLoader = vi.fn(async () => structuredClone(response));
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      diagramArtifactLoader: artifactLoader,
      pollIntervalMs: 5,
      sessionLoader,
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [
            response.result.status === 'available'
              ? response.result.session.summary
              : (() => {
                  throw new Error('fixture must be available');
                })(),
          ],
          status: 'available',
        },
      })),
    });

    await waitFor(() => expect(artifactLoader).toHaveBeenCalledTimes(1));
    const mountedDiagram = container.querySelector('.diagram-section');
    expect(mountedDiagram).not.toBeNull();
    await waitFor(() => expect(sessionLoader.mock.calls.length).toBeGreaterThanOrEqual(3));

    expect(container.querySelector('.diagram-section')).toBe(mountedDiagram);
    expect(artifactLoader).toHaveBeenCalledTimes(1);
    expect(diagramRenderer.render).toHaveBeenCalledTimes(1);
  });

  it('projects Agent execution without internal identifiers or raw event codes', async () => {
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    const session = response.result.session;
    render(AgentWorkspace, {
      activeProject: true,
      activityLoader: vi.fn(async () => activeAgentActivity()),
      pollIntervalMs: 60_000,
      sessionLoader: vi.fn(async () => response),
      sessionsLoader: vi.fn(async (): Promise<AgentSessionsResponseV1> => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [session.summary],
          status: 'available',
        },
      })),
      workPlanLoader: vi.fn(async () => adaptiveWorkPlan()),
    });

    expect(await screen.findByText('Änderungen werden umgesetzt')).toBeTruthy();
    expect(screen.getByRole('complementary', { name: 'Agentenlauf' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Fortschritt' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Änderungen' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Review' })).toBeTruthy();
    expect(screen.getByText('Umsetzung vorbereitet')).toBeTruthy();
    expect(screen.getByText('Sichere Aktion ausgeführt')).toBeTruthy();
    expect(screen.getByRole('heading', { name: '1 von 3 Schritten erledigt' })).toBeTruthy();
    expect(screen.getByText('Serializer ergänzen und Adapter anbinden')).toBeTruthy();
    expect(screen.getByText('Integrationstests ausführen')).toBeTruthy();
    expect(screen.getByText(/Nach einem neuen Befund angepasst/)).toBeTruthy();
    expect(screen.queryByText('controllerDecision')).toBeNull();
    expect(screen.queryByText('policyDecision')).toBeNull();
    expect(screen.queryByText(/dddddddd/u)).toBeNull();
    expect(screen.queryByText(/eeeeeeee/u)).toBeNull();
  });
});

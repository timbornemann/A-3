import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import AgentWorkspace from './AgentWorkspace.svelte';
import * as sessionApi from './agent-session';
import type { AgentActivityResponseV1 } from './agent-activity';
import type { TaskLensTaskResponseV1 } from './task-lens';
import { patchApprovalResponse } from './agent-approval.fixture';
import type {
  AgentSessionControlActionV1,
  AgentSessionResponseV1,
  AgentSessionsResponseV1,
  AgentSlashCommandsResponseV1,
} from './agent-session';
import {
  activeAgentSession,
  activeAgentActivity,
  adaptiveWorkPlan,
} from './agent-execution.fixture';

// This suite verifies conversation/poll ownership, not Mermaid's layout in jsdom.
// Renderer and sanitizer contracts live in AgentDiagrams and agent-diagram-rendering tests.
const diagramRenderer = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn(async () => ({
    svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>Start</text></svg>',
  })),
}));
vi.mock('mermaid', () => ({ default: diagramRenderer }));
// Global settings have their own IPC/component contract; workspace tests own conversation state.
vi.mock('./agent-permissions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./agent-permissions')>()),
  queryAgentPermissions: vi.fn(async () => ({
    protocolVersion: 1,
    revision: '1',
    mode: 'askPermissions',
  })),
  subscribeAgentPermissions: vi.fn(async () => () => {}),
}));

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

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('AgentWorkspace', () => {
  it('keeps research before the compact plan and opens all plan content with focus return', async () => {
    const completed = reviewedPlan();
    if (completed.result.status !== 'available') throw new Error('available fixture required');
    let response = structuredClone(completed);
    if (response.result.status !== 'available') throw new Error('available fixture required');
    response.result.session.summary.state = 'running';
    response.result.session.entries = response.result.session.entries.slice(0, 1);
    const summary = structuredClone(response.result.session.summary);
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 30,
      sessionLoader: async () => structuredClone(response),
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
    });
    await screen.findByText('Überarbeite den Agent Workspace');
    const research = container.querySelector('.ask-research');
    expect(research).not.toBeNull();
    response = completed;
    const trigger = await screen.findByRole('button', { name: 'Plan öffnen' });
    expect(container.querySelector('.ask-research')).toBe(research);
    const plan = container.querySelector('.plan-message');
    if (!research || !plan) throw new Error('missing conversation cards');
    expect(research.compareDocumentPosition(plan) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText('Ein exakter Implementierungsplan')).toBeNull();
    trigger.focus();
    await fireEvent.click(trigger);
    const dialog = screen.getByRole('dialog', { name: 'Vorgehensplan' });
    expect(dialog.tagName).toBe('DIALOG');
    expect(within(dialog).getByText('Ein exakter Implementierungsplan')).toBeTruthy();
    expect(document.activeElement).toBe(
      within(dialog).getByRole('button', { name: 'Dialog schließen' }),
    );
    await fireEvent(dialog, new Event('cancel', { cancelable: true }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(trigger);
    expect(container.querySelector('.ask-research')).toBe(research);
  });

  it('retains the draft and approval choice across dock switches and resets on another task', async () => {
    let response = activeAgentSession();
    const activity = activeAgentActivity();
    if (activity.result.status !== 'available' || !activity.result.activity.run)
      throw new Error('run fixture required');
    const run = activity.result.activity.run;
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = structuredClone(response.result.session.summary);
    const controller = vi.fn();
    const sessionLoader = vi.fn(async () => structuredClone(response));
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 20,
      sessionLoader,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
      approvalLoader: async () => patchApprovalResponse(),
      approvalController: controller,
      activityLoader: async () => structuredClone(activity),
      workPlanLoader: async () => adaptiveWorkPlan(),
    });
    const input = await screen.findByRole('textbox', { name: 'Nachricht an A^3' });
    await fireEvent.input(input, { target: { value: 'Bitte auch den Randfall prüfen.' } });
    response = structuredClone(response);
    if (response.result.status !== 'available') throw new Error('available fixture required');
    run.state = 'awaitApproval';
    response.result.session.summary.revision = '2';
    const allow = await screen.findByRole<HTMLInputElement>('radio', {
      name: 'Diese Aktion einmal erlauben',
    });
    expect(allow.checked).toBe(false);
    expect(
      screen.getByRole<HTMLButtonElement>('button', { name: 'Entscheidung bestätigen' }).disabled,
    ).toBe(true);
    await fireEvent.click(allow);
    const approval = container.querySelector('.approval-center');
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht schreiben' }));
    expect(document.activeElement).toBe(input);
    expect((input as HTMLTextAreaElement).value).toBe('Bitte auch den Randfall prüfen.');
    const returnButton = screen.getByRole('button', { name: /Zur Freigabe/ });
    for (const state of ['awaitingApproval', 'running', 'awaitingApproval'] as const) {
      response.result.session.summary.state = state;
      response.result.session.summary.revision = String(
        Number(response.result.session.summary.revision) + 1,
      );
      const reads = sessionLoader.mock.calls.length;
      await waitFor(() => expect(sessionLoader.mock.calls.length).toBeGreaterThan(reads + 1));
      expect(container.querySelector('.approval-center')).toBe(approval);
      expect(screen.getByRole('textbox', { name: 'Nachricht an A^3' })).toBe(input);
      expect(screen.getByRole('button', { name: /Zur Freigabe/ })).toBe(returnButton);
    }
    expect(screen.getByRole('textbox', { name: 'Nachricht an A^3' })).toBe(input);
    await fireEvent.click(returnButton);
    expect(container.querySelector('.approval-center')).toBe(approval);
    expect(
      screen.getByRole<HTMLInputElement>('radio', { name: 'Diese Aktion einmal erlauben' }).checked,
    ).toBe(true);
    expect(controller).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht schreiben' }));
    response = structuredClone(response);
    if (response.result.status !== 'available') throw new Error('available fixture required');
    response.result.session.activeTaskId = 'c'.repeat(64);
    response.result.session.summary.revision = '6';
    await waitFor(() =>
      expect(screen.queryByRole('textbox', { name: 'Nachricht an A^3' })).toBeNull(),
    );
    await waitFor(() =>
      expect(
        screen.getByRole<HTMLInputElement>('radio', { name: 'Diese Aktion einmal erlauben' })
          .checked,
      ).toBe(false),
    );
    expect(controller).not.toHaveBeenCalled();
  });

  it('keeps the actual bottom reachable when execution focus is above the end', async () => {
    let notifyResize = () => {};
    class ResizeObserverMock {
      constructor(callback: ResizeObserverCallback) {
        notifyResize = () => callback([], this);
      }
      observe(): void {}
      disconnect(): void {}
      unobserve(): void {}
    }
    vi.stubGlobal('ResizeObserver', ResizeObserverMock);
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = response.result.session.summary;
    const { container } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 60_000,
      sessionLoader: async () => structuredClone(response),
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
      activityLoader: async () => activeAgentActivity(),
      workPlanLoader: async () => adaptiveWorkPlan(),
    });
    await screen.findByText('Serializer ergänzen und Adapter anbinden', {
      selector: '.execution-focus strong',
    });
    const viewport = container.querySelector<HTMLDivElement>('.message-scroll');
    const focus = container.querySelector<HTMLElement>('.execution-focus');
    if (!viewport || !focus) throw new Error('missing execution viewport');
    let height = 1_500;
    Object.defineProperty(viewport, 'clientHeight', { configurable: true, value: 400 });
    Object.defineProperty(viewport, 'scrollHeight', { configurable: true, get: () => height });
    vi.spyOn(viewport, 'getBoundingClientRect').mockReturnValue({ bottom: 400 } as DOMRect);
    vi.spyOn(focus, 'getBoundingClientRect').mockImplementation(
      () => ({ bottom: 1_000 - viewport.scrollTop }) as DOMRect,
    );
    await fireEvent.wheel(viewport, { deltaY: 100 });
    viewport.scrollTop = 1_100;
    await fireEvent.scroll(viewport);
    notifyResize();
    await new Promise((resolve) => window.setTimeout(resolve, 40));
    expect(viewport.scrollTop).toBe(1_100);
    // At the end another downward wheel gesture emits no native scroll event.
    await fireEvent.wheel(focus, { deltaY: 100 });
    height = 1_600;
    notifyResize();
    await waitFor(() => expect(viewport.scrollTop).toBe(1_200));
    await fireEvent.wheel(viewport, { deltaY: -200 });
    viewport.scrollTop = 800;
    await fireEvent.scroll(viewport);
    height = 1_700;
    notifyResize();
    await new Promise((resolve) => window.setTimeout(resolve, 40));
    expect(viewport.scrollTop).toBe(800);
  });

  it.each([false, true])(
    'does not insert another initial plan loader above the activity after a failed read (open=%s)',
    async (open) => {
      const response = activeAgentSession();
      if (response.result.status !== 'available') throw new Error('available fixture required');
      let rejectPlan: ((error: Error) => void) | undefined;
      const workPlanLoader = vi
        .fn()
        .mockRejectedValueOnce(new Error('invalid IPC response'))
        .mockImplementation(
          () => new Promise<TaskLensTaskResponseV1>((_resolve, reject) => (rejectPlan = reject)),
        );
      const { container, unmount } = render(AgentWorkspace, {
        activeProject: true,
        pollIntervalMs: 30,
        sessionLoader: async () => structuredClone(response),
        sessionsLoader: async () => ({
          protocolVersion: 1,
          result: {
            status: 'available',
            sessions:
              response.result.status === 'available' ? [response.result.session.summary] : [],
            nextCursor: null,
          },
        }),
        activityLoader: async () => activeAgentActivity(),
        workPlanLoader,
      });
      await screen.findByText(/Letzter bestätigter Stand/);
      const history = container.querySelector<HTMLButtonElement>('.execution-activity');
      if (!history) throw new Error('activity disclosure required');
      if (open) await fireEvent.click(history);
      const dialog = screen.queryByRole('dialog', { name: 'Aktivitätsverlauf' });
      for (let index = 0; index < 3; index += 1) {
        await waitFor(() => expect(workPlanLoader).toHaveBeenCalledTimes(index + 2));
        expect(screen.queryByText('Arbeitsplan wird geladen …')).toBeNull();
        expect(container.querySelector('.execution-activity')).toBe(history);
        expect(screen.queryByRole('dialog', { name: 'Aktivitätsverlauf' })).toBe(dialog);
        expect(Boolean(dialog)).toBe(open);
        rejectPlan?.(new Error('temporary read failure'));
      }
      unmount();
    },
  );

  it.each([false, true])(
    'keeps the activity disclosure mounted (open=%s) while a fresh session waits for its plan',
    async (open) => {
      const response = activeAgentSession();
      if (response.result.status !== 'available') throw new Error('available fixture required');
      const summary = response.result.session.summary;
      let finishPlan: ((response: TaskLensTaskResponseV1) => void) | undefined;
      const workPlanLoader = vi
        .fn()
        .mockResolvedValueOnce(adaptiveWorkPlan())
        .mockImplementation(
          () => new Promise<TaskLensTaskResponseV1>((resolve) => (finishPlan = resolve)),
        );
      let revision = 0;
      const { container, unmount } = render(AgentWorkspace, {
        activeProject: true,
        pollIntervalMs: 50,
        sessionLoader: async () => {
          const next = structuredClone(response);
          if (next.result.status === 'available')
            next.result.session.summary.revision = String(++revision);
          return next;
        },
        sessionsLoader: async () => ({
          protocolVersion: 1,
          result: { status: 'available', sessions: [summary], nextCursor: null },
        }),
        activityLoader: async () => activeAgentActivity(),
        workPlanLoader,
      });
      await screen.findByText('Serializer ergänzen und Adapter anbinden', {
        selector: '.execution-focus strong',
      });
      const history = container.querySelector<HTMLButtonElement>('.execution-activity');
      if (!history) throw new Error('activity disclosure required');
      if (open) await fireEvent.click(history);
      const dialog = screen.queryByRole('dialog', { name: 'Aktivitätsverlauf' });
      const plan = container.querySelector('.agent-work-plan');
      for (let index = 0; index < 3; index += 1) {
        await waitFor(() => expect(workPlanLoader).toHaveBeenCalledTimes(index + 2));
        expect(screen.queryByText('Arbeitsplan wird geladen …')).toBeNull();
        expect(container.querySelector('.execution-activity')).toBe(history);
        expect(screen.queryByRole('dialog', { name: 'Aktivitätsverlauf' })).toBe(dialog);
        expect(Boolean(dialog)).toBe(open);
        expect(container.querySelector('.agent-work-plan')).toBe(plan);
        finishPlan?.(adaptiveWorkPlan());
      }
      unmount();
    },
  );

  it('updates the history status when the selected running session needs approval', async () => {
    let response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = structuredClone(response.result.session.summary);
    const { unmount } = render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 20,
      sessionLoader: async () => structuredClone(response),
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
      activityLoader: async () => activeAgentActivity(),
      workPlanLoader: async () => adaptiveWorkPlan(),
      approvalLoader: async () => patchApprovalResponse(),
    });
    await screen.findByRole('button', { name: /Geprüfte Änderung umsetzen.*Arbeitet/ });
    response = structuredClone(response);
    if (response.result.status !== 'available') throw new Error('available fixture required');
    response.result.session.summary.state = 'awaitingApproval';
    response.result.session.summary.revision = '2';
    await screen.findByRole('button', { name: /Geprüfte Änderung umsetzen.*Freigabe nötig/ });
    unmount();
  });

  it('keeps an opened inspection mounted while new run events arrive', async () => {
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = response.result.session.summary;
    let timestamp = 101;
    const plan = adaptiveWorkPlan();
    if (plan.result.status === 'available') plan.result.steps[0].status = 'blocked';
    const activityLoader = vi.fn(async () => {
      const activity = activeAgentActivity();
      if (activity.result.status === 'available' && activity.result.activity.run)
        activity.result.activity.run.updatedAtUnixMillis = String(timestamp++);
      return activity;
    });
    const inspectionLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: { status: 'ledgerUnavailable' as const },
    }));
    const { container, unmount } = render(AgentWorkspace, {
      activeProject: true,
      activityLoader,
      inspectionLoader,
      pollIntervalMs: 20,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
      workPlanLoader: async () => plan,
    });
    await screen.findByRole('article', { name: 'Änderungen werden umgesetzt' });
    await waitFor(() =>
      expect(container.querySelector('.execution-focus strong')?.textContent).toBe(
        'Serializer ergänzen und Adapter anbinden',
      ),
    );
    expect(inspectionLoader).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Änderungen & Prüfungen' }));
    expect(screen.getByRole('button', { name: '↓ Zum neuesten Schritt' })).toBeTruthy();
    await screen.findByText('Für diese Aufgabe liegt noch kein prüfbarer Arbeitsplan vor.');
    const panel = container.querySelector('.inspection-panel');
    const calls = activityLoader.mock.calls.length;
    await waitFor(() => expect(activityLoader.mock.calls.length).toBeGreaterThan(calls + 2));
    expect(container.querySelector('.inspection-panel')).toBe(panel);
    expect(inspectionLoader.mock.calls.length).toBeGreaterThan(1);
    unmount();
  });

  it('keeps the known run status during delayed and failed background reads', async () => {
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available fixture required');
    const summary = response.result.session.summary;
    let rejectRefresh: ((error: Error) => void) | undefined;
    const activityLoader = vi
      .fn()
      .mockResolvedValueOnce(activeAgentActivity())
      .mockImplementation(
        () =>
          new Promise<AgentActivityResponseV1>((_resolve, reject) => {
            rejectRefresh = reject;
          }),
      );
    const onRunStatusChange = vi.fn();
    const { unmount } = render(AgentWorkspace, {
      activeProject: true,
      activityLoader,
      onRunStatusChange,
      pollIntervalMs: 20,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [summary],
          status: 'available',
        },
      }),
      workPlanLoader: async () => adaptiveWorkPlan(),
    });
    await waitFor(() =>
      expect(onRunStatusChange).toHaveBeenLastCalledWith({ kind: 'available', state: 'execute' }),
    );
    onRunStatusChange.mockClear();
    await waitFor(() => expect(activityLoader.mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(onRunStatusChange.mock.calls.every(([status]) => status.kind === 'available')).toBe(
      true,
    );
    rejectRefresh?.(new Error('temporary read failure'));
    await screen.findByText(/Letzter bestätigter Stand/);
    expect(onRunStatusChange.mock.calls.every(([status]) => status.kind === 'available')).toBe(
      true,
    );
    expect(screen.getByRole('article', { name: 'Änderungen werden umgesetzt' })).toBeTruthy();
    unmount();
  });

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

  it('uses bounded keyboard resizing for the conversation history', async () => {
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
    expect(history.getAttribute('tabindex')).toBe('0');
    expect(history.getAttribute('aria-orientation')).toBe('vertical');
    await fireEvent.keyDown(history, { key: 'Home' });
    expect(history.getAttribute('aria-valuenow')).toBe('220');
    await fireEvent.keyDown(history, { key: 'ArrowRight' });
    expect(history.getAttribute('aria-valuenow')).toBe('236');
    await fireEvent.keyDown(history, { key: 'End' });
    await fireEvent.keyDown(history, { key: 'ArrowRight' });
    expect(history.getAttribute('aria-valuenow')).toBe('360');
    expect(screen.queryByRole('separator', { name: 'Inspectorbreite ändern' })).toBeNull();
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
      { mode: 'agent', requiresPlanReview: false, selectable: true },
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

  it('honors the Core-owned selectable modes without an Agent plan-review halt', async () => {
    const response = askSession('completed');
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.modeOptions = [
      { mode: 'ask', requiresPlanReview: false, selectable: true },
      { mode: 'plan', requiresPlanReview: false, selectable: false },
      { mode: 'agent', requiresPlanReview: false, selectable: true },
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
    expect(agent.getAttribute('aria-pressed')).toBe('true');
    expect(agent.textContent).not.toContain('Nach Planfreigabe');
    expect(agent.textContent).toBe('Umsetzen');
    const ask = screen.getByRole('button', { name: /Ask\s*Nur lesen und antworten/u });
    await fireEvent.click(ask);
    expect(agent.getAttribute('aria-pressed')).toBe('false');
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
        false,
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

    const execution = await screen.findByRole('article', {
      name: 'Änderungen werden umgesetzt',
    });
    expect(screen.queryByRole('complementary', { name: 'Agentenlauf' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Fortschritt' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Änderungen' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Review' })).toBeNull();
    expect(
      within(execution).getAllByRole('heading', { name: 'Änderungen werden umgesetzt' }),
    ).toHaveLength(1);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(within(execution).queryByRole('region', { name: 'Dateien und Prüfungen' })).toBeNull();
    expect(within(execution).getByText('1 von 3 Schritten erledigt · 2 offen')).toBeTruthy();
    await fireEvent.click(within(execution).getByRole('button', { name: /Arbeitsschritte/ }));
    const steps = screen.getByRole('dialog', { name: 'Arbeitsschritte' });
    expect(within(steps).getByRole('list', { name: 'Alle Arbeitsschritte' })).toBeTruthy();
    expect(within(steps).getByText('Integrationstests ausführen')).toBeTruthy();
    expect(within(steps).getByText(/Nach einem neuen Befund angepasst/)).toBeTruthy();
    await fireEvent.click(within(steps).getByRole('button', { name: 'Dialog schließen' }));
    await fireEvent.click(within(execution).getByRole('button', { name: /Aktivitätsverlauf/ }));
    const history = screen.getByRole('dialog', { name: 'Aktivitätsverlauf' });
    expect(within(history).getByRole('list', { name: 'Aktivitäten des Agenten' })).toBeTruthy();
    expect(within(history).getByText('Umsetzung vorbereitet')).toBeTruthy();
    expect(within(history).getByText('Sichere Aktion ausgeführt')).toBeTruthy();
    expect(screen.queryByText('controllerDecision')).toBeNull();
    expect(screen.queryByText('policyDecision')).toBeNull();
    expect(screen.queryByText(/dddddddd/u)).toBeNull();
    expect(screen.queryByText(/eeeeeeee/u)).toBeNull();
  });

  it('shows a concrete terminal failure inside the conversation instead of only in history', async () => {
    const response = activeAgentSession();
    const activity = activeAgentActivity();
    if (response.result.status !== 'available' || activity.result.status !== 'available') {
      throw new Error('available fixtures required');
    }
    response.result.session.summary.state = 'failed';
    const summary = response.result.session.summary;
    const run = activity.result.activity.run;
    if (!run) throw new Error('run fixture required');
    run.state = 'failed';
    run.terminal = true;
    run.timeline.push({
      code: 'invalidModelOutput',
      event: { kind: 'modelInteraction', turn: null },
      occurredAtUnixMillis: '102',
      outcome: 'failed',
      sequence: '3',
      snapshotId: 'c'.repeat(64),
    });

    render(AgentWorkspace, {
      activeProject: true,
      activityLoader: async () => activity,
      pollIntervalMs: 60_000,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: {
          nextCursor: null,
          sessions: [summary],
          status: 'available',
        },
      }),
      workPlanLoader: async () => adaptiveWorkPlan(),
    });

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Warum der Lauf angehalten wurde');
    expect(alert.textContent).toContain('keine gültige, sicher ausführbare Aktion');
    expect(screen.queryByRole('complementary', { name: 'Agentenlauf' })).toBeNull();
  });

  it('ignores a late waiting run from the previous task and restores input when work continues', async () => {
    const response = activeAgentSession();
    if (response.result.status !== 'available') throw new Error('available session required');
    const session = response.result.session;
    const initialTaskId = session.activeTaskId;
    const waiting = activeAgentActivity();
    if (waiting.result.status !== 'available' || !waiting.result.activity.run)
      throw new Error('available run required');
    waiting.result.activity.run.state = 'awaitApproval';
    let finishOldRead: ((value: AgentActivityResponseV1) => void) | undefined;
    let currentActivity = activeAgentActivity();
    const activityLoader = vi.fn((taskId: string) =>
      taskId === initialTaskId
        ? new Promise<AgentActivityResponseV1>((resolve) => (finishOldRead = resolve))
        : Promise.resolve(structuredClone(currentActivity)),
    );
    const approvalLoader = vi.fn(async () => patchApprovalResponse());
    const approvalController = vi.fn();
    render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 20,
      sessionLoader: async () => structuredClone(response),
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: {
          status: 'available',
          sessions: [structuredClone(session.summary)],
          nextCursor: null,
        },
      }),
      activityLoader,
      approvalLoader,
      approvalController,
      workPlanLoader: async () => adaptiveWorkPlan(),
    });
    const input = await screen.findByRole('textbox', { name: 'Nachricht an A^3' });
    await fireEvent.input(input, { target: { value: 'Entwurf für den nächsten Schritt' } });
    await waitFor(() => expect(finishOldRead).toBeDefined());
    session.activeTaskId = 'c'.repeat(64);
    session.summary.revision = '2';
    await waitFor(() => expect(activityLoader).toHaveBeenCalledWith(session.activeTaskId));
    finishOldRead?.(waiting);
    await screen.findByRole('article', { name: 'Änderungen werden umgesetzt' });
    expect(approvalLoader).not.toHaveBeenCalled();
    expect(screen.queryByRole('region', { name: 'Erforderliche Freigabe' })).toBeNull();

    currentActivity = waiting;
    await screen.findByRole('heading', { name: 'Aktion freigeben' });
    expect(approvalLoader).toHaveBeenCalledWith(session.activeTaskId);
    expect(screen.queryByRole('textbox', { name: 'Nachricht an A^3' })).toBeNull();
    currentActivity = activeAgentActivity();
    expect(await screen.findByRole('textbox', { name: 'Nachricht an A^3' })).toBe(input);
    expect((input as HTMLTextAreaElement).value).toBe('Entwurf für den nächsten Schritt');
    expect(screen.queryByRole('region', { name: 'Erforderliche Freigabe' })).toBeNull();
    expect(approvalController).not.toHaveBeenCalled();
  });

  it.each(['completed', 'failed', 'cancelled', 'archived', 'paused'] as const)(
    'does not reopen approval for a %s session with a retained waiting run',
    async (state) => {
      const response = activeAgentSession();
      const activity = activeAgentActivity();
      if (
        response.result.status !== 'available' ||
        activity.result.status !== 'available' ||
        !activity.result.activity.run
      )
        throw new Error('available fixtures required');
      response.result.session.summary.state = state;
      activity.result.activity.run.state = 'awaitApproval';
      const summary = response.result.session.summary;
      const approvalLoader = vi.fn(async () => patchApprovalResponse());
      render(AgentWorkspace, {
        activeProject: true,
        pollIntervalMs: 60_000,
        sessionLoader: async () => response,
        sessionsLoader: async () => ({
          protocolVersion: 1,
          result: { status: 'available', sessions: [summary], nextCursor: null },
        }),
        activityLoader: async () => activity,
        approvalLoader,
        workPlanLoader: async () => adaptiveWorkPlan(),
      });
      await screen.findByRole('article', { name: 'Wartet auf deine Freigabe' });
      expect(screen.getByRole('textbox', { name: 'Nachricht an A^3' })).toBeTruthy();
      expect(approvalLoader).not.toHaveBeenCalled();
    },
  );

  it('does not offer an approval decision when the exact action is unavailable', async () => {
    const response = activeAgentSession();
    const activity = activeAgentActivity();
    if (
      response.result.status !== 'available' ||
      activity.result.status !== 'available' ||
      !activity.result.activity.run
    )
      throw new Error('available fixtures required');
    const summary = response.result.session.summary;
    activity.result.activity.run.state = 'awaitApproval';
    const approvalController = vi.fn();
    render(AgentWorkspace, {
      activeProject: true,
      pollIntervalMs: 60_000,
      sessionLoader: async () => response,
      sessionsLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', sessions: [summary], nextCursor: null },
      }),
      activityLoader: async () => activity,
      approvalLoader: async () => ({ protocolVersion: 1, result: { status: 'unavailable' } }),
      approvalController,
      workPlanLoader: async () => adaptiveWorkPlan(),
    });
    await screen.findByText('Für diese Aufgabe ist gerade keine Freigabe erforderlich.');
    expect(screen.queryByRole('radio', { name: 'Diese Aktion einmal erlauben' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Entscheidung bestätigen' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht schreiben' }));
    expect(screen.getByRole('textbox', { name: 'Nachricht an A^3' })).toBeTruthy();
    expect(approvalController).not.toHaveBeenCalled();
  });

  it.each(['running', 'awaitingApproval'] as const)(
    'shows the waiting run approval outside the conversation while the session is %s',
    async (sessionState) => {
      const response = activeAgentSession();
      const activity = activeAgentActivity();
      if (response.result.status !== 'available' || activity.result.status !== 'available') {
        throw new Error('available fixtures required');
      }
      response.result.session.summary.state = sessionState;
      const summary = response.result.session.summary;
      if (!activity.result.activity.run) throw new Error('run fixture required');
      activity.result.activity.run.state = 'awaitApproval';

      const approvalController = vi.fn();
      const { container } = render(AgentWorkspace, {
        activeProject: true,
        activityLoader: async () => activity,
        approvalLoader: async () => patchApprovalResponse(),
        approvalController,
        pollIntervalMs: 60_000,
        sessionLoader: async () => response,
        sessionsLoader: async () => ({
          protocolVersion: 1,
          result: {
            nextCursor: null,
            sessions: [summary],
            status: 'available',
          },
        }),
        workPlanLoader: async () => adaptiveWorkPlan(),
      });

      const heading = await screen.findByRole('heading', { name: 'Aktion freigeben' });
      expect(heading.closest('.message-scroll')).toBeNull();
      expect(heading.closest('.composer-wrap')).not.toBeNull();
      const permissions = screen.getByRole('group', { name: 'Appweite Agent-Berechtigungen' });
      expect(permissions.closest('.composer-wrap')).not.toBeNull();
      expect(within(permissions).getByRole('button', { name: 'Full machine' })).toBeTruthy();
      expect(within(permissions).getByRole('button', { name: 'Ask permissions' })).toBeTruthy();
      expect(screen.queryByRole('textbox', { name: 'Nachricht an A^3' })).toBeNull();
      expect(screen.getByRole('radio', { name: 'Diese Aktion einmal erlauben' })).toBeTruthy();
      expect(
        screen.getByRole<HTMLInputElement>('radio', { name: 'Diese Aktion einmal erlauben' })
          .checked,
      ).toBe(false);
      expect(
        screen.getByRole<HTMLButtonElement>('button', { name: 'Entscheidung bestätigen' }).disabled,
      ).toBe(true);
      expect(container.querySelector('.conversation-heading p')?.textContent).toContain(
        'Freigabe nötig',
      );
      expect(container.querySelector('.execution-state')?.textContent).toContain('Freigabe nötig');
      expect(
        screen.getByRole('button', { name: /Geprüfte Änderung umsetzen.*Freigabe nötig/ }),
      ).toBeTruthy();
      expect(approvalController).not.toHaveBeenCalled();
      expect(screen.queryByRole('button', { name: 'Review' })).toBeNull();
    },
  );
});

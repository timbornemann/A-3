import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import AgentWorkspace from './AgentWorkspace.svelte';
import type { AgentSessionResponseV1, AgentSessionsResponseV1 } from './agent-session';

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

describe('AgentWorkspace', () => {
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

  it('sends only the selected new-session mode and visible message', async () => {
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
    await fireEvent.input(screen.getByLabelText('Nachricht an A^3'), {
      target: { value: 'Wie funktioniert der Index?' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Nachricht senden' }));

    await waitFor(() =>
      expect(messageSubmitter).toHaveBeenCalledWith({
        message: 'Wie funktioniert der Index?',
        mode: 'ask',
      }),
    );
  });

  it('implements only the exact visible plan revision', async () => {
    const response = reviewedPlan();
    const sessionController = vi.fn(async () => response);
    render(AgentWorkspace, {
      activeProject: true,
      sessionController,
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
    await waitFor(() =>
      expect(sessionController).toHaveBeenCalledWith(sessionId, '2', {
        kind: 'implementPlan',
        planRevision: 1,
      }),
    );
  });

  it('turns a completed Ask session into a planning conversation through an explicit control', async () => {
    const response = reviewedPlan();
    if (response.result.status !== 'available') throw new Error('fixture must be available');
    response.result.session.summary.mode = 'ask';
    response.result.session.summary.state = 'completed';
    response.result.session.summary.currentPlanRevision = null;
    const sessionSummary = response.result.session.summary;
    const sessionController = vi.fn(async () => response);
    render(AgentWorkspace, {
      activeProject: true,
      sessionController,
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

    await fireEvent.click(await screen.findByRole('button', { name: 'Session-Aktionen' }));
    await fireEvent.click(screen.getByRole('button', { name: 'In Plan wechseln' }));

    await waitFor(() =>
      expect(sessionController).toHaveBeenCalledWith(sessionId, '2', { kind: 'switchToPlan' }),
    );
  });
});

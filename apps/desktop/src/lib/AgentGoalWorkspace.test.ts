import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import AgentGoalWorkspace from './AgentGoalWorkspace.svelte';
import type { AgentActivityResponseV1 } from './agent-activity';
import type {
  AgentGoalContractV1,
  AgentGoalDraftInputV1,
  AgentGoalMutationResponseV1,
  AgentGoalResponseV1,
} from './agent-goal';
import type { TaskLensTaskResponseV1, TaskLensTasksResponseV1 } from './task-lens';

const taskId = '1'.repeat(64);
const criterionId = '2'.repeat(64);

function goal(revision = 1): AgentGoalContractV1 {
  return {
    acceptanceCriteria: [
      {
        criterionId,
        requirement: 'must',
        statement: 'Das Ziel bleibt während der Bearbeitung sichtbar.',
      },
    ],
    constraints: ['Lokal und offline bleiben.'],
    createdAtUnixMillis: revision === 1 ? '1786000000000' : '1786000001000',
    nonGoals: ['Noch keinen Agent Run starten.'],
    objective: revision === 1 ? 'Agent Workspace aufbauen' : 'Agent Workspace vollständig aufbauen',
    previousRevision: revision === 1 ? null : revision - 1,
    revision,
    revisionReason: revision === 1 ? null : 'Ziel präzisiert',
    successVerification: 'Die gespeicherte Revision erneut laden und vergleichen.',
    taskId,
    userDecisions: ['Revisionen bleiben unveränderlich.'],
  };
}

function tasks(revision: number): TaskLensTasksResponseV1 {
  return {
    protocolVersion: 1,
    result: {
      status: 'available',
      tasks: [{ goalRevision: revision, objective: goal(revision).objective, taskId }],
      truncated: false,
    },
  };
}

function availableGoal(revision: number): AgentGoalResponseV1 {
  return { protocolVersion: 1, result: { goal: goal(revision), status: 'available' } };
}

describe('AgentGoalWorkspace', () => {
  it('does not cross the Core boundary without an active project', () => {
    const tasksLoader = vi.fn<() => Promise<TaskLensTasksResponseV1>>();

    render(AgentGoalWorkspace, { activeProject: false, tasksLoader });

    expect(screen.getByText(/Öffne einen lokalen Worktree/)).toBeTruthy();
    expect(tasksLoader).not.toHaveBeenCalled();
  });

  it('creates a complete durable Goal Contract while Core assigns every identity', async () => {
    const initialTasks: TaskLensTasksResponseV1 = {
      protocolVersion: 1,
      result: { status: 'available', tasks: [], truncated: false },
    };
    const tasksLoader = vi
      .fn<() => Promise<TaskLensTasksResponseV1>>()
      .mockResolvedValueOnce(initialTasks)
      .mockResolvedValue(tasks(1));
    const goalCreator = vi.fn<
      (draft: AgentGoalDraftInputV1) => Promise<AgentGoalMutationResponseV1>
    >(async () => ({ goal: goal(1), protocolVersion: 1 }));
    const goalLoader = vi.fn<(selectedTaskId: string) => Promise<AgentGoalResponseV1>>(async () =>
      availableGoal(1),
    );

    render(AgentGoalWorkspace, {
      activeProject: true,
      goalCreator,
      goalLoader,
      tasksLoader,
    });

    expect(await screen.findByRole('heading', { name: 'Goal Contract anlegen' })).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Ziel'), {
      target: { value: 'Agent Workspace aufbauen' },
    });
    await fireEvent.input(screen.getByLabelText('Kriterium 1'), {
      target: { value: 'Das Ziel bleibt während der Bearbeitung sichtbar.' },
    });
    await fireEvent.input(screen.getByLabelText('Abschlussprüfung'), {
      target: { value: 'Die gespeicherte Revision erneut laden und vergleichen.' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Goal Contract anlegen' }));

    await waitFor(() => expect(goalCreator).toHaveBeenCalledTimes(1));
    expect(goalCreator).toHaveBeenCalledWith({
      acceptanceCriteria: [
        {
          criterionId: null,
          requirement: 'must',
          statement: 'Das Ziel bleibt während der Bearbeitung sichtbar.',
        },
      ],
      constraints: [],
      nonGoals: [],
      objective: 'Agent Workspace aufbauen',
      successVerification: 'Die gespeicherte Revision erneut laden und vergleichen.',
      userDecisions: [],
    });
    expect(await screen.findByText('Goal Contract dauerhaft angelegt.')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Agent Workspace aufbauen' })).toBeTruthy();
    expect(screen.getByText('Muss')).toBeTruthy();
    expect(goalLoader).toHaveBeenCalledWith(taskId);
  });

  it('appends a revision against the visible predecessor and retains criterion identities', async () => {
    const tasksLoader = vi
      .fn<() => Promise<TaskLensTasksResponseV1>>()
      .mockResolvedValueOnce(tasks(1))
      .mockResolvedValue(tasks(2));
    const goalLoader = vi
      .fn<(selectedTaskId: string) => Promise<AgentGoalResponseV1>>()
      .mockResolvedValueOnce(availableGoal(1))
      .mockResolvedValue(availableGoal(2));
    const goalReviser = vi.fn<
      (
        selectedTaskId: string,
        expectedRevision: number,
        reason: string,
        draft: AgentGoalDraftInputV1,
      ) => Promise<AgentGoalMutationResponseV1>
    >(async () => ({ goal: goal(2), protocolVersion: 1 }));

    render(AgentGoalWorkspace, {
      activeProject: true,
      goalLoader,
      goalReviser,
      tasksLoader,
    });

    expect(await screen.findByRole('heading', { name: 'Agent Workspace aufbauen' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Neue Revision' }));
    await fireEvent.input(screen.getByLabelText('Änderungsgrund'), {
      target: { value: 'Ziel präzisiert' },
    });
    await fireEvent.input(screen.getByLabelText('Ziel'), {
      target: { value: 'Agent Workspace vollständig aufbauen' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Neue Revision anhängen' }));

    await waitFor(() => expect(goalReviser).toHaveBeenCalledTimes(1));
    const [selectedTaskId, expectedRevision, reason, submittedDraft] = goalReviser.mock.calls[0];
    expect({ expectedRevision, reason, selectedTaskId }).toEqual({
      expectedRevision: 1,
      reason: 'Ziel präzisiert',
      selectedTaskId: taskId,
    });
    expect(submittedDraft.acceptanceCriteria[0]?.criterionId).toBe(criterionId);
    expect(submittedDraft.objective).toBe('Agent Workspace vollständig aufbauen');
    expect(await screen.findByText('Revision 2 dauerhaft angehängt.')).toBeTruthy();
    expect(
      screen.getByRole('heading', { name: 'Agent Workspace vollständig aufbauen' }),
    ).toBeTruthy();
    expect(screen.getByText('Änderungsgrund: Ziel präzisiert')).toBeTruthy();
  });

  it('keeps the durable goal and owned current step visible together', async () => {
    const ledgerLoader = vi.fn<(query: { taskId: string }) => Promise<TaskLensTaskResponseV1>>(
      async () => ({
        protocolVersion: 1,
        result: {
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          status: 'available',
          steps: [
            {
              intendedOutcome: 'Core-Grenze implementieren',
              status: 'inProgress',
              stepId: '3'.repeat(64),
            },
            { intendedOutcome: 'Gesamtgate ausführen', status: 'pending', stepId: '4'.repeat(64) },
          ],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
    );

    render(AgentGoalWorkspace, {
      activeProject: true,
      goalLoader: async () => availableGoal(1),
      ledgerLoader,
      tasksLoader: async () => tasks(1),
    });

    expect(await screen.findByRole('heading', { name: 'Agent Workspace aufbauen' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Core-Grenze implementieren' })).toBeTruthy();
    expect(screen.getByText('Ledger R3 · Store 7')).toBeTruthy();
    expect(screen.getAllByText('In Arbeit')).toHaveLength(2);
    expect(screen.getByText('Gesamtgate ausführen')).toBeTruthy();
    expect(ledgerLoader).toHaveBeenCalledWith({ taskId });
  });

  it('shows bounded budget, blockers, and separates model selection from execution', async () => {
    const stepId = '3'.repeat(64);
    const snapshotId = '4'.repeat(64);
    const activityLoader = vi.fn<(selectedTaskId: string) => Promise<AgentActivityResponseV1>>(
      async () => ({
        protocolVersion: 1,
        result: {
          activity: {
            blockers: [{ reason: 'Explizite Freigabe fehlt.', status: 'awaitingApproval', stepId }],
            currentLedgerRevision: 3,
            ledgerStoreVersion: '7',
            run: {
              attemptNumber: 1,
              budget: {
                actionLimit: 8,
                durationLimitMillis: '60000',
                outputTokenLimit: '2000',
                promptTokenLimit: '8000',
                repairLimit: 2,
                turnLimit: 8,
              },
              createdAtUnixMillis: '1786000000000',
              currentSnapshotId: snapshotId,
              earlierEventsOmitted: false,
              ledgerRevision: 3,
              ledgerRevisionMatchesCurrent: true,
              runId: '5'.repeat(64),
              state: 'awaitApproval',
              stepId,
              terminal: false,
              timeline: [
                {
                  code: 'none',
                  event: { kind: 'runStarted' },
                  occurredAtUnixMillis: '1786000000000',
                  outcome: null,
                  sequence: '1',
                  snapshotId,
                },
                {
                  code: 'none',
                  event: {
                    kind: 'modelInteraction',
                    turn: {
                      outputTokens: 40,
                      promptTokens: 120,
                      repairUsed: false,
                      selectedAction: 'run',
                    },
                  },
                  occurredAtUnixMillis: '1786000000010',
                  outcome: 'succeeded',
                  sequence: '2',
                  snapshotId,
                },
                {
                  code: 'policyDecision',
                  event: { kind: 'toolAction' },
                  occurredAtUnixMillis: '1786000000020',
                  outcome: 'denied',
                  sequence: '3',
                  snapshotId,
                },
              ],
              updatedAtUnixMillis: '1786000000020',
              usage: {
                actionCount: 1,
                elapsedAtLastEventMillis: '20',
                outputTokens: '40',
                promptTokens: '120',
                repairCount: 0,
                turnCount: 1,
              },
            },
          },
          status: 'available',
        },
      }),
    );

    render(AgentGoalWorkspace, {
      activeProject: true,
      activityLoader,
      goalLoader: async () => availableGoal(1),
      ledgerLoader: async () => ({
        protocolVersion: 1,
        result: {
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          status: 'available',
          steps: [{ intendedOutcome: 'Freigabe abwarten', status: 'awaitingApproval', stepId }],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
      tasksLoader: async () => tasks(1),
    });

    expect(await screen.findByText('Explizite Freigabe fehlt.')).toBeTruthy();
    expect(screen.getByText('Aktionsauswahl Prozess · noch keine Ausführung')).toBeTruthy();
    expect(screen.getByText('Ausführungsaktion · Tool tatsächlich aufgerufen')).toBeTruthy();
    expect(screen.getAllByText('1 / 8')).toHaveLength(2);
    expect(screen.getByText('Run aktiv oder fortsetzbar')).toBeTruthy();
    expect(activityLoader).toHaveBeenCalledWith(taskId);
  });

  it('keeps a terminal run state visible', async () => {
    const snapshotId = '4'.repeat(64);
    render(AgentGoalWorkspace, {
      activeProject: true,
      activityLoader: async () => ({
        protocolVersion: 1,
        result: {
          activity: {
            blockers: [],
            currentLedgerRevision: 3,
            ledgerStoreVersion: '7',
            run: {
              attemptNumber: 1,
              budget: {
                actionLimit: 8,
                durationLimitMillis: '60000',
                outputTokenLimit: '2000',
                promptTokenLimit: '8000',
                repairLimit: 2,
                turnLimit: 8,
              },
              createdAtUnixMillis: '1786000000000',
              currentSnapshotId: snapshotId,
              earlierEventsOmitted: false,
              ledgerRevision: 3,
              ledgerRevisionMatchesCurrent: true,
              runId: '5'.repeat(64),
              state: 'cancelled',
              stepId: '3'.repeat(64),
              terminal: true,
              timeline: [
                {
                  code: 'none',
                  event: { kind: 'runStarted' },
                  occurredAtUnixMillis: '1786000000000',
                  outcome: null,
                  sequence: '1',
                  snapshotId,
                },
                {
                  code: 'cancellation',
                  event: { from: 'intake', kind: 'stateTransition', to: 'cancelled' },
                  occurredAtUnixMillis: '1786000000010',
                  outcome: 'cancelled',
                  sequence: '2',
                  snapshotId,
                },
              ],
              updatedAtUnixMillis: '1786000000010',
              usage: {
                actionCount: 0,
                elapsedAtLastEventMillis: '10',
                outputTokens: '0',
                promptTokens: '0',
                repairCount: 0,
                turnCount: 0,
              },
            },
          },
          status: 'available',
        },
      }),
      goalLoader: async () => availableGoal(1),
      ledgerLoader: async () => ({
        protocolVersion: 1,
        result: {
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          status: 'available',
          steps: [
            {
              intendedOutcome: 'Run beenden',
              status: 'cancelled',
              stepId: '3'.repeat(64),
            },
          ],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
      tasksLoader: async () => tasks(1),
    });

    expect(await screen.findByText('Terminaler Zustand')).toBeTruthy();
    expect(screen.getAllByText('Abgebrochen')).not.toHaveLength(0);
  });
});

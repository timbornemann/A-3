import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import AgentGoalWorkspace from './AgentGoalWorkspace.svelte';
import type { AgentActivityResponseV1 } from './agent-activity';
import type { AgentTaskControlResponseV1, AgentTaskRecoveryResponseV1 } from './agent-control';
import type {
  AgentGoalContractV1,
  AgentGoalDraftInputV1,
  AgentGoalMutationResponseV1,
  AgentGoalResponseV1,
} from './agent-goal';
import type { AgentInspectionResponseV1 } from './agent-inspection';
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

function minimalActiveActivity(): AgentActivityResponseV1 {
  const snapshotId = '4'.repeat(64);
  return {
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
          state: 'execute',
          stepId: '3'.repeat(64),
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
          ],
          updatedAtUnixMillis: '1786000000000',
          usage: {
            actionCount: 0,
            elapsedAtLastEventMillis: '0',
            outputTokens: '0',
            promptTokens: '0',
            repairCount: 0,
            turnCount: 0,
          },
        },
      },
      status: 'available',
    },
  };
}

function availableRecovery(
  status: 'available' | 'paused' = 'available',
): AgentTaskRecoveryResponseV1 {
  return {
    protocolVersion: 1,
    result: {
      recovery: {
        canResume: false,
        interruptedToolAttempts: 1,
        ledgerRevision: 3,
        ledgerStoreVersion: '7',
        mutationReconciliationRequired: false,
        mutationReplanRequired: false,
        publishedSnapshotId: '6'.repeat(64),
        runSnapshotId: '4'.repeat(64),
        snapshotChanged: true,
        staleEvidenceCount: 2,
        state: 'execute',
      },
      status,
    },
  };
}

function cancelledActivity(): AgentActivityResponseV1 {
  const response = minimalActiveActivity();
  if (response.result.status !== 'available' || response.result.activity.run === null) {
    throw new Error('minimal activity fixture must contain a run');
  }
  const run = response.result.activity.run;
  run.state = 'cancelled';
  run.terminal = true;
  run.updatedAtUnixMillis = '1786000000010';
  run.timeline.push({
    code: 'cancellation',
    event: { from: 'execute', kind: 'stateTransition', to: 'cancelled' },
    occurredAtUnixMillis: '1786000000010',
    outcome: 'cancelled',
    sequence: '2',
    snapshotId: run.currentSnapshotId,
  });
  return response;
}

describe('AgentGoalWorkspace', () => {
  it('cancels the native goal dialog without saving and restores the opener focus', async () => {
    const goalCreator = vi.fn();
    render(AgentGoalWorkspace, {
      activeProject: true,
      goalCreator,
      tasksLoader: async () => ({
        protocolVersion: 1,
        result: { status: 'available', tasks: [], truncated: false },
      }),
    });
    const initialDialog = await screen.findByRole('dialog', { name: 'Aufgabe anlegen' });
    expect(initialDialog.tagName).toBe('DIALOG');
    await fireEvent(initialDialog, new Event('cancel', { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    const opener = screen.getByRole('button', { name: 'Neue Aufgabe' });
    opener.focus();
    await fireEvent.click(opener);
    expect(await screen.findByRole('dialog', { name: 'Aufgabe anlegen' })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByLabelText('Ziel'));
    await fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    await waitFor(() => expect(document.activeElement).toBe(opener));
    expect(goalCreator).not.toHaveBeenCalled();
  });

  it('does not cross the Core boundary without an active project', () => {
    const tasksLoader = vi.fn<() => Promise<TaskLensTasksResponseV1>>();

    render(AgentGoalWorkspace, { activeProject: false, tasksLoader });

    expect(screen.getByText(/Öffne ein lokales Projekt/)).toBeTruthy();
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

    expect(await screen.findByRole('heading', { name: 'Aufgabe anlegen' })).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Ziel'), {
      target: { value: 'Agent Workspace aufbauen' },
    });
    await fireEvent.input(screen.getByLabelText('Kriterium 1'), {
      target: { value: 'Das Ziel bleibt während der Bearbeitung sichtbar.' },
    });
    await fireEvent.input(screen.getByLabelText('Abschlussprüfung'), {
      target: { value: 'Die gespeicherte Revision erneut laden und vergleichen.' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Aufgabe anlegen' }));

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
    expect(await screen.findByText('Aufgabe dauerhaft angelegt.')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Agent Workspace aufbauen' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Details' }));
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
    await fireEvent.click(screen.getByRole('button', { name: 'Details' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Auftrag anpassen' }));
    await fireEvent.input(screen.getByLabelText('Änderungsgrund'), {
      target: { value: 'Ziel präzisiert' },
    });
    await fireEvent.input(screen.getByLabelText('Ziel'), {
      target: { value: 'Agent Workspace vollständig aufbauen' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Änderung speichern' }));

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
    expect(screen.getByText('Planrevision 3 · 0 von 2 Schritten erledigt')).toBeTruthy();
    expect(screen.getByText(/Arbeitsplan wurde anhand eines neuen Befunds angepasst/)).toBeTruthy();
    expect(screen.getAllByText('In Arbeit')).toHaveLength(2);
    expect(screen.getByText('Gesamtgate ausführen')).toBeTruthy();
    expect(ledgerLoader).toHaveBeenCalledWith({ taskId });
  });

  it('binds the inspection panel to the selected durable task', async () => {
    const inspectionLoader = vi.fn<(selectedTaskId: string) => Promise<AgentInspectionResponseV1>>(
      async () => ({ protocolVersion: 1, result: { status: 'ledgerUnavailable' } }),
    );

    render(AgentGoalWorkspace, {
      activeProject: true,
      goalLoader: async () => availableGoal(1),
      inspectionLoader,
      tasksLoader: async () => tasks(1),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Review' }));
    expect(await screen.findByRole('heading', { name: 'Änderungen & Prüfungen' })).toBeTruthy();
    await waitFor(() => expect(inspectionLoader).toHaveBeenCalledWith(taskId));
    expect(screen.getByText(/noch kein prüfbarer Arbeitsplan/u)).toBeTruthy();
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
    const onRunStatusChange = vi.fn();

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
      onRunStatusChange,
      tasksLoader: async () => tasks(1),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Aktivität' }));
    expect(await screen.findByText('Explizite Freigabe fehlt.')).toBeTruthy();
    expect(screen.getByText('Aktionsauswahl Prozess · noch keine Ausführung')).toBeTruthy();
    expect(screen.getByText('Ausführungsaktion · Tool tatsächlich aufgerufen')).toBeTruthy();
    const technical = screen.getByText('Technische Laufdetails').closest('details');
    expect(technical?.open).toBe(false);
    expect(screen.getByRole('heading', { name: 'Nutzung und Limits' }).closest('details')).toBe(
      technical,
    );
    await fireEvent.click(screen.getByText('Technische Laufdetails'));
    expect(screen.getByRole('heading', { name: 'Nutzung und Limits' })).toBeTruthy();
    expect(screen.getAllByText('1 / 8')).toHaveLength(2);
    expect(screen.getByText('Aktiv oder fortsetzbar')).toBeTruthy();
    expect(activityLoader).toHaveBeenCalledWith(taskId);
    expect(onRunStatusChange).toHaveBeenLastCalledWith({
      kind: 'available',
      state: 'awaitApproval',
    });
  });

  it('keeps unsafe Resume disabled while Cancel remains reachable and task-bound', async () => {
    const recoveryLoader = vi
      .fn<(selectedTaskId: string) => Promise<AgentTaskRecoveryResponseV1>>()
      .mockResolvedValueOnce(availableRecovery('paused'))
      .mockResolvedValue({
        protocolVersion: 1,
        result: { state: 'cancelled', status: 'runNotControllable' },
      });
    const activityLoader = vi
      .fn<(selectedTaskId: string) => Promise<AgentActivityResponseV1>>()
      .mockResolvedValueOnce(minimalActiveActivity())
      .mockResolvedValue(cancelledActivity());
    const runController = vi.fn<
      (
        selectedTaskId: string,
        expectedLedgerRevision: number,
        expectedLedgerStoreVersion: string,
        action: 'cancel' | 'pause' | 'replan' | 'resume',
      ) => Promise<AgentTaskControlResponseV1>
    >(async () => ({
      protocolVersion: 1,
      result: {
        interruptedToolAttempts: 1,
        ledgerStoreVersion: '8',
        outcome: 'cancelled',
        reopenedStepCount: 2,
        runtimeStart: null,
        state: 'cancelled',
        status: 'applied',
      },
    }));

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
          steps: [
            {
              intendedOutcome: 'Recovery sicher anwenden',
              status: 'inProgress',
              stepId: '3'.repeat(64),
            },
          ],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
      recoveryLoader,
      runController,
      tasksLoader: async () => tasks(1),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Aktivität' }));
    const resume = await screen.findByRole('button', { name: 'Fortsetzen' });
    const replan = screen.getByRole('button', { name: 'Neu planen' });
    const cancel = screen.getByRole('button', { name: 'Abbrechen' });
    expect(resume.hasAttribute('disabled')).toBe(true);
    expect(replan.hasAttribute('disabled')).toBe(false);
    expect(cancel.hasAttribute('disabled')).toBe(false);
    expect(screen.getByText(/Frühere Nachweise sind veraltet/)).toBeTruthy();

    await fireEvent.click(cancel);
    await waitFor(() => expect(runController).toHaveBeenCalledTimes(1));
    expect(runController).toHaveBeenCalledWith(taskId, 3, '7', 'cancel');
    expect(await screen.findByText('Der Agentenlauf wurde dauerhaft abgebrochen.')).toBeTruthy();
    expect(await screen.findByText('Lauf beendet')).toBeTruthy();
    expect(screen.getAllByText('Abgebrochen')).not.toHaveLength(0);
    expect(activityLoader).toHaveBeenCalledTimes(2);
  });

  it('requests Pause only for a Core-owned running worker and reloads Pausing', async () => {
    const runtimeResponse = (runtimeState: 'pausing' | 'running', canPause: boolean) => ({
      protocolVersion: 1 as const,
      result: {
        runtime: {
          canPause,
          controllerState: 'execute' as const,
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          runtimeState,
        },
        status: 'runtimeOwned' as const,
      },
    });
    const recoveryLoader = vi
      .fn<(selectedTaskId: string) => Promise<AgentTaskRecoveryResponseV1>>()
      .mockResolvedValueOnce(runtimeResponse('running', true))
      .mockResolvedValueOnce(runtimeResponse('pausing', false))
      .mockResolvedValue(availableRecovery('paused'));
    const runController = vi.fn<
      (
        selectedTaskId: string,
        expectedLedgerRevision: number,
        expectedLedgerStoreVersion: string,
        action: 'cancel' | 'pause' | 'replan' | 'resume',
      ) => Promise<AgentTaskControlResponseV1>
    >(async () => ({
      protocolVersion: 1,
      result: { outcome: 'pauseRequested', status: 'accepted' },
    }));

    render(AgentGoalWorkspace, {
      activeProject: true,
      activityLoader: async () => minimalActiveActivity(),
      goalLoader: async () => availableGoal(1),
      ledgerLoader: async () => ({
        protocolVersion: 1,
        result: {
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          status: 'available',
          steps: [
            {
              intendedOutcome: 'Besessenen Worker pausieren',
              status: 'inProgress',
              stepId: '3'.repeat(64),
            },
          ],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
      recoveryLoader,
      runController,
      tasksLoader: async () => tasks(1),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Aktivität' }));
    const pause = await screen.findByRole('button', { name: 'Pausieren' });
    expect(pause.hasAttribute('disabled')).toBe(false);
    await fireEvent.click(pause);
    await waitFor(() => expect(runController).toHaveBeenCalledTimes(1));
    expect(runController).toHaveBeenCalledWith(taskId, 3, '7', 'pause');
    expect(await screen.findByText(/Pause wurde angefordert/)).toBeTruthy();
    expect(await screen.findByText('Pause läuft')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Pausieren' }).hasAttribute('disabled')).toBe(true);
    expect(screen.queryByText(/Sicher pausiert/)).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
    expect(await screen.findByText(/Sicher pausiert/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Fortsetzen' })).toBeTruthy();
  });

  it.each([
    {
      action: 'resume' as const,
      button: 'Fortsetzen',
      message: /Der Agent setzt die Aufgabe fort/u,
      outcome: 'resumed' as const,
    },
    {
      action: 'replan' as const,
      button: 'Neu planen',
      message: /Die Neuplanung startet gleich/u,
      outcome: 'replanRequired' as const,
    },
  ])('starts $button only after the durable recovery commit', async (scenario) => {
    const recovery = availableRecovery('paused');
    if (recovery.result.status !== 'paused') throw new Error('paused recovery fixture required');
    recovery.result.recovery.canResume = true;
    recovery.result.recovery.staleEvidenceCount = 0;
    const runController = vi.fn<
      (
        selectedTaskId: string,
        expectedLedgerRevision: number,
        expectedLedgerStoreVersion: string,
        action: 'cancel' | 'pause' | 'replan' | 'resume',
      ) => Promise<AgentTaskControlResponseV1>
    >(async () => ({
      protocolVersion: 1,
      result: {
        interruptedToolAttempts: 1,
        ledgerStoreVersion: '8',
        outcome: scenario.outcome,
        reopenedStepCount: scenario.action === 'replan' ? 2 : 0,
        runtimeStart: 'queued',
        state: 'execute',
        status: 'applied',
      },
    }));

    render(AgentGoalWorkspace, {
      activeProject: true,
      activityLoader: async () => minimalActiveActivity(),
      goalLoader: async () => availableGoal(1),
      ledgerLoader: async () => ({
        protocolVersion: 1,
        result: {
          ledgerRevision: 3,
          ledgerStoreVersion: '7',
          status: 'available',
          steps: [
            {
              intendedOutcome: 'Recovery sicher fortsetzen',
              status: 'inProgress',
              stepId: '3'.repeat(64),
            },
          ],
          task: { goalRevision: 1, objective: goal(1).objective, taskId },
        },
      }),
      recoveryLoader: async () => recovery,
      runController,
      tasksLoader: async () => tasks(1),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Aktivität' }));
    await fireEvent.click(await screen.findByRole('button', { name: scenario.button }));
    await waitFor(() => expect(runController).toHaveBeenCalledTimes(1));
    expect(runController).toHaveBeenCalledWith(taskId, 3, '7', scenario.action);
    expect(await screen.findByText(scenario.message)).toBeTruthy();
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

    await fireEvent.click(await screen.findByRole('button', { name: 'Aktivität' }));
    expect(await screen.findByText('Lauf beendet')).toBeTruthy();
    expect(screen.getAllByText('Abgebrochen')).not.toHaveLength(0);
  });
});

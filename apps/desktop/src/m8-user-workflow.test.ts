import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { AgentActivityResponseV1 } from './lib/agent-activity';
import type { AgentApprovalResponseV1 } from './lib/agent-approval';
import type { AgentTaskRecoveryResponseV1 } from './lib/agent-control';
import type { AgentGoalResponseV1 } from './lib/agent-goal';
import type { AgentInspectionResponseV1 } from './lib/agent-inspection';
import type { HealthResponseV1 } from './lib/health';
import type { OpenProjectResponseV1, ProjectSummaryV1 } from './lib/project';
import type { ProjectStatusResponseV1 } from './lib/project-status';
import type { TaskLensTaskResponseV1, TaskLensTasksResponseV1 } from './lib/task-lens';

const id = (value: string): string => value.repeat(64);
const taskId = id('1');
const criterionId = id('2');
const stepId = id('3');
const verificationSpecId = id('4');
const snapshotId = id('5');
const runId = id('6');
const evidenceId = id('7');

const project: ProjectSummaryV1 = {
  head: { kind: 'born', objectId: id('a'), reference: 'refs/heads/main' },
  repositoryId: id('8'),
  worktreeId: id('9'),
  worktreeRootDisplay: String.raw`C:\m8-worktree`,
};

const health: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: 1,
  status: 'ready',
};

const opened: OpenProjectResponseV1 = {
  protocolVersion: 1,
  result: { project, status: 'opened' },
};

const noProject: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: { status: 'noProject' },
};

const activeProject: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    index: {
      latestAttemptSnapshotId: snapshotId,
      latestSnapshot: { generation: '1', snapshotId },
      publishedSnapshotId: snapshotId,
      state: 'published',
    },
    project,
    projectId: id('b'),
    rebuildState: 'idle',
    status: 'active',
    storageBytes: '4096',
  },
};

const tasks: TaskLensTasksResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    tasks: [{ goalRevision: 1, objective: 'M8 vollständig verifizieren', taskId }],
    truncated: false,
  },
};

const goal: AgentGoalResponseV1 = {
  protocolVersion: 1,
  result: {
    goal: {
      acceptanceCriteria: [
        {
          criterionId,
          requirement: 'must',
          statement: 'Der vollständige lokale Workflow endet evidenzverifiziert.',
        },
      ],
      constraints: ['Offline und innerhalb des aktiven Worktrees bleiben.'],
      createdAtUnixMillis: '1786000000000',
      nonGoals: ['Keine externe Veröffentlichung.'],
      objective: 'M8 vollständig verifizieren',
      previousRevision: null,
      revision: 1,
      revisionReason: null,
      successVerification: 'Aktuellen Must-Beweis und terminalen Run gemeinsam laden.',
      taskId,
      userDecisions: ['Nur Core-verifizierte Evidence ist autoritativ.'],
    },
    status: 'available',
  },
};

const ledger: TaskLensTaskResponseV1 = {
  protocolVersion: 1,
  result: {
    ledgerRevision: 2,
    ledgerStoreVersion: '7',
    status: 'available',
    steps: [{ intendedOutcome: 'Workflow verifizieren', status: 'completed', stepId }],
    task: { goalRevision: 1, objective: 'M8 vollständig verifizieren', taskId },
  },
};

const activity: AgentActivityResponseV1 = {
  protocolVersion: 1,
  result: {
    activity: {
      blockers: [],
      currentLedgerRevision: 2,
      ledgerStoreVersion: '7',
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
        createdAtUnixMillis: '1786000000000',
        currentSnapshotId: snapshotId,
        earlierEventsOmitted: false,
        ledgerRevision: 2,
        ledgerRevisionMatchesCurrent: true,
        runId,
        state: 'done',
        stepId,
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
            code: 'none',
            event: { kind: 'verificationRecorded' },
            occurredAtUnixMillis: '1786000001000',
            outcome: 'succeeded',
            sequence: '2',
            snapshotId,
          },
          {
            code: 'controllerDecision',
            event: { from: 'verify', kind: 'stateTransition', to: 'done' },
            occurredAtUnixMillis: '1786000002000',
            outcome: 'succeeded',
            sequence: '3',
            snapshotId,
          },
        ],
        updatedAtUnixMillis: '1786000002000',
        usage: {
          actionCount: 2,
          elapsedAtLastEventMillis: '2000',
          outputTokens: '100',
          promptTokens: '400',
          repairCount: 0,
          turnCount: 2,
        },
      },
    },
    status: 'available',
  },
};

const recovery: AgentTaskRecoveryResponseV1 = {
  protocolVersion: 1,
  result: { state: 'done', status: 'runNotControllable' },
};

const approval: AgentApprovalResponseV1 = {
  protocolVersion: 1,
  result: { status: 'unavailable' },
};

const inspection: AgentInspectionResponseV1 = {
  protocolVersion: 1,
  result: {
    inspection: {
      inspectionRevision: null,
      patch: null,
      processes: [],
      verification: {
        criteria: [
          {
            criterionId,
            proofState: 'proven',
            proofs: [{ evidenceIds: [evidenceId], stepId }],
            requirement: 'must',
            statement: 'Der vollständige lokale Workflow endet evidenzverifiziert.',
          },
        ],
        goalRevision: 1,
        ledgerRevision: 2,
        ledgerStoreVersion: '7',
        publishedSnapshotId: snapshotId,
        steps: [
          {
            attempts: [
              {
                evidence: [
                  {
                    detail: {
                      confirmedAtUnixMillis: '1786000002000',
                      kind: 'userConfirmation',
                      scopeId: id('c'),
                    },
                    evaluation: { status: 'passed' },
                    evidenceId,
                    freshness: { status: 'fresh' },
                    method: 'userConfirm',
                    runId,
                    snapshotId,
                  },
                ],
                number: 1,
                outcome: { status: 'passed' },
              },
            ],
            intendedOutcome: 'Workflow verifizieren',
            method: 'userConfirm',
            staleCause: null,
            status: 'completed',
            stepId,
            verificationSpecId,
          },
        ],
      },
    },
    status: 'available',
  },
};

describe('M8 desktop user workflow', () => {
  it('moves from explicit project open to a Core-proven terminal Done projection', async () => {
    const projectOpener = vi.fn(async () => opened);
    const projectStatusLoader = vi
      .fn<() => Promise<ProjectStatusResponseV1>>()
      .mockResolvedValueOnce(noProject)
      .mockResolvedValue(activeProject);
    const tasksLoader = vi.fn(async () => tasks);
    const goalLoader = vi.fn(async () => goal);
    const ledgerLoader = vi.fn(async () => ledger);
    const activityLoader = vi.fn(async () => activity);
    const inspectionLoader = vi.fn(async () => inspection);

    render(App, {
      props: {
        agentActivityLoader: activityLoader,
        agentApprovalLoader: async () => approval,
        agentGoalLoader: goalLoader,
        agentGoalTasksLoader: tasksLoader,
        agentInspectionLoader: inspectionLoader,
        agentRecoveryLoader: async () => recovery,
        healthLoader: async () => health,
        projectOpener,
        projectStatusLoader,
        taskLensTaskLoader: ledgerLoader,
      },
    });

    expect((await screen.findAllByText('Kein Projekt geöffnet')).length).toBeGreaterThan(0);
    expect(tasksLoader).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));
    expect(await screen.findByRole('heading', { name: 'Aktives Projekt' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('link', { name: 'Agent' }));
    expect(
      await screen.findByRole('heading', { name: 'M8 vollständig verifizieren' }),
    ).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Aktivität', pressed: false }));
    expect(await screen.findByText('Terminaler Zustand')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Review', pressed: false }));
    expect(
      await screen.findByText(/Done · alle Muss-Kriterien besitzen exakte frische Beweise/u),
    ).toBeTruthy();

    const globalStatus = screen.getByRole('region', { name: 'Globaler Arbeitsstatus' });
    await waitFor(() => expect(globalStatus.textContent).toContain('Done'));
    expect(screen.getAllByText(stepId).length).toBeGreaterThan(0);
    expect(screen.getAllByText(evidenceId).length).toBeGreaterThan(0);
    expect(projectOpener).toHaveBeenCalledTimes(1);
    expect(projectStatusLoader).toHaveBeenCalledTimes(2);
    expect(goalLoader).toHaveBeenCalledWith(taskId);
    expect(activityLoader).toHaveBeenCalledWith(taskId);
    expect(inspectionLoader).toHaveBeenCalledWith(taskId);
  });
});

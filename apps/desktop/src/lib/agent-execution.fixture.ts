import type { AgentSessionResponseV1 } from './agent-session';
import type { AgentActivityResponseV1 } from './agent-activity';
import type { TaskLensTaskResponseV1 } from './task-lens';
const sessionId = 'a'.repeat(64);

export const activeAgentSession = (): AgentSessionResponseV1 => ({
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

export const activeAgentActivity = (): AgentActivityResponseV1 => ({
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

export const adaptiveWorkPlan = (): TaskLensTaskResponseV1 => ({
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

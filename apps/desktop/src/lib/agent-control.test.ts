import { describe, expect, it, vi } from 'vitest';
import {
  controlAgentTaskRun,
  parseAgentTaskControlResponseV1,
  parseAgentTaskRecoveryResponseV1,
  queryAgentTaskRecovery,
} from './agent-control';

const id = (value: string) => value.repeat(64);

function recoveryResponse() {
  return {
    protocolVersion: 1,
    result: {
      recovery: {
        canResume: false,
        interruptedToolAttempts: 1,
        ledgerRevision: 2,
        ledgerStoreVersion: '7',
        mutationReconciliationRequired: false,
        mutationReplanRequired: false,
        publishedSnapshotId: id('2'),
        runSnapshotId: id('1'),
        snapshotChanged: true,
        staleEvidenceCount: 1,
        state: 'execute',
      },
      status: 'available',
    },
  };
}

describe('Agent task recovery V1', () => {
  it('validates exact recovery invariants and rejects leaked nested fields', () => {
    const response = parseAgentTaskRecoveryResponseV1(recoveryResponse());
    expect(response.result.status).toBe('available');

    const inconsistent = recoveryResponse();
    inconsistent.result.recovery.canResume = true;
    expect(() => parseAgentTaskRecoveryResponseV1(inconsistent)).toThrow(/projection/u);

    const leaked = recoveryResponse() as ReturnType<typeof recoveryResponse> & {
      result: { recovery: { runId?: string } };
    };
    leaked.result.recovery.runId = id('3');
    expect(() => parseAgentTaskRecoveryResponseV1(leaked)).toThrow(/projection/u);
  });

  it('sends only protocol version and the selected task for inspection', async () => {
    const invoke = vi.fn().mockResolvedValue(recoveryResponse());
    await queryAgentTaskRecovery(id('a'), invoke);
    expect(invoke).toHaveBeenCalledWith('query_agent_task_recovery', {
      request: { protocolVersion: 1, taskId: id('a') },
    });
  });

  it('binds a closed control action to exact ledger anchors without run identity', async () => {
    const invoke = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: {
        interruptedToolAttempts: 1,
        ledgerStoreVersion: '8',
        outcome: 'cancelled',
        reopenedStepCount: 2,
        state: 'cancelled',
        status: 'applied',
      },
    });
    const response = await controlAgentTaskRun(id('a'), 2, '7', 'cancel', invoke);
    expect(response.result.status).toBe('applied');
    expect(invoke).toHaveBeenCalledWith('control_agent_task_run', {
      request: {
        action: 'cancel',
        expectedLedgerRevision: 2,
        expectedLedgerStoreVersion: '7',
        protocolVersion: 1,
        taskId: id('a'),
      },
    });
  });

  it('rejects outcome/state contradictions and unknown response states', () => {
    expect(() =>
      parseAgentTaskControlResponseV1({
        protocolVersion: 1,
        result: {
          interruptedToolAttempts: 0,
          ledgerStoreVersion: '8',
          outcome: 'cancelled',
          reopenedStepCount: 0,
          state: 'execute',
          status: 'applied',
        },
      }),
    ).toThrow(/unsupported state/u);
    expect(() =>
      parseAgentTaskControlResponseV1({ protocolVersion: 1, result: { status: 'paused' } }),
    ).toThrow(/unsupported state/u);
  });
});

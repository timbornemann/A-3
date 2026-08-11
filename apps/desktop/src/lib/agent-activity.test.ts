import { describe, expect, it, vi } from 'vitest';
import { parseAgentActivityResponseV1, queryAgentActivity } from './agent-activity';

const id = (value: string) => value.repeat(64);

function availableResponse() {
  return {
    protocolVersion: 1,
    result: {
      activity: {
        blockers: [
          { reason: 'Freigabe erforderlich', status: 'awaitingApproval', stepId: id('3') },
        ],
        currentLedgerRevision: 2,
        ledgerStoreVersion: '7',
        run: {
          attemptNumber: 1,
          budget: {
            actionLimit: 10,
            durationLimitMillis: '10000',
            outputTokenLimit: '1000',
            promptTokenLimit: '5000',
            repairLimit: 2,
            turnLimit: 10,
          },
          createdAtUnixMillis: '100',
          currentSnapshotId: id('4'),
          earlierEventsOmitted: false,
          ledgerRevision: 2,
          ledgerRevisionMatchesCurrent: true,
          runId: id('2'),
          state: 'awaitApproval',
          stepId: id('3'),
          terminal: false,
          timeline: [
            {
              code: 'none',
              event: { kind: 'runStarted' },
              occurredAtUnixMillis: '100',
              outcome: null,
              sequence: '1',
              snapshotId: id('1'),
            },
            {
              code: 'none',
              event: {
                kind: 'modelInteraction',
                turn: {
                  outputTokens: 20,
                  promptTokens: 100,
                  repairUsed: false,
                  selectedAction: 'run',
                },
              },
              occurredAtUnixMillis: '110',
              outcome: 'succeeded',
              sequence: '2',
              snapshotId: id('1'),
            },
            {
              code: 'policyDecision',
              event: { kind: 'toolAction' },
              occurredAtUnixMillis: '120',
              outcome: 'denied',
              sequence: '3',
              snapshotId: id('4'),
            },
          ],
          updatedAtUnixMillis: '120',
          usage: {
            actionCount: 1,
            elapsedAtLastEventMillis: '20',
            outputTokens: '20',
            promptTokens: '100',
            repairCount: 0,
            turnCount: 1,
          },
        },
      },
      status: 'available',
    },
  };
}

describe('Agent activity V1', () => {
  it('keeps model selection distinct from a real tool action', () => {
    const response = parseAgentActivityResponseV1(availableResponse());
    expect(response.result.status).toBe('available');
    if (response.result.status !== 'available' || response.result.activity.run === null) return;
    expect(response.result.activity.run.timeline[1].event.kind).toBe('modelInteraction');
    expect(response.result.activity.run.timeline[2].event.kind).toBe('toolAction');
    expect(response.result.activity.blockers[0].reason).toBe('Freigabe erforderlich');
  });

  it('rejects noncontiguous journals and unknown nested fields', () => {
    const response = availableResponse();
    response.result.activity.run.timeline[1].sequence = '3';
    expect(() => parseAgentActivityResponseV1(response)).toThrow(/contiguous/u);

    const nested = availableResponse() as ReturnType<typeof availableResponse> & {
      result: { activity: { leakedPath?: string } };
    };
    nested.result.activity.leakedPath = 'C:\\secret';
    expect(() => parseAgentActivityResponseV1(nested)).toThrow(/ledger projection/u);
  });

  it('sends only protocol version and task identity to Core', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValue({ protocolVersion: 1, result: { status: 'taskNotFound' } });
    await queryAgentActivity(id('a'), invoke);
    expect(invoke).toHaveBeenCalledWith('query_agent_activity', {
      request: { protocolVersion: 1, taskId: id('a') },
    });
  });
});

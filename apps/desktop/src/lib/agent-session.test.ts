import { describe, expect, it, vi } from 'vitest';
import {
  parseAgentSessionResponseV1,
  parseAgentSessionsResponseV1,
  parseUiPreferencesV1,
  submitAgentMessage,
} from './agent-session';

const id = (value: string): string => value.repeat(64);

function summary() {
  return {
    currentPlanRevision: 1,
    mode: 'plan',
    revision: '2',
    sessionId: id('a'),
    state: 'awaitingPlanReview',
    title: 'Plan the workspace',
    updatedAtUnixMillis: '100',
  };
}

describe('Agent session V1', () => {
  it('accepts a bounded conversation and keeps its plan revision exact', () => {
    const parsed = parseAgentSessionResponseV1({
      protocolVersion: 1,
      result: {
        session: {
          activeTaskId: null,
          entries: [
            {
              createdAtUnixMillis: '99',
              kind: 'userMessage',
              planRevision: null,
              sequence: '1',
              text: 'Plan it',
            },
            {
              createdAtUnixMillis: '100',
              kind: 'plan',
              planRevision: 1,
              sequence: '2',
              text: 'Implementation plan',
            },
          ],
          hasOlderEntries: false,
          summary: summary(),
        },
        status: 'available',
      },
    });

    expect(parsed.result.status).toBe('available');
    if (parsed.result.status === 'available') {
      expect(parsed.result.session.entries[1].planRevision).toBe(1);
    }
  });

  it('rejects unknown authority fields and contradictory plan entries', () => {
    expect(() =>
      parseAgentSessionsResponseV1({
        protocolVersion: 1,
        result: { nextCursor: null, sessions: [], status: 'available', worktreePath: 'D:/escape' },
      }),
    ).toThrow(/does not match V1/u);
    expect(() =>
      parseAgentSessionResponseV1({
        protocolVersion: 1,
        result: {
          session: {
            activeTaskId: null,
            entries: [
              {
                createdAtUnixMillis: '100',
                kind: 'plan',
                planRevision: null,
                sequence: '1',
                text: 'Plan',
              },
            ],
            hasOlderEntries: false,
            summary: summary(),
          },
          status: 'available',
        },
      }),
    ).toThrow(/does not match V1/u);
  });

  it('submits only the narrow new-session contract', async () => {
    const invoke = vi.fn(async () => ({ protocolVersion: 1, result: { status: 'noProject' } }));
    await submitAgentMessage({ message: 'Explain the index', mode: 'ask' }, invoke);
    expect(invoke).toHaveBeenCalledWith('submit_agent_message_v2', {
      request: {
        contextReferences: [],
        expectedSessionRevision: null,
        message: 'Explain the index',
        protocolVersion: 1,
        researchDepth: 'standard',
        sessionId: null,
        startMode: 'ask',
      },
    });
  });

  it('validates content-free layout bounds', () => {
    expect(
      parseUiPreferencesV1({
        inspectorCollapsed: false,
        inspectorWidth: 400,
        protocolVersion: 1,
        revision: '0',
        sessionRailCollapsed: false,
        sessionRailWidth: 264,
      }).inspectorWidth,
    ).toBe(400);
  });
});

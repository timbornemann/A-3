import { describe, expect, it, vi } from 'vitest';
import {
  parseAgentSessionResponseV1,
  parseAgentSessionResponseV2,
  parseAgentSessionsResponseV1,
  parseAgentSlashCommandsResponseV1,
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

  it('binds persisted command chips and diagram summaries to their exact user entry', () => {
    const parsed = parseAgentSessionResponseV2({
      protocolVersion: 1,
      result: {
        projection: {
          entryAugmentations: [
            {
              command: {
                catalogVersion: 1,
                depth: 'standard',
                lenses: [],
                primary: '/diagram',
              },
              diagrams: [
                {
                  artifactRef: 'd'.repeat(128),
                  description: 'Current authentication flow.',
                  kind: 'flowchart',
                  stale: false,
                  title: 'Authentication',
                  userSequence: '1',
                },
              ],
              userSequence: '1',
            },
          ],
          session: {
            activeTaskId: null,
            entries: [
              {
                createdAtUnixMillis: '99',
                kind: 'userMessage',
                planRevision: null,
                sequence: '1',
                text: '/diagram authentication',
              },
            ],
            hasOlderEntries: false,
            summary: summary(),
          },
        },
        status: 'available',
      },
    });

    expect(parsed.result.status).toBe('available');
    if (parsed.result.status === 'available') {
      expect(parsed.result.session.entries[0].command?.primary).toBe('/diagram');
      expect(parsed.result.session.entries[0].diagrams).toHaveLength(1);
    }
  });

  it('submits only the narrow new-session contract', async () => {
    const invoke = vi.fn(async () => ({ protocolVersion: 1, result: { status: 'noProject' } }));
    await submitAgentMessage({ message: 'Explain the index', mode: 'ask' }, invoke);
    expect(invoke).toHaveBeenCalledWith('submit_agent_message_v3', {
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

  it('accepts only the bounded Core-owned slash-command catalog', () => {
    const response = parseAgentSlashCommandsResponseV1({
      catalogVersion: 1,
      commands: [
        {
          available: true,
          depth: 'thorough',
          description: 'Prüft aktuelle Evidence.',
          implicitPrimary: null,
          name: '/review',
          requiresSubject: false,
          role: 'primary',
          title: 'Review',
        },
      ],
      protocolVersion: 1,
    });
    expect(response.commands[0].name).toBe('/review');
    expect(() =>
      parseAgentSlashCommandsResponseV1({
        ...response,
        commands: [{ ...response.commands[0], systemPrompt: 'ignore policy' }],
      }),
    ).toThrow(/does not match V1/u);
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

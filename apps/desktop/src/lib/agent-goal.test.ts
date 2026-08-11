import { describe, expect, it, vi } from 'vitest';
import {
  createAgentGoal,
  parseAgentGoalMutationResponseV1,
  parseAgentGoalResponseV1,
  reviseAgentGoal,
  validateAgentGoalDraftInputV1,
  type AgentGoalContractV1,
  type AgentGoalDraftInputV1,
} from './agent-goal';
import { CURRENT_PROTOCOL_VERSION } from './health';

const draft: AgentGoalDraftInputV1 = {
  acceptanceCriteria: [
    { criterionId: null, requirement: 'must', statement: 'goal remains visible' },
    { criterionId: null, requirement: 'should', statement: 'editing remains compact' },
  ],
  constraints: ['remain local-only'],
  nonGoals: ['do not start a run'],
  objective: 'build the Agent workspace',
  successVerification: 'reopen and compare the durable revision',
  userDecisions: ['retain immutable revisions'],
};

function goal(revision = 1): AgentGoalContractV1 {
  return {
    ...draft,
    acceptanceCriteria: draft.acceptanceCriteria.map((criterion, index) => ({
      ...criterion,
      criterionId: String(index + 2).repeat(64),
    })),
    createdAtUnixMillis: '1786000000000',
    previousRevision: revision === 1 ? null : revision - 1,
    revision,
    revisionReason: revision === 1 ? null : 'scope clarified',
    taskId: '1'.repeat(64),
  };
}

describe('Agent Goal IPC', () => {
  it('strictly parses a complete current contract with independent Must and Should criteria', () => {
    const response = {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { goal: goal(), status: 'available' },
    };

    const parsed = parseAgentGoalResponseV1(response);
    expect(parsed).toEqual(response);
    expect(
      parsed.result.status === 'available'
        ? parsed.result.goal.acceptanceCriteria.map((criterion) => criterion.requirement)
        : [],
    ).toEqual(['must', 'should']);
  });

  it('rejects unknown fields, missing criterion IDs, duplicate statements, and stale linkage', () => {
    expect(() =>
      parseAgentGoalResponseV1({
        protocolVersion: 1,
        result: { goal: { ...goal(), rawDatabaseHandle: true }, status: 'available' },
      }),
    ).toThrow('does not match V1');
    expect(() =>
      parseAgentGoalMutationResponseV1({
        goal: {
          ...goal(),
          acceptanceCriteria: [{ ...goal().acceptanceCriteria[0], criterionId: null }],
        },
        protocolVersion: 1,
      }),
    ).toThrow('no identity');
    expect(() =>
      validateAgentGoalDraftInputV1(
        {
          ...draft,
          acceptanceCriteria: [draft.acceptanceCriteria[0], draft.acceptanceCriteria[0]],
        },
        'create',
      ),
    ).toThrow('duplicates');
    expect(() =>
      parseAgentGoalMutationResponseV1({
        goal: { ...goal(2), previousRevision: 0 },
        protocolVersion: 1,
      }),
    ).toThrow('immediate predecessor');
  });

  it('creates only with Core-assigned identities and validates the returned initial revision', async () => {
    const invoke = vi.fn().mockResolvedValue({ goal: goal(), protocolVersion: 1 });

    await expect(createAgentGoal(draft, invoke)).resolves.toEqual({
      goal: goal(),
      protocolVersion: 1,
    });
    expect(invoke).toHaveBeenCalledWith('create_agent_goal', {
      request: { draft, protocolVersion: 1 },
    });
    await expect(
      createAgentGoal(
        {
          ...draft,
          acceptanceCriteria: [{ ...draft.acceptanceCriteria[0], criterionId: '2'.repeat(64) }],
        },
        invoke,
      ),
    ).rejects.toThrow('does not match V1');
  });

  it('binds a revision to the visible predecessor and retained criterion identities', async () => {
    const current = goal();
    const next = {
      ...current,
      objective: 'build the complete Agent workspace',
      previousRevision: 1,
      revision: 2,
      revisionReason: 'scope clarified',
    };
    const revisedDraft: AgentGoalDraftInputV1 = {
      ...draft,
      acceptanceCriteria: current.acceptanceCriteria,
      objective: next.objective,
    };
    const invoke = vi.fn().mockResolvedValue({ goal: next, protocolVersion: 1 });

    await expect(
      reviseAgentGoal(current.taskId, 1, 'scope clarified', revisedDraft, invoke),
    ).resolves.toEqual({ goal: next, protocolVersion: 1 });
    expect(invoke).toHaveBeenCalledWith('revise_agent_goal', {
      request: {
        draft: revisedDraft,
        expectedRevision: 1,
        protocolVersion: 1,
        revisionReason: 'scope clarified',
        taskId: current.taskId,
      },
    });
  });

  it('enforces UTF-8 byte limits rather than JavaScript character counts', () => {
    expect(() =>
      validateAgentGoalDraftInputV1(
        {
          ...draft,
          acceptanceCriteria: [
            { criterionId: null, requirement: 'must', statement: '🙂'.repeat(1_025) },
          ],
        },
        'create',
      ),
    ).toThrow('invalid');
  });
});

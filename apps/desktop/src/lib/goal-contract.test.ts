import { describe, expect, it } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import { parseGoalContractV1, type GoalContractV1 } from './goal-contract';

const contract: GoalContractV1 = {
  acceptanceCriteria: [
    {
      criterionId: '2'.repeat(64),
      statement: 'the goal survives restart',
    },
  ],
  constraints: ['remain local-only'],
  createdAtUnixMillis: '1786000000000',
  nonGoals: ['do not start the controller'],
  objective: 'implement the durable goal',
  previousRevision: 1,
  protocolVersion: CURRENT_PROTOCOL_VERSION,
  revision: 2,
  revisionReason: 'the user clarified the outcome',
  successVerification: 'reopen and compare both revisions',
  taskId: '1'.repeat(64),
  userDecisions: ['retain old revisions'],
};

describe('Goal Contract IPC projection', () => {
  it('accepts the exact bounded V1 shape without losing 64-bit milliseconds', () => {
    expect(parseGoalContractV1(contract)).toEqual(contract);
    expect(
      parseGoalContractV1({
        ...contract,
        createdAtUnixMillis: '9007199254740992',
      }).createdAtUnixMillis,
    ).toBe('9007199254740992');
  });

  it('enforces the initial and successor revision invariants', () => {
    expect(
      parseGoalContractV1({
        ...contract,
        previousRevision: null,
        revision: 1,
        revisionReason: null,
      }),
    ).toMatchObject({ previousRevision: null, revision: 1, revisionReason: null });
    expect(() => parseGoalContractV1({ ...contract, previousRevision: 0 })).toThrowError(
      'immediate predecessor',
    );
    expect(() =>
      parseGoalContractV1({ ...contract, revision: 1, revisionReason: null }),
    ).toThrowError('revision metadata');
  });

  it('rejects unknown nested fields and duplicate criteria', () => {
    expect(() =>
      parseGoalContractV1({
        ...contract,
        acceptanceCriteria: [{ ...contract.acceptanceCriteria[0], executable: true }],
      }),
    ).toThrowError('invalid acceptance criterion');
    expect(() =>
      parseGoalContractV1({
        ...contract,
        acceptanceCriteria: [
          contract.acceptanceCriteria[0],
          { ...contract.acceptanceCriteria[0], criterionId: '3'.repeat(64) },
        ],
      }),
    ).toThrowError('duplicate acceptance criteria');
  });

  it('rejects unbounded, non-canonical, and structurally invalid content', () => {
    expect(() => parseGoalContractV1({ ...contract, objective: ` ${contract.objective}` })).toThrow(
      'invalid objective',
    );
    expect(() =>
      parseGoalContractV1({ ...contract, constraints: Array(65).fill('constraint') }),
    ).toThrowError('invalid constraints list');
    expect(() => parseGoalContractV1({ ...contract, rawDatabaseHandle: true })).toThrowError(
      'does not match the V1 schema',
    );
    expect(() => parseGoalContractV1({ ...contract, createdAtUnixMillis: 1 })).toThrowError(
      'invalid creation timestamp',
    );
  });
});

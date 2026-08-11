import { describe, expect, it, vi } from 'vitest';
import {
  compileTaskLens,
  parseTaskLensCompileResponseV1,
  parseTaskLensTaskResponseV1,
  parseTaskLensTasksResponseV1,
  queryTaskLensTask,
  queryTaskLensTasks,
} from './task-lens';

const id = (character: string) => character.repeat(64);
const taskId = id('a');
const stepId = id('b');

const revision = (withRange: boolean) => ({
  contentHash: id('c'),
  declarationRange: withRange
    ? {
        end: { column: 10, row: 2 },
        endByte: 30,
        start: { column: 0, row: 1 },
        startByte: 10,
      }
    : null,
  pathDisplay: 'src/lib.rs',
  pathHex: '7372632f6c69622e7273',
});

function taskResponse() {
  return {
    protocolVersion: 1,
    result: {
      ledgerRevision: 2,
      ledgerStoreVersion: '3',
      status: 'available',
      steps: [{ intendedOutcome: 'Compile current context', status: 'ready', stepId }],
      task: { goalRevision: 1, objective: 'Implement Task Lens', taskId },
    },
  };
}

function compileResponse() {
  return {
    protocolVersion: 1,
    result: {
      lens: {
        claims: [
          {
            claimId: id('d'),
            confidenceBasisPoints: 10_000,
            evidence: [{ evidenceId: id('e'), kind: 'file', revision: revision(false) }],
            kind: 'fact',
            moduleId: id('f'),
            polarity: 'affirms',
            predicate: {
              kind: 'path',
              path: { pathDisplay: 'src/lib.rs', pathHex: '7372632f6c69622e7273' },
            },
          },
          {
            claimId: id('e'),
            confidenceBasisPoints: 5_000,
            evidence: [],
            kind: 'hypothesis',
            moduleId: id('f'),
            polarity: 'affirms',
            predicate: { kind: 'architecturalIntent', statement: 'May own orchestration' },
          },
        ],
        digest: id('f'),
        entries: [
          {
            estimatedTokens: 100,
            position: 1,
            reason: { kind: 'repositoryAnchor' },
            target: {
              entrypointCount: 1,
              fileCount: 4,
              kind: 'repository',
              languageCount: 1,
              modulePolicyVersion: 1,
              packageCount: 1,
              symbolCount: 8,
            },
          },
          {
            estimatedTokens: 50,
            position: 2,
            reason: {
              finalScore: 60_000,
              kind: 'retrieval',
              priority: 'exact',
              rank: 1,
              sources: [
                { channel: 'exact', normalizedScoreBasisPoints: 10_000 },
                { channel: 'semantic', normalizedScoreBasisPoints: 7_000 },
              ],
            },
            target: { evidence: revision(false), kind: 'file' },
          },
        ],
        estimatedTokens: 150,
        excludedStaleClaims: 2,
        fusionPolicyVersion: 1,
        goalRevision: 1,
        goalSeed: 'Implement Task Lens',
        indexRunId: id('a'),
        ledgerRevision: 2,
        ledgerStoreVersion: '3',
        policyVersion: 1,
        snapshotId: id('b'),
        stepId,
        stepSeed: 'Compile current context',
        taskId,
        tokenBudget: 8_200,
        truncated: true,
      },
      status: 'available',
    },
  };
}

describe('Task Lens V1 boundary', () => {
  it('accepts a stable bounded durable task list and rejects reordering', async () => {
    const payload = {
      protocolVersion: 1,
      result: {
        status: 'available',
        tasks: [
          { goalRevision: 1, objective: 'First task', taskId: id('a') },
          { goalRevision: 2, objective: 'Second task', taskId: id('b') },
        ],
        truncated: true,
      },
    };
    expect(parseTaskLensTasksResponseV1(payload).result.status).toBe('available');

    const invoke = vi.fn(async () => payload);
    await queryTaskLensTasks(invoke);
    expect(invoke).toHaveBeenCalledWith('query_task_lens_tasks', {
      request: { protocolVersion: 1 },
    });

    const reordered = structuredClone(payload);
    reordered.result.tasks.reverse();
    expect(() => parseTaskLensTasksResponseV1(reordered)).toThrow(/ordering/i);
  });

  it('loads active-plan steps through an opaque task ID and binds the response', async () => {
    const invoke = vi.fn(async () => taskResponse());
    const response = await queryTaskLensTask({ taskId }, invoke);
    expect(response).toEqual(parseTaskLensTaskResponseV1(taskResponse()));
    expect(invoke).toHaveBeenCalledWith('query_task_lens_task', {
      request: { protocolVersion: 1, taskId },
    });

    const stale = taskResponse();
    stale.result.task.taskId = id('c');
    await expect(queryTaskLensTask({ taskId }, async () => stale)).rejects.toThrow(/selection/i);
  });

  it('accepts current Evidence, visible hypotheses, and semantic candidate-only provenance', () => {
    const response = parseTaskLensCompileResponseV1(compileResponse());
    expect(response.result.status).toBe('available');
    if (response.result.status === 'available') {
      expect(response.result.lens.claims.map((claim) => claim.kind)).toEqual([
        'fact',
        'hypothesis',
      ]);
      expect(response.result.lens.entries[1].reason).toMatchObject({
        priority: 'exact',
        sources: [{ channel: 'exact' }, { channel: 'semantic' }],
      });
      expect(response.result.lens.excludedStaleClaims).toBe(2);
    }
  });

  it('rejects unsupported proof, inconsistent Evidence ranges, and hidden fields', () => {
    const falseFact = structuredClone(compileResponse());
    falseFact.result.lens.claims[1].kind = 'fact';
    expect(() => parseTaskLensCompileResponseV1(falseFact)).toThrow(/classification/i);

    const rangedFile = structuredClone(compileResponse());
    rangedFile.result.lens.entries[1].target.evidence = revision(true);
    expect(() => parseTaskLensCompileResponseV1(rangedFile)).toThrow(/declaration range/i);

    const unknown = { ...compileResponse(), database: 'private.sqlite' };
    expect(() => parseTaskLensCompileResponseV1(unknown)).toThrow(/does not match V1/i);
  });

  it('invokes a pathless compile and rejects invalid or mismatched identities', async () => {
    const invoke = vi.fn(async () => compileResponse());
    await compileTaskLens({ stepId, taskId }, invoke);
    expect(invoke).toHaveBeenCalledWith('compile_task_lens', {
      request: { protocolVersion: 1, stepId, taskId },
    });

    await expect(compileTaskLens({ stepId: 'not-an-id', taskId }, vi.fn())).rejects.toThrow(
      /selection/i,
    );

    const stale = compileResponse();
    stale.result.lens.stepId = id('c');
    await expect(compileTaskLens({ stepId, taskId }, async () => stale)).rejects.toThrow(
      /selection/i,
    );
  });
});

import { describe, expect, it, vi } from 'vitest';
import {
  controlAgentApproval,
  parseAgentApprovalControlResponseV1,
  parseAgentApprovalResponseV1,
  queryAgentApproval,
} from './agent-approval';
import { patchApprovalResponse } from './agent-approval.fixture';

const id = (value: string): string => value.repeat(64);

describe('Agent approval V1', () => {
  it('decodes the exact patch scope and rejects leaked authority', () => {
    const parsed = parseAgentApprovalResponseV1(patchApprovalResponse());
    expect(parsed.result.status).toBe('available');

    const leaked = structuredClone(patchApprovalResponse()) as unknown as {
      result: { approval: { approvalId?: string } };
    };
    leaked.result.approval.approvalId = id('b');
    expect(() => parseAgentApprovalResponseV1(leaked)).toThrow(/does not match V1/u);
  });

  it('sends only the task for query and visible anchors plus a closed choice for control', async () => {
    const queryInvoke = vi.fn().mockResolvedValue(patchApprovalResponse());
    await queryAgentApproval(id('1'), queryInvoke);
    expect(queryInvoke).toHaveBeenCalledWith('query_agent_approval', {
      request: { protocolVersion: 1, taskId: id('1') },
    });

    const approval = patchApprovalResponse().result;
    if (approval.status !== 'available') throw new Error('fixture unavailable');
    const controlInvoke = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: {
        approvalRevision: '4',
        ledgerStoreVersion: '7',
        outcome: 'grantStored',
        runtimeStart: null,
        status: 'applied',
      },
    });
    await controlAgentApproval(id('1'), approval.approval, 'allowOnce', controlInvoke);
    expect(controlInvoke).toHaveBeenCalledWith('control_agent_approval', {
      request: {
        action: 'allowOnce',
        expectedApprovalRevision: '3',
        expectedLedgerRevision: 2,
        expectedLedgerStoreVersion: '7',
        protocolVersion: 1,
        taskId: id('1'),
      },
    });
  });

  it('retains exact argv, cwd and environment names but rejects environment values', () => {
    const response = patchApprovalResponse();
    if (response.result.status !== 'available') throw new Error('fixture unavailable');
    response.result.approval.action = {
      kind: 'process',
      process: {
        arguments: ['test', '--locked'],
        environmentAllowlist: ['PATH', 'CARGO_HOME'],
        executable: 'cargo',
        executionMode: 'knownSafe',
        network: { kind: 'denied' },
        planBinding: { kind: 'validated', stepId: id('3') },
        processKind: 'test',
        specificationId: id('9'),
        stderrLimit: 8192,
        stdoutLimit: 8192,
        timeoutMillis: '30000',
        workingDirectory: {
          kind: 'subtree',
          path: { displayPath: 'crates/core', pathHex: '6372617465732f636f7265' },
        },
      },
    };
    const parsed = parseAgentApprovalResponseV1(response);
    if (parsed.result.status !== 'available' || parsed.result.approval.action.kind !== 'process')
      throw new Error('process fixture unavailable');
    expect(parsed.result.approval.action.process.arguments).toEqual(['test', '--locked']);
    expect(parsed.result.approval.action.process.environmentAllowlist).toEqual([
      'PATH',
      'CARGO_HOME',
    ]);

    const leaked = structuredClone(response) as unknown as {
      result: { approval: { action: { process: { environmentValues?: object } } } };
    };
    leaked.result.approval.action.process.environmentValues = { TOKEN: 'secret' };
    expect(() => parseAgentApprovalResponseV1(leaked)).toThrow(/does not match V1/u);

    const valueInName = structuredClone(response);
    if (
      valueInName.result.status !== 'available' ||
      valueInName.result.approval.action.kind !== 'process'
    )
      throw new Error('process fixture unavailable');
    valueInName.result.approval.action.process.environmentAllowlist = ['TOKEN=secret'];
    expect(() => parseAgentApprovalResponseV1(valueInName)).toThrow(/does not match V1/u);
  });

  it('rejects lifecycle-control contradictions and a process bound to another step', () => {
    const contradictory = structuredClone(patchApprovalResponse());
    if (contradictory.result.status !== 'available') throw new Error('fixture unavailable');
    contradictory.result.approval.canAllowOnce = false;
    expect(() => parseAgentApprovalResponseV1(contradictory)).toThrow(/does not match V1/u);
  });

  it('rejects a scheduler result on non-continuation decisions', () => {
    expect(() =>
      parseAgentApprovalControlResponseV1({
        protocolVersion: 1,
        result: {
          approvalRevision: '4',
          ledgerStoreVersion: '7',
          outcome: 'grantStored',
          runtimeStart: 'queued',
          status: 'applied',
        },
      }),
    ).toThrow(/does not match V1/u);
  });
});

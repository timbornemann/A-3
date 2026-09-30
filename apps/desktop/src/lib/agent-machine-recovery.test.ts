import { describe, expect, it, vi } from 'vitest';
import {
  parseMachineRecovery,
  recoverMachineEffect,
  type MachineRecoveryScope,
} from './agent-machine-recovery';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
const task = 'a'.repeat(64);
const scope: MachineRecoveryScope = {
  ledgerRevision: 2,
  ledgerStoreVersion: '3',
  scope: 'b'.repeat(64),
  resourceKind: 'file',
  target: 'D:\\scratch\\file.txt',
};
const response = { protocolVersion: 1, result: { status: 'available', recovery: scope } };
describe('Machine recovery V1', () => {
  it('rejects malformed scopes, revisions and executable fields', () => {
    expect(parseMachineRecovery(response)).toEqual(response.result);
    for (const change of [
      { scope: 'B'.repeat(64) },
      { ledgerRevision: 0 },
      { ledgerStoreVersion: '03' },
      { ledgerStoreVersion: '9223372036854775808' },
      { target: 'x'.repeat(4097) },
      { resourceKind: 'shell' },
      { argv: ['erase'] },
    ]) {
      expect(() =>
        parseMachineRecovery({
          ...response,
          result: { status: 'available', recovery: { ...scope, ...change } },
        }),
      ).toThrow();
    }
    expect(() =>
      parseMachineRecovery({ protocolVersion: 2, result: { status: 'queued' } }),
    ).toThrow();
  });
  it('sends only current opaque anchors and human consent, never the displayed target', async () => {
    invoke.mockResolvedValue({ protocolVersion: 1, result: { status: 'queued' } });
    await recoverMachineEffect(task, scope);
    expect(invoke).toHaveBeenCalledExactlyOnceWith('recover_agent_machine_effect', {
      request: {
        protocolVersion: 1,
        taskId: task,
        expectedLedgerRevision: 2,
        expectedLedgerStoreVersion: '3',
        expectedScope: scope.scope,
        action: 'observeFileAndReplan',
      },
    });
  });
});

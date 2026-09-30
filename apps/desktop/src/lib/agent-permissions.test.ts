import { describe, expect, it, vi } from 'vitest';
import {
  parseAgentPermissions,
  queryAgentPermissions,
  updateAgentPermissions,
} from './agent-permissions';

const initial = { protocolVersion: 1 as const, revision: '1', mode: 'askPermissions' as const };
describe('app-wide permissions contract', () => {
  it('rejects malformed revisions and extra authority', () => {
    expect(parseAgentPermissions(initial)).toEqual(initial);
    for (const revision of ['0', '01', '-1', '9223372036854775808', '1.5']) {
      expect(() => parseAgentPermissions({ ...initial, revision })).toThrow();
    }
    expect(() => parseAgentPermissions({ ...initial, root: '/' })).toThrow();
    expect(() => parseAgentPermissions({ ...initial, mode: 'shell' })).toThrow();
  });
  it('uses only the explicit selection and visible revision', async () => {
    const invoke = vi.fn().mockResolvedValue(initial);
    await queryAgentPermissions(invoke);
    expect(invoke).toHaveBeenCalledWith('query_agent_permissions', {
      request: { protocolVersion: 1 },
    });
    await updateAgentPermissions(initial, 'fullMachine', invoke);
    expect(invoke).toHaveBeenLastCalledWith('update_agent_permissions', {
      request: { protocolVersion: 1, expectedRevision: '1', mode: 'fullMachine' },
    });
  });
});

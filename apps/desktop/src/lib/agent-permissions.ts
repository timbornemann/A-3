import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

export type AgentPermissionMode = 'askPermissions' | 'fullMachine';
export interface AgentPermissions {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  revision: string;
  mode: AgentPermissionMode;
}

export function parseAgentPermissions(value: unknown): AgentPermissions {
  if (typeof value !== 'object' || value === null || Array.isArray(value))
    throw new Error('Invalid agent permissions');
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).sort().join(',') !== 'mode,protocolVersion,revision' ||
    record.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    (record.mode !== 'askPermissions' && record.mode !== 'fullMachine') ||
    typeof record.revision !== 'string' ||
    !/^[1-9][0-9]{0,18}$/u.test(record.revision) ||
    BigInt(record.revision) > 9223372036854775807n
  )
    throw new Error('Invalid agent permissions');
  return record as unknown as AgentPermissions;
}

export async function queryAgentPermissions(
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentPermissions> {
  return parseAgentPermissions(
    await invokeCommand('query_agent_permissions', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
}

export async function updateAgentPermissions(
  settings: AgentPermissions,
  mode: AgentPermissionMode,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentPermissions> {
  parseAgentPermissions(settings);
  return parseAgentPermissions(
    await invokeCommand('update_agent_permissions', {
      request: {
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        expectedRevision: settings.revision,
        mode,
      },
    }),
  );
}

export async function subscribeAgentPermissions(
  receive: (settings: AgentPermissions) => void,
): Promise<() => void> {
  return listen('a3:agent-permissions-changed', (event) => {
    try {
      receive(parseAgentPermissions(event.payload));
    } catch {
      /* Invalid events grant no authority. */
    }
  });
}

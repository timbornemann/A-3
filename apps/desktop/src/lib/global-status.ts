import type { AgentControllerStateV1 } from './agent-activity';

export const WORKSPACE_AREAS = ['projects', 'map', 'agent', 'settings'] as const;

export type WorkspaceArea = (typeof WORKSPACE_AREAS)[number];
export type GlobalStatusTone = 'failed' | 'neutral' | 'pending' | 'ready' | 'warning';

export interface GlobalStatusItem {
  tone: GlobalStatusTone;
  value: string;
}

export type GlobalRunStatus =
  | { kind: 'noProject' }
  | { kind: 'loading' }
  | { kind: 'idle' }
  | { kind: 'unavailable' }
  | { kind: 'error' }
  | { kind: 'available'; state: AgentControllerStateV1 };

export function workspaceAreaFromHash(hash: string): WorkspaceArea {
  const candidate = hash.startsWith('#') ? hash.slice(1) : hash;
  return WORKSPACE_AREAS.find((area) => area === candidate) ?? 'projects';
}

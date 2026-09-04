import { describe, expect, it } from 'vitest';
import {
  agentGoalRecoveryMessage,
  agentSessionRecoveryMessage,
  deepMapRecoveryMessage,
  parseCommandErrorV1,
  projectActionRecoveryMessage,
  projectOpenRecoveryMessage,
} from './command-error';

function commandError(code: string, message = 'Safe Core message'): Record<string, unknown> {
  return { code, message, protocolVersion: 1 };
}

describe('command error recovery', () => {
  it.each([
    ['projectSelectionUnavailable', 'Laufwerk und Zugriffsrechte'],
    ['notGitRepository', 'Stammordner'],
    ['projectRootRequired', 'keinen Unterordner'],
    ['invalidRepositoryMetadata', 'repariere das Repository'],
    ['localStorageUpgradeRequired', 'neueren A^3-Version'],
    ['projectIdentityConflict', 'richtigen Worktree-Root'],
  ])('maps %s to concrete local recovery', (code, expected) => {
    expect(projectOpenRecoveryMessage(commandError(code))).toContain(expected);
  });

  it('never renders raw adapter details for malformed or unknown failures', () => {
    const secret = 'C:\\secret\\repository\\config';

    expect(projectOpenRecoveryMessage(new Error(secret))).not.toContain(secret);
    expect(projectOpenRecoveryMessage(commandError('unknownCode', secret))).not.toContain(secret);
    expect(
      projectActionRecoveryMessage(commandError('unknownCode', secret), 'remove'),
    ).not.toContain(secret);
  });

  it('strictly rejects extensions and control characters', () => {
    expect(
      parseCommandErrorV1({
        ...commandError('notGitRepository'),
        adapterPath: 'C:\\private',
      }),
    ).toBeNull();
    expect(parseCommandErrorV1(commandError('notGitRepository', 'unsafe\nmessage'))).toBeNull();
  });

  it('gives maintenance actions stable retry guidance', () => {
    expect(
      projectActionRecoveryMessage(commandError('indexRebuildAlreadyPending'), 'rebuild'),
    ).toContain('Aktualisiere den Status');
    expect(
      projectActionRecoveryMessage(commandError('projectRemovalUnavailable'), 'remove'),
    ).toContain('starte A^3');
  });

  it.each([
    'invalidRepositoryTreeQuery',
    'repositoryTreeDirectoryUnavailable',
    'invalidModuleTreeQuery',
    'moduleTreeParentUnavailable',
    'invalidModuleDependencyGraphQuery',
  ])('recognizes the narrow indexed-navigation error code %s', (code) => {
    expect(parseCommandErrorV1(commandError(code))?.code).toBe(code);
  });

  it('maps only known Deep Map codes and never renders a raw boundary message', () => {
    const secret = 'http://private-provider:11434/api/chat';
    expect(deepMapRecoveryMessage(commandError('deepMapNotPaused', secret))).toContain(
      'kein validierter pausierter',
    );
    expect(deepMapRecoveryMessage(commandError('unknownCode', secret))).not.toContain(secret);
  });

  it('maps revision conflicts without rendering Goal Contract or adapter content', () => {
    const secret = 'PRIVATE GOAL http://provider.internal';

    expect(agentGoalRecoveryMessage(commandError('agentGoalRevisionConflict', secret))).toContain(
      'aktuelle Revision',
    );
    expect(
      agentGoalRecoveryMessage(commandError('agentGoalRevisionConflict', secret)),
    ).not.toContain(secret);
    expect(agentGoalRecoveryMessage(new Error(secret))).not.toContain(secret);
  });

  it.each(['invalidAgentInspectionQuery', 'agentInspectionUnavailable'])(
    'recognizes the bounded inspection error code %s',
    (code) => {
      expect(parseCommandErrorV1(commandError(code))?.code).toBe(code);
    },
  );

  it.each([
    ['invalidAgentSessionRequest', 'aktuellen Zustand'],
    ['agentSessionRevisionConflict', 'Session wurde'],
    ['agentSessionUnavailable', 'lokale Agentenlaufzeit'],
    ['agentSessionBusy', 'aktuelle Arbeit'],
  ])('maps the Agent-session boundary code %s without exposing adapter text', (code, expected) => {
    const secret = 'C:\\private\\agent.db';
    const message = agentSessionRecoveryMessage(commandError(code, secret), 'control');
    expect(message).toContain(expected);
    expect(message).not.toContain(secret);
  });

  it('does not claim a busy plan start was queued when no queue acceptance was returned', () => {
    const message = agentSessionRecoveryMessage(commandError('agentSessionBusy'), 'implementPlan');
    expect(message).toContain('nicht gestartet');
    expect(message).not.toContain('wird nach');
  });
});

import { invoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type { ModelRoleV1 } from './settings';

export interface SettingsRecoveryV1 {
  protocolVersion: 1;
  settingsRevision: string;
  invalidProfiles: ModelRoleV1[];
}

const ROLES: ModelRoleV1[] = ['coding', 'mapping', 'embedding'];
const validRevision = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^(0|[1-9][0-9]{0,18})$/.test(value) &&
  // Canonical decimal strings compare exactly without lossy Number conversion
  // or introducing a BigInt syntax requirement in the recovery error screen.
  (value.length < 19 || value <= '9223372036854775807');

export function parseSettingsRecovery(payload: unknown): SettingsRecoveryV1 {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    throw new Error('Invalid settings recovery response.');
  }
  const value = payload as Record<string, unknown>;
  const roles = value.invalidProfiles;
  if (
    Object.keys(value).sort().join(',') !== 'invalidProfiles,protocolVersion,settingsRevision' ||
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    !validRevision(value.settingsRevision) ||
    !Array.isArray(roles) ||
    roles.length > 3 ||
    roles.some(
      (role, i) =>
        !ROLES.includes(role) || (i > 0 && ROLES.indexOf(roles[i - 1]) >= ROLES.indexOf(role)),
    )
  ) {
    throw new Error('Invalid settings recovery response.');
  }
  return {
    protocolVersion: 1,
    settingsRevision: value.settingsRevision,
    invalidProfiles: [...roles],
  };
}

export async function querySettingsRecovery(
  command: InvokeCommand = invoke,
): Promise<SettingsRecoveryV1> {
  return parseSettingsRecovery(
    await command('query_settings_recovery', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
}

export async function recoverInvalidModelProfiles(
  revision: string,
  command: InvokeCommand = invoke,
): Promise<SettingsRecoveryV1> {
  if (!validRevision(revision)) throw new Error('Invalid settings revision.');
  return parseSettingsRecovery(
    await command('recover_invalid_model_profiles', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION, expectedSettingsRevision: revision },
    }),
  );
}

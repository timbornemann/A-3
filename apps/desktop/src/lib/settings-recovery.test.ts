import { describe, expect, it, vi } from 'vitest';
import {
  parseSettingsRecovery,
  querySettingsRecovery,
  recoverInvalidModelProfiles,
} from './settings-recovery';

const diagnosis = { protocolVersion: 1, settingsRevision: '100', invalidProfiles: ['coding'] };

describe('settings recovery boundary', () => {
  it.each(['0', '9223372036854775807'])('preserves boundary revision %s losslessly', (revision) => {
    expect(
      parseSettingsRecovery({ ...diagnosis, settingsRevision: revision }).settingsRevision,
    ).toBe(revision);
  });
  it('sends no model, path, role selection or capability over IPC', async () => {
    const command = vi.fn(async () => diagnosis);
    await querySettingsRecovery(command);
    expect(command).toHaveBeenLastCalledWith('query_settings_recovery', {
      request: { protocolVersion: 1 },
    });
    await recoverInvalidModelProfiles('100', command);
    expect(command).toHaveBeenLastCalledWith('recover_invalid_model_profiles', {
      request: { protocolVersion: 1, expectedSettingsRevision: '100' },
    });
  });
  it.each(['', '-1', '01', '1e2', '9223372036854775808', '1'.repeat(100)])(
    'rejects invalid CAS %s before IPC',
    async (revision) => {
      const command = vi.fn();
      await expect(recoverInvalidModelProfiles(revision, command)).rejects.toThrow();
      expect(command).not.toHaveBeenCalled();
    },
  );
  it.each([
    { ...diagnosis, invalidProfiles: ['coding', 'coding'] },
    { ...diagnosis, invalidProfiles: ['mapping', 'coding'] },
    { ...diagnosis, invalidProfiles: ['unknown'] },
    { ...diagnosis, path: '/outside' },
    { ...diagnosis, protocolVersion: 2 },
    { ...diagnosis, settingsRevision: '9223372036854775808' },
  ])('rejects malformed or over-authoritative diagnostics', (value) => {
    expect(() => parseSettingsRecovery(value)).toThrow();
  });
});

import { describe, expect, it, vi } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import {
  confirmProjectCommandAllowlist,
  parseProjectSettingsResponseV1,
  queryProjectSettings,
  type ProjectSettingsResponseV1,
} from './project-settings';

const commandId = '22'.repeat(32);
const catalogId = '11'.repeat(32);

const availableResponse: ProjectSettingsResponseV1 = {
  protocolVersion: CURRENT_PROTOCOL_VERSION,
  result: {
    settings: {
      commands: {
        catalogId,
        commands: [
          {
            arguments: ['test', '--workspace'],
            commandId,
            evidenceCount: 2,
            executable: 'cargo',
            kind: 'test',
            selected: false,
            workingDirectoryHex: null,
          },
        ],
        confirmation: { status: 'notConfirmed' },
        status: 'available',
      },
      ignore: { configurationPresent: true, patterns: ['target/**', 'generated/**'] },
    },
    status: 'available',
  },
};

describe('project Settings IPC client', () => {
  it('queries only the Core-selected active project', async () => {
    const invokeCommand = vi.fn(async () => availableResponse);

    await expect(queryProjectSettings(invokeCommand)).resolves.toEqual(availableResponse);
    expect(invokeCommand).toHaveBeenCalledWith('query_project_settings', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('confirms only sorted IDs with exact catalog and CAS anchors', async () => {
    const secondCommandId = '33'.repeat(32);
    const invokeCommand = vi.fn(async () => availableResponse);

    await confirmProjectCommandAllowlist(
      catalogId,
      '7',
      [secondCommandId, commandId],
      invokeCommand,
    );

    expect(invokeCommand).toHaveBeenCalledWith('confirm_project_command_allowlist', {
      request: {
        commandIds: [commandId, secondCommandId],
        expectedAllowlistRevision: '7',
        expectedCatalogId: catalogId,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
      },
    });
  });

  it('rejects duplicate selections and unknown response authority', async () => {
    const invokeCommand = vi.fn(async () => availableResponse);
    await expect(
      confirmProjectCommandAllowlist(catalogId, null, [commandId, commandId], invokeCommand),
    ).rejects.toThrow('duplicated');
    expect(invokeCommand).not.toHaveBeenCalled();

    expect(() =>
      parseProjectSettingsResponseV1({
        ...availableResponse,
        projectPath: 'C:\\untrusted',
      }),
    ).toThrow('does not match');
    expect(() =>
      parseProjectSettingsResponseV1({
        ...availableResponse,
        result: {
          ...availableResponse.result,
          settings: {
            ...('settings' in availableResponse.result ? availableResponse.result.settings : {}),
            commands: {
              ...('settings' in availableResponse.result
                ? availableResponse.result.settings.commands
                : {}),
              shell: 'cargo test',
            },
          },
        },
      }),
    ).toThrow('invalid command settings');
  });

  it('rejects stale-selection contradictions and unbounded projections', () => {
    if (availableResponse.result.status !== 'available') throw new Error('invalid fixture');
    const activeSettings = availableResponse.result.settings;
    const commands = activeSettings.commands;
    if (commands.status !== 'available') throw new Error('invalid fixture');

    expect(() =>
      parseProjectSettingsResponseV1({
        ...availableResponse,
        result: {
          settings: {
            ...activeSettings,
            commands: {
              ...commands,
              commands: [{ ...commands.commands[0], selected: true }],
              confirmation: {
                confirmedAtUnixMillis: '1786612345678',
                revision: '1',
                status: 'stale',
              },
            },
          },
          status: 'available',
        },
      }),
    ).toThrow('inconsistent command selection');

    expect(() =>
      parseProjectSettingsResponseV1({
        ...availableResponse,
        result: {
          settings: {
            ...activeSettings,
            ignore: {
              configurationPresent: true,
              patterns: Array.from({ length: 257 }, () => 'generated/**'),
            },
          },
          status: 'available',
        },
      }),
    ).toThrow('invalid ignore settings');
  });
});

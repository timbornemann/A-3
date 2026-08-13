import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ProjectSettingsPanel from './ProjectSettingsPanel.svelte';
import type { ProjectCommandConfirmationV1, ProjectSettingsResponseV1 } from './project-settings';

const commandId = '22'.repeat(32);
const catalogId = '11'.repeat(32);

function response(
  confirmation: ProjectCommandConfirmationV1 = { status: 'notConfirmed' },
  selected = false,
): ProjectSettingsResponseV1 {
  return {
    protocolVersion: 1,
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
              selected,
              workingDirectoryHex: null,
            },
          ],
          confirmation,
          status: 'available',
        },
        ignore: { configurationPresent: true, patterns: ['target/**', 'generated/**'] },
      },
      status: 'available',
    },
  };
}

describe('ProjectSettingsPanel', () => {
  it('keeps project settings pathless when no Core-selected project exists', async () => {
    render(ProjectSettingsPanel, {
      projectSettingsLoader: vi.fn().mockResolvedValue({
        protocolVersion: 1,
        result: { status: 'noProject' },
      }),
    });

    expect(await screen.findByText(/Kein aktiver Worktree/)).toBeTruthy();
    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('presents repository ignore patterns read-only and exact direct argv', async () => {
    render(ProjectSettingsPanel, {
      projectSettingsLoader: vi.fn().mockResolvedValue(response()),
    });

    expect(await screen.findByText('target/**')).toBeTruthy();
    expect(screen.getByText('generated/**')).toBeTruthy();
    expect(screen.getByText('cargo "test" "--workspace"')).toBeTruthy();
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.getByText(/hier nur bestätigt, nicht ausgeführt/)).toBeTruthy();
  });

  it('makes a stale confirmation visibly ineffective and unchecked', async () => {
    render(ProjectSettingsPanel, {
      projectSettingsLoader: vi
        .fn()
        .mockResolvedValue(
          response({ confirmedAtUnixMillis: '1786612345678', revision: '3', status: 'stale' }),
        ),
    });

    expect(await screen.findByText(/vollständig deaktiviert/)).toBeTruthy();
    expect((screen.getByRole('checkbox') as HTMLInputElement).checked).toBe(false);
    expect(
      (
        screen.getByRole('button', {
          name: 'Ausgewählte direkte Befehle bestätigen',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it('confirms the visible selection with current catalog and stale CAS revision', async () => {
    const allowlistConfirmer = vi
      .fn()
      .mockResolvedValue(
        response(
          { confirmedAtUnixMillis: '1786612345680', revision: '4', status: 'current' },
          true,
        ),
      );
    render(ProjectSettingsPanel, {
      allowlistConfirmer,
      projectSettingsLoader: vi
        .fn()
        .mockResolvedValue(
          response({ confirmedAtUnixMillis: '1786612345678', revision: '3', status: 'stale' }),
        ),
    });

    const checkbox = (await screen.findByRole('checkbox')) as HTMLInputElement;
    await fireEvent.click(checkbox);
    await fireEvent.click(
      screen.getByRole('button', { name: 'Ausgewählte direkte Befehle bestätigen' }),
    );

    await waitFor(() => expect(allowlistConfirmer).toHaveBeenCalledTimes(1));
    expect(allowlistConfirmer).toHaveBeenCalledWith(catalogId, '3', [commandId]);
    expect(await screen.findByText(/Es wurde kein Befehl ausgeführt/)).toBeTruthy();
  });
});

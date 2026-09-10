import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import SettingsRecovery from './SettingsRecovery.svelte';
import type { SettingsRecoveryV1 } from './settings-recovery';

const invalid: SettingsRecoveryV1 = {
  protocolVersion: 1,
  settingsRevision: '100',
  invalidProfiles: ['coding'],
};

describe('SettingsRecovery', () => {
  it('requires diagnosis and explicit confirmation and allows cancellation', async () => {
    const diagnose = vi.fn(async () => invalid);
    const recover = vi.fn(async () => ({
      ...invalid,
      settingsRevision: '101',
      invalidProfiles: [],
    }));
    const onrecovered = vi.fn();
    render(SettingsRecovery, { diagnose, recover, onrecovered });
    expect(diagnose).not.toHaveBeenCalled();
    expect(recover).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' }));
    expect(await screen.findByText(/Betroffene Rollen: Agent \/ Coding/)).toBeTruthy();
    expect(screen.getByText(/API-Schlüssel und Projektwissen bleiben erhalten/)).toBeTruthy();
    expect(recover).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(recover).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' }));
    await fireEvent.click(
      await screen.findByRole('button', { name: 'Ungültige Zuordnungen jetzt deaktivieren' }),
    );
    await waitFor(() => expect(onrecovered).toHaveBeenCalledOnce());
    expect(recover).toHaveBeenCalledExactlyOnceWith('100');
  });
  it('offers reload without mutation when the configuration is healthy', async () => {
    const recover = vi.fn();
    const onrecovered = vi.fn();
    render(SettingsRecovery, {
      diagnose: async () => ({ ...invalid, invalidProfiles: [] }),
      recover,
      onrecovered,
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' }));
    await fireEvent.click(
      await screen.findByRole('button', { name: 'Einstellungen erneut laden' }),
    );
    expect(onrecovered).toHaveBeenCalledOnce();
    expect(recover).not.toHaveBeenCalled();
  });
  it('never offers a destructive reset when diagnosis fails', async () => {
    render(SettingsRecovery, {
      diagnose: async () => {
        throw new Error('sensitive storage detail');
      },
      onrecovered: vi.fn(),
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' }));
    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      expect.stringContaining('Es wurde nichts zurückgesetzt'),
    );
    expect(screen.queryByText(/sensitive storage detail/)).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Ungültige Zuordnungen jetzt deaktivieren' }),
    ).toBeNull();
    expect(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' })).toBeTruthy();
  });
  it('discards an outdated diagnosis and requires a new inspection', async () => {
    render(SettingsRecovery, {
      diagnose: async () => invalid,
      recover: async () => {
        throw { protocolVersion: 1, code: 'invalidSettingsRequest', message: 'conflict' };
      },
      onrecovered: vi.fn(),
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Modellkonfiguration prüfen' }));
    await fireEvent.click(
      await screen.findByRole('button', { name: 'Ungültige Zuordnungen jetzt deaktivieren' }),
    );
    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      expect.stringContaining('inzwischen geändert'),
    );
    expect(
      screen.queryByRole('button', { name: 'Ungültige Zuordnungen jetzt deaktivieren' }),
    ).toBeNull();
  });
});

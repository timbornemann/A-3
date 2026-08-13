import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import SettingsPanel from './SettingsPanel.svelte';
import type { SettingsResponseV1 } from './settings';

function response(overrides: Partial<SettingsResponseV1['settings']> = {}): SettingsResponseV1 {
  return {
    protocolVersion: 1,
    settings: {
      codingProfile: null,
      embeddingProfile: null,
      endpoint: null,
      mappingProfile: null,
      privacy: {
        automaticProviderDiscoveryEnabled: false,
        cloudSyncEnabled: false,
        promptResponseLoggingEnabled: false,
        remoteRequestsWithoutApprovalEnabled: false,
        telemetryEnabled: false,
      },
      probeActive: false,
      providerHealth: null,
      revision: '0',
      ...overrides,
    },
  };
}

const localEndpoint = {
  origin: 'http://127.0.0.1:11434',
  providerId: 'ollama',
  scope: 'localLoopback' as const,
};

describe('SettingsPanel', () => {
  it('keeps the index-capable model-free state visible and probes disabled', async () => {
    render(SettingsPanel, {
      healthLoader: vi.fn().mockResolvedValue({
        applicationVersion: '0.1.0',
        platform: 'windows',
        protocolVersion: 1,
        status: 'ready',
      }),
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });

    expect(await screen.findByText(/Modellfreier Betrieb ist aktiv/)).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Darstellung' })).toBeTruthy();
    expect(screen.getByRole('group', { name: 'Farbschema' })).toBeTruthy();
    const about = screen.getByText('Über A^3').closest('details');
    expect(about?.open).toBe(false);
    expect(within(about!).getByText('0.1.0')).toBeTruthy();
    expect(within(about!).getByText('V1')).toBeTruthy();
    expect(within(about!).getByText('windows')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    const probeButtons = screen.getAllByRole('button', { name: 'Explizit live prüfen' });
    expect(probeButtons).toHaveLength(3);
    expect(probeButtons.every((button) => (button as HTMLButtonElement).disabled)).toBe(true);
  });

  it('shows a prominent warning and never enables a remote probe', async () => {
    render(SettingsPanel, {
      settingsLoader: vi.fn().mockResolvedValue(
        response({
          endpoint: {
            origin: 'https://models.example.test',
            providerId: 'ollama',
            scope: 'remote',
          },
          providerHealth: { checkedAtUnixMillis: null, status: 'remoteBlocked' },
          revision: '1',
        }),
      ),
    });

    expect((await screen.findByRole('alert')).textContent).toContain('Remote-Verbindung blockiert');
    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    const probeButtons = screen.getAllByRole('button', { name: 'Explizit live prüfen' });
    expect(probeButtons.every((button) => (button as HTMLButtonElement).disabled)).toBe(true);
  });

  it('presents a capability-limited LLM profile as non-executable', async () => {
    render(SettingsPanel, {
      settingsLoader: vi.fn().mockResolvedValue(
        response({
          codingProfile: {
            activation: 'capabilityLimited',
            contextTokens: 16_384,
            modelId: 'plain-chat-model',
            outputTokens: 2_048,
            parallelism: 1,
            probedAtUnixMillis: '1786612345678',
            profileId: 'a'.repeat(64),
            structuredOutput: 'unavailable',
            toolCallMode: 'disabled',
          },
          endpoint: localEndpoint,
          providerHealth: {
            checkedAtUnixMillis: '1786612345678',
            status: 'capabilityLimited',
          },
          revision: '2',
        }),
      ),
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    expect(
      await screen.findByText(/Nicht ausführbar · erforderliches Structured Output fehlt/),
    ).toBeTruthy();
    expect(screen.queryByText(/Ausführbar · Structured Output live verifiziert/)).toBeNull();
  });

  it('starts a local Coding probe only after an explicit submit', async () => {
    const roleProber = vi
      .fn()
      .mockResolvedValue(response({ endpoint: localEndpoint, revision: '2' }));
    render(SettingsPanel, {
      roleProber,
      settingsLoader: vi
        .fn()
        .mockResolvedValue(response({ endpoint: localEndpoint, revision: '1' })),
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    const modelInputs = (await screen.findAllByLabelText('Modell-ID')) as HTMLInputElement[];
    await fireEvent.input(modelInputs[0] as HTMLInputElement, {
      target: { value: 'qwen2.5-coder:7b' },
    });
    await fireEvent.click(screen.getAllByRole('button', { name: 'Explizit live prüfen' })[0]!);

    await waitFor(() => expect(roleProber).toHaveBeenCalledTimes(1));
    expect(roleProber).toHaveBeenCalledWith('1', {
      contextTokens: 16_384,
      modelId: 'qwen2.5-coder:7b',
      outputTokens: 2_048,
      parallelism: 1,
      role: 'coding',
    });
  });
});

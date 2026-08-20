import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import SettingsPanel from './SettingsPanel.svelte';
import type { ProviderModelsResponseV1, SettingsResponseV1 } from './settings';

function response(overrides: Partial<SettingsResponseV1['settings']> = {}): SettingsResponseV1 {
  return {
    protocolVersion: 1,
    settings: {
      codingProfile: null,
      credential: null,
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
  access: 'local' as const,
  origin: 'http://127.0.0.1:11434',
  providerId: 'ollama',
  scope: 'localLoopback' as const,
};

function catalog(revision = '1'): ProviderModelsResponseV1 {
  return {
    modelIds: ['nomic-embed-text:latest', 'qwen2.5-coder:7b'],
    protocolVersion: 1,
    providerKind: 'ollama',
    settingsRevision: revision,
    truncated: false,
  };
}

describe('SettingsPanel', () => {
  it('shows a narrow settings navigation and keeps model-free operation clear', async () => {
    render(SettingsPanel, { settingsLoader: vi.fn().mockResolvedValue(response()) });

    expect(await screen.findByRole('heading', { name: 'Allgemein' })).toBeTruthy();
    expect(screen.getByRole('navigation', { name: 'Einstellungsbereiche' })).toBeTruthy();
    expect(screen.getByRole('group', { name: 'Farbschema' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Provider' }));
    expect(await screen.findByText('Kein Provider eingerichtet')).toBeTruthy();
    expect(screen.getByText(/lokaler Indexbrowser voll nutzbar/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    expect(await screen.findByText('Zuerst einen Provider verbinden')).toBeTruthy();
  });

  it('creates an Ollama provider through a focused modal and a typed CAS mutation', async () => {
    const providerConfigurer = vi
      .fn()
      .mockResolvedValue(response({ endpoint: localEndpoint, revision: '1' }));
    render(SettingsPanel, {
      providerConfigurer,
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Provider' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Provider hinzufügen' })[0]!);
    const dialog = screen.getByRole('dialog', { name: 'Provider hinzufügen' });
    expect((within(dialog).getByLabelText('Provider') as HTMLSelectElement).value).toBe('ollama');
    await fireEvent.input(within(dialog).getByLabelText('Endpoint'), {
      target: { value: 'http://127.0.0.1:11434' },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Provider hinzufügen' }));

    await waitFor(() => expect(providerConfigurer).toHaveBeenCalledTimes(1));
    expect(providerConfigurer).toHaveBeenCalledWith('0', 'ollama', 'http://127.0.0.1:11434');
    expect(await screen.findByText('Ollama')).toBeTruthy();
  });

  it('keeps a remote provider visibly blocked and disables model discovery', async () => {
    render(SettingsPanel, {
      settingsLoader: vi.fn().mockResolvedValue(
        response({
          endpoint: {
            access: 'remoteBlocked',
            origin: 'https://models.example.test',
            providerId: 'ollama',
            scope: 'remote',
          },
          providerHealth: { checkedAtUnixMillis: null, status: 'remoteBlocked' },
          revision: '1',
        }),
      ),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Provider' }));
    expect((await screen.findByRole('alert')).textContent).toContain('Remote-Verbindung blockiert');
    expect(
      (screen.getByRole('button', { name: 'Modelle erkennen' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('discovers local models only after a click and probes the selected Coding model', async () => {
    const modelDiscoverer = vi.fn().mockResolvedValue(catalog());
    const roleProber = vi.fn().mockResolvedValue(
      response({
        codingProfile: {
          activation: 'executable',
          contextTokens: 16_384,
          modelId: 'qwen2.5-coder:7b',
          outputTokens: 2_048,
          parallelism: 1,
          probedAtUnixMillis: '1786612345678',
          profileId: 'a'.repeat(64),
          structuredOutput: 'verified',
          toolCallMode: 'disabled',
        },
        endpoint: localEndpoint,
        providerHealth: { checkedAtUnixMillis: '1786612345678', status: 'healthy' },
        revision: '2',
      }),
    );
    render(SettingsPanel, {
      modelDiscoverer,
      roleProber,
      settingsLoader: vi
        .fn()
        .mockResolvedValue(response({ endpoint: localEndpoint, revision: '1' })),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Modelle' }));
    expect(modelDiscoverer).not.toHaveBeenCalled();
    await fireEvent.click(screen.getAllByRole('button', { name: 'Modelle erkennen' })[0]!);
    await waitFor(() => expect(modelDiscoverer).toHaveBeenCalledWith('1'));
    expect(await screen.findByText('2 Modelle gefunden')).toBeTruthy();

    const codingRow = screen.getByText('Coding Agent').closest('article');
    expect(codingRow).toBeTruthy();
    await fireEvent.click(within(codingRow!).getByRole('button', { name: 'Einrichten' }));
    const dialog = screen.getByRole('dialog', { name: 'Coding Agent einrichten' });
    await fireEvent.change(within(dialog).getByLabelText('Modell'), {
      target: { value: 'qwen2.5-coder:7b' },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }));

    await waitFor(() => expect(roleProber).toHaveBeenCalledTimes(1));
    expect(roleProber).toHaveBeenCalledWith('1', {
      contextTokens: 16_384,
      modelId: 'qwen2.5-coder:7b',
      outputTokens: 2_048,
      parallelism: 1,
      role: 'coding',
    });
    expect(await screen.findByText('Verifiziert')).toBeTruthy();
  });

  it('presents a capability-limited profile as non-executable in the compact role list', async () => {
    render(SettingsPanel, {
      modelDiscoverer: vi.fn().mockResolvedValue(catalog('2')),
      settingsLoader: vi.fn().mockResolvedValue(
        response({
          codingProfile: {
            activation: 'capabilityLimited',
            contextTokens: 16_384,
            modelId: 'plain-chat-model',
            outputTokens: 2_048,
            parallelism: 1,
            probedAtUnixMillis: '1786612345678',
            profileId: 'b'.repeat(64),
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

    await fireEvent.click(await screen.findByRole('button', { name: 'Modelle' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Modelle erkennen' })[0]!);
    expect(await screen.findByText('Capability fehlt')).toBeTruthy();
    expect(screen.queryByText('Verifiziert')).toBeNull();
  });

  it('loads desktop metadata only when the dedicated Info page is opened', async () => {
    const healthLoader = vi.fn().mockResolvedValue({
      applicationVersion: '0.1.0',
      platform: 'windows',
      protocolVersion: 1,
      status: 'ready',
    });
    render(SettingsPanel, {
      healthLoader,
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });
    await screen.findByRole('heading', { name: 'Allgemein' });
    expect(healthLoader).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Info' }));
    expect(await screen.findByText('0.1.0')).toBeTruthy();
    expect(healthLoader).toHaveBeenCalledTimes(1);
  });

  it('creates a Google Gemini provider and discovers Gemini models', async () => {
    const geminiEndpoint = {
      access: 'explicitUserInitiatedRemote' as const,
      origin: 'https://generativelanguage.googleapis.com',
      providerId: 'gemini',
      scope: 'remote' as const,
    };
    const providerConfigurer = vi.fn().mockResolvedValue(
      response({
        credential: { requirement: 'apiKey', status: 'missing' },
        endpoint: geminiEndpoint,
        revision: '1',
      }),
    );
    let capturedCredentialBytes: number[] = [];
    let credentialAttempt = 0;
    const credentialSetter = vi.fn(async (_revision: string, bytes: Uint8Array) => {
      capturedCredentialBytes = Array.from(bytes);
      credentialAttempt += 1;
      if (credentialAttempt === 1) throw new Error('credential store unavailable');
      return response({
        credential: { requirement: 'apiKey', status: 'configured' },
        endpoint: geminiEndpoint,
        revision: '3',
      });
    });
    const modelDiscoverer = vi.fn().mockResolvedValue({
      modelIds: ['gemini-2.5-flash', 'gemini-embedding-001'],
      protocolVersion: 1,
      providerKind: 'gemini',
      settingsRevision: '3',
      truncated: false,
    });

    const settingsLoader = vi
      .fn()
      .mockResolvedValueOnce(response())
      .mockResolvedValue(
        response({
          credential: { requirement: 'apiKey', status: 'missing' },
          endpoint: geminiEndpoint,
          revision: '1',
        }),
      );
    const rendered = render(SettingsPanel, {
      modelDiscoverer,
      providerConfigurer,
      credentialSetter,
      settingsLoader,
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Provider' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Provider hinzufügen' })[0]!);
    const dialog = screen.getByRole('dialog', { name: 'Provider hinzufügen' });

    await fireEvent.change(within(dialog).getByLabelText('Provider'), {
      target: { value: 'gemini' },
    });
    expect((within(dialog).getByLabelText('Endpoint') as HTMLInputElement).value).toBe(
      'https://generativelanguage.googleapis.com',
    );
    expect(within(dialog).getByText(/Betriebssystem-Schlüsselspeicher/)).toBeTruthy();

    await fireEvent.click(within(dialog).getByRole('button', { name: 'Provider hinzufügen' }));
    await waitFor(() => expect(providerConfigurer).toHaveBeenCalledTimes(1));
    expect(providerConfigurer).toHaveBeenCalledWith(
      '0',
      'gemini',
      'https://generativelanguage.googleapis.com',
    );
    expect(await screen.findByText('Google Gemini')).toBeTruthy();

    const discoveryButton = screen.getByRole('button', { name: 'Modelle erkennen' });
    expect((discoveryButton as HTMLButtonElement).disabled).toBe(true);
    const credentialInput = screen.getByLabelText('Neuer API-Key') as HTMLInputElement;
    await fireEvent.input(credentialInput, { target: { value: 'test-gemini-key' } });
    await fireEvent.submit(credentialInput.closest('form')!);
    expect(credentialInput.value).toBe('');
    await waitFor(() => expect(credentialSetter).toHaveBeenCalledTimes(1));
    expect((screen.getByLabelText('Neuer API-Key') as HTMLInputElement).value).toBe('');

    const retryCredentialInput = screen.getByLabelText('Neuer API-Key') as HTMLInputElement;
    await fireEvent.input(retryCredentialInput, { target: { value: 'test-gemini-key' } });
    await fireEvent.submit(retryCredentialInput.closest('form')!);
    expect(retryCredentialInput.value).toBe('');
    await waitFor(() => expect(credentialSetter).toHaveBeenCalledTimes(2));
    expect(credentialSetter.mock.calls[0]?.[0]).toBe('1');
    expect(capturedCredentialBytes).toEqual(
      Array.from(new TextEncoder().encode('test-gemini-key')),
    );
    expect(
      (screen.getByRole('button', { name: 'Modelle erkennen' }) as HTMLButtonElement).disabled,
    ).toBe(false);

    const configuredCredentialInput = screen.getByLabelText('Neuer API-Key') as HTMLInputElement;
    await fireEvent.input(configuredCredentialInput, { target: { value: 'discard-on-close' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Modelle' }));
    expect(configuredCredentialInput.value).toBe('');
    await fireEvent.click(screen.getByRole('button', { name: 'Provider' }));
    expect((screen.getByLabelText('Neuer API-Key') as HTMLInputElement).value).toBe('');

    await fireEvent.click(screen.getByRole('button', { name: 'Modelle erkennen' }));
    await waitFor(() => expect(modelDiscoverer).toHaveBeenCalledWith('3'));

    await fireEvent.click(screen.getByRole('button', { name: 'Provider' }));
    const unmountedCredentialInput = screen.getByLabelText('Neuer API-Key') as HTMLInputElement;
    await fireEvent.input(unmountedCredentialInput, { target: { value: 'discard-on-unmount' } });
    rendered.unmount();
    expect(unmountedCredentialInput.value).toBe('');
  });
});

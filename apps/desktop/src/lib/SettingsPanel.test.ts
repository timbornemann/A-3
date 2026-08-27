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
  it('uses top tabs and keeps the model setup flow clear without a provider', async () => {
    const { container } = render(SettingsPanel, {
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });

    expect(await screen.findByRole('heading', { name: 'Allgemein' })).toBeTruthy();
    expect(screen.getByRole('navigation', { name: 'Einstellungsbereiche' })).toBeTruthy();
    expect(container.querySelector('.settings-navigation')).toBeNull();
    expect(screen.getByRole('group', { name: 'Farbschema' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'KI & Modelle' }));
    expect(await screen.findByText('Bereit für deine Modellverbindung')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Ollama verbinden' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'OpenAI verwenden' })).toBeTruthy();
    expect(screen.getByText('Provider erforderlich')).toBeTruthy();
  });

  it('creates an Ollama provider through a focused modal and a typed CAS mutation', async () => {
    const providerConfigurer = vi
      .fn()
      .mockResolvedValue(response({ endpoint: localEndpoint, revision: '1' }));
    const modelDiscoverer = vi.fn().mockResolvedValue(catalog());
    render(SettingsPanel, {
      modelDiscoverer,
      providerConfigurer,
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Ollama verbinden' }));
    const dialog = screen.getByRole('dialog', { name: 'Ollama verbinden' });
    expect((within(dialog).getByLabelText('Provider') as HTMLSelectElement).value).toBe('ollama');
    await fireEvent.input(within(dialog).getByLabelText('Endpoint'), {
      target: { value: 'http://127.0.0.1:11434' },
    });
    await fireEvent.click(
      within(dialog).getByRole('button', { name: 'Verbinden und Modelle laden' }),
    );

    await waitFor(() => expect(providerConfigurer).toHaveBeenCalledTimes(1));
    expect(providerConfigurer).toHaveBeenCalledWith('0', 'ollama', 'http://127.0.0.1:11434');
    await waitFor(() => expect(modelDiscoverer).toHaveBeenCalledWith('1'));
    expect(await screen.findByText('2 Modelle gefunden')).toBeTruthy();
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

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    expect((await screen.findByRole('alert')).textContent).toContain('Remote-Verbindung blockiert');
    expect(
      (screen.getByRole('button', { name: 'Modelle aktualisieren' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('updates models on demand and probes the selected Coding model', async () => {
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

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    expect(modelDiscoverer).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Modelle aktualisieren' }));
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
    await waitFor(() => expect(screen.getAllByText('Verifiziert')).toHaveLength(2));
  });

  it('keeps persisted role assignments visible without reloading the model catalog', async () => {
    const modelDiscoverer = vi.fn().mockResolvedValue(catalog('2'));
    render(SettingsPanel, {
      modelDiscoverer,
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

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    expect(await screen.findByText('Capability fehlt')).toBeTruthy();
    expect(screen.getByText('plain-chat-model')).toBeTruthy();
    expect(modelDiscoverer).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Modelle aktualisieren' })).toBeTruthy();
    expect(screen.queryByText('Verifiziert')).toBeNull();

    const codingRow = screen.getByText('Coding Agent').closest('article');
    expect(codingRow).toBeTruthy();
    await fireEvent.click(within(codingRow!).getByLabelText('Status für Coding Agent erklären'));
    expect(
      within(codingRow!).getByText('Strukturiertes JSON konnte nicht verifiziert werden'),
    ).toBeTruthy();
    expect(within(codingRow!).getByText(/Chatten kann ein Modell trotzdem/)).toBeTruthy();
    expect(within(codingRow!).getByText(/Wähle ein anderes Modell/)).toBeTruthy();

    await fireEvent.click(screen.getByLabelText('Providerstatus erklären'));
    expect(screen.getByText('Mindestens eine Modellprüfung war eingeschränkt')).toBeTruthy();
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
    const roleProber = vi.fn().mockResolvedValue(
      response({
        codingProfile: {
          activation: 'executable',
          contextTokens: 16_384,
          modelId: 'gemini-2.5-flash',
          outputTokens: 2_048,
          parallelism: 1,
          probedAtUnixMillis: '1786612345678',
          profileId: 'c'.repeat(64),
          structuredOutput: 'verified',
          toolCallMode: 'disabled',
        },
        credential: { requirement: 'apiKey', status: 'configured' },
        endpoint: geminiEndpoint,
        providerHealth: { checkedAtUnixMillis: '1786612345678', status: 'healthy' },
        revision: '4',
      }),
    );

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
      roleProber,
      settingsLoader,
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Google Gemini verwenden' }));
    const dialog = screen.getByRole('dialog', { name: 'Google Gemini verbinden' });

    expect((within(dialog).getByLabelText('Endpoint') as HTMLInputElement).value).toBe(
      'https://generativelanguage.googleapis.com',
    );
    expect(within(dialog).getByText(/Betriebssystem-Schlüsselspeicher/)).toBeTruthy();
    const credentialInput = within(dialog).getByLabelText('API-Key') as HTMLInputElement;
    expect(credentialInput.placeholder).toBe('');
    await fireEvent.input(credentialInput, { target: { value: 'test-gemini-key' } });
    await fireEvent.click(
      within(dialog).getByRole('button', { name: 'Verbinden und Modelle laden' }),
    );
    expect(credentialInput.value).toBe('');
    await waitFor(() => expect(providerConfigurer).toHaveBeenCalledTimes(1));
    expect(providerConfigurer).toHaveBeenCalledWith(
      '0',
      'gemini',
      'https://generativelanguage.googleapis.com',
    );
    await waitFor(() => expect(credentialSetter).toHaveBeenCalledTimes(1));
    expect(credentialSetter.mock.calls[0]?.[0]).toBe('1');
    expect(await screen.findByRole('dialog', { name: 'Google Gemini bearbeiten' })).toBeTruthy();

    const retryDialog = screen.getByRole('dialog', { name: 'Google Gemini bearbeiten' });
    const retryCredentialInput = within(retryDialog).getByLabelText('API-Key') as HTMLInputElement;
    await fireEvent.input(retryCredentialInput, { target: { value: 'test-gemini-key' } });
    await fireEvent.click(
      within(retryDialog).getByRole('button', { name: 'Änderungen speichern' }),
    );
    expect(retryCredentialInput.value).toBe('');
    await waitFor(() => expect(credentialSetter).toHaveBeenCalledTimes(2));
    expect(capturedCredentialBytes).toEqual(
      Array.from(new TextEncoder().encode('test-gemini-key')),
    );
    expect(providerConfigurer).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('Google Gemini')).toBeTruthy();
    await waitFor(() => expect(modelDiscoverer).toHaveBeenCalledWith('3'));

    const codingRow = screen.getByText('Coding Agent').closest('article');
    expect(codingRow).toBeTruthy();
    await fireEvent.click(within(codingRow!).getByRole('button', { name: 'Einrichten' }));
    const roleDialog = screen.getByRole('dialog', { name: 'Coding Agent einrichten' });
    await fireEvent.click(within(roleDialog).getByRole('button', { name: 'Auswählen und prüfen' }));
    await waitFor(() =>
      expect(roleProber).toHaveBeenCalledWith('3', {
        contextTokens: 16_384,
        modelId: 'gemini-2.5-flash',
        outputTokens: 2_048,
        parallelism: 1,
        role: 'coding',
      }),
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Bearbeiten' }));
    const configuredDialog = screen.getByRole('dialog', { name: 'Google Gemini bearbeiten' });
    const configuredCredentialInput = within(configuredDialog).getByLabelText(
      'API-Key',
    ) as HTMLInputElement;
    expect(configuredCredentialInput.value).toBe('');
    expect(configuredCredentialInput.placeholder).toBe('********');
    await fireEvent.input(configuredCredentialInput, { target: { value: 'discard-on-close' } });
    await fireEvent.click(
      within(configuredDialog).getByRole('button', { name: 'Dialog schließen' }),
    );
    expect(configuredCredentialInput.value).toBe('');

    await fireEvent.click(screen.getByRole('button', { name: 'Bearbeiten' }));
    const unmountedDialog = screen.getByRole('dialog', { name: 'Google Gemini bearbeiten' });
    const unmountedCredentialInput = within(unmountedDialog).getByLabelText(
      'API-Key',
    ) as HTMLInputElement;
    await fireEvent.input(unmountedCredentialInput, { target: { value: 'discard-on-unmount' } });
    rendered.unmount();
    expect(unmountedCredentialInput.value).toBe('');
  });

  it('creates an OpenAI provider with a one-way key and explicit model discovery', async () => {
    const openAiEndpoint = {
      access: 'explicitUserInitiatedRemote' as const,
      origin: 'https://api.openai.com',
      providerId: 'openai',
      scope: 'remote' as const,
    };
    const providerConfigurer = vi.fn().mockResolvedValue(
      response({
        credential: { requirement: 'apiKey', status: 'missing' },
        endpoint: openAiEndpoint,
        revision: '1',
      }),
    );
    let capturedCredentialBytes: number[] = [];
    const credentialSetter = vi.fn(async (_revision: string, bytes: Uint8Array) => {
      capturedCredentialBytes = Array.from(bytes);
      return response({
        credential: { requirement: 'apiKey', status: 'configured' },
        endpoint: openAiEndpoint,
        revision: '2',
      });
    });
    const modelDiscoverer = vi.fn().mockResolvedValue({
      modelIds: ['gpt-5.4', 'text-embedding-3-small'],
      protocolVersion: 1,
      providerKind: 'openai',
      settingsRevision: '2',
      truncated: false,
    });
    render(SettingsPanel, {
      credentialSetter,
      modelDiscoverer,
      providerConfigurer,
      settingsLoader: vi.fn().mockResolvedValue(response()),
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
    await fireEvent.click(screen.getByRole('button', { name: 'OpenAI verwenden' }));
    const dialog = screen.getByRole('dialog', { name: 'OpenAI verbinden' });
    const endpoint = within(dialog).getByLabelText('Endpoint') as HTMLInputElement;
    expect(endpoint.value).toBe('https://api.openai.com');
    expect(endpoint.readOnly).toBe(true);
    const credentialInput = within(dialog).getByLabelText('API-Key') as HTMLInputElement;
    await fireEvent.input(credentialInput, { target: { value: 'test-openai-key' } });
    await fireEvent.click(
      within(dialog).getByRole('button', { name: 'Verbinden und Modelle laden' }),
    );

    await waitFor(() =>
      expect(providerConfigurer).toHaveBeenCalledWith('0', 'openai', 'https://api.openai.com'),
    );
    await waitFor(() => expect(credentialSetter).toHaveBeenCalledTimes(1));
    expect(capturedCredentialBytes).toEqual(
      Array.from(new TextEncoder().encode('test-openai-key')),
    );
    await waitFor(() => expect(modelDiscoverer).toHaveBeenCalledWith('2'));
    expect(await screen.findByText('OpenAI')).toBeTruthy();
    expect(screen.getByText('OpenAI Cloud')).toBeTruthy();
    expect(screen.getByText(/Kosten verursachen/)).toBeTruthy();
    expect(await screen.findByText('2 Modelle gefunden')).toBeTruthy();
  });
});

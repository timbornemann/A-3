import { describe, expect, it, vi } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import {
  cancelModelProbe,
  configureModelProvider,
  deleteModelProviderCredential,
  discoverProviderModels,
  parseProviderModelsResponseV1,
  parseSettingsResponseV1,
  probeModelRole,
  querySettings,
  setModelProviderCredential,
  type SettingsResponseV1,
} from './settings';

const emptyResponse: SettingsResponseV1 = {
  protocolVersion: CURRENT_PROTOCOL_VERSION,
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
  },
};

const verifiedCodingProfile = {
  activation: 'executable' as const,
  contextTokens: 16_384,
  modelId: 'qwen2.5-coder:7b',
  outputTokens: 2_048,
  parallelism: 1,
  probedAtUnixMillis: '1786612345678',
  profileId: 'a'.repeat(64),
  structuredOutput: 'verified' as const,
  toolCallMode: 'disabled' as const,
};

describe('settings IPC client', () => {
  it('queries without endpoint, project identity, or provider access authority', async () => {
    const invokeCommand = vi.fn(async () => emptyResponse);

    await expect(querySettings(invokeCommand)).resolves.toEqual(emptyResponse);
    expect(invokeCommand).toHaveBeenCalledWith('query_settings', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('sends only role input and user-selected bounds during an LLM probe', async () => {
    const response: SettingsResponseV1 = {
      ...emptyResponse,
      settings: { ...emptyResponse.settings, codingProfile: verifiedCodingProfile, revision: '1' },
    };
    const invokeCommand = vi.fn(async () => response);

    await expect(
      probeModelRole(
        '0',
        {
          contextTokens: 16_384,
          modelId: 'qwen2.5-coder:7b',
          outputTokens: 2_048,
          parallelism: 1,
          role: 'coding',
        },
        invokeCommand,
      ),
    ).resolves.toEqual(response);
    expect(invokeCommand).toHaveBeenCalledWith('probe_model_role', {
      request: {
        embeddingLimits: null,
        expectedSettingsRevision: '0',
        llmLimits: { contextTokens: 16_384, outputTokens: 2_048, parallelism: 1 },
        modelId: 'qwen2.5-coder:7b',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        role: 'coding',
      },
    });
  });

  it('keeps embedding dimension exclusively in the Core response', async () => {
    const response: SettingsResponseV1 = {
      ...emptyResponse,
      settings: {
        ...emptyResponse.settings,
        embeddingProfile: {
          dimension: 768,
          maxBatchSize: 8,
          modelId: 'nomic-embed-text',
          probedAtUnixMillis: '1786612345678',
          profileId: 'b'.repeat(64),
        },
        revision: '1',
      },
    };
    const invokeCommand = vi.fn(async () => response);

    await probeModelRole(
      '0',
      { maxBatchSize: 8, modelId: 'nomic-embed-text', role: 'embedding' },
      invokeCommand,
    );
    expect(invokeCommand).toHaveBeenCalledWith('probe_model_role', {
      request: {
        embeddingLimits: { maxBatchSize: 8 },
        expectedSettingsRevision: '0',
        llmLimits: null,
        modelId: 'nomic-embed-text',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        role: 'embedding',
      },
    });
  });

  it('rejects an executable profile without verified structured output', () => {
    expect(() =>
      parseSettingsResponseV1({
        ...emptyResponse,
        settings: {
          ...emptyResponse.settings,
          codingProfile: { ...verifiedCodingProfile, structuredOutput: 'unavailable' },
        },
      }),
    ).toThrowError('invalid LLM profile');
  });

  it('rejects relaxed privacy state and unknown response authority', () => {
    expect(() =>
      parseSettingsResponseV1({
        ...emptyResponse,
        settings: {
          ...emptyResponse.settings,
          privacy: { ...emptyResponse.settings.privacy, telemetryEnabled: true },
        },
      }),
    ).toThrowError('fail-closed privacy');
    expect(() =>
      parseSettingsResponseV1({ ...emptyResponse, endpointCredential: 'secret' }),
    ).toThrow('does not match');
  });

  it('uses CAS for provider changes and strictly parses cancellation acknowledgement', async () => {
    const configureInvoke = vi.fn(async () => ({
      ...emptyResponse,
      settings: {
        ...emptyResponse.settings,
        endpoint: {
          access: 'local',
          origin: 'http://127.0.0.1:11434',
          providerId: 'ollama',
          scope: 'localLoopback',
        },
        revision: '1',
      },
    }));
    await configureModelProvider('0', 'ollama', 'http://127.0.0.1:11434', configureInvoke);
    expect(configureInvoke).toHaveBeenCalledWith('configure_model_provider', {
      request: {
        endpointOrigin: 'http://127.0.0.1:11434',
        expectedSettingsRevision: '0',
        providerKind: 'ollama',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
      },
    });

    const cancelInvoke = vi.fn(async () => ({
      cancellationRequested: true,
      protocolVersion: CURRENT_PROTOCOL_VERSION,
    }));
    await expect(cancelModelProbe(cancelInvoke)).resolves.toEqual({
      cancellationRequested: true,
      protocolVersion: CURRENT_PROTOCOL_VERSION,
    });
  });

  it('sends credential bytes only through one-way CAS commands', async () => {
    const configuredResponse: SettingsResponseV1 = {
      ...emptyResponse,
      settings: {
        ...emptyResponse.settings,
        credential: { requirement: 'apiKey', status: 'configured' },
        endpoint: {
          access: 'explicitUserInitiatedRemote',
          origin: 'https://generativelanguage.googleapis.com',
          providerId: 'gemini',
          scope: 'remote',
        },
        revision: '3',
      },
    };
    let serializedCredentialSnapshot: unknown;
    const setInvoke = vi.fn(async (_command: string, arguments_: Record<string, unknown>) => {
      serializedCredentialSnapshot = JSON.parse(JSON.stringify(arguments_));
      return configuredResponse;
    });
    const key = new TextEncoder().encode('test-secret');

    await expect(setModelProviderCredential('1', key, setInvoke)).resolves.toEqual(
      configuredResponse,
    );
    expect(serializedCredentialSnapshot).toEqual({
      request: {
        apiKeyBytes: Array.from(key),
        expectedSettingsRevision: '1',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
      },
    });
    expect(setInvoke).toHaveBeenCalledWith('set_model_provider_credential', {
      request: {
        apiKeyBytes: Array(key.byteLength).fill(0),
        expectedSettingsRevision: '1',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
      },
    });
    expect(JSON.stringify(configuredResponse)).not.toContain('test-secret');

    const deletedResponse: SettingsResponseV1 = {
      ...configuredResponse,
      settings: {
        ...configuredResponse.settings,
        credential: { requirement: 'apiKey', status: 'missing' },
        revision: '5',
      },
    };
    const deleteInvoke = vi.fn(async () => deletedResponse);
    await expect(deleteModelProviderCredential('3', deleteInvoke)).resolves.toEqual(
      deletedResponse,
    );
    expect(deleteInvoke).toHaveBeenCalledWith('delete_model_provider_credential', {
      request: { expectedSettingsRevision: '3', protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('discovers only a canonical model-id list bound to the visible settings revision', async () => {
    const response = {
      modelIds: ['nomic-embed-text:latest', 'qwen2.5-coder:7b'],
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      providerKind: 'ollama' as const,
      settingsRevision: '4',
      truncated: false,
    };
    const invokeCommand = vi.fn(async () => response);

    await expect(discoverProviderModels('4', invokeCommand)).resolves.toEqual(response);
    expect(invokeCommand).toHaveBeenCalledWith('discover_provider_models', {
      request: {
        expectedSettingsRevision: '4',
        protocolVersion: CURRENT_PROTOCOL_VERSION,
      },
    });
    expect(() =>
      parseProviderModelsResponseV1({ ...response, endpointOrigin: 'http://127.0.0.1:11434' }),
    ).toThrow('does not match');
    expect(() =>
      parseProviderModelsResponseV1({
        ...response,
        modelIds: ['qwen2.5-coder:7b', 'nomic-embed-text:latest'],
      }),
    ).toThrow('non-canonical');

    const openAiResponse = {
      ...response,
      modelIds: ['gpt-5.4', 'text-embedding-3-small'],
      providerKind: 'openai' as const,
    };
    expect(parseProviderModelsResponseV1(openAiResponse)).toEqual(openAiResponse);
  });
});

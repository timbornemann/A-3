import { describe, expect, it, vi } from 'vitest';
import { CURRENT_PROTOCOL_VERSION } from './health';
import {
  cancelModelProbe,
  configureModelEndpoint,
  parseSettingsResponseV1,
  probeModelRole,
  querySettings,
  type SettingsResponseV1,
} from './settings';

const emptyResponse: SettingsResponseV1 = {
  protocolVersion: CURRENT_PROTOCOL_VERSION,
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

  it('uses CAS for endpoint changes and strictly parses cancellation acknowledgement', async () => {
    const configureInvoke = vi.fn(async () => ({
      ...emptyResponse,
      settings: {
        ...emptyResponse.settings,
        endpoint: {
          origin: 'http://127.0.0.1:11434',
          providerId: 'ollama',
          scope: 'localLoopback',
        },
        revision: '1',
      },
    }));
    await configureModelEndpoint('0', 'http://127.0.0.1:11434', configureInvoke);
    expect(configureInvoke).toHaveBeenCalledWith('configure_model_endpoint', {
      request: {
        endpointOrigin: 'http://127.0.0.1:11434',
        expectedSettingsRevision: '0',
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
});

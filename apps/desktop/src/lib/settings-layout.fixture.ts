// Synthetic settings for component contracts and offline browser layout QA.
import type { ModelProviderKindV1, ProviderSettingsV2, SettingsResponseV2 } from './settings';

export function settingsLayoutFixture(): SettingsResponseV2 {
  const slot = (providerKind: ModelProviderKindV1, origin: string): ProviderSettingsV2 => ({
    providerKind,
    defaultOrigin: origin,
    endpoint: {
      providerId: providerKind,
      origin,
      scope: providerKind === 'ollama' ? 'localLoopback' : 'remote',
      access: providerKind === 'ollama' ? 'local' : 'explicitUserInitiatedRemote',
    },
    enabled: true,
    configurationRevision: '1',
    credential: providerKind === 'ollama' ? null : { requirement: 'apiKey', status: 'configured' },
    connectionVerifiedAtUnixMillis: '1788732000000',
    health: { checkedAtUnixMillis: '1788732000000', status: 'healthy' },
  });
  return {
    protocolVersion: 1,
    settings: {
      revision: '1',
      providers: [
        slot('ollama', 'http://127.0.0.1:11434'),
        slot('gemini', 'https://generativelanguage.googleapis.com'),
        slot('openai', 'https://api.openai.com'),
      ],
      codingProfile: null,
      mappingProfile: null,
      embeddingProfile: null,
      privacy: {
        automaticProviderDiscoveryEnabled: false,
        cloudSyncEnabled: false,
        promptResponseLoggingEnabled: false,
        remoteRequestsWithoutApprovalEnabled: false,
        telemetryEnabled: false,
      },
      probeActive: false,
    },
  };
}

export const layoutModelIds = Array.from(
  { length: 240 },
  (_, index) =>
    `fixture-model-${String(index + 1).padStart(3, '0')}${index === 239 ? '-with-a-very-long-name-to-check-that-the-dialog-and-page-remain-bounded' : ''}`,
);

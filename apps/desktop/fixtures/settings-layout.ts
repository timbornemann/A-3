// Offline only. The production component receives synthetic ports; no Tauri or provider calls.
import { mount } from 'svelte';
import SettingsPanel from '../src/lib/SettingsPanel.svelte';
import { settingsLayoutFixture, layoutModelIds } from '../src/lib/settings-layout.fixture';
import type { SettingsResponseV2 } from '../src/lib/settings';
import '../src/styles.css';

const target = document.getElementById('app');
if (!target) throw new Error('Missing fixture mount');
let response = settingsLayoutFixture();
const update = (): SettingsResponseV2 => {
  response = {
    ...response,
    settings: { ...response.settings, revision: String(Number(response.settings.revision) + 1) },
  };
  return structuredClone(response);
};
mount(SettingsPanel, {
  target,
  props: {
    settingsLoaderV2: async () => structuredClone(response),
    modelDiscovererV2: async (_revision, providerKind) => ({
      ...update(),
      providerKind,
      modelIds: layoutModelIds,
      truncated: false,
    }),
    providerConfigurerV2: async (_revision, kind, origin) => {
      response.settings.providers = response.settings.providers.map((slot) =>
        slot.providerKind === kind
          ? {
              ...slot,
              endpoint: origin ? { ...slot.endpoint!, origin } : null,
              enabled: false,
              health: null,
              connectionVerifiedAtUnixMillis: null,
            }
          : slot,
      ) as typeof response.settings.providers;
      return update();
    },
    credentialSetterV2: async () => update(),
    credentialDeleterV2: async () => update(),
    providerEnablerV2: async (_revision, kind, enabled) => {
      response.settings.providers = response.settings.providers.map((slot) =>
        slot.providerKind === kind ? { ...slot, enabled } : slot,
      ) as typeof response.settings.providers;
      return update();
    },
    roleProberV2: async (_revision, providerKind, input) => {
      const common = {
        providerKind,
        modelId: input.modelId,
        probedAtUnixMillis: '1788732000000',
        profileId: 'a'.repeat(64),
      };
      if (input.role === 'embedding')
        response.settings.embeddingProfile = {
          ...common,
          maxBatchSize: input.maxBatchSize,
          dimension: 768,
        };
      else
        response.settings[input.role === 'coding' ? 'codingProfile' : 'mappingProfile'] = {
          ...common,
          contextTokens: input.contextTokens,
          outputTokens: input.outputTokens,
          parallelism: input.parallelism,
          activation: 'executable',
          structuredOutput: 'verified',
          toolCallMode: 'disabled',
        };
      return update();
    },
    operationCanceller: async () => ({ protocolVersion: 1, cancellationRequested: true }),
  },
});

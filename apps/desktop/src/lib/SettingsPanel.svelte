<script lang="ts">
  import { onMount } from 'svelte';
  import { parseCommandErrorV1 } from './command-error';
  import {
    cancelModelProbe,
    configureModelEndpoint,
    probeModelRole,
    querySettings,
    type CancelModelProbeResponseV1,
    type EmbeddingRoleProfileV1,
    type LlmRoleProfileV1,
    type ModelProbeInputV1,
    type ModelRoleV1,
    type SettingsResponseV1,
    type SettingsV1,
  } from './settings';

  interface Props {
    endpointConfigurer?: (
      expectedRevision: string,
      endpointOrigin: string | null,
    ) => Promise<SettingsResponseV1>;
    probeCanceller?: () => Promise<CancelModelProbeResponseV1>;
    roleProber?: (
      expectedRevision: string,
      input: ModelProbeInputV1,
    ) => Promise<SettingsResponseV1>;
    settingsLoader?: () => Promise<SettingsResponseV1>;
  }

  type View =
    | { kind: 'loading' }
    | { kind: 'ready'; settings: SettingsV1 }
    | { kind: 'error'; message: string };
  type Action =
    | { kind: 'idle' }
    | { kind: 'configuring' }
    | { kind: 'probing'; role: ModelRoleV1 }
    | { kind: 'cancelling'; role: ModelRoleV1 }
    | { kind: 'error'; message: string };

  let {
    endpointConfigurer = configureModelEndpoint,
    probeCanceller = cancelModelProbe,
    roleProber = probeModelRole,
    settingsLoader = querySettings,
  }: Props = $props();

  let view = $state<View>({ kind: 'loading' });
  let action = $state<Action>({ kind: 'idle' });
  let endpointOrigin = $state('http://127.0.0.1:11434');
  let codingModelId = $state('');
  let codingContextTokens = $state(16_384);
  let codingOutputTokens = $state(2_048);
  let codingParallelism = $state(1);
  let mappingModelId = $state('');
  let mappingContextTokens = $state(16_384);
  let mappingOutputTokens = $state(2_048);
  let mappingParallelism = $state(1);
  let embeddingModelId = $state('');
  let embeddingBatchSize = $state(8);

  onMount(() => {
    void loadSettings();
  });

  async function loadSettings(): Promise<void> {
    view = { kind: 'loading' };
    action = { kind: 'idle' };
    try {
      applyResponse(await settingsLoader());
    } catch (error) {
      view = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  function applyResponse(response: SettingsResponseV1): void {
    view = { kind: 'ready', settings: response.settings };
    if (response.settings.endpoint !== null) {
      endpointOrigin = response.settings.endpoint.origin;
    }
    if (response.settings.codingProfile !== null) {
      codingModelId = response.settings.codingProfile.modelId;
      codingContextTokens = response.settings.codingProfile.contextTokens;
      codingOutputTokens = response.settings.codingProfile.outputTokens;
      codingParallelism = response.settings.codingProfile.parallelism;
    }
    if (response.settings.mappingProfile !== null) {
      mappingModelId = response.settings.mappingProfile.modelId;
      mappingContextTokens = response.settings.mappingProfile.contextTokens;
      mappingOutputTokens = response.settings.mappingProfile.outputTokens;
      mappingParallelism = response.settings.mappingProfile.parallelism;
    }
    if (response.settings.embeddingProfile !== null) {
      embeddingModelId = response.settings.embeddingProfile.modelId;
      embeddingBatchSize = response.settings.embeddingProfile.maxBatchSize;
    }
    action = { kind: 'idle' };
  }

  async function saveEndpoint(clear: boolean): Promise<void> {
    if (view.kind !== 'ready') return;
    const origin = clear ? null : endpointOrigin.trim();
    action = { kind: 'configuring' };
    try {
      applyResponse(await endpointConfigurer(view.settings.revision, origin));
      if (clear) endpointOrigin = 'http://127.0.0.1:11434';
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  async function probe(input: ModelProbeInputV1): Promise<void> {
    if (view.kind !== 'ready' || !canProbe(view.settings)) return;
    action = { kind: 'probing', role: input.role };
    try {
      applyResponse(await roleProber(view.settings.revision, input));
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
      await refreshAfterAction();
    }
  }

  async function cancelProbe(role: ModelRoleV1): Promise<void> {
    action = { kind: 'cancelling', role };
    try {
      await probeCanceller();
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  async function refreshAfterAction(): Promise<void> {
    try {
      applyResponse(await settingsLoader());
    } catch {
      // Preserve the actionable mutation error instead of replacing it with a refresh failure.
    }
  }

  function canProbe(settings: SettingsV1): boolean {
    return settings.endpoint?.scope === 'localLoopback' && action.kind === 'idle';
  }

  function isRoleBusy(role: ModelRoleV1): boolean {
    return (action.kind === 'probing' || action.kind === 'cancelling') && action.role === role;
  }

  function llmState(profile: LlmRoleProfileV1 | null): string {
    if (profile === null) return 'Noch nicht live geprüft';
    return profile.activation === 'executable'
      ? 'Ausführbar · Structured Output live verifiziert'
      : 'Nicht ausführbar · erforderliches Structured Output fehlt';
  }

  function embeddingState(profile: EmbeddingRoleProfileV1 | null): string {
    return profile === null
      ? 'Noch nicht live geprüft'
      : `Live verifiziert · ${profile.dimension} Dimensionen`;
  }

  function healthLabel(settings: SettingsV1): string {
    const status = settings.providerHealth?.status ?? 'notChecked';
    const labels: Record<NonNullable<SettingsV1['providerHealth']>['status'], string> = {
      cancelled: 'Letzte Prüfung abgebrochen',
      capabilityLimited: 'Erreichbar, aber Capability begrenzt',
      healthy: 'Live-Prüfung erfolgreich',
      notChecked: 'Noch nicht geprüft',
      remoteBlocked: 'Remote-Endpunkt blockiert',
      unreachable: 'Bei letzter Prüfung nicht erreichbar',
    };
    return labels[status];
  }

  function recoveryMessage(error: unknown): string {
    const parsed = parseCommandErrorV1(error);
    if (parsed?.code === 'modelEndpointInvalid') {
      return 'Der Endpoint ist ungültig, nicht credential-frei oder für diese Prüfung nicht freigegeben.';
    }
    if (parsed?.code === 'invalidSettingsRequest') {
      return 'Die Settings haben sich geändert oder ein Wert liegt außerhalb der sicheren Grenzen. Lade neu und prüfe die Eingaben.';
    }
    if (parsed?.code === 'modelProbeAlreadyActive') {
      return 'Eine andere explizite Modellprüfung läuft bereits.';
    }
    return (
      parsed?.message ?? 'Die lokalen Modell-Settings konnten nicht sicher verarbeitet werden.'
    );
  }
</script>

<section class="settings-card" aria-labelledby="settings-heading">
  <div class="section-heading">
    <div>
      <p class="section-kicker">Settings</p>
      <h2 id="settings-heading">Modelle, Ressourcen und Datenschutz</h2>
    </div>
    <button type="button" onclick={loadSettings}>Neu laden</button>
  </div>

  {#if view.kind === 'loading'}
    <p class="settings-status" role="status" aria-live="polite">Settings werden lokal gelesen …</p>
  {:else if view.kind === 'error'}
    <div class="settings-error" role="status" aria-live="polite">
      <p>{view.message}</p>
      <button type="button" onclick={loadSettings}>Settings erneut laden</button>
    </div>
  {:else}
    <div class="settings-section" aria-labelledby="provider-settings-heading">
      <div class="settings-section-heading">
        <div>
          <h3 id="provider-settings-heading">Lokaler Provider</h3>
          <p>Prüfungen starten nur durch deinen Klick. Es gibt keine Hintergrund-Erkennung.</p>
        </div>
        <span class="settings-badge">{healthLabel(view.settings)}</span>
      </div>
      <form
        class="endpoint-form"
        onsubmit={(event) => {
          event.preventDefault();
          void saveEndpoint(false);
        }}
      >
        <label for="model-endpoint">Ollama Endpoint (credential-freier Origin)</label>
        <div class="endpoint-controls">
          <input
            id="model-endpoint"
            type="url"
            maxlength="2048"
            required
            spellcheck="false"
            autocomplete="off"
            bind:value={endpointOrigin}
            disabled={action.kind !== 'idle'}
          />
          <button type="submit" disabled={action.kind !== 'idle'}>Speichern</button>
          <button type="button" disabled={action.kind !== 'idle'} onclick={() => saveEndpoint(true)}
            >Modellfrei</button
          >
        </div>
      </form>
      {#if view.settings.endpoint === null}
        <p class="model-free-notice" role="status">
          Modellfreier Betrieb ist aktiv. Projektbrowser, Fast Index und gespeicherte Fakten bleiben
          nutzbar.
        </p>
      {:else if view.settings.endpoint.scope === 'remote'}
        <div class="remote-warning" role="alert">
          <strong>Remote-Verbindung blockiert</strong>
          <p>
            Dieser HTTPS-Endpoint verlässt den lokalen Rechner. A^3 führt ohne exakte Freigabe weder
            Prüfung noch Anfrage aus; es wurden keine Repository-Daten gesendet.
          </p>
        </div>
      {:else}
        <p class="local-endpoint-note">
          Lokal gebunden: <code>{view.settings.endpoint.origin}</code>
        </p>
      {/if}
    </div>

    <div class="model-profile-grid" aria-label="Modellprofile">
      <form
        class="model-profile-card"
        onsubmit={(event) => {
          event.preventDefault();
          void probe({
            contextTokens: codingContextTokens,
            modelId: codingModelId,
            outputTokens: codingOutputTokens,
            parallelism: codingParallelism,
            role: 'coding',
          });
        }}
      >
        <div>
          <p class="profile-role">Coding</p>
          <h3>Coding Agent</h3>
          <p
            class:capability-limited={view.settings.codingProfile?.activation ===
              'capabilityLimited'}
          >
            {llmState(view.settings.codingProfile)}
          </p>
        </div>
        <label>
          Modell-ID
          <input required maxlength="512" spellcheck="false" bind:value={codingModelId} />
        </label>
        <div class="resource-grid">
          <label>
            Kontext
            <input type="number" min="1024" max="1048576" bind:value={codingContextTokens} />
          </label>
          <label>
            Output
            <input type="number" min="1" max="262144" bind:value={codingOutputTokens} />
          </label>
          <label>
            Parallelität
            <input type="number" min="1" max="64" bind:value={codingParallelism} />
          </label>
        </div>
        {#if isRoleBusy('coding')}
          <button type="button" class="cancel-probe" onclick={() => cancelProbe('coding')}>
            {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Prüfung abbrechen'}
          </button>
        {:else}
          <button type="submit" disabled={!canProbe(view.settings)}>Explizit live prüfen</button>
        {/if}
      </form>

      <form
        class="model-profile-card"
        onsubmit={(event) => {
          event.preventDefault();
          void probe({
            contextTokens: mappingContextTokens,
            modelId: mappingModelId,
            outputTokens: mappingOutputTokens,
            parallelism: mappingParallelism,
            role: 'mapping',
          });
        }}
      >
        <div>
          <p class="profile-role">Mapping</p>
          <h3>Deep Map</h3>
          <p
            class:capability-limited={view.settings.mappingProfile?.activation ===
              'capabilityLimited'}
          >
            {llmState(view.settings.mappingProfile)}
          </p>
        </div>
        <label>
          Modell-ID
          <input required maxlength="512" spellcheck="false" bind:value={mappingModelId} />
        </label>
        <div class="resource-grid">
          <label>
            Kontext
            <input type="number" min="1024" max="1048576" bind:value={mappingContextTokens} />
          </label>
          <label>
            Output
            <input type="number" min="1" max="262144" bind:value={mappingOutputTokens} />
          </label>
          <label>
            Parallelität
            <input type="number" min="1" max="64" bind:value={mappingParallelism} />
          </label>
        </div>
        {#if isRoleBusy('mapping')}
          <button type="button" class="cancel-probe" onclick={() => cancelProbe('mapping')}>
            {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Prüfung abbrechen'}
          </button>
        {:else}
          <button type="submit" disabled={!canProbe(view.settings)}>Explizit live prüfen</button>
        {/if}
      </form>

      <form
        class="model-profile-card"
        onsubmit={(event) => {
          event.preventDefault();
          void probe({
            maxBatchSize: embeddingBatchSize,
            modelId: embeddingModelId,
            role: 'embedding',
          });
        }}
      >
        <div>
          <p class="profile-role">Embedding</p>
          <h3>Semantischer Abruf</h3>
          <p>{embeddingState(view.settings.embeddingProfile)}</p>
        </div>
        <label>
          Modell-ID
          <input required maxlength="512" spellcheck="false" bind:value={embeddingModelId} />
        </label>
        <label>
          Maximale Batch-Größe
          <input type="number" min="1" max="64" bind:value={embeddingBatchSize} />
        </label>
        <p class="derived-setting">
          Die Vektordimension wird aus einer echten Modellantwort abgeleitet und ist nicht
          editierbar.
        </p>
        {#if isRoleBusy('embedding')}
          <button type="button" class="cancel-probe" onclick={() => cancelProbe('embedding')}>
            {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Prüfung abbrechen'}
          </button>
        {:else}
          <button type="submit" disabled={!canProbe(view.settings)}>Explizit live prüfen</button>
        {/if}
      </form>
    </div>

    {#if action.kind === 'error'}
      <p class="settings-error-message" role="alert">{action.message}</p>
    {:else if action.kind === 'configuring'}
      <p class="settings-status" role="status" aria-live="polite">
        Endpoint wird lokal validiert …
      </p>
    {:else if action.kind === 'probing' || action.kind === 'cancelling'}
      <p class="settings-status" role="status" aria-live="polite">
        {action.kind === 'cancelling'
          ? 'Kooperativer Abbruch wurde angefordert …'
          : 'Explizite lokale Capability-Prüfung läuft …'}
      </p>
    {/if}

    <div class="privacy-settings" aria-labelledby="privacy-heading">
      <div>
        <p class="section-kicker">Daten &amp; Datenschutz</p>
        <h3 id="privacy-heading">Fail-closed in diesem Build</h3>
      </div>
      <ul>
        <li><span>Telemetry</span><strong>Aus</strong></li>
        <li><span>Cloud-Synchronisierung</span><strong>Aus</strong></li>
        <li><span>Automatische Provider-Erkennung</span><strong>Aus</strong></li>
        <li><span>Prompt-/Antwort-Logging</span><strong>Aus</strong></li>
        <li><span>Remote-Anfragen ohne exakte Freigabe</span><strong>Aus</strong></li>
      </ul>
      <p>
        Diese Werte sind Core-Aussagen, keine UI-Schalter. Eine gelockerte Projektion wird vom
        Frontend als Protokollverletzung verworfen.
      </p>
    </div>
  {/if}
</section>

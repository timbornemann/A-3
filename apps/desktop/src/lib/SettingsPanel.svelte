<script lang="ts">
  import { onMount } from 'svelte';
  import { parseCommandErrorV1 } from './command-error';
  import ProjectSettingsPanel from './ProjectSettingsPanel.svelte';
  import ThemeControls from './ThemeControls.svelte';
  import { queryHealth, type HealthResponseV1 } from './health';
  import {
    cancelModelProbe,
    configureModelProvider,
    discoverProviderModels,
    probeModelRole,
    querySettings,
    type CancelModelProbeResponseV1,
    type EmbeddingRoleProfileV1,
    type LlmRoleProfileV1,
    type ModelProbeInputV1,
    type ModelProviderKindV1,
    type ModelRoleV1,
    type ProviderModelsResponseV1,
    type SettingsResponseV1,
    type SettingsV1,
  } from './settings';

  interface Props {
    healthLoader?: () => Promise<HealthResponseV1>;
    modelDiscoverer?: (expectedRevision: string) => Promise<ProviderModelsResponseV1>;
    operationCanceller?: () => Promise<CancelModelProbeResponseV1>;
    providerConfigurer?: (
      expectedRevision: string,
      providerKind: ModelProviderKindV1,
      endpointOrigin: string | null,
    ) => Promise<SettingsResponseV1>;
    roleProber?: (
      expectedRevision: string,
      input: ModelProbeInputV1,
    ) => Promise<SettingsResponseV1>;
    settingsLoader?: () => Promise<SettingsResponseV1>;
  }

  type SettingsSection = 'general' | 'provider' | 'models' | 'project' | 'privacy' | 'about';
  type View =
    | { kind: 'loading' }
    | { kind: 'ready'; settings: SettingsV1 }
    | { kind: 'error'; message: string };
  type Action =
    | { kind: 'idle' }
    | { kind: 'configuring' }
    | { kind: 'discovering' }
    | { kind: 'probing'; role: ModelRoleV1 }
    | { kind: 'cancelling'; role: ModelRoleV1 | null }
    | { kind: 'error'; message: string };
  type HealthView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'ready'; health: HealthResponseV1 }
    | { kind: 'error' };
  type ProviderDialog = 'closed' | 'create' | 'edit' | 'remove';

  const settingsSections: { id: SettingsSection; label: string }[] = [
    { id: 'general', label: 'Allgemein' },
    { id: 'provider', label: 'Provider' },
    { id: 'models', label: 'Modelle' },
    { id: 'project', label: 'Projekt' },
    { id: 'privacy', label: 'Datenschutz' },
    { id: 'about', label: 'Info' },
  ];
  const modelRoles: ModelRoleV1[] = ['coding', 'mapping', 'embedding'];

  let {
    healthLoader = queryHealth,
    modelDiscoverer = discoverProviderModels,
    operationCanceller = cancelModelProbe,
    providerConfigurer = configureModelProvider,
    roleProber = probeModelRole,
    settingsLoader = querySettings,
  }: Props = $props();

  let view = $state<View>({ kind: 'loading' });
  let healthView = $state<HealthView>({ kind: 'idle' });
  let action = $state<Action>({ kind: 'idle' });
  let settingsView = $state<SettingsSection>('general');
  let providerDialog = $state<ProviderDialog>('closed');
  let roleDialog = $state<ModelRoleV1 | null>(null);
  let providerKind = $state<ModelProviderKindV1>('ollama');
  let endpointOrigin = $state('http://127.0.0.1:11434');
  let modelCatalog = $state<ProviderModelsResponseV1 | null>(null);
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
      applyResponse(await settingsLoader(), false);
    } catch (error) {
      view = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  async function loadAppInfo(): Promise<void> {
    if (healthView.kind === 'loading' || healthView.kind === 'ready') return;
    healthView = { kind: 'loading' };
    try {
      healthView = { kind: 'ready', health: await healthLoader() };
    } catch {
      healthView = { kind: 'error' };
    }
  }

  function selectSettingsView(section: SettingsSection): void {
    settingsView = section;
    if (section === 'about') void loadAppInfo();
  }

  function applyResponse(response: SettingsResponseV1, preserveCatalog: boolean): void {
    const previousEndpoint = view.kind === 'ready' ? endpointKey(view.settings) : null;
    const nextEndpoint = endpointKey(response.settings);
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
    if (preserveCatalog && modelCatalog !== null && previousEndpoint === nextEndpoint) {
      modelCatalog = { ...modelCatalog, settingsRevision: response.settings.revision };
    } else if (!preserveCatalog || previousEndpoint !== nextEndpoint) {
      modelCatalog = null;
    }
    action = { kind: 'idle' };
  }

  function endpointKey(settings: SettingsV1): string | null {
    return settings.endpoint === null
      ? null
      : `${settings.endpoint.providerId}\u0000${settings.endpoint.origin}`;
  }

  function openProviderDialog(mode: Exclude<ProviderDialog, 'closed'>): void {
    if (view.kind === 'ready' && view.settings.endpoint !== null) {
      endpointOrigin = view.settings.endpoint.origin;
    } else {
      endpointOrigin = 'http://127.0.0.1:11434';
    }
    providerKind = 'ollama';
    providerDialog = mode;
  }

  async function saveProvider(endpoint: string | null): Promise<void> {
    if (view.kind !== 'ready') return;
    action = { kind: 'configuring' };
    try {
      applyResponse(
        await providerConfigurer(view.settings.revision, providerKind, endpoint),
        false,
      );
      providerDialog = 'closed';
      if (endpoint === null) endpointOrigin = 'http://127.0.0.1:11434';
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  async function discoverModels(): Promise<void> {
    if (view.kind !== 'ready' || !canUseLocalProvider(view.settings)) return;
    action = { kind: 'discovering' };
    try {
      const catalog = await modelDiscoverer(view.settings.revision);
      if (
        catalog.settingsRevision !== view.settings.revision ||
        catalog.providerKind !== 'ollama'
      ) {
        throw new Error('Provider model catalog is stale.');
      }
      modelCatalog = catalog;
      action = { kind: 'idle' };
    } catch (error) {
      action = { kind: 'error', message: discoveryRecoveryMessage(error) };
    }
  }

  function openRoleDialog(role: ModelRoleV1): void {
    if (modelCatalog === null || modelCatalog.modelIds.length === 0) return;
    const currentModel = roleModelId(view.kind === 'ready' ? view.settings : null, role);
    setSelectedModel(role, currentModel ?? modelCatalog.modelIds[0]!);
    roleDialog = role;
  }

  async function probeSelectedRole(): Promise<void> {
    if (roleDialog === null || view.kind !== 'ready' || !canProbe(view.settings)) return;
    const role = roleDialog;
    const input: ModelProbeInputV1 =
      role === 'coding'
        ? {
            contextTokens: codingContextTokens,
            modelId: codingModelId,
            outputTokens: codingOutputTokens,
            parallelism: codingParallelism,
            role,
          }
        : role === 'mapping'
          ? {
              contextTokens: mappingContextTokens,
              modelId: mappingModelId,
              outputTokens: mappingOutputTokens,
              parallelism: mappingParallelism,
              role,
            }
          : { maxBatchSize: embeddingBatchSize, modelId: embeddingModelId, role };
    action = { kind: 'probing', role };
    try {
      applyResponse(await roleProber(view.settings.revision, input), true);
      roleDialog = null;
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
      await refreshAfterAction();
    }
  }

  async function cancelOperation(role: ModelRoleV1 | null): Promise<void> {
    action = { kind: 'cancelling', role };
    try {
      await operationCanceller();
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  async function refreshAfterAction(): Promise<void> {
    const preservedAction = action;
    const previousEndpoint = view.kind === 'ready' ? endpointKey(view.settings) : null;
    try {
      const catalog = modelCatalog;
      applyResponse(await settingsLoader(), false);
      if (
        catalog !== null &&
        view.kind === 'ready' &&
        previousEndpoint === endpointKey(view.settings)
      ) {
        modelCatalog = { ...catalog, settingsRevision: view.settings.revision };
      }
      action = preservedAction;
    } catch {
      // Preserve the actionable operation error instead of replacing it with a refresh failure.
    }
  }

  function setSelectedModel(role: ModelRoleV1, modelId: string): void {
    if (role === 'coding') codingModelId = modelId;
    if (role === 'mapping') mappingModelId = modelId;
    if (role === 'embedding') embeddingModelId = modelId;
  }

  function selectedModel(role: ModelRoleV1): string {
    if (role === 'coding') return codingModelId;
    if (role === 'mapping') return mappingModelId;
    return embeddingModelId;
  }

  function modelOptions(role: ModelRoleV1): string[] {
    const options = modelCatalog?.modelIds ?? [];
    const selected = selectedModel(role);
    return selected !== '' && !options.includes(selected) ? [selected, ...options] : options;
  }

  function canUseLocalProvider(settings: SettingsV1): boolean {
    return settings.endpoint?.scope === 'localLoopback' && !operationBusy();
  }

  function canProbe(settings: SettingsV1): boolean {
    return canUseLocalProvider(settings) && modelCatalog !== null;
  }

  function operationBusy(): boolean {
    return (
      action.kind === 'configuring' ||
      action.kind === 'discovering' ||
      action.kind === 'probing' ||
      action.kind === 'cancelling'
    );
  }

  function handleRoleModelChange(modelId: string): void {
    if (roleDialog !== null) setSelectedModel(roleDialog, modelId);
  }

  function roleModelId(settings: SettingsV1 | null, role: ModelRoleV1): string | null {
    if (settings === null) return null;
    if (role === 'coding') return settings.codingProfile?.modelId ?? null;
    if (role === 'mapping') return settings.mappingProfile?.modelId ?? null;
    return settings.embeddingProfile?.modelId ?? null;
  }

  function roleLabel(role: ModelRoleV1): string {
    if (role === 'coding') return 'Coding Agent';
    if (role === 'mapping') return 'Deep Map';
    return 'Embeddings';
  }

  function rolePurpose(role: ModelRoleV1): string {
    if (role === 'coding') return 'Agentenaktionen und Codeänderungen';
    if (role === 'mapping') return 'Projektanalyse und Module Cards';
    return 'Semantische Suche';
  }

  function roleStatus(settings: SettingsV1, role: ModelRoleV1): string {
    if (role === 'embedding') return embeddingState(settings.embeddingProfile);
    const profile = role === 'coding' ? settings.codingProfile : settings.mappingProfile;
    return llmState(profile);
  }

  function llmState(profile: LlmRoleProfileV1 | null): string {
    if (profile === null) return 'Nicht eingerichtet';
    return profile.activation === 'executable' ? 'Verifiziert' : 'Capability fehlt';
  }

  function embeddingState(profile: EmbeddingRoleProfileV1 | null): string {
    return profile === null ? 'Nicht eingerichtet' : `Verifiziert · ${profile.dimension}D`;
  }

  function healthLabel(settings: SettingsV1): string {
    const status = settings.providerHealth?.status ?? 'notChecked';
    const labels: Record<NonNullable<SettingsV1['providerHealth']>['status'], string> = {
      cancelled: 'Abgebrochen',
      capabilityLimited: 'Begrenzt',
      healthy: 'Verifiziert',
      notChecked: 'Nicht geprüft',
      remoteBlocked: 'Remote blockiert',
      unreachable: 'Nicht erreichbar',
    };
    return labels[status];
  }

  function presentModal(node: HTMLDialogElement): { destroy: () => void } {
    if (typeof node.showModal === 'function') node.showModal();
    else node.setAttribute('open', '');
    return {
      destroy: () => {
        if (typeof node.close === 'function' && node.open) node.close();
      },
    };
  }

  function discoveryRecoveryMessage(error: unknown): string {
    const parsed = parseCommandErrorV1(error);
    if (parsed?.code === 'modelEndpointInvalid') {
      return 'Die Modellerkennung ist nur für eine lokale, gültige Providerverbindung verfügbar.';
    }
    if (parsed?.code === 'modelProbeAlreadyActive') {
      return 'Eine andere Modelloperation läuft bereits.';
    }
    if (parsed?.code === 'invalidSettingsRequest') {
      return 'Die Providerverbindung hat sich geändert. Lade die Einstellungen neu.';
    }
    return 'Die lokalen Modelle konnten nicht abgefragt werden. Prüfe, ob Ollama läuft, und versuche es erneut.';
  }

  function recoveryMessage(error: unknown): string {
    const parsed = parseCommandErrorV1(error);
    if (parsed?.code === 'modelEndpointInvalid') {
      return 'Der Endpoint ist ungültig, enthält Credentials oder ist für diese lokale Aktion nicht freigegeben.';
    }
    if (parsed?.code === 'invalidSettingsRequest') {
      return 'Die Einstellungen haben sich geändert oder ein Wert liegt außerhalb der sicheren Grenzen. Lade neu und prüfe die Eingaben.';
    }
    if (parsed?.code === 'modelProbeAlreadyActive') {
      return 'Eine andere Modelloperation läuft bereits.';
    }
    return parsed?.message ?? 'Die Modell-Einstellungen konnten nicht sicher verarbeitet werden.';
  }
</script>

<section class="settings-shell" aria-label="Einstellungen">
  <aside class="settings-navigation">
    <p>Einstellungen</p>
    <nav aria-label="Einstellungsbereiche">
      {#each settingsSections as section (section.id)}
        <button
          type="button"
          aria-current={settingsView === section.id ? 'page' : undefined}
          onclick={() => selectSettingsView(section.id)}>{section.label}</button
        >
      {/each}
    </nav>
  </aside>

  <div class="settings-content">
    {#if settingsView === 'about'}
      <section class="settings-page" aria-labelledby="about-heading">
        <header class="settings-page-heading">
          <h3 id="about-heading">Info</h3>
          <p>A^3 · Autonomous Agent Assistant</p>
        </header>
        {#if healthView.kind === 'idle' || healthView.kind === 'loading'}
          <p class="settings-status" role="status" aria-live="polite">
            App-Informationen werden geladen …
          </p>
        {:else if healthView.kind === 'ready'}
          <dl class="about-list">
            <div>
              <dt>Version</dt>
              <dd>{healthView.health.applicationVersion}</dd>
            </div>
            <div>
              <dt>Protokoll</dt>
              <dd>V{healthView.health.protocolVersion}</dd>
            </div>
            <div>
              <dt>Plattform</dt>
              <dd>{healthView.health.platform}</dd>
            </div>
          </dl>
        {:else}
          <div class="settings-empty-state">
            <strong>Informationen nicht verfügbar</strong>
            <button type="button" onclick={loadAppInfo}>Erneut laden</button>
          </div>
        {/if}
      </section>
    {:else if view.kind === 'loading'}
      <p class="settings-status" role="status" aria-live="polite">
        Einstellungen werden lokal gelesen …
      </p>
    {:else if view.kind === 'error'}
      <div class="settings-error" role="status" aria-live="polite">
        <p>{view.message}</p>
        <button type="button" onclick={loadSettings}>Erneut laden</button>
      </div>
    {:else}
      {#if settingsView === 'general'}
        <section class="settings-page" aria-labelledby="general-settings-heading">
          <header class="settings-page-heading">
            <h3 id="general-settings-heading">Allgemein</h3>
            <p>Darstellung und grundlegendes Verhalten der Desktopanwendung.</p>
          </header>
          <div class="settings-list">
            <div class="settings-row settings-row-control">
              <div>
                <strong>Farbschema</strong>
                <span>Systemeinstellung verwenden oder bewusst überschreiben.</span>
              </div>
              <ThemeControls />
            </div>
          </div>
        </section>
      {:else if settingsView === 'provider'}
        <section class="settings-page" aria-labelledby="provider-settings-heading">
          <header class="settings-page-heading settings-page-heading-action">
            <div>
              <h3 id="provider-settings-heading">Provider</h3>
              <p>Lokale Modellverbindungen anlegen und verwalten.</p>
            </div>
            {#if view.settings.endpoint === null}
              <button
                class="primary-action"
                type="button"
                onclick={() => openProviderDialog('create')}>Provider hinzufügen</button
              >
            {/if}
          </header>

          {#if view.settings.endpoint === null}
            <div class="settings-empty-state">
              <strong>Kein Provider eingerichtet</strong>
              <p>
                A^3 bleibt als lokaler Indexbrowser voll nutzbar. Für Agentenfunktionen kannst du
                eine Ollama-Verbindung hinzufügen.
              </p>
              <button type="button" onclick={() => openProviderDialog('create')}
                >Ollama verbinden</button
              >
            </div>
          {:else}
            <div class="provider-list" aria-label="Eingerichtete Provider">
              <article class="provider-row">
                <div class="provider-logo" aria-hidden="true">O</div>
                <div class="provider-summary">
                  <div>
                    <strong>Ollama</strong>
                    <span class="settings-badge">{healthLabel(view.settings)}</span>
                  </div>
                  <code>{view.settings.endpoint.origin}</code>
                </div>
                <div class="provider-actions">
                  <button
                    type="button"
                    disabled={!canUseLocalProvider(view.settings)}
                    onclick={discoverModels}>Modelle erkennen</button
                  >
                  <button type="button" onclick={() => openProviderDialog('edit')}
                    >Bearbeiten</button
                  >
                  <button
                    class="subtle-danger-action"
                    type="button"
                    onclick={() => openProviderDialog('remove')}>Entfernen</button
                  >
                </div>
              </article>
            </div>
            {#if view.settings.endpoint.scope === 'remote'}
              <div class="remote-warning" role="alert">
                <strong>Remote-Verbindung blockiert</strong>
                <p>
                  A^3 führt ohne exakte Freigabe weder Modellerkennung noch Capability-Prüfung aus.
                </p>
              </div>
            {/if}
          {/if}
        </section>
      {:else if settingsView === 'models'}
        <section class="settings-page" aria-labelledby="model-settings-heading">
          <header class="settings-page-heading settings-page-heading-action">
            <div>
              <h3 id="model-settings-heading">Modelle</h3>
              <p>Installierte Modelle erkennen und klaren Aufgaben zuordnen.</p>
            </div>
            {#if view.settings.endpoint?.scope === 'localLoopback'}
              {#if action.kind === 'discovering' || (action.kind === 'cancelling' && action.role === null)}
                <button type="button" onclick={() => cancelOperation(null)}>
                  {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Erkennung abbrechen'}
                </button>
              {:else}
                <button type="button" disabled={operationBusy()} onclick={discoverModels}
                  >{modelCatalog === null ? 'Modelle erkennen' : 'Liste aktualisieren'}</button
                >
              {/if}
            {/if}
          </header>

          {#if view.settings.endpoint === null}
            <div class="settings-empty-state">
              <strong>Zuerst einen Provider verbinden</strong>
              <p>Danach kann A^3 die lokal installierten Modelle direkt abfragen.</p>
              <button type="button" onclick={() => selectSettingsView('provider')}
                >Zu Provider</button
              >
            </div>
          {:else if view.settings.endpoint.scope === 'remote'}
            <div class="remote-warning" role="alert">
              <strong>Lokale Modellerkennung nicht verfügbar</strong>
              <p>Der konfigurierte Endpoint ist nicht an den lokalen Rechner gebunden.</p>
            </div>
          {:else if modelCatalog === null}
            <div class="settings-empty-state">
              <strong>Noch keine Modellliste geladen</strong>
              <p>
                Die Abfrage startet nur durch deinen Klick. Sie liest ausschließlich Modellnamen vom
                lokalen Ollama-Endpoint.
              </p>
              <button type="button" onclick={discoverModels}>Modelle erkennen</button>
            </div>
          {:else}
            <div class="model-catalog-summary" role="status">
              <span>{modelCatalog.modelIds.length} Modelle gefunden</span>
              {#if modelCatalog.truncated}<span>Liste begrenzt</span>{/if}
            </div>
            {#if modelCatalog.modelIds.length === 0}
              <div class="settings-empty-state">
                <strong>Keine installierten Modelle gefunden</strong>
                <p>Installiere zuerst ein Modell in Ollama und aktualisiere danach die Liste.</p>
              </div>
            {:else}
              <div class="model-role-list" aria-label="Modellzuordnungen">
                {#each modelRoles as role (role)}
                  <article class="model-role-row">
                    <div>
                      <strong>{roleLabel(role)}</strong>
                      <span>{rolePurpose(role)}</span>
                    </div>
                    <div class="model-role-selection">
                      <code>{roleModelId(view.settings, role) ?? 'Nicht zugeordnet'}</code>
                      <span
                        class:capability-limited={roleStatus(view.settings, role) ===
                          'Capability fehlt'}>{roleStatus(view.settings, role)}</span
                      >
                    </div>
                    <button type="button" onclick={() => openRoleDialog(role)}>
                      {roleModelId(view.settings, role) === null ? 'Einrichten' : 'Ändern'}
                    </button>
                  </article>
                {/each}
              </div>
            {/if}
          {/if}
        </section>
      {:else if settingsView === 'project'}
        <section class="settings-page" aria-labelledby="project-page-heading">
          <header class="settings-page-heading">
            <h3 id="project-page-heading">Projekt</h3>
            <p>Repository-eigene Index- und Ausführungsgrenzen.</p>
          </header>
          <ProjectSettingsPanel />
        </section>
      {:else if settingsView === 'privacy'}
        <section class="settings-page" aria-labelledby="privacy-heading">
          <header class="settings-page-heading">
            <h3 id="privacy-heading">Datenschutz</h3>
            <p>
              Local-first Grenzen dieses Builds. Nicht verfügbare Funktionen sind keine Schalter.
            </p>
          </header>
          <div class="settings-list privacy-list">
            <div class="settings-row"><span>Telemetrie</span><strong>Aus</strong></div>
            <div class="settings-row"><span>Cloud-Synchronisierung</span><strong>Aus</strong></div>
            <div class="settings-row">
              <span>Provider-Erkennung im Hintergrund</span><strong>Aus</strong>
            </div>
            <div class="settings-row">
              <span>Prompt- und Antwort-Logging</span><strong>Aus</strong>
            </div>
            <div class="settings-row">
              <span>Remote ohne exakte Freigabe</span><strong>Aus</strong>
            </div>
          </div>
        </section>
      {/if}

      {#if action.kind === 'error'}
        <p class="settings-error-message" role="alert">{action.message}</p>
      {:else if action.kind === 'configuring'}
        <p class="settings-status" role="status" aria-live="polite">
          Provider wird lokal validiert …
        </p>
      {:else if action.kind === 'discovering'}
        <p class="settings-status" role="status" aria-live="polite">
          Installierte Ollama-Modelle werden abgefragt …
        </p>
      {:else if action.kind === 'probing'}
        <p class="settings-status" role="status" aria-live="polite">
          Capability-Prüfung für {roleLabel(action.role)} läuft …
        </p>
      {/if}
    {/if}
  </div>
</section>

{#if providerDialog === 'create' || providerDialog === 'edit'}
  <dialog
    class="modal-dialog settings-dialog"
    aria-labelledby="provider-dialog-heading"
    use:presentModal
    oncancel={(event) => {
      event.preventDefault();
      providerDialog = 'closed';
    }}
  >
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void saveProvider(endpointOrigin.trim());
      }}
    >
      <div class="modal-heading">
        <div>
          <h3 id="provider-dialog-heading">
            {providerDialog === 'create' ? 'Provider hinzufügen' : 'Provider bearbeiten'}
          </h3>
          <p>Verbindung wird erst bei einer bewussten Aktion verwendet.</p>
        </div>
        <button
          type="button"
          aria-label="Dialog schließen"
          onclick={() => (providerDialog = 'closed')}>×</button
        >
      </div>
      <div class="settings-dialog-body">
        <label for="provider-kind">
          Provider
          <select id="provider-kind" bind:value={providerKind} disabled={operationBusy()}>
            <option value="ollama">Ollama</option>
          </select>
        </label>
        <label for="model-endpoint">
          Endpoint
          <input
            id="model-endpoint"
            type="url"
            maxlength="2048"
            required
            spellcheck="false"
            autocomplete="off"
            bind:value={endpointOrigin}
            disabled={operationBusy()}
          />
        </label>
        <p>Standard: <code>http://127.0.0.1:11434</code></p>
      </div>
      <div class="modal-actions">
        <button type="button" onclick={() => (providerDialog = 'closed')}>Abbrechen</button>
        <button class="primary-action" type="submit" disabled={operationBusy()}>
          {providerDialog === 'create' ? 'Provider hinzufügen' : 'Änderungen speichern'}
        </button>
      </div>
    </form>
  </dialog>
{/if}

{#if providerDialog === 'remove'}
  <dialog
    class="modal-dialog settings-dialog"
    aria-labelledby="remove-provider-heading"
    use:presentModal
    oncancel={(event) => {
      event.preventDefault();
      providerDialog = 'closed';
    }}
  >
    <div class="modal-heading">
      <div>
        <h3 id="remove-provider-heading">Provider entfernen?</h3>
        <p>Alle verifizierten Modellzuordnungen werden ungültig.</p>
      </div>
      <button
        type="button"
        aria-label="Dialog schließen"
        onclick={() => (providerDialog = 'closed')}>×</button
      >
    </div>
    <p>Der lokale Ollama-Dienst und seine Modelle werden nicht gelöscht.</p>
    <div class="modal-actions">
      <button type="button" onclick={() => (providerDialog = 'closed')}>Abbrechen</button>
      <button class="risk-action" type="button" onclick={() => saveProvider(null)}
        >Provider entfernen</button
      >
    </div>
  </dialog>
{/if}

{#if roleDialog !== null && view.kind === 'ready'}
  <dialog
    class="modal-dialog settings-dialog model-dialog"
    aria-labelledby="model-dialog-heading"
    use:presentModal
    oncancel={(event) => {
      event.preventDefault();
      if (action.kind === 'idle' || action.kind === 'error') roleDialog = null;
    }}
  >
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void probeSelectedRole();
      }}
    >
      <div class="modal-heading">
        <div>
          <h3 id="model-dialog-heading">{roleLabel(roleDialog)} einrichten</h3>
          <p>{rolePurpose(roleDialog)}</p>
        </div>
        <button
          type="button"
          aria-label="Dialog schließen"
          disabled={action.kind === 'probing' || action.kind === 'cancelling'}
          onclick={() => (roleDialog = null)}>×</button
        >
      </div>
      <div class="settings-dialog-body">
        <label for="role-model-select">
          Modell
          <select
            id="role-model-select"
            value={selectedModel(roleDialog)}
            disabled={operationBusy()}
            onchange={(event) => handleRoleModelChange(event.currentTarget.value)}
          >
            {#each modelOptions(roleDialog) as modelId (modelId)}
              <option value={modelId}>{modelId}</option>
            {/each}
          </select>
        </label>

        <details class="advanced-model-settings">
          <summary>Erweiterte Limits</summary>
          {#if roleDialog === 'coding'}
            <div class="resource-grid">
              <label
                >Kontext <input
                  type="number"
                  min="1024"
                  max="1048576"
                  bind:value={codingContextTokens}
                /></label
              >
              <label
                >Output <input
                  type="number"
                  min="1"
                  max="262144"
                  bind:value={codingOutputTokens}
                /></label
              >
              <label
                >Parallelität <input
                  type="number"
                  min="1"
                  max="64"
                  bind:value={codingParallelism}
                /></label
              >
            </div>
          {:else if roleDialog === 'mapping'}
            <div class="resource-grid">
              <label
                >Kontext <input
                  type="number"
                  min="1024"
                  max="1048576"
                  bind:value={mappingContextTokens}
                /></label
              >
              <label
                >Output <input
                  type="number"
                  min="1"
                  max="262144"
                  bind:value={mappingOutputTokens}
                /></label
              >
              <label
                >Parallelität <input
                  type="number"
                  min="1"
                  max="64"
                  bind:value={mappingParallelism}
                /></label
              >
            </div>
          {:else}
            <label>
              Maximale Batch-Größe
              <input type="number" min="1" max="64" bind:value={embeddingBatchSize} />
            </label>
            <p>Die Vektordimension wird aus einer echten Modellantwort abgeleitet.</p>
          {/if}
        </details>
        <p class="probe-explanation">
          A^3 prüft die erforderliche Capability live. Der Modellname allein aktiviert nichts.
        </p>
      </div>
      <div class="modal-actions">
        {#if (action.kind === 'probing' || action.kind === 'cancelling') && action.role === roleDialog}
          <button type="button" class="cancel-probe" onclick={() => cancelOperation(roleDialog)}>
            {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Prüfung abbrechen'}
          </button>
        {:else}
          <button type="button" onclick={() => (roleDialog = null)}>Abbrechen</button>
          <button class="primary-action" type="submit" disabled={!canProbe(view.settings)}
            >Auswählen und prüfen</button
          >
        {/if}
      </div>
    </form>
  </dialog>
{/if}

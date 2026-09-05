<script lang="ts">
  import { onMount } from 'svelte';
  import { parseCommandErrorV1 } from './command-error';
  import ProjectSettingsPanel from './ProjectSettingsPanel.svelte';
  import ThemeControls from './ThemeControls.svelte';
  import { queryHealth, type HealthResponseV1 } from './health';
  import {
    cancelModelProbe,
    configureModelProvider,
    deleteModelProviderCredential,
    discoverProviderModels,
    probeModelRole,
    querySettings,
    setModelProviderCredential,
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
    credentialSetter?: (
      expectedRevision: string,
      apiKeyBytes: Uint8Array,
    ) => Promise<SettingsResponseV1>;
    credentialDeleter?: (expectedRevision: string) => Promise<SettingsResponseV1>;
    roleProber?: (
      expectedRevision: string,
      input: ModelProbeInputV1,
    ) => Promise<SettingsResponseV1>;
    settingsLoader?: () => Promise<SettingsResponseV1>;
  }

  type SettingsSection = 'general' | 'models' | 'project' | 'privacy' | 'about';
  type View =
    | { kind: 'loading' }
    | { kind: 'ready'; settings: SettingsV1 }
    | { kind: 'error'; message: string };
  type Action =
    | { kind: 'idle' }
    | { kind: 'configuring' }
    | { kind: 'credential'; operation: 'storing' | 'deleting' }
    | { kind: 'discovering' }
    | { kind: 'probing'; role: ModelRoleV1 }
    | { kind: 'cancelling'; role: ModelRoleV1 | null }
    | { kind: 'error'; message: string };
  type HealthView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'ready'; health: HealthResponseV1 }
    | { kind: 'error' };
  type ProviderDialog = 'closed' | 'create' | 'edit' | 'remove' | 'deleteCredential';
  type StatusExplanation = { title: string; detail: string; nextStep: string };

  const settingsSections: { id: SettingsSection; label: string }[] = [
    { id: 'general', label: 'Allgemein' },
    { id: 'models', label: 'KI & Modelle' },
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
    credentialSetter = setModelProviderCredential,
    credentialDeleter = deleteModelProviderCredential,
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
  let apiKeyInput = $state<HTMLInputElement | null>(null);

  onMount(() => {
    void loadSettings();
    return clearCredentialInput;
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
    clearCredentialInput();
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

  function openProviderDialog(
    mode: Exclude<ProviderDialog, 'closed'>,
    preferredKind?: ModelProviderKindV1,
  ): void {
    clearCredentialInput();
    if (view.kind === 'ready' && view.settings.endpoint !== null) {
      endpointOrigin = view.settings.endpoint.origin;
      providerKind = providerKindFromId(view.settings.endpoint.providerId);
    } else {
      handleProviderKindChange(preferredKind ?? 'ollama');
    }
    providerDialog = mode;
  }

  function handleProviderKindChange(kind: ModelProviderKindV1): void {
    providerKind = kind;
    endpointOrigin = defaultProviderOrigin(kind);
  }

  function providerLabel(providerId: string): string {
    if (providerId === 'gemini') return 'Google Gemini';
    if (providerId === 'openai') return 'OpenAI';
    return 'Ollama';
  }

  function providerKindFromId(providerId: string): ModelProviderKindV1 {
    if (providerId === 'gemini') return 'gemini';
    if (providerId === 'openai') return 'openai';
    return 'ollama';
  }

  function defaultProviderOrigin(kind: ModelProviderKindV1): string {
    if (kind === 'gemini') return 'https://generativelanguage.googleapis.com';
    if (kind === 'openai') return 'https://api.openai.com';
    return 'http://127.0.0.1:11434';
  }

  function providerInitial(providerId: string): string {
    if (providerId === 'gemini') return 'G';
    if (providerId === 'openai') return 'A';
    return 'O';
  }

  function requiresApiKey(kind: ModelProviderKindV1): boolean {
    return kind === 'gemini' || kind === 'openai';
  }

  function remoteProviderHost(providerId: string): string {
    return providerId === 'openai' ? 'api.openai.com' : 'generativelanguage.googleapis.com';
  }

  async function saveProvider(endpoint: string | null): Promise<void> {
    if (view.kind !== 'ready') return;
    const initialConnection = providerDialog === 'create';
    const needsCatalogAfterCredential = view.settings.credential?.status !== 'configured';
    const credentialBytes = captureCredentialInput();
    let providerConfigured = false;
    let credentialAttempted = false;
    try {
      if (endpoint !== null && activeProviderMatches(providerKind, endpoint)) {
        if (credentialBytes !== null) {
          credentialAttempted = true;
          action = { kind: 'credential', operation: 'storing' };
          applyResponse(await credentialSetter(view.settings.revision, credentialBytes), false);
        }
        providerDialog = 'closed';
        if (credentialBytes !== null && needsCatalogAfterCredential) {
          await refreshModelCatalogAfterConnection();
        }
        return;
      }

      action = { kind: 'configuring' };
      const response = await providerConfigurer(view.settings.revision, providerKind, endpoint);
      providerConfigured = true;
      applyResponse(response, false);
      if (credentialBytes !== null) {
        credentialAttempted = true;
        action = { kind: 'credential', operation: 'storing' };
        applyResponse(await credentialSetter(response.settings.revision, credentialBytes), false);
      }
      providerDialog = 'closed';
      if (endpoint === null) endpointOrigin = 'http://127.0.0.1:11434';
      if (endpoint !== null && initialConnection) await refreshModelCatalogAfterConnection();
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
      if (credentialAttempted) await refreshAfterAction();
      if (providerConfigured && endpoint !== null) providerDialog = 'edit';
    } finally {
      credentialBytes?.fill(0);
      clearCredentialInput();
    }
  }

  function clearCredentialInput(): void {
    if (apiKeyInput !== null) apiKeyInput.value = '';
  }

  function closeProviderDialog(): void {
    clearCredentialInput();
    providerDialog = 'closed';
  }

  function captureCredentialInput(): Uint8Array | null {
    if (!requiresApiKey(providerKind) || apiKeyInput === null || apiKeyInput.value.length === 0) {
      return null;
    }
    const encoded = new TextEncoder().encode(apiKeyInput.value);
    clearCredentialInput();
    return encoded;
  }

  function activeProviderMatches(provider: ModelProviderKindV1, endpoint: string): boolean {
    return (
      view.kind === 'ready' &&
      view.settings.endpoint?.providerId === provider &&
      view.settings.endpoint.origin === endpoint
    );
  }

  async function deleteCredential(): Promise<void> {
    if (view.kind !== 'ready') return;
    clearCredentialInput();
    action = { kind: 'credential', operation: 'deleting' };
    try {
      applyResponse(await credentialDeleter(view.settings.revision), false);
      providerDialog = 'closed';
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
      await refreshAfterAction();
    }
  }

  async function discoverModels(): Promise<void> {
    if (view.kind !== 'ready' || !canUseActiveProvider(view.settings)) return;
    action = { kind: 'discovering' };
    try {
      const catalog = await modelDiscoverer(view.settings.revision);
      if (
        catalog.settingsRevision !== view.settings.revision ||
        catalog.providerKind !== providerKindFromId(view.settings.endpoint?.providerId ?? '')
      ) {
        throw new Error('Provider model catalog is stale.');
      }
      modelCatalog = catalog;
      action = { kind: 'idle' };
    } catch (error) {
      action = {
        kind: 'error',
        message: discoveryRecoveryMessage(error, view.settings.endpoint?.providerId),
      };
    }
  }

  async function refreshModelCatalogAfterConnection(): Promise<void> {
    if (view.kind === 'ready' && canUseActiveProvider(view.settings)) await discoverModels();
  }

  function openRoleDialog(role: ModelRoleV1): void {
    const currentModel = roleModelId(view.kind === 'ready' ? view.settings : null, role);
    const firstDiscoveredModel = modelCatalog?.modelIds[0] ?? null;
    if (currentModel === null && firstDiscoveredModel === null) return;
    setSelectedModel(role, currentModel ?? firstDiscoveredModel!);
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

  function hasRoleAssignments(settings: SettingsV1): boolean {
    return modelRoles.some((role) => roleModelId(settings, role) !== null);
  }

  function canOpenRoleDialog(settings: SettingsV1, role: ModelRoleV1): boolean {
    return roleModelId(settings, role) !== null || (modelCatalog?.modelIds.length ?? 0) > 0;
  }

  function canUseActiveProvider(settings: SettingsV1): boolean {
    if (settings.endpoint === null) return false;
    if (settings.endpoint.access === 'explicitUserInitiatedRemote') {
      return settings.credential?.status === 'configured' && !operationBusy();
    }
    return settings.endpoint.scope === 'localLoopback' && !operationBusy();
  }

  function canProbe(settings: SettingsV1): boolean {
    return canUseActiveProvider(settings) && modelCatalog !== null;
  }

  function operationBusy(): boolean {
    return (
      action.kind === 'configuring' ||
      action.kind === 'credential' ||
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

  function roleStatusExplanation(settings: SettingsV1, role: ModelRoleV1): StatusExplanation {
    if (role === 'embedding') {
      if (settings.embeddingProfile !== null) {
        return {
          title: 'Embedding-Fähigkeit verifiziert',
          detail:
            'A^3 hat eine echte Vektorantwort mit einer gültigen Dimension erhalten. Diese Prüfung ist unabhängig von Coding und Deep Map.',
          nextStep:
            'Die semantische Suche kann dieses Modell verwenden, solange Provider und Modell unverändert bleiben.',
        };
      }
      return {
        title: 'Noch nicht eingerichtet',
        detail:
          'Für diese Aufgabe wurde noch kein Modell live geprüft. Ein Modellname allein schaltet keine Funktion frei.',
        nextStep: 'Wähle ein Modell aus der geladenen Liste und starte die Prüfung.',
      };
    }
    const profile = role === 'coding' ? settings.codingProfile : settings.mappingProfile;
    if (profile === null) {
      return {
        title: 'Noch nicht eingerichtet',
        detail:
          'Für diese Aufgabe wurde noch kein Modell live geprüft. Ein Modellname allein schaltet keine Funktion frei.',
        nextStep: 'Wähle ein Modell aus der geladenen Liste und starte die Prüfung.',
      };
    }
    if (profile.activation === 'capabilityLimited') {
      return {
        title: 'Strukturiertes JSON konnte nicht verifiziert werden',
        detail:
          'A^3 hat für dieses Modell eine minimale Antwort nach einem festen JSON-Schema angefordert. Die Antwort war nicht schema-konform oder diese API-Funktion war für das Modell nicht verfügbar. Chatten kann ein Modell trotzdem – Agentenaktionen und Deep Map bleiben aber sicher gesperrt.',
        nextStep: `Wähle ein anderes Modell aus der geladenen Liste und prüfe es erneut. Bleibt der Status bei allen Modellen gleich, prüfe ${providerLabel(settings.endpoint?.providerId ?? '')}, API-Key und Verbindung und versuche die Prüfung später erneut.`,
      };
    }
    return {
      title: 'Capability verifiziert',
      detail:
        'Das Modell hat die erforderliche strukturierte JSON-Antwort live geliefert. Nur dadurch darf A^3 diese Rolle für kontrollierte Ausgaben verwenden.',
      nextStep:
        'Änderungen an Provider oder Modell machen die Prüfung ungültig; führe sie danach erneut aus.',
    };
  }

  function providerHealthExplanation(settings: SettingsV1): StatusExplanation {
    switch (settings.providerHealth?.status ?? 'notChecked') {
      case 'healthy':
        return {
          title: 'Letzte Prüfung erfolgreich',
          detail: 'Die letzte explizite Modellprüfung beim aktiven Provider war erfolgreich.',
          nextStep: 'Die einzelnen Rollen behalten trotzdem ihren eigenen Verifikationsstatus.',
        };
      case 'capabilityLimited':
        return {
          title: 'Mindestens eine Modellprüfung war eingeschränkt',
          detail:
            'Der Provider ist erreichbar, aber eine Rollenprüfung konnte die nötige Capability nicht bestätigen.',
          nextStep:
            'Öffne die betroffene Rolle über „Ändern“, lies die Statushilfe und prüfe ein anderes Modell.',
        };
      case 'unreachable':
        return {
          title: 'Provider nicht erreichbar',
          detail:
            'Die letzte bewusste Modelloperation konnte den aktiven Provider nicht erreichen.',
          nextStep:
            'Prüfe die lokale Provider-App beziehungsweise Internetverbindung und lade die Modelle erneut.',
        };
      case 'cancelled':
        return {
          title: 'Prüfung abgebrochen',
          detail:
            'Die letzte bewusste Modelloperation wurde beendet, bevor sie eine Capability bestätigen konnte.',
          nextStep: 'Starte die gewünschte Modellprüfung erneut, wenn der Provider bereit ist.',
        };
      case 'remoteBlocked':
        return {
          title: 'Remote-Verbindung blockiert',
          detail:
            'Dieser Endpoint gehört nicht zu einer für diese Aktion erlaubten Providerverbindung.',
          nextStep:
            'Verwende einen unterstützten Provider-Endpunkt; eine allgemeine Remote-Freigabe gibt es nicht.',
        };
      default:
        return {
          title: 'Noch nicht geprüft',
          detail:
            'A^3 prüft Provider nicht automatisch. Einstellungen lesen und speichern erzeugt keinen Netzwerkzugriff.',
          nextStep:
            'Lade die Modelle oder prüfe eine Rolle bewusst, wenn du die Verbindung verwenden möchtest.',
        };
    }
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

  function credentialLabel(settings: SettingsV1): string {
    const status = settings.credential?.status;
    if (status === 'configured') return 'API-Key sicher gespeichert';
    if (status === 'recoveryRequired') return 'API-Key muss repariert werden';
    if (status === 'unavailable') return 'OS-Schlüsselspeicher nicht verfügbar';
    return 'API-Key fehlt';
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

  function discoveryRecoveryMessage(error: unknown, providerId?: string): string {
    const parsed = parseCommandErrorV1(error);
    if (parsed?.code === 'modelEndpointInvalid') {
      return 'Die Modellerkennung ist nur für eine gültige Providerverbindung verfügbar.';
    }
    if (parsed?.code === 'modelProbeAlreadyActive') {
      return 'Eine andere Modelloperation läuft bereits.';
    }
    if (parsed?.code === 'invalidSettingsRequest') {
      return 'Die Providerverbindung hat sich geändert. Lade die Einstellungen neu.';
    }
    if (providerId === 'gemini') {
      return 'Die Gemini-Modelle konnten nicht abgefragt werden. Prüfe den gespeicherten API-Key und deine Internetverbindung.';
    }
    if (providerId === 'openai') {
      return 'Die OpenAI-Modelle konnten nicht abgefragt werden. Prüfe den gespeicherten API-Key, deinen OpenAI-Zugriff und die Internetverbindung.';
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
    if (parsed?.code === 'providerCredentialInvalid') {
      return 'Der API-Key ist leer, zu lang oder enthält ungültige Zeichen.';
    }
    if (parsed?.code === 'providerCredentialMissing') {
      return 'Hinterlege zuerst den API-Key des aktiven Providers.';
    }
    if (parsed?.code === 'providerCredentialRecoveryRequired') {
      return 'Der API-Key-Zustand ist unvollständig. Ersetze oder lösche den Schlüssel.';
    }
    if (parsed?.code === 'providerCredentialStoreUnavailable') {
      return 'Der Betriebssystem-Schlüsselspeicher ist gesperrt oder nicht verfügbar.';
    }
    return parsed?.message ?? 'Die Modell-Einstellungen konnten nicht sicher verarbeitet werden.';
  }
</script>

<section class="settings-shell" aria-label="Einstellungen">
  <nav class="settings-tabs" aria-label="Einstellungsbereiche">
    {#each settingsSections as section (section.id)}
      <button
        type="button"
        aria-current={settingsView === section.id ? 'page' : undefined}
        onclick={() => selectSettingsView(section.id)}>{section.label}</button
      >
    {/each}
  </nav>

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
            <p>Gestalte A^3 so, wie du am liebsten arbeitest.</p>
          </header>
          <div class="settings-list">
            <div class="settings-row settings-row-control">
              <div>
                <strong>Farbschema</strong>
                <span>Automatisch, hell oder dunkel.</span>
              </div>
              <ThemeControls />
            </div>
          </div>
        </section>
      {:else if settingsView === 'models'}
        <section class="settings-page model-setup-page" aria-labelledby="model-settings-heading">
          <header class="settings-page-heading">
            <div>
              <h3 id="model-settings-heading">KI &amp; Modelle</h3>
              <p>Verbinde deine KI und wähle die Modelle für deine Arbeit.</p>
            </div>
          </header>

          <section class="model-setup-section" aria-labelledby="provider-setup-heading">
            <header class="setup-section-heading">
              <div>
                <span class="setup-step" aria-hidden="true">1</span>
                <div>
                  <h4 id="provider-setup-heading">KI verbinden</h4>
                  <p>Lokal mit Ollama oder über einen unterstützten Online-Anbieter.</p>
                </div>
              </div>
            </header>

            {#if view.settings.endpoint === null}
              <div class="setup-empty-state">
                <div>
                  <strong>Bereit für deine Modellverbindung</strong>
                  <p>
                    Deinen Code kannst du bereits erkunden. Verbinde jetzt ein Modell für Fragen,
                    Pläne und Agentenaufgaben.
                  </p>
                </div>
                <div class="provider-choice-grid" aria-label="Provider auswählen">
                  <button
                    class="primary-action"
                    type="button"
                    onclick={() => openProviderDialog('create', 'ollama')}>Ollama verbinden</button
                  >
                  <button type="button" onclick={() => openProviderDialog('create', 'gemini')}
                    >Google Gemini verwenden</button
                  >
                  <button type="button" onclick={() => openProviderDialog('create', 'openai')}
                    >OpenAI verwenden</button
                  >
                </div>
              </div>
            {:else}
              <div class="provider-list" aria-label="Aktive Providerverbindung">
                <article class="provider-row">
                  <div class="provider-logo" aria-hidden="true">
                    {providerInitial(view.settings.endpoint.providerId)}
                  </div>
                  <div class="provider-summary">
                    <div>
                      <strong>{providerLabel(view.settings.endpoint.providerId)}</strong>
                      <span class="settings-badge">{healthLabel(view.settings)}</span>
                      <details class="status-help">
                        <summary aria-label="Providerstatus erklären"
                          ><span aria-hidden="true">i</span></summary
                        >
                        <div class="status-help-popover" role="note">
                          <strong>{providerHealthExplanation(view.settings).title}</strong>
                          <p>{providerHealthExplanation(view.settings).detail}</p>
                          <p>{providerHealthExplanation(view.settings).nextStep}</p>
                        </div>
                      </details>
                    </div>
                    <code>{view.settings.endpoint.origin}</code>
                  </div>
                  <div class="provider-actions">
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
              {#if view.settings.endpoint.access === 'remoteBlocked'}
                <div class="remote-warning" role="alert">
                  <strong>Remote-Verbindung blockiert</strong>
                  <p>
                    A^3 führt ohne exakte Freigabe weder Modellerkennung noch Capability-Prüfung
                    aus.
                  </p>
                </div>
              {/if}
              {#if view.settings.endpoint.access === 'explicitUserInitiatedRemote'}
                <div class="remote-warning" role="note">
                  <strong>{providerLabel(view.settings.endpoint.providerId)} Cloud</strong>
                  <p>
                    Modellerkennung und Capability-Prüfung senden erst nach deinem Klick Daten an
                    <code>{remoteProviderHost(view.settings.endpoint.providerId)}</code>. Spätere
                    Agentenläufe können Prompts und ausgewählten Quelltext nur mit einer eigenen
                    laufgebundenen Netzwerkfreigabe an {providerLabel(
                      view.settings.endpoint.providerId,
                    )} senden. Das Speichern des Keys erzeugt keinen Netzwerkzugriff.
                  </p>
                  {#if view.settings.endpoint.providerId === 'openai'}
                    <p>OpenAI-Anfragen können abhängig vom Konto und Modell Kosten verursachen.</p>
                  {/if}
                </div>
              {/if}
            {/if}
          </section>

          <section class="model-setup-section" aria-labelledby="role-setup-heading">
            <header class="setup-section-heading setup-section-heading-action">
              <div>
                <span class="setup-step" aria-hidden="true">2</span>
                <div>
                  <h4 id="role-setup-heading">Aufgaben zuordnen</h4>
                  <p>
                    Jede Zuordnung wird separat geprüft. Ein Modellname allein aktiviert keine
                    Funktion.
                  </p>
                </div>
              </div>
              {#if view.settings.endpoint !== null}
                {#if action.kind === 'discovering' || (action.kind === 'cancelling' && action.role === null)}
                  <button type="button" onclick={() => cancelOperation(null)}>
                    {action.kind === 'cancelling' ? 'Abbruch angefordert …' : 'Erkennung abbrechen'}
                  </button>
                {:else}
                  <button
                    type="button"
                    disabled={!canUseActiveProvider(view.settings)}
                    onclick={discoverModels}>Modelle aktualisieren</button
                  >
                {/if}
              {/if}
            </header>

            {#if view.settings.endpoint === null}
              <div class="setup-pending-state">
                <strong>Provider erforderlich</strong>
                <p>Verbinde zuerst einen Provider. Die aktuelle Modellliste wird danach geladen.</p>
              </div>
            {:else if view.settings.endpoint.access === 'remoteBlocked'}
              <div class="remote-warning" role="note">
                <strong>Modellzuordnung nicht verfügbar</strong>
                <p>Der konfigurierte Endpoint ist nicht an den lokalen Rechner gebunden.</p>
              </div>
            {:else if view.settings.endpoint.access === 'explicitUserInitiatedRemote' && view.settings.credential?.status !== 'configured'}
              <div class="setup-pending-state">
                <strong
                  >{providerLabel(view.settings.endpoint.providerId)} API-Key erforderlich</strong
                >
                <p>Speichere oder repariere den API-Key im vorherigen Schritt.</p>
              </div>
            {:else if modelCatalog !== null}
              <div class="model-catalog-summary" role="status">
                <span>{modelCatalog.modelIds.length} Modelle gefunden</span>
                {#if modelCatalog.truncated}<span>Liste begrenzt</span>{/if}
              </div>
              {#if modelCatalog.modelIds.length === 0}
                <div class="setup-pending-state">
                  <strong>Keine Modelle gefunden</strong>
                  <p>Prüfe den Provider und aktualisiere anschließend die Modellliste.</p>
                </div>
              {/if}
            {/if}

            {#if (modelCatalog !== null && modelCatalog.modelIds.length > 0) || hasRoleAssignments(view.settings)}
              <div class="model-role-list" aria-label="Modellzuordnungen">
                {#each modelRoles as role (role)}
                  <article class="model-role-row">
                    <div>
                      <strong>{roleLabel(role)}</strong>
                      <span>{rolePurpose(role)}</span>
                    </div>
                    <div class="model-role-selection">
                      <code>{roleModelId(view.settings, role) ?? 'Nicht zugeordnet'}</code>
                      <div class="model-role-status">
                        <span
                          class:capability-limited={roleStatus(view.settings, role) ===
                            'Capability fehlt'}>{roleStatus(view.settings, role)}</span
                        >
                        <details class="status-help">
                          <summary aria-label={`Status für ${roleLabel(role)} erklären`}
                            ><span aria-hidden="true">i</span></summary
                          >
                          <div class="status-help-popover" role="note">
                            <strong>{roleStatusExplanation(view.settings, role).title}</strong>
                            <p>{roleStatusExplanation(view.settings, role).detail}</p>
                            <p>{roleStatusExplanation(view.settings, role).nextStep}</p>
                          </div>
                        </details>
                      </div>
                    </div>
                    <button
                      type="button"
                      disabled={!canOpenRoleDialog(view.settings, role)}
                      onclick={() => openRoleDialog(role)}
                      >{roleModelId(view.settings, role) === null ? 'Einrichten' : 'Ändern'}</button
                    >
                  </article>
                {/each}
              </div>
            {:else}
              <div class="setup-pending-state">
                <strong>Modellliste wird benötigt</strong>
                <p>
                  Nach dem Verbinden lädt A^3 die aktuelle Modellliste. Falls sie nicht verfügbar
                  ist, aktualisiere sie über die Schaltfläche oben.
                </p>
              </div>
            {/if}
          </section>
        </section>
      {:else if settingsView === 'project'}
        <section class="settings-page" aria-labelledby="project-page-heading">
          <header class="settings-page-heading">
            <h3 id="project-page-heading">Projekt</h3>
            <p>Dateien und Befehle, die A^3 in diesem Projekt nutzen darf.</p>
          </header>
          <ProjectSettingsPanel />
        </section>
      {:else if settingsView === 'privacy'}
        <section class="settings-page" aria-labelledby="privacy-heading">
          <header class="settings-page-heading">
            <h3 id="privacy-heading">Datenschutz</h3>
            <p>Du bestimmst, welche Daten A^3 weitergeben darf.</p>
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
      {:else if action.kind === 'credential'}
        <p class="settings-status" role="status" aria-live="polite">
          {action.operation === 'storing'
            ? 'API-Key wird sicher gespeichert …'
            : 'API-Key wird aus dem Betriebssystem-Schlüsselspeicher gelöscht …'}
        </p>
      {:else if action.kind === 'discovering'}
        <p class="settings-status" role="status" aria-live="polite">
          Modellliste wird aktualisiert …
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
      closeProviderDialog();
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
            {providerDialog === 'create'
              ? `${providerLabel(providerKind)} verbinden`
              : `${providerLabel(providerKind)} bearbeiten`}
          </h3>
          <p>
            {providerDialog === 'create'
              ? 'Mit „Verbinden und Modelle laden“ bestätigst du die Verbindung und die einmalige Modellerkennung.'
              : 'Speichern erzeugt keinen Netzwerkzugriff. Aktualisiere die Modellliste anschließend bei Bedarf.'}
          </p>
        </div>
        <button type="button" aria-label="Dialog schließen" onclick={closeProviderDialog}>×</button>
      </div>
      <div class="settings-dialog-body">
        <label for="provider-kind">
          Provider
          <select
            id="provider-kind"
            value={providerKind}
            disabled={operationBusy()}
            onchange={(event) =>
              handleProviderKindChange(event.currentTarget.value as ModelProviderKindV1)}
          >
            <option value="ollama">Ollama</option>
            <option value="gemini">Google Gemini</option>
            <option value="openai">OpenAI</option>
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
            readonly={requiresApiKey(providerKind)}
            disabled={operationBusy()}
          />
        </label>
        {#if requiresApiKey(providerKind)}
          <p>Standard: <code>{defaultProviderOrigin(providerKind)}</code></p>
          <p class="probe-explanation">
            Der API-Key wird beim Speichern direkt in den geschützten
            Betriebssystem-Schlüsselspeicher übernommen.
          </p>
          <div class="dialog-credential">
            <div>
              <strong>{providerLabel(providerKind)} API-Key</strong>
              <span
                role="status"
                aria-live="polite"
                class:credential-warning={view.kind === 'ready' &&
                  view.settings.credential?.status !== 'configured'}
              >
                {view.kind === 'ready'
                  ? credentialLabel(view.settings)
                  : 'API-Key wird beim Speichern geschützt abgelegt'}
              </span>
            </div>
            <label for="provider-api-key">
              API-Key
              <input
                id="provider-api-key"
                bind:this={apiKeyInput}
                type="password"
                maxlength="4096"
                autocomplete="new-password"
                spellcheck="false"
                autocapitalize="none"
                required={view.kind !== 'ready' ||
                  view.settings.credential?.status !== 'configured'}
                placeholder={view.kind === 'ready' &&
                view.settings.credential?.status === 'configured'
                  ? '********'
                  : undefined}
                disabled={operationBusy()}
              />
            </label>
            <p>
              Der gespeicherte Key wird nie angezeigt. Die Sternchen sind ein fester Platzhalter und
              geben weder Wert noch Länge preis.
            </p>
            {#if providerDialog === 'edit' && view.kind === 'ready' && view.settings.credential?.status !== 'missing'}
              <button
                class="subtle-danger-action"
                type="button"
                disabled={operationBusy()}
                onclick={() => {
                  clearCredentialInput();
                  providerDialog = 'deleteCredential';
                }}>Key löschen</button
              >
            {/if}
          </div>
        {:else}
          <p>Standard: <code>http://127.0.0.1:11434</code></p>
        {/if}
      </div>
      <div class="modal-actions">
        <button type="button" onclick={() => (providerDialog = 'closed')}>Abbrechen</button>
        <button class="primary-action" type="submit" disabled={operationBusy()}>
          {providerDialog === 'create' ? 'Verbinden und Modelle laden' : 'Änderungen speichern'}
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
    <p>Der Provider-Endpunkt und seine API-Verbindung werden aus den Einstellungen entfernt.</p>
    {#if view.kind === 'ready' && view.settings.endpoint?.access === 'explicitUserInitiatedRemote'}
      <p>
        Der gespeicherte {providerLabel(view.settings.endpoint.providerId)} API-Key wird zuerst aus dem
        OS-Schlüsselspeicher gelöscht.
      </p>
    {/if}
    <div class="modal-actions">
      <button type="button" onclick={() => (providerDialog = 'closed')}>Abbrechen</button>
      <button class="risk-action" type="button" onclick={() => saveProvider(null)}
        >Provider entfernen</button
      >
    </div>
  </dialog>
{/if}

{#if providerDialog === 'deleteCredential'}
  <dialog
    class="modal-dialog settings-dialog"
    aria-labelledby="delete-credential-heading"
    use:presentModal
    oncancel={(event) => {
      event.preventDefault();
      clearCredentialInput();
      providerDialog = 'closed';
    }}
  >
    <div class="modal-heading">
      <div>
        <h3 id="delete-credential-heading">{providerLabel(providerKind)} API-Key löschen?</h3>
        <p>
          {providerLabel(providerKind)}-Modellerkennung und Capability-Prüfungen werden danach
          deaktiviert.
        </p>
      </div>
      <button
        type="button"
        aria-label="Dialog schließen"
        onclick={() => (providerDialog = 'closed')}>×</button
      >
    </div>
    <p>
      Der Key wird aus dem Betriebssystem-Schlüsselspeicher entfernt und nicht wieder angezeigt.
    </p>
    <div class="modal-actions">
      <button type="button" onclick={() => (providerDialog = 'closed')}>Abbrechen</button>
      <button class="risk-action" type="button" onclick={deleteCredential}>Key löschen</button>
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

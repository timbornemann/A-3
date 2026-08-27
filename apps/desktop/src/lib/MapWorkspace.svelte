<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    queryProjectMapScene,
    type ProjectMapSceneModuleV1,
    type ProjectMapSceneResponseV1,
    type ProjectMapSceneV1,
  } from './project-map-scene';
  import {
    queryProjectMapSearch,
    type ProjectMapSearchHitV1,
    type ProjectMapSearchResponseV1,
  } from './project-map-search';
  import {
    queryModuleCardDetail,
    type ModuleCardDetailResponseV1,
    type ModuleCardDetailV1,
    type ModuleCardFieldKindV1,
  } from './module-card-detail';
  import {
    queryModuleCardEvidence,
    type ModuleCardEvidenceQueryV1,
    type ModuleCardEvidenceResponseV1,
  } from './module-card-evidence';
  import {
    queryProjectMapSourcePreview,
    type ProjectMapSourcePreviewResponseV1,
  } from './project-map-source-preview';
  import {
    queryModuleRuntimeMap,
    type ModuleRuntimeMapResponseV1,
    type ModuleRuntimeRootV1,
  } from './module-runtime';
  import {
    compileTaskLens,
    queryTaskLensTask,
    queryTaskLensTasks,
    type TaskLensCompileResponseV1,
    type TaskLensTaskResponseV1,
    type TaskLensTasksResponseV1,
  } from './task-lens';
  import {
    cancelDeepMap,
    pauseDeepMap,
    queryDeepMap,
    resumeDeepMap,
    startDeepMap,
    type DeepMapBudgetV1,
    type DeepMapControlResponseV1,
    type DeepMapStatusResponseV1,
  } from './deep-map';

  interface Props {
    projectKey: string;
    sceneLoader?: (query: { focusModuleId: string | null }) => Promise<ProjectMapSceneResponseV1>;
    searchLoader?: (query: { query: string }) => Promise<ProjectMapSearchResponseV1>;
    cardLoader?: (query: { moduleId: string }) => Promise<ModuleCardDetailResponseV1>;
    evidenceLoader?: (query: ModuleCardEvidenceQueryV1) => Promise<ModuleCardEvidenceResponseV1>;
    sourcePreviewLoader?: (
      query: ModuleCardEvidenceQueryV1,
    ) => Promise<ProjectMapSourcePreviewResponseV1>;
    runtimeLoader?: (query: {
      entrypointLimit: number;
      moduleId: string;
      testLimit: number;
    }) => Promise<ModuleRuntimeMapResponseV1>;
    taskLensTasksLoader?: () => Promise<TaskLensTasksResponseV1>;
    taskLensTaskLoader?: (query: { taskId: string }) => Promise<TaskLensTaskResponseV1>;
    taskLensCompiler?: (query: {
      stepId: string;
      taskId: string;
    }) => Promise<TaskLensCompileResponseV1>;
    deepMapStatusLoader?: () => Promise<DeepMapStatusResponseV1>;
    deepMapStarter?: (budget: DeepMapBudgetV1) => Promise<DeepMapControlResponseV1>;
    deepMapPauser?: () => Promise<DeepMapControlResponseV1>;
    deepMapResumer?: () => Promise<DeepMapControlResponseV1>;
    deepMapCanceller?: () => Promise<DeepMapControlResponseV1>;
  }

  const {
    projectKey,
    sceneLoader = queryProjectMapScene,
    searchLoader = queryProjectMapSearch,
    cardLoader = queryModuleCardDetail,
    evidenceLoader = queryModuleCardEvidence,
    sourcePreviewLoader = queryProjectMapSourcePreview,
    runtimeLoader = queryModuleRuntimeMap,
    taskLensTasksLoader = queryTaskLensTasks,
    taskLensTaskLoader = queryTaskLensTask,
    taskLensCompiler = compileTaskLens,
    deepMapStatusLoader = queryDeepMap,
    deepMapStarter = startDeepMap,
    deepMapPauser = pauseDeepMap,
    deepMapResumer = resumeDeepMap,
    deepMapCanceller = cancelDeepMap,
  }: Props = $props();

  type SceneView =
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'empty'; message: string }
    | { kind: 'available'; scene: ProjectMapSceneV1 };
  type SearchView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'empty' }
    | { kind: 'available'; hits: ProjectMapSearchHitV1[] };
  type CardView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'unavailable' }
    | { kind: 'available'; detail: ModuleCardDetailV1 };
  type RuntimeView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'unavailable' }
    | {
        kind: 'available';
        entrypoints: ModuleRuntimeRootV1[];
        tests: ModuleRuntimeRootV1[];
      };
  type EvidenceView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'unavailable'; message: string }
    | {
        kind: 'available';
        query: ModuleCardEvidenceQueryV1;
        detail: Extract<ModuleCardEvidenceResponseV1['result'], { status: 'available' }>['detail'];
      };
  type PreviewView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'unavailable'; message: string }
    | {
        kind: 'available';
        preview: Extract<
          ProjectMapSourcePreviewResponseV1['result'],
          { status: 'available' }
        >['preview'];
      };

  const PRESETS = {
    fast: { tokenLimit: 8_000, timeLimitMillis: 60_000, toolCallLimit: 16 },
    standard: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
    thorough: { tokenLimit: 128_000, timeLimitMillis: 600_000, toolCallLimit: 256 },
  } as const;

  let sceneView = $state<SceneView>({ kind: 'loading' });
  let overviewScene = $state<ProjectMapSceneV1 | null>(null);
  let selectedModuleId = $state<string | null>(null);
  let selectedSearchHit = $state<ProjectMapSearchHitV1 | null>(null);
  let searchText = $state('');
  let searchView = $state<SearchView>({ kind: 'idle' });
  let lensOpen = $state(false);
  let lensTasks = $state<TaskLensTasksResponseV1['result'] | null>(null);
  let lensTask = $state<TaskLensTaskResponseV1['result'] | null>(null);
  let lens = $state<
    Extract<TaskLensCompileResponseV1['result'], { status: 'available' }>['lens'] | null
  >(null);
  let selectedTaskId = $state('');
  let selectedStepId = $state('');
  let lensBusy = $state(false);
  let lensError = $state(false);
  let zoom = $state(1);
  let inspectorOpen = $state(false);
  let cardView = $state<CardView>({ kind: 'idle' });
  let runtimeView = $state<RuntimeView>({ kind: 'idle' });
  let evidenceView = $state<EvidenceView>({ kind: 'idle' });
  let previewView = $state<PreviewView>({ kind: 'idle' });
  let deepMap = $state<DeepMapStatusResponseV1['result'] | null>(null);
  let deepMapError = $state(false);
  let deepMapBusy = $state(false);
  let dockExpanded = $state(false);
  let selectedPreset = $state<'fast' | 'standard' | 'thorough' | 'advanced'>('standard');
  let customBudget = $state<DeepMapBudgetV1>({ ...PRESETS.standard });
  let sceneRequest = 0;
  let inspectorRequest = 0;

  const selectedModule = $derived(
    sceneView.kind === 'available'
      ? (sceneView.scene.modules.find((module) => module.moduleId === selectedModuleId) ?? null)
      : null,
  );
  const lensModuleIds = $derived.by(() => {
    const ids = new SvelteSet<string>();
    if (lens === null) return ids;
    for (const entry of lens.entries) {
      if (entry.target.kind === 'module') ids.add(entry.target.moduleId);
    }
    for (const claim of lens.claims) ids.add(claim.moduleId);
    return ids;
  });

  onMount(() => {
    let active = true;
    void loadScene(null);
    void loadDeepMap();
    const timer = window.setInterval(() => {
      if (active) void loadDeepMap(true);
    }, 1_500);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  });

  $effect(() => {
    void projectKey;
    selectedModuleId = null;
    selectedSearchHit = null;
    overviewScene = null;
    cardView = { kind: 'idle' };
    runtimeView = { kind: 'idle' };
    evidenceView = { kind: 'idle' };
    previewView = { kind: 'idle' };
  });

  async function loadScene(focusModuleId: string | null): Promise<void> {
    const request = ++sceneRequest;
    sceneView = { kind: 'loading' };
    try {
      const response = await sceneLoader({ focusModuleId });
      if (request !== sceneRequest) return;
      if (response.result.status === 'available') {
        sceneView = { kind: 'available', scene: response.result.scene };
        if (focusModuleId === null) overviewScene = response.result.scene;
        return;
      }
      const messages = {
        focusUnavailable: 'Das gewählte Modul gehört nicht mehr zur aktuellen Publikation.',
        noProject: 'Öffne ein Projekt, um den Architektur-Atlas zu verwenden.',
        noPublishedIndex: 'Der Atlas erscheint nach der ersten veröffentlichten Indexierung.',
        projectionUnavailable: 'Die aktuelle Map-Projektion ist noch nicht verfügbar.',
      } as const;
      sceneView = { kind: 'empty', message: messages[response.result.status] };
    } catch {
      if (request === sceneRequest) sceneView = { kind: 'error' };
    }
  }

  async function selectModule(module: ProjectMapSceneModuleV1): Promise<void> {
    selectedModuleId = module.moduleId;
    selectedSearchHit = null;
    inspectorOpen = true;
    evidenceView = { kind: 'idle' };
    previewView = { kind: 'idle' };
    const request = ++inspectorRequest;
    cardView = { kind: 'loading' };
    runtimeView = { kind: 'loading' };
    const [card, runtime] = await Promise.allSettled([
      cardLoader({ moduleId: module.moduleId }),
      runtimeLoader({ entrypointLimit: 20, moduleId: module.moduleId, testLimit: 20 }),
    ]);
    if (request !== inspectorRequest) return;
    if (card.status === 'fulfilled') {
      cardView =
        card.value.result.status === 'available'
          ? { kind: 'available', detail: card.value.result.detail }
          : { kind: 'unavailable' };
    } else cardView = { kind: 'error' };
    if (runtime.status === 'fulfilled') {
      runtimeView =
        runtime.value.result.status === 'available'
          ? {
              entrypoints: runtime.value.result.map.entrypoints.roots,
              kind: 'available',
              tests: runtime.value.result.map.tests.roots,
            }
          : { kind: 'unavailable' };
    } else runtimeView = { kind: 'error' };
    if (sceneView.kind === 'available' && sceneView.scene.focusModuleId !== module.moduleId) {
      void loadScene(module.moduleId);
    }
  }

  async function showOverview(): Promise<void> {
    selectedModuleId = null;
    inspectorOpen = false;
    if (overviewScene !== null) sceneView = { kind: 'available', scene: overviewScene };
    else await loadScene(null);
  }

  async function submitSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const query = searchText.trim();
    if (query.length < 3) return;
    searchView = { kind: 'loading' };
    try {
      const response = await searchLoader({ query });
      if (response.result.status !== 'available' || response.result.search.hits.length === 0) {
        searchView = { kind: 'empty' };
      } else {
        searchView = { kind: 'available', hits: response.result.search.hits };
      }
    } catch {
      searchView = { kind: 'error' };
    }
  }

  async function inspectSearchHit(hit: ProjectMapSearchHitV1): Promise<void> {
    selectedSearchHit = hit;
    selectedModuleId = hit.moduleId;
    inspectorOpen = true;
    if (hit.moduleId !== null) {
      const module =
        overviewScene?.modules.find((candidate) => candidate.moduleId === hit.moduleId) ?? null;
      if (module !== null) {
        await selectModule(module);
        selectedSearchHit = hit;
      } else {
        await loadScene(hit.moduleId);
      }
    }
  }

  async function toggleLens(): Promise<void> {
    lensOpen = !lensOpen;
    if (!lensOpen || lensTasks !== null) return;
    lensBusy = true;
    lensError = false;
    try {
      lensTasks = (await taskLensTasksLoader()).result;
    } catch {
      lensError = true;
    } finally {
      lensBusy = false;
    }
  }

  async function selectTask(event: Event): Promise<void> {
    selectedTaskId = (event.currentTarget as HTMLSelectElement).value;
    selectedStepId = '';
    lensTask = null;
    lens = null;
    if (selectedTaskId === '') return;
    lensBusy = true;
    lensError = false;
    try {
      lensTask = (await taskLensTaskLoader({ taskId: selectedTaskId })).result;
    } catch {
      lensError = true;
    } finally {
      lensBusy = false;
    }
  }

  async function applyLens(): Promise<void> {
    if (selectedTaskId === '' || selectedStepId === '') return;
    lensBusy = true;
    lensError = false;
    try {
      const response = await taskLensCompiler({ stepId: selectedStepId, taskId: selectedTaskId });
      lens = response.result.status === 'available' ? response.result.lens : null;
      lensOpen = false;
    } catch {
      lensError = true;
    } finally {
      lensBusy = false;
    }
  }

  function clearLens(): void {
    lens = null;
    selectedTaskId = '';
    selectedStepId = '';
    lensTask = null;
  }

  function evidenceQuery(
    detail: ModuleCardDetailV1,
    evidenceId: string,
  ): ModuleCardEvidenceQueryV1 {
    return {
      cardId: detail.cardId,
      currentIndexRunId: detail.currentIndexRunId,
      currentSnapshotId: detail.currentSnapshotId,
      evidenceId,
      moduleId: detail.moduleId,
      sourceIndexRunId: detail.sourceIndexRunId,
      sourceSnapshotId: detail.sourceSnapshotId,
    };
  }

  async function inspectEvidence(evidenceId: string): Promise<void> {
    if (cardView.kind !== 'available') return;
    const query = evidenceQuery(cardView.detail, evidenceId);
    evidenceView = { kind: 'loading' };
    previewView = { kind: 'idle' };
    try {
      const response = await evidenceLoader(query);
      evidenceView =
        response.result.status === 'available'
          ? { detail: response.result.detail, kind: 'available', query }
          : { kind: 'unavailable', message: 'Diese Evidence ist nicht mehr aktuell verfügbar.' };
    } catch {
      evidenceView = { kind: 'error' };
    }
  }

  async function openSourcePreview(): Promise<void> {
    if (evidenceView.kind !== 'available') return;
    previewView = { kind: 'loading' };
    try {
      const response = await sourcePreviewLoader(evidenceView.query);
      previewView =
        response.result.status === 'available'
          ? { kind: 'available', preview: response.result.preview }
          : {
              kind: 'unavailable',
              message:
                response.result.status === 'staleEvidence'
                  ? 'Veraltete Evidence wird nicht als Source angezeigt.'
                  : 'Der Ausschnitt ist für diese aktuelle Auswahl nicht verfügbar.',
            };
    } catch {
      previewView = { kind: 'error' };
    }
  }

  async function loadDeepMap(silent = false): Promise<void> {
    if (!silent) deepMapError = false;
    try {
      deepMap = (await deepMapStatusLoader()).result;
    } catch {
      if (!silent) deepMapError = true;
    }
  }

  function chosenBudget(): DeepMapBudgetV1 {
    return selectedPreset === 'advanced' ? customBudget : { ...PRESETS[selectedPreset] };
  }

  async function startMapping(): Promise<void> {
    deepMapBusy = true;
    try {
      await deepMapStarter(chosenBudget());
      await loadDeepMap();
      dockExpanded = true;
    } catch {
      deepMapError = true;
    } finally {
      deepMapBusy = false;
    }
  }

  async function controlMapping(action: () => Promise<DeepMapControlResponseV1>): Promise<void> {
    deepMapBusy = true;
    try {
      await action();
      await loadDeepMap();
    } catch {
      deepMapError = true;
    } finally {
      deepMapBusy = false;
    }
  }

  function modulePosition(rank: number, total: number): { x: number; y: number } {
    const columns = Math.max(1, Math.ceil(Math.sqrt(total)));
    const rows = Math.max(1, Math.ceil(total / columns));
    const index = rank - 1;
    return {
      x: ((index % columns) + 0.5) * (1000 / columns),
      y: (Math.floor(index / columns) + 0.5) * (700 / rows),
    };
  }

  function routePath(scene: ProjectMapSceneV1, sourceId: string, targetId: string): string {
    const source = scene.modules.find((module) => module.moduleId === sourceId);
    const target = scene.modules.find((module) => module.moduleId === targetId);
    if (source === undefined || target === undefined) return '';
    const start = modulePosition(source.rank, scene.modules.length);
    const end = modulePosition(target.rank, scene.modules.length);
    const bend = Math.max(20, Math.abs(end.x - start.x) * 0.2);
    return `M ${start.x} ${start.y} C ${start.x + bend} ${start.y}, ${end.x - bend} ${end.y}, ${end.x} ${end.y}`;
  }

  function statusLabel(status: ProjectMapSceneModuleV1['mappingStatus']): string {
    return {
      current: 'Current',
      needsReview: 'Needs review',
      stale: 'Stale',
      unmapped: 'Nicht gemappt',
    }[status];
  }

  function fieldLabel(field: ModuleCardFieldKindV1): string {
    return {
      dataFlows: 'Datenflüsse',
      dependencies: 'Abhängigkeiten',
      entrypoints: 'Entry Points',
      invariants: 'Invarianten',
      openQuestions: 'Offene Fragen',
      paths: 'Pfade',
      publicSurface: 'Öffentliche API',
      purpose: 'Zweck',
      responsibilities: 'Verantwortung',
      risks: 'Risiken',
      tests: 'Tests',
      title: 'Titel',
    }[field];
  }

  function targetLabel(hit: ProjectMapSearchHitV1): string {
    return hit.target.kind === 'file'
      ? hit.target.evidence.pathDisplay
      : hit.target.qualifiedName || hit.target.name;
  }

  function deepMapStateLabel(state: string): string {
    return (
      {
        cancelled: 'Abgebrochen',
        cancelling: 'Wird abgebrochen',
        failed: 'Fehlgeschlagen',
        idle: 'Bereit',
        paused: 'Pausiert',
        pausing: 'Wird pausiert',
        queued: 'Eingeplant',
        running: 'Mapping läuft',
        succeeded: 'Veröffentlicht',
      }[state] ?? state
    );
  }

  function formatSeconds(milliseconds: number): string {
    return `${Math.round(milliseconds / 1_000)} s`;
  }

  function phaseLabel(phase: string | null): string {
    if (phase === null) return 'Noch nicht gestartet';
    return (
      {
        claiming: 'Claims erzeugen',
        exploring: 'Evidence erkunden',
        planning: 'Planung',
        publishing: 'Veröffentlichen',
        verifying: 'Verifizieren',
      }[phase] ?? phase
    );
  }

  function actionLabel(action: string | null): string {
    if (action === null) return '–';
    return (
      {
        buildPlan: 'Plan erstellen',
        generateClaims: 'Claims strukturieren',
        inspect: 'Evidence lesen',
        propose: 'Schritt bestätigen',
        publishCards: 'Cards publizieren',
        search: 'Index durchsuchen',
        verifyEvidence: 'Evidence prüfen',
      }[action] ?? action
    );
  }

  function moduleLabel(moduleId: string | null): string {
    if (moduleId === null) return 'Gesamtprojekt';
    return (
      overviewScene?.modules.find((module) => module.moduleId === moduleId)?.displayName ??
      `Modul ${moduleId.slice(0, 8)}`
    );
  }
</script>

<section class="map-shell" aria-labelledby="map-workspace-title" data-project-key={projectKey}>
  <header class="command-bar">
    <div class="map-title">
      <span class="map-mark" aria-hidden="true">A³</span>
      <div>
        <h2 id="map-workspace-title">Code Atlas</h2>
        <p>Architektur, Code-Evidence und Mapping in einer Ansicht</p>
      </div>
    </div>
    <form class="map-search" role="search" onsubmit={submitSearch}>
      <label class="sr-only" for="atlas-search">Code durchsuchen</label>
      <span aria-hidden="true">⌕</span>
      <input
        id="atlas-search"
        type="search"
        autocomplete="off"
        maxlength="4096"
        placeholder="Datei, Symbol oder Signatur suchen …"
        bind:value={searchText}
      />
      <button
        type="submit"
        disabled={searchText.trim().length < 3 || searchView.kind === 'loading'}
      >
        {searchView.kind === 'loading' ? 'Sucht …' : 'Suchen'}
      </button>
    </form>
    <div class="lens-control">
      <button
        class:active={lens !== null}
        type="button"
        aria-expanded={lensOpen}
        aria-controls="task-lens-picker"
        onclick={toggleLens}
      >
        <span aria-hidden="true">◎</span>
        {lens === null ? 'Task Lens' : 'Lens aktiv'}
      </button>
      {#if lens !== null}
        <button
          class="icon-button"
          type="button"
          aria-label="Task Lens entfernen"
          onclick={clearLens}>×</button
        >
      {/if}
      {#if lensOpen}
        <section id="task-lens-picker" class="lens-popover" aria-label="Task Lens auswählen">
          <strong>Aktuellen Arbeitsschritt fokussieren</strong>
          {#if lensBusy}
            <p role="status">Task Lens wird geladen …</p>
          {:else if lensError}
            <p role="alert">Task Lens ist gerade nicht verfügbar.</p>
          {:else if lensTasks?.status === 'available'}
            <label for="lens-task">Task</label>
            <select id="lens-task" value={selectedTaskId} onchange={selectTask}>
              <option value="">Task wählen</option>
              {#each lensTasks.tasks as task (task.taskId)}
                <option value={task.taskId}>{task.objective}</option>
              {/each}
            </select>
            {#if lensTask?.status === 'available'}
              <label for="lens-step">Schritt</label>
              <select id="lens-step" bind:value={selectedStepId}>
                <option value="">Schritt wählen</option>
                {#each lensTask.steps as step (step.stepId)}
                  <option value={step.stepId}>{step.intendedOutcome}</option>
                {/each}
              </select>
              <button type="button" disabled={selectedStepId === ''} onclick={applyLens}
                >Anwenden</button
              >
            {:else if selectedTaskId !== ''}
              <p>Für diesen Task ist kein aktueller Schritt verfügbar.</p>
            {/if}
          {:else}
            <p>Keine laufenden Tasks vorhanden.</p>
          {/if}
        </section>
      {/if}
    </div>
  </header>

  <div class="map-body" class:inspector-visible={inspectorOpen}>
    <main class="atlas-panel">
      <div class="atlas-toolbar">
        <div>
          <strong
            >{sceneView.kind === 'available'
              ? `${sceneView.scene.modules.length} Regionen`
              : 'Architektur-Atlas'}</strong
          >
          {#if sceneView.kind === 'available'}
            <span>
              {sceneView.scene.relations.length} Routen · {sceneView.scene.unmappedEdgeCount}
              unzugeordnet
            </span>
          {/if}
        </div>
        <div class="atlas-controls" aria-label="Kartensteuerung">
          {#if sceneView.kind === 'available' && sceneView.scene.focusModuleId !== null}
            <button type="button" onclick={showOverview}>← Übersicht</button>
          {/if}
          <button
            class="icon-button"
            type="button"
            aria-label="Herauszoomen"
            onclick={() => (zoom = Math.max(0.75, zoom - 0.25))}>−</button
          >
          <output aria-label="Zoomstufe">{Math.round(zoom * 100)} %</output>
          <button
            class="icon-button"
            type="button"
            aria-label="Hineinzoomen"
            onclick={() => (zoom = Math.min(1.75, zoom + 0.25))}>+</button
          >
          <button type="button" onclick={() => (zoom = 1)}>Einpassen</button>
        </div>
      </div>

      {#if searchView.kind !== 'idle'}
        <aside class="search-results" aria-label="Suchergebnisse">
          <div>
            <strong>Suchergebnisse</strong>
            <button
              class="icon-button"
              type="button"
              aria-label="Suchergebnisse schließen"
              onclick={() => (searchView = { kind: 'idle' })}>×</button
            >
          </div>
          {#if searchView.kind === 'available'}
            <ul>
              {#each searchView.hits as hit (hit.rank)}
                <li>
                  <button type="button" onclick={() => inspectSearchHit(hit)}>
                    <span>{hit.target.kind === 'file' ? 'Datei' : hit.target.symbolKind}</span>
                    <strong>{targetLabel(hit)}</strong>
                    <small>{hit.target.evidence.pathDisplay}</small>
                  </button>
                </li>
              {/each}
            </ul>
          {:else if searchView.kind === 'loading'}
            <p role="status">Der aktuelle Snapshot wird durchsucht …</p>
          {:else if searchView.kind === 'empty'}
            <p>Keine Treffer im veröffentlichten Snapshot.</p>
          {:else}
            <p role="alert">Die Suche konnte nicht sicher ausgeführt werden.</p>
          {/if}
        </aside>
      {/if}

      <div class="atlas-viewport" aria-label="Zoombarer Architektur-Atlas">
        {#if sceneView.kind === 'loading'}
          <div class="atlas-empty" role="status">
            <span class="loader"></span>Atlas wird aufgebaut …
          </div>
        {:else if sceneView.kind === 'error'}
          <div class="atlas-empty" role="alert">
            <p>Der Architektur-Atlas konnte nicht sicher geladen werden.</p>
            <button type="button" onclick={() => loadScene(null)}>Erneut laden</button>
          </div>
        {:else if sceneView.kind === 'empty'}
          <div class="atlas-empty"><p>{sceneView.message}</p></div>
        {:else}
          <div
            class="atlas-canvas"
            style={`width:${zoom * 100}%;height:${zoom * 100}%;min-width:${zoom * 900}px;min-height:${zoom * 620}px`}
          >
            <svg
              class="route-layer"
              viewBox="0 0 1000 700"
              aria-hidden="true"
              preserveAspectRatio="none"
            >
              <defs>
                <marker
                  id="route-arrow"
                  viewBox="0 0 10 10"
                  refX="8"
                  refY="5"
                  markerWidth="5"
                  markerHeight="5"
                  orient="auto-start-reverse"
                >
                  <path d="M 0 0 L 10 5 L 0 10 z"></path>
                </marker>
              </defs>
              {#each sceneView.scene.relations as relation (`${relation.sourceModuleId}:${relation.targetModuleId}:${relation.relation}`)}
                <path
                  class={`route route-${relation.relation}`}
                  d={routePath(sceneView.scene, relation.sourceModuleId, relation.targetModuleId)}
                  marker-end="url(#route-arrow)"
                ></path>
              {/each}
            </svg>
            {#each sceneView.scene.modules as module (module.moduleId)}
              {@const position = modulePosition(module.rank, sceneView.scene.modules.length)}
              <button
                type="button"
                class="module-region"
                class:selected={selectedModuleId === module.moduleId}
                class:lens-muted={lens !== null && !lensModuleIds.has(module.moduleId)}
                class:lens-match={lensModuleIds.has(module.moduleId)}
                data-status={module.mappingStatus}
                style={`left:${position.x / 10}%;top:${position.y / 7}%`}
                aria-label={`${module.displayName}, ${statusLabel(module.mappingStatus)}, ${module.fileCount} Dateien, ${module.symbolCount} Symbole`}
                onclick={() => selectModule(module)}
              >
                <span class="region-kind"
                  >{module.kind === 'manifestBoundary' ? 'Package' : 'Modul'}</span
                >
                <strong>{module.displayName}</strong>
                <span class="region-counts"
                  >{module.fileCount} Dateien · {module.symbolCount} Symbole</span
                >
                <span class="region-status"
                  ><i aria-hidden="true"></i>{statusLabel(module.mappingStatus)}</span
                >
              </button>
            {/each}
          </div>
          <details class="atlas-summary">
            <summary>Nichtgrafische Zusammenfassung</summary>
            <ul>
              {#each sceneView.scene.modules as module (module.moduleId)}
                <li>
                  <button type="button" onclick={() => selectModule(module)}>
                    {module.displayName}: {statusLabel(module.mappingStatus)}, {module.entrypointCount}
                    Entry Points, {module.testCount} Tests
                  </button>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    </main>

    <aside class="inspector" class:open={inspectorOpen} aria-label="Code Inspector">
      <header>
        <div>
          <span>Inspector</span>
          <h3>
            {selectedModule?.displayName ??
              (selectedSearchHit === null ? 'Auswahl' : targetLabel(selectedSearchHit))}
          </h3>
        </div>
        <button
          class="icon-button"
          type="button"
          aria-label="Inspector schließen"
          onclick={() => (inspectorOpen = false)}>×</button
        >
      </header>
      {#if selectedSearchHit !== null}
        <section class="inspector-section">
          <span class="eyebrow">Suchtreffer · Rang {selectedSearchHit.rank}</span>
          <h4>{targetLabel(selectedSearchHit)}</h4>
          <dl>
            <div>
              <dt>Art</dt>
              <dd>{selectedSearchHit.target.kind}</dd>
            </div>
            <div>
              <dt>Pfad</dt>
              <dd>{selectedSearchHit.target.evidence.pathDisplay}</dd>
            </div>
            <div>
              <dt>Atlas-Region</dt>
              <dd>
                {selectedSearchHit.moduleId === null
                  ? 'Nicht eindeutig zuordenbar'
                  : moduleLabel(selectedSearchHit.moduleId)}
              </dd>
            </div>
            {#if selectedSearchHit.target.kind === 'symbol'}
              <div>
                <dt>Signatur</dt>
                <dd>{selectedSearchHit.target.signature ?? 'Nicht erfasst'}</dd>
              </div>
            {/if}
          </dl>
          <p class="inspector-note">
            Der Treffer bleibt nutzbar, auch wenn keine belastbare Modulbindung vorliegt.
          </p>
        </section>
      {:else if selectedModule !== null}
        <section class="module-overview">
          <div class={`status-badge ${selectedModule.mappingStatus}`}>
            {statusLabel(selectedModule.mappingStatus)}
          </div>
          <div class="metric-grid">
            <div><strong>{selectedModule.fileCount}</strong><span>Dateien</span></div>
            <div><strong>{selectedModule.symbolCount}</strong><span>Symbole</span></div>
            <div><strong>{selectedModule.entrypointCount}</strong><span>Entry Points</span></div>
            <div><strong>{selectedModule.testCount}</strong><span>Tests</span></div>
          </div>
          {#if selectedModule.cardCoverageBasisPoints !== null}
            <div class="coverage">
              <span>Card Coverage</span><strong
                >{Math.round(selectedModule.cardCoverageBasisPoints / 100)} %</strong
              >
              <i><b style={`width:${selectedModule.cardCoverageBasisPoints / 100}%`}></b></i>
            </div>
          {/if}
        </section>

        <details class="inspector-section" open>
          <summary>Verständnis & Claims</summary>
          {#if cardView.kind === 'loading'}
            <p role="status">Module Card wird geladen …</p>
          {:else if cardView.kind === 'available'}
            <div class="confidence-row">
              <span>Confidence</span><strong
                >{Math.round(cardView.detail.confidenceBasisPoints / 100)} %</strong
              >
              <span>{cardView.detail.lifecycle.status}</span>
            </div>
            {#each cardView.detail.fields as field (field.kind)}
              <article class="claim-group">
                <h5>{fieldLabel(field.kind)}</h5>
                {#each field.values as item (item.claim.claimId)}
                  <div class="claim">
                    <p>{item.value}</p>
                    <div>
                      <span
                        >{item.claim.kind} · {Math.round(item.claim.confidenceBasisPoints / 100)} %</span
                      >
                      {#each item.claim.evidenceIds as evidenceId (evidenceId)}
                        <button type="button" onclick={() => inspectEvidence(evidenceId)}
                          >Evidence öffnen</button
                        >
                      {/each}
                    </div>
                  </div>
                {/each}
              </article>
            {/each}
          {:else if cardView.kind === 'unavailable'}
            <p>Für dieses Modul ist noch keine veröffentlichte Card verfügbar.</p>
          {:else if cardView.kind === 'error'}
            <p role="alert">Die Card konnte nicht sicher geladen werden.</p>
          {/if}
        </details>

        <details class="inspector-section">
          <summary>Code-Landmarks</summary>
          {#if runtimeView.kind === 'loading'}
            <p role="status">Entry Points und Tests werden geladen …</p>
          {:else if runtimeView.kind === 'available'}
            <h5>Entry Points</h5>
            <ul class="landmark-list">
              {#each runtimeView.entrypoints as root (root.symbol.symbolId)}
                <li><span>{root.symbol.symbolKind}</span><strong>{root.symbol.name}</strong></li>
              {:else}
                <li>Keine Entry Points erkannt.</li>
              {/each}
            </ul>
            <h5>Tests</h5>
            <ul class="landmark-list">
              {#each runtimeView.tests as root (root.symbol.symbolId)}
                <li><span>{root.symbol.symbolKind}</span><strong>{root.symbol.name}</strong></li>
              {:else}
                <li>Keine Tests erkannt.</li>
              {/each}
            </ul>
          {:else if runtimeView.kind === 'unavailable'}
            <p>Für das Modul sind keine Runtime-Landmarks verfügbar.</p>
          {:else if runtimeView.kind === 'error'}
            <p role="alert">Landmarks konnten nicht geladen werden.</p>
          {/if}
        </details>

        <details class="inspector-section">
          <summary>Direkte Routen</summary>
          {#if sceneView.kind === 'available'}
            <ul class="relation-list">
              {#each sceneView.scene.relations.filter((relation) => relation.sourceModuleId === selectedModule.moduleId || relation.targetModuleId === selectedModule.moduleId) as relation (`${relation.sourceModuleId}:${relation.targetModuleId}:${relation.relation}`)}
                <li>
                  <span>{relation.relation}</span><strong
                    >{relation.observedEvidenceCount} Evidence</strong
                  >
                </li>
              {:else}
                <li>Keine direkten Routen in der aktuellen Ansicht.</li>
              {/each}
            </ul>
          {/if}
        </details>

        {#if evidenceView.kind !== 'idle'}
          <section class="evidence-panel" aria-live="polite">
            <span class="eyebrow">Evidence</span>
            {#if evidenceView.kind === 'loading'}
              <p>Evidence wird revalidiert …</p>
            {:else if evidenceView.kind === 'available'}
              <h4>{evidenceView.detail.payload.kind}</h4>
              <p>Freshness: {evidenceView.detail.freshness}</p>
              <button
                type="button"
                disabled={evidenceView.detail.freshness !== 'current'}
                onclick={openSourcePreview}>Begrenzten Codeausschnitt anzeigen</button
              >
            {:else if evidenceView.kind === 'unavailable'}
              <p>{evidenceView.message}</p>
            {:else}
              <p role="alert">Evidence konnte nicht sicher geöffnet werden.</p>
            {/if}
          </section>
        {/if}

        {#if previewView.kind !== 'idle'}
          <section class="source-preview" aria-label="Sicherer Codeausschnitt">
            {#if previewView.kind === 'loading'}
              <p role="status">Source wird erneut geprüft …</p>
            {:else if previewView.kind === 'available'}
              <header>
                <strong>{previewView.preview.pathDisplay}</strong><span
                  >{previewView.preview.lineCount} Zeilen</span
                >
              </header>
              <pre><code
                  >{#each previewView.preview.text.split('\n') as line, index (index)}<span
                      ><i>{previewView.preview.startLine + index}</i>{line || ' '}</span
                    >{/each}</code
                ></pre>
            {:else if previewView.kind === 'unavailable'}
              <p>{previewView.message}</p>
            {:else}
              <p role="alert">Der Source-Ausschnitt konnte nicht sicher gelesen werden.</p>
            {/if}
          </section>
        {/if}
      {:else}
        <div class="inspector-empty">
          <span aria-hidden="true">⌁</span>
          <h3>Region auswählen</h3>
          <p>
            Öffne ein Modul oder einen Suchtreffer, um belastbare Details und Evidence zu sehen.
          </p>
        </div>
      {/if}
    </aside>
  </div>

  <section class="deep-map-dock" class:expanded={dockExpanded} aria-labelledby="deep-map-title">
    <button
      class="dock-summary"
      type="button"
      aria-expanded={dockExpanded}
      onclick={() => (dockExpanded = !dockExpanded)}
    >
      <span class="deep-map-icon" aria-hidden="true">✦</span>
      <span
        ><strong id="deep-map-title">Deep Map</strong><small
          >Verifiziertes Repository-Verständnis</small
        ></span
      >
      {#if deepMap?.status === 'available'}
        <span class={`run-state ${deepMap.activity.state}`}
          >{deepMapStateLabel(deepMap.activity.state)}</span
        >
        <span class="dock-progress">
          <i
            ><b
              style={`width:${deepMap.activity.totalSteps === '0' ? 0 : Math.min(100, (Number(deepMap.activity.confirmedSteps) / Number(deepMap.activity.totalSteps)) * 100)}%`}
            ></b></i
          >
          <small>{deepMap.activity.confirmedSteps}/{deepMap.activity.totalSteps} bestätigt</small>
        </span>
      {/if}
      <span aria-hidden="true">{dockExpanded ? '⌄' : '⌃'}</span>
    </button>
    {#if dockExpanded}
      <div class="dock-content">
        {#if deepMapError}
          <p role="alert">
            Der Deep-Map-Status oder die Aktion konnte nicht sicher verarbeitet werden.
          </p>
        {:else if deepMap?.status === 'unavailable'}
          <p>Ein verifiziertes lokales Mapping-Modell ist noch nicht konfiguriert.</p>
        {:else if deepMap?.status === 'available'}
          <div class="preset-panel">
            <div class="preset-grid" role="radiogroup" aria-label="Mapping-Budget">
              {#each [['fast', 'Schnell', PRESETS.fast], ['standard', 'Standard', PRESETS.standard], ['thorough', 'Gründlich', PRESETS.thorough]] as preset (preset[0])}
                <button
                  type="button"
                  role="radio"
                  aria-checked={selectedPreset === preset[0]}
                  class:active={selectedPreset === preset[0]}
                  onclick={() => (selectedPreset = preset[0] as 'fast' | 'standard' | 'thorough')}
                >
                  <strong>{preset[1]}</strong>
                  <span
                    >{(preset[2] as DeepMapBudgetV1).tokenLimit.toLocaleString('de-DE')} Tokens</span
                  >
                  <small
                    >{formatSeconds((preset[2] as DeepMapBudgetV1).timeLimitMillis)} · {(
                      preset[2] as DeepMapBudgetV1
                    ).toolCallLimit} Reads</small
                  >
                </button>
              {/each}
              <button
                type="button"
                role="radio"
                aria-checked={selectedPreset === 'advanced'}
                class:active={selectedPreset === 'advanced'}
                onclick={() => (selectedPreset = 'advanced')}
              >
                <strong>Erweitert</strong><span>Eigene Grenzen</span><small
                  >Validierte Min-/Max-Werte</small
                >
              </button>
            </div>
            {#if selectedPreset === 'advanced'}
              <div class="advanced-budget">
                <label
                  >Tokens <input
                    type="number"
                    min={deepMap.configuration.minimumBudget.tokenLimit}
                    max={deepMap.configuration.maximumBudget.tokenLimit}
                    bind:value={customBudget.tokenLimit}
                  /></label
                >
                <label
                  >Sekunden <input
                    type="number"
                    min="1"
                    max="86400"
                    value={Math.round(customBudget.timeLimitMillis / 1000)}
                    onchange={(event) =>
                      (customBudget.timeLimitMillis =
                        Number((event.currentTarget as HTMLInputElement).value) * 1000)}
                  /></label
                >
                <label
                  >Reads <input
                    type="number"
                    min={deepMap.configuration.minimumBudget.toolCallLimit}
                    max={deepMap.configuration.maximumBudget.toolCallLimit}
                    bind:value={customBudget.toolCallLimit}
                  /></label
                >
              </div>
            {/if}
          </div>
          <div class="run-panel">
            <div class="run-facts">
              <div>
                <span>Status</span><strong>{deepMapStateLabel(deepMap.activity.state)}</strong>
              </div>
              <div>
                <span>Planbudget</span><strong
                  >{deepMap.activity.budget?.tokenLimit.toLocaleString('de-DE') ??
                    chosenBudget().tokenLimit.toLocaleString('de-DE')} Tokens</strong
                >
              </div>
              <div>
                <span>Fortschritt</span><strong
                  >{deepMap.activity.progress === null
                    ? 'Noch keine Phase'
                    : `${deepMap.activity.progress.completed}/${deepMap.activity.progress.total}`}</strong
                >
              </div>
              <div><span>Phase</span><strong>{phaseLabel(deepMap.activity.phase)}</strong></div>
              <div>
                <span>Aktuelles Modul</span><strong
                  >{moduleLabel(deepMap.activity.currentModuleId)}</strong
                >
              </div>
              <div>
                <span>Aktion</span><strong>{actionLabel(deepMap.activity.safeAction)}</strong>
              </div>
            </div>
            {#if deepMap.activity.events.length > 0}
              <ol class="activity-feed" aria-label="Aktuelle sichere Deep-Map-Ereignisse">
                {#each deepMap.activity.events.slice().reverse() as event (event.sequence)}
                  <li class:confirmed={event.confirmed}>
                    <span>{event.sequence}</span>
                    <div>
                      <strong>{phaseLabel(event.phase)}</strong>
                      <small>
                        {moduleLabel(event.currentModuleId)} · {actionLabel(
                          event.safeAction,
                        )}{event.stepPosition === null
                          ? ''
                          : ` · Schritt ${event.stepPosition}/${event.totalSteps}`}
                      </small>
                    </div>
                    <i>{event.confirmed ? 'Bestätigt' : 'Aktiv'}</i>
                  </li>
                {/each}
              </ol>
            {:else}
              <p class="feed-empty">
                Nach dem Start erscheinen hier ausschließlich sichere Phasen und bestätigte Schritte
                – keine Prompts oder Modellbegründungen.
              </p>
            {/if}
            <div class="run-actions">
              <button
                class="primary"
                type="button"
                disabled={deepMapBusy ||
                  !['idle', 'succeeded', 'failed', 'cancelled'].includes(deepMap.activity.state)}
                onclick={startMapping}>Deep Map starten</button
              >
              <button
                type="button"
                disabled={deepMapBusy || deepMap.activity.state !== 'running'}
                onclick={() => controlMapping(deepMapPauser)}>Pausieren</button
              >
              <button
                type="button"
                disabled={deepMapBusy || deepMap.activity.state !== 'paused'}
                onclick={() => controlMapping(deepMapResumer)}>Fortsetzen</button
              >
              <button
                class="danger"
                type="button"
                disabled={deepMapBusy ||
                  !['queued', 'running', 'pausing', 'paused', 'cancelling'].includes(
                    deepMap.activity.state,
                  )}
                onclick={() => controlMapping(deepMapCanceller)}>Abbrechen</button
              >
            </div>
          </div>
        {:else}
          <p role="status">Deep Map wird geladen …</p>
        {/if}
      </div>
    {/if}
  </section>
</section>

<style>
  :global(*) {
    box-sizing: border-box;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  .map-shell {
    --map-bg: color-mix(in srgb, var(--color-canvas-deep) 94%, var(--color-info) 6%);
    --line: color-mix(in srgb, currentColor 14%, transparent);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-height: min(780px, calc(100vh - 9.5rem));
    height: calc(100vh - 9.5rem);
    width: 100%;
    overflow: hidden;
    color: var(--color-text);
    background: var(--map-bg);
    border: 1px solid var(--line);
    border-radius: 18px;
  }
  button,
  input,
  select {
    font: inherit;
  }
  button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 10px;
    color: inherit;
    background: color-mix(in srgb, var(--map-bg) 84%, var(--color-surface-raised) 5%);
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button.active {
    border-color: color-mix(in srgb, var(--color-accent) 65%, currentColor);
    background: color-mix(in srgb, var(--color-accent) 17%, var(--map-bg));
  }
  button:focus-visible,
  input:focus-visible,
  select:focus-visible,
  summary:focus-visible {
    outline: var(--focus-width) solid var(--color-focus);
    outline-offset: 2px;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.48;
  }
  .command-bar {
    z-index: 8;
    display: grid;
    grid-template-columns: minmax(180px, 0.6fr) minmax(280px, 1.4fr) auto;
    gap: 16px;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--map-bg) 94%, transparent);
    backdrop-filter: blur(18px);
  }
  .map-title {
    display: flex;
    gap: 11px;
    align-items: center;
    min-width: 0;
  }
  .map-mark {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    flex: 0 0 auto;
    border-radius: 12px;
    color: var(--color-info);
    background: color-mix(in srgb, var(--color-info) 17%, transparent);
    font-weight: 800;
  }
  .map-title h2,
  .map-title p {
    margin: 0;
  }
  .map-title h2 {
    font-size: 1rem;
  }
  .map-title p {
    overflow: hidden;
    color: var(--color-muted);
    font-size: 0.76rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .map-search {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 4px 4px 13px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: color-mix(in srgb, var(--map-bg) 80%, var(--color-overlay) 8%);
  }
  .map-search:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-focus) 24%, transparent);
  }
  .map-search input {
    min-width: 0;
    min-height: var(--control-min-size);
    color: inherit;
    border: 0;
    outline: 0;
    background: transparent;
  }
  .map-search button {
    min-height: var(--control-min-size);
    padding-inline: 16px;
    color: var(--color-on-accent);
    background: var(--color-accent);
    font-weight: 750;
  }
  .lens-control {
    position: relative;
    display: flex;
    gap: 6px;
  }
  .lens-control > button {
    padding-inline: 14px;
  }
  .icon-button {
    width: 44px;
    padding: 0;
    font-size: 1.15rem;
  }
  .lens-popover {
    position: absolute;
    z-index: 30;
    top: calc(100% + 10px);
    right: 0;
    display: grid;
    gap: 8px;
    width: min(360px, calc(100vw - 32px));
    padding: 16px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: color-mix(in srgb, var(--map-bg) 96%, var(--color-overlay));
    box-shadow: 0 18px 50px color-mix(in srgb, var(--color-shadow) 53%, transparent);
  }
  .lens-popover p {
    margin: 0;
    color: var(--color-muted);
  }
  .lens-popover label {
    font-size: 0.75rem;
    color: var(--color-muted);
  }
  .lens-popover select {
    min-height: 44px;
    width: 100%;
    padding-inline: 10px;
    color: inherit;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--map-bg);
  }
  .map-body {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 0;
    transition: grid-template-columns 0.2s ease;
  }
  .map-body.inspector-visible {
    grid-template-columns: minmax(0, 1fr) minmax(300px, 380px);
  }
  .atlas-panel {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .atlas-toolbar {
    z-index: 5;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: center;
    min-height: 54px;
    padding: 7px 14px;
    border-bottom: 1px solid var(--line);
  }
  .atlas-toolbar > div:first-child {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  .atlas-toolbar span,
  .atlas-controls output {
    color: var(--color-muted);
    font-size: 0.76rem;
  }
  .atlas-controls {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .atlas-controls button {
    min-height: var(--control-min-size);
  }
  .atlas-controls output {
    min-width: 48px;
    text-align: center;
  }
  .atlas-viewport {
    position: relative;
    min-height: 0;
    overflow: auto;
    background-image: radial-gradient(
      circle,
      color-mix(in srgb, currentColor 13%, transparent) 1px,
      transparent 1px
    );
    background-size: 25px 25px;
    scrollbar-color: color-mix(in srgb, currentColor 22%, transparent) transparent;
  }
  .atlas-canvas {
    position: relative;
    min-width: 900px;
    min-height: 620px;
    transform-origin: top left;
  }
  .route-layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .route {
    fill: none;
    stroke: color-mix(in srgb, var(--color-info) 44%, transparent);
    stroke-width: 1.4;
    vector-effect: non-scaling-stroke;
  }
  .route-tests {
    stroke: color-mix(in srgb, var(--color-hypothesis) 60%, transparent);
    stroke-dasharray: 5 4;
  }
  #route-arrow path {
    fill: color-mix(in srgb, var(--color-info) 67%, transparent);
  }
  .module-region {
    position: absolute;
    translate: -50% -50%;
    display: grid;
    gap: 5px;
    width: clamp(128px, 15%, 190px);
    min-height: 92px;
    padding: 11px 12px;
    overflow: hidden;
    text-align: left;
    border: 1px solid color-mix(in srgb, var(--color-info) 28%, var(--line));
    border-left: 4px solid var(--color-info);
    border-radius: 13px;
    background: color-mix(in srgb, var(--map-bg) 88%, var(--color-info) 5%);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--color-shadow) 20%, transparent);
    transition:
      opacity 0.15s ease,
      scale 0.15s ease,
      box-shadow 0.15s ease;
  }
  .module-region:hover,
  .module-region.selected {
    z-index: 3;
    scale: 1.035;
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--color-info) 33%, transparent),
      0 14px 35px color-mix(in srgb, var(--color-shadow) 40%, transparent);
  }
  .module-region[data-status='current'] {
    border-left-color: var(--color-positive);
  }
  .module-region[data-status='needsReview'] {
    border-left-color: var(--color-warning);
  }
  .module-region[data-status='stale'] {
    border-left-color: var(--color-danger);
  }
  .module-region[data-status='unmapped'] {
    border-left-color: var(--color-neutral);
    border-style: dashed;
  }
  .module-region.lens-muted {
    opacity: 0.24;
  }
  .module-region.lens-match {
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--color-hypothesis) 40%, transparent),
      0 12px 30px color-mix(in srgb, var(--color-shadow) 33%, transparent);
  }
  .region-kind {
    color: var(--color-info);
    font-size: 0.66rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .module-region strong {
    overflow: hidden;
    font-size: 0.86rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .region-counts {
    color: var(--color-muted);
    font-size: 0.67rem;
  }
  .region-status {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 0.67rem;
  }
  .region-status i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .atlas-empty {
    display: grid;
    place-content: center;
    gap: 12px;
    height: 100%;
    min-height: 340px;
    padding: 32px;
    text-align: center;
    color: var(--color-muted);
  }
  .loader {
    justify-self: center;
    width: 24px;
    height: 24px;
    border: 3px solid var(--line);
    border-top-color: var(--color-info);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .atlas-summary {
    position: sticky;
    left: 12px;
    bottom: 12px;
    width: max-content;
    max-width: calc(100% - 24px);
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--map-bg);
    font-size: 0.75rem;
  }
  .atlas-summary ul {
    max-height: 180px;
    overflow: auto;
    padding-left: 18px;
  }
  .atlas-summary button {
    min-height: var(--control-min-size);
    border: 0;
    background: transparent;
    text-align: left;
  }
  .search-results {
    position: absolute;
    z-index: 10;
    top: 66px;
    left: 14px;
    width: min(430px, calc(100% - 28px));
    max-height: min(480px, calc(100% - 80px));
    overflow: auto;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: color-mix(in srgb, var(--map-bg) 96%, var(--color-overlay));
    box-shadow: 0 20px 45px color-mix(in srgb, var(--color-shadow) 47%, transparent);
  }
  .search-results > div {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .search-results ul {
    display: grid;
    gap: 6px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  .search-results li button {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 9px;
    width: 100%;
    padding: 9px 11px;
    text-align: left;
  }
  .search-results li span {
    grid-row: span 2;
    align-self: center;
    padding: 3px 6px;
    color: var(--color-info);
    border-radius: 5px;
    background: color-mix(in srgb, var(--color-info) 11%, transparent);
    font-size: 0.62rem;
    text-transform: uppercase;
  }
  .search-results li strong,
  .search-results li small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .search-results li small {
    color: var(--color-muted);
  }
  .inspector {
    z-index: 12;
    min-width: 0;
    overflow: auto;
    border-left: 1px solid var(--line);
    background: color-mix(in srgb, var(--map-bg) 97%, var(--color-overlay) 3%);
    opacity: 0;
    pointer-events: none;
  }
  .inspector.open {
    opacity: 1;
    pointer-events: auto;
  }
  .inspector > header {
    position: sticky;
    z-index: 3;
    top: 0;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--map-bg) 96%, transparent);
    backdrop-filter: blur(15px);
  }
  .inspector > header span,
  .eyebrow {
    color: var(--color-info);
    font-size: 0.68rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .inspector h3,
  .inspector h4,
  .inspector h5,
  .inspector p {
    margin-block: 0;
  }
  .inspector h3 {
    margin-top: 3px;
    font-size: 1rem;
  }
  .module-overview,
  .inspector-section,
  .evidence-panel,
  .source-preview {
    padding: 15px 16px;
    border-bottom: 1px solid var(--line);
  }
  .status-badge {
    display: inline-flex;
    padding: 4px 8px;
    border-radius: 999px;
    color: var(--color-neutral);
    background: var(--color-neutral-surface);
    font-size: 0.7rem;
    font-weight: 750;
  }
  .status-badge.current {
    color: var(--color-positive);
    background: var(--color-positive-surface);
  }
  .status-badge.needsReview {
    color: var(--color-warning);
    background: var(--color-warning-surface);
  }
  .status-badge.stale {
    color: var(--color-danger);
    background: var(--color-danger-surface);
  }
  .metric-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 7px;
    margin-top: 12px;
  }
  .metric-grid div {
    display: grid;
    gap: 2px;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 9px;
  }
  .metric-grid strong {
    font-size: 0.9rem;
  }
  .metric-grid span {
    color: var(--color-muted);
    font-size: 0.59rem;
  }
  .coverage {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 5px;
    margin-top: 13px;
    font-size: 0.72rem;
  }
  .coverage i,
  .dock-progress i {
    grid-column: 1 / -1;
    height: 4px;
    overflow: hidden;
    border-radius: 10px;
    background: var(--line);
  }
  .coverage b,
  .dock-progress b {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, var(--color-info), var(--color-positive));
  }
  details.inspector-section {
    padding: 0;
  }
  .inspector-section summary {
    min-height: 48px;
    padding: 14px 16px;
    cursor: pointer;
    font-weight: 750;
  }
  .inspector-section > :not(summary) {
    margin-inline: 16px;
  }
  .confidence-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 12px;
    color: var(--color-muted);
    font-size: 0.72rem;
  }
  .confidence-row strong {
    color: var(--color-positive);
  }
  .claim-group {
    margin-bottom: 15px;
  }
  .claim-group h5 {
    margin-bottom: 6px;
    color: var(--color-muted);
    font-size: 0.69rem;
    text-transform: uppercase;
  }
  .claim {
    padding: 10px;
    border-left: 2px solid color-mix(in srgb, var(--color-info) 47%, transparent);
    background: color-mix(in srgb, var(--map-bg) 86%, var(--color-surface-raised) 2%);
  }
  .claim + .claim {
    margin-top: 5px;
  }
  .claim p {
    font-size: 0.79rem;
    line-height: 1.45;
  }
  .claim > div {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    margin-top: 7px;
  }
  .claim span {
    color: var(--color-muted);
    font-size: 0.63rem;
  }
  .claim button {
    min-height: var(--control-min-size);
    padding-inline: 8px;
    font-size: 0.66rem;
  }
  .landmark-list,
  .relation-list {
    display: grid;
    gap: 5px;
    padding: 0 0 12px;
    list-style: none;
  }
  .landmark-list li,
  .relation-list li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 9px;
    border: 1px solid var(--line);
    border-radius: 8px;
    font-size: 0.72rem;
  }
  .landmark-list span,
  .relation-list span {
    color: var(--color-info);
  }
  .evidence-panel {
    background: color-mix(in srgb, var(--color-info) 4%, transparent);
  }
  .evidence-panel > * + * {
    margin-top: 8px;
  }
  .source-preview header {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 8px;
    font-size: 0.7rem;
  }
  .source-preview pre {
    max-height: 360px;
    overflow: auto;
    margin: 0;
    padding: 10px 0;
    color: var(--color-on-code);
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--color-code);
    font-size: 0.68rem;
    line-height: 1.5;
  }
  .source-preview code span {
    display: block;
    white-space: pre;
  }
  .source-preview code i {
    display: inline-block;
    width: 3.2rem;
    margin-right: 10px;
    padding-right: 9px;
    color: var(--color-subtle);
    border-right: 1px solid color-mix(in srgb, var(--color-on-code) 7%, transparent);
    text-align: right;
    font-style: normal;
    user-select: none;
  }
  .inspector-note {
    margin-top: 12px !important;
    color: var(--color-muted);
    font-size: 0.76rem;
  }
  .inspector-section dl {
    display: grid;
    gap: 7px;
  }
  .inspector-section dl div {
    display: grid;
    gap: 2px;
  }
  .inspector-section dt {
    color: var(--color-muted);
    font-size: 0.68rem;
  }
  .inspector-section dd {
    overflow-wrap: anywhere;
    margin: 0;
    font-size: 0.78rem;
  }
  .inspector-empty {
    display: grid;
    place-items: center;
    align-content: center;
    min-height: 100%;
    padding: 30px;
    text-align: center;
    color: var(--color-muted);
  }
  .inspector-empty > span {
    font-size: 2rem;
  }
  .deep-map-dock {
    z-index: 15;
    border-top: 1px solid var(--line);
    background: color-mix(in srgb, var(--map-bg) 96%, var(--color-overlay) 2%);
  }
  .dock-summary {
    display: grid;
    grid-template-columns: auto minmax(150px, auto) auto minmax(160px, 1fr) auto;
    gap: 12px;
    align-items: center;
    width: 100%;
    min-height: 58px;
    padding: 7px 16px;
    border: 0;
    border-radius: 0;
    text-align: left;
    background: transparent;
  }
  .dock-summary > span:nth-child(2) {
    display: grid;
  }
  .dock-summary small {
    color: var(--color-muted);
  }
  .deep-map-icon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    color: var(--color-hypothesis);
    border-radius: 10px;
    background: var(--color-hypothesis-surface);
  }
  .run-state {
    justify-self: start;
    padding: 4px 9px;
    color: var(--color-neutral);
    border-radius: 999px;
    background: var(--color-neutral-surface);
    font-size: 0.68rem;
  }
  .run-state.running,
  .run-state.queued {
    color: var(--color-info);
    background: var(--color-info-surface);
  }
  .run-state.succeeded {
    color: var(--color-positive);
    background: var(--color-positive-surface);
  }
  .run-state.failed {
    color: var(--color-danger);
    background: var(--color-danger-surface);
  }
  .dock-progress {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 5px 9px;
    align-items: center;
  }
  .dock-progress i {
    grid-column: auto;
  }
  .dock-content {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(270px, 0.8fr);
    gap: 18px;
    max-height: min(320px, 45vh);
    overflow: auto;
    padding: 14px 16px 18px;
    border-top: 1px solid var(--line);
  }
  .preset-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(110px, 1fr));
    gap: 7px;
  }
  .preset-grid button {
    display: grid;
    gap: 3px;
    min-height: 80px;
    padding: 9px;
    text-align: left;
  }
  .preset-grid span,
  .preset-grid small {
    color: var(--color-muted);
    font-size: 0.68rem;
  }
  .advanced-budget {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-top: 8px;
  }
  .advanced-budget label {
    display: grid;
    gap: 4px;
    color: var(--color-muted);
    font-size: 0.68rem;
  }
  .advanced-budget input {
    min-width: 0;
    min-height: var(--control-min-size);
    padding: 6px 8px;
    color: inherit;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--map-bg);
  }
  .run-panel {
    display: grid;
    gap: 13px;
  }
  .run-facts {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 7px;
  }
  .run-facts div {
    display: grid;
    gap: 3px;
    padding: 9px;
    border: 1px solid var(--line);
    border-radius: 9px;
  }
  .run-facts span {
    color: var(--color-muted);
    font-size: 0.64rem;
  }
  .run-facts strong {
    font-size: 0.75rem;
  }
  .run-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .run-actions button {
    padding-inline: 12px;
  }
  .run-actions .primary {
    color: var(--color-on-accent);
    background: var(--color-accent);
    font-weight: 800;
  }
  .run-actions .danger {
    color: var(--color-danger);
  }
  .activity-feed {
    display: grid;
    gap: 5px;
    max-height: 150px;
    overflow: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .activity-feed li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    padding: 7px 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    font-size: 0.68rem;
  }
  .activity-feed li > span {
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
  }
  .activity-feed li div {
    display: grid;
    min-width: 0;
  }
  .activity-feed li small {
    overflow: hidden;
    color: var(--color-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .activity-feed li i {
    color: var(--color-info);
    font-style: normal;
  }
  .activity-feed li.confirmed i {
    color: var(--color-positive);
  }
  .feed-empty {
    margin: 0;
    color: var(--color-muted);
    font-size: 0.72rem;
    line-height: 1.4;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  @media (max-width: 1050px) {
    .command-bar {
      grid-template-columns: auto 1fr auto;
    }
    .map-title p {
      display: none;
    }
    .dock-content {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 899px) {
    .map-shell {
      min-height: calc(100vh - 7.5rem);
      height: calc(100vh - 7.5rem);
      border-radius: 12px;
    }
    .command-bar {
      grid-template-columns: 1fr auto;
    }
    .map-search {
      grid-column: 1 / -1;
      grid-row: 2;
    }
    .map-body,
    .map-body.inspector-visible {
      position: relative;
      display: block;
    }
    .atlas-panel {
      height: 100%;
    }
    .inspector {
      position: absolute;
      inset: 0 0 0 auto;
      width: min(430px, 92vw);
      box-shadow: -18px 0 45px color-mix(in srgb, var(--color-shadow) 53%, transparent);
      transform: translateX(105%);
      transition: transform 0.2s ease;
    }
    .inspector.open {
      transform: translateX(0);
    }
    .dock-summary {
      grid-template-columns: auto 1fr auto auto;
    }
    .dock-progress {
      display: none;
    }
    .preset-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }
  @media (max-width: 720px) {
    .map-title div {
      display: none;
    }
    .atlas-toolbar {
      align-items: flex-start;
    }
    .atlas-toolbar > div:first-child {
      display: grid;
    }
    .atlas-controls button:not(.icon-button) {
      font-size: 0;
      width: 44px;
    }
    .atlas-controls button:not(.icon-button)::after {
      content: '⌖';
      font-size: 1rem;
    }
    .atlas-controls button:first-child:not(.icon-button)::after {
      content: '←';
    }
    .dock-summary {
      gap: 7px;
      padding-inline: 10px;
    }
    .dock-summary > span:nth-child(2) small {
      display: none;
    }
    .run-state {
      max-width: 120px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .dock-content {
      max-height: 52vh;
    }
    .metric-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      scroll-behavior: auto !important;
      transition-duration: 0.001ms !important;
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
    }
  }
</style>

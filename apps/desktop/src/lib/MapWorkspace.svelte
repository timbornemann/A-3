<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import DeepMapDock from './DeepMapDock.svelte';
  import DeepMapInspector from './DeepMapInspector.svelte';
  import { createFrameCoalescedResize } from './frame-coalesced-resize';
  import MapAtlasCanvas from './MapAtlasCanvas.svelte';
  import MapInspector from './MapInspector.svelte';
  import type { IndexActivityStateV1 } from './index-activity';
  import {
    queryProjectMapAtlasScene,
    queryProjectMapEntityContext,
    queryProjectMapFlowScene,
    queryProjectMapInventoryPage,
    type ProjectMapAtlasNodeV1,
    type ProjectMapAtlasSceneResponseV1,
    type ProjectMapAtlasSceneV1,
    type ProjectMapEntityContextResponseV1,
    type ProjectMapEntityContextV1,
    type ProjectMapEntitySelectionV1,
    type ProjectMapFlowPresetV1,
    type ProjectMapFlowSceneResponseV1,
    type ProjectMapFlowSceneV1,
    type ProjectMapIndexEvidenceSelectionV1,
    type ProjectMapInventoryPageResponseV1,
    type ProjectMapInventoryPageV1,
    type ProjectMapInventoryViewV1,
  } from './project-map-atlas';
  import {
    queryProjectMapSearch,
    type ProjectMapSearchHitV1,
    type ProjectMapSearchResponseV1,
  } from './project-map-search';
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './project-rebuild';
  import {
    queryProjectMapSourcePreview,
    type ProjectMapSourcePreviewQueryV1,
    type ProjectMapSourcePreviewResponseV1,
    type ProjectMapSourcePreviewV1,
  } from './project-map-source-preview';
  import {
    compileTaskLens,
    queryTaskLensTask,
    queryTaskLensTasks,
    type TaskLensCompileResponseV1,
    type TaskLensTaskResponseV1,
    type TaskLensTasksResponseV1,
  } from './task-lens';
  import type {
    DeepMapControlResponseV1,
    DeepMapEntryDetailResponseV1,
    DeepMapEntryPageResponseV1,
    DeepMapModeV2,
    DeepMapRunPageResponseV1,
    DeepMapStartResponseV2,
    DeepMapStatusResponseV3,
  } from './deep-map';
  import {
    queryDeepMapAtlasImpact,
    queryDeepMapModuleSteps,
    queryDeepMapRunDashboard,
    queryDeepMapRunModules,
    type DeepMapAtlasImpactResponseV1,
    type DeepMapModuleStepsResponseV1,
    type DeepMapRunDashboardResponseV1,
    type DeepMapRunModulesResponseV1,
  } from './deep-map-dashboard';
  import {
    queryModuleCardDetail,
    type ModuleCardDetailQueryV1,
    type ModuleCardDetailResponseV1,
  } from './module-card-detail';

  type ReadState<T> =
    | { kind: 'idle' | 'loading' | 'error' }
    | { kind: 'available'; value: T }
    | { kind: 'unavailable'; message: string };
  interface Props {
    initialSelection?: ProjectMapEntitySelectionV1 | null;
    onFunctionFlow?: (selection: import('./function-flow').FlowSelection) => void;
    /** @deprecated U11 compatibility seam; the progressive Atlas uses `atlasSceneLoader`. */
    sceneLoader?: (query: { focusModuleId: string | null }) => Promise<unknown>;
    /** @deprecated U11 compatibility seam; Module Cards no longer source Atlas scenes. */
    cardLoader?: (query: { moduleId: string }) => Promise<unknown>;
    /** @deprecated U11 compatibility seam; preview reads now use typed index Evidence. */
    evidenceLoader?: (query: unknown) => Promise<unknown>;
    /** @deprecated U11 compatibility seam; landmarks are delivered by Atlas scenes. */
    runtimeLoader?: (query: unknown) => Promise<unknown>;
    atlasSceneLoader?: (
      selection: ProjectMapEntitySelectionV1 | null,
    ) => Promise<ProjectMapAtlasSceneResponseV1>;
    contextLoader?: (
      selection: ProjectMapEntitySelectionV1,
    ) => Promise<ProjectMapEntityContextResponseV1>;
    inventoryLoader?: (
      selection: ProjectMapEntitySelectionV1,
      view: ProjectMapInventoryViewV1,
      cursor: string | null,
    ) => Promise<ProjectMapInventoryPageResponseV1>;
    flowLoader?: (
      selection: ProjectMapEntitySelectionV1,
      preset: ProjectMapFlowPresetV1,
    ) => Promise<ProjectMapFlowSceneResponseV1>;
    indexActivityState?: IndexActivityStateV1;
    indexRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    publicationKey?: string | null;
    searchLoader?: (query: { query: string }) => Promise<ProjectMapSearchResponseV1>;
    sourcePreviewLoader?: (
      query: ProjectMapSourcePreviewQueryV1,
    ) => Promise<ProjectMapSourcePreviewResponseV1>;
    projectKey: string;
    taskLensTasksLoader?: () => Promise<TaskLensTasksResponseV1>;
    taskLensTaskLoader?: (query: { taskId: string }) => Promise<TaskLensTaskResponseV1>;
    taskLensCompiler?: (query: {
      stepId: string;
      taskId: string;
    }) => Promise<TaskLensCompileResponseV1>;
    deepMapStatusLoader?: () => Promise<DeepMapStatusResponseV3>;
    deepMapStarter?: (mode: DeepMapModeV2) => Promise<DeepMapStartResponseV2>;
    deepMapPauser?: () => Promise<DeepMapControlResponseV1>;
    deepMapResumer?: () => Promise<DeepMapControlResponseV1>;
    deepMapCanceller?: () => Promise<DeepMapControlResponseV1>;
    deepMapRunsLoader?: (cursor?: string | null) => Promise<DeepMapRunPageResponseV1>;
    deepMapEntriesLoader?: (
      runSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapEntryPageResponseV1>;
    deepMapDetailLoader?: (
      runSelection: string,
      entrySelection: string,
    ) => Promise<DeepMapEntryDetailResponseV1>;
    deepMapDashboardLoader?: (runSelection: string) => Promise<DeepMapRunDashboardResponseV1>;
    deepMapModulesLoader?: (
      runSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapRunModulesResponseV1>;
    deepMapStepsLoader?: (
      runSelection: string,
      moduleSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapModuleStepsResponseV1>;
    deepMapAtlasImpactLoader?: (
      runSelection: string,
      moduleSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapAtlasImpactResponseV1>;
    deepMapCardLoader?: (query: ModuleCardDetailQueryV1) => Promise<ModuleCardDetailResponseV1>;
  }
  const {
    initialSelection = null,
    onFunctionFlow,
    projectKey,
    publicationKey = null,
    atlasSceneLoader = queryProjectMapAtlasScene,
    contextLoader = queryProjectMapEntityContext,
    inventoryLoader = queryProjectMapInventoryPage,
    flowLoader = queryProjectMapFlowScene,
    indexActivityState = 'idle',
    indexRebuilder = rebuildProjectIndex,
    searchLoader = queryProjectMapSearch,
    sourcePreviewLoader = queryProjectMapSourcePreview,
    taskLensTasksLoader = queryTaskLensTasks,
    taskLensTaskLoader = queryTaskLensTask,
    taskLensCompiler = compileTaskLens,
    deepMapStatusLoader,
    deepMapStarter,
    deepMapPauser,
    deepMapResumer,
    deepMapCanceller,
    deepMapRunsLoader,
    deepMapEntriesLoader,
    deepMapDashboardLoader = queryDeepMapRunDashboard,
    deepMapModulesLoader = queryDeepMapRunModules,
    deepMapStepsLoader = queryDeepMapModuleSteps,
    deepMapAtlasImpactLoader = queryDeepMapAtlasImpact,
    deepMapCardLoader = queryModuleCardDetail,
  }: Props = $props();

  let scene = $state<ReadState<ProjectMapAtlasSceneV1>>({ kind: 'loading' });
  let selected = $state<ProjectMapAtlasNodeV1 | null>(null);
  let context = $state<ReadState<ProjectMapEntityContextV1>>({ kind: 'idle' });
  let inventory = $state<ReadState<ProjectMapInventoryPageV1>>({ kind: 'idle' });
  let flow = $state<ReadState<ProjectMapFlowSceneV1>>({ kind: 'idle' });
  let preview = $state<ReadState<ProjectMapSourcePreviewV1>>({ kind: 'idle' });
  let searchText = $state('');
  let searchState = $state<ReadState<ProjectMapSearchHitV1[]>>({ kind: 'idle' });
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
  let indexRebuildView = $state<'error' | 'idle' | 'queued' | 'submitting'>('idle');
  let indexRebuildSawActivity = $state(false);
  let retrySelection = $state<ProjectMapEntitySelectionV1 | null>(null);
  const INSPECTOR_DEFAULT_WIDTH = 380;
  const INSPECTOR_MIN_WIDTH = 320;
  const INSPECTOR_MAX_WIDTH = 720;
  const ATLAS_MIN_WIDTH = 360;
  const INSPECTOR_KEYBOARD_STEP = 24;
  let workspaceBody = $state<HTMLElement | null>(null);
  let workspaceBodyWidth = $state(0);
  let inspectorWidth = $state(INSPECTOR_DEFAULT_WIDTH);
  let resizingInspector = $state(false);
  let inspectorMode = $state<'code' | 'deepMap' | null>(null);
  let deepMapFailureFocusEpoch = $state(0);
  let deepMapRunStartedEpoch = $state(0);
  let inspectorResizePointerId: number | null = null;
  let inspectorResizeHandle: HTMLElement | null = null;
  let requestGeneration = 0;
  let contextGeneration = 0;
  let activeSceneLoad: {
    key: string;
    promise: Promise<ProjectMapAtlasSceneV1 | null>;
  } | null = null;

  const lensModuleIds = $derived.by(() => {
    const ids = new SvelteSet<string>();
    if (lens === null) return ids;
    for (const entry of lens.entries)
      if (entry.target.kind === 'module') ids.add(entry.target.moduleId);
    for (const claim of lens.claims) ids.add(claim.moduleId);
    return ids;
  });

  const inspectorMaxWidth = $derived(
    workspaceBodyWidth > 0
      ? Math.max(
          INSPECTOR_MIN_WIDTH,
          Math.min(INSPECTOR_MAX_WIDTH, workspaceBodyWidth - ATLAS_MIN_WIDTH),
        )
      : INSPECTOR_MAX_WIDTH,
  );
  const effectiveInspectorWidth = $derived(
    Math.min(Math.max(inspectorWidth, INSPECTOR_MIN_WIDTH), inspectorMaxWidth),
  );
  const inspectorOpen = $derived(
    inspectorMode === 'deepMap' || (inspectorMode === 'code' && selected !== null),
  );
  const indexRebuildActive = $derived(
    indexActivityState === 'queued' ||
      indexActivityState === 'running' ||
      indexActivityState === 'cancelling',
  );
  const indexRebuildDisabled = $derived(
    indexRebuildView === 'submitting' ||
      (indexRebuildView === 'queued' && !indexRebuildSawActivity) ||
      indexRebuildActive,
  );
  const indexRebuildLabel = $derived.by(() => {
    if (indexRebuildView === 'submitting') return 'Wird eingeplant …';
    if (indexActivityState === 'queued') return 'Startet …';
    if (indexActivityState === 'running') return 'Fast Index läuft';
    if (indexActivityState === 'cancelling') return 'Wird beendet …';
    if (indexRebuildView === 'queued') return 'Eingeplant';
    if (indexRebuildView === 'error') return 'Erneut versuchen';
    return 'Fast Index';
  });

  onMount(() => {
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (inspectorMode === 'deepMap') inspectorMode = null;
        else void goBack();
      }
    };
    const updateWorkspaceWidth = (width: number) => {
      if (workspaceBodyWidth !== width) workspaceBodyWidth = width;
    };
    const resize = createFrameCoalescedResize((size) => updateWorkspaceWidth(size.width));
    const scheduleWorkspaceMeasurement = () =>
      resize.schedule({
        height: workspaceBody?.clientHeight ?? 0,
        width: workspaceBody?.clientWidth ?? 0,
      });
    const resizeObserver =
      typeof ResizeObserver === 'undefined'
        ? null
        : new ResizeObserver(([entry]) =>
            resize.schedule({ height: entry.contentRect.height, width: entry.contentRect.width }),
          );
    if (workspaceBody !== null) resizeObserver?.observe(workspaceBody);
    updateWorkspaceWidth(workspaceBody?.clientWidth ?? 0);
    window.addEventListener('keydown', keydown);
    window.addEventListener('pointermove', continueInspectorResize);
    window.addEventListener('pointerup', stopInspectorResize);
    window.addEventListener('pointercancel', stopInspectorResize);
    window.addEventListener('resize', scheduleWorkspaceMeasurement);
    return () => {
      resizeObserver?.disconnect();
      resize.dispose();
      window.removeEventListener('keydown', keydown);
      window.removeEventListener('pointermove', continueInspectorResize);
      window.removeEventListener('pointerup', stopInspectorResize);
      window.removeEventListener('pointercancel', stopInspectorResize);
      window.removeEventListener('resize', scheduleWorkspaceMeasurement);
    };
  });

  $effect(() => {
    void projectKey;
    inspectorWidth = INSPECTOR_DEFAULT_WIDTH;
    indexRebuildView = 'idle';
    indexRebuildSawActivity = false;
  });

  $effect(() => {
    if (indexRebuildView !== 'queued') return;
    if (indexRebuildActive) {
      indexRebuildSawActivity = true;
    } else if (indexRebuildSawActivity) {
      indexRebuildView = 'idle';
      indexRebuildSawActivity = false;
    }
  });

  $effect(() => {
    void projectKey;
    void publicationKey;
    requestGeneration += 1;
    contextGeneration += 1;
    selected = null;
    inspectorMode = null;
    context = { kind: 'idle' };
    inventory = { kind: 'idle' };
    flow = { kind: 'idle' };
    preview = { kind: 'idle' };
    void loadScene(initialSelection).then((loaded) => {
      if (!loaded || !initialSelection) return;
      const node = loaded.nodes.find(
        (n) => selectionKey(n.selection) === selectionKey(initialSelection),
      );
      if (node) void selectNode(node);
    });
  });

  function clampInspectorWidth(width: number): number {
    return Math.min(Math.max(width, INSPECTOR_MIN_WIDTH), inspectorMaxWidth);
  }

  function resizeInspectorAt(clientX: number): void {
    if (workspaceBody === null) return;
    inspectorWidth = clampInspectorWidth(workspaceBody.getBoundingClientRect().right - clientX);
  }

  function startInspectorResize(event: PointerEvent): void {
    if (event.button !== 0 || !inspectorOpen) return;
    event.preventDefault();
    resizingInspector = true;
    inspectorResizePointerId = event.pointerId;
    inspectorResizeHandle = event.currentTarget as HTMLElement;
    inspectorResizeHandle.setPointerCapture?.(event.pointerId);
    resizeInspectorAt(event.clientX);
  }

  function continueInspectorResize(event: PointerEvent): void {
    if (!resizingInspector || event.pointerId !== inspectorResizePointerId) return;
    resizeInspectorAt(event.clientX);
  }

  function stopInspectorResize(event: PointerEvent): void {
    if (!resizingInspector || event.pointerId !== inspectorResizePointerId) return;
    resizingInspector = false;
    if (inspectorResizeHandle?.hasPointerCapture?.(event.pointerId))
      inspectorResizeHandle.releasePointerCapture(event.pointerId);
    inspectorResizePointerId = null;
    inspectorResizeHandle = null;
  }

  function resizeInspectorWithKeyboard(event: KeyboardEvent): void {
    if (!inspectorOpen) return;
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      inspectorWidth = clampInspectorWidth(effectiveInspectorWidth + INSPECTOR_KEYBOARD_STEP);
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      inspectorWidth = clampInspectorWidth(effectiveInspectorWidth - INSPECTOR_KEYBOARD_STEP);
    } else if (event.key === 'Home') {
      event.preventDefault();
      inspectorWidth = INSPECTOR_MIN_WIDTH;
    } else if (event.key === 'End') {
      event.preventDefault();
      inspectorWidth = inspectorMaxWidth;
    }
  }

  function loadScene(
    selection: ProjectMapEntitySelectionV1 | null,
  ): Promise<ProjectMapAtlasSceneV1 | null> {
    const key = `${projectKey}:${publicationKey ?? ''}:${selectionKey(selection)}`;
    if (activeSceneLoad?.key === key) return activeSceneLoad.promise;
    const promise = performSceneLoad(selection);
    activeSceneLoad = { key, promise };
    void promise.finally(() => {
      if (activeSceneLoad?.promise === promise) activeSceneLoad = null;
    });
    return promise;
  }

  async function performSceneLoad(
    selection: ProjectMapEntitySelectionV1 | null,
  ): Promise<ProjectMapAtlasSceneV1 | null> {
    const request = ++requestGeneration;
    retrySelection = selection;
    scene = { kind: 'loading' };
    try {
      const response = await atlasSceneLoader(selection);
      if (request !== requestGeneration) return null;
      if (response.result.status === 'available') {
        scene = { kind: 'available', value: response.result.scene };
        selected = null;
        context = { kind: 'idle' };
        inventory = { kind: 'idle' };
        flow = { kind: 'idle' };
        preview = { kind: 'idle' };
        return response.result.scene;
      }
      scene = {
        kind: 'unavailable',
        message: (
          {
            noProject: 'Öffne ein Projekt, um den Code Atlas zu verwenden.',
            noPublishedIndex: 'Der Atlas erscheint nach der ersten atomaren Index-Publikation.',
            projectionUnavailable:
              'Diese Publikation enthält noch keine deterministische Modulprojektion.',
            selectionChanged: 'Die Auswahl wurde durch eine neuere Publikation ersetzt.',
          } as const
        )[response.result.status],
      };
    } catch {
      if (request === requestGeneration) scene = { kind: 'error' };
    }
    return null;
  }

  async function selectNode(node: ProjectMapAtlasNodeV1): Promise<void> {
    if (selected?.nodeId === node.nodeId && context.kind === 'loading') return;
    selected = node;
    inspectorMode = 'code';
    inventory = { kind: 'idle' };
    flow = { kind: 'idle' };
    preview = { kind: 'idle' };
    if (node.selection === null) {
      context = { kind: 'idle' };
      return;
    }
    const request = ++contextGeneration;
    context = { kind: 'loading' };
    try {
      const response = await contextLoader(node.selection);
      if (request !== contextGeneration) return;
      context =
        response.result.status === 'available'
          ? { kind: 'available', value: response.result.context }
          : {
              kind: 'unavailable',
              message: 'Diese Auswahl gehört nicht mehr zur aktuellen Publikation.',
            };
    } catch {
      if (request === contextGeneration) context = { kind: 'error' };
    }
  }

  async function showDeepMapModuleInAtlas(
    runSelection: string,
    moduleSelection: string,
  ): Promise<void> {
    try {
      const card = await deepMapCardLoader({ runSelection, moduleSelection });
      if (card.result.status !== 'available') return;
      const selection: ProjectMapEntitySelectionV1 = {
        kind: 'module',
        moduleId: card.result.detail.moduleId,
      };
      const overview = await loadScene(null);
      const visible = overview?.nodes.find(
        (node) =>
          node.selection?.kind === 'module' && node.selection.moduleId === selection.moduleId,
      );
      if (visible !== undefined) {
        await selectNode(visible);
        return;
      }
      await loadScene(selection);
      const response = await contextLoader(selection);
      if (response.result.status !== 'available') return;
      selected = response.result.context.entity;
      context = { kind: 'available', value: response.result.context };
      inspectorMode = 'code';
    } catch {
      context = { kind: 'error' };
    }
  }

  async function openNode(node: ProjectMapAtlasNodeV1): Promise<void> {
    if (node.selection === null) return;
    contextGeneration += 1;
    await loadScene(node.selection);
  }

  function selectionKey(selection: ProjectMapEntitySelectionV1 | null): string {
    if (selection === null) return 'project';
    if (selection.kind === 'module') return `module:${selection.moduleId}`;
    if (selection.kind === 'file') {
      return `file:${selection.moduleId}:${selection.evidenceId}:${selection.ordinal}`;
    }
    return `symbol:${selection.moduleId}:${selection.evidenceId}:${selection.symbolId}`;
  }

  async function goBack(): Promise<void> {
    if (scene.kind !== 'available') return;
    const crumbs = scene.value.breadcrumb;
    if (crumbs.length <= 1) {
      selected = null;
      return;
    }
    await loadScene(crumbs[crumbs.length - 2].selection);
  }

  async function loadInventory(
    view: ProjectMapInventoryViewV1,
    cursor: string | null,
  ): Promise<void> {
    if (selected === null || selected.selection === null) return;
    inventory = { kind: 'loading' };
    try {
      const response = await inventoryLoader(selected.selection, view, cursor);
      inventory =
        response.result.status === 'available'
          ? { kind: 'available', value: response.result.page }
          : { kind: 'unavailable', message: 'Inventarauswahl oder Cursor wurde ersetzt.' };
    } catch {
      inventory = { kind: 'error' };
    }
  }

  async function loadFlow(preset: ProjectMapFlowPresetV1): Promise<void> {
    if (selected === null || selected.selection === null) return;
    flow = { kind: 'loading' };
    try {
      const response = await flowLoader(selected.selection, preset);
      flow =
        response.result.status === 'available'
          ? { kind: 'available', value: response.result.flow }
          : { kind: 'unavailable', message: 'Für diese aktuelle Auswahl ist kein Flow verfügbar.' };
    } catch {
      flow = { kind: 'error' };
    }
  }

  async function openEvidence(evidence: ProjectMapIndexEvidenceSelectionV1): Promise<void> {
    preview = { kind: 'loading' };
    try {
      const response = await sourcePreviewLoader({ evidence, kind: 'index' });
      preview =
        response.result.status === 'available'
          ? { kind: 'available', value: response.result.preview }
          : {
              kind: 'unavailable',
              message:
                response.result.status === 'staleEvidence'
                  ? 'Stale Evidence bleibt metadata-only.'
                  : 'Die Evidence gehört nicht mehr zur aktuellen Publikation.',
            };
    } catch {
      preview = { kind: 'error' };
    }
  }

  async function submitSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const query = searchText.trim();
    if (query.length < 3) return;
    searchState = { kind: 'loading' };
    try {
      const response = await searchLoader({ query });
      searchState =
        response.result.status === 'available'
          ? response.result.search.hits.length === 0
            ? { kind: 'unavailable', message: 'Keine Treffer im aktuellen Snapshot.' }
            : { kind: 'available', value: response.result.search.hits }
          : {
              kind: 'unavailable',
              message: 'Die Suche ist für diese Publikation nicht verfügbar.',
            };
    } catch {
      searchState = { kind: 'error' };
    }
  }

  async function openSearchHit(hit: ProjectMapSearchHitV1): Promise<void> {
    let targetScene = scene.kind === 'available' ? scene.value : null;
    if (
      hit.moduleId !== null &&
      (targetScene?.level !== 'module' || targetScene.selection?.moduleId !== hit.moduleId)
    ) {
      targetScene = await loadScene({ kind: 'module', moduleId: hit.moduleId });
    }
    if (hit.moduleId !== null && hit.evidenceSelection.kind === 'symbol') {
      await loadScene({
        evidenceId: hit.evidenceSelection.evidenceId,
        kind: 'symbol',
        moduleId: hit.moduleId,
        symbolId: hit.evidenceSelection.symbolId,
      });
      searchState = { kind: 'idle' };
      return;
    }
    const matched =
      targetScene?.nodes.find((node) => node.evidenceId === hit.evidenceSelection.evidenceId) ??
      null;
    if (matched !== null) await selectNode(matched);
    else {
      selected = {
        claimBadgeCount: 0,
        currentRiskCount: '0',
        detail:
          hit.target.kind === 'symbol' ? hit.target.signature : hit.target.evidence.pathDisplay,
        dimmed: false,
        displayName:
          hit.target.kind === 'symbol'
            ? hit.target.qualifiedName || hit.target.name
            : hit.target.evidence.pathDisplay,
        evidenceId: hit.evidenceSelection.evidenceId,
        fileCount: hit.target.kind === 'file' ? '1' : '0',
        kind: hit.target.kind === 'file' ? 'file' : 'callable',
        mappingStatus: null,
        memberCount: '0',
        nodeId: hit.evidenceSelection.evidenceId,
        parentNodeId: null,
        purpose: null,
        rank: 1,
        selection: null,
        symbolCount: hit.target.kind === 'symbol' ? '1' : '0',
        volume: '1',
      };
      inspectorMode = 'code';
      context = {
        kind: 'unavailable',
        message:
          hit.moduleId === null
            ? 'Der Treffer ist nicht eindeutig einem Primärmodul zugeordnet.'
            : 'Der Treffer liegt außerhalb der begrenzten aktuellen Kartenszene.',
      };
    }
    searchState = { kind: 'idle' };
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
  function hitLabel(hit: ProjectMapSearchHitV1): string {
    return hit.target.kind === 'symbol'
      ? hit.target.qualifiedName || hit.target.name
      : hit.target.evidence.pathDisplay;
  }
  function reloadPublished(): void {
    void loadScene(scene.kind === 'available' ? scene.value.selection : null);
  }

  async function requestFastIndex(): Promise<void> {
    if (indexRebuildDisabled) return;
    indexRebuildView = 'submitting';
    indexRebuildSawActivity = false;
    try {
      await indexRebuilder();
      indexRebuildView = 'queued';
    } catch {
      indexRebuildView = 'error';
    }
  }

  function toggleDeepMapInspector(focusFailure: boolean): void {
    if (focusFailure) {
      inspectorMode = 'deepMap';
      deepMapFailureFocusEpoch += 1;
      return;
    }
    inspectorMode = inspectorMode === 'deepMap' ? null : 'deepMap';
  }

  function showStartedDeepMapRun(): void {
    inspectorMode = 'deepMap';
    deepMapRunStartedEpoch += 1;
  }
</script>

<section
  class="map-shell map-workspace"
  aria-labelledby="atlas-title"
  data-project-key={projectKey}
>
  <header class="command-bar">
    <div class="title">
      <span aria-hidden="true">A³</span>
      <div>
        <h2 id="atlas-title">Code Atlas</h2>
        <p>Projekt → Modul → Datei → Typ/Symbol</p>
      </div>
    </div>
    <form role="search" onsubmit={submitSearch}>
      <label class="sr-only" for="atlas-search">Code durchsuchen</label><span aria-hidden="true"
        >⌕</span
      ><input
        id="atlas-search"
        type="search"
        autocomplete="off"
        maxlength="4096"
        placeholder="Datei, Klasse, Funktion oder Signatur …"
        bind:value={searchText}
      /><button
        type="submit"
        disabled={searchText.trim().length < 3 || searchState.kind === 'loading'}
        >{searchState.kind === 'loading' ? 'Sucht …' : 'Suchen'}</button
      >
    </form>
    <div class="command-actions">
      <div class="fast-index-control">
        <button type="button" disabled={indexRebuildDisabled} onclick={requestFastIndex}
          >↻ {indexRebuildLabel}</button
        >
        {#if indexRebuildView === 'error'}
          <span class="fast-index-error" role="alert"
            >Fast Index konnte nicht gestartet werden.</span
          >
        {/if}
      </div>
      <div class="lens">
        <button
          type="button"
          class:active={lens !== null}
          aria-expanded={lensOpen}
          onclick={toggleLens}>◎ Task Lens{lens === null ? '' : ` · ${lensModuleIds.size}`}</button
        >
        {#if lensOpen}<div class="lens-popover">
            <strong>Aktuellen Task fokussieren</strong>{#if lensBusy}<p role="status">
                Task Lens wird geladen …
              </p>{:else if lensError}<p role="alert">
                Task Lens konnte nicht geladen werden.
              </p>{:else}<label
                >Task<select value={selectedTaskId} onchange={selectTask}
                  ><option value="">Task wählen</option
                  >{#if lensTasks?.status === 'available'}{#each lensTasks.tasks as task (task.taskId)}<option
                        value={task.taskId}>{task.objective}</option
                      >{/each}{/if}</select
                ></label
              >{#if lensTask?.status === 'available'}<label
                  >Schritt<select bind:value={selectedStepId}
                    ><option value="">Schritt wählen</option
                    >{#each lensTask.steps as step (step.stepId)}<option value={step.stepId}
                        >{step.intendedOutcome}</option
                      >{/each}</select
                  ></label
                ><button type="button" disabled={selectedStepId === ''} onclick={applyLens}
                  >Lens anwenden</button
                >{/if}{#if lens !== null}<button type="button" onclick={clearLens}
                  >Lens entfernen</button
                >{/if}{/if}
          </div>{/if}
      </div>
    </div>
  </header>

  {#if searchState.kind !== 'idle'}
    <aside class="search-results" aria-label="Suchergebnisse">
      {#if searchState.kind === 'loading'}<p role="status">
          Der aktuelle Snapshot wird durchsucht …
        </p>
      {:else if searchState.kind === 'error'}<p role="alert">
          Die Suche konnte nicht sicher ausgeführt werden.
        </p>
      {:else if searchState.kind === 'unavailable'}<p>{searchState.message}</p>
      {:else if searchState.kind === 'available'}<ul>
          {#each searchState.value as hit (hit.rank)}<li>
              <button type="button" onclick={() => openSearchHit(hit)}
                ><span>{hit.target.kind} · Rang {hit.rank}</span><strong>{hitLabel(hit)}</strong
                ><small
                  >{hit.target.evidence.pathDisplay}{hit.moduleId === null
                    ? ' · nicht zugeordnet'
                    : ''}</small
                ></button
              >
            </li>{/each}
        </ul>{/if}
    </aside>
  {/if}

  <div class="atlas-toolbar">
    <nav aria-label="Atlas Breadcrumb">
      {#if scene.kind === 'available'}{#each scene.value.breadcrumb as crumb, index (`${index}:${crumb.label}`)}<button
            type="button"
            aria-current={index === scene.value.breadcrumb.length - 1 ? 'page' : undefined}
            onclick={() => loadScene(crumb.selection)}>{crumb.label}</button
          >{#if index < scene.value.breadcrumb.length - 1}<span aria-hidden="true">›</span
            >{/if}{/each}{/if}
    </nav>
    {#if scene.kind === 'available'}<div class="scene-facts">
        <strong>{scene.value.nodeCount} Objekte</strong><span
          >{scene.value.relationCount} Routen</span
        ><span>{scene.value.boundaryCount} Boundaries</span
        >{#if scene.value.nodesTruncated || scene.value.relationsTruncated}<b>begrenzt</b>{/if}
      </div>{/if}
    <div class="legend">
      <span><i class="small"></i>kleiner</span><span><i class="large"></i>größer</span><em
        >Fläche 1:8 begrenzt</em
      >
    </div>
  </div>

  <main
    class:resizing={resizingInspector}
    class="workspace-body"
    bind:this={workspaceBody}
    style={`--inspector-width: ${effectiveInspectorWidth}px`}
  >
    <div class="atlas-stage">
      {#if scene.kind === 'loading'}<div class="empty" role="status">
          Atlas wird aus der aktuellen Publikation aufgebaut …
        </div>
      {:else if scene.kind === 'error'}<div class="empty" role="alert">
          <p>Der Atlas konnte nicht sicher geladen werden.</p>
          <div class="empty-actions">
            <button type="button" onclick={() => loadScene(retrySelection)}>Erneut laden</button>
            {#if retrySelection !== null}
              <button type="button" onclick={() => loadScene(null)}>Zur Projektübersicht</button>
            {/if}
          </div>
        </div>
      {:else if scene.kind === 'unavailable'}<div class="empty">
          <p>{scene.message}</p>
          <button type="button" onclick={() => loadScene(null)}>Übersicht laden</button>
        </div>
      {:else if scene.kind === 'available'}<MapAtlasCanvas
          scene={scene.value}
          selectedNodeId={selected?.nodeId ?? null}
          {lensModuleIds}
          onselect={selectNode}
          onopen={openNode}
        />{/if}
    </div>
    {#if inspectorOpen}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="inspector-resizer"
        role="separator"
        aria-label="Breite des Inspectors ändern"
        aria-orientation="vertical"
        aria-valuemin={INSPECTOR_MIN_WIDTH}
        aria-valuemax={inspectorMaxWidth}
        aria-valuenow={effectiveInspectorWidth}
        tabindex="0"
        onkeydown={resizeInspectorWithKeyboard}
        onpointerdown={startInspectorResize}
        onlostpointercapture={() => (resizingInspector = false)}
      ></div>
    {/if}
    <MapInspector
      onfunctionflow={onFunctionFlow
        ? () => {
            if (
              selected?.selection?.kind !== 'symbol' ||
              scene.kind !== 'available' ||
              ['queued', 'running', 'cancelling'].includes(indexActivityState)
            )
              return;
            onFunctionFlow?.({
              runId: scene.value.indexRunId,
              root: selected.selection.symbolId,
              callPath: [],
            });
          }
        : undefined}
      selected={inspectorMode === 'code' ? selected : null}
      {context}
      {inventory}
      {flow}
      {preview}
      onclose={() => {
        selected = null;
        inspectorMode = null;
      }}
      onopen={openNode}
      onselect={selectNode}
      oninventory={loadInventory}
      onflow={loadFlow}
      onevidence={openEvidence}
    />
    <DeepMapInspector
      open={inspectorMode === 'deepMap'}
      focusFailureEpoch={deepMapFailureFocusEpoch}
      runStartedEpoch={deepMapRunStartedEpoch}
      runsLoader={deepMapRunsLoader}
      entriesLoader={deepMapEntriesLoader}
      dashboardLoader={deepMapDashboardLoader}
      modulesLoader={deepMapModulesLoader}
      stepsLoader={deepMapStepsLoader}
      atlasImpactLoader={deepMapAtlasImpactLoader}
      cardLoader={deepMapCardLoader}
      {sourcePreviewLoader}
      onshowinatlas={showDeepMapModuleInAtlas}
      onclose={() => (inspectorMode = null)}
    />
  </main>

  <DeepMapDock
    statusLoader={deepMapStatusLoader}
    starter={deepMapStarter}
    pauser={deepMapPauser}
    resumer={deepMapResumer}
    canceller={deepMapCanceller}
    ondetails={toggleDeepMapInspector}
    onpublished={reloadPublished}
    onrunstarted={showStartedDeepMapRun}
  />
</section>

<style>
  .map-shell {
    --surface: var(--color-surface);
    --surface-raised: var(--color-surface-raised);
    --surface-canvas: var(--color-canvas-deep);
    --line: var(--color-border);
    --text: var(--color-text);
    --muted: var(--color-muted);
    --accent: var(--color-accent);
    --focus: var(--color-focus);
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    width: 100%;
    min-width: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    background: var(--surface-canvas);
    color: var(--text);
  }
  .command-bar {
    position: relative;
    z-index: 40;
    display: grid;
    grid-template-columns: minmax(210px, auto) minmax(260px, 1fr) auto;
    align-items: center;
    gap: 16px;
    min-height: 70px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .title > span {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    background: var(--color-surface-muted);
    font-weight: 800;
  }
  .title h2 {
    margin: 0;
    font-size: 1rem;
  }
  .title p {
    margin: 3px 0 0;
    color: var(--muted);
    font-size: 0.69rem;
  }
  .command-bar form {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    border: 1px solid var(--line);
    background: var(--surface-canvas);
  }
  .command-bar input {
    min-width: 0;
    min-height: 44px;
    border: 0;
    outline: 0;
    background: transparent;
    color: inherit;
  }
  .command-bar button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
    padding: 8px 12px;
  }
  .command-bar form button {
    border-width: 0 0 0 1px;
    background: var(--accent);
    color: var(--color-on-accent);
    font-weight: 750;
  }
  .command-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fast-index-control {
    position: relative;
  }
  .fast-index-control > button:disabled {
    color: var(--muted);
    cursor: wait;
  }
  .fast-index-error {
    position: absolute;
    z-index: 2;
    top: 50px;
    right: 0;
    width: 230px;
    padding: 8px 10px;
    border: 1px solid var(--color-danger);
    background: var(--surface);
    color: var(--color-danger);
    font-size: 0.7rem;
  }
  .lens {
    position: relative;
  }
  .lens > button.active {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  .lens-popover {
    position: absolute;
    top: 50px;
    right: 0;
    display: grid;
    gap: 9px;
    width: 320px;
    padding: 14px;
    border: 1px solid var(--line);
    background: var(--surface);
    box-shadow: 0 18px 45px color-mix(in srgb, var(--color-shadow) 35%, transparent);
  }
  .lens-popover label {
    display: grid;
    gap: 4px;
    color: var(--muted);
    font-size: 0.7rem;
  }
  .lens-popover select {
    min-height: 42px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-canvas);
    color: var(--text);
  }
  .search-results {
    position: absolute;
    z-index: 35;
    top: 70px;
    left: max(230px, 24%);
    width: min(620px, 64vw);
    max-height: 380px;
    overflow: auto;
    border: 1px solid var(--line);
    background: var(--surface);
    box-shadow: 0 18px 45px color-mix(in srgb, var(--color-shadow) 35%, transparent);
  }
  .search-results ul {
    margin: 0;
    padding: 5px;
    list-style: none;
  }
  .search-results button {
    display: grid;
    width: 100%;
    min-height: 58px;
    padding: 8px 10px;
    border: 0;
    border-bottom: 1px solid var(--line);
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .search-results span,
  .search-results small {
    color: var(--muted);
    font-size: 0.67rem;
  }
  .search-results p {
    padding: 12px;
  }
  .atlas-toolbar {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 14px;
    min-height: 48px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: 0.72rem;
  }
  .atlas-toolbar nav {
    display: flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
    overflow: hidden;
  }
  .atlas-toolbar nav button {
    min-height: 36px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
  }
  .atlas-toolbar nav button[aria-current='page'] {
    color: var(--text);
    font-weight: 750;
  }
  .scene-facts,
  .legend {
    display: flex;
    align-items: center;
    gap: 10px;
    white-space: nowrap;
  }
  .scene-facts span,
  .legend {
    color: var(--muted);
  }
  .scene-facts b {
    color: var(--color-warning);
  }
  .legend i {
    display: inline-block;
    margin-right: 4px;
    border: 1px solid var(--accent);
  }
  .legend .small {
    width: 8px;
    height: 8px;
  }
  .legend .large {
    width: 16px;
    height: 16px;
  }
  .legend em {
    font-style: normal;
  }
  .workspace-body {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .workspace-body.resizing {
    cursor: col-resize;
    user-select: none;
  }
  .workspace-body.resizing :global(.inspector) {
    transition: none;
  }
  .atlas-stage {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }
  .inspector-resizer {
    position: relative;
    z-index: 4;
    flex: 0 0 11px;
    width: 11px;
    min-height: 44px;
    margin-left: -5px;
    border: 0;
    outline: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }
  .inspector-resizer::after {
    position: absolute;
    inset: 0 auto 0 5px;
    width: 1px;
    background: var(--line);
    content: '';
  }
  .inspector-resizer:hover::after,
  .inspector-resizer:focus-visible::after,
  .workspace-body.resizing .inspector-resizer::after {
    width: 3px;
    margin-left: -1px;
    background: var(--focus);
  }
  .inspector-resizer:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  .empty {
    display: grid;
    place-content: center;
    height: 100%;
    padding: 30px;
    color: var(--muted);
    text-align: center;
  }
  .empty button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
  }
  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
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
  button:focus-visible,
  input:focus-visible,
  select:focus-visible,
  .lens-popover strong:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .command-bar {
      grid-template-columns: 1fr auto;
    }
    .command-bar form {
      grid-column: 1 / -1;
      grid-row: 2;
    }
    .title p {
      display: none;
    }
    .atlas-toolbar {
      grid-template-columns: 1fr auto;
    }
    .legend {
      display: none;
    }
    .workspace-body {
      overflow: visible;
    }
    .inspector-resizer {
      display: none;
    }
  }
  @media (max-width: 700px) {
    .scene-facts span {
      display: none;
    }
    .command-bar {
      padding: 7px 9px;
      gap: 8px;
    }
    .command-actions {
      gap: 4px;
    }
    .command-actions button {
      padding-inline: 8px;
    }
    .atlas-toolbar {
      gap: 5px;
    }
    .search-results {
      left: 8px;
      width: calc(100vw - 16px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    * {
      scroll-behavior: auto !important;
    }
  }
</style>

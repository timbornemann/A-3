<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import {
    queryAgentAskResearchDetail,
    queryAgentAskResearchSourcePreview,
    queryAgentAskResearchSources,
    queryAgentWorkTraceProjection,
    queryAgentWorkTraceSourcesV2,
    type AgentAskResearchDetailV1,
    type AgentWorkTracePresentationV1,
    type AgentWorkTraceSourceV2,
  } from './agent-ask-research';
  import type { ProjectMapSourcePreviewV1 } from './project-map-source-preview';

  interface Props {
    compact?: boolean;
    detailLoader?: typeof queryAgentAskResearchDetail;
    live?: boolean;
    mirror?: boolean;
    oncontinue?: () => void;
    onpresentationchange?: (presentation: AgentWorkTracePresentationV1) => void;
    onprojectionchange?: (projection: {
      detail: AgentAskResearchDetailV1;
      sources: AgentWorkTraceSourceV2[];
    }) => void;
    previewLoader?: typeof queryAgentAskResearchSourcePreview;
    presentation?: AgentWorkTracePresentationV1 | null;
    projectionLoader?: typeof queryAgentWorkTraceProjection;
    recentlyCompleted?: boolean;
    refreshKey: string;
    responseVisible?: boolean;
    sessionId: string;
    sourceRequest?: { label: string; nonce: number; userSequence: string } | null;
    sourcesLoader?: typeof queryAgentAskResearchSources;
    sourcesV2Loader?: typeof queryAgentWorkTraceSourcesV2;
    userSequence: string;
  }

  let {
    compact = false,
    detailLoader = queryAgentAskResearchDetail,
    live = false,
    mirror = false,
    oncontinue = () => {},
    onpresentationchange = () => {},
    onprojectionchange = () => {},
    previewLoader = queryAgentAskResearchSourcePreview,
    presentation = null,
    projectionLoader = queryAgentWorkTraceProjection,
    recentlyCompleted = false,
    refreshKey,
    responseVisible = false,
    sessionId,
    sourceRequest = null,
    sourcesLoader = queryAgentAskResearchSources,
    sourcesV2Loader = queryAgentWorkTraceSourcesV2,
    userSequence,
  }: Props = $props();
  let expanded = $state(false);
  let expansionInitialized = false;
  let detail = $state<AgentAskResearchDetailV1 | null>(null);
  let sources = $state<AgentWorkTraceSourceV2[]>([]);
  let loadState = $state<'loading' | 'available' | 'notRecorded' | 'missing' | 'error'>('loading');
  let sourceLoadState = $state<'loading' | 'available' | 'updating' | 'error'>('loading');
  let selectedSource = $state<string | null>(null);
  let preview = $state<ProjectMapSourcePreviewV1 | null>(null);
  let previewState = $state<'idle' | 'loading' | 'stale' | 'error'>('idle');
  let visibleStepCount = $state(0);
  let timelineOffset = $state(0);
  let reducedMotion = $state(false);
  let request = 0;
  let loadInFlight = false;
  let loadQueued = false;
  let destroyed = false;
  let activeTurnKey = '';
  let revealTarget = 0;
  let revealDeadline = 0;
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  let collapseTimer: ReturnType<typeof setTimeout> | null = null;
  let autoCollapseEligible = false;
  let autoCollapseDone = false;
  let terminalDisclosureOverride = false;
  let handledSourceRequest = 0;
  let additionalSourcesExpanded = $state(false);

  const REVEAL_WINDOW_MILLIS = 900;
  const MAX_REVEAL_INTERVAL_MILLIS = 180;
  const TERMINAL_DWELL_MILLIS = 700;

  type TimelineStep = AgentAskResearchDetailV1['steps'][number] & {
    id: string;
    section: string;
  };
  const timelineSteps = $derived(groupTimeline(detail?.steps ?? []));
  const latest = $derived(timelineSteps.at(-1) ?? null);
  const visibleSteps = $derived(timelineSteps.slice(0, visibleStepCount));
  const usedSources = $derived(sources.filter((source) => source.usedForAnswer));
  const additionalSources = $derived(sources.filter((source) => !source.usedForAnswer));
  const searchLimited = $derived(
    detail?.steps.some((step) => step.completeness === 'limited') ?? false,
  );
  const loadIdentity = $derived(
    `${sessionId}:${userSequence}:${refreshKey}:${live}:${recentlyCompleted}:${mirror}`,
  );
  const mirroredPresentation = $derived(mirror ? presentation : null);

  $effect(() => {
    void loadIdentity;
    // Only input changes may request a read. Publishing a projection must never
    // subscribe this effect to its own detail, preview or parent's presentation.
    untrack(() => {
      const turnKey = `${sessionId}:${userSequence}`;
      const isLive = live || recentlyCompleted;
      if (turnKey !== activeTurnKey) {
        activeTurnKey = turnKey;
        request += 1;
        resetPresentation();
        timelineOffset = 0;
        detail = null;
        sources = [];
        selectedSource = null;
        preview = null;
        previewState = 'idle';
        additionalSourcesExpanded = false;
        loadState = 'loading';
        sourceLoadState = 'loading';
        expansionInitialized = false;
        if (presentation?.detail.userSequence === userSequence) {
          restorePresentation(presentation);
          expansionInitialized = true;
        }
      }
      if (!expansionInitialized) {
        expanded = isLive;
        expansionInitialized = true;
      }
      if (isLive) autoCollapseEligible = true;
      if (mirror) {
        return;
      }
      requestResearchLoad();
    });
  });

  $effect(() => {
    const next = mirroredPresentation;
    if (next)
      untrack(() => {
        detail = next.detail;
        loadState = next.loadState;
        sourceLoadState = next.sourceLoadState;
        sources = next.sources;
        visibleStepCount = next.visibleStepCount;
      });
  });

  $effect(() => {
    if (mirror) return;
    if (!sourceRequest || sourceRequest.userSequence !== userSequence) return;
    if (sourceRequest.nonce === handledSourceRequest) return;
    const source = sources.find((candidate) => candidate.referenceLabel === sourceRequest.label);
    if (!source) return;
    handledSourceRequest = sourceRequest.nonce;
    untrack(() => {
      expanded = true;
      terminalDisclosureOverride = true;
      clearCollapseTimer();
      publishPresentation();
      void showSource(source.sourceRef);
    });
  });

  $effect(() => {
    void responseVisible;
    observeTerminalState();
  });

  onMount(() => {
    if (typeof window.matchMedia !== 'function') return;
    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = (): void => {
      reducedMotion = query.matches;
      if (reducedMotion && detail) synchronizeVisibleSteps(timelineSteps.length);
    };
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  });

  onDestroy(() => {
    destroyed = true;
    request += 1;
    loadQueued = false;
    clearRevealTimer();
    clearCollapseTimer();
  });

  function requestResearchLoad(): void {
    if (destroyed) return;
    loadQueued = true;
    if (!loadInFlight) void drainResearchLoads();
  }

  async function drainResearchLoads(): Promise<void> {
    loadInFlight = true;
    while (loadQueued && !destroyed) {
      loadQueued = false;
      await loadResearch(request);
    }
    loadInFlight = false;
  }

  async function loadResearch(current: number): Promise<void> {
    const requestedSessionId = sessionId;
    const requestedUserSequence = userSequence;
    if (detail === null) loadState = 'loading';
    try {
      const legacyLoaderWasProvided =
        detailLoader !== queryAgentAskResearchDetail ||
        sourcesLoader !== queryAgentAskResearchSources;
      const projectionLoaderWasProvided = projectionLoader !== queryAgentWorkTraceProjection;
      if (legacyLoaderWasProvided && !projectionLoaderWasProvided) {
        await loadLegacyResearch(current, requestedSessionId, requestedUserSequence);
        return;
      }
      let response;
      try {
        response = await projectionLoader(requestedSessionId, requestedUserSequence);
      } catch {
        if (current !== request) return;
        await loadLegacyResearch(current, requestedSessionId, requestedUserSequence);
        return;
      }
      if (
        current !== request ||
        requestedSessionId !== sessionId ||
        requestedUserSequence !== userSequence
      )
        return;
      if (response.result.status === 'updating') {
        if (hasLoadedProjection()) return;
        sourceLoadState = 'updating';
        return;
      }
      if (response.result.status === 'notRecorded') {
        if (hasLoadedProjection()) return;
        loadState = 'notRecorded';
        return;
      }
      if (response.result.status !== 'available') {
        if (hasLoadedProjection()) return;
        loadState = 'missing';
        return;
      }
      const nextDetail = response.result.detail;
      if (windowAdvance(nextDetail.steps) === null) {
        return;
      }
      const hadLoadedProjection = hasLoadedProjection();
      const found = [...response.result.sources];
      let cursor = response.result.nextCursor;
      while (cursor !== null && found.length < 200) {
        let page: Awaited<ReturnType<typeof sourcesV2Loader>>;
        try {
          page = await sourcesV2Loader(
            requestedSessionId,
            requestedUserSequence,
            response.result.projectionRef,
            cursor,
          );
        } catch {
          if (current === request && !hadLoadedProjection)
            commitProjection(nextDetail, found, 'error');
          return;
        }
        if (current !== request) return;
        if (page.result.status === 'projectionChanged') {
          if (!hadLoadedProjection) sourceLoadState = 'updating';
          return;
        }
        if (page.result.status !== 'available') {
          if (!hadLoadedProjection) {
            commitProjection(nextDetail, found, 'error');
          }
          return;
        }
        found.push(...page.result.sources);
        cursor = page.result.nextCursor;
      }
      commitProjection(nextDetail, found, 'available');
    } catch {
      if (current === request) {
        if (detail === null) loadState = 'error';
        else {
          sourceLoadState = 'error';
          publishPresentation();
        }
      }
    }
  }

  async function loadLegacyResearch(
    current: number,
    requestedSessionId: string,
    requestedUserSequence: string,
  ): Promise<void> {
    const response = await detailLoader(requestedSessionId, requestedUserSequence);
    if (current !== request) return;
    if (response.result.status === 'notRecorded') {
      if (hasLoadedProjection()) return;
      loadState = 'notRecorded';
      return;
    }
    if (response.result.status !== 'available') {
      if (hasLoadedProjection()) return;
      loadState = 'missing';
      return;
    }
    const nextDetail = response.result.detail;
    if (windowAdvance(nextDetail.steps) === null) {
      return;
    }
    const hadLoadedProjection = hasLoadedProjection();
    const found: AgentWorkTraceSourceV2[] = [];
    let cursor: string | null = null;
    do {
      let page: Awaited<ReturnType<typeof sourcesLoader>>;
      try {
        page = await sourcesLoader(requestedSessionId, requestedUserSequence, cursor);
      } catch {
        if (current === request && !hadLoadedProjection)
          commitProjection(nextDetail, found, 'error');
        return;
      }
      if (current !== request) return;
      if (page.result.status !== 'available') {
        if (!hadLoadedProjection) commitProjection(nextDetail, found, 'error');
        return;
      }
      for (const source of page.result.sources) {
        found.push({ ...source, referenceLabel: `S${found.length + 1}` });
      }
      cursor = page.result.nextCursor;
    } while (cursor !== null && found.length < 200);
    commitProjection(nextDetail, found, 'available');
  }

  function isAppendOnlyUpdate(previous: TimelineStep[], next: TimelineStep[]): boolean {
    if (next.length < previous.length) return false;
    return previous.every((step, index) => step.id === next[index]?.id);
  }

  function windowAdvance(next: AgentAskResearchDetailV1['steps']): number | null {
    if (!detail || isAppendOnlyUpdate(timelineSteps, groupTimeline(next))) return 0;
    // V35 retains the entire journal, but projects only its latest 64 events.
    // A matching suffix is forward progress, not a regressing/truncated poll.
    if (next.length !== 64 || detail.steps.length === 0) return null;
    const identity = (step: AgentAskResearchDetailV1['steps'][number]): string =>
      JSON.stringify([
        step.occurredAtUnixMillis,
        step.phase,
        step.state,
        step.action,
        step.query,
        step.completeness,
        step.note && { ...step.note, sourceRefs: step.note.sourceRefs.length },
      ]);
    for (let dropped = 1; dropped < detail.steps.length; dropped += 1) {
      if (
        detail.steps.slice(dropped).every((step, index) => identity(step) === identity(next[index]))
      )
        return dropped;
    }
    // Polling can miss a complete window. Accept only an entirely later tail;
    // never use presentation timestamps as execution or evidence authority.
    const previousLast = detail.steps.at(-1);
    if (
      previousLast &&
      next.every(
        (step) => BigInt(step.occurredAtUnixMillis) > BigInt(previousLast.occurredAtUnixMillis),
      )
    )
      return detail.steps.length;
    return null;
  }

  function hasLoadedProjection(): boolean {
    return detail !== null && loadState === 'available';
  }

  function commitProjection(
    nextDetail: AgentAskResearchDetailV1,
    nextSources: AgentWorkTraceSourceV2[],
    nextSourceLoadState: 'available' | 'error',
  ): void {
    const advance = windowAdvance(nextDetail.steps);
    if (advance === null) return;
    const detailChanged = JSON.stringify(detail) !== JSON.stringify(nextDetail);
    const sourcesChanged = JSON.stringify(sources) !== JSON.stringify(nextSources);
    const selectedLabel = sources.find(
      (source) => source.sourceRef === selectedSource,
    )?.referenceLabel;
    if (detailChanged) {
      if (advance > 0 && detail) {
        const removed = timelineSteps.length - groupTimeline(detail.steps.slice(advance)).length;
        timelineOffset += advance;
        visibleStepCount = Math.max(0, visibleStepCount - removed);
        revealTarget = Math.max(0, revealTarget - removed);
      }
      detail = nextDetail;
    }
    if (sourcesChanged) {
      sources = nextSources;
      // Opaque action references rotate with the trace revision; S labels retain
      // their turn-local identity and must not remount controls or lose focus.
      if (selectedLabel)
        selectedSource =
          sources.find((source) => source.referenceLabel === selectedLabel)?.sourceRef ?? null;
    }
    if (nextDetail.stale && preview) {
      preview = null;
      previewState = 'stale';
    }
    loadState = 'available';
    sourceLoadState = nextSourceLoadState;
    synchronizeVisibleSteps(groupTimeline(nextDetail.steps).length);
    publishPresentation();
    if (detailChanged || sourcesChanged)
      onprojectionchange({ detail: detail ?? nextDetail, sources });
  }

  function groupTimeline(steps: AgentAskResearchDetailV1['steps']): TimelineStep[] {
    const grouped: TimelineStep[] = [];
    let round = 0;
    for (const [index, step] of steps.entries()) {
      if (step.phase === 'deciding') round += 1;
      const section =
        step.phase === 'answeringOrPlanning' || step.phase === 'completed'
          ? 'Abschluss'
          : round > 0
            ? `Recherche-Runde ${round}`
            : 'Vorbereitung';
      const previous = grouped.at(-1);
      const mergePreparation =
        section === 'Vorbereitung' &&
        previous?.section === section &&
        previous.phase === step.phase &&
        (step.phase === 'preparing' || step.phase === 'locating');
      if (mergePreparation && previous) {
        grouped[grouped.length - 1] = { ...step, id: previous.id, section };
      } else {
        grouped.push({
          ...step,
          id: `${timelineOffset + index}:${step.occurredAtUnixMillis}:${step.phase}`,
          section,
        });
      }
    }
    return grouped;
  }

  function synchronizeVisibleSteps(stepCount: number): void {
    if (stepCount > revealTarget) revealDeadline = performance.now() + REVEAL_WINDOW_MILLIS;
    revealTarget = stepCount;
    if (stepCount === 0) {
      visibleStepCount = 0;
      clearRevealTimer();
      return;
    }
    if (!(live || recentlyCompleted) || reducedMotion) {
      clearRevealTimer();
      visibleStepCount = stepCount;
      observeTerminalState();
      publishPresentation();
      return;
    }
    if (visibleStepCount === 0) {
      visibleStepCount = 1;
      publishPresentation();
    }
    if (visibleStepCount < revealTarget) scheduleRemainingSteps();
    else observeTerminalState();
  }

  function scheduleRemainingSteps(): void {
    // An unchanged poll or an appended batch must not postpone a pending reveal.
    if (revealTimer !== null) return;
    const remaining = revealTarget - visibleStepCount;
    if (remaining <= 0) {
      observeTerminalState();
      return;
    }
    const interval = Math.min(
      MAX_REVEAL_INTERVAL_MILLIS,
      Math.max(1, Math.floor((revealDeadline - performance.now()) / remaining)),
    );
    const revealNext = (): void => {
      revealTimer = null;
      visibleStepCount = Math.min(visibleStepCount + 1, revealTarget);
      publishPresentation();
      if (visibleStepCount < revealTarget) scheduleRemainingSteps();
      else observeTerminalState();
    };
    revealTimer = setTimeout(revealNext, interval);
  }

  function observeTerminalState(): void {
    if (
      !detail ||
      visibleStepCount < timelineSteps.length ||
      !isSuccessfulTerminal(timelineSteps.at(-1))
    )
      return;
    if (
      !autoCollapseEligible ||
      !responseVisible ||
      autoCollapseDone ||
      terminalDisclosureOverride ||
      collapseTimer !== null
    )
      return;
    collapseTimer = setTimeout(() => {
      collapseTimer = null;
      if (!terminalDisclosureOverride) expanded = false;
      autoCollapseDone = true;
      publishPresentation();
    }, TERMINAL_DWELL_MILLIS);
  }

  function isTerminal(step: AgentAskResearchDetailV1['steps'][number] | undefined): boolean {
    return step !== undefined && (step.state !== 'running' || step.phase === 'completed');
  }

  function isSuccessfulTerminal(
    step: AgentAskResearchDetailV1['steps'][number] | undefined,
  ): boolean {
    return step !== undefined && step.state === 'completed' && step.phase === 'completed';
  }

  function handleDisclosureClick(event: MouseEvent): void {
    event.preventDefault();
    expanded = !expanded;
    terminalDisclosureOverride = true;
    clearCollapseTimer();
    publishPresentation();
  }

  function resetPresentation(): void {
    clearRevealTimer();
    clearCollapseTimer();
    visibleStepCount = 0;
    revealTarget = 0;
    revealDeadline = 0;
    autoCollapseEligible = false;
    autoCollapseDone = false;
    terminalDisclosureOverride = false;
  }

  function clearRevealTimer(): void {
    if (revealTimer === null) return;
    clearTimeout(revealTimer);
    revealTimer = null;
  }

  function clearCollapseTimer(): void {
    if (collapseTimer === null) return;
    clearTimeout(collapseTimer);
    collapseTimer = null;
  }

  function publishPresentation(): void {
    if (destroyed || mirror || !detail || loadState !== 'available') return;
    onpresentationchange({
      additionalSourcesExpanded,
      autoCollapseDone,
      disclosureOverride: terminalDisclosureOverride,
      detail,
      expanded,
      loadState: 'available',
      preview,
      previewState,
      selectedSource,
      sourceLoadState,
      sources,
      visibleStepCount,
    });
  }

  function restorePresentation(next: AgentWorkTracePresentationV1): void {
    additionalSourcesExpanded = next.additionalSourcesExpanded ?? false;
    autoCollapseDone = next.autoCollapseDone ?? false;
    terminalDisclosureOverride = next.disclosureOverride ?? false;
    detail = next.detail;
    expanded = next.expanded;
    loadState = next.loadState;
    preview = next.preview;
    previewState = next.previewState;
    selectedSource = next.selectedSource;
    sourceLoadState = next.sourceLoadState;
    sources = next.sources;
    visibleStepCount = next.visibleStepCount;
  }

  async function showSource(sourceRef: string): Promise<void> {
    const current = request;
    const label = sourceForRef(sourceRef)?.referenceLabel;
    selectedSource = sourceRef;
    preview = null;
    previewState = 'loading';
    publishPresentation();
    try {
      const response = await previewLoader(sessionId, userSequence, sourceRef);
      if (
        current !== request ||
        !label ||
        sourceForRef(selectedSource ?? '')?.referenceLabel !== label
      )
        return;
      if (detail?.stale) {
        previewState = 'stale';
        publishPresentation();
        return;
      }
      if (response.result.status === 'available') {
        preview = response.result.preview;
        previewState = 'idle';
      } else if (response.result.status === 'stale') previewState = 'stale';
      else previewState = 'error';
    } catch {
      if (
        current === request &&
        label &&
        sourceForRef(selectedSource ?? '')?.referenceLabel === label
      )
        previewState = 'error';
    }
    publishPresentation();
  }

  function phaseLabel(phase: string): string {
    return (
      {
        answeringOrPlanning: 'Ergebnis wird formuliert',
        completed: 'Recherche abgeschlossen',
        deciding: 'Nächster Schritt wird gewählt',
        evaluating: 'Befunde werden ausgewertet',
        locating: 'Suchraum wird lokalisiert',
        preparing: 'Projektstand wird gebunden',
        reading: 'Quellen werden geprüft',
      }[phase] ?? phase
    );
  }

  function stepLabel(phase: string, state: string): string {
    if (state === 'failed') return 'Recherche fehlgeschlagen';
    if (state === 'cancelled') return 'Recherche abgebrochen';
    if (state === 'awaitingContinuation') return 'Fortsetzung erforderlich';
    return phaseLabel(phase);
  }

  function modeLabel(mode: AgentAskResearchDetailV1['mode']): string {
    return { agent: 'Agent-Vorbereitung', ask: 'Ask-Recherche', plan: 'Plan-Recherche' }[mode];
  }

  function depthLabel(depth: AgentAskResearchDetailV1['depth']): string {
    return depth === 'thorough' ? 'Gründlich' : 'Standard';
  }

  function findingKindLabel(
    kind: NonNullable<AgentAskResearchDetailV1['steps'][number]['note']>['findingKind'],
  ): string {
    return {
      conclusion: 'Belegte Schlussfolgerung',
      hypothesis: 'Hypothese · noch unbelegt',
      observation: 'Beobachtung',
    }[kind];
  }

  function sourceForRef(sourceRef: string): AgentWorkTraceSourceV2 | undefined {
    return sources.find((source) => source.sourceRef === sourceRef);
  }

  function reasonLabel(reason: AgentWorkTraceSourceV2['reason']): string {
    return {
      exactNameOrPath: 'Exakter Name oder Pfad',
      indexedText: 'Indexierter Text',
      relationship: 'Beziehung',
      semanticCandidate: 'Semantischer Kandidat, aktuell geprüft',
      sourceText: 'Treffer im aktuellen Quelltext',
      test: 'Zugehöriger Test',
      verifiedModuleKnowledge: 'Verifiziertes Modulwissen',
    }[reason];
  }

  type PresentationState = 'active' | 'awaiting' | 'cancelled' | 'completed' | 'failed';

  function presentationState(index: number): PresentationState {
    const step = timelineSteps[index];
    if (!step) return 'completed';
    if (step.state === 'failed') return 'failed';
    if (step.state === 'cancelled') return 'cancelled';
    if (step.state === 'awaitingContinuation') return 'awaiting';
    if (index < visibleStepCount - 1) return 'completed';
    if (step.state === 'completed' || step.phase === 'completed') return 'completed';
    return 'active';
  }

  function presentationLabel(state: PresentationState, terminal: boolean): string {
    if (state === 'active') return 'In Arbeit';
    if (state === 'failed') return 'Fehlgeschlagen';
    if (state === 'cancelled') return 'Abgebrochen';
    if (state === 'awaiting') return 'Fortsetzung nötig';
    return terminal ? 'Abgeschlossen' : 'Erledigt';
  }

  function markerLabel(state: PresentationState): string {
    if (state === 'completed') return '✓';
    if (state === 'failed') return '!';
    if (state === 'cancelled') return '–';
    if (state === 'awaiting') return '…';
    return '';
  }

  function sourceLocation(source: AgentWorkTraceSourceV2, short = false): string {
    const path = short ? (source.path.split(/[\\/]/u).at(-1) ?? source.path) : source.path;
    if (source.startLine === null) return path;
    if (source.endLine === null || source.endLine === source.startLine)
      return `${path}:${source.startLine}`;
    return `${path}:${source.startLine}–${source.endLine}`;
  }

  function sourceAccessibleLabel(source: AgentWorkTraceSourceV2): string {
    const symbol = source.symbol ? `, Symbol ${source.symbol}` : '';
    return `Quelle ${source.referenceLabel}: ${sourceLocation(source)}${symbol}, ${reasonLabel(source.reason)} öffnen`;
  }
</script>

<details class:compact class="ask-research" data-live={live} bind:open={expanded}>
  <summary onclick={handleDisclosureClick}>
    <span class:live-dot={live}></span>
    <span>
      <strong>{live ? 'A^3 arbeitet' : 'Recherche & Quellen'}</strong>
      <small
        >{latest
          ? `${stepLabel(latest.phase, latest.state)} · ${latest.action}`
          : live
            ? 'Projektstand und Recherche werden vorbereitet'
            : 'Rechercheweg laden'}</small
      >
    </span>
    {#if detail}<em
        >{modeLabel(detail.mode)} · {depthLabel(detail.depth)} · {detail.sourceCount} Quellen</em
      >{/if}
  </summary>
  <div class="research-body">
    {#if loadState === 'loading'}
      <p role="status">Rechercheweg wird geladen …</p>
    {:else if loadState === 'notRecorded'}
      {#if live}
        <p role="status">Der Rechercheweg wird vorbereitet …</p>
      {:else}
        <p>Der Rechercheweg wurde bei dieser älteren Antwort noch nicht aufgezeichnet.</p>
      {/if}
    {:else if loadState === 'missing'}
      <p>Für diesen Beitrag existiert kein aufgezeichneter Arbeitsweg.</p>
    {:else if loadState === 'error'}
      <p role="alert">Die Rechercheinformationen konnten gerade nicht geladen werden.</p>
      <button type="button" onclick={requestResearchLoad}>Erneut laden</button>
    {:else if detail}
      {#if detail.stale}
        <p class="stale-note">
          Diese Recherche beschreibt einen älteren Projektstand. Metadaten bleiben sichtbar;
          Quelltextvorschauen sind gesperrt.
        </p>
      {/if}
      {#if !mirror && (live || recentlyCompleted)}
        <p class="sr-only" role="status" aria-live="polite" aria-atomic="true">
          {latest ? `${stepLabel(latest.phase, latest.state)}: ${latest.action}` : ''}
        </p>
      {/if}
      {#if detail.researchWork}
        <section class="research-checklist" aria-label="Recherche-Prüfstand">
          <h4>Was noch zu klären ist</h4>
          <ul>
            {#each detail.researchWork.questions as question (question.id)}
              <li data-question-id={question.id} data-question-status={question.status}>
                <strong>{question.outcome}</strong>
                <small
                  >{{ required: 'Erforderlich', supporting: 'Grundlage', optional: 'Zusatzdetail' }[
                    question.priority
                  ]} · {{
                    open: 'Offen',
                    active: 'Wird geprüft',
                    answered: 'Beantwortet',
                    limited: 'Mit Einschränkung beantwortet',
                    blocked: 'Noch unbeantwortet',
                    stale: 'Erneut zu prüfen',
                  }[question.status]}</small
                >
                {#if question.result}
                  <details>
                    <summary
                      >{question.status === 'stale'
                        ? 'Früheres Ergebnis'
                        : 'Ergebnis und Belege'}</summary
                    >
                    <p>{question.result}</p>
                    <small
                      >{question.resultKind === 'designDecision'
                        ? 'Vorgeschlagene Gestaltung'
                        : question.resultKind === 'boundedUnknown'
                          ? 'Begrenzte Aussage'
                          : 'Quellengestützte Interpretation'}</small
                    >
                    {#each question.sourceRefs as ref (ref)}
                      <button type="button" disabled={detail.stale} onclick={() => showSource(ref)}
                        >Beleg {sourceForRef(ref)?.referenceLabel ?? ''} öffnen</button
                      >
                    {/each}
                  </details>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      <ol class="research-steps" aria-label="Rechercheverlauf">
        {#each visibleSteps as step, index (step.id)}
          {@const displayState = presentationState(index)}
          {@const terminalStep = index === timelineSteps.length - 1 && isTerminal(step)}
          <li
            class:active={displayState === 'active'}
            class:cancelled={displayState === 'cancelled'}
            class:completed={displayState === 'completed'}
            class:failed={displayState === 'failed'}
            class:awaiting={displayState === 'awaiting'}
            class:animate={(live || recentlyCompleted) && !reducedMotion}
            aria-current={displayState === 'active' ? 'step' : undefined}
            data-step-state={displayState}
          >
            {#if index === 0 || visibleSteps[index - 1]?.section !== step.section}
              <div class="timeline-section">{step.section}</div>
            {/if}
            <span class="step-rail" aria-hidden="true">
              <span class="step-marker">{markerLabel(displayState)}</span>
            </span>
            <div class="step-content">
              <div class="step-heading">
                <strong>{stepLabel(step.phase, step.state)}</strong>
                <small class="step-state">{presentationLabel(displayState, terminalStep)}</small>
              </div>
              <p>{step.action}</p>
              {#if step.note}
                <section class="work-note" aria-label="Öffentliche Arbeitsnotiz">
                  <div>
                    <span>Ziel</span>
                    <p>{step.note.goal}</p>
                  </div>
                  <div>
                    <span>{findingKindLabel(step.note.findingKind)}</span>
                    <p>{step.note.finding}</p>
                  </div>
                  <div>
                    <span>Offene Evidenzlücke</span>
                    <p>{step.note.gap}</p>
                  </div>
                  <div>
                    <span>Nächster Schritt</span>
                    <p>{step.note.nextStep}</p>
                  </div>
                  {#if step.note.sourceRefs.length > 0}
                    <div class="note-sources">
                      <span>Quellen dieses Befunds</span>
                      <div>
                        {#each step.note.sourceRefs as sourceRef, sourceIndex (sourceIndex)}
                          {@const noteSource = sourceForRef(sourceRef)}
                          <button
                            type="button"
                            disabled={!noteSource}
                            title={noteSource ? sourceAccessibleLabel(noteSource) : undefined}
                            aria-label={noteSource ? sourceAccessibleLabel(noteSource) : undefined}
                            onclick={() => void showSource(sourceRef)}
                          >
                            {noteSource
                              ? `【${noteSource.referenceLabel}】 ${sourceLocation(noteSource, true)}`
                              : 'Quelle wird geladen …'}
                          </button>
                        {/each}
                      </div>
                    </div>
                  {/if}
                </section>
              {/if}
              {#if step.query}<code>{step.query}</code
                >{/if}{#if step.completeness !== 'notApplicable'}<small class="completeness"
                  >{step.completeness === 'complete' ? 'Vollständig' : 'Begrenzt'}</small
                >{/if}
            </div>
          </li>
        {/each}
      </ol>
      {#if latest?.state === 'awaitingContinuation'}
        <section class="continuation-card">
          <strong>Für eine belastbare Antwort ist weitere Recherche nötig.</strong>
          <p>
            Die bisherigen Befunde und Quellen bleiben erhalten. Eine Fortsetzung bindet den dann
            aktuellen Projektstand neu.
          </p>
          <button type="button" onclick={oncontinue}>Recherche fortsetzen</button>
        </section>
      {/if}
      {#if searchLimited}<p class="limited">
          Mindestens eine Suche wurde durch eine feste Sicherheits- oder Ressourcengrenze beendet.
          Sie beweist nicht, dass es keine weiteren Treffer gibt.
        </p>{/if}
      <section class="sources-section">
        <h4>Quellen</h4>
        {#if sourceLoadState === 'updating'}
          <p role="status">
            Die Quellen werden gerade mit dem neuesten Recherchestand abgeglichen …
          </p>
        {:else if sourceLoadState === 'error' && detail.sourceCount > 0}
          <div class="source-retry" role="alert">
            <p>
              {detail.sourceCount} Quellen wurden gefunden, ihre Details konnten noch nicht vollständig
              geladen werden.
            </p>
            <button type="button" onclick={requestResearchLoad}>Erneut laden</button>
          </div>
        {:else if sourceLoadState === 'available' && detail.sourceCount === 0}
          <p>Noch keine zitierbare aktuelle Quelle gefunden.</p>
        {/if}
        {#if usedSources.length > 0}
          <div class="source-group-heading">
            <h5>Für die Antwort verwendet</h5>
            <span>{usedSources.length}</span>
          </div>
          <ul class="source-list">
            {#each usedSources as source (source.referenceLabel)}
              <li>
                <button
                  type="button"
                  title={sourceAccessibleLabel(source)}
                  aria-label={sourceAccessibleLabel(source)}
                  onclick={() => void showSource(source.sourceRef)}
                  aria-pressed={selectedSource === source.sourceRef}
                >
                  <strong>【{source.referenceLabel}】</strong>
                  <span class="source-location">{sourceLocation(source, true)}</span>
                  <small
                    >{source.symbol ? `${source.symbol} · ` : ''}{reasonLabel(source.reason)}</small
                  >
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if additionalSources.length > 0}
          <button
            class="additional-sources-toggle"
            type="button"
            aria-expanded={additionalSourcesExpanded}
            aria-label={`Zusätzlich gefunden: ${additionalSources.length} Quellen ${additionalSourcesExpanded ? 'einklappen' : 'anzeigen'}`}
            onclick={() => {
              additionalSourcesExpanded = !additionalSourcesExpanded;
              publishPresentation();
            }}
          >
            <span>Zusätzlich gefunden</span><small>{additionalSources.length}</small>
          </button>
          {#if additionalSourcesExpanded}
            <ul class="source-list additional-source-list">
              {#each additionalSources as source (source.referenceLabel)}
                <li>
                  <button
                    type="button"
                    title={sourceAccessibleLabel(source)}
                    aria-label={sourceAccessibleLabel(source)}
                    onclick={() => void showSource(source.sourceRef)}
                    aria-pressed={selectedSource === source.sourceRef}
                  >
                    <strong>【{source.referenceLabel}】</strong>
                    <span class="source-location">{sourceLocation(source, true)}</span>
                    <small
                      >{source.symbol ? `${source.symbol} · ` : ''}{reasonLabel(
                        source.reason,
                      )}</small
                    >
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>
      {#if previewState === 'loading'}<p role="status">Sicherer Quellenausschnitt wird geladen …</p>
      {:else if previewState === 'stale'}<p>
          Der Quellenausschnitt ist für diesen älteren Projektstand gesperrt.
        </p>
      {:else if previewState === 'error'}<p role="alert">
          Der sichere Quellenausschnitt ist nicht mehr verfügbar.
        </p>
      {:else if preview}
        <section class="source-preview">
          <header>
            <strong>{preview.pathDisplay}</strong><span>ab Zeile {preview.startLine}</span>
          </header>
          <pre><code>{preview.text}</code></pre>
        </section>
      {/if}
    {/if}
  </div>
</details>

<style>
  .ask-research {
    margin: var(--space-3) 0 var(--space-5);
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
  }
  summary {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    min-height: 3.5rem;
    padding: var(--space-2) var(--space-3);
    gap: var(--space-2);
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary > span:nth-child(2) {
    display: grid;
    gap: 0.15rem;
  }
  summary small,
  summary em {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    font-style: normal;
  }
  .live-dot,
  summary > span:first-child {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    background: var(--color-border-strong);
  }
  .live-dot {
    background: var(--color-status-pending);
    box-shadow: 0 0 0 4px var(--color-status-pending-ring);
  }
  .research-body {
    display: grid;
    padding: 0 var(--space-3) var(--space-3);
    gap: var(--space-3);
  }
  .research-body p {
    margin: 0;
    color: var(--color-muted);
    line-height: 1.45;
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
  code {
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .completeness {
    width: fit-content;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    color: var(--color-muted);
    background: var(--color-surface-subtle);
    font-size: var(--font-size-xs);
  }
  .limited,
  .stale-note {
    padding: var(--space-2);
    border-inline-start: 3px solid var(--color-warning);
    background: var(--color-surface);
  }
  .research-steps {
    display: grid;
    padding: 0;
    margin: 0;
    gap: var(--space-2);
    list-style: none;
  }
  .research-steps {
    gap: var(--space-1);
  }
  .research-steps li {
    display: grid;
    position: relative;
    grid-template-columns: 1.35rem minmax(0, 1fr);
    min-width: 0;
    padding-bottom: var(--space-2);
    gap: var(--space-2);
  }
  .timeline-section {
    grid-column: 1 / -1;
    margin-top: var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .research-steps li.animate {
    animation: research-step-in 180ms ease-out both;
  }
  .step-rail {
    display: flex;
    position: relative;
    justify-content: center;
    padding-top: 0.1rem;
  }
  .step-rail::after {
    position: absolute;
    z-index: 0;
    top: 1.15rem;
    bottom: calc(-1 * var(--space-1));
    left: calc(50% - 1px);
    width: 2px;
    background: var(--color-border-soft);
    content: '';
  }
  .research-steps li:last-child .step-rail::after {
    display: none;
  }
  .research-steps li.completed .step-rail::after {
    background: var(--color-status-ready);
  }
  .step-marker {
    display: grid;
    position: relative;
    z-index: 1;
    place-items: center;
    width: 1rem;
    height: 1rem;
    border: 2px solid var(--color-border-strong);
    border-radius: 50%;
    color: var(--color-surface);
    background: var(--color-surface);
    font-size: 0.68rem;
    font-weight: 700;
    line-height: 1;
  }
  .research-steps li.completed .step-marker {
    border-color: var(--color-status-ready);
    background: var(--color-status-ready);
  }
  .research-steps li.active .step-marker {
    border-color: var(--color-status-pending);
    box-shadow: 0 0 0 4px var(--color-status-pending-ring);
    animation: research-active-pulse 1.4s ease-in-out infinite;
  }
  .research-steps li.active .step-marker::before {
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 50%;
    background: var(--color-status-pending);
    content: '';
  }
  .research-steps li.failed .step-marker {
    border-color: var(--color-status-failed);
    background: var(--color-status-failed);
  }
  .research-steps li.cancelled .step-marker {
    border-color: var(--color-warning-strong);
    background: var(--color-warning-strong);
  }
  .research-steps li.awaiting .step-marker {
    border-color: var(--color-warning-strong);
    color: var(--color-surface);
    background: var(--color-warning-strong);
  }
  .step-content {
    display: grid;
    min-width: 0;
    gap: 0.15rem;
  }
  .step-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .step-heading strong {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .step-state {
    flex: 0 0 auto;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .research-steps li.active .step-state {
    color: var(--color-status-pending);
  }
  .research-steps li.completed .step-state {
    color: var(--color-status-ready);
  }
  .research-steps li.failed .step-state {
    color: var(--color-status-failed);
  }
  .research-steps li.cancelled .step-state {
    color: var(--color-warning);
  }
  .research-steps li.awaiting .step-state {
    color: var(--color-warning);
  }
  .work-note {
    display: grid;
    margin-top: var(--space-2);
    padding: var(--space-2);
    gap: var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    border-inline-start: 1px solid var(--color-border);
  }
  .work-note > div {
    display: grid;
    gap: 0.1rem;
  }
  .work-note span {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    font-weight: 600;
  }
  .note-sources > div {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .note-sources button,
  .continuation-card button {
    min-height: var(--control-min-size);
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
    font-size: var(--font-size-xs);
  }
  .continuation-card {
    display: grid;
    padding: var(--space-3);
    gap: var(--space-2);
    border-inline-start: 3px solid var(--color-warning);
    background: var(--color-surface);
  }
  .continuation-card button {
    width: fit-content;
    min-height: var(--control-min-size);
    padding: 0 var(--space-2);
    color: var(--color-heading);
    background: var(--color-surface-subtle);
  }
  .research-steps p {
    margin-top: 0.15rem;
    font-size: var(--font-size-xs);
  }
  h4,
  h5 {
    margin: 0 0 var(--space-2);
  }
  h5 {
    margin: 0;
    color: var(--color-heading);
    font-size: var(--font-size-xs);
  }
  .source-retry {
    display: grid;
    padding: var(--space-2);
    gap: var(--space-2);
    border-inline-start: 3px solid var(--color-warning);
    background: var(--color-surface);
  }
  .source-retry button {
    width: fit-content;
  }
  .sources-section {
    display: grid;
    min-width: 0;
    gap: var(--space-2);
  }
  .source-group-heading {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .source-group-heading > span,
  .additional-sources-toggle small {
    min-width: 1.35rem;
    padding: 0.05rem 0.35rem;
    border-radius: 999px;
    color: var(--color-muted);
    background: var(--color-surface-muted);
    font-size: 0.68rem;
    line-height: 1.25;
    text-align: center;
  }
  .source-list {
    display: flex;
    flex-wrap: wrap;
    min-width: 0;
    padding: 0;
    margin: 0;
    gap: 0.35rem;
    list-style: none;
  }
  .source-list li {
    min-width: 0;
    max-width: 100%;
  }
  .source-list button {
    display: inline-flex;
    align-items: baseline;
    width: auto;
    max-width: 100%;
    min-height: var(--control-min-size);
    padding: 0.2rem 0.45rem;
    gap: 0.3rem;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    color: var(--color-muted);
    text-align: start;
    background: transparent;
    cursor: pointer;
    font-size: var(--font-size-xs);
  }
  .source-list button[aria-pressed='true'] {
    border-color: var(--color-accent);
    background: var(--color-accent-surface);
  }
  .source-list strong {
    flex: 0 0 auto;
    color: var(--color-heading);
    font-size: 0.72rem;
    font-weight: 600;
  }
  .source-location {
    min-width: 0;
    max-width: 16rem;
    overflow: hidden;
    color: var(--color-text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .source-list small {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 13rem;
    overflow: hidden;
    color: var(--color-muted);
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .additional-sources-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    width: fit-content;
    min-height: var(--control-min-size);
    padding: 0.2rem 0.45rem;
    gap: var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
    font-size: var(--font-size-xs);
  }
  .additional-sources-toggle:hover,
  .additional-sources-toggle:focus-visible {
    color: var(--color-heading);
    background: var(--color-surface-muted);
  }
  .source-preview {
    min-width: 0;
  }
  .source-preview header {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: var(--font-size-xs);
  }
  .source-preview pre {
    max-height: 20rem;
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    white-space: pre;
  }
  .compact {
    margin: 0;
  }
  .compact summary {
    grid-template-columns: auto minmax(0, 1fr);
  }
  .compact summary em {
    grid-column: 2;
  }
  @keyframes research-step-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes research-active-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 3px var(--color-status-pending-ring);
    }
    50% {
      box-shadow: 0 0 0 6px var(--color-status-pending-ring);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    * {
      scroll-behavior: auto !important;
    }
    .research-steps li.animate,
    .research-steps li.active .step-marker {
      animation: none;
    }
  }
</style>

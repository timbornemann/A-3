<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    queryAgentAskResearchDetail,
    queryAgentAskResearchSourcePreview,
    queryAgentAskResearchSources,
    type AgentAskResearchDetailV1,
    type AgentAskResearchSourceV1,
  } from './agent-ask-research';
  import type { ProjectMapSourcePreviewV1 } from './project-map-source-preview';

  interface Props {
    compact?: boolean;
    detailLoader?: typeof queryAgentAskResearchDetail;
    live?: boolean;
    oncontinue?: () => void;
    previewLoader?: typeof queryAgentAskResearchSourcePreview;
    recentlyCompleted?: boolean;
    refreshKey: string;
    sessionId: string;
    sourcesLoader?: typeof queryAgentAskResearchSources;
    userSequence: string;
  }

  let {
    compact = false,
    detailLoader = queryAgentAskResearchDetail,
    live = false,
    oncontinue = () => {},
    previewLoader = queryAgentAskResearchSourcePreview,
    recentlyCompleted = false,
    refreshKey,
    sessionId,
    sourcesLoader = queryAgentAskResearchSources,
    userSequence,
  }: Props = $props();
  let expanded = $state(false);
  let expansionInitialized = false;
  let detail = $state<AgentAskResearchDetailV1 | null>(null);
  let sources = $state<AgentAskResearchSourceV1[]>([]);
  let loadState = $state<'loading' | 'available' | 'notRecorded' | 'missing' | 'error'>('loading');
  let selectedSource = $state<string | null>(null);
  let preview = $state<ProjectMapSourcePreviewV1 | null>(null);
  let previewState = $state<'idle' | 'loading' | 'stale' | 'error'>('idle');
  let visibleStepCount = $state(0);
  let reducedMotion = $state(false);
  let request = 0;
  let loadInFlight = false;
  let loadQueued = false;
  let activeTurnKey = '';
  let revealTarget = 0;
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  let collapseTimer: ReturnType<typeof setTimeout> | null = null;
  let autoCollapseEligible = false;
  let autoCollapseDone = false;
  let terminalObserved = false;
  let terminalDisclosureOverride = false;

  const REVEAL_WINDOW_MILLIS = 900;
  const MAX_REVEAL_INTERVAL_MILLIS = 180;
  const TERMINAL_DWELL_MILLIS = 700;

  const latest = $derived(detail?.steps.at(-1) ?? null);
  const visibleSteps = $derived(detail?.steps.slice(0, visibleStepCount) ?? []);
  const searchLimited = $derived(
    detail?.steps.some((step) => step.completeness === 'limited') ?? false,
  );

  $effect(() => {
    const turnKey = `${sessionId}:${userSequence}`;
    if (turnKey !== activeTurnKey) {
      activeTurnKey = turnKey;
      resetPresentation();
      detail = null;
      sources = [];
      selectedSource = null;
      preview = null;
      previewState = 'idle';
      loadState = 'loading';
      expansionInitialized = false;
    }
    if (!expansionInitialized) {
      expanded = live || recentlyCompleted;
      expansionInitialized = true;
    }
    if (live || recentlyCompleted) autoCollapseEligible = true;
    void refreshKey;
    requestResearchLoad();
  });

  onMount(() => {
    if (typeof window.matchMedia !== 'function') return;
    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = (): void => {
      reducedMotion = query.matches;
      if (reducedMotion && detail) synchronizeVisibleSteps(detail.steps.length);
    };
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  });

  onDestroy(() => {
    clearRevealTimer();
    clearCollapseTimer();
  });

  function requestResearchLoad(): void {
    request += 1;
    loadQueued = true;
    if (!loadInFlight) void drainResearchLoads();
  }

  async function drainResearchLoads(): Promise<void> {
    loadInFlight = true;
    while (loadQueued) {
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
      const response = await detailLoader(requestedSessionId, requestedUserSequence);
      if (
        current !== request ||
        requestedSessionId !== sessionId ||
        requestedUserSequence !== userSequence
      )
        return;
      if (response.result.status === 'notRecorded') {
        loadState = 'notRecorded';
        return;
      }
      if (response.result.status !== 'available') {
        loadState = 'missing';
        return;
      }
      const nextDetail = response.result.detail;
      if (detail && !isAppendOnlyUpdate(detail.steps, nextDetail.steps)) {
        resetPresentation();
        autoCollapseEligible = live || recentlyCompleted;
      }
      detail = nextDetail;
      loadState = 'available';
      synchronizeVisibleSteps(nextDetail.steps.length);
      const found: AgentAskResearchSourceV1[] = [];
      let cursor: string | null = null;
      do {
        const page = await sourcesLoader(requestedSessionId, requestedUserSequence, cursor);
        if (current !== request) return;
        if (page.result.status !== 'available') break;
        found.push(...page.result.sources);
        cursor = page.result.nextCursor;
      } while (cursor !== null && found.length < 200);
      sources = found;
    } catch {
      if (current === request && detail === null) loadState = 'error';
    }
  }

  function isAppendOnlyUpdate(
    previous: AgentAskResearchDetailV1['steps'],
    next: AgentAskResearchDetailV1['steps'],
  ): boolean {
    if (next.length < previous.length) return false;
    return previous.every((step, index) => stepKey(step, index) === stepKey(next[index], index));
  }

  function stepKey(step: AgentAskResearchDetailV1['steps'][number], index: number): string {
    return `${index}:${step.occurredAtUnixMillis}:${step.phase}:${step.state}:${step.action}:${step.query ?? ''}`;
  }

  function synchronizeVisibleSteps(stepCount: number): void {
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
      return;
    }
    if (visibleStepCount === 0) visibleStepCount = 1;
    if (visibleStepCount < revealTarget) scheduleRemainingSteps();
    else observeTerminalState();
  }

  function scheduleRemainingSteps(): void {
    clearRevealTimer();
    const remaining = revealTarget - visibleStepCount;
    if (remaining <= 0) {
      observeTerminalState();
      return;
    }
    const interval = Math.min(
      MAX_REVEAL_INTERVAL_MILLIS,
      Math.max(1, Math.floor(REVEAL_WINDOW_MILLIS / remaining)),
    );
    const revealNext = (): void => {
      revealTimer = null;
      visibleStepCount = Math.min(visibleStepCount + 1, revealTarget);
      if (visibleStepCount < revealTarget) revealTimer = setTimeout(revealNext, interval);
      else observeTerminalState();
    };
    revealTimer = setTimeout(revealNext, interval);
  }

  function observeTerminalState(): void {
    if (
      !detail ||
      visibleStepCount < detail.steps.length ||
      !isSuccessfulTerminal(detail.steps.at(-1))
    )
      return;
    terminalObserved = true;
    if (
      !autoCollapseEligible ||
      autoCollapseDone ||
      terminalDisclosureOverride ||
      collapseTimer !== null
    )
      return;
    collapseTimer = setTimeout(() => {
      collapseTimer = null;
      if (!terminalDisclosureOverride) expanded = false;
      autoCollapseDone = true;
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

  function handleDisclosureClick(): void {
    if (!terminalObserved) return;
    terminalDisclosureOverride = true;
    clearCollapseTimer();
  }

  function resetPresentation(): void {
    clearRevealTimer();
    clearCollapseTimer();
    visibleStepCount = 0;
    revealTarget = 0;
    autoCollapseEligible = false;
    autoCollapseDone = false;
    terminalObserved = false;
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

  async function showSource(sourceRef: string): Promise<void> {
    selectedSource = sourceRef;
    preview = null;
    previewState = 'loading';
    try {
      const response = await previewLoader(sessionId, userSequence, sourceRef);
      if (selectedSource !== sourceRef) return;
      if (response.result.status === 'available') {
        preview = response.result.preview;
        previewState = 'idle';
      } else if (response.result.status === 'stale') previewState = 'stale';
      else previewState = 'error';
    } catch {
      if (selectedSource === sourceRef) previewState = 'error';
    }
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

  function sourceForRef(sourceRef: string): AgentAskResearchSourceV1 | undefined {
    return sources.find((source) => source.sourceRef === sourceRef);
  }

  function reasonLabel(reason: AgentAskResearchSourceV1['reason']): string {
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
    if (!detail) return 'completed';
    const step = detail.steps[index];
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

  function sourceFeedback(): string {
    if (!detail) return '';
    const found = `${detail.sourceCount} ${detail.sourceCount === 1 ? 'Quelle' : 'Quellen'} gefunden`;
    if (detail.citedSourceCount === 0) return found;
    return `${found} · ${detail.citedSourceCount} für das Ergebnis verwendet`;
  }
</script>

<details class:compact class="ask-research" bind:open={expanded}>
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
      {#if latest}
        <section class="current-action" role="status" aria-live="polite" aria-atomic="true">
          <span>{isTerminal(latest) ? 'Aktueller Stand' : 'Was passiert gerade?'}</span>
          <strong>{stepLabel(latest.phase, latest.state)}</strong>
          <p>{latest.action}</p>
          {#if latest.query}<code>{latest.query}</code>{/if}
          <small class="source-feedback">{sourceFeedback()}</small>
          {#if latest.completeness !== 'notApplicable'}
            <small class="completeness"
              >{latest.completeness === 'complete' ? 'Suche vollständig' : 'Suche begrenzt'}</small
            >
          {/if}
          {#if searchLimited}<p class="limited">
              Mindestens eine Suche wurde durch eine feste Sicherheits- oder Ressourcengrenze
              beendet. Sie beweist nicht, dass es keine weiteren Treffer gibt.
            </p>{/if}
        </section>
      {/if}
      <ol class="research-steps" aria-label="Rechercheverlauf">
        {#each visibleSteps as step, index (`${step.occurredAtUnixMillis}-${index}`)}
          {@const displayState = presentationState(index)}
          {@const terminalStep = index === detail.steps.length - 1 && isTerminal(step)}
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
                        {#each step.note.sourceRefs as sourceRef (sourceRef)}
                          {@const noteSource = sourceForRef(sourceRef)}
                          <button type="button" onclick={() => void showSource(sourceRef)}>
                            {noteSource
                              ? `${noteSource.path}${noteSource.startLine ? `:${noteSource.startLine}` : ''}`
                              : 'Quelle öffnen'}
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
      <section>
        <h4>Gefundene und verwendete Quellen</h4>
        {#if sources.length === 0}<p>Noch keine zitierbare aktuelle Quelle gefunden.</p>
        {:else}
          <ul class="source-list">
            {#each sources as source (source.sourceRef)}
              <li>
                <button
                  type="button"
                  onclick={() => void showSource(source.sourceRef)}
                  aria-pressed={selectedSource === source.sourceRef}
                >
                  <strong>{source.path}{source.startLine ? `:${source.startLine}` : ''}</strong>
                  {#if source.symbol}<span>{source.symbol}</span>{/if}
                  <small
                    >{reasonLabel(source.reason)} · {source.usedForAnswer
                      ? 'Für Ergebnis verwendet'
                      : 'Zusätzlich bereitgestellt'}</small
                  >
                </button>
              </li>
            {/each}
          </ul>
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
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-card);
    background: var(--color-surface-subtle);
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
  .current-action {
    display: grid;
    padding: var(--space-3);
    gap: var(--space-1);
    border-radius: var(--radius-control);
    background: var(--color-surface);
  }
  .current-action > span {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
  }
  .source-feedback {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
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
  .research-steps,
  .source-list {
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
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface);
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
    min-height: 2rem;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-heading);
    background: var(--color-surface-subtle);
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
  }
  .research-steps p {
    margin-top: 0.15rem;
    font-size: var(--font-size-xs);
  }
  h4 {
    margin: 0 0 var(--space-2);
  }
  .source-list button {
    display: grid;
    width: 100%;
    min-height: 3.1rem;
    padding: var(--space-2);
    gap: 0.15rem;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    color: var(--color-text);
    text-align: start;
    background: var(--color-surface);
    cursor: pointer;
  }
  .source-list button[aria-pressed='true'] {
    border-color: var(--color-accent);
    background: var(--color-accent-surface);
  }
  .source-list span,
  .source-list small {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    overflow-wrap: anywhere;
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
      transform: translateY(0.25rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
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

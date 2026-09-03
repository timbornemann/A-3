<script lang="ts">
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
    previewLoader?: typeof queryAgentAskResearchSourcePreview;
    refreshKey: string;
    sessionId: string;
    sourcesLoader?: typeof queryAgentAskResearchSources;
    userSequence: string;
  }

  let {
    compact = false,
    detailLoader = queryAgentAskResearchDetail,
    live = false,
    previewLoader = queryAgentAskResearchSourcePreview,
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
  let request = 0;
  let loadInFlight = false;
  let loadQueued = false;
  let activeTurnKey = '';

  const latest = $derived(detail?.steps.at(-1) ?? null);
  const searchLimited = $derived(
    detail?.steps.some((step) => step.completeness === 'limited') ?? false,
  );

  $effect(() => {
    const turnKey = `${sessionId}:${userSequence}`;
    if (turnKey !== activeTurnKey) {
      activeTurnKey = turnKey;
      detail = null;
      sources = [];
      selectedSource = null;
      preview = null;
      previewState = 'idle';
      loadState = 'loading';
    }
    if (!expansionInitialized) {
      expanded = live;
      expansionInitialized = true;
    }
    void refreshKey;
    requestResearchLoad();
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
      detail = response.result.detail;
      loadState = 'available';
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
        answering: 'Antwort wird belegt',
        completed: 'Recherche abgeschlossen',
        inspectingSource: 'Quelle wird geprüft',
        preparing: 'Projektstand wird gebunden',
        searchingSource: 'Quelltext wird durchsucht',
        selectingEvidence: 'Task Lens wählt Evidence',
      }[phase] ?? phase
    );
  }

  function stepLabel(phase: string, state: string): string {
    if (state === 'failed') return 'Recherche fehlgeschlagen';
    if (state === 'cancelled') return 'Recherche abgebrochen';
    return phaseLabel(phase);
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
</script>

<details class:compact class="ask-research" bind:open={expanded}>
  <summary>
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
    {#if detail}<em>{detail.sourceCount} Quellen · {detail.citedSourceCount} verwendet</em>{/if}
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
      <p>Für diesen Beitrag existiert kein Ask-Rechercheweg.</p>
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
        <section class="current-action">
          <span>Was passiert gerade?</span>
          <strong>{stepLabel(latest.phase, latest.state)}</strong>
          <p>{latest.action}</p>
          {#if latest.query}<code>{latest.query}</code>{/if}
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
        {#each detail.steps as step, index (`${step.occurredAtUnixMillis}-${index}`)}
          <li class:active={step === latest && step.state === 'running'}>
            <span></span>
            <div>
              <strong>{stepLabel(step.phase, step.state)}</strong>
              <p>{step.action}</p>
              {#if step.query}<code>{step.query}</code
                >{/if}{#if step.completeness !== 'notApplicable'}<small class="completeness"
                  >{step.completeness === 'complete' ? 'Vollständig' : 'Begrenzt'}</small
                >{/if}
            </div>
          </li>
        {/each}
      </ol>
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
                      ? 'Für Antwort verwendet'
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
  .research-steps li {
    display: grid;
    grid-template-columns: 0.65rem minmax(0, 1fr);
    gap: var(--space-2);
  }
  .research-steps li > span {
    width: 0.45rem;
    height: 0.45rem;
    margin-top: 0.35rem;
    border-radius: 50%;
    background: var(--color-border-strong);
  }
  .research-steps li.active > span {
    background: var(--color-status-pending);
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
  @media (prefers-reduced-motion: reduce) {
    * {
      scroll-behavior: auto !important;
    }
  }
</style>

<script lang="ts">
  import {
    queryDeepMapEntries,
    queryDeepMapEntryDetail,
    queryDeepMapRuns,
    type DeepMapEntryDetailResponseV1,
    type DeepMapEntryPageResponseV1,
    type DeepMapEntryV1,
    type DeepMapFailureV3,
    type DeepMapRunPageResponseV1,
    type DeepMapRunV1,
  } from './deep-map';

  interface Props {
    detailLoader?: (
      runSelection: string,
      entrySelection: string,
    ) => Promise<DeepMapEntryDetailResponseV1>;
    entriesLoader?: (
      runSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapEntryPageResponseV1>;
    focusFailureEpoch?: number;
    onclose: () => void;
    open: boolean;
    runsLoader?: (cursor?: string | null) => Promise<DeepMapRunPageResponseV1>;
  }

  const {
    open,
    focusFailureEpoch = 0,
    onclose,
    runsLoader = queryDeepMapRuns,
    entriesLoader = queryDeepMapEntries,
    detailLoader = queryDeepMapEntryDetail,
  }: Props = $props();

  let runPage = $state<DeepMapRunPageResponseV1 | null>(null);
  let entryPage = $state<DeepMapEntryPageResponseV1 | null>(null);
  let selectedRun = $state<DeepMapRunV1 | null>(null);
  let selectedEntry = $state<DeepMapEntryV1 | null>(null);
  let detail = $state<DeepMapEntryDetailResponseV1 | null>(null);
  let busy = $state(false);
  let failed = $state(false);
  let seenFailureEpoch = 0;
  let previouslyOpen = false;
  let generation = 0;

  $effect(() => {
    if (open && !previouslyOpen) void loadRuns(null, false);
    previouslyOpen = open;
  });

  $effect(() => {
    if (!open || focusFailureEpoch <= seenFailureEpoch) return;
    seenFailureEpoch = focusFailureEpoch;
    void focusLatestFailure();
  });

  async function loadRuns(cursor: string | null, focusFailure: boolean): Promise<void> {
    const request = ++generation;
    busy = true;
    failed = false;
    try {
      const page = await runsLoader(cursor);
      if (request !== generation) return;
      runPage = page;
      const run = focusFailure
        ? (page.runs.find((item) => item.state === 'failed') ?? page.runs[0] ?? null)
        : (page.runs[0] ?? null);
      selectedRun = run;
      if (run === null) {
        entryPage = null;
        selectedEntry = null;
        detail = null;
      } else {
        await loadEntries(run, null, focusFailure, request);
      }
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function loadEntries(
    run: DeepMapRunV1,
    cursor: string | null,
    focusFailure: boolean,
    request = ++generation,
  ): Promise<void> {
    busy = true;
    failed = false;
    try {
      const page = await entriesLoader(run.selection, cursor);
      if (request !== generation) return;
      entryPage = page;
      const entry = focusFailure
        ? ([...page.entries].reverse().find((item) => item.failure !== null) ??
          page.entries.at(-1) ??
          null)
        : (page.entries.at(-1) ?? null);
      selectedEntry = entry;
      detail = null;
      if (entry !== null) await loadDetail(run, entry, request);
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function loadDetail(
    run: DeepMapRunV1,
    entry: DeepMapEntryV1,
    request = ++generation,
  ): Promise<void> {
    busy = true;
    failed = false;
    selectedEntry = entry;
    detail = null;
    try {
      const response = await detailLoader(run.selection, entry.selection);
      if (request === generation) detail = response;
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function selectRun(event: Event): Promise<void> {
    const selection = (event.currentTarget as HTMLSelectElement).value;
    const run = runPage?.runs.find((item) => item.selection === selection) ?? null;
    selectedRun = run;
    entryPage = null;
    selectedEntry = null;
    detail = null;
    if (run !== null) await loadEntries(run, null, false);
  }

  async function focusLatestFailure(): Promise<void> {
    if (runPage === null) {
      await loadRuns(null, true);
      return;
    }
    const failedRun = runPage.runs.find((run) => run.state === 'failed');
    if (failedRun === undefined) return;
    selectedRun = failedRun;
    await loadEntries(failedRun, null, true);
  }

  function stateLabel(value: string): string {
    return (
      {
        queued: 'Eingeplant',
        running: 'Läuft',
        pausing: 'Wird pausiert',
        paused: 'Pausiert',
        cancelling: 'Wird abgebrochen',
        succeeded: 'Abgeschlossen',
        failed: 'Fehlgeschlagen',
        cancelled: 'Abgebrochen',
        interrupted: 'Unterbrochen',
      }[value] ?? value
    );
  }

  function phaseLabel(value: string | null): string {
    if (value === null) return 'Pipeline';
    return (
      {
        planning: 'Planung',
        exploring: 'Exploration',
        claiming: 'Claims',
        verifying: 'Verifikation',
        publishing: 'Publikation',
      }[value] ?? value
    );
  }

  function actionLabel(value: string | null): string {
    if (value === null) return 'Status aktualisiert';
    return (
      {
        buildPlan: 'Plan erstellen',
        inspect: 'Evidence lesen',
        search: 'Index durchsuchen',
        propose: 'Schritt bestätigen',
        generateClaims: 'Claims erzeugen',
        verifyEvidence: 'Evidence prüfen',
        publishCards: 'Cards publizieren',
      }[value] ?? value
    );
  }

  function failureInfo(code: DeepMapFailureV3): { title: string; action: string } {
    const values: Record<DeepMapFailureV3, { title: string; action: string }> = {
      noPublishedIndex: {
        title: 'Kein veröffentlichter Index',
        action: 'Warte den Indexlauf ab oder erstelle die Code-Analyse neu.',
      },
      staleIndex: {
        title: 'Projektstand wurde ersetzt',
        action: 'Starte Deep Map auf dem aktuellen Index erneut.',
      },
      planning: {
        title: 'Planung fehlgeschlagen',
        action: 'Prüfe den Indexstatus und starte Deep Map erneut.',
      },
      modelUnavailable: {
        title: 'Modell nicht erreichbar',
        action: 'Prüfe Provider, Verbindung und Zugangsdaten.',
      },
      modelRejected: {
        title: 'Modell hat die Anfrage abgelehnt',
        action: 'Verifiziere das Mapping-Modell oder wähle ein anderes Modell.',
      },
      modelTimeout: {
        title: 'Modell-Zeitlimit erreicht',
        action:
          'Versuche es erneut oder wähle einen schnelleren Modus beziehungsweise ein anderes Modell.',
      },
      invalidModelResponse: {
        title: 'Modellantwort war nicht verwendbar',
        action: 'Verifiziere das Mapping-Modell erneut.',
      },
      read: {
        title: 'Evidence konnte nicht gelesen werden',
        action: 'Erstelle die Code-Analyse neu und starte danach erneut.',
      },
      verification: {
        title: 'Claims konnten nicht verifiziert werden',
        action: 'Starte einen neuen Lauf auf dem aktuellen Index.',
      },
      publicationRejected: {
        title: 'Publikation wurde sicher abgewiesen',
        action: 'Erstelle bei einem geänderten Stand zuerst einen neuen Index.',
      },
      publicationStorage: {
        title: 'Publikationsspeicher nicht verfügbar',
        action: 'Prüfe den lokalen Speicher und versuche es erneut.',
      },
      publicationTimeout: {
        title: 'Publikation hat zu lange gedauert',
        action: 'Versuche die Publikation erneut.',
      },
      publicationProgress: {
        title: 'Publikationsfortschritt nicht verfügbar',
        action: 'Lade den Status neu und starte bei Bedarf erneut.',
      },
      invalidCheckpoint: {
        title: 'Pause-Checkpoint ist ungültig',
        action: 'Beginne einen neuen Deep-Map-Lauf.',
      },
      progressUnavailable: {
        title: 'Laufstatus ist widersprüchlich',
        action: 'Starte Deep Map oder A^3 neu.',
      },
      interrupted: {
        title: 'Lauf wurde unterbrochen',
        action: 'Starte Deep Map auf dem aktuellen Index neu.',
      },
    };
    return values[code];
  }

  function formatTime(value: string): string {
    return new Intl.DateTimeFormat('de-DE', {
      dateStyle: 'short',
      timeStyle: 'medium',
    }).format(new Date(Number(value)));
  }
</script>

<aside
  class:open
  class="inspector deep-map-inspector"
  aria-label="Deep-Map-Details"
  aria-hidden={!open}
  inert={!open}
>
  <header class="inspector-head">
    <div>
      <span>Laufjournal</span>
      <h3>Deep Map</h3>
    </div>
    <button type="button" aria-label="Deep-Map-Details schließen" onclick={onclose}>×</button>
  </header>

  {#if open}
    <div class="content">
      {#if failed}
        <div class="notice error" role="alert">
          <strong>Details konnten nicht sicher geladen werden.</strong>
          <button type="button" onclick={() => loadRuns(null, false)}>Erneut laden</button>
        </div>
      {/if}

      <section class="run-choice" aria-labelledby="deep-map-run-heading">
        <div>
          <span>Neueste 20 Läufe</span>
          <select
            id="deep-map-run-heading"
            aria-label="Deep-Map-Lauf"
            value={selectedRun?.selection ?? ''}
            onchange={selectRun}
          >
            {#if runPage?.runs.length === 0}<option value="">Kein Journal vorhanden</option>{/if}
            {#each runPage?.runs ?? [] as run (run.selection)}
              <option value={run.selection}>
                {formatTime(run.startedAtUnixMillis)} · {stateLabel(run.state)}
              </option>
            {/each}
          </select>
        </div>
        {#if runPage?.nextCursor !== null && runPage?.nextCursor !== undefined}
          <button
            type="button"
            disabled={busy}
            onclick={() => loadRuns(runPage?.nextCursor ?? null, false)}>Ältere</button
          >
        {:else if runPage !== null && runPage.runs.length > 0}
          <button type="button" disabled={busy} onclick={() => loadRuns(null, false)}
            >Neueste</button
          >
        {/if}
      </section>

      {#if selectedRun === null && !busy}
        <div class="notice">
          <strong>Keine Einzelschritte verfügbar</strong>
          <p>
            Historische Module Cards bleiben aktuell nutzbar; für sie existiert noch kein
            Laufjournal.
          </p>
        </div>
      {:else if selectedRun !== null}
        <section class="run-summary">
          <div><span>Status</span><strong>{stateLabel(selectedRun.state)}</strong></div>
          <div><span>Modus</span><strong>{selectedRun.mode}</strong></div>
          <div>
            <span>Fortschritt</span><strong
              >{selectedRun.confirmedSteps}/{selectedRun.totalSteps}</strong
            >
          </div>
          {#if selectedRun.detailsIncomplete}<p>Details unvollständig</p>{/if}
        </section>

        <ol class="timeline" aria-label="Chronologische Deep-Map-Einträge">
          {#each entryPage?.entries ?? [] as entry (entry.selection)}
            <li
              class:selected={selectedEntry?.selection === entry.selection}
              class:failed={entry.failure !== null}
            >
              <button type="button" onclick={() => loadDetail(selectedRun!, entry)}>
                <span>{entry.sequence}</span>
                <div>
                  <strong>{phaseLabel(entry.phase)}</strong>
                  <small>{actionLabel(entry.action)} · {stateLabel(entry.state)}</small>
                </div>
              </button>
            </li>
          {/each}
        </ol>
        {#if entryPage?.nextCursor !== null && entryPage?.nextCursor !== undefined}
          <button
            class="page-button"
            type="button"
            disabled={busy}
            onclick={() => loadEntries(selectedRun!, entryPage?.nextCursor ?? null, false)}
            >Ältere Einträge</button
          >
        {/if}
      {/if}

      {#if busy}<p class="loading" role="status">Details werden geladen …</p>{/if}

      {#if detail !== null}
        <section class="detail" aria-labelledby="deep-map-entry-heading">
          <header>
            <span>Eintrag {detail.entry.sequence}</span>
            <h4 id="deep-map-entry-heading">{actionLabel(detail.entry.action)}</h4>
          </header>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>{stateLabel(detail.entry.state)}</dd>
            </div>
            <div>
              <dt>Zeitpunkt</dt>
              <dd>{formatTime(detail.entry.occurredAtUnixMillis)}</dd>
            </div>
            <div>
              <dt>Dauer</dt>
              <dd>{Number(detail.durationMillis).toLocaleString('de-DE')} ms</dd>
            </div>
            <div>
              <dt>Phase</dt>
              <dd>{phaseLabel(detail.entry.phase)}</dd>
            </div>
            <div>
              <dt>Aktion</dt>
              <dd>{actionLabel(detail.entry.action)}</dd>
            </div>
            <div>
              <dt>Zielart</dt>
              <dd>{detail.entry.targetKind ?? 'Projekt'}</dd>
            </div>
            <div>
              <dt>Schritt</dt>
              <dd>{detail.entry.stepPosition ?? '–'} / {detail.entry.totalSteps ?? '–'}</dd>
            </div>
            <div>
              <dt>Bestätigt</dt>
              <dd>{detail.entry.confirmed ? 'Ja' : 'Nein'}</dd>
            </div>
            <div>
              <dt>Ergebnis</dt>
              <dd>{detail.entry.result}</dd>
            </div>
          </dl>

          {#if detail.entry.failure !== null}
            {@const info = failureInfo(detail.entry.failure)}
            <div class="failure-detail" role="alert">
              <strong>{info.title}</strong>
              <p><b>Nächster Schritt:</b> {detail.nextAction ?? info.action}</p>
              <small>Diagnosecode <code>{detail.entry.failure}</code></small>
            </div>
          {/if}

          <details class="technical" open>
            <summary>Technische Details</summary>
            <dl>
              <div>
                <dt>Provider / Modell</dt>
                <dd>{detail.providerId} · {detail.modelId}</dd>
              </div>
              <div>
                <dt>Profil</dt>
                <dd>{detail.profileId.slice(0, 12)} · v{detail.profileVersion}</dd>
              </div>
              <div>
                <dt>Feste Grenze</dt>
                <dd>
                  {detail.tokenBudget.toLocaleString('de-DE')} Tokens · {detail.toolCallBudget} Reads
                </dd>
              </div>
              <div>
                <dt>Index / Snapshot</dt>
                <dd>{detail.indexReference} · {detail.snapshotReference}</dd>
              </div>
              <div>
                <dt>Planstopp</dt>
                <dd>{detail.planStopReason ?? '–'}</dd>
              </div>
              <div>
                <dt>Publikation</dt>
                <dd>{detail.publicationResult ?? '–'}</dd>
              </div>
              {#if detail.step !== null}
                <div>
                  <dt>Reservierte Schrittkosten</dt>
                  <dd>
                    {detail.step.reservedTokens} Tokens · {detail.step.reservedToolCalls} Read(s) · {detail
                      .step.reservedTimeMillis} ms
                  </dd>
                </div>
                <div>
                  <dt>Informationsgewinn</dt>
                  <dd>
                    {(detail.step.informationGainBasisPoints / 100).toLocaleString('de-DE')} %
                  </dd>
                </div>
                <div>
                  <dt>Coverage / Evidence</dt>
                  <dd>
                    {detail.step.coverageFieldCount} Felder · {detail.step.evidenceRequirement}
                  </dd>
                </div>
                <div>
                  <dt>Verifikation</dt>
                  <dd>{detail.step.verificationMethod}</dd>
                </div>
              {/if}
            </dl>
          </details>
        </section>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .inspector {
    flex: 0 0 auto;
    min-width: 0;
    width: 0;
    overflow: hidden auto;
    border-left: 0 solid var(--line);
    background: var(--surface);
    transition: width 140ms ease;
  }
  .inspector.open {
    width: var(--inspector-width, 380px);
    border-left-width: 1px;
  }
  .inspector-head {
    position: sticky;
    top: 0;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-width: 300px;
    min-height: 66px;
    padding: 10px 12px 10px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .inspector-head span,
  .detail header span,
  .run-choice span,
  dt {
    color: var(--muted);
    font-size: 0.64rem;
  }
  .inspector-head h3,
  .detail h4 {
    margin: 3px 0 0;
  }
  .inspector-head button,
  .run-choice button,
  .page-button,
  .notice button {
    min-width: 44px;
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
  }
  .content {
    display: grid;
    gap: 10px;
    min-width: 300px;
    padding: 12px;
  }
  .run-choice {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 6px;
  }
  .run-choice > div {
    display: grid;
    gap: 4px;
  }
  select {
    min-width: 0;
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-canvas);
    color: inherit;
  }
  .run-summary {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1px;
    background: var(--line);
  }
  .run-summary > div {
    display: grid;
    gap: 3px;
    padding: 8px;
    background: var(--surface-raised);
    font-size: 0.72rem;
  }
  .run-summary span {
    color: var(--muted);
    font-size: 0.62rem;
  }
  .run-summary p {
    grid-column: 1 / -1;
    margin: 0;
    padding: 7px;
    background: var(--surface-raised);
    color: var(--color-warning);
    font-size: 0.72rem;
  }
  .timeline {
    max-height: 300px;
    margin: 0;
    padding: 0;
    overflow: auto;
    border: 1px solid var(--line);
    list-style: none;
  }
  .timeline button {
    display: grid;
    grid-template-columns: 32px 1fr;
    gap: 7px;
    width: 100%;
    min-height: 48px;
    padding: 6px 8px;
    border: 0;
    border-bottom: 1px solid var(--line);
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .timeline li.selected button {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .timeline li.failed strong {
    color: var(--color-status-failed);
  }
  .timeline div {
    display: grid;
  }
  .timeline small,
  .timeline button > span {
    color: var(--muted);
  }
  .detail {
    display: grid;
    gap: 9px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    margin: 0;
    background: var(--line);
  }
  dl > div {
    min-width: 0;
    padding: 8px;
    background: var(--surface-raised);
  }
  dd {
    margin: 3px 0 0;
    overflow-wrap: anywhere;
    font-size: 0.72rem;
  }
  .technical summary {
    min-height: 44px;
    padding: 12px 8px;
    cursor: pointer;
  }
  .failure-detail,
  .notice {
    padding: 10px;
    border: 1px solid var(--line);
    background: var(--surface-raised);
    font-size: 0.75rem;
  }
  .failure-detail {
    border-color: var(--color-status-failed-ring);
  }
  .failure-detail p,
  .notice p {
    margin: 6px 0;
  }
  .error {
    color: var(--color-status-failed);
  }
  .loading {
    color: var(--muted);
  }
  button:focus-visible,
  select:focus-visible,
  summary:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  @media (max-width: 899px) {
    .inspector {
      position: absolute;
      z-index: 20;
      inset: 0 0 0 auto;
      box-shadow: -10px 0 30px color-mix(in srgb, var(--color-shadow) 28%, transparent);
    }
    .inspector.open {
      width: min(390px, 92vw);
    }
  }
  @media (max-width: 420px) {
    dl {
      grid-template-columns: 1fr;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .inspector {
      transition: none;
    }
  }
</style>

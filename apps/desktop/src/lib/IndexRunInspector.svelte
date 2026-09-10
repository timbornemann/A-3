<script lang="ts">
  import { onMount } from 'svelte';
  import {
    controlIndexRun,
    describeIndexRunInspectionLoadFailure,
    queryIndexRunFiles,
    queryIndexRunInspection,
    type IndexRunControlActionV1,
    type IndexRunDetailV1,
    type IndexRunFileFilterV1,
    type IndexRunFilesResponseV1,
    type IndexRunInspectionResponseV1,
    type IndexRunPhaseV1,
  } from './index-run-inspection';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();
  let dialog: HTMLDialogElement;
  let closeButton: HTMLButtonElement;
  let response = $state<IndexRunInspectionResponseV1 | null>(null);
  let files = $state<IndexRunFilesResponseV1 | null>(null);
  let selected = $state<'current' | 'previous'>('current');
  let tab = $state<'overview' | 'files' | 'events'>('overview');
  let search = $state('');
  let filter = $state<IndexRunFileFilterV1>('all');
  let loading = $state(true);
  let filesLoading = $state(false);
  let actionPending = $state(false);
  let error = $state<string | null>(null);
  let actionNotice = $state<string | null>(null);
  let destroyed = false;
  let refreshing = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let loadedFileKey = '';

  onMount(() => {
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    closeButton.focus();
    void refresh();
    return () => {
      destroyed = true;
      if (timer !== null) clearTimeout(timer);
    };
  });

  async function refresh(): Promise<void> {
    if (destroyed || refreshing) return;
    refreshing = true;
    try {
      const next = await queryIndexRunInspection();
      if (destroyed) return;
      response = next;
      error = null;
      loading = false;
      const result = next.result;
      if (result.status === 'available') {
        if (selected === 'previous' && result.previous === null) selected = 'current';
        const run = selected === 'previous' ? result.previous : result.current;
        if (
          tab === 'files' &&
          run !== null &&
          loadedFileKey !== `${run.runRef}:${run.revision}:${search}:${filter}`
        ) {
          await loadFiles(run, null);
        }
      }
    } catch (cause) {
      if (!destroyed) {
        const failure = describeIndexRunInspectionLoadFailure(cause);
        error = `${failure.message} ${failure.recovery} (${failure.code})`;
        loading = false;
      }
    } finally {
      refreshing = false;
      if (!destroyed) timer = setTimeout(() => void refresh(), 500);
    }
  }

  function selectedRun(): IndexRunDetailV1 | null {
    if (response?.result.status !== 'available') return null;
    return selected === 'previous' ? response.result.previous : response.result.current;
  }

  function stalled(run: IndexRunDetailV1): boolean {
    if (
      response?.result.status !== 'available' ||
      !['queued', 'running', 'cancelling'].includes(run.state)
    )
      return false;
    return (
      BigInt(response.result.serverTimeUnixMillis) - BigInt(run.lastActivityAtUnixMillis) >=
      BigInt(response.result.stallThresholdSeconds * 1000)
    );
  }

  async function chooseRun(value: 'current' | 'previous'): Promise<void> {
    selected = value;
    files = null;
    loadedFileKey = '';
    const run = selectedRun();
    if (tab === 'files' && run !== null) await loadFiles(run, null);
  }

  async function chooseTab(value: 'overview' | 'files' | 'events'): Promise<void> {
    tab = value;
    if (value === 'files') {
      const run = selectedRun();
      if (run !== null) await loadFiles(run, null);
    }
  }

  async function loadFiles(run: IndexRunDetailV1, cursor: string | null): Promise<void> {
    if (filesLoading) return;
    filesLoading = true;
    try {
      const next = await queryIndexRunFiles(run, search, filter, cursor);
      if (!destroyed && selectedRun()?.runRef === run.runRef) {
        files = next;
        loadedFileKey =
          next.result.status === 'page' ? `${run.runRef}:${run.revision}:${search}:${filter}` : '';
      }
    } catch {
      if (!destroyed) error = 'Die Dateiliste konnte nicht sicher geladen werden.';
    } finally {
      filesLoading = false;
    }
  }

  async function applyFileQuery(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    loadedFileKey = '';
    const run = selectedRun();
    if (run !== null) await loadFiles(run, null);
  }

  async function runAction(action: IndexRunControlActionV1): Promise<void> {
    const run = selectedRun();
    if (run === null || actionPending) return;
    actionPending = true;
    actionNotice = null;
    try {
      const outcome = await controlIndexRun(run, action);
      actionNotice = controlMessage(outcome.result.status, action);
      await refresh();
    } catch {
      actionNotice = 'Die Aktion konnte nicht sicher ausgeführt werden.';
    } finally {
      actionPending = false;
    }
  }

  function close(): void {
    if (typeof dialog.close === 'function') dialog.close();
    else dialog.removeAttribute('open');
    onClose();
  }

  function controlMessage(status: string, action: IndexRunControlActionV1): string {
    if (status === 'accepted')
      return action === 'cancel'
        ? 'Abbruch wurde angefordert.'
        : 'Vollständiger neuer Lauf wurde eingeplant.';
    if (status === 'staleRevision')
      return 'Der Lauf hat sich geändert. Die Ansicht wird aktualisiert.';
    if (status === 'invalidState')
      return 'Diese Aktion passt nicht mehr zum aktuellen Laufzustand.';
    if (status === 'busy') return 'Ein anderer Indexauftrag ist bereits vorgemerkt.';
    return 'Die Aktion ist für diesen Lauf nicht verfügbar.';
  }

  function time(value: string | null): string {
    return value === null
      ? '–'
      : new Intl.DateTimeFormat('de-DE', { dateStyle: 'short', timeStyle: 'medium' }).format(
          Number(value),
        );
  }
  function duration(value: string): string {
    const seconds = Math.floor(Number(value) / 1000);
    if (seconds < 60) return `${seconds} s`;
    return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
  }
  function stateLabel(value: string): string {
    return (
      (
        {
          pending: 'Ausstehend',
          queued: 'Eingeplant',
          running: 'Läuft',
          cancelling: 'Abbruch läuft',
          succeeded: 'Erfolgreich',
          failed: 'Fehlgeschlagen',
          cancelled: 'Abgebrochen',
          interrupted: 'Unterbrochen',
        } as Record<string, string>
      )[value] ?? value
    );
  }
  function triggerLabel(value: string): string {
    return (
      (
        {
          initialObservation: 'Projektstart',
          fileChanges: 'Dateiänderungen',
          recoveryRescan: 'Wiederherstellung',
          manualRetry: 'Manuell erneut versucht',
        } as Record<string, string>
      )[value] ?? value
    );
  }
  function phaseLabel(value: IndexRunPhaseV1): string {
    return (
      {
        discover: 'Quellcode finden',
        hash: 'Änderungen erkennen',
        parse: 'Struktur analysieren',
        link: 'Beziehungen verknüpfen',
        rank: 'Signale gewichten',
        publish: 'Index veröffentlichen',
      } as Record<IndexRunPhaseV1, string>
    )[value];
  }
  function eventLabel(value: string): string {
    return (
      (
        {
          queued: 'Lauf eingeplant',
          started: 'Lauf gestartet',
          phaseStarted: 'Phase gestartet',
          progress: 'Fortschritt',
          fileObserved: 'Datei verarbeitet',
          diagnostic: 'Diagnose',
          cancellationRequested: 'Abbruch angefordert',
          succeeded: 'Lauf erfolgreich',
          failed: 'Lauf fehlgeschlagen',
          cancelled: 'Lauf abgebrochen',
          interrupted: 'Lauf unterbrochen',
        } as Record<string, string>
      )[value] ?? value
    );
  }
  function changeLabel(value: string): string {
    return (
      (
        {
          pending: 'Noch nicht klassifiziert',
          new: 'Neu',
          changed: 'Geändert',
          unchanged: 'Unverändert',
          deleted: 'Gelöscht',
        } as Record<string, string>
      )[value] ?? value
    );
  }
  function workLabel(value: string): string {
    return (
      (
        {
          pending: 'Hash ausstehend',
          hashed: 'Gehasht',
          reused: 'Wiederverwendet',
          notApplicable: 'Nicht anwendbar',
          structural: 'Strukturell analysiert',
          generic: 'Generisch analysiert',
          failed: 'Fehlgeschlagen',
        } as Record<string, string>
      )[value] ?? value
    );
  }
</script>

<dialog
  bind:this={dialog}
  class="index-inspector"
  aria-labelledby="index-inspector-heading"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <header>
    <div>
      <p class="eyebrow">Fast Index</p>
      <h2 id="index-inspector-heading">Indexlauf-Details</h2>
    </div>
    <button
      bind:this={closeButton}
      type="button"
      class="close"
      aria-label="Indexlauf-Details schließen"
      onclick={close}>×</button
    >
  </header>

  {#if loading}
    <p role="status">Indexlauf-Details werden geladen …</p>
  {:else if error !== null && response === null}
    <section class="notice error" role="alert">
      <p>{error}</p>
      <button
        type="button"
        onclick={() => {
          error = null;
          loading = true;
          void refresh();
        }}>Erneut laden</button
      >
    </section>
  {:else if response?.result.status === 'noProject'}
    <p>Öffne zuerst ein lokales Projekt.</p>
  {:else if response?.result.status === 'noRuns'}
    <p>Noch kein Fast-Index-Lauf vorhanden.</p>
  {:else if response?.result.status === 'available'}
    {@const available = response.result}
    <div class="run-switch" aria-label="Indexlauf auswählen">
      <button
        type="button"
        class:active={selected === 'current'}
        onclick={() => chooseRun('current')}
        >{['queued', 'running', 'cancelling'].includes(available.current.state)
          ? 'Aktueller Lauf'
          : 'Letzter Lauf'}</button
      >
      {#if available.previous !== null}
        <button
          type="button"
          class:active={selected === 'previous'}
          onclick={() => chooseRun('previous')}>Vorheriger Lauf</button
        >
      {/if}
    </div>
    {@const run = selectedRun()}
    {#if run !== null}
      {#if stalled(run)}
        <section class="notice warning" role="alert">
          <strong>Seit mindestens 60 Sekunden kein Fortschritt.</strong>
          <span>A^3 beendet den Lauf nicht automatisch. Du kannst ihn kontrolliert abbrechen.</span>
          {#if run.state !== 'cancelling'}<button
              type="button"
              disabled={actionPending}
              onclick={() => runAction('cancel')}>Lauf abbrechen</button
            >{/if}
        </section>
      {/if}
      {#if run.detailsIncomplete}<p class="notice warning">
          Das Diagnosejournal ist unvollständig; das Indexergebnis kann trotzdem gültig sein.
        </p>{/if}
      {#if error !== null}<p role="alert" class="notice error">{error}</p>{/if}
      {#if actionNotice !== null}<p role="status" class="notice">{actionNotice}</p>{/if}

      <nav class="tabs" aria-label="Indexlauf-Details">
        <button
          type="button"
          aria-current={tab === 'overview' ? 'page' : undefined}
          onclick={() => chooseTab('overview')}>Übersicht</button
        >
        <button
          type="button"
          aria-current={tab === 'files' ? 'page' : undefined}
          onclick={() => chooseTab('files')}>Dateien</button
        >
        <button
          type="button"
          aria-current={tab === 'events' ? 'page' : undefined}
          onclick={() => chooseTab('events')}>Fehler &amp; Ereignisse</button
        >
      </nav>

      {#if tab === 'overview'}
        <section class="overview" aria-label="Laufübersicht">
          <dl class="facts">
            <div>
              <dt>Zustand</dt>
              <dd>{stateLabel(run.state)}</dd>
            </div>
            <div>
              <dt>Auslöser</dt>
              <dd>{triggerLabel(run.trigger)}</dd>
            </div>
            <div>
              <dt>Start</dt>
              <dd>{time(run.startedAtUnixMillis)}</dd>
            </div>
            <div>
              <dt>Ende</dt>
              <dd>{time(run.endedAtUnixMillis)}</dd>
            </div>
            <div>
              <dt>Dauer</dt>
              <dd>{duration(run.durationMillis)}</dd>
            </div>
            <div>
              <dt>Letzte Aktivität</dt>
              <dd>{time(run.lastActivityAtUnixMillis)}</dd>
            </div>
          </dl>
          {#if run.previousPublicationAvailable}<p class="snapshot">
              Der zuletzt veröffentlichte Index bleibt während dieses Laufs nutzbar.
            </p>{/if}
          {#if run.currentFile !== null}<p>
              <strong>Aktuelle Datei:</strong>
              <code title={run.currentFile.truncated ? 'Anzeige gekürzt' : undefined}
                >{run.currentFile.display}{run.currentFile.truncated ? ' …' : ''}</code
              >
            </p>{/if}
          <ol class="timeline">
            {#each run.phases as phase (phase.phase)}
              <li class={phase.state}>
                <span>{phaseLabel(phase.phase)}</span>
                <strong>{stateLabel(phase.state)}</strong>
                {#if phase.completed !== null && phase.total !== null}<small
                    >{phase.completed}/{phase.total}</small
                  >{/if}
              </li>
            {/each}
          </ol>
          <dl class="counts">
            <div>
              <dt>Berücksichtigt</dt>
              <dd>{run.counts.discovered}</dd>
            </div>
            <div>
              <dt>Noch nicht klassifiziert</dt>
              <dd>{run.counts.pending}</dd>
            </div>
            <div>
              <dt>Neu</dt>
              <dd>{run.counts.newFiles}</dd>
            </div>
            <div>
              <dt>Geändert</dt>
              <dd>{run.counts.changed}</dd>
            </div>
            <div>
              <dt>Unverändert</dt>
              <dd>{run.counts.unchanged}</dd>
            </div>
            <div>
              <dt>Gelöscht</dt>
              <dd>{run.counts.deleted}</dd>
            </div>
            <div>
              <dt>Gehasht</dt>
              <dd>{run.counts.hashed}</dd>
            </div>
            <div>
              <dt>Hash wiederverwendet</dt>
              <dd>{run.counts.hashReused}</dd>
            </div>
            <div>
              <dt>Strukturell analysiert</dt>
              <dd>{run.counts.structural}</dd>
            </div>
            <div>
              <dt>Analyse wiederverwendet</dt>
              <dd>{run.counts.parseReused}</dd>
            </div>
            <div>
              <dt>Generisch</dt>
              <dd>{run.counts.generic}</dd>
            </div>
            <div>
              <dt>Fehlgeschlagen</dt>
              <dd>{run.counts.failed}</dd>
            </div>
          </dl>
        </section>
      {:else if tab === 'files'}
        <section aria-label="Verarbeitete Dateien">
          {#if run.currentFile !== null}<p>
              <strong>Aktuell:</strong> <code>{run.currentFile.display}</code>
            </p>{/if}
          <form class="file-query" onsubmit={applyFileQuery}>
            <label
              >Suche <input
                bind:value={search}
                maxlength="256"
                placeholder="Repository-relativer Pfad"
              /></label
            >
            <label
              >Ergebnis <select bind:value={filter}
                ><option value="all">Alle</option><option value="new">Neu</option><option
                  value="changed">Geändert</option
                ><option value="unchanged">Unverändert</option><option value="deleted"
                  >Gelöscht</option
                ><option value="hashed">Gehasht</option><option value="hashReused"
                  >Hash wiederverwendet</option
                ><option value="structural">Strukturell</option><option value="parseReused"
                  >Analyse wiederverwendet</option
                ><option value="generic">Generisch</option><option value="failed"
                  >Fehlgeschlagen</option
                ></select
              ></label
            >
            <button type="submit" disabled={filesLoading}>Anwenden</button>
          </form>
          {#if filesLoading && files === null}<p role="status">Dateien werden geladen …</p>
          {:else if files?.result.status === 'page'}
            <p>{files.result.total} passende Dateien</p>
            <div class="file-list">
              {#each files.result.files as file, index (`${file.path.display}:${index}`)}
                <article>
                  <code title={file.path.truncated ? 'Anzeige gekürzt' : undefined}
                    >{file.path.display}{file.path.truncated ? ' …' : ''}</code
                  ><span
                    >{changeLabel(file.change)} · {workLabel(file.hash)}{file.parse === null
                      ? ''
                      : ` · ${workLabel(file.parse)}`}</span
                  >{#each file.failures as failure (failure.code)}<small
                      >{failure.explanation}</small
                    >{/each}{#if file.failuresTruncated}<small
                      >Weitere Diagnosen wurden begrenzt.</small
                    >{/if}
                </article>
              {:else}<p>Keine passenden Dateien.</p>{/each}
            </div>
            <div class="pagination">
              <button
                type="button"
                disabled={files.result.previousCursor === null || filesLoading}
                onclick={() =>
                  loadFiles(
                    run,
                    files?.result.status === 'page' ? files.result.previousCursor : null,
                  )}>Zurück</button
              ><button
                type="button"
                disabled={files.result.nextCursor === null || filesLoading}
                onclick={() =>
                  loadFiles(run, files?.result.status === 'page' ? files.result.nextCursor : null)}
                >Weiter</button
              >
            </div>
          {:else if files !== null}<p role="status">
              Die Dateiseite ist nicht mehr aktuell und wird neu geladen.
            </p>{/if}
        </section>
      {:else}
        <section aria-label="Fehler und Ereignisse">
          {#if run.failure !== null}<article class="failure">
              <strong>{run.failure.explanation}</strong>
              <p>{run.failure.recovery}</p>
            </article>{/if}
          <ol class="events">
            {#each [...run.events].reverse() as event (event.revision)}
              <li>
                <time datetime={new Date(Number(event.occurredAtUnixMillis)).toISOString()}
                  >{time(event.occurredAtUnixMillis)}</time
                ><strong>{eventLabel(event.kind)}</strong>{#if event.phase !== null}<span
                    >{phaseLabel(event.phase)}</span
                  >{/if}{#if event.file !== null}<code
                    >{event.file.display}{event.file.truncated ? ' …' : ''}</code
                  >{/if}{#if event.failure !== null}<p>
                    {event.failure.explanation}<br />{event.failure.recovery}
                  </p>{/if}
              </li>
            {:else}<li>Noch keine Ereignisse.</li>{/each}
          </ol>
        </section>
      {/if}

      <footer>
        {#if ['queued', 'running'].includes(run.state)}<button
            type="button"
            disabled={actionPending}
            onclick={() => runAction('cancel')}>Abbrechen</button
          >{/if}
        {#if ['failed', 'cancelled', 'interrupted'].includes(run.state)}<button
            type="button"
            disabled={actionPending}
            onclick={() => runAction('retry')}>Neu versuchen</button
          >{/if}
        <button type="button" onclick={close}>Schließen</button>
      </footer>
    {/if}
  {/if}
</dialog>

<style>
  .index-inspector {
    width: min(72rem, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    border: 1px solid var(--color-border);
    border-radius: 0.8rem;
    background: var(--color-surface-raised);
    color: var(--color-text);
    padding: 0;
  }
  .index-inspector::backdrop {
    background: color-mix(in srgb, var(--color-overlay) 62%, transparent);
  }
  header,
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--color-border);
  }
  footer {
    justify-content: flex-end;
    border-top: 1px solid var(--color-border);
    border-bottom: 0;
  }
  h2,
  .eyebrow {
    margin: 0;
  }
  .eyebrow {
    color: var(--color-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .close {
    font-size: 1.5rem;
    min-width: 2.3rem;
  }
  .run-switch,
  .tabs,
  .overview,
  section[aria-label='Verarbeitete Dateien'],
  section[aria-label='Fehler und Ereignisse'],
  .notice {
    margin: 1rem 1.25rem;
  }
  .run-switch,
  .tabs,
  .pagination,
  .file-query {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  button,
  input,
  select {
    border: 1px solid var(--color-border);
    border-radius: 0.4rem;
    background: var(--color-surface-muted);
    color: inherit;
    padding: 0.5rem 0.7rem;
  }
  button {
    cursor: pointer;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  button.active,
  [aria-current='page'] {
    border-color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  }
  .notice {
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    padding: 0.75rem;
    display: grid;
    gap: 0.35rem;
  }
  .warning {
    border-color: var(--color-warning-strong);
  }
  .error,
  .failure {
    border-color: var(--color-danger-strong);
  }
  .facts,
  .counts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: 0.6rem;
  }
  .facts div,
  .counts div {
    padding: 0.65rem;
    border-radius: 0.45rem;
    background: var(--color-surface-muted);
  }
  dt {
    color: var(--color-muted);
    font-size: 0.78rem;
  }
  dd {
    margin: 0.2rem 0 0;
  }
  .timeline {
    display: grid;
    grid-template-columns: repeat(6, minmax(7rem, 1fr));
    list-style: none;
    padding: 0;
    overflow-x: auto;
  }
  .timeline li {
    border-top: 3px solid var(--color-border-strong);
    padding: 0.65rem 0.4rem;
    display: grid;
    gap: 0.25rem;
  }
  .timeline li.running {
    border-color: var(--color-info);
  }
  .timeline li.succeeded {
    border-color: var(--color-positive);
  }
  .timeline li.failed {
    border-color: var(--color-danger);
  }
  .file-query {
    align-items: end;
  }
  .file-query label {
    display: grid;
    gap: 0.3rem;
    flex: 1 1 15rem;
  }
  .file-list {
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    max-height: 23rem;
    overflow: auto;
  }
  .file-list article {
    display: grid;
    gap: 0.25rem;
    padding: 0.65rem;
    border-bottom: 1px solid var(--color-border);
  }
  .file-list span,
  small {
    color: var(--color-muted);
  }
  .events {
    display: grid;
    gap: 0.6rem;
    padding-left: 1.3rem;
    max-height: 28rem;
    overflow: auto;
  }
  .events li {
    display: grid;
    gap: 0.25rem;
  }
  .events time {
    color: var(--color-muted);
    font-size: 0.8rem;
  }
  code {
    overflow-wrap: anywhere;
  }
  .snapshot {
    color: var(--color-muted);
  }
  @media (max-width: 760px) {
    .timeline {
      grid-template-columns: 1fr;
    }
  }
</style>

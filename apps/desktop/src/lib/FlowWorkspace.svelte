<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    queryFunctionFlows,
    type FlowDirection,
    type FlowEntry,
    type FlowQuery,
    type FlowResponse,
    type FlowSelection,
    type FlowStepKind,
    type FlowTrace,
    type FlowValue,
    type FlowView,
  } from './function-flow';
  import {
    queryProjectMapSourcePreview,
    type ProjectMapSourcePreviewQueryV1,
    type ProjectMapSourcePreviewResponseV1,
    type ProjectMapSourcePreviewV1,
  } from './project-map-source-preview';
  import type { ProjectMapEntitySelectionV1 } from './project-map-atlas';

  interface Props {
    projectKey: string | null;
    publicationKey: string | null;
    indexBusy?: boolean;
    initialSelection?: FlowSelection | null;
    loader?: (query: FlowQuery) => Promise<FlowResponse>;
    sourceLoader?: (
      query: ProjectMapSourcePreviewQueryV1,
    ) => Promise<ProjectMapSourcePreviewResponseV1>;
    onMap?: (selection: ProjectMapEntitySelectionV1) => void;
  }
  let {
    projectKey,
    publicationKey,
    indexBusy = false,
    initialSelection = null,
    loader = queryFunctionFlows,
    sourceLoader = queryProjectMapSourcePreview,
    onMap,
  }: Props = $props();
  let term = $state('');
  let entries = $state<FlowEntry[]>([]);
  let more = $state(false);
  let catalogOffset = $state(0);
  let flow = $state<FlowView | null>(null);
  let trace = $state<FlowTrace | null>(null);
  let preview = $state<ProjectMapSourcePreviewV1 | null>(null);
  let stepOffset = $state(0);
  let valueOffset = $state(0);
  let expanded = $state<number | null>(null);
  let busy = $state(false);
  let message = $state('');
  let error = $state(false);
  let request = 0;
  let mounted = false;
  const labels: Record<FlowStepKind, string> = {
    call: 'Funktion aufrufen',
    process: 'Skript oder Prozess starten',
    assign: 'Wert festlegen',
    condition: 'Bedingung prüfen',
    branch: 'Möglicher Zweig',
    loop: 'Wiederholen',
    return: 'Ergebnis zurückgeben',
    throw: 'Fehler weitergeben',
    break: 'Wiederholung verlassen',
    continue: 'Nächster Durchlauf',
    await: 'Auf Ergebnis warten',
    handler: 'Fehler oder Abschluss behandeln',
    deferred: 'Später ausführbarer Code',
    unknown: 'Nicht sicher analysierbar',
  };
  const categories = {
    entrypoint: 'Startpunkte',
    script: 'Skripte',
    test: 'Tests',
    function: 'Funktionen',
  } as const;
  const valueLabels = {
    parameter: 'Eingabe',
    local: 'Lokaler Wert',
    external: 'Externer Ursprung',
    callResult: 'Aufrufergebnis',
    merge: 'Mögliche Werte',
    scriptArgument: 'Skriptargument',
  };
  onMount(() => {
    mounted = true;
    return () => {
      mounted = false;
      request += 1;
    };
  });
  $effect(() => {
    const anchor = [projectKey, publicationKey, indexBusy];
    const initial = initialSelection;
    untrack(() => {
      request += 1;
      flow = null;
      trace = null;
      preview = null;
      expanded = null;
      entries = [];
      message = '';
      busy = false;
      if (anchor[0] && !anchor[2]) {
        if (initial) void inspect(initial, 0, 0);
        else void catalog(0);
      }
    });
  });
  async function run(query: FlowQuery): Promise<FlowResponse | null> {
    const ticket = ++request;
    busy = true;
    error = false;
    message = '';
    try {
      const response = await loader(query);
      if (!mounted || ticket !== request) return null;
      if (['noProject', 'noPublishedIndex', 'selectionChanged'].includes(response.result.status)) {
        flow = null;
        trace = null;
        preview = null;
        entries = [];
        message =
          response.result.status === 'selectionChanged'
            ? 'Der Code hat sich geändert oder dieser Ablauf ist noch nicht verfügbar. Bitte wähle ihn im aktuellen Index erneut aus.'
            : 'Für dieses Projekt liegt noch keine aktuelle Ablaufanalyse vor. Fast Index erstellt sie zusammen mit dem normalen Index.';
      }
      return response;
    } catch {
      if (mounted && ticket === request) {
        error = true;
        message =
          'Die aktuellen Ablaufdaten konnten nicht sicher geladen werden. Bitte erneut versuchen.';
      }
      return null;
    } finally {
      if (mounted && ticket === request) busy = false;
    }
  }
  async function catalog(offset = 0): Promise<void> {
    const response = await run({ kind: 'catalog', term: term.trim(), offset });
    if (response?.result.status === 'catalog') {
      entries = response.result.page.entries;
      more = response.result.page.hasMore;
      catalogOffset = offset;
      flow = null;
      trace = null;
      preview = null;
    }
  }
  async function inspect(selection: FlowSelection, steps = 0, values = 0): Promise<void> {
    const response = await run({
      kind: 'inspect',
      selection,
      stepOffset: steps,
      valueOffset: values,
    });
    if (response?.result.status === 'flow') {
      flow = response.result.flow;
      trace = null;
      preview = null;
      expanded = null;
      stepOffset = steps;
      valueOffset = values;
    }
  }
  async function traceValue(value: FlowValue, direction: FlowDirection): Promise<void> {
    if (!flow) return;
    const response = await run({
      kind: 'trace',
      selection: flow.selection,
      value: value.id,
      direction,
    });
    if (response?.result.status === 'trace') trace = response.result.trace;
  }
  async function showSource(): Promise<void> {
    if (!flow?.source.preview) return;
    const ticket = ++request;
    busy = true;
    error = false;
    try {
      const response = await sourceLoader({ kind: 'index', evidence: flow.source.preview });
      if (!mounted || ticket !== request) return;
      if (response.result.status === 'available') preview = response.result.preview;
      else {
        preview = null;
        message = 'Die Quelle ist nicht mehr aktuell. Bitte den Ablauf neu auswählen.';
      }
    } catch {
      if (mounted && ticket === request) {
        error = true;
        message = 'Der Quelltext konnte nicht sicher gelesen werden.';
      }
    } finally {
      if (mounted && ticket === request) busy = false;
    }
  }
  function valueName(id: number): string {
    return flow?.values.find((v) => v.id === id)?.name ?? 'Wert auf einer anderen Seite';
  }
  async function showStepSource(step: number): Promise<void> {
    if (!flow) return;
    const response = await run({ kind: 'source', selection: flow.selection, step });
    if (response?.result.status === 'source') preview = response.result.preview;
  }
</script>

<section class="flows" aria-labelledby="flows-heading" aria-busy={busy}>
  <header class="flows-heading">
    <div>
      <p class="eyebrow">CODE ERKUNDEN</p>
      <h2 id="flows-heading">Abläufe verstehen</h2>
      <p>Was ruft was auf – und woher kommen die Werte?</p>
    </div>
    {#if flow}<button type="button" onclick={() => catalog(0)} disabled={busy}
        >← Alle Abläufe</button
      >{/if}
  </header>
  <p class="explanation">
    Eine Landkarte möglicher Wege im Code, keine Aufzeichnung einer Ausführung. Bedingungen,
    Wiederholungen und unbekannte Ziele bleiben sichtbar. Hier wird kein Code ausgeführt.
  </p>
  {#if !projectKey}
    <div class="empty">
      <h3>Mit einem Projekt beginnen</h3>
      <p>
        Öffne einen lokalen Worktree unter „Projects“. Die Analyse entsteht automatisch im Fast
        Index.
      </p>
    </div>
  {:else if indexBusy}
    <div class="empty" role="status">
      <h3>Der Code wird neu eingelesen</h3>
      <p>Die bisherigen Abläufe sind ausgeblendet, bis der aktuelle Fast Index bereit ist.</p>
    </div>
  {:else}
    {#if busy}<p role="status">Aktuelle Belege werden geprüft …</p>{/if}
    {#if message}<div class="notice" class:error role={error ? 'alert' : 'status'}>
        <p>{message}</p>
        <button type="button" disabled={busy} onclick={() => catalog(0)}>Neu laden</button>
      </div>{/if}
    {#if !flow}
      <form
        class="search"
        onsubmit={(event) => {
          event.preventDefault();
          void catalog(0);
        }}
      >
        <label for="flow-search">Funktion, Skript oder Datei suchen</label>
        <div>
          <input
            id="flow-search"
            bind:value={term}
            maxlength="512"
            placeholder="Zum Beispiel: Anmeldung oder export"
          /><button type="submit" disabled={busy}>Suchen</button>
        </div>
      </form>
      {#if entries.length === 0 && !busy && !message}<p>
          Keine passenden Abläufe auf dieser Seite gefunden.
        </p>{/if}
      <div class="catalog">
        {#each Object.entries(categories) as [category, label] (category)}
          {@const group = entries.filter((entry) => entry.category === category)}
          {#if group.length}
            <section aria-label={label}>
              <h3>{label}</h3>
              <ul>
                {#each group as entry (entry.selection.root)}
                  <li>
                    <button
                      class="entry"
                      type="button"
                      disabled={busy}
                      onclick={() => inspect(entry.selection)}
                      ><strong>{entry.name}</strong><span
                        >{entry.source.path} · Zeile {entry.source.line}</span
                      ><small>Ablauf öffnen →</small></button
                    >
                  </li>
                {/each}
              </ul>
            </section>
          {/if}
        {/each}
      </div>
      <nav class="pagination" aria-label="Abläufe-Seiten">
        <button
          type="button"
          disabled={busy || catalogOffset === 0}
          onclick={() => catalog(Math.max(0, catalogOffset - 50))}>Zurück</button
        ><span>Seite {catalogOffset / 50 + 1}</span><button
          type="button"
          disabled={busy || !more}
          onclick={() => catalog(catalogOffset + 50)}>Weiter</button
        >
      </nav>
    {:else}
      <nav class="breadcrumbs" aria-label="Dein Weg durch die Aufrufe">
        {#each flow.breadcrumbs as crumb, index (index)}
          {#if index > 0}<span aria-hidden="true">→</span>{/if}
          <button
            type="button"
            disabled={busy}
            aria-current={index === flow.breadcrumbs.length - 1 ? 'location' : undefined}
            onclick={() => inspect(crumb.selection)}>{crumb.name}</button
          >
        {/each}
      </nav>
      <div class="function-heading">
        <div>
          <h3>{flow.name}</h3>
          <p>{flow.source.path} · ab Zeile {flow.source.line}</p>
        </div>
        <div class="actions">
          <button type="button" disabled={busy || !flow.source.preview} onclick={showSource}
            >Quelltext ansehen</button
          >
          {#if onMap && flow.source.mapSelection}<button
              type="button"
              onclick={() => {
                if (flow?.source.mapSelection) onMap?.(flow.source.mapSelection);
              }}>In der Karte zeigen</button
            >{/if}
        </div>
      </div>
      {#if flow.gaps.length}
        <details class="notice">
          <summary
            >{flow.gaps.length}{flow.gapsTruncated ? '+' : ''} Stellen sind nur teilweise analysiert</summary
          >
          <ul>
            {#each flow.gaps as gap, index (index)}<li>
                Zeile {gap.line}: {gap.kind === 'dynamic'
                  ? 'Das Ziel oder der Zustand hängt von der Ausführung ab.'
                  : gap.kind === 'limit'
                    ? 'Die feste Analysegrenze wurde erreicht.'
                    : gap.kind === 'parseError'
                      ? 'Dieser Codeabschnitt konnte nicht vollständig gelesen werden.'
                      : 'Diese Sprachkonstruktion ist noch nicht vollständig unterstützt.'}
              </li>{/each}
          </ul>
        </details>
      {/if}
      <div class="flow-layout">
        <section class="steps" aria-labelledby="flow-steps-heading">
          <h4 id="flow-steps-heading">Schritt für Schritt <span>{flow.stepTotal}</span></h4>
          {#if flow.steps.length === 0}<p>
              Hier wurden keine ausführbaren Schritte erkannt. Deklarationen allein führen ihre
              Funktionskörper nicht aus.
            </p>{/if}
          <ol class="step-list">
            {#each flow.steps as step (step.id)}
              <li class:conditional={step.parent !== null}>
                <div class="step-row">
                  <button
                    type="button"
                    class="step-main"
                    aria-expanded={expanded === step.id}
                    onclick={() => (expanded = expanded === step.id ? null : step.id)}
                    ><span class="step-number">{step.id}</span><span
                      ><strong>{labels[step.kind]}{step.name ? ': ' + step.name : ''}</strong><small
                        >Zeile {step.line}{step.parent !== null
                          ? ' · innerhalb von Schritt ' + step.parent
                          : ''}</small
                      ></span
                    ></button
                  >
                  {#if step.target}<button
                      class="open-call"
                      type="button"
                      disabled={busy}
                      aria-label={'Aufruf ' + (step.name ?? 'Funktion') + ' öffnen'}
                      onclick={() => {
                        if (step.target) void inspect(step.target);
                      }}>Hinein →</button
                    >{/if}
                </div>
                {#if expanded === step.id}<div class="step-detail">
                    <button type="button" disabled={busy} onclick={() => showStepSource(step.id)}
                      >Quelle dieses Schritts</button
                    >
                    {#if (step.kind === 'call' || step.kind === 'process') && !step.target && step.processMode !== 'compileOnly'}<p
                      >
                        Kein eindeutig auflösbares lokales Ziel – oder die maximale Aufruftiefe ist
                        erreicht.
                      </p>{/if}
                    {#if step.kind === 'branch' || step.kind === 'condition'}<p>
                        Dieser Weg ist eine Möglichkeit. Welche Alternative läuft, entscheidet sich
                        erst bei der Ausführung.
                      </p>{/if}
                    {#if step.kind === 'loop'}<p>
                        Der Körper kann mehrfach durchlaufen werden. Eine feste Anzahl wird nicht
                        behauptet.
                      </p>{/if}
                    {#if step.processMode === 'wait'}<p>
                        Dieser Aufruf wartet auf das Ende des Prozesses. Sein Ergebnis ist kein
                        Funktionsrückgabewert des Skripts.
                      </p>{/if}
                    {#if step.processMode === 'spawn'}<p>
                        Startet einen Prozess. Wann dieser fertig ist, ist hier nicht festgelegt.
                      </p>{/if}
                    {#if step.processMode === 'compileOnly'}<p>
                        Kompiliert Code; das erzeugte Programm wird dadurch nicht ausgeführt.
                      </p>{/if}
                    {#if step.valuesTruncated}<p>
                        Weitere Wertverknüpfungen sind aus Platzgründen ausgeblendet.
                      </p>{/if}
                    {#if step.inputs.length}<p>
                        Liest: {step.inputs.map(valueName).join(', ')}
                      </p>{/if}
                    {#if step.outputs.length}<p>
                        Erzeugt: {step.outputs.map(valueName).join(', ')}
                      </p>{/if}
                  </div>{/if}
              </li>
            {/each}
          </ol>
          <nav class="pagination" aria-label="Schritte-Seiten">
            <button
              type="button"
              disabled={busy || stepOffset === 0}
              onclick={() => {
                if (flow) void inspect(flow.selection, Math.max(0, stepOffset - 50), valueOffset);
              }}>Zurück</button
            ><span
              >{flow.stepTotal === 0 ? 0 : stepOffset + 1}–{Math.min(
                stepOffset + 50,
                flow.stepTotal,
              )} von {flow.stepTotal}</span
            ><button
              type="button"
              disabled={busy || stepOffset + 50 >= flow.stepTotal}
              onclick={() => {
                if (flow) void inspect(flow.selection, stepOffset + 50, valueOffset);
              }}>Weiter</button
            >
          </nav>
        </section>
        <aside class="values" aria-labelledby="flow-values-heading">
          <h4 id="flow-values-heading">Werte verstehen</h4>
          {#if trace}
            <button type="button" onclick={() => (trace = null)}>← Zur Werteübersicht</button>
            <h5>
              {trace.direction === 'origins'
                ? 'Woher kommt dieser Wert?'
                : 'Wo wird dieser Wert verwendet?'}
            </h5>
            <p class="muted">
              Statische Wertbeziehungen, keine zeitliche Reihenfolge. Gleiche Namen können
              verschiedene Aufrufe bezeichnen.
            </p>
            <ul class="value-list">
              {#each trace.nodes as node, index (index)}<li>
                  <strong>{node.functionName} · {node.value.name}</strong><small
                    >{node.path}:{node.value.line} · Aufrufweg {node.selection.callPath.join(
                      ' → ',
                    ) || 'Start'}</small
                  >{#if node.unknown}<small>Ursprung oder Wirkung teilweise unbekannt</small
                    >{/if}<button
                    type="button"
                    disabled={busy}
                    onclick={() => inspect(node.selection)}>Diesen Kontext öffnen</button
                  >
                </li>{/each}
            </ul>
            {#if trace.truncated}<p class="notice">
                Ein Ausschnitt: Die feste Grenze für Schritte, Aufrufkontexte oder Beziehungen ist
                erreicht.
              </p>{/if}
          {:else}
            <p class="muted">
              Eingaben, lokale Definitionen und Ergebnisse – keine gespeicherten Laufzeitwerte.
            </p>
            <ul class="value-list">
              {#each flow.values as value (value.id)}<li>
                  <strong>{value.name} <small>#{value.id}</small></strong><small
                    >{valueLabels[value.kind]} · Zeile {value.line}</small
                  >
                  <div class="actions">
                    <button
                      type="button"
                      disabled={busy}
                      aria-label={'Herkunft von ' + value.name + ' Version ' + value.id}
                      onclick={() => traceValue(value, 'origins')}>Woher?</button
                    ><button
                      type="button"
                      disabled={busy}
                      aria-label={'Verwendung von ' + value.name + ' Version ' + value.id}
                      onclick={() => traceValue(value, 'uses')}>Wohin?</button
                    >
                  </div>
                </li>{/each}
            </ul>
            {#if flow.values.length === 0}<p>Keine benannten Werte erkannt.</p>{/if}
            <nav class="pagination" aria-label="Werte-Seiten">
              <button
                type="button"
                disabled={busy || valueOffset === 0}
                onclick={() => {
                  if (flow) void inspect(flow.selection, stepOffset, Math.max(0, valueOffset - 50));
                }}>Zurück</button
              ><span>{flow.valueTotal} Werte</span><button
                type="button"
                disabled={busy || valueOffset + 50 >= flow.valueTotal}
                onclick={() => {
                  if (flow) void inspect(flow.selection, stepOffset, valueOffset + 50);
                }}>Weiter</button
              >
            </nav>
          {/if}
        </aside>
      </div>
      {#if preview}<section class="source-preview" aria-label="Geprüfter Quelltext">
          <header>
            <h4>{preview.pathDisplay}</h4>
            <button type="button" onclick={() => (preview = null)}>Schließen</button>
          </header>
          <pre><code>{preview.text}</code></pre>
          <p>Ab Zeile {preview.startLine}{preview.truncatedAfter ? ' · Ausschnitt gekürzt' : ''}</p>
        </section>{/if}
    {/if}
  {/if}
</section>

<style>
  .flows {
    padding: clamp(var(--space-4), 3vw, var(--space-7));
    max-width: 76rem;
    margin: auto;
  }
  .flows-heading,
  .function-heading,
  .source-preview header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
  }
  h2,
  h3,
  h4,
  h5,
  p {
    margin: 0 0 0.65rem;
  }
  h2 {
    font-size: 1.5rem;
    font-family: var(--font-display);
    letter-spacing: -0.025em;
  }
  h3 {
    font-size: 1.25rem;
  }
  h4 {
    font-size: 1rem;
  }
  h5 {
    font-size: 1rem;
    margin-top: 1rem;
  }
  .eyebrow {
    font-size: 0.7rem;
    letter-spacing: 0.12em;
    color: var(--color-muted);
  }
  .explanation,
  .muted {
    font-size: 0.85rem;
    color: var(--color-muted);
    line-height: 1.6;
  }
  .explanation {
    max-width: 850px;
    margin: 1rem 0 1.5rem;
  }
  .search {
    max-width: 680px;
    margin: 1.5rem 0;
  }
  .search label {
    display: block;
    font-weight: 600;
    margin-bottom: 0.5rem;
  }
  .search div {
    display: flex;
    gap: 0.5rem;
  }
  .search input {
    flex: 1;
    min-width: 0;
  }
  input,
  button {
    font: inherit;
    border-radius: var(--radius-control);
    min-height: var(--control-min-size);
  }
  button {
    cursor: pointer;
    min-height: 44px;
    color: var(--color-text);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    padding: 0.5rem 0.75rem;
  }
  button:hover:not(:disabled) {
    border-color: var(--color-focus);
  }
  button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .catalog {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-6);
  }
  ul,
  .step-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .entry {
    display: grid;
    flex-direction: column;
    text-align: left;
    width: 100%;
    gap: 0.45rem;
    padding: var(--space-4) var(--space-2);
    margin-bottom: 0;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    align-items: center;
    border: 0;
    border-block-end: 1px solid var(--color-border-soft);
    border-radius: 0;
    background: transparent;
  }
  .entry span,
  .entry strong {
    overflow-wrap: anywhere;
  }
  .entry span,
  small {
    color: var(--color-muted);
    font-size: 0.78rem;
  }
  .pagination {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    justify-content: space-between;
    font-size: 0.8rem;
    margin-top: 1rem;
  }
  .breadcrumbs {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--color-border);
    padding-bottom: 1rem;
    margin-bottom: 1rem;
  }
  .breadcrumbs button[aria-current] {
    border-color: var(--color-focus);
    font-weight: 700;
  }
  .function-heading p {
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .flow-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, 340px);
    gap: 1.5rem;
    margin-top: 1.5rem;
  }
  .steps,
  .values {
    min-width: 0;
  }
  .values {
    border-left: 1px solid var(--color-border);
    padding-left: 1.5rem;
  }
  .step-list > li {
    border: 0;
    border-radius: 0;
    margin: 0;
    overflow: hidden;
    border-block-end: 1px solid var(--color-border-soft);
  }
  .step-list > li.conditional {
    border-left: 3px solid var(--color-focus);
  }
  .step-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.2rem;
  }
  .step-main {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    text-align: left;
    flex: 1;
    border: 0;
    background: transparent;
    padding: 0.65rem;
    min-width: 0;
  }
  .step-main strong {
    font-size: 0.87rem;
    overflow-wrap: anywhere;
  }
  .step-main small {
    display: block;
    margin-top: 0.3rem;
  }
  .step-number {
    font-variant-numeric: tabular-nums;
    color: var(--color-muted);
    font-size: 0.8rem;
  }
  .open-call {
    flex-shrink: 0;
    font-size: 0.78rem;
  }
  .step-detail {
    padding: var(--space-4);
    border-top: 1px solid var(--color-border);
    font-size: 0.82rem;
    overflow-wrap: anywhere;
    background: var(--color-surface-subtle);
  }
  .value-list li {
    padding: 0.8rem 0;
    border-bottom: 1px solid var(--color-border);
  }
  .value-list strong,
  .value-list small {
    display: block;
    overflow-wrap: anywhere;
  }
  .value-list .actions {
    margin-top: 0.5rem;
  }
  .value-list button {
    font-size: 0.78rem;
  }
  .value-list strong small {
    display: inline;
  }
  .notice,
  .empty {
    padding: 1rem;
    border: 0;
    border-radius: 0;
    margin: 1rem 0;
    border-inline-start: 2px solid var(--color-border);
    background: var(--color-surface-subtle);
  }
  .notice {
    font-size: 0.85rem;
  }
  .notice li {
    margin-top: 0.6rem;
  }
  .notice summary {
    cursor: pointer;
  }
  .error {
    border-color: var(--color-danger);
  }
  .source-preview {
    margin-top: 1.5rem;
    border-top: 1px solid var(--color-border);
    padding-top: 1rem;
  }
  .source-preview pre {
    overflow: auto;
    max-height: 28rem;
    padding: 1rem;
    background: var(--color-canvas);
    font-size: 0.8rem;
  }
  @media (max-width: 800px) {
    .flow-layout {
      grid-template-columns: 1fr;
    }
    .values {
      border-left: 0;
      border-top: 1px solid var(--color-border);
      padding: 1rem 0;
    }
  }
</style>

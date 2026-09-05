<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import type {
    ProjectMapAtlasNodeV1,
    ProjectMapEntityContextV1,
    ProjectMapFlowPresetV1,
    ProjectMapFlowSceneV1,
    ProjectMapIndexEvidenceSelectionV1,
    ProjectMapInventoryPageV1,
    ProjectMapInventoryViewV1,
  } from './project-map-atlas';
  import type { ProjectMapSourcePreviewV1 } from './project-map-source-preview';

  type ReadState<T> =
    | { kind: 'idle' | 'loading' | 'error' }
    | { kind: 'available'; value: T }
    | { kind: 'unavailable'; message: string };
  interface Props {
    onfunctionflow?: () => void;
    context: ReadState<ProjectMapEntityContextV1>;
    flow: ReadState<ProjectMapFlowSceneV1>;
    inventory: ReadState<ProjectMapInventoryPageV1>;
    onclose: () => void;
    onevidence: (evidence: ProjectMapIndexEvidenceSelectionV1) => void;
    onflow: (preset: ProjectMapFlowPresetV1) => void;
    oninventory: (view: ProjectMapInventoryViewV1, cursor: string | null) => void;
    onopen: (node: ProjectMapAtlasNodeV1) => void;
    onselect: (node: ProjectMapAtlasNodeV1) => void;
    preview: ReadState<ProjectMapSourcePreviewV1>;
    selected: ProjectMapAtlasNodeV1 | null;
  }

  const {
    onfunctionflow,
    selected,
    context,
    inventory,
    flow,
    preview,
    onclose,
    onopen,
    onselect,
    oninventory,
    onflow,
    onevidence,
  }: Props = $props();

  function inventoryView(node: ProjectMapAtlasNodeV1): ProjectMapInventoryViewV1 | null {
    if (node.selection?.kind === 'module') return 'files';
    if (node.selection?.kind === 'file') return 'symbols';
    if (node.selection?.kind === 'symbol') return 'members';
    return null;
  }

  function nodeEvidence(node: ProjectMapAtlasNodeV1): ProjectMapIndexEvidenceSelectionV1 | null {
    const selection = node.selection;
    if (selection?.kind === 'file') return selection;
    if (selection?.kind === 'symbol') return selection;
    return null;
  }

  function uniqueNodes(nodes: readonly ProjectMapAtlasNodeV1[]): ProjectMapAtlasNodeV1[] {
    const seen = new SvelteSet<string>();
    return nodes.filter((node) => {
      if (seen.has(node.nodeId)) return false;
      seen.add(node.nodeId);
      return true;
    });
  }

  function nodeKindLabel(kind: ProjectMapAtlasNodeV1['kind']): string {
    return (
      {
        boundary: 'Externes Ziel',
        callable: 'Funktion',
        file: 'Datei',
        manifestModule: 'Paket',
        member: 'Member',
        namespace: 'Namespace',
        pathModule: 'Modul',
        type: 'Typ',
      } as const
    )[kind];
  }

  function mappingStatusLabel(status: ProjectMapAtlasNodeV1['mappingStatus']): string | null {
    if (status === null) return null;
    return (
      {
        current: 'Aktuell',
        needsReview: 'Prüfung nötig',
        stale: 'Veraltet',
        unmapped: 'Nicht gemappt',
      } as const
    )[status];
  }

  function relationLabel(
    relation: ProjectMapEntityContextV1['relationCounts'][number]['relation'],
  ) {
    return (
      {
        builds: 'Erzeugt',
        calls: 'Ruft auf',
        configures: 'Konfiguriert',
        contains: 'Enthält',
        defines: 'Definiert',
        documents: 'Dokumentiert',
        exports: 'Exportiert',
        extends: 'Erweitert',
        implements: 'Implementiert',
        imports: 'Importiert',
        reads: 'Liest',
        tests: 'Testet',
        writes: 'Schreibt',
      } as const
    )[relation];
  }

  function inventoryLabel(node: ProjectMapAtlasNodeV1): string {
    const view = inventoryView(node);
    if (view === 'files') return 'Dateien im Modul';
    if (view === 'symbols') return 'Klassen und Funktionen';
    return 'Member';
  }

  function inventoryActionLabel(node: ProjectMapAtlasNodeV1): string {
    const view = inventoryView(node);
    if (view === 'files') return 'Dateiliste laden';
    if (view === 'symbols') return 'Symbolliste laden';
    return 'Memberliste laden';
  }

  function openLabel(node: ProjectMapAtlasNodeV1): string {
    if (node.selection?.kind === 'module') return 'Modul öffnen';
    if (node.selection?.kind === 'file') return 'Datei öffnen';
    return 'Symbol öffnen';
  }

  function summaryStats(node: ProjectMapAtlasNodeV1): { label: string; value: string }[] {
    const stats: { label: string; value: string }[] = [];
    if (node.selection?.kind === 'module') {
      stats.push({ label: 'Dateien', value: node.fileCount });
      stats.push({ label: 'Symbole', value: node.symbolCount });
    } else if (node.selection?.kind === 'file') {
      stats.push({ label: 'Symbole', value: node.symbolCount });
    } else if (node.selection?.kind === 'symbol') {
      stats.push({ label: 'Member', value: node.memberCount });
    }
    if (node.currentRiskCount !== '0')
      stats.push({ label: 'Aktuelle Risiken', value: node.currentRiskCount });
    if (node.claimBadgeCount > 0)
      stats.push({ label: 'Verifizierte Hinweise', value: String(node.claimBadgeCount) });
    return stats;
  }

  const flowPresets: { description: string; label: string; preset: ProjectMapFlowPresetV1 }[] = [
    { description: 'Wer nutzt diese Auswahl?', label: 'Aufrufer', preset: 'callers' },
    { description: 'Was wird von hier genutzt?', label: 'Aufrufe', preset: 'callees' },
    { description: 'Welche Tests sind verbunden?', label: 'Tests', preset: 'tests' },
    {
      description: 'Welche Daten werden gelesen oder geschrieben?',
      label: 'Daten',
      preset: 'dataAccess',
    },
  ];
</script>

<aside
  class:open={selected !== null}
  class="inspector"
  aria-label="Code Inspector"
  aria-hidden={selected === null}
  inert={selected === null}
>
  <header class="inspector-head">
    <div>
      <span>{selected === null ? 'Inspector' : nodeKindLabel(selected.kind)}</span>
      <h3>{selected?.displayName ?? 'Keine Auswahl'}</h3>
    </div>
    <button type="button" aria-label="Inspector schließen" onclick={onclose}>×</button>
  </header>

  {#if selected === null}
    <div class="empty">
      <strong>Region auswählen</strong>
      <p>Klick wählt aus. Enter, Doppelklick oder „Öffnen“ zoomt semantisch hinein.</p>
    </div>
  {:else}
    <section class="identity">
      <div class="status-row">
        {#if mappingStatusLabel(selected.mappingStatus) !== null}<span
            class="status"
            data-status={selected.mappingStatus}>{mappingStatusLabel(selected.mappingStatus)}</span
          >{/if}
        {#if selected.claimBadgeCount > 0}<span class="verified">✓ Durch Deep Map belegt</span>{/if}
      </div>
      <p class="description">
        {selected.purpose ?? selected.detail ?? 'Deterministisch erkannte Code-Struktur.'}
      </p>
      {#if selected.purpose !== null && selected.detail !== null}<p class="technical-detail">
          {selected.detail}
        </p>{/if}
      {#if summaryStats(selected).length > 0}<dl class="key-facts">
          {#each summaryStats(selected) as stat (stat.label)}<div>
              <dd>{stat.value}</dd>
              <dt>{stat.label}</dt>
            </div>{/each}
        </dl>{/if}
      <div class="primary-actions">
        {#if selected.selection !== null}<button
            class="primary"
            type="button"
            onclick={() => onopen(selected)}>{openLabel(selected)}</button
          >{/if}
        {#if nodeEvidence(selected) !== null}<button
            type="button"
            onclick={() => {
              const evidence = nodeEvidence(selected);
              if (evidence !== null) onevidence(evidence);
            }}>Code anzeigen</button
          >{/if}
      </div>
    </section>

    {#if preview.kind !== 'idle'}
      <section class="preview" aria-label="Sicherer Codeausschnitt">
        <div class="section-heading">
          <div>
            <span>Aktuelle Quellstelle</span>
            <h4>Codeausschnitt</h4>
          </div>
          <span class="secure">Lokal geprüft</span>
        </div>
        {#if preview.kind === 'loading'}<p role="status">
            Revision und Quelltext werden erneut geprüft …
          </p>
        {:else if preview.kind === 'error'}<p role="alert">
            Der Codeausschnitt konnte nicht sicher gelesen werden.
          </p>
        {:else if preview.kind === 'unavailable'}<p>{preview.message}</p>
        {:else if preview.kind === 'available'}
          <header>
            <strong>{preview.value.pathDisplay}</strong><span>{preview.value.lineCount} Zeilen</span
            >
          </header>
          <pre><code
              >{#each preview.value.text.split('\n') as line, index (index)}<span
                  class:highlight={preview.value.highlight !== null &&
                    preview.value.startLine + index >= preview.value.highlight.startLine &&
                    preview.value.startLine + index <= preview.value.highlight.endLine}
                  ><i>{preview.value.startLine + index}</i>{line || ' '}</span
                >{/each}</code
            ></pre>
        {/if}
      </section>
    {/if}

    {#if onfunctionflow && selected.kind === 'callable' && selected.selection?.kind === 'symbol'}
      <button type="button" onclick={onfunctionflow}>Schritte und Werte in Abläufe erkunden</button>
    {/if}
    {#if selected.selection !== null}
      <details class="inspector-section" open>
        <summary
          ><span>Verbindungen</span>{#if context.kind === 'available'}<b
              >{context.value.architectureRelationCount}</b
            >{/if}</summary
        >
        {#if context.kind === 'loading'}<p role="status">Direkte Beziehungen werden geladen …</p>
        {:else if context.kind === 'error'}<p role="alert">
            Der Kontext konnte nicht sicher geladen werden.
          </p>
        {:else if context.kind === 'unavailable'}<p>{context.message}</p>
        {:else if context.kind === 'available'}
          <div class="relation-counts">
            {#each context.value.relationCounts as count, index (`${count.relation}:${index}`)}
              <div>
                <strong>{relationLabel(count.relation)}</strong>
                <span><b>{count.incoming}</b> eingehend</span>
                <span><b>{count.outgoing}</b> ausgehend</span>
              </div>
            {:else}<p class="quiet">Keine direkten Verbindungen erkannt.</p>{/each}
          </div>
          {#if uniqueNodes(context.value.relatedNodes).length > 0}
            <h4>Direkt verbunden</h4>
            <ul class="entity-list">
              {#each uniqueNodes(context.value.relatedNodes) as node (node.nodeId)}
                <li>
                  <button type="button" onclick={() => onselect(node)}
                    ><span>{nodeKindLabel(node.kind)}</span><strong>{node.displayName}</strong
                    ></button
                  >
                </li>
              {/each}
            </ul>
          {/if}
          {#if context.value.architectureRelations.length > 0 || context.value.boundaryCount !== '0' || context.value.documentRelationCount !== '0' || context.value.sourceEdgesTruncated}
            <details class="supporting-details">
              <summary>Nachweise und weitere Angaben</summary>
              {#if context.value.architectureRelations.length > 0}
                <ul class="route-list">
                  {#each context.value.architectureRelations as relation, index (`${relation.sourceNodeId}:${relation.targetNodeId}:${relation.relation}:${index}`)}
                    <li>
                      <div>
                        <strong>{relationLabel(relation.relation)}</strong><span
                          >{relation.evidenceCount}
                          {relation.evidenceCount === '1' ? 'Nachweis' : 'Nachweise'}</span
                        >
                      </div>
                      {#if relation.claimBadgeCount > 0}<span class="verified"
                          >✓ {relation.claimBadgeCount} verifiziert</span
                        >{/if}
                      {#if relation.evidence}<button
                          type="button"
                          onclick={() => onevidence(relation.evidence!)}>Code anzeigen</button
                        >{/if}
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if context.value.boundaryCount !== '0'}<p class="notice">
                  {context.value.boundaryCount} externe oder nicht eindeutig aufgelöste Ziele. Diese Verbindungen
                  sind keine bestätigten lokalen Fakten.
                </p>{/if}
              {#if context.value.documentRelationCount !== '0'}<p class="quiet">
                  {context.value.documentRelationCount} Verbindungen zu Dokumentation erkannt.
                </p>{/if}
              {#if context.value.sourceEdgesTruncated}<p class="notice">
                  Weitere Quellverbindungen sind vorhanden. Die Ansicht bleibt bewusst begrenzt.
                </p>{/if}
            </details>
          {/if}
        {/if}
      </details>

      <details class="inspector-section">
        <summary
          ><span>{inventoryLabel(selected)}</span>{#if inventory.kind === 'available'}<b
              >{inventory.value.totalCount}</b
            >{/if}</summary
        >
        <p class="section-intro">
          Öffnet jeweils eine übersichtliche Seite mit bis zu 50 Einträgen.
        </p>
        {#if inventoryView(selected) !== null && inventory.kind === 'idle'}<button
            type="button"
            onclick={() => {
              const view = inventoryView(selected);
              if (view !== null) oninventory(view, null);
            }}>{inventoryActionLabel(selected)}</button
          >{/if}
        {#if inventory.kind === 'loading'}<p role="status">50er-Seite wird geladen …</p>
        {:else if inventory.kind === 'error'}<p role="alert">
            Das Inventar konnte nicht geladen werden.
          </p>
        {:else if inventory.kind === 'unavailable'}<p>{inventory.message}</p>
        {:else if inventory.kind === 'available'}
          <p class="page-status">
            Seite {inventory.value.pageNumber} · {inventory.value.totalCount} gesamt
          </p>
          <ul class="entity-list inventory">
            {#each uniqueNodes(inventory.value.items) as node (node.nodeId)}<li>
                <button type="button" onclick={() => onselect(node)}
                  ><span>{nodeKindLabel(node.kind)}</span><strong>{node.displayName}</strong
                  ></button
                >
              </li>{/each}
          </ul>
          <div class="pager">
            <button
              type="button"
              disabled={inventory.value.previousCursor === null}
              onclick={() => oninventory(inventory.value.view, inventory.value.previousCursor)}
              >Zurück</button
            >
            <button
              type="button"
              disabled={inventory.value.nextCursor === null}
              onclick={() => oninventory(inventory.value.view, inventory.value.nextCursor)}
              >Weiter</button
            >
          </div>
        {/if}
      </details>

      <details class="inspector-section">
        <summary
          ><span>Abläufe verfolgen</span>{#if flow.kind === 'available'}<b
              >{flow.value.targetCount}</b
            >{/if}</summary
        >
        <p class="section-intro">Zeigt nur direkte, statisch belegte Wege rund um die Auswahl.</p>
        <div class="flow-actions">
          {#each flowPresets as item (item.preset)}<button
              type="button"
              title={item.description}
              onclick={() => onflow(item.preset)}
              ><strong>{item.label}</strong><span>{item.description}</span></button
            >{/each}
        </div>
        {#if flow.kind === 'loading'}<p role="status">Flow wird deterministisch verfolgt …</p>
        {:else if flow.kind === 'error'}<p role="alert">Der Flow konnte nicht geladen werden.</p>
        {:else if flow.kind === 'unavailable'}<p>{flow.message}</p>
        {:else if flow.kind === 'available'}
          <p class="page-status">{flow.value.targetCount} verbundene Ziele</p>
          <ul class="entity-list">
            {#each uniqueNodes(flow.value.nodes) as node (node.nodeId)}<li>
                <button type="button" onclick={() => onselect(node)}
                  ><span>{nodeKindLabel(node.kind)}</span><strong>{node.displayName}</strong
                  ></button
                >
              </li>{/each}
          </ul>
          {#if flow.value.targetsTruncated}<p class="notice">
              Weitere Ziele wurden nach der festen 31er-Grenze ausgelassen.
            </p>{/if}
        {/if}
      </details>
    {:else}
      <section class="boundary-note">
        <strong>Externe Referenz</strong>
        <p>
          Dieses Ziel liegt außerhalb des eindeutig aufgelösten Projektcodes und kann nicht weiter
          geöffnet werden.
        </p>
      </section>
    {/if}
  {/if}
</aside>

<style>
  .inspector {
    flex: 0 0 auto;
    min-width: 0;
    width: 0;
    overflow: hidden auto;
    background: var(--surface);
    transition: width 140ms ease;
  }
  .inspector.open {
    width: var(--inspector-width, 380px);
  }
  .inspector-head {
    position: sticky;
    top: 0;
    z-index: 3;
    display: flex;
    justify-content: space-between;
    align-items: center;
    min-width: 300px;
    min-height: 66px;
    padding: 10px 12px 10px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .inspector-head > div {
    min-width: 0;
  }
  .inspector-head span {
    color: var(--muted);
    font-size: 0.64rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .inspector-head h3 {
    margin: 3px 0 0;
    overflow: hidden;
    font-size: 0.98rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .inspector-head button {
    flex: 0 0 auto;
    width: 44px;
    height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
    font-size: 1.2rem;
  }
  .empty,
  .identity,
  .preview,
  .boundary-note {
    min-width: 300px;
    padding: 16px;
    border-bottom: 1px solid var(--line);
  }
  .empty {
    padding-top: 48px;
    color: var(--muted);
  }
  .identity p,
  .inspector-section p,
  .preview p,
  .boundary-note p {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }
  .status-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    min-height: 22px;
  }
  .status,
  .verified,
  .secure {
    display: inline-flex;
    align-items: center;
    min-height: 22px;
    padding: 2px 7px;
    border: 1px solid var(--line);
    font-size: 0.66rem;
    font-weight: 700;
  }
  .status[data-status='current'],
  .verified,
  .secure {
    color: var(--color-positive);
  }
  .status[data-status='stale'],
  .status[data-status='needsReview'] {
    color: var(--color-warning);
  }
  .description {
    margin: 12px 0 0;
    color: var(--text) !important;
    font-size: 0.84rem !important;
  }
  .technical-detail {
    margin: 5px 0 0;
    font-size: 0.7rem !important;
  }
  .key-facts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(104px, 1fr));
    gap: 8px 14px;
    margin: 16px 0;
  }
  .key-facts div {
    min-width: 0;
    border-left: 2px solid var(--line);
    padding-left: 9px;
  }
  .key-facts dt {
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.64rem;
  }
  .key-facts dd {
    margin: 0;
    font-size: 0.94rem;
    font-weight: 750;
  }
  button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
    padding: 8px 12px;
  }
  button:hover:not(:disabled) {
    background: var(--surface-raised);
  }
  .primary {
    background: var(--accent);
    color: var(--color-on-accent);
    font-weight: 750;
  }
  .primary:hover:not(:disabled) {
    background: var(--accent);
  }
  .primary-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .inspector-section {
    min-width: 300px;
    padding: 0;
    border-bottom: 1px solid var(--line);
  }
  .inspector-section > summary {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 44px;
    padding: 14px 16px;
    cursor: pointer;
    font-weight: 700;
    list-style-position: inside;
  }
  .inspector-section > summary span {
    flex: 1 1 auto;
  }
  .inspector-section > summary b {
    color: var(--muted);
    font-size: 0.7rem;
  }
  .inspector-section > :not(summary) {
    margin-left: 16px;
    margin-right: 16px;
  }
  .inspector-section > :last-child {
    margin-bottom: 16px;
  }
  .section-intro {
    margin-top: 0;
  }
  .relation-counts {
    display: grid;
    gap: 6px;
  }
  .relation-counts > div {
    display: grid;
    grid-template-columns: minmax(90px, 1fr) auto auto;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    padding: 5px 8px;
    border: 1px solid var(--line);
    font-size: 0.68rem;
  }
  .relation-counts span {
    color: var(--muted);
    white-space: nowrap;
  }
  .relation-counts span b {
    color: var(--text);
  }
  ul {
    list-style: none;
    padding: 0;
  }
  .entity-list {
    display: grid;
    gap: 4px;
    max-height: 300px;
    margin-top: 7px;
    overflow: auto;
  }
  .entity-list button {
    display: grid;
    width: 100%;
    text-align: left;
  }
  .entity-list span {
    color: var(--muted);
    font-size: 0.64rem;
    text-transform: uppercase;
  }
  .supporting-details {
    margin: 12px 16px 16px !important;
    border: 1px solid var(--line);
  }
  .supporting-details > summary {
    min-height: 42px;
    padding: 11px;
    cursor: pointer;
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
  }
  .supporting-details > :not(summary) {
    margin-right: 11px;
    margin-left: 11px;
  }
  .supporting-details > :last-child {
    margin-bottom: 11px;
  }
  .route-list li {
    display: grid;
    grid-template-columns: minmax(90px, 1fr) auto auto;
    align-items: center;
    gap: 7px;
    min-height: 44px;
    border-bottom: 1px solid var(--line);
    font-size: 0.72rem;
  }
  .route-list li > div {
    display: grid;
  }
  .route-list li > div span {
    color: var(--muted);
    font-size: 0.65rem;
  }
  .route-list button {
    min-height: 36px;
    padding: 4px 8px;
  }
  .notice {
    padding: 9px;
    border-left: 3px solid var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 10%, transparent);
  }
  .quiet,
  .page-status {
    color: var(--muted);
  }
  .page-status {
    font-weight: 700;
  }
  .pager {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .flow-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .flow-actions button {
    display: grid;
    min-width: 0;
    min-height: 58px;
    text-align: left;
  }
  .flow-actions span {
    margin-top: 2px;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.63rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .section-heading,
  .preview header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .section-heading span:first-child {
    color: var(--muted);
    font-size: 0.64rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .section-heading h4 {
    margin: 2px 0 0;
  }
  .preview header {
    margin-top: 12px;
    font-size: 0.73rem;
  }
  .preview header strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview header span {
    flex: 0 0 auto;
    color: var(--muted);
  }
  .preview pre {
    max-height: 340px;
    overflow: auto;
    border: 1px solid var(--line);
    background: var(--color-code);
    color: var(--color-on-code);
  }
  .preview code span {
    display: block;
    white-space: pre;
  }
  .preview code span.highlight {
    background: color-mix(in srgb, var(--color-accent) 24%, transparent);
  }
  .preview i {
    display: inline-block;
    width: 42px;
    margin-right: 10px;
    color: var(--color-subtle);
    text-align: right;
    font-style: normal;
    user-select: none;
  }
  button:focus-visible,
  summary:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  @media (max-width: 899px) {
    .inspector {
      position: absolute;
      z-index: 20;
      top: 0;
      right: 0;
      bottom: 0;
      box-shadow: -10px 0 30px color-mix(in srgb, var(--color-shadow) 28%, transparent);
    }
    .inspector.open {
      width: min(390px, 92vw);
      border-left: 1px solid var(--line);
    }
  }
  @media (max-width: 420px) {
    .flow-actions {
      grid-template-columns: 1fr;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .inspector {
      transition: none;
    }
  }
</style>

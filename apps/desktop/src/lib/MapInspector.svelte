<script lang="ts">
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
    const seen: string[] = [];
    return nodes.filter((node) => {
      if (seen.includes(node.nodeId)) return false;
      seen.push(node.nodeId);
      return true;
    });
  }

  const flowPresets: { label: string; preset: ProjectMapFlowPresetV1 }[] = [
    { label: 'Aufrufer', preset: 'callers' },
    { label: 'Aufrufe', preset: 'callees' },
    { label: 'Tests', preset: 'tests' },
    { label: 'Datenzugriff', preset: 'dataAccess' },
  ];
</script>

<aside class:open={selected !== null} class="inspector" aria-label="Code Inspector">
  <header class="inspector-head">
    <div>
      <span>Inspector</span>
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
      <div>
        <span>{selected.kind}</span>{#if selected.mappingStatus}<b
            data-status={selected.mappingStatus}>{selected.mappingStatus}</b
          >{/if}
      </div>
      <p>{selected.detail ?? selected.purpose ?? 'Deterministisch erkannte Code-Struktur.'}</p>
      <dl>
        <div>
          <dt>Dateien</dt>
          <dd>{selected.fileCount}</dd>
        </div>
        <div>
          <dt>Symbole</dt>
          <dd>{selected.symbolCount}</dd>
        </div>
        <div>
          <dt>Member</dt>
          <dd>{selected.memberCount}</dd>
        </div>
        <div>
          <dt>Claims</dt>
          <dd>{selected.claimBadgeCount}</dd>
        </div>
      </dl>
      {#if selected.selection !== null}<button
          class="primary"
          type="button"
          onclick={() => onopen(selected)}>Öffnen</button
        >{/if}
      {#if nodeEvidence(selected) !== null}<button
          type="button"
          onclick={() => {
            const evidence = nodeEvidence(selected);
            if (evidence !== null) onevidence(evidence);
          }}>Codeausschnitt</button
        >{/if}
    </section>

    <details open>
      <summary>Beziehungen</summary>
      {#if context.kind === 'loading'}<p role="status">Direkte Beziehungen werden geladen …</p>
      {:else if context.kind === 'error'}<p role="alert">
          Der Kontext konnte nicht sicher geladen werden.
        </p>
      {:else if context.kind === 'unavailable'}<p>{context.message}</p>
      {:else if context.kind === 'available'}
        <div class="relation-counts">
          {#each context.value.relationCounts as count, index (`${count.relation}:${index}`)}
            <span><strong>{count.relation}</strong> ← {count.incoming} · → {count.outgoing}</span>
          {:else}<span>Keine direkten Relationen.</span>{/each}
        </div>
        <ul class="entity-list">
          {#each uniqueNodes(context.value.relatedNodes) as node (node.nodeId)}
            <li>
              <button type="button" onclick={() => onselect(node)}
                ><span>{node.kind}</span><strong>{node.displayName}</strong></button
              >
            </li>
          {/each}
        </ul>
        {#if context.value.architectureRelations.length > 0}
          <h4>Architekturrouten</h4>
          <ul class="route-list">
            {#each context.value.architectureRelations as relation, index (`${relation.sourceNodeId}:${relation.targetNodeId}:${relation.relation}:${index}`)}
              <li>
                <span>{relation.relation}</span>
                <strong>{relation.evidenceCount} Evidence</strong>
                {#if relation.claimBadgeCount > 0}<b>{relation.claimBadgeCount} Claims</b>{/if}
                {#if relation.evidence}<button
                    type="button"
                    onclick={() => onevidence(relation.evidence!)}>Quellstelle</button
                  >{/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if context.value.boundaryNodes.length > 0}<p class="notice">
            {context.value.boundaryCount} externe oder ungelöste Ziele; gestrichelte Routen sind keine
            bestätigten lokalen Fakten.
          </p>{/if}
        {#if context.value.documentRelationCount !== '0'}<p>
            {context.value.documentRelationCount} Dokumentationsbeziehungen sind im Inspector erfasst.
          </p>{/if}
      {/if}
    </details>

    <details open>
      <summary>Vollständiges Inventar</summary>
      {#if inventoryView(selected) !== null && inventory.kind === 'idle'}<button
          type="button"
          onclick={() => {
            const view = inventoryView(selected);
            if (view !== null) oninventory(view, null);
          }}
          >{inventoryView(selected) === 'files'
            ? 'Alle Dateien'
            : inventoryView(selected) === 'symbols'
              ? 'Alle Symbole'
              : 'Alle Member'} laden</button
        >{/if}
      {#if inventory.kind === 'loading'}<p role="status">50er-Seite wird geladen …</p>
      {:else if inventory.kind === 'error'}<p role="alert">
          Das Inventar konnte nicht geladen werden.
        </p>
      {:else if inventory.kind === 'unavailable'}<p>{inventory.message}</p>
      {:else if inventory.kind === 'available'}
        <p>Seite {inventory.value.pageNumber} · {inventory.value.totalCount} Einträge</p>
        <ul class="entity-list inventory">
          {#each uniqueNodes(inventory.value.items) as node (node.nodeId)}<li>
              <button type="button" onclick={() => onselect(node)}
                ><span>{node.kind}</span><strong>{node.displayName}</strong></button
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

    <details>
      <summary>Fokussierte Flows</summary>
      <div class="flow-actions">
        {#each flowPresets as item (item.preset)}<button
            type="button"
            onclick={() => onflow(item.preset)}>{item.label}</button
          >{/each}
      </div>
      {#if flow.kind === 'loading'}<p role="status">Flow wird deterministisch verfolgt …</p>
      {:else if flow.kind === 'error'}<p role="alert">Der Flow konnte nicht geladen werden.</p>
      {:else if flow.kind === 'unavailable'}<p>{flow.message}</p>
      {:else if flow.kind === 'available'}
        <p>{flow.value.targetCount} Ziele · {flow.value.preset}</p>
        <ul class="entity-list">
          {#each uniqueNodes(flow.value.nodes) as node (node.nodeId)}<li>
              <button type="button" onclick={() => onselect(node)}
                ><span>{node.kind}</span><strong>{node.displayName}</strong></button
              >
            </li>{/each}
        </ul>
        {#if flow.value.targetsTruncated}<p class="notice">
            Weitere Ziele wurden nach der festen 31er-Grenze ausgelassen.
          </p>{/if}
      {/if}
    </details>

    {#if preview.kind !== 'idle'}
      <section class="preview" aria-label="Sicherer Codeausschnitt">
        <h4>Evidence-gebundener Code</h4>
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
  {/if}
</aside>

<style>
  .inspector {
    min-width: 0;
    width: 0;
    overflow: hidden auto;
    border-left: 0 solid var(--line);
    background: var(--surface);
    transition: width 140ms ease;
  }
  .inspector.open {
    width: 360px;
    border-left-width: 1px;
  }
  .inspector-head {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    justify-content: space-between;
    align-items: center;
    min-width: 330px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .inspector-head span {
    color: var(--muted);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .inspector-head h3 {
    margin: 3px 0 0;
    font-size: 1rem;
  }
  .inspector-head button {
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
  details,
  .preview {
    min-width: 328px;
    padding: 16px;
    border-bottom: 1px solid var(--line);
  }
  .empty {
    padding-top: 48px;
    color: var(--muted);
  }
  .identity p,
  details p {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }
  .identity > div {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    text-transform: capitalize;
  }
  .identity b {
    color: var(--color-positive);
    font-size: 0.7rem;
  }
  .identity dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    margin: 14px 0;
    background: var(--line);
  }
  .identity dl div {
    padding: 10px;
    background: var(--surface-raised);
  }
  .identity dt {
    color: var(--muted);
    font-size: 0.68rem;
  }
  .identity dd {
    margin: 4px 0 0;
    font-weight: 700;
  }
  button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
    padding: 8px 12px;
  }
  .primary {
    background: var(--accent);
    color: var(--color-on-accent);
    font-weight: 750;
  }
  .identity > button {
    margin-right: 7px;
  }
  details {
    padding: 0;
  }
  summary {
    min-height: 44px;
    padding: 14px 16px;
    cursor: pointer;
    font-weight: 700;
  }
  details > :not(summary) {
    margin-left: 16px;
    margin-right: 16px;
  }
  details > :last-child {
    margin-bottom: 16px;
  }
  .relation-counts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .relation-counts span {
    padding: 6px 8px;
    border: 1px solid var(--line);
    font-size: 0.68rem;
  }
  .relation-counts strong {
    text-transform: capitalize;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  .entity-list {
    display: grid;
    gap: 5px;
    max-height: 300px;
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
  .route-list li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 7px;
    min-height: 38px;
    border-bottom: 1px solid var(--line);
    font-size: 0.72rem;
  }
  .route-list span {
    text-transform: capitalize;
  }
  .route-list button {
    min-height: 32px;
    padding: 4px 7px;
  }
  .notice {
    padding: 9px;
    border-left: 3px solid var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 10%, transparent);
  }
  .pager,
  .flow-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .preview header {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 0.73rem;
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
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .inspector {
      transition: none;
    }
  }
</style>

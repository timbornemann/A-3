<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { createFrameCoalescedResize } from './frame-coalesced-resize';
  import { atlasRelationKey, layoutAtlasNodes } from './map-atlas-layout';
  import type {
    ProjectMapAtlasNodeV1,
    ProjectMapAtlasRelationV1,
    ProjectMapAtlasSceneV1,
  } from './project-map-atlas';

  interface Props {
    lensModuleIds?: ReadonlySet<string>;
    onopen: (node: ProjectMapAtlasNodeV1) => void;
    onselect: (node: ProjectMapAtlasNodeV1) => void;
    scene: ProjectMapAtlasSceneV1;
    selectedNodeId: string | null;
  }

  const { scene, selectedNodeId, lensModuleIds = new Set(), onselect, onopen }: Props = $props();
  let host: HTMLDivElement;
  let width = $state(900);
  let height = $state(620);
  let zoom = $state(1);
  let dragging = $state(false);
  let selectionTimer: ReturnType<typeof setTimeout> | null = null;
  let dragState:
    | {
        pointerId: number;
        startX: number;
        startY: number;
        startScrollLeft: number;
        startScrollTop: number;
      }
    | undefined;
  const visibleRelations = $derived.by(() => {
    if (selectedNodeId === null) return scene.relations.slice(0, 24);
    return scene.relations.filter((relation) => isIncident(relation)).slice(0, 32);
  });
  const layout = $derived(
    layoutAtlasNodes(scene.nodes, scene.relations, width, height, visibleRelations),
  );
  const nodesById = $derived(new Map(scene.nodes.map((node) => [node.nodeId, node])));
  const connectedNodeIds = $derived(
    new Set(scene.relations.flatMap((relation) => [relation.sourceNodeId, relation.targetNodeId])),
  );
  const labeledRouteKeys = $derived(
    new Set(
      selectedNodeId === null
        ? []
        : scene.relations
            .filter((relation) => isIncident(relation))
            .slice(0, 12)
            .map(relationKey),
    ),
  );

  onMount(() => {
    width = Math.max(320, host.clientWidth || width);
    height = Math.max(280, host.clientHeight || height);
    if (typeof ResizeObserver === 'undefined') return;
    const resize = createFrameCoalescedResize((size) => {
      const nextWidth = Math.max(320, size.width);
      const nextHeight = Math.max(280, size.height);
      if (width !== nextWidth) width = nextWidth;
      if (height !== nextHeight) height = nextHeight;
    });
    const observer = new ResizeObserver(([entry]) => {
      resize.schedule({ height: entry.contentRect.height, width: entry.contentRect.width });
    });
    observer.observe(host);
    return () => {
      observer.disconnect();
      resize.dispose();
    };
  });

  function relationKey(relation: ProjectMapAtlasRelationV1): string {
    return atlasRelationKey(relation);
  }

  function isIncident(relation: ProjectMapAtlasRelationV1): boolean {
    return (
      selectedNodeId !== null &&
      (relation.sourceNodeId === selectedNodeId || relation.targetNodeId === selectedNodeId)
    );
  }

  function connectionCount(nodeId: string): number {
    return scene.relations.filter(
      (relation) => relation.sourceNodeId === nodeId || relation.targetNodeId === nodeId,
    ).length;
  }

  function relationLabel(relation: ProjectMapAtlasRelationV1): string {
    return {
      builds: 'baut',
      calls: 'ruft auf',
      configures: 'konfiguriert',
      contains: 'enthält',
      defines: 'definiert',
      documents: 'dokumentiert',
      exports: 'exportiert',
      extends: 'erweitert',
      implements: 'implementiert',
      imports: 'importiert',
      reads: 'liest',
      tests: 'testet',
      writes: 'schreibt',
    }[relation.relation];
  }

  function isLensMuted(node: ProjectMapAtlasNodeV1): boolean {
    if (lensModuleIds.size === 0 || node.selection === null) return false;
    return !lensModuleIds.has(node.selection.moduleId);
  }

  function keydown(event: KeyboardEvent, node: ProjectMapAtlasNodeV1): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      onopen(node);
    }
  }

  function scheduleSelection(event: MouseEvent, node: ProjectMapAtlasNodeV1): void {
    cancelScheduledSelection();
    if (event.detail === 0) {
      onselect(node);
      return;
    }
    selectionTimer = setTimeout(() => {
      selectionTimer = null;
      onselect(node);
    }, 180);
  }

  function openImmediately(node: ProjectMapAtlasNodeV1): void {
    cancelScheduledSelection();
    onopen(node);
  }

  function cancelScheduledSelection(): void {
    if (selectionTimer === null) return;
    clearTimeout(selectionTimer);
    selectionTimer = null;
  }

  function beginPan(event: PointerEvent): void {
    if (event.button !== 0 || !(event.target instanceof Element)) return;
    if (event.target.closest('.atlas-node')) return;
    event.preventDefault();
    dragState = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      startScrollLeft: host.scrollLeft,
      startScrollTop: host.scrollTop,
    };
    dragging = true;
    host.setPointerCapture?.(event.pointerId);
  }

  function continuePan(event: PointerEvent): void {
    if (dragState?.pointerId !== event.pointerId) return;
    host.scrollLeft = dragState.startScrollLeft - (event.clientX - dragState.startX);
    host.scrollTop = dragState.startScrollTop - (event.clientY - dragState.startY);
  }

  function endPan(event: PointerEvent): void {
    if (dragState?.pointerId !== event.pointerId) return;
    if (host.hasPointerCapture?.(event.pointerId)) host.releasePointerCapture(event.pointerId);
    dragState = undefined;
    dragging = false;
  }

  async function zoomAtPointer(event: WheelEvent): Promise<void> {
    event.preventDefault();
    const deltaMultiplier = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? height : 1;
    const nextZoom = Math.min(
      2,
      Math.max(
        0.5,
        Math.round(zoom * Math.exp(-event.deltaY * deltaMultiplier * 0.0015) * 1000) / 1000,
      ),
    );
    if (nextZoom === zoom) return;

    const bounds = host.getBoundingClientRect();
    const pointerX = event.clientX - bounds.left;
    const pointerY = event.clientY - bounds.top;
    const worldX = (host.scrollLeft + pointerX) / zoom;
    const worldY = (host.scrollTop + pointerY) / zoom;
    zoom = nextZoom;
    await tick();
    host.scrollLeft = worldX * nextZoom - pointerX;
    host.scrollTop = worldY * nextZoom - pointerY;
  }

  onDestroy(cancelScheduledSelection);

  function kindLabel(node: ProjectMapAtlasNodeV1): string {
    return {
      boundary: 'Boundary',
      callable: 'Funktion',
      file: 'Datei',
      manifestModule: 'Package',
      member: 'Member',
      namespace: 'Namespace',
      pathModule: 'Modul',
      type: 'Typ',
    }[node.kind];
  }
</script>

<div class="canvas-shell">
  <div
    class:dragging
    class="canvas-host"
    bind:this={host}
    aria-label="Progressiver Architektur-Atlas"
    data-zoom={zoom}
    role="region"
    onpointerdown={beginPan}
    onpointermove={continuePan}
    onpointerup={endPan}
    onpointercancel={endPan}
    onwheel={zoomAtPointer}
  >
    <div
      class="canvas-space"
      style={`width:${layout.width * zoom}px;height:${layout.height * zoom}px`}
    >
      <div
        class="canvas-plane"
        style={`width:${layout.width}px;height:${layout.height}px;transform:scale(${zoom})`}
      >
        {#each layout.bands as band (`${band.kind}:${band.x}`)}
          <span
            class="atlas-band-label"
            data-kind={band.kind}
            style={`left:${band.x}px;width:${band.width}px`}>{band.label}</span
          >
        {/each}
        <svg
          class="route-layer"
          viewBox={`0 0 ${layout.width} ${layout.height}`}
          aria-hidden="true"
        >
          <defs>
            <marker
              id="atlas-arrow"
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
          {#each visibleRelations as relation (relationKey(relation))}
            {@const routed = layout.routes.get(relationKey(relation))}
            {#if routed !== undefined}
              <path
                class:uncertain={relation.uncertainty !== null}
                class:incident={isIncident(relation)}
                class:muted={selectedNodeId !== null && !isIncident(relation)}
                class={`route relation-${relation.relation}`}
                data-relation={relation.relation}
                d={routed.d}
                marker-end="url(#atlas-arrow)"
              ></path>
              {#if labeledRouteKeys.has(relationKey(relation))}
                <text class="route-label" x={routed.labelX} y={routed.labelY} text-anchor="middle"
                  >{relationLabel(relation)}</text
                >
              {/if}
            {/if}
          {/each}
        </svg>

        {#each scene.nodes as node (node.nodeId)}
          {@const rect = layout.byId.get(node.nodeId)}
          {#if rect !== undefined}
            <button
              type="button"
              class="atlas-node"
              class:selected={node.nodeId === selectedNodeId}
              class:boundary={node.kind === 'boundary'}
              class:lens-muted={isLensMuted(node)}
              data-connected={connectedNodeIds.has(node.nodeId)}
              data-kind={node.kind}
              data-status={node.mappingStatus ?? 'none'}
              style={`left:${rect.x}px;top:${rect.y}px;width:${rect.width}px;height:${rect.height}px`}
              aria-label={`${node.displayName}, ${kindLabel(node)}, ${node.fileCount} Dateien, ${node.symbolCount} Symbole, ${connectionCount(node.nodeId)} sichtbare Verbindungen`}
              onclick={(event) => scheduleSelection(event, node)}
              ondblclick={() => openImmediately(node)}
              onkeydown={(event) => keydown(event, node)}
            >
              <span class="node-kind">{kindLabel(node)}</span>
              <strong>{node.displayName}</strong>
              {#if rect.height > 102 && node.detail !== null}<small>{node.detail}</small>{/if}
              <span class="node-counts">
                {#if node.kind === 'file'}{node.symbolCount} Struktursymbole
                {:else if node.kind === 'type' || node.kind === 'callable'}{node.memberCount} Member
                {:else}{node.fileCount} Dateien · {node.symbolCount} Symbole{/if}
                · {connectionCount(node.nodeId)} Routen
              </span>
              {#if node.mappingStatus !== null}
                <span class="node-status">{node.mappingStatus}</span>
              {/if}
              {#if node.claimBadgeCount > 0}<span class="claim-badge"
                  >{node.claimBadgeCount} Claims</span
                >{/if}
              {#if node.currentRiskCount !== '0'}<span class="risk-badge"
                  >{node.currentRiskCount} Risiken</span
                >{/if}
            </button>
          {/if}
        {/each}
      </div>
    </div>
  </div>
  <details class="atlas-summary">
    <summary>Nichtgrafische Zusammenfassung</summary>
    <strong>Objekte</strong>
    <ul>
      {#each scene.nodes as node (node.nodeId)}
        <li>
          <button type="button" onclick={() => onselect(node)}
            >{node.displayName} · {kindLabel(node)} · {connectionCount(node.nodeId)} Routen</button
          >
        </li>
      {/each}
    </ul>
    {#if scene.relations.length > 0}
      <strong>Verbindungen</strong>
      <ul>
        {#each scene.relations as relation (relationKey(relation))}
          <li>
            {nodesById.get(relation.sourceNodeId)?.displayName ?? 'Unbekannt'}
            → {relationLabel(relation)} →
            {nodesById.get(relation.targetNodeId)?.displayName ?? 'Unbekannt'}
          </li>
        {/each}
      </ul>
    {/if}
  </details>
</div>

<style>
  .canvas-shell {
    position: relative;
    min-width: 0;
    min-height: 280px;
    height: 100%;
    overflow: hidden;
    background: var(--surface-canvas);
  }
  .canvas-host {
    position: relative;
    min-width: 0;
    min-height: 280px;
    height: 100%;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-color: var(--line) var(--surface-canvas);
    scrollbar-width: thin;
    background: var(--surface-canvas);
    cursor: grab;
    touch-action: none;
  }
  .canvas-host.dragging {
    cursor: grabbing;
    user-select: none;
  }
  .canvas-space {
    position: relative;
    min-width: 100%;
    min-height: 100%;
  }
  .canvas-plane {
    position: relative;
    transform-origin: 0 0;
    background-color: var(--surface-canvas);
    background-image: radial-gradient(
      circle,
      color-mix(in srgb, var(--line) 52%, transparent) 1px,
      transparent 1px
    );
    background-size: 24px 24px;
  }
  .route-layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 1;
  }
  .route {
    fill: none;
    stroke: color-mix(in srgb, var(--accent) 50%, var(--line));
    stroke-width: 1.65;
    stroke-linecap: square;
    stroke-linejoin: miter;
    opacity: 0.58;
    vector-effect: non-scaling-stroke;
    transition:
      opacity 120ms ease,
      stroke-width 120ms ease;
  }
  .route.incident {
    stroke: var(--focus);
    stroke-width: 3;
    opacity: 1;
  }
  .route.muted {
    opacity: 0.1;
  }
  .route.uncertain {
    stroke-dasharray: 7 5;
    opacity: 0.8;
  }
  .route.incident.uncertain {
    opacity: 1;
  }
  .relation-contains,
  .relation-defines {
    opacity: 0.32;
  }
  .relation-tests {
    stroke: var(--color-positive);
  }
  .relation-calls {
    stroke: var(--color-info);
  }
  .relation-reads,
  .relation-writes {
    stroke: var(--color-warning);
  }
  .route-label {
    fill: var(--text);
    stroke: var(--surface-canvas);
    stroke-width: 5px;
    paint-order: stroke;
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .atlas-band-label {
    position: absolute;
    z-index: 2;
    top: 16px;
    display: block;
    overflow: hidden;
    padding-top: 7px;
    border-top: 1px dashed var(--line);
    color: var(--muted);
    font-size: 0.65rem;
    font-weight: 720;
    letter-spacing: 0.06em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .atlas-band-label[data-kind='boundary'] {
    color: var(--color-warning);
  }
  .atlas-node {
    position: absolute;
    z-index: 2;
    min-width: 44px;
    min-height: 44px;
    padding: 12px;
    border: 1px solid var(--line);
    border-left: 4px solid var(--accent);
    border-radius: 0;
    background: var(--surface-raised);
    color: var(--text);
    text-align: left;
    overflow: hidden;
    box-shadow: 0 5px 18px color-mix(in srgb, var(--surface-canvas) 62%, transparent);
    transition:
      background-color 120ms ease,
      border-color 120ms ease;
    cursor: pointer;
  }
  .atlas-node:hover {
    background: color-mix(in srgb, var(--surface-raised) 84%, var(--accent));
    border-color: var(--accent);
  }
  .atlas-node.selected {
    border-color: var(--focus);
    outline: 3px solid var(--focus);
    outline-offset: -3px;
    z-index: 3;
  }
  .atlas-node.boundary {
    border-style: dashed;
    border-left-color: var(--color-warning);
    background: var(--surface-raised);
  }
  .atlas-node[data-connected='false']:not(.boundary) {
    opacity: 0.72;
  }
  .atlas-node.lens-muted {
    opacity: 0.22;
    filter: saturate(0.25);
  }
  .atlas-node[data-status='current'] {
    border-left-color: var(--color-positive);
  }
  .atlas-node[data-status='stale'] {
    border-left-color: var(--color-warning);
  }
  .atlas-node[data-status='needsReview'] {
    border-left-color: var(--color-danger);
  }
  .atlas-node[data-status='unmapped'] {
    border-left-color: var(--color-neutral);
  }
  .atlas-node strong,
  .atlas-node small,
  .atlas-node span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .atlas-node strong {
    margin-top: 5px;
    font-size: 0.86rem;
  }
  .node-kind {
    color: var(--color-accent-text);
    font-size: 0.65rem;
    font-weight: 760;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .atlas-node small,
  .node-counts {
    margin-top: 5px;
    color: var(--muted);
    font-size: 0.7rem;
  }
  .node-status,
  .claim-badge,
  .risk-badge {
    display: inline-block !important;
    margin: 7px 5px 0 0;
    color: var(--muted);
    font-size: 0.65rem;
    text-transform: capitalize;
  }
  .claim-badge {
    color: var(--color-accent-text);
  }
  .risk-badge {
    color: var(--color-danger);
  }
  .atlas-summary {
    position: absolute;
    z-index: 6;
    left: 10px;
    bottom: 10px;
    max-width: min(360px, 70%);
    max-height: min(60%, 420px);
    overflow: auto;
    border: 1px solid var(--line);
    background: var(--surface);
    font-size: 0.75rem;
    box-shadow: 0 8px 24px color-mix(in srgb, var(--surface-canvas) 72%, transparent);
  }
  .atlas-summary summary {
    padding: 9px 12px;
    cursor: pointer;
  }
  .atlas-summary ul {
    max-height: 220px;
    margin: 0;
    padding: 0 12px 10px 30px;
    overflow: auto;
  }
  .atlas-summary > strong {
    display: block;
    padding: 4px 12px;
    color: var(--muted);
    font-size: 0.68rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }
  .atlas-summary button {
    min-height: 32px;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  @media (prefers-reduced-motion: reduce) {
    .atlas-node,
    .route {
      transition: none;
    }
  }
</style>

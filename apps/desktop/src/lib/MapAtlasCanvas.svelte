<script lang="ts">
  import { onMount } from 'svelte';
  import { layoutAtlasNodes } from './map-atlas-layout';
  import type { ProjectMapAtlasNodeV1, ProjectMapAtlasSceneV1 } from './project-map-atlas';

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
  const layout = $derived(layoutAtlasNodes(scene.nodes, width, height));

  onMount(() => {
    width = Math.max(320, host.clientWidth || width);
    height = Math.max(280, host.clientHeight || height);
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      width = Math.max(320, entry.contentRect.width);
      height = Math.max(280, entry.contentRect.height);
    });
    observer.observe(host);
    return () => observer.disconnect();
  });

  function center(nodeId: string): { x: number; y: number } | null {
    const rect = layout.byId.get(nodeId);
    return rect === undefined ? null : { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  }

  function route(sourceId: string, targetId: string): string {
    const source = center(sourceId);
    const target = center(targetId);
    if (source === null || target === null) return '';
    const bend = Math.max(18, Math.abs(target.x - source.x) * 0.22);
    return `M ${source.x} ${source.y} C ${source.x + bend} ${source.y}, ${target.x - bend} ${target.y}, ${target.x} ${target.y}`;
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

<div class="canvas-host" bind:this={host} aria-label="Progressiver Architektur-Atlas">
  <svg class="route-layer" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
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
    {#each scene.relations as relation (`${relation.sourceNodeId}:${relation.targetNodeId}:${relation.relation}`)}
      <path
        class:uncertain={relation.uncertainty !== null}
        class={`route relation-${relation.relation}`}
        d={route(relation.sourceNodeId, relation.targetNodeId)}
        marker-end="url(#atlas-arrow)"
      ></path>
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
        data-kind={node.kind}
        data-status={node.mappingStatus ?? 'none'}
        style={`left:${rect.x}px;top:${rect.y}px;width:${rect.width}px;height:${rect.height}px`}
        aria-label={`${node.displayName}, ${kindLabel(node)}, ${node.fileCount} Dateien, ${node.symbolCount} Symbole`}
        onclick={() => onselect(node)}
        ondblclick={() => onopen(node)}
        onkeydown={(event) => keydown(event, node)}
      >
        <span class="node-kind">{kindLabel(node)}</span>
        <strong>{node.displayName}</strong>
        {#if rect.height > 68 && node.detail !== null}<small>{node.detail}</small>{/if}
        {#if rect.height > 88}
          <span class="node-counts">
            {#if node.kind === 'file'}{node.symbolCount} Struktursymbole
            {:else if node.kind === 'type' || node.kind === 'callable'}{node.memberCount} Member
            {:else}{node.fileCount} Dateien · {node.symbolCount} Symbole{/if}
          </span>
        {/if}
        {#if node.mappingStatus !== null}
          <span class="node-status">{node.mappingStatus}</span>
        {/if}
        {#if node.claimBadgeCount > 0}<span class="claim-badge">{node.claimBadgeCount} Claims</span
          >{/if}
        {#if node.currentRiskCount !== '0'}<span class="risk-badge"
            >{node.currentRiskCount} Risiken</span
          >{/if}
      </button>
    {/if}
  {/each}

  <details class="atlas-summary">
    <summary>Nichtgrafische Zusammenfassung</summary>
    <ul>
      {#each scene.nodes as node (node.nodeId)}
        <li>
          <button type="button" onclick={() => onselect(node)}
            >{node.displayName} · {kindLabel(node)}</button
          >
        </li>
      {/each}
    </ul>
  </details>
</div>

<style>
  .canvas-host {
    position: relative;
    min-width: 0;
    min-height: 280px;
    height: 100%;
    overflow: hidden;
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
    stroke-width: 1.4;
    opacity: 0.68;
    vector-effect: non-scaling-stroke;
  }
  .route.uncertain {
    stroke-dasharray: 7 5;
    opacity: 0.8;
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
  .atlas-node {
    position: absolute;
    z-index: 2;
    min-width: 44px;
    min-height: 44px;
    padding: 12px;
    border: 1px solid var(--line);
    border-left: 4px solid var(--accent);
    border-radius: 0;
    background: color-mix(in srgb, var(--surface-raised) 95%, transparent);
    color: var(--text);
    text-align: left;
    overflow: hidden;
    box-shadow: none;
    transition:
      background-color 120ms ease,
      border-color 120ms ease;
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
    background: color-mix(in srgb, var(--surface-raised) 86%, transparent);
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
    font-size: clamp(0.77rem, 1.2vw, 0.98rem);
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
    z-index: 5;
    left: 10px;
    bottom: 10px;
    max-width: min(360px, 70%);
    border: 1px solid var(--line);
    background: var(--surface);
    font-size: 0.75rem;
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
  .atlas-summary button {
    min-height: 32px;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  @media (prefers-reduced-motion: reduce) {
    .atlas-node {
      transition: none;
    }
  }
</style>

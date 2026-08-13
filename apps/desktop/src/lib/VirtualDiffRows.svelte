<script lang="ts">
  import type { AgentDiffRowV1 } from './agent-inspection';

  interface Props {
    layout: 'sideBySide' | 'unified';
    overscan?: number;
    rowHeight?: number;
    rows: AgentDiffRowV1[];
    viewportRows?: number;
  }

  let { layout, overscan = 4, rowHeight = 28, rows, viewportRows = 12 }: Props = $props();
  let scrollTop = $state(0);
  let startIndex = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - Math.max(0, overscan)));
  let endIndex = $derived(
    Math.min(rows.length, startIndex + Math.max(1, viewportRows) + Math.max(0, overscan) * 2),
  );
  let visibleRows = $derived(
    rows.slice(startIndex, endIndex).map((row, offset) => ({ index: startIndex + offset, row })),
  );

  function updateScroll(event: Event): void {
    const target = event.currentTarget;
    if (target instanceof HTMLElement) scrollTop = target.scrollTop;
  }

  function linePrefix(row: AgentDiffRowV1): string {
    return row.kind === 'added' ? '+' : row.kind === 'removed' ? '−' : ' ';
  }
</script>

<p class="virtual-range" role="status">
  Zeilen {rows.length === 0 ? 0 : startIndex + 1}–{endIndex} von {rows.length}; nur der sichtbare
  Ausschnitt wird gerendert.
</p>
<!-- svelte-ignore a11y_no_noninteractive_tabindex (the overflow viewport must be keyboard-scrollable) -->
<div
  class="virtual-diff"
  role="table"
  aria-label={`${layout === 'unified' ? 'Unified' : 'Side-by-side'} Diff`}
  aria-rowcount={rows.length}
  tabindex="0"
  style:height={`${Math.max(1, Math.min(rows.length, viewportRows)) * rowHeight}px`}
  onscroll={updateScroll}
>
  <div class="virtual-canvas" role="rowgroup" style:height={`${rows.length * rowHeight}px`}>
    {#each visibleRows as item (item.index)}
      {@const row = item.row}
      {#if layout === 'unified'}
        <div
          class:added={row.kind === 'added'}
          class:removed={row.kind === 'removed'}
          class="diff-row"
          role="row"
          aria-rowindex={item.index + 1}
          style:height={`${rowHeight}px`}
          style:top={`${item.index * rowHeight}px`}
        >
          <span class="line-number" role="cell">{row.kind === 'added' ? '' : row.beforeLine}</span>
          <span class="line-number" role="cell">{row.kind === 'removed' ? '' : row.afterLine}</span>
          <span class="prefix" role="cell">{linePrefix(row)}</span>
          <span class="source-cell" role="cell"><code>{row.line.text}</code></span>
        </div>
      {:else}
        <div
          class="side-row"
          role="row"
          aria-rowindex={item.index + 1}
          style:height={`${rowHeight}px`}
          style:top={`${item.index * rowHeight}px`}
        >
          <div class:removed={row.kind === 'removed'} class="side-cell" role="cell">
            <span class="line-number">{row.kind === 'added' ? '' : row.beforeLine}</span>
            <code>{row.kind === 'added' ? '' : row.line.text}</code>
          </div>
          <div class:added={row.kind === 'added'} class="side-cell" role="cell">
            <span class="line-number">{row.kind === 'removed' ? '' : row.afterLine}</span>
            <code>{row.kind === 'removed' ? '' : row.line.text}</code>
          </div>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .virtual-range {
    margin: 0;
    padding: 0.35rem 0.55rem;
    color: var(--color-muted);
    font-size: 0.75rem;
  }

  .virtual-diff {
    max-width: 100%;
    overflow: auto;
    contain: strict;
    background: var(--color-surface-subtle);
  }

  .virtual-canvas {
    position: relative;
    min-width: max-content;
  }

  .diff-row,
  .side-row {
    position: absolute;
    right: 0;
    left: 0;
    box-sizing: border-box;
  }

  .diff-row {
    display: grid;
    grid-template-columns: 3rem 3rem 1.2rem minmax(20rem, 1fr);
    min-width: max-content;
  }

  .diff-row > *,
  .side-cell > * {
    min-width: 0;
    overflow: hidden;
    padding: 0.18rem 0.35rem;
    text-overflow: ellipsis;
    white-space: pre;
  }

  .line-number {
    color: var(--color-subtle);
    text-align: right;
    user-select: none;
  }

  .diff-row.added,
  .side-cell.added {
    background: var(--color-positive-surface);
  }

  .diff-row.removed,
  .side-cell.removed {
    background: var(--color-danger-surface);
  }

  .side-row {
    display: grid;
    grid-template-columns: minmax(18rem, 1fr) minmax(18rem, 1fr);
    min-width: max-content;
  }

  .side-cell {
    display: grid;
    grid-template-columns: 3rem minmax(15rem, 1fr);
    border-right: 1px solid var(--color-border-soft);
  }

  code {
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  }
</style>

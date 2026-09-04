<script lang="ts">
  import type { AgentWorkTraceSourceV2 } from './agent-ask-research';
  import { sourceCitationAccessibleName, splitSourceCitations } from './source-citations';

  interface Props {
    onsource?: (source: AgentWorkTraceSourceV2) => void;
    sources?: AgentWorkTraceSourceV2[];
    text: string;
  }

  let { onsource = () => {}, sources = [], text }: Props = $props();
  const segments = $derived(splitSourceCitations(text, sources));
</script>

{#each segments as segment, index (index)}{#if segment.kind === 'text'}{segment.text}{:else}<button
      class="source-citation"
      type="button"
      title={sourceCitationAccessibleName(segment.source)}
      aria-label={sourceCitationAccessibleName(segment.source)}
      onclick={() => onsource(segment.source)}>{segment.text}</button
    >{/if}{/each}

<style>
  .source-citation {
    display: inline;
    padding: 0 0.08rem;
    border: 0;
    border-bottom: 1px dotted color-mix(in srgb, var(--color-muted) 55%, transparent);
    border-radius: 0.1rem;
    color: var(--color-muted);
    background: transparent;
    font: inherit;
    font-size: 0.76em;
    font-weight: 500;
    line-height: 1;
    vertical-align: 0.08em;
    cursor: pointer;
    opacity: 0.82;
  }
  .source-citation:hover,
  .source-citation:focus-visible {
    color: var(--color-accent);
    opacity: 1;
    text-decoration: underline;
  }
</style>

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
    padding: 0.05rem 0.18rem;
    border: 0;
    border-radius: 0.2rem;
    color: var(--color-accent);
    background: var(--color-accent-surface);
    font: inherit;
    font-size: 0.92em;
    font-weight: 600;
    cursor: pointer;
  }
  .source-citation:hover,
  .source-citation:focus-visible {
    text-decoration: underline;
  }
</style>

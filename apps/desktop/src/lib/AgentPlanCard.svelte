<script lang="ts">
  import AgentDetailDialog from './AgentDetailDialog.svelte';
  import ChatMarkdown from './ChatMarkdown.svelte';
  import type { AgentWorkTraceSourceV2 } from './agent-ask-research';

  let {
    text,
    sources,
    onsource,
  }: {
    text: string;
    sources: AgentWorkTraceSourceV2[];
    onsource: (source: AgentWorkTraceSourceV2) => void;
  } = $props();
  let open = $state(false);
</script>

<div class="plan-preview">
  <div>
    <strong>Vorgehensplan</strong>
    <p>Schritte und Prüfkriterien ansehen.</p>
  </div>
  <button type="button" aria-haspopup="dialog" onclick={() => (open = true)}>Plan öffnen</button>
</div>
{#if open}
  <AgentDetailDialog title="Vorgehensplan" onclose={() => (open = false)}>
    <ChatMarkdown
      {text}
      {sources}
      onsource={(source) => {
        open = false;
        onsource(source);
      }}
    />
  </AgentDetailDialog>
{/if}

<style>
  .plan-preview {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  strong {
    color: var(--color-heading);
  }
  p {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  button {
    flex: 0 0 auto;
    min-height: var(--control-min-size);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }
  button:hover {
    border-color: var(--color-accent);
    background: var(--color-accent-surface);
  }
</style>

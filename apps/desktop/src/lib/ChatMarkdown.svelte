<script lang="ts">
  import SourceLinkedText from './SourceLinkedText.svelte';
  import { parseChatMarkdown } from './chat-markdown';
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
  // Sources and live polling can change independently of the published text.
  const content = $derived(text);
  const blocks = $derived(parseChatMarkdown(content));
</script>

{#each blocks as block, blockIndex (blockIndex)}
  {#if block.kind === 'heading'}
    <div class="markdown-heading" data-level={block.level}>
      <SourceLinkedText text={block.text} {sources} {onsource} />
    </div>
  {:else if block.kind === 'paragraph'}
    <p>
      <SourceLinkedText text={block.text} {sources} {onsource} />
    </p>
  {:else if block.kind === 'list'}
    {#if block.ordered}
      <ol>
        {#each block.items as item, itemIndex (itemIndex)}<li>
            <SourceLinkedText text={item} {sources} {onsource} />
          </li>{/each}
      </ol>
    {:else}
      <ul>
        {#each block.items as item, itemIndex (itemIndex)}<li>
            <SourceLinkedText text={item} {sources} {onsource} />
          </li>{/each}
      </ul>
    {/if}
  {:else if block.kind === 'quote'}
    <blockquote>
      <SourceLinkedText text={block.text} {sources} {onsource} />
    </blockquote>
  {:else}
    <pre><code data-language={block.language}>{block.text}</code></pre>
  {/if}
{/each}

<style>
  p {
    margin: 0 0 var(--space-3);
    white-space: pre-wrap;
  }
  p:last-child,
  :is(ul, ol, pre, blockquote):last-child {
    margin-bottom: 0;
  }
  :is(ul, ol) {
    margin: 0 0 var(--space-3);
    padding-inline-start: var(--space-5);
  }
  li + li {
    margin-top: var(--space-1);
  }
  .markdown-heading {
    margin: var(--space-4) 0 var(--space-2);
    color: var(--color-heading);
    font-weight: 750;
  }
  .markdown-heading:first-child {
    margin-top: 0;
  }
  .markdown-heading[data-level='1'],
  .markdown-heading[data-level='2'] {
    font-size: 1.05rem;
  }
  blockquote {
    margin: 0 0 var(--space-3);
    padding-inline-start: var(--space-3);
    border-inline-start: 2px solid var(--color-border-strong);
    color: var(--color-muted);
  }
  pre {
    max-width: 100%;
    margin: 0 0 var(--space-3);
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    white-space: pre;
  }
</style>

<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, children, onclose }: { title: string; children: Snippet; onclose: () => void } =
    $props();
  const id = $props.id();

  function present(node: HTMLDialogElement): { destroy: () => void } {
    const trigger = document.activeElement;
    if (typeof node.showModal === 'function') node.showModal();
    else node.setAttribute('open', '');
    node.querySelector<HTMLButtonElement>('button')?.focus({ preventScroll: true });
    return {
      destroy: () => {
        node.close?.();
        if (trigger instanceof HTMLElement && trigger.isConnected)
          trigger.focus({ preventScroll: true });
      },
    };
  }
</script>

<dialog
  class="agent-detail-dialog"
  aria-labelledby={`${id}-title`}
  use:present
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <header>
    <h2 id={`${id}-title`}>{title}</h2>
    <button type="button" aria-label="Dialog schließen" onclick={onclose}>×</button>
  </header>
  <!-- The bounded reading pane must be keyboard-scrollable even without links. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="dialog-body" tabindex="0" role="region" aria-label={title}>
    {@render children()}
  </div>
</dialog>

<style>
  .agent-detail-dialog {
    width: min(52rem, calc(100vw - 2rem));
    max-width: none;
    max-height: calc(100dvh - 2rem);
    padding: 0;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-card);
    color: var(--color-text);
    background: var(--color-surface);
    overflow: hidden;
    animation: app-surface-in 120ms var(--ease-out);
  }
  .agent-detail-dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .agent-detail-dialog::backdrop {
    background: color-mix(in srgb, var(--color-overlay) 55%, transparent);
  }
  header {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-border-soft);
  }
  h2 {
    margin: 0;
    font-size: var(--font-size-base);
  }
  button {
    min-width: var(--control-min-size);
    min-height: var(--control-min-size);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-muted);
    font-size: 1.5rem;
    cursor: pointer;
  }
  button:hover {
    color: var(--color-text);
    background: var(--color-surface-muted);
  }
  .dialog-body {
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    padding: var(--space-4) var(--space-5);
    overflow-wrap: anywhere;
  }
  @media (prefers-reduced-motion: reduce) {
    .agent-detail-dialog {
      animation: none;
    }
  }
</style>

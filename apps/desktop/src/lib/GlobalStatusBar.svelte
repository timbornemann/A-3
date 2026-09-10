<script lang="ts">
  import type { GlobalStatusItem } from './global-status';

  interface Props {
    index: GlobalStatusItem;
    model: GlobalStatusItem;
    onIndexClick: () => void;
    project: GlobalStatusItem;
    run: GlobalStatusItem;
  }

  let { index, model, onIndexClick, project, run }: Props = $props();
</script>

<section class="global-status" aria-label="Globaler Arbeitsstatus" aria-live="polite">
  <dl>
    <div
      class:failed={project.tone === 'failed'}
      class:pending={project.tone === 'pending'}
      class:ready={project.tone === 'ready'}
      class:warning={project.tone === 'warning'}
    >
      <dt>Projekt</dt>
      <dd title={project.value}>{project.value}</dd>
    </div>
    <div
      class:failed={index.tone === 'failed'}
      class:pending={index.tone === 'pending'}
      class:ready={index.tone === 'ready'}
      class:warning={index.tone === 'warning'}
    >
      <dt>Index</dt>
      <dd>
        <button
          type="button"
          class="index-inspector-trigger"
          title={`${index.value} – Details zum Indexlauf öffnen`}
          aria-haspopup="dialog"
          onclick={onIndexClick}>{index.value}</button
        >
      </dd>
    </div>
    <div
      class:failed={model.tone === 'failed'}
      class:pending={model.tone === 'pending'}
      class:ready={model.tone === 'ready'}
      class:warning={model.tone === 'warning'}
    >
      <dt>Modell</dt>
      <dd title={model.value}>{model.value}</dd>
    </div>
    <div
      class:failed={run.tone === 'failed'}
      class:pending={run.tone === 'pending'}
      class:ready={run.tone === 'ready'}
      class:warning={run.tone === 'warning'}
    >
      <dt>Agent</dt>
      <dd title={run.value}>{run.value}</dd>
    </div>
  </dl>
</section>

<style>
  .index-inspector-trigger {
    appearance: none;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font: inherit;
    padding: 0.08rem 0.2rem;
    text-align: left;
  }

  .index-inspector-trigger:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  .index-inspector-trigger:focus-visible {
    outline: 2px solid currentColor;
    outline-offset: 2px;
  }
</style>

<script lang="ts">
  import type { GlobalStatusItem } from './global-status';

  interface Props {
    index: GlobalStatusItem;
    model: GlobalStatusItem;
    project: GlobalStatusItem;
    run: GlobalStatusItem;
  }

  let { index, model, project, run }: Props = $props();
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
      <dd>{project.value}</dd>
    </div>
    <div
      class:failed={index.tone === 'failed'}
      class:pending={index.tone === 'pending'}
      class:ready={index.tone === 'ready'}
      class:warning={index.tone === 'warning'}
    >
      <dt>Index</dt>
      <dd>{index.value}</dd>
    </div>
    <div
      class:failed={model.tone === 'failed'}
      class:pending={model.tone === 'pending'}
      class:ready={model.tone === 'ready'}
      class:warning={model.tone === 'warning'}
    >
      <dt>Modell</dt>
      <dd>{model.value}</dd>
    </div>
    <div
      class:failed={run.tone === 'failed'}
      class:pending={run.tone === 'pending'}
      class:ready={run.tone === 'ready'}
      class:warning={run.tone === 'warning'}
    >
      <dt>Run</dt>
      <dd>{run.value}</dd>
    </div>
  </dl>
</section>

<script lang="ts">
  import { onMount } from 'svelte';
  import { queryHealth, type HealthResponseV1 } from './lib/health';

  interface Props {
    healthLoader?: () => Promise<HealthResponseV1>;
  }

  type ViewState =
    { kind: 'loading' } | { health: HealthResponseV1; kind: 'ready' } | { kind: 'error' };

  let { healthLoader = queryHealth }: Props = $props();
  let state = $state<ViewState>({ kind: 'loading' });

  async function loadHealth(): Promise<void> {
    state = { kind: 'loading' };

    try {
      state = { health: await healthLoader(), kind: 'ready' };
    } catch {
      state = { kind: 'error' };
    }
  }

  onMount(() => {
    void loadHealth();
  });
</script>

<svelte:head>
  <title>A^3</title>
</svelte:head>

<main class="app-shell">
  <header class="product-header">
    <p class="eyebrow">Local-first coding agent</p>
    <h1>A^3</h1>
    <p class="subtitle">Autonomous Agent Assistant</p>
  </header>

  <section class="health-card" aria-labelledby="health-heading">
    <div class="section-heading">
      <div>
        <p class="section-kicker">Systemstatus</p>
        <h2 id="health-heading">Desktop Core</h2>
      </div>
      <span
        class:pending={state.kind === 'loading'}
        class:failed={state.kind === 'error'}
        class="status-dot"
        aria-hidden="true"
      ></span>
    </div>

    {#if state.kind === 'loading'}
      <p class="status-message" role="status" aria-live="polite">Core wird geprüft …</p>
    {:else if state.kind === 'ready'}
      <p class="ready-label" role="status" aria-live="polite">Bereit</p>
      <dl class="health-grid">
        <div>
          <dt>App-Version</dt>
          <dd>{state.health.applicationVersion}</dd>
        </div>
        <div>
          <dt>Protokoll</dt>
          <dd>V{state.health.protocolVersion}</dd>
        </div>
        <div>
          <dt>Plattform</dt>
          <dd>{state.health.platform}</dd>
        </div>
      </dl>
    {:else}
      <div class="error-state" role="alert">
        <p>Die Health-Abfrage ist fehlgeschlagen.</p>
        <button type="button" onclick={loadHealth}>Erneut prüfen</button>
      </div>
    {/if}
  </section>

  <footer>
    <span>Offline by default</span>
    <span aria-hidden="true">·</span>
    <span>Typed IPC</span>
    <span aria-hidden="true">·</span>
    <span>Local core</span>
  </footer>
</main>

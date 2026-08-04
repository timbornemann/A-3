<script lang="ts">
  import { onMount } from 'svelte';
  import { queryHealth, type HealthResponseV1 } from './lib/health';
  import { openProject, type OpenProjectResponseV1, type ProjectSummaryV1 } from './lib/project';

  interface Props {
    healthLoader?: () => Promise<HealthResponseV1>;
    projectOpener?: () => Promise<OpenProjectResponseV1>;
  }

  type ViewState =
    { kind: 'loading' } | { health: HealthResponseV1; kind: 'ready' } | { kind: 'error' };
  type ProjectView =
    | { kind: 'idle' }
    | { kind: 'opening' }
    | { kind: 'cancelled' }
    | { kind: 'opened'; project: ProjectSummaryV1 }
    | { kind: 'error' };

  let { healthLoader = queryHealth, projectOpener = openProject }: Props = $props();
  let healthView = $state<ViewState>({ kind: 'loading' });
  let projectView = $state<ProjectView>({ kind: 'idle' });

  async function loadHealth(): Promise<void> {
    healthView = { kind: 'loading' };

    try {
      healthView = { health: await healthLoader(), kind: 'ready' };
    } catch {
      healthView = { kind: 'error' };
    }
  }

  onMount(() => {
    void loadHealth();
  });

  async function chooseProject(): Promise<void> {
    projectView = { kind: 'opening' };
    try {
      const response = await projectOpener();
      projectView =
        response.result.status === 'opened'
          ? { kind: 'opened', project: response.result.project }
          : { kind: 'cancelled' };
    } catch {
      projectView = { kind: 'error' };
    }
  }
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
        class:pending={healthView.kind === 'loading'}
        class:failed={healthView.kind === 'error'}
        class="status-dot"
        aria-hidden="true"
      ></span>
    </div>

    {#if healthView.kind === 'loading'}
      <p class="status-message" role="status" aria-live="polite">Core wird geprüft …</p>
    {:else if healthView.kind === 'ready'}
      <p class="ready-label" role="status" aria-live="polite">Bereit</p>
      <dl class="health-grid">
        <div>
          <dt>App-Version</dt>
          <dd>{healthView.health.applicationVersion}</dd>
        </div>
        <div>
          <dt>Protokoll</dt>
          <dd>V{healthView.health.protocolVersion}</dd>
        </div>
        <div>
          <dt>Plattform</dt>
          <dd>{healthView.health.platform}</dd>
        </div>
      </dl>
    {:else}
      <div class="error-state" role="alert">
        <p>Die Health-Abfrage ist fehlgeschlagen.</p>
        <button type="button" onclick={loadHealth}>Erneut prüfen</button>
      </div>
    {/if}
  </section>

  <section class="project-card" aria-labelledby="project-heading">
    <div class="section-heading">
      <div>
        <p class="section-kicker">Lokaler Workspace</p>
        <h2 id="project-heading">Projekt öffnen</h2>
      </div>
    </div>

    <p class="project-copy">
      Wähle den Root eines Git-Worktrees. A^3 erhält nur Zugriff auf diesen ausdrücklich gewählten
      Ordner.
    </p>
    <button
      class="primary-action"
      type="button"
      disabled={projectView.kind === 'opening'}
      onclick={chooseProject}
    >
      {projectView.kind === 'opening'
        ? 'Ordnerdialog geöffnet …'
        : projectView.kind === 'opened'
          ? 'Anderen Worktree auswählen'
          : 'Projektordner auswählen'}
    </button>

    {#if projectView.kind === 'cancelled'}
      <p class="project-status" role="status" aria-live="polite">Auswahl abgebrochen.</p>
    {:else if projectView.kind === 'opened'}
      <div class="project-result" role="status" aria-live="polite">
        <p class="ready-label">Worktree sicher geöffnet</p>
        <dl class="project-grid">
          <div>
            <dt>Root</dt>
            <dd>{projectView.project.worktreeRootDisplay}</dd>
          </div>
          <div>
            <dt>Worktree-ID</dt>
            <dd>{projectView.project.worktreeId}</dd>
          </div>
        </dl>
      </div>
    {:else if projectView.kind === 'error'}
      <p class="project-error" role="alert">
        Der gewählte Ordner konnte nicht als sicherer Git-Worktree geöffnet werden.
      </p>
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

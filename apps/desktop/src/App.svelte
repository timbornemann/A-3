<script lang="ts">
  import { onMount } from 'svelte';
  import { queryHealth, type HealthResponseV1 } from './lib/health';
  import { openProject, type GitHeadV1, type OpenProjectResponseV1 } from './lib/project';
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
  import {
    queryProjectStatus,
    type IndexStateV1,
    type ProjectStatusResponseV1,
    type RebuildStateV1,
  } from './lib/project-status';
  import {
    listRecentProjects,
    type RecentProjectSummaryV1,
    type RecentProjectsResponseV1,
  } from './lib/recent-projects';

  interface Props {
    healthLoader?: () => Promise<HealthResponseV1>;
    projectOpener?: () => Promise<OpenProjectResponseV1>;
    projectRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    projectStatusLoader?: () => Promise<ProjectStatusResponseV1>;
    recentProjectsLoader?: () => Promise<RecentProjectsResponseV1>;
  }

  type ViewState =
    { kind: 'loading' } | { health: HealthResponseV1; kind: 'ready' } | { kind: 'error' };
  type ProjectView =
    | { kind: 'idle' }
    | { kind: 'opening' }
    | { kind: 'cancelled' }
    | { kind: 'opened' }
    | { kind: 'error' };
  type ProjectStatusView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'active'; result: Extract<ProjectStatusResponseV1['result'], { status: 'active' }> }
    | { kind: 'error' };
  type RebuildView = { kind: 'idle' } | { kind: 'submitting' } | { kind: 'error' };
  type RecentProjectsView =
    { kind: 'loading' } | { kind: 'ready'; projects: RecentProjectSummaryV1[] } | { kind: 'error' };

  let {
    healthLoader = queryHealth,
    projectOpener = openProject,
    projectRebuilder = rebuildProjectIndex,
    projectStatusLoader = queryProjectStatus,
    recentProjectsLoader = listRecentProjects,
  }: Props = $props();
  let healthView = $state<ViewState>({ kind: 'loading' });
  let projectView = $state<ProjectView>({ kind: 'idle' });
  let projectStatusView = $state<ProjectStatusView>({ kind: 'loading' });
  let rebuildView = $state<RebuildView>({ kind: 'idle' });
  let recentProjectsView = $state<RecentProjectsView>({ kind: 'loading' });

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
    void loadProjectStatus();
    void loadRecentProjects();
  });

  async function loadProjectStatus(): Promise<void> {
    projectStatusView = { kind: 'loading' };
    try {
      const response = await projectStatusLoader();
      projectStatusView =
        response.result.status === 'active'
          ? { kind: 'active', result: response.result }
          : { kind: 'noProject' };
    } catch {
      projectStatusView = { kind: 'error' };
    }
  }

  async function loadRecentProjects(): Promise<void> {
    recentProjectsView = { kind: 'loading' };
    try {
      const response = await recentProjectsLoader();
      recentProjectsView = { kind: 'ready', projects: response.projects };
    } catch {
      recentProjectsView = { kind: 'error' };
    }
  }

  async function chooseProject(): Promise<void> {
    projectView = { kind: 'opening' };
    try {
      const response = await projectOpener();
      if (response.result.status === 'opened') {
        projectView = { kind: 'opened' };
        await loadProjectStatus();
        await loadRecentProjects();
      } else {
        projectView = { kind: 'cancelled' };
      }
    } catch {
      projectView = { kind: 'error' };
    }
  }

  async function requestIndexRebuild(): Promise<void> {
    rebuildView = { kind: 'submitting' };
    try {
      await projectRebuilder();
      rebuildView = { kind: 'idle' };
      await loadProjectStatus();
    } catch {
      rebuildView = { kind: 'error' };
    }
  }

  function branchLabel(head: GitHeadV1): string {
    if (head.kind === 'born') {
      return head.reference === null
        ? 'Detached HEAD'
        : head.reference.replace(/^refs\/heads\//, '');
    }
    return `${head.reference.replace(/^refs\/heads\//, '')} (unborn)`;
  }

  function indexStateLabel(state: IndexStateV1): string {
    const labels: Record<IndexStateV1, string> = {
      notStarted: 'Noch nicht gestartet',
      building: 'Index wird aufgebaut',
      published: 'Veröffentlicht',
      failed: 'Letzter Lauf fehlgeschlagen',
      cancelled: 'Letzter Lauf abgebrochen',
    };
    return labels[state];
  }

  function storageSizeLabel(bytes: string | null): string {
    return bytes === null
      ? 'Nicht verfügbar'
      : `${new Intl.NumberFormat('de-DE').format(BigInt(bytes))} Bytes`;
  }

  function rebuildStateLabel(state: RebuildStateV1): string {
    const labels = {
      idle: 'Bereit',
      queued: 'Rebuild wartet',
      running: 'Regenerierbare Daten werden entfernt',
      succeeded: 'Rebuild abgeschlossen; Neuindexierung angefordert',
      failed: 'Rebuild fehlgeschlagen',
      cancelled: 'Rebuild abgebrochen',
    } as const;
    return labels[state];
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
      <p class="ready-label" role="status" aria-live="polite">Worktree sicher geöffnet</p>
    {:else if projectView.kind === 'error'}
      <p class="project-error" role="alert">
        Der gewählte Ordner konnte nicht als sicherer Git-Worktree geöffnet werden.
      </p>
    {/if}

    {#if projectStatusView.kind === 'loading'}
      <p class="project-status" role="status" aria-live="polite">Projektstatus wird geladen …</p>
    {:else if projectStatusView.kind === 'active'}
      <div class="project-result" aria-labelledby="active-project-heading">
        <h3 id="active-project-heading">Aktiver Worktree</h3>
        <dl class="project-grid">
          <div>
            <dt>Root</dt>
            <dd>{projectStatusView.result.project.worktreeRootDisplay}</dd>
          </div>
          <div>
            <dt>Branch</dt>
            <dd>{branchLabel(projectStatusView.result.project.head)}</dd>
          </div>
          <div>
            <dt>Worktree-ID</dt>
            <dd>{projectStatusView.result.project.worktreeId}</dd>
          </div>
          <div>
            <dt>Indexstatus</dt>
            <dd>{indexStateLabel(projectStatusView.result.index.state)}</dd>
          </div>
          <div>
            <dt>A^3-Speicher</dt>
            <dd>{storageSizeLabel(projectStatusView.result.storageBytes)}</dd>
          </div>
          <div>
            <dt>Letzter Snapshot</dt>
            {#if projectStatusView.result.index.latestSnapshot === null}
              <dd>Noch kein Snapshot</dd>
            {:else}
              <dd>
                Generation {projectStatusView.result.index.latestSnapshot.generation}<br />
                {projectStatusView.result.index.latestSnapshot.snapshotId}
              </dd>
            {/if}
          </div>
        </dl>
        <div class="project-maintenance" aria-labelledby="rebuild-heading">
          <h4 id="rebuild-heading">Index neu aufbauen</h4>
          <p>
            Entfernt ausschließlich regenerierbare Indexprojektionen. Quellcode, Snapshots,
            Aufgaben, Entscheidungen und User-Evidence bleiben erhalten.
          </p>
          <p class="project-status" role="status" aria-live="polite">
            {rebuildStateLabel(projectStatusView.result.rebuildState)}
          </p>
          <div class="project-actions">
            <button
              type="button"
              disabled={rebuildView.kind === 'submitting' ||
                projectStatusView.result.rebuildState === 'queued' ||
                projectStatusView.result.rebuildState === 'running'}
              onclick={requestIndexRebuild}
            >
              {rebuildView.kind === 'submitting'
                ? 'Rebuild wird angefordert …'
                : 'Regenerierbaren Index neu aufbauen'}
            </button>
            <button type="button" onclick={loadProjectStatus}>Status aktualisieren</button>
          </div>
          {#if rebuildView.kind === 'error'}
            <p class="project-error" role="alert">
              Der Rebuild konnte nicht sicher angefordert werden. Der bestehende Index bleibt
              erhalten.
            </p>
          {/if}
        </div>
      </div>
    {:else if projectStatusView.kind === 'error'}
      <div class="recent-projects-error" role="alert">
        <p>Der aktive Projektstatus konnte nicht sicher geladen werden.</p>
        <button type="button" onclick={loadProjectStatus}>Status erneut laden</button>
      </div>
    {/if}

    <div class="recent-projects" aria-labelledby="recent-projects-heading">
      <h3 id="recent-projects-heading">Zuletzt verwendet</h3>
      {#if recentProjectsView.kind === 'loading'}
        <p class="project-status" role="status" aria-live="polite">Projektliste wird geladen …</p>
      {:else if recentProjectsView.kind === 'error'}
        <div class="recent-projects-error" role="alert">
          <p>Die lokale Projektliste konnte nicht geladen werden.</p>
          <button type="button" onclick={loadRecentProjects}>Erneut laden</button>
        </div>
      {:else if recentProjectsView.projects.length === 0}
        <p class="project-status">Noch keine Projekte gespeichert.</p>
      {:else}
        <ol class="recent-project-list">
          {#each recentProjectsView.projects as recent (recent.project.worktreeId)}
            <li>
              <span>{recent.project.worktreeRootDisplay}</span>
              <span>{branchLabel(recent.project.head)}</span>
              <code>{recent.project.worktreeId}</code>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  </section>

  <footer>
    <span>Offline by default</span>
    <span aria-hidden="true">·</span>
    <span>Typed IPC</span>
    <span aria-hidden="true">·</span>
    <span>Local core</span>
  </footer>
</main>

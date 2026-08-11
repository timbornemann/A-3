<script lang="ts">
  import { onMount } from 'svelte';
  import { projectActionRecoveryMessage, projectOpenRecoveryMessage } from './lib/command-error';
  import { queryHealth, type HealthResponseV1 } from './lib/health';
  import {
    queryIndexActivity,
    type IndexActivityResponseV1,
    type IndexActivityStateV1,
    type IndexPhaseV1,
  } from './lib/index-activity';
  import {
    queryIndexOverview,
    type IndexDiagnosticCodeV1,
    type IndexDiagnosticSeverityV1,
    type IndexLanguageV1,
    type IndexOverviewResponseV1,
  } from './lib/index-overview';
  import { openProject, type GitHeadV1, type OpenProjectResponseV1 } from './lib/project';
  import { rebuildProjectIndex, type RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
  import { removeProject, type RemoveProjectResponseV1 } from './lib/project-removal';
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
    indexActivityLoader?: () => Promise<IndexActivityResponseV1>;
    indexOverviewLoader?: () => Promise<IndexOverviewResponseV1>;
    projectOpener?: () => Promise<OpenProjectResponseV1>;
    projectRebuilder?: () => Promise<RebuildProjectIndexResponseV1>;
    projectRemover?: () => Promise<RemoveProjectResponseV1>;
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
    | { kind: 'error'; message: string };
  type ProjectStatusView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'active'; result: Extract<ProjectStatusResponseV1['result'], { status: 'active' }> }
    | { kind: 'error' };
  type IndexActivityView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'active'; result: Extract<IndexActivityResponseV1['result'], { status: 'active' }> }
    | { kind: 'error' };
  type IndexOverviewView =
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'noPublishedIndex' }
    | {
        kind: 'published';
        result: Extract<IndexOverviewResponseV1['result'], { status: 'published' }>;
      }
    | { kind: 'error' };
  type RebuildView = { kind: 'idle' } | { kind: 'submitting' } | { kind: 'error'; message: string };
  type RemovalView =
    | { kind: 'idle' }
    | { kind: 'confirming' }
    | { kind: 'submitting' }
    | { kind: 'removed' }
    | { kind: 'error'; message: string };
  type RecentProjectsView =
    { kind: 'loading' } | { kind: 'ready'; projects: RecentProjectSummaryV1[] } | { kind: 'error' };

  let {
    healthLoader = queryHealth,
    indexActivityLoader = queryIndexActivity,
    indexOverviewLoader = queryIndexOverview,
    projectOpener = openProject,
    projectRebuilder = rebuildProjectIndex,
    projectRemover = removeProject,
    projectStatusLoader = queryProjectStatus,
    recentProjectsLoader = listRecentProjects,
  }: Props = $props();
  let healthView = $state<ViewState>({ kind: 'loading' });
  let projectView = $state<ProjectView>({ kind: 'idle' });
  let projectStatusView = $state<ProjectStatusView>({ kind: 'loading' });
  let indexActivityView = $state<IndexActivityView>({ kind: 'loading' });
  let indexOverviewView = $state<IndexOverviewView>({ kind: 'loading' });
  let rebuildView = $state<RebuildView>({ kind: 'idle' });
  let removalView = $state<RemovalView>({ kind: 'idle' });
  let recentProjectsView = $state<RecentProjectsView>({ kind: 'loading' });
  let indexActivityObserved = false;

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
    void loadIndexActivity();
    void loadIndexOverview();
    const activityTimer = window.setInterval(() => {
      void loadIndexActivity();
    }, 500);
    return () => window.clearInterval(activityTimer);
  });

  async function loadIndexActivity(): Promise<void> {
    try {
      const previousSucceeded =
        indexActivityView.kind === 'active' &&
        indexActivityView.result.activity.state === 'succeeded';
      const response = await indexActivityLoader();
      indexActivityView =
        response.result.status === 'active'
          ? { kind: 'active', result: response.result }
          : { kind: 'noProject' };
      if (
        indexActivityObserved &&
        response.result.status === 'active' &&
        response.result.activity.state === 'succeeded' &&
        !previousSucceeded
      ) {
        void loadIndexOverview();
      } else if (response.result.status === 'noProject') {
        indexOverviewView = { kind: 'noProject' };
      }
      indexActivityObserved = true;
    } catch {
      indexActivityView = { kind: 'error' };
    }
  }

  async function loadIndexOverview(): Promise<void> {
    indexOverviewView = { kind: 'loading' };
    try {
      const response = await indexOverviewLoader();
      if (response.result.status === 'published') {
        indexOverviewView = { kind: 'published', result: response.result };
      } else if (response.result.status === 'noPublishedIndex') {
        indexOverviewView = { kind: 'noPublishedIndex' };
      } else {
        indexOverviewView = { kind: 'noProject' };
      }
    } catch {
      indexOverviewView = { kind: 'error' };
    }
  }

  async function loadProjectStatus(): Promise<void> {
    projectStatusView = { kind: 'loading' };
    try {
      const response = await projectStatusLoader();
      projectStatusView =
        response.result.status === 'active'
          ? { kind: 'active', result: response.result }
          : { kind: 'noProject' };
      if (response.result.status === 'noProject') {
        indexOverviewView = { kind: 'noProject' };
      }
    } catch {
      projectStatusView = { kind: 'error' };
    }
  }

  async function refreshProjectDetails(): Promise<void> {
    await Promise.all([loadProjectStatus(), loadIndexOverview()]);
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
        removalView = { kind: 'idle' };
        indexActivityObserved = false;
        await loadProjectStatus();
        await loadIndexActivity();
        await loadIndexOverview();
        await loadRecentProjects();
      } else {
        projectView = { kind: 'cancelled' };
      }
    } catch (error) {
      projectView = { kind: 'error', message: projectOpenRecoveryMessage(error) };
    }
  }

  async function requestIndexRebuild(): Promise<void> {
    rebuildView = { kind: 'submitting' };
    try {
      await projectRebuilder();
      rebuildView = { kind: 'idle' };
      await loadProjectStatus();
    } catch (error) {
      rebuildView = {
        kind: 'error',
        message: projectActionRecoveryMessage(error, 'rebuild'),
      };
    }
  }

  function requestRemovalConfirmation(): void {
    removalView = { kind: 'confirming' };
  }

  function cancelRemoval(): void {
    removalView = { kind: 'idle' };
  }

  async function confirmProjectRemoval(): Promise<void> {
    removalView = { kind: 'submitting' };
    try {
      await projectRemover();
      removalView = { kind: 'removed' };
      projectView = { kind: 'idle' };
      projectStatusView = { kind: 'noProject' };
      indexActivityView = { kind: 'noProject' };
      indexOverviewView = { kind: 'noProject' };
      indexActivityObserved = false;
      await loadRecentProjects();
    } catch (error) {
      removalView = {
        kind: 'error',
        message: projectActionRecoveryMessage(error, 'remove'),
      };
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

  function indexActivityStateLabel(state: IndexActivityStateV1): string {
    const labels: Record<IndexActivityStateV1, string> = {
      idle: 'Noch kein Lauf in dieser Sitzung',
      queued: 'Indexlauf wartet auf einen Worker',
      running: 'Fast Index läuft',
      cancelling: 'Indexlauf wird kontrolliert beendet',
      succeeded: 'Fast Index abgeschlossen',
      failed: 'Indexlauf fehlgeschlagen; veröffentlichter Snapshot bleibt lesbar',
      cancelled: 'Indexlauf abgebrochen; veröffentlichter Snapshot bleibt lesbar',
    };
    return labels[state];
  }

  function indexPhaseLabel(phase: IndexPhaseV1): string {
    const labels: Record<IndexPhaseV1, string> = {
      discover: 'Dateien ermitteln',
      hash: 'Inhalte hashen',
      parse: 'Quellcode parsen',
      link: 'Beziehungen verknüpfen',
      rank: 'Symbole und Module gewichten',
      publish: 'Snapshot atomar veröffentlichen',
    };
    return labels[phase];
  }

  function countLabel(value: string): string {
    return new Intl.NumberFormat('de-DE').format(BigInt(value));
  }

  function coverageLabel(value: number | null): string {
    return value === null
      ? 'Keine strukturellen Parserdaten'
      : new Intl.NumberFormat('de-DE', {
          maximumFractionDigits: 2,
          minimumFractionDigits: 2,
          style: 'percent',
        }).format(value / 10_000);
  }

  function indexLanguageLabel(language: IndexLanguageV1): string {
    const labels: Record<IndexLanguageV1, string> = {
      generic: 'Generisch',
      python: 'Python',
      rust: 'Rust',
      typeScriptJavaScript: 'TypeScript/JavaScript',
    };
    return labels[language];
  }

  function diagnosticCodeLabel(code: IndexDiagnosticCodeV1): string {
    const labels: Record<IndexDiagnosticCodeV1, string> = {
      invalidEncoding: 'Ungültige Zeichenkodierung',
      missingSyntax: 'Fehlende Syntax',
      outputTruncated: 'Begrenzte Parserausgabe',
      syntaxError: 'Syntaxfehler',
      unsupportedSyntax: 'Nicht unterstützte Syntax',
    };
    return labels[code];
  }

  function diagnosticSeverityLabel(severity: IndexDiagnosticSeverityV1): string {
    const labels: Record<IndexDiagnosticSeverityV1, string> = {
      error: 'Fehler',
      information: 'Hinweis',
      warning: 'Warnung',
    };
    return labels[severity];
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
      <p class="project-error" role="alert">{projectView.message}</p>
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
            <dt>Aktueller Indexlauf</dt>
            {#if indexActivityView.kind === 'active'}
              <dd>{indexActivityStateLabel(indexActivityView.result.activity.state)}</dd>
            {:else if indexActivityView.kind === 'loading'}
              <dd>Wird geladen …</dd>
            {:else}
              <dd>Nicht verfügbar</dd>
            {/if}
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
        {#if indexActivityView.kind === 'active' && indexActivityView.result.activity.phase !== null}
          <div class="index-progress" aria-labelledby="index-progress-heading">
            <h4 id="index-progress-heading">Fast-Index-Fortschritt</h4>
            <p role="status" aria-live="polite">
              {#if indexActivityView.result.activity.completedPhases === indexActivityView.result.activity.totalPhases}
                Alle {indexActivityView.result.activity.totalPhases} Phasen abgeschlossen:
                {indexPhaseLabel(indexActivityView.result.activity.phase)}
              {:else}
                Phase {indexActivityView.result.activity.completedPhases + 1} von
                {indexActivityView.result.activity.totalPhases}:
                {indexPhaseLabel(indexActivityView.result.activity.phase)}
              {/if}
            </p>
            <progress
              aria-label="Fast-Index-Fortschritt"
              max={indexActivityView.result.activity.totalPhases}
              value={indexActivityView.result.activity.completedPhases}
            ></progress>
            {#if (indexActivityView.result.activity.state === 'queued' || indexActivityView.result.activity.state === 'running' || indexActivityView.result.activity.state === 'cancelling') && projectStatusView.result.index.publishedSnapshotId !== null}
              <p>
                Der zuletzt veröffentlichte Snapshot bleibt während dieses Laufs vollständig lesbar.
              </p>
            {/if}
          </div>
        {/if}
        <div class="index-overview" aria-labelledby="index-overview-heading">
          <h4 id="index-overview-heading">Veröffentlichter Fast Index</h4>
          {#if indexOverviewView.kind === 'loading'}
            <p class="project-status" role="status" aria-live="polite">
              Veröffentlichter Index wird gelesen …
            </p>
          {:else if indexOverviewView.kind === 'noPublishedIndex'}
            <p class="project-status">
              Noch kein vollständiger Snapshot veröffentlicht. Ein laufender Aufbau bleibt davon
              getrennt.
            </p>
          {:else if indexOverviewView.kind === 'published'}
            <p class="index-snapshot">
              Snapshot <code>{indexOverviewView.result.overview.snapshotId}</code>
            </p>
            <dl class="index-metrics">
              <div>
                <dt>Dateien</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.fileCount)}</dd>
              </div>
              <div>
                <dt>Symbole</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.symbolCount)}</dd>
              </div>
              <div>
                <dt>Diagnostics</dt>
                <dd>{countLabel(indexOverviewView.result.overview.counts.diagnosticCount)}</dd>
              </div>
              <div>
                <dt>Parse Coverage</dt>
                <dd>{coverageLabel(indexOverviewView.result.overview.coverageBasisPoints)}</dd>
              </div>
            </dl>
            <p class="index-coverage-note">
              {countLabel(indexOverviewView.result.overview.counts.parsedFileCount)} von
              {countLabel(indexOverviewView.result.overview.counts.fileCount)} Dateien strukturell geparst.
            </p>
            {#if indexOverviewView.result.overview.diagnosticFiles.length === 0}
              <p class="ready-label">Keine Parser-Diagnostics im veröffentlichten Snapshot.</p>
            {:else}
              <div class="file-diagnostics" aria-labelledby="file-diagnostics-heading">
                <h5 id="file-diagnostics-heading">Indexfehler pro Datei</h5>
                <ul>
                  {#each indexOverviewView.result.overview.diagnosticFiles as file, fileIndex (fileIndex)}
                    <li>
                      <div class="diagnostic-file-heading">
                        <code>{file.pathDisplay}{file.pathDisplayTruncated ? '…' : ''}</code>
                        <span>{indexLanguageLabel(file.language)}</span>
                      </div>
                      <p>
                        {countLabel(file.diagnosticCount)} Diagnostics · Coverage
                        {coverageLabel(file.coverageBasisPoints)}
                      </p>
                      <ul>
                        {#each file.diagnostics as diagnostic, diagnosticIndex (diagnosticIndex)}
                          <li>
                            <strong>{diagnosticSeverityLabel(diagnostic.severity)}:</strong>
                            {diagnosticCodeLabel(diagnostic.code)} · {diagnostic.message}
                            <span>Bytes {diagnostic.startByte}–{diagnostic.endByte}</span>
                          </li>
                        {/each}
                      </ul>
                      {#if file.diagnosticsTruncated}
                        <p>
                          Weitere Diagnostics dieser Datei sind in dieser begrenzten Ansicht
                          verborgen.
                        </p>
                      {/if}
                    </li>
                  {/each}
                </ul>
                {#if indexOverviewView.result.overview.diagnosticFilesTruncated}
                  <p>
                    Weitere fehlerhafte Dateien sind in dieser auf 64 Dateien begrenzten Ansicht
                    verborgen.
                  </p>
                {/if}
              </div>
            {/if}
          {:else if indexOverviewView.kind === 'error'}
            <div class="recent-projects-error" role="alert">
              <p>Der veröffentlichte Index konnte nicht sicher gelesen werden.</p>
              <button type="button" onclick={loadIndexOverview}>Indexübersicht erneut laden</button>
            </div>
          {/if}
        </div>
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
            <button type="button" onclick={refreshProjectDetails}>Status aktualisieren</button>
          </div>
          {#if rebuildView.kind === 'error'}
            <p class="project-error" role="alert">{rebuildView.message}</p>
          {/if}
        </div>
        <div class="project-maintenance project-removal" aria-labelledby="removal-heading">
          <h4 id="removal-heading">Worktree aus A^3 entfernen</h4>
          <p>
            Entfernt nur diesen Eintrag aus der A^3-Projektliste. Repository-Dateien werden nie
            gelöscht. Private A^3-Daten bleiben erhalten und stehen beim sicheren Wiederöffnen
            erneut bereit.
          </p>
          {#if removalView.kind === 'confirming'}
            <div class="removal-confirmation" role="group" aria-labelledby="removal-confirmation">
              <p id="removal-confirmation">
                Wirklich nur aus der Projektliste entfernen? Der lokale Worktree bleibt vollständig
                bestehen.
              </p>
              <div class="project-actions">
                <button class="risk-action" type="button" onclick={confirmProjectRemoval}
                  >Entfernen bestätigen</button
                >
                <button type="button" onclick={cancelRemoval}>Abbrechen</button>
              </div>
            </div>
          {:else}
            <div class="project-actions">
              <button
                class="risk-action"
                type="button"
                disabled={removalView.kind === 'submitting'}
                onclick={requestRemovalConfirmation}
              >
                {removalView.kind === 'submitting'
                  ? 'Worktree wird entfernt …'
                  : 'Nur aus A^3 entfernen'}
              </button>
            </div>
          {/if}
          {#if removalView.kind === 'error'}
            <p class="project-error" role="alert">
              {removalView.message} Repository und private A^3-Daten wurden nicht gelöscht.
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

    {#if removalView.kind === 'removed'}
      <p class="ready-label" role="status" aria-live="polite">
        Worktree aus der A^3-Projektliste entfernt. Repository und private A^3-Daten bleiben
        erhalten.
      </p>
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

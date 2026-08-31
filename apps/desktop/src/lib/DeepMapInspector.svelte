<script lang="ts">
  import {
    queryDeepMapEntries,
    queryDeepMapRuns,
    type DeepMapEntryPageResponseV1,
    type DeepMapEntryV1,
    type DeepMapFailureV3,
    type DeepMapRunPageResponseV1,
    type DeepMapRunV1,
  } from './deep-map';
  import {
    queryDeepMapAtlasImpact,
    queryDeepMapModuleSteps,
    queryDeepMapRunDashboard,
    queryDeepMapRunModules,
    type DeepMapAtlasImpactResponseV1,
    type DeepMapCardFieldV1,
    type DeepMapDashboardPhaseV1,
    type DeepMapModuleStepsResponseV1,
    type DeepMapRunDashboardResponseV1,
    type DeepMapRunModuleV1,
    type DeepMapRunModulesResponseV1,
    type DeepMapSelectionReasonV1,
  } from './deep-map-dashboard';
  import {
    queryModuleCardDetail,
    type ModuleCardDetailQueryV1,
    type ModuleCardDetailResponseV1,
    type ModuleCardDetailV1,
    type ModuleCardFieldKindV1,
  } from './module-card-detail';
  import {
    queryProjectMapSourcePreview,
    type ProjectMapSourcePreviewQueryV1,
    type ProjectMapSourcePreviewResponseV1,
    type ProjectMapSourcePreviewV1,
  } from './project-map-source-preview';

  interface Props {
    atlasImpactLoader?: (
      runSelection: string,
      moduleSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapAtlasImpactResponseV1>;
    cardLoader?: (query: ModuleCardDetailQueryV1) => Promise<ModuleCardDetailResponseV1>;
    dashboardLoader?: (runSelection: string) => Promise<DeepMapRunDashboardResponseV1>;
    entriesLoader?: (
      runSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapEntryPageResponseV1>;
    focusFailureEpoch?: number;
    modulesLoader?: (
      runSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapRunModulesResponseV1>;
    onclose: () => void;
    onshowinatlas?: (runSelection: string, moduleSelection: string) => void | Promise<void>;
    open: boolean;
    runsLoader?: (cursor?: string | null) => Promise<DeepMapRunPageResponseV1>;
    sourcePreviewLoader?: (
      query: ProjectMapSourcePreviewQueryV1,
    ) => Promise<ProjectMapSourcePreviewResponseV1>;
    stepsLoader?: (
      runSelection: string,
      moduleSelection: string,
      cursor?: string | null,
    ) => Promise<DeepMapModuleStepsResponseV1>;
  }

  const {
    open,
    focusFailureEpoch = 0,
    onclose,
    onshowinatlas = () => undefined,
    runsLoader = queryDeepMapRuns,
    dashboardLoader = queryDeepMapRunDashboard,
    modulesLoader = queryDeepMapRunModules,
    stepsLoader = queryDeepMapModuleSteps,
    entriesLoader = queryDeepMapEntries,
    cardLoader = queryModuleCardDetail,
    atlasImpactLoader = queryDeepMapAtlasImpact,
    sourcePreviewLoader = queryProjectMapSourcePreview,
  }: Props = $props();

  let runPage = $state<DeepMapRunPageResponseV1 | null>(null);
  let selectedRun = $state<DeepMapRunV1 | null>(null);
  let dashboard = $state<DeepMapRunDashboardResponseV1 | null>(null);
  let modules = $state<DeepMapRunModuleV1[]>([]);
  let moduleCursor = $state<string | null>(null);
  let entryPage = $state<DeepMapEntryPageResponseV1 | null>(null);
  let expandedModule = $state<string | null>(null);
  let stepsByModule = $state<Record<string, DeepMapModuleStepsResponseV1>>({});
  let cardsByModule = $state<Record<string, ModuleCardDetailResponseV1>>({});
  let impactsByModule = $state<Record<string, DeepMapAtlasImpactResponseV1>>({});
  let preview = $state<ProjectMapSourcePreviewV1 | null>(null);
  let previewKey = $state<string | null>(null);
  let busy = $state(false);
  let failed = $state(false);
  let pollTick = $state(0);
  let generation = 0;
  let seenFailureEpoch = 0;
  let previouslyOpen = false;

  const activeRun = $derived(
    dashboard !== null &&
      ['queued', 'running', 'pausing', 'paused', 'cancelling'].includes(dashboard.state),
  );
  const expanded = $derived(
    expandedModule === null
      ? null
      : (modules.find((module) => module.selection === expandedModule) ?? null),
  );

  $effect(() => {
    if (open && !previouslyOpen) void loadRuns(null, false);
    previouslyOpen = open;
  });

  $effect(() => {
    if (!open || focusFailureEpoch <= seenFailureEpoch) return;
    seenFailureEpoch = focusFailureEpoch;
    void focusLatestFailure();
  });

  $effect(() => {
    void pollTick;
    if (!open || selectedRun === null || !activeRun) return;
    const timer = window.setTimeout(async () => {
      await refreshSelectedRun(true);
      pollTick += 1;
    }, 1_500);
    return () => window.clearTimeout(timer);
  });

  async function loadRuns(cursor: string | null, focusFailure: boolean): Promise<void> {
    const request = ++generation;
    busy = true;
    failed = false;
    try {
      const page = await runsLoader(cursor);
      if (request !== generation) return;
      runPage = page;
      const run = focusFailure
        ? (page.runs.find((item) => item.state === 'failed') ?? page.runs[0] ?? null)
        : (page.runs[0] ?? null);
      await chooseRun(run, request);
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function chooseRun(run: DeepMapRunV1 | null, request = ++generation): Promise<void> {
    selectedRun = run;
    dashboard = null;
    modules = [];
    moduleCursor = null;
    entryPage = null;
    expandedModule = null;
    stepsByModule = {};
    cardsByModule = {};
    impactsByModule = {};
    preview = null;
    previewKey = null;
    if (run === null) return;
    await refreshSelectedRun(false, request);
  }

  async function refreshSelectedRun(silent: boolean, request = ++generation): Promise<void> {
    const run = selectedRun;
    if (run === null) return;
    if (!silent) busy = true;
    failed = false;
    try {
      const [nextDashboard, nextModules, nextEntries] = await Promise.all([
        dashboardLoader(run.selection),
        modulesLoader(run.selection, null),
        entriesLoader(run.selection, null),
      ]);
      if (request !== generation) return;
      dashboard = nextDashboard;
      const retainedExpanded = silent
        ? modules.find(
            (item) =>
              item.selection === expandedModule &&
              !nextModules.modules.some((next) => next.selection === item.selection),
          )
        : undefined;
      modules =
        retainedExpanded === undefined
          ? nextModules.modules
          : [...nextModules.modules, retainedExpanded];
      moduleCursor = nextModules.nextCursor;
      entryPage = nextEntries;
      if (expandedModule !== null) {
        const module = modules.find((item) => item.selection === expandedModule);
        if (module !== undefined) await loadModuleDetails(module, request);
      }
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation && !silent) busy = false;
    }
  }

  async function loadMoreModules(): Promise<void> {
    if (selectedRun === null || moduleCursor === null) return;
    const request = generation;
    busy = true;
    try {
      const page = await modulesLoader(selectedRun.selection, moduleCursor);
      if (request !== generation) return;
      const known = new Set(modules.map((module) => module.selection));
      modules = [...modules, ...page.modules.filter((module) => !known.has(module.selection))];
      moduleCursor = page.nextCursor;
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function loadMoreSteps(module: DeepMapRunModuleV1): Promise<void> {
    const run = selectedRun;
    const current = stepsByModule[module.selection];
    if (run === null || current?.nextCursor === null || current === undefined) return;
    busy = true;
    try {
      const page = await stepsLoader(run.selection, module.selection, current.nextCursor);
      stepsByModule = {
        ...stepsByModule,
        [module.selection]: {
          ...page,
          steps: [...current.steps, ...page.steps],
          historicalDetailsLimited:
            current.historicalDetailsLimited || page.historicalDetailsLimited,
        },
      };
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }

  async function loadMoreImpact(module: DeepMapRunModuleV1): Promise<void> {
    const run = selectedRun;
    const response = impactsByModule[module.selection];
    const current = response?.result;
    if (
      run === null ||
      current === undefined ||
      current.status !== 'available' ||
      current.nextCursor === null
    )
      return;
    busy = true;
    try {
      const page = await atlasImpactLoader(run.selection, module.selection, current.nextCursor);
      if (page.result.status !== 'available') return;
      impactsByModule = {
        ...impactsByModule,
        [module.selection]: {
          ...page,
          result: { ...page.result, items: [...current.items, ...page.result.items] },
        },
      };
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }

  async function toggleModule(module: DeepMapRunModuleV1): Promise<void> {
    if (expandedModule === module.selection) {
      expandedModule = null;
      preview = null;
      previewKey = null;
      return;
    }
    expandedModule = module.selection;
    preview = null;
    previewKey = null;
    await loadModuleDetails(module);
  }

  async function loadModuleDetails(
    module: DeepMapRunModuleV1,
    request = generation,
  ): Promise<void> {
    const run = selectedRun;
    if (run === null) return;
    const work: Promise<unknown>[] = [];
    if (stepsByModule[module.selection] === undefined) {
      work.push(
        stepsLoader(run.selection, module.selection, null).then((response) => {
          if (request === generation)
            stepsByModule = { ...stepsByModule, [module.selection]: response };
        }),
      );
    }
    if (module.cardAvailable && cardsByModule[module.selection] === undefined) {
      work.push(
        cardLoader({ runSelection: run.selection, moduleSelection: module.selection }).then(
          (response) => {
            if (request === generation)
              cardsByModule = { ...cardsByModule, [module.selection]: response };
          },
        ),
      );
    }
    if (module.cardAvailable && impactsByModule[module.selection] === undefined) {
      work.push(
        atlasImpactLoader(run.selection, module.selection, null).then((response) => {
          if (request === generation)
            impactsByModule = { ...impactsByModule, [module.selection]: response };
        }),
      );
    }
    if (work.length === 0) return;
    busy = true;
    try {
      await Promise.all(work);
    } catch {
      if (request === generation) failed = true;
    } finally {
      if (request === generation) busy = false;
    }
  }

  async function selectRun(event: Event): Promise<void> {
    const index = Number((event.currentTarget as HTMLSelectElement).value);
    const run = Number.isInteger(index) && index >= 0 ? (runPage?.runs[index] ?? null) : null;
    await chooseRun(run);
  }

  async function focusLatestFailure(): Promise<void> {
    const failedRun = runPage?.runs.find((run) => run.state === 'failed');
    if (failedRun !== undefined) await chooseRun(failedRun);
    else await loadRuns(null, true);
  }

  async function showSource(card: ModuleCardDetailV1, evidenceId: string): Promise<void> {
    const key = `${card.cardId}:${evidenceId}`;
    if (previewKey === key) {
      preview = null;
      previewKey = null;
      return;
    }
    busy = true;
    preview = null;
    previewKey = key;
    try {
      const response = await sourcePreviewLoader({
        kind: 'moduleCard',
        cardId: card.cardId,
        currentIndexRunId: card.currentIndexRunId,
        currentSnapshotId: card.currentSnapshotId,
        evidenceId,
        moduleId: card.moduleId,
        sourceIndexRunId: card.sourceIndexRunId,
        sourceSnapshotId: card.sourceSnapshotId,
      });
      if (previewKey === key && response.result.status === 'available') {
        preview = response.result.preview;
      }
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }

  function availableCard(moduleSelection: string): ModuleCardDetailV1 | null {
    const result = cardsByModule[moduleSelection]?.result;
    return result?.status === 'available' && result.detail.lifecycle.status === 'current'
      ? result.detail
      : null;
  }

  function availableImpact(
    moduleSelection: string,
  ): Extract<DeepMapAtlasImpactResponseV1['result'], { status: 'available' }> | null {
    const result = impactsByModule[moduleSelection]?.result;
    return result?.status === 'available' ? result : null;
  }

  async function loadOlderHistory(): Promise<void> {
    const run = selectedRun;
    const cursor = entryPage?.nextCursor;
    if (run === null || cursor === null || cursor === undefined) return;
    busy = true;
    try {
      entryPage = await entriesLoader(run.selection, cursor);
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }

  function formatTime(value: string): string {
    return new Intl.DateTimeFormat('de-DE', { dateStyle: 'short', timeStyle: 'short' }).format(
      new Date(Number(value)),
    );
  }

  function progressPercent(confirmed: string, total: string): number {
    const denominator = BigInt(total);
    if (denominator === BigInt(0)) return 0;
    return Number((BigInt(confirmed) * BigInt(100)) / denominator);
  }

  function stateLabel(value: string): string {
    return (
      (
        {
          queued: 'Eingeplant',
          running: 'Läuft',
          pausing: 'Wird pausiert',
          paused: 'Pausiert',
          cancelling: 'Wird abgebrochen',
          completed: 'Abgeschlossen',
          succeeded: 'Abgeschlossen',
          alreadyCurrent: 'Atlas war bereits aktuell',
          failed: 'Fehlgeschlagen',
          cancelled: 'Abgebrochen',
          interrupted: 'Unterbrochen',
        } as Record<string, string>
      )[value] ?? value
    );
  }

  function moduleStateLabel(value: string): string {
    return (
      (
        {
          planned: 'Geplant',
          exploring: 'In Arbeit',
          verifying: 'Wird geprüft',
          published: 'Veröffentlicht',
          incomplete: 'Unvollständig',
        } as Record<string, string>
      )[value] ?? value
    );
  }

  function phaseLabel(value: DeepMapDashboardPhaseV1): string {
    return {
      planning: 'Planen',
      exploring: 'Erkunden',
      creatingCards: 'Module Cards erstellen',
      verifying: 'Prüfen',
      updatingAtlas: 'Atlas aktualisieren',
    }[value];
  }

  function fieldLabel(value: DeepMapCardFieldV1 | ModuleCardFieldKindV1): string {
    return {
      title: 'Titel',
      paths: 'Pfade',
      purpose: 'Zweck',
      responsibilities: 'Aufgaben',
      publicSurface: 'Schnittstellen',
      entrypoints: 'Einstiegspunkte',
      dependencies: 'Abhängigkeiten',
      dataFlows: 'Datenflüsse',
      invariants: 'Regeln',
      tests: 'Tests',
      risks: 'Risiken',
      openQuestions: 'Offene Fragen',
    }[value];
  }

  function reasonLabel(value: DeepMapSelectionReasonV1): string {
    return {
      manifest: 'Das Manifest zeigt Modulgrenzen und Abhängigkeiten.',
      entrypoint: 'Dieser Einstiegspunkt zeigt öffentliches Verhalten und Abläufe.',
      centralSymbol: 'Dieses zentrale Symbol erklärt die Kernaufgaben des Moduls.',
      testRoot: 'Diese Tests machen Regeln, Sonderfälle und Risiken sichtbar.',
      graphCommunity: 'Starke Beziehungen weisen auf einen gemeinsamen Architekturbereich hin.',
      uncoveredModule: 'Für wichtige Card-Felder fehlen noch bestätigte Hinweise.',
    }[value];
  }

  function eventSentence(entry: DeepMapEntryV1): string {
    const action =
      (
        {
          buildPlan: 'Der Erkundungsplan wurde vorbereitet.',
          inspect: 'Ein ausgewähltes Ziel wurde untersucht.',
          search: 'Der Projektindex wurde nach passenden Hinweisen durchsucht.',
          propose: 'Die Untersuchung wurde als Ergebnis festgehalten.',
          generateClaims: 'Aus bestätigten Hinweisen wurden Card-Aussagen erstellt.',
          verifyEvidence: 'Aussagen und Quellen wurden geprüft.',
          publishCards: 'Verifizierte Module Cards wurden im Atlas veröffentlicht.',
        } as Record<string, string>
      )[entry.action ?? ''] ?? 'Der Laufstatus wurde aktualisiert.';
    if (entry.result === 'paused')
      return 'Der Lauf wurde sicher pausiert; der Arbeitsstand bleibt erhalten.';
    if (entry.result === 'resumed') return 'Der pausierte Arbeitsstand wurde fortgesetzt.';
    if (entry.result === 'cancelled') return 'Der Lauf wurde abgebrochen.';
    if (entry.result === 'interrupted')
      return 'Der Lauf wurde durch das Ende des vorherigen Prozesses unterbrochen.';
    return action;
  }

  function claimLabel(value: 'fact' | 'observation' | 'hypothesis'): string {
    return { fact: 'Belegt', observation: 'Beobachtung', hypothesis: 'Hypothese' }[value];
  }

  function failureInfo(code: DeepMapFailureV3): {
    title: string;
    cause: string;
    action: string;
  } {
    const fallback = {
      title: 'Deep Map konnte nicht abgeschlossen werden',
      cause: 'Ein sicherer Verarbeitungsschritt ist fehlgeschlagen.',
      action: 'Starte Deep Map auf dem aktuellen Projektstand erneut.',
    };
    return (
      (
        {
          noPublishedIndex: {
            title: 'Noch kein nutzbarer Projektindex',
            cause: 'Deep Map braucht zuerst eine vollständige Code-Analyse.',
            action: 'Warte den Fast Index ab und starte danach erneut.',
          },
          staleIndex: {
            title: 'Der Projektstand hat sich geändert',
            cause: 'Dieser Lauf gehört nicht mehr zur aktuellen Code-Analyse.',
            action: 'Starte eine neue Deep Map für den aktuellen Projektstand.',
          },
          modelUnavailable: {
            title: 'Das Mapping-Modell ist nicht erreichbar',
            cause: 'A^3 konnte die lokale Modellverbindung nicht verwenden.',
            action: 'Prüfe die Modellverbindung und starte den Lauf erneut.',
          },
          modelRejected: {
            title: 'Das Modell hat die Anfrage abgelehnt',
            cause:
              'Der begrenzte Mapping-Schritt wurde vom eingerichteten Modell nicht angenommen.',
            action: 'Prüfe das Mapping-Modell und starte den Lauf erneut.',
          },
          modelTimeout: {
            title: 'Die Untersuchung hat zu lange gedauert',
            cause: 'Das Modell hat einen begrenzten Arbeitsschritt nicht rechtzeitig beendet.',
            action: 'Versuche es erneut oder verwende einen schnelleren Modus.',
          },
          verification: {
            title: 'Aussagen konnten nicht sicher bestätigt werden',
            cause: 'Mindestens eine Aussage passte nicht eindeutig zu aktueller Evidenz.',
            action: 'Erstelle bei Änderungen zuerst einen neuen Index und starte erneut.',
          },
          publicationStorage: {
            title: 'Der Atlas konnte nicht gespeichert werden',
            cause: 'Der lokale Veröffentlichungsschritt war nicht verfügbar.',
            action: 'Prüfe den lokalen Speicher und versuche es erneut.',
          },
          interrupted: {
            title: 'Der Lauf wurde unterbrochen',
            cause: 'A^3 wurde beendet, bevor der Lauf abgeschlossen war.',
            action: 'Starte Deep Map auf dem aktuellen Projektstand erneut.',
          },
        } as Partial<Record<DeepMapFailureV3, { title: string; cause: string; action: string }>>
      )[code] ?? fallback
    );
  }
</script>

<aside
  class:open
  class="inspector deep-map-inspector"
  aria-label="Deep-Map-Details"
  aria-hidden={!open}
  inert={!open}
>
  <header class="inspector-head">
    <div>
      <span>Live-Informationssystem</span>
      <h3>Deep Map</h3>
    </div>
    <button type="button" aria-label="Deep-Map-Details schließen" onclick={onclose}>×</button>
  </header>

  {#if open}
    <div class="content">
      {#if failed}
        <div class="notice error" role="alert">
          <strong>Die aktuellen Informationen konnten nicht vollständig geladen werden.</strong>
          <button type="button" onclick={() => refreshSelectedRun(false)}>Erneut laden</button>
        </div>
      {/if}

      <section class="run-choice" aria-labelledby="deep-map-run-heading">
        <label for="deep-map-run-heading">Lauf auswählen</label>
        <div>
          <select
            id="deep-map-run-heading"
            value={String(
              runPage?.runs.findIndex((run) => run.selection === selectedRun?.selection) ?? -1,
            )}
            onchange={selectRun}
          >
            {#if runPage?.runs.length === 0}<option value="">Noch kein Lauf vorhanden</option>{/if}
            {#each runPage?.runs ?? [] as run, runIndex (run.selection)}
              <option value={runIndex}
                >{formatTime(run.startedAtUnixMillis)} · {stateLabel(run.state)}</option
              >
            {/each}
          </select>
          {#if runPage?.nextCursor}
            <button
              type="button"
              disabled={busy}
              onclick={() => loadRuns(runPage?.nextCursor ?? null, false)}>Ältere</button
            >
          {/if}
        </div>
      </section>

      {#if selectedRun === null && !busy}
        <div class="notice">
          <strong>Noch keine Deep Map vorhanden</strong>
          <p>
            Starte Deep Map, damit A^3 Module gezielt untersucht und den Atlas mit belegten
            Informationen anreichert.
          </p>
        </div>
      {:else if dashboard !== null}
        <section class="overview" aria-labelledby="deep-map-overview-heading">
          <div class="overview-title">
            <div>
              <span>Gesamtzustand</span>
              <h4 id="deep-map-overview-heading">{stateLabel(dashboard.state)}</h4>
            </div>
            <strong>{dashboard.confirmedSteps} von {dashboard.totalSteps} Zielen bestätigt</strong>
          </div>
          <div
            class="progress"
            aria-label={`Fortschritt: ${dashboard.confirmedSteps} von ${dashboard.totalSteps}`}
          >
            <span
              style={`width: ${progressPercent(dashboard.confirmedSteps, dashboard.totalSteps)}%`}
            ></span>
          </div>
          <ol class="phases" aria-label="Deep-Map-Phasen">
            {#each dashboard.phases as phase (phase.phase)}
              <li class={phase.state}>
                <span aria-hidden="true"
                  >{phase.state === 'completed'
                    ? '✓'
                    : phase.state === 'active'
                      ? '●'
                      : phase.state === 'stopped'
                        ? '!'
                        : '○'}</span
                >
                <strong>{phaseLabel(phase.phase)}</strong>
              </li>
            {/each}
          </ol>
          {#if dashboard.freshness === 'historical'}
            <div class="historical">
              <strong>Dieser Lauf beschreibt einen älteren Projektstand.</strong>
              <p>
                Heutige Module Cards werden hier bewusst nicht eingeblendet. Starte eine neue Deep
                Map, um den aktuellen Atlas anzureichern.
              </p>
            </div>
          {/if}
        </section>

        {#if dashboard.failure !== null}
          {@const info = failureInfo(dashboard.failure.cause)}
          <section class="failure-card" role="alert" aria-labelledby="deep-map-failure-heading">
            <span>Hilfe</span>
            <h4 id="deep-map-failure-heading">{info.title}</h4>
            <p><b>Ursache:</b> {info.cause}</p>
            <p>
              <b>Erhaltener Stand:</b>
              {dashboard.failure.confirmedWorkRetained
                ? 'Bereits bestätigte Schritte bleiben im Laufjournal nachvollziehbar.'
                : 'Es wurden noch keine Schritte bestätigt.'}
            </p>
            <p><b>Nächster Schritt:</b> {info.action}</p>
            {#if dashboard.failure.diagnosticCode}<details>
                <summary>Diagnosecode anzeigen</summary><code
                  >{dashboard.failure.diagnosticCode}</code
                >
              </details>{/if}
          </section>
        {/if}

        <section class="current-work" aria-labelledby="deep-map-current-heading">
          <span>Was passiert gerade?</span>
          <h4 id="deep-map-current-heading">
            {dashboard.currentActivity?.moduleName ??
              (activeRun
                ? 'Der nächste sichere Schritt wird vorbereitet'
                : 'Dieser Lauf arbeitet gerade nicht')}
          </h4>
          {#if dashboard.currentActivity !== null}
            {#if dashboard.currentActivity.targetLabel}<p>
                <b>Untersucht wird:</b>
                {dashboard.currentActivity.targetLabel}
              </p>{/if}
            {#if dashboard.currentActivity.selectionReason}<p>
                <b>Warum:</b>
                {reasonLabel(dashboard.currentActivity.selectionReason)}
              </p>{/if}
            {#if dashboard.currentActivity.cardFields.length > 0}<div
                class="chips"
                aria-label="Gesuchte Informationen"
              >
                {#each dashboard.currentActivity.cardFields as field (field)}<span
                    >{fieldLabel(field)}</span
                  >{/each}
              </div>{/if}
          {:else}
            <p>
              {dashboard.state === 'completed'
                ? 'Alle geplanten Ziele wurden verarbeitet und die bestätigten Ergebnisse veröffentlicht.'
                : dashboard.state === 'paused'
                  ? 'Der Arbeitsstand ist sicher gespeichert und kann fortgesetzt werden.'
                  : 'Der Lauf wartet auf den nächsten Schritt.'}
            </p>
          {/if}
        </section>

        <section class="plan" aria-labelledby="deep-map-plan-heading">
          <div class="section-heading">
            <div>
              <span>Der Plan</span>
              <h4 id="deep-map-plan-heading">Erkundungsziele nach Modul</h4>
            </div>
            <small>{modules.length} Module geladen</small>
          </div>
          {#if dashboard.historicalPlanLimited}<p class="limited">
              Dieser ältere Lauf enthält nur einen kompakten Plan; konkrete Ziele und Card-Felder
              wurden damals noch nicht gespeichert.
            </p>{/if}
          <div class="module-list">
            {#each modules as module, moduleIndex (module.selection)}
              <article class:expanded={expandedModule === module.selection} class="module-card">
                <button
                  class="module-toggle"
                  type="button"
                  aria-expanded={expandedModule === module.selection}
                  onclick={() => toggleModule(module)}
                >
                  <div>
                    <span class={`status ${module.state}`}>{moduleStateLabel(module.state)}</span>
                    <h5>{module.displayName}</h5>
                    <small>{module.confirmedSteps} von {module.plannedSteps} Zielen bestätigt</small
                    >
                  </div>
                  <span aria-hidden="true">{expandedModule === module.selection ? '−' : '+'}</span>
                </button>
                {#if expandedModule === module.selection}
                  <div class="module-body">
                    <section aria-labelledby={`deep-map-steps-${moduleIndex}`}>
                      <h6 id={`deep-map-steps-${moduleIndex}`}>Was wird erkundet?</h6>
                      {#if stepsByModule[module.selection] === undefined}<p class="loading">
                          Plan wird aufgelöst …
                        </p>{:else}
                        <ol class="step-list">
                          {#each stepsByModule[module.selection].steps as step (step.position)}
                            <li class={step.state}>
                              <div>
                                <span
                                  >{step.state === 'confirmed'
                                    ? 'Bestätigt'
                                    : step.state === 'exploring'
                                      ? 'Wird erkundet'
                                      : 'Geplant'}</span
                                ><strong>{step.targetLabel ?? 'Historisches Ziel'}</strong>
                              </div>
                              <p>{reasonLabel(step.selectionReason)}</p>
                              {#if step.cardFields !== null}<div class="chips">
                                  {#each step.cardFields as field (field)}<span
                                      >{fieldLabel(field)}</span
                                    >{/each}
                                </div>{/if}
                            </li>
                          {/each}
                        </ol>
                        {#if stepsByModule[module.selection].historicalDetailsLimited}<p
                            class="limited"
                          >
                            Für diese historischen Schritte sind Ziel und geplante Card-Felder nicht
                            mehr vollständig verfügbar.
                          </p>{/if}
                        {#if stepsByModule[module.selection].nextCursor}<button
                            class="page-button"
                            type="button"
                            disabled={busy}
                            onclick={() => loadMoreSteps(module)}>Weitere Ziele laden</button
                          >{/if}
                      {/if}
                    </section>

                    <section class="card-detail" aria-labelledby={`deep-map-card-${moduleIndex}`}>
                      <div class="card-heading">
                        <h6 id={`deep-map-card-${moduleIndex}`}>Module Card</h6>
                        <span class={`status ${module.state}`}
                          >{moduleStateLabel(module.state)}</span
                        >
                      </div>
                      {#if !module.cardAvailable}<p>
                          {module.state === 'incomplete'
                            ? 'Der Lauf endete, bevor eine vollständige Card sicher veröffentlicht werden konnte.'
                            : 'Die Card erscheint hier erst nach erfolgreicher Prüfung und Veröffentlichung.'}
                        </p>
                      {:else if cardsByModule[module.selection] === undefined}<p class="loading">
                          Veröffentlichte Card wird geladen …
                        </p>
                      {:else}
                        {@const card = availableCard(module.selection)}
                        {#if card !== null}<div class="fields">
                            {#each card.fields as field (field.kind)}
                              <section>
                                <h6>{fieldLabel(field.kind)}</h6>
                                <ul>
                                  {#each field.values as item (item.claim.claimId)}
                                    <li>
                                      <p>{item.value}</p>
                                      <div class="claim-meta">
                                        <span class={item.claim.kind}
                                          >{claimLabel(item.claim.kind)}</span
                                        ><span
                                          >{item.claim.evidenceIds.length}
                                          {item.claim.evidenceIds.length === 1
                                            ? 'Quelle'
                                            : 'Quellen'}</span
                                        >{#if item.claim.evidenceIds[0]}<button
                                            type="button"
                                            onclick={() =>
                                              showSource(card, item.claim.evidenceIds[0])}
                                            >{previewKey ===
                                            `${card.cardId}:${item.claim.evidenceIds[0]}`
                                              ? 'Quelle schließen'
                                              : 'Quelle ansehen'}</button
                                          >{/if}
                                      </div>
                                      {#if previewKey === `${card.cardId}:${item.claim.evidenceIds[0]}` && preview !== null}<div
                                          class="source-preview"
                                        >
                                          <strong
                                            >{preview.pathDisplay} · Zeile {preview.startLine}</strong
                                          >
                                          <pre>{preview.text}</pre>
                                        </div>{/if}
                                    </li>
                                  {/each}
                                </ul>
                              </section>
                            {/each}
                          </div>
                        {:else}<p>
                            Die veröffentlichte Card ist für den aktuellen Projektstand nicht mehr
                            verfügbar.
                          </p>{/if}
                      {/if}
                    </section>
                  </div>
                {/if}
              </article>
            {/each}
          </div>
          {#if moduleCursor}<button
              class="page-button"
              type="button"
              disabled={busy}
              onclick={loadMoreModules}>Weitere Module laden</button
            >{/if}
        </section>

        <section class="atlas-impact" aria-labelledby="deep-map-impact-heading">
          <div class="section-heading">
            <div>
              <span>Im Atlas ergänzt</span>
              <h4 id="deep-map-impact-heading">Was sichtbar geworden ist</h4>
            </div>
            {#if expanded !== null && expanded.cardAvailable}<button
                type="button"
                onclick={() => onshowinatlas(selectedRun!.selection, expanded.selection)}
                >Im Atlas zeigen</button
              >{/if}
          </div>
          {#if expanded === null}<p>
              Klappe ein veröffentlichtes Modul auf, um seine konkrete Atlas-Anreicherung zu sehen.
            </p>
          {:else if !expanded.cardAvailable}<p>
              Für {expanded.displayName} gibt es noch keine aktuelle, verifizierte Atlas-Anreicherung.
            </p>
          {:else if impactsByModule[expanded.selection] === undefined}<p class="loading">
              Atlas-Anreicherung wird geladen …
            </p>
          {:else}
            {@const impact = availableImpact(expanded.selection)}
            {#if impact !== null}<div class="impact-summary">
                <div>
                  <span>Modulzweck</span><strong
                    >{impact.summary.purpose ?? 'Noch kein Zweck veröffentlicht'}</strong
                  >
                </div>
                <div><span>Sichtbare Risiken</span><strong>{impact.summary.riskCount}</strong></div>
                <div>
                  <span>Bestätigte Hinweise</span><strong
                    >{impact.summary.fileCount} Dateien · {impact.summary.symbolCount} Symbole · {impact
                      .summary.relationCount} Beziehungen</strong
                  >
                </div>
              </div>
              <ul class="impact-list">
                {#each impact.items as item (`${item.kind}:${item.label}`)}<li>
                    <span
                      >{item.kind === 'file'
                        ? 'Datei'
                        : item.kind === 'symbol'
                          ? 'Symbol'
                          : 'Beziehung'}</span
                    ><strong>{item.label}</strong><small
                      >{item.confirmedClaimCount} bestätigte {item.confirmedClaimCount === '1'
                        ? 'Aussage'
                        : 'Aussagen'}</small
                    >
                  </li>{/each}
              </ul>
              {#if impact.nextCursor}<button
                  class="page-button"
                  type="button"
                  disabled={busy}
                  onclick={() => loadMoreImpact(expanded!)}>Weitere Hinweise laden</button
                >{/if}
            {:else}<p>
                Dieser Lauf kann nicht sicher mit dem heutigen Atlas verbunden werden.
              </p>{/if}
          {/if}
        </section>

        <details class="history">
          <summary>Verlauf · {entryPage?.entries.length ?? 0} Ereignisse</summary>
          <ol>
            {#each entryPage?.entries ?? [] as entry (entry.selection)}<li
                class:failed={entry.failure !== null}
              >
                <time>{formatTime(entry.occurredAtUnixMillis)}</time>
                <p>{eventSentence(entry)}</p>
              </li>{/each}
          </ol>
          {#if entryPage?.nextCursor}<button
              type="button"
              disabled={busy}
              onclick={loadOlderHistory}>Älteren Verlauf laden</button
            >{/if}
        </details>
      {/if}

      {#if busy}<p class="loading" role="status">
          Deep-Map-Informationen werden aktualisiert …
        </p>{/if}
    </div>
  {/if}
</aside>

<style>
  .inspector {
    flex: 0 0 auto;
    min-width: 0;
    width: 0;
    overflow: hidden auto;
    border-left: 0 solid var(--line);
    background: var(--surface);
    transition: width 140ms ease;
  }
  .inspector.open {
    width: var(--inspector-width, 560px);
    border-left-width: 1px;
  }
  .inspector-head {
    position: sticky;
    top: 0;
    z-index: 4;
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-width: 320px;
    min-height: 66px;
    padding: 10px 12px 10px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .inspector-head span,
  .section-heading span,
  .current-work > span,
  .failure-card > span,
  .overview-title span {
    color: var(--muted);
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .inspector-head h3,
  .overview h4,
  .current-work h4,
  .section-heading h4,
  .failure-card h4 {
    margin: 3px 0 0;
  }
  .inspector-head button,
  .run-choice button,
  .page-button,
  .notice button,
  .atlas-impact button,
  .claim-meta button,
  .history button {
    min-height: 44px;
    border: 1px solid var(--line);
    background: transparent;
    color: inherit;
  }
  .content {
    display: grid;
    gap: 14px;
    min-width: 320px;
    padding: 14px;
  }
  .run-choice {
    display: grid;
    gap: 5px;
  }
  .run-choice label {
    font-size: 0.72rem;
    color: var(--muted);
  }
  .run-choice > div {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
  }
  .run-choice select {
    min-width: 0;
    min-height: 42px;
    border: 1px solid var(--line);
    background: var(--surface-canvas);
    color: inherit;
  }
  .overview,
  .current-work,
  .plan,
  .atlas-impact,
  .failure-card,
  .history,
  .notice {
    border: 1px solid var(--line);
    background: var(--surface-raised);
  }
  .overview,
  .current-work,
  .plan,
  .atlas-impact,
  .failure-card,
  .notice {
    padding: 12px;
  }
  .overview-title,
  .section-heading,
  .card-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .overview-title > strong {
    font-size: 0.72rem;
    color: var(--muted);
    text-align: right;
  }
  .progress {
    height: 5px;
    margin: 12px 0;
    background: var(--line);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .phases {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 5px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .phases li {
    display: grid;
    align-content: start;
    gap: 5px;
    min-height: 62px;
    padding: 7px;
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 0.66rem;
  }
  .phases li strong {
    overflow-wrap: anywhere;
  }
  .phases li.completed {
    color: var(--text);
    border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
  }
  .phases li.active {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .phases li.stopped {
    color: var(--color-status-failed);
  }
  .historical,
  .limited {
    margin: 10px 0 0;
    padding: 9px;
    border-left: 3px solid var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 8%, transparent);
    font-size: 0.74rem;
  }
  .historical p,
  .failure-card p,
  .current-work p,
  .notice p,
  .atlas-impact > p {
    margin: 6px 0;
  }
  .failure-card {
    border-color: var(--color-status-failed-ring);
  }
  .failure-card details {
    margin-top: 8px;
  }
  .current-work {
    border-left: 4px solid var(--accent);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 8px;
  }
  .chips span,
  .status,
  .claim-meta > span {
    padding: 3px 6px;
    border: 1px solid var(--line);
    font-size: 0.65rem;
  }
  .module-list {
    display: grid;
    gap: 7px;
    margin-top: 10px;
  }
  .module-card {
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .module-card.expanded {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  }
  .module-toggle {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 64px;
    padding: 9px;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .module-toggle h5 {
    margin: 5px 0 2px;
    font-size: 0.86rem;
  }
  .module-toggle small {
    color: var(--muted);
  }
  .status.published {
    border-color: var(--color-status-success);
    color: var(--color-status-success);
  }
  .status.incomplete {
    border-color: var(--color-warning);
    color: var(--color-warning);
  }
  .status.exploring,
  .status.verifying {
    border-color: var(--accent);
    color: var(--accent);
  }
  .module-body {
    display: grid;
    gap: 14px;
    padding: 12px;
    border-top: 1px solid var(--line);
  }
  .module-body h6,
  .card-detail h6 {
    margin: 0 0 8px;
    font-size: 0.78rem;
  }
  .step-list {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .step-list li {
    padding: 8px;
    border-left: 3px solid var(--line);
    background: var(--surface-raised);
  }
  .step-list li.exploring {
    border-color: var(--accent);
  }
  .step-list li.confirmed {
    border-color: var(--color-status-success);
  }
  .step-list li > div:first-child {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .step-list li > div span {
    color: var(--muted);
    font-size: 0.64rem;
  }
  .step-list li p {
    margin: 6px 0;
    font-size: 0.72rem;
  }
  .fields {
    display: grid;
    gap: 9px;
  }
  .fields > section {
    padding: 9px;
    border: 1px solid var(--line);
    background: var(--surface-raised);
  }
  .fields ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .fields li + li {
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .fields p {
    margin: 0;
    font-size: 0.76rem;
    line-height: 1.45;
  }
  .claim-meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 7px;
    color: var(--muted);
    font-size: 0.65rem;
  }
  .claim-meta .fact {
    color: var(--color-status-success);
  }
  .claim-meta .hypothesis {
    color: var(--color-warning);
  }
  .claim-meta button {
    min-height: 44px;
    margin-left: auto;
  }
  .inspector-head button {
    min-width: 44px;
  }
  .source-preview {
    margin-top: 8px;
    padding: 8px;
    border: 1px solid var(--line);
    background: var(--surface-canvas);
    font-size: 0.68rem;
  }
  .source-preview pre {
    max-height: 190px;
    margin: 7px 0 0;
    overflow: auto;
    white-space: pre-wrap;
  }
  .impact-summary {
    display: grid;
    grid-template-columns: 2fr 0.7fr 1.5fr;
    gap: 1px;
    margin-top: 10px;
    background: var(--line);
  }
  .impact-summary > div {
    display: grid;
    gap: 4px;
    padding: 9px;
    background: var(--surface);
  }
  .impact-summary span,
  .impact-list span {
    color: var(--muted);
    font-size: 0.65rem;
  }
  .impact-summary strong {
    font-size: 0.73rem;
  }
  .impact-list {
    display: grid;
    gap: 1px;
    margin: 8px 0 0;
    padding: 0;
    background: var(--line);
    list-style: none;
  }
  .impact-list li {
    display: grid;
    grid-template-columns: 62px 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 8px;
    background: var(--surface);
  }
  .impact-list strong {
    font-size: 0.72rem;
    overflow-wrap: anywhere;
  }
  .impact-list small {
    color: var(--muted);
    font-size: 0.63rem;
  }
  .history summary {
    min-height: 44px;
    padding: 12px;
    cursor: pointer;
  }
  .history ol {
    margin: 0;
    padding: 0 12px 12px;
    list-style: none;
  }
  .history li {
    display: grid;
    grid-template-columns: 92px 1fr;
    gap: 9px;
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }
  .history time {
    color: var(--muted);
    font-size: 0.65rem;
  }
  .history p {
    margin: 0;
    font-size: 0.72rem;
  }
  .history li.failed p {
    color: var(--color-status-failed);
  }
  .history button {
    margin: 0 12px 12px;
  }
  .notice.error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    color: var(--color-status-failed);
  }
  .loading {
    color: var(--muted);
    font-size: 0.72rem;
  }
  .page-button {
    width: 100%;
    margin-top: 8px;
  }
  button:focus-visible,
  select:focus-visible,
  summary:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  @media (max-width: 899px) {
    .inspector {
      position: absolute;
      z-index: 20;
      inset: 0 0 0 auto;
      box-shadow: -10px 0 30px color-mix(in srgb, var(--color-shadow) 28%, transparent);
    }
    .inspector.open {
      width: min(620px, 96vw);
    }
  }
  @media (max-width: 560px) {
    .phases {
      grid-template-columns: 1fr;
    }
    .phases li {
      grid-template-columns: 24px 1fr;
      min-height: 0;
    }
    .impact-summary {
      grid-template-columns: 1fr;
    }
    .impact-list li {
      grid-template-columns: 1fr;
    }
    .overview-title,
    .section-heading {
      display: grid;
    }
    .overview-title > strong {
      text-align: left;
    }
    .content {
      padding: 10px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .inspector {
      transition: none;
    }
  }
</style>

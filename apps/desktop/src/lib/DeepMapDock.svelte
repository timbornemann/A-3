<script lang="ts">
  import { onMount } from 'svelte';
  import {
    cancelDeepMap,
    pauseDeepMap,
    queryDeepMap,
    resumeDeepMap,
    startDeepMap,
    type DeepMapBudgetV1,
    type DeepMapControlResponseV1,
    type DeepMapFailureV1,
    type DeepMapStatusResponseV1,
  } from './deep-map';

  interface Props {
    canceller?: () => Promise<DeepMapControlResponseV1>;
    onpublished?: () => void;
    pauser?: () => Promise<DeepMapControlResponseV1>;
    resumer?: () => Promise<DeepMapControlResponseV1>;
    starter?: (budget: DeepMapBudgetV1) => Promise<DeepMapControlResponseV1>;
    statusLoader?: () => Promise<DeepMapStatusResponseV1>;
  }
  const {
    statusLoader = queryDeepMap,
    starter = startDeepMap,
    pauser = pauseDeepMap,
    resumer = resumeDeepMap,
    canceller = cancelDeepMap,
    onpublished = () => {},
  }: Props = $props();

  const presets = {
    fast: { tokenLimit: 8_000, timeLimitMillis: 60_000, toolCallLimit: 16 },
    standard: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
    thorough: { tokenLimit: 128_000, timeLimitMillis: 600_000, toolCallLimit: 256 },
  } as const;
  let status = $state<DeepMapStatusResponseV1['result'] | null>(null);
  let expanded = $state(false);
  let busy = $state(false);
  let failed = $state(false);
  let preset = $state<'fast' | 'standard' | 'thorough' | 'advanced'>('standard');
  let custom = $state<DeepMapBudgetV1>({ ...presets.standard });
  let publishedRun = $state(false);

  onMount(() => {
    let active = true;
    void load();
    const timer = window.setInterval(() => {
      if (active) void load(true);
    }, 1_500);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  });

  async function load(silent = false): Promise<void> {
    if (!silent) failed = false;
    try {
      status = (await statusLoader()).result;
      const published =
        status.status === 'available' &&
        status.activity.publicationSummary?.atomicallyPublished === true;
      if (published && !publishedRun) onpublished();
      publishedRun = published;
    } catch {
      if (!silent) failed = true;
    }
  }
  function budget(): DeepMapBudgetV1 {
    return preset === 'advanced' ? custom : { ...presets[preset] };
  }
  async function start(): Promise<void> {
    busy = true;
    try {
      await starter(budget());
      expanded = true;
      await load();
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }
  async function control(action: () => Promise<DeepMapControlResponseV1>): Promise<void> {
    busy = true;
    try {
      await action();
      await load();
    } catch {
      failed = true;
    } finally {
      busy = false;
    }
  }
  function phase(value: string | null): string {
    return value === null
      ? 'Noch nicht gestartet'
      : ({
          planning: 'Planung',
          exploring: 'Evidence erkunden',
          claiming: 'Claims erzeugen',
          verifying: 'Verifizieren',
          publishing: 'Atomar veröffentlichen',
        }[value] ?? value);
  }
  function action(value: string | null): string {
    return value === null
      ? '–'
      : ({
          buildPlan: 'Plan erstellen',
          inspect: 'Evidence lesen',
          search: 'Index durchsuchen',
          propose: 'Schritt bestätigen',
          generateClaims: 'Claims strukturieren',
          verifyEvidence: 'Evidence prüfen',
          publishCards: 'Cards publizieren',
        }[value] ?? value);
  }
  function stateLabel(value: string): string {
    return (
      {
        idle: 'Bereit',
        queued: 'Eingeplant',
        running: 'Mapping läuft',
        pausing: 'Wird pausiert',
        paused: 'Pausiert',
        cancelling: 'Wird abgebrochen',
        succeeded: 'Veröffentlicht',
        failed: 'Fehlgeschlagen',
        cancelled: 'Abgebrochen',
      }[value] ?? value
    );
  }
  function providerLabel(providerId: string): string {
    return (
      {
        gemini: 'Google Gemini',
        ollama: 'Ollama',
        openai: 'OpenAI',
      }[providerId] ?? providerId
    );
  }
  function failureDetail(
    failure: DeepMapFailureV1,
    providerId: string,
    modelId: string,
  ): { explanation: string; recovery: string; title: string } {
    const provider = providerLabel(providerId);
    const details: Record<
      DeepMapFailureV1,
      { explanation: string; recovery: string; title: string }
    > = {
      noPublishedIndex: {
        title: 'Kein veröffentlichter Index',
        explanation:
          'Deep Map konnte keinen aktuellen, atomar veröffentlichten Projektstand laden.',
        recovery:
          'Warte den Indexlauf ab oder erstelle die Code-Analyse in den Projektoptionen neu.',
      },
      staleSnapshot: {
        title: 'Projektstand hat sich geändert',
        explanation:
          'Der Lauf war an einen älteren Snapshot gebunden und wurde vor weiterer Modellarbeit sicher beendet.',
        recovery:
          'Starte Deep Map erneut; der neue Lauf bindet sich an den aktuellen veröffentlichten Snapshot.',
      },
      planning: {
        title: 'Mapping-Plan konnte nicht erstellt werden',
        explanation:
          'Aus dem aktuellen Index ließ sich innerhalb der festen Grenzen kein gültiger Deep-Map-Plan ableiten.',
        recovery: 'Prüfe den Indexstatus und starte Deep Map danach erneut.',
      },
      modelUnavailable: {
        title: `${provider} ist nicht erreichbar`,
        explanation: `Die Verbindung zu ${provider} für „${modelId}“ endete auch nach dem begrenzten sicheren Wiederholungsversuch. Teilantworten wurden verworfen.`,
        recovery:
          'Prüfe Internetverbindung, Providerstatus und Zugangsdaten und versuche es anschließend erneut.',
      },
      modelRejected: {
        title: `${provider} hat die strukturierte Anfrage abgelehnt`,
        explanation: `Das Modell „${modelId}“ hat den begrenzten Strict-Structured-Output-Request nicht akzeptiert. Es wurde keine Modellantwort als Evidence übernommen.`,
        recovery:
          'Aktualisiere die Modellliste und verifiziere die Mapping-Capability erneut. Wähle bei wiederholter Ablehnung ein anderes GPT-Modell.',
      },
      modelTimedOut: {
        title: 'Die Modellantwort hat zu lange gedauert',
        explanation: `Der vollständige Request an ${provider} überschritt seine feste Deadline; eine unvollständige Antwort wurde nicht verwendet.`,
        recovery:
          'Versuche es erneut oder wähle ein kleineres beziehungsweise schnelleres Mapping-Modell.',
      },
      invalidModelResponse: {
        title: 'Die Modellantwort war nicht verwendbar',
        explanation: `${provider} lieferte keine vollständig abgeschlossene Antwort, die den gebundenen Structured-Output- und Evidence-Regeln entsprach.`,
        recovery: 'Verifiziere die Mapping-Capability erneut oder verwende ein anderes Modell.',
      },
      read: {
        title: 'Repository-Evidence konnte nicht gelesen werden',
        explanation:
          'Ein begrenzter, snapshotgebundener Read aus dem veröffentlichten Index schlug fehl.',
        recovery: 'Erstelle die Code-Analyse neu und starte Deep Map anschließend erneut.',
      },
      verification: {
        title: 'Claims konnten nicht verifiziert werden',
        explanation:
          'Mindestens ein vorgeschlagener Claim ließ sich nicht gegen aktuelle Repository-Evidence bestätigen.',
        recovery:
          'Starte einen neuen Lauf auf dem aktuellen Index; unverifizierte Claims wurden nicht publiziert.',
      },
      publication: {
        title: 'Module Cards konnten nicht veröffentlicht werden',
        explanation:
          'Die atomare Veröffentlichung der verifizierten Mapping-Ergebnisse wurde nicht abgeschlossen.',
        recovery:
          'Starte Deep Map erneut. Vorherige veröffentlichte Ergebnisse bleiben unverändert nutzbar.',
      },
      invalidCheckpoint: {
        title: 'Pause-Checkpoint ist nicht mehr gültig',
        explanation:
          'Der gespeicherte Checkpoint passt nicht mehr zu Plan, Budget oder aktuellem Projektstand.',
        recovery: 'Beginne einen neuen Deep-Map-Lauf statt den alten Checkpoint fortzusetzen.',
      },
      progressUnavailable: {
        title: 'Laufstatus ist widersprüchlich',
        explanation:
          'Scheduler und Deep-Map-Lifecycle konnten keinen gemeinsam bestätigten Fortschrittsstand herstellen.',
        recovery:
          'Starte Deep Map erneut. Falls der Zustand wiederkehrt, starte A^3 neu und versuche es nochmals.',
      },
    };
    return details[failure];
  }
</script>

<section class:expanded class="deep-map-dock" aria-labelledby="deep-map-title">
  <button
    class="summary"
    type="button"
    aria-expanded={expanded}
    onclick={() => (expanded = !expanded)}
  >
    <span class="mark" aria-hidden="true">✦</span><span
      ><strong id="deep-map-title">Deep Map</strong><small
        >Verifiziertes Repository-Verständnis</small
      ></span
    >
    {#if status?.status === 'available'}
      <b class={`state ${status.activity.state}`}>{stateLabel(status.activity.state)}</b>
      <span class="progress"
        ><i
          ><em
            style={`width:${status.activity.totalSteps === '0' ? 0 : Math.min(100, (Number(status.activity.confirmedSteps) / Number(status.activity.totalSteps)) * 100)}%`}
          ></em></i
        ><small>{status.activity.confirmedSteps}/{status.activity.totalSteps} bestätigt</small
        ></span
      >
    {/if}
    <span aria-hidden="true">{expanded ? '⌄' : '⌃'}</span>
  </button>
  {#if expanded}
    <div class="content">
      {#if failed}<p role="alert">Deep Map konnte nicht sicher gelesen oder gesteuert werden.</p>
      {:else if status?.status === 'unavailable'}<p>
          Kein verifiziertes lokales Mapping-Modell ist verfügbar.
        </p>
      {:else if status?.status === 'available'}
        <div class="presets" role="radiogroup" aria-label="Mapping-Budget">
          {#each [['fast', 'Schnell'], ['standard', 'Standard'], ['thorough', 'Gründlich'], ['advanced', 'Erweitert']] as option (option[0])}
            <button
              type="button"
              role="radio"
              aria-checked={preset === option[0]}
              class:active={preset === option[0]}
              onclick={() => (preset = option[0] as typeof preset)}
              ><strong>{option[1]}</strong>{#if option[0] !== 'advanced'}{@const value =
                  presets[option[0] as keyof typeof presets]}<span
                  >{value.tokenLimit.toLocaleString('de-DE')} Tokens</span
                ><small>{value.timeLimitMillis / 1000} s · {value.toolCallLimit} Reads</small
                >{:else}<span>Eigene Grenzen</span><small>Validierte Min-/Max-Werte</small
                >{/if}</button
            >
          {/each}
        </div>
        {#if preset === 'advanced'}
          <div class="advanced">
            <label
              >Tokens <input
                type="number"
                min={status.configuration.minimumBudget.tokenLimit}
                max={status.configuration.maximumBudget.tokenLimit}
                bind:value={custom.tokenLimit}
              /></label
            ><label
              >Sekunden <input
                type="number"
                min="1"
                max="86400"
                value={custom.timeLimitMillis / 1000}
                onchange={(event) =>
                  (custom.timeLimitMillis = Number(event.currentTarget.value) * 1000)}
              /></label
            ><label
              >Reads <input
                type="number"
                min={status.configuration.minimumBudget.toolCallLimit}
                max={status.configuration.maximumBudget.toolCallLimit}
                bind:value={custom.toolCallLimit}
              /></label
            >
          </div>
        {/if}
        <div class="run">
          <dl>
            <div>
              <dt>Phase</dt>
              <dd>{phase(status.activity.phase)}</dd>
            </div>
            <div>
              <dt>Aktion</dt>
              <dd>{action(status.activity.safeAction)}</dd>
            </div>
            <div>
              <dt>Zielart</dt>
              <dd>{status.activity.targetKind ?? 'Projekt'}</dd>
            </div>
            <div>
              <dt>Modul</dt>
              <dd>{status.activity.currentModuleId?.slice(0, 12) ?? 'Gesamtprojekt'}</dd>
            </div>
            <div>
              <dt>Planbudget</dt>
              <dd>
                {status.activity.budget?.tokenLimit.toLocaleString('de-DE') ??
                  budget().tokenLimit.toLocaleString('de-DE')} Tokens
              </dd>
            </div>
            <div>
              <dt>Schritt</dt>
              <dd>{status.activity.stepPosition ?? '–'} / {status.activity.totalSteps}</dd>
            </div>
          </dl>
          <div class="actions">
            {#if ['idle', 'succeeded', 'failed', 'cancelled'].includes(status.activity.state)}<button
                class="primary"
                type="button"
                disabled={busy}
                onclick={start}>Deep Map starten</button
              >{/if}
            {#if status.activity.state === 'running'}<button
                type="button"
                disabled={busy}
                onclick={() => control(pauser)}>Pausieren</button
              >{/if}
            {#if status.activity.state === 'paused'}<button
                type="button"
                disabled={busy}
                onclick={() => control(resumer)}>Fortsetzen</button
              >{/if}
            {#if ['queued', 'running', 'pausing', 'paused'].includes(status.activity.state)}<button
                type="button"
                disabled={busy}
                onclick={() => control(canceller)}>Abbrechen</button
              >{/if}
          </div>
        </div>
        {#if status.activity.state === 'failed' && status.activity.failure !== null}
          {@const detail = failureDetail(
            status.activity.failure,
            status.configuration.model.providerId,
            status.configuration.model.modelId,
          )}
          <section class="failure-detail" role="alert" aria-labelledby="deep-map-failure-title">
            <div>
              <span aria-hidden="true">!</span>
              <h3 id="deep-map-failure-title">{detail.title}</h3>
            </div>
            <p>{detail.explanation}</p>
            <p><strong>Nächster Schritt:</strong> {detail.recovery}</p>
            <small>Diagnosecode <code>{status.activity.failure}</code></small>
          </section>
        {/if}
        {#if status.activity.events.length > 0}
          <ol class="feed" aria-label="Aktuelle sichere Deep-Map-Ereignisse">
            {#each status.activity.events.slice().reverse() as event (event.sequence)}<li
                class:confirmed={event.confirmed}
              >
                <span>{event.sequence}</span>
                <div>
                  <strong>{phase(event.phase)}</strong><small
                    >{action(event.safeAction)} · {event.targetKind}{event.stepPosition === null
                      ? ''
                      : ` · ${event.stepPosition}/${event.totalSteps}`}</small
                  >
                </div>
              </li>{/each}
          </ol>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .deep-map-dock {
    position: relative;
    z-index: 30;
    flex: 0 0 auto;
    border-top: 1px solid var(--line);
    background: var(--surface);
  }
  .summary {
    display: grid;
    grid-template-columns: 44px minmax(170px, auto) auto 1fr 24px;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 58px;
    padding: 7px 14px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }
  .summary > span:nth-child(2) {
    display: grid;
  }
  .summary small {
    color: var(--muted);
  }
  .mark {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    background: color-mix(in srgb, var(--accent) 25%, transparent);
    color: var(--color-accent-text);
  }
  .state {
    padding: 5px 9px;
    border: 1px solid var(--line);
    font-size: 0.68rem;
  }
  .progress {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 9px;
  }
  .progress i {
    height: 4px;
    background: var(--line);
  }
  .progress em {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .content {
    display: grid;
    grid-template-columns: minmax(420px, 1fr) minmax(300px, 0.8fr);
    gap: 14px;
    max-height: 360px;
    padding: 14px;
    border-top: 1px solid var(--line);
    overflow: auto;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(4, minmax(120px, 1fr));
    gap: 5px;
    grid-column: 1 / -1;
  }
  .presets button {
    display: grid;
    min-height: 74px;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-raised);
    color: inherit;
    text-align: left;
  }
  .presets button.active {
    outline: 3px solid var(--focus);
    outline-offset: -3px;
  }
  .presets span,
  .presets small {
    color: var(--muted);
  }
  .advanced {
    display: flex;
    gap: 8px;
    grid-column: 1 / -1;
  }
  .advanced label {
    display: grid;
    gap: 4px;
    font-size: 0.72rem;
  }
  .advanced input {
    min-height: 36px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-canvas);
    color: inherit;
  }
  .run dl {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1px;
    margin: 0;
    background: var(--line);
  }
  .run dl div {
    padding: 8px;
    background: var(--surface-raised);
  }
  .run dt {
    color: var(--muted);
    font-size: 0.65rem;
  }
  .run dd {
    margin: 3px 0;
    font-size: 0.75rem;
    font-weight: 700;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 9px;
  }
  .actions button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
    color: inherit;
  }
  .actions .primary {
    background: var(--accent);
    color: var(--color-on-accent);
    font-weight: 750;
  }
  .failure-detail {
    display: grid;
    gap: 8px;
    grid-column: 1 / -1;
    padding: 12px;
    border: 1px solid var(--color-status-failed-ring);
    background: color-mix(in srgb, var(--color-status-failed) 8%, var(--surface-raised));
  }
  .failure-detail > div {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .failure-detail > div > span {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--color-status-failed-ring);
    color: var(--color-status-failed);
    font-weight: 800;
  }
  .failure-detail h3,
  .failure-detail p {
    margin: 0;
  }
  .failure-detail h3 {
    font-size: 0.9rem;
  }
  .failure-detail p,
  .failure-detail small {
    font-size: 0.75rem;
  }
  .failure-detail small {
    color: var(--muted);
  }
  .feed {
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: auto;
  }
  .feed li {
    display: grid;
    grid-template-columns: 32px 1fr;
    gap: 7px;
    padding: 7px;
    border-bottom: 1px solid var(--line);
  }
  .feed li > span {
    color: var(--muted);
  }
  .feed div {
    display: grid;
  }
  .feed small {
    color: var(--muted);
  }
  @media (max-width: 899px) {
    .content {
      position: absolute;
      right: 0;
      bottom: 58px;
      left: 0;
      grid-template-columns: 1fr;
      max-height: min(70vh, 520px);
      background: var(--surface);
      box-shadow: 0 -10px 30px color-mix(in srgb, var(--color-shadow) 30%, transparent);
    }
    .presets {
      grid-template-columns: 1fr 1fr;
    }
    .summary {
      grid-template-columns: 38px 1fr auto 20px;
    }
    .progress {
      display: none;
    }
  }
  @media (max-width: 700px) {
    .summary > span:nth-child(2) small {
      display: none;
    }
    .run dl {
      grid-template-columns: 1fr 1fr;
    }
  }
</style>

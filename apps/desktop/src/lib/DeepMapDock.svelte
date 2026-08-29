<script lang="ts">
  import { onMount } from 'svelte';
  import {
    cancelDeepMap,
    pauseDeepMap,
    queryDeepMap,
    resumeDeepMap,
    startDeepMap,
    type DeepMapControlResponseV1,
    type DeepMapLifecycleV3,
    type DeepMapModeV2,
    type DeepMapStartResponseV2,
    type DeepMapStatusResponseV3,
  } from './deep-map';

  interface Props {
    canceller?: () => Promise<DeepMapControlResponseV1>;
    ondetails?: (focusFailure: boolean) => void;
    onpublished?: () => void;
    pauser?: () => Promise<DeepMapControlResponseV1>;
    resumer?: () => Promise<DeepMapControlResponseV1>;
    starter?: (mode: DeepMapModeV2) => Promise<DeepMapStartResponseV2>;
    statusLoader?: () => Promise<DeepMapStatusResponseV3>;
  }

  const {
    statusLoader = queryDeepMap,
    starter = startDeepMap,
    pauser = pauseDeepMap,
    resumer = resumeDeepMap,
    canceller = cancelDeepMap,
    ondetails = () => {},
    onpublished = () => {},
  }: Props = $props();

  let result = $state<DeepMapStatusResponseV3['result'] | null>(null);
  let mode = $state<DeepMapModeV2>('standard');
  let busy = $state(false);
  let readFailed = $state(false);
  let wasCurrent = $state(false);

  const lifecycle = $derived(
    result?.status === 'available' ? result.lifecycle : ({ state: 'ready' } as const),
  );
  const current = $derived(lifecycle.state === 'current');
  const active = $derived(
    ['queued', 'running', 'pausing', 'paused', 'cancelling'].includes(lifecycle.state),
  );
  const progress = $derived('progress' in lifecycle ? lifecycle.progress : null);
  const progressPercent = $derived.by(() => {
    if (progress === null || progress.totalSteps === '0') return 0;
    return Math.min(100, (Number(progress.confirmedSteps) / Number(progress.totalSteps)) * 100);
  });
  const primary = $derived.by(mainAction);

  onMount(() => {
    let mounted = true;
    void load();
    const timer = window.setInterval(() => {
      if (mounted) void load(true);
    }, 1_500);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  });

  async function load(silent = false): Promise<void> {
    if (!silent) readFailed = false;
    try {
      result = (await statusLoader()).result;
      const isCurrent = result.status === 'available' && result.lifecycle.state === 'current';
      if (isCurrent && !wasCurrent) onpublished();
      wasCurrent = isCurrent;
    } catch {
      if (!silent) readFailed = true;
    }
  }

  async function start(): Promise<void> {
    busy = true;
    readFailed = false;
    try {
      await starter(mode);
      await load();
    } catch {
      readFailed = true;
    } finally {
      busy = false;
    }
  }

  async function control(action: () => Promise<DeepMapControlResponseV1>): Promise<void> {
    busy = true;
    readFailed = false;
    try {
      await action();
      await load();
    } catch {
      readFailed = true;
    } finally {
      busy = false;
    }
  }

  function mainAction(): { disabled: boolean; label: string; run: () => Promise<void> } {
    if (current) return { disabled: true, label: '✓ Aktuell', run: start };
    if (lifecycle.state === 'running') {
      return { disabled: busy, label: 'Pause', run: () => control(pauser) };
    }
    if (lifecycle.state === 'paused') {
      return { disabled: busy, label: 'Fortsetzen', run: () => control(resumer) };
    }
    if (['queued', 'pausing', 'cancelling'].includes(lifecycle.state)) {
      return { disabled: true, label: stateLabel(lifecycle.state), run: start };
    }
    return {
      disabled: busy || result?.status !== 'available',
      label: 'Start',
      run: start,
    };
  }

  function stateLabel(state: DeepMapLifecycleV3['state']): string {
    return (
      {
        ready: 'Bereit',
        current: 'Aktuell',
        queued: 'Eingeplant',
        running: 'Läuft',
        pausing: 'Pausiert gleich',
        paused: 'Pausiert',
        cancelling: 'Bricht ab',
        succeeded: 'Abgeschlossen',
        failed: 'Fehlgeschlagen',
        cancelled: 'Abgebrochen',
      }[state] ?? state
    );
  }

  function shortStatus(): string {
    if (readFailed) return 'Status nicht verfügbar';
    if (result?.status === 'unavailable') return 'Kein Mapping-Modell';
    if (result?.status === 'noProject') return 'Kein Projekt';
    if (lifecycle.state === 'current') return `${lifecycle.cardCount} Cards · aktuell`;
    if (progress === null) return stateLabel(lifecycle.state);
    const action =
      progress.action === null
        ? stateLabel(lifecycle.state)
        : ({
            buildPlan: 'Plan erstellen',
            inspect: 'Evidence lesen',
            search: 'Index durchsuchen',
            propose: 'Schritt bestätigen',
            generateClaims: 'Claims erzeugen',
            verifyEvidence: 'Evidence prüfen',
            publishCards: 'Cards publizieren',
          }[progress.action] ?? progress.action);
    return `${progress.confirmedSteps}/${progress.totalSteps} · ${action}`;
  }
</script>

<section class="deep-map-bar" aria-labelledby="deep-map-title">
  <div class="brand">
    <span aria-hidden="true">✦</span>
    <strong id="deep-map-title">Deep Map</strong>
  </div>
  <label>
    <span class="sr-only">Deep-Map-Modus</span>
    <select bind:value={mode} disabled={busy || active || current} aria-label="Deep-Map-Modus">
      <option value="fast">Schnell</option>
      <option value="standard">Standard</option>
      <option value="thorough">Gründlich</option>
    </select>
  </label>
  <button class="primary" type="button" disabled={primary.disabled} onclick={primary.run}
    >{primary.label}</button
  >
  {#if active}
    <button
      class="cancel"
      type="button"
      disabled={busy || lifecycle.state === 'cancelling'}
      aria-label="Deep Map abbrechen"
      title="Abbrechen"
      onclick={() => control(canceller)}>×</button
    >
  {/if}
  <div class="status" role="status" aria-live="polite">
    {#if lifecycle.state === 'failed'}
      <button type="button" class="failure" onclick={() => ondetails(true)}>{shortStatus()}</button>
    {:else}
      <span>{shortStatus()}</span>
    {/if}
    <i aria-hidden="true"><em style={`width:${progressPercent}%`}></em></i>
  </div>
  <button class="details" type="button" aria-label="Details" onclick={() => ondetails(false)}
    >Details</button
  >
</section>

<style>
  .deep-map-bar {
    position: relative;
    z-index: 30;
    display: flex;
    flex: 0 0 52px;
    align-items: center;
    gap: 8px;
    min-width: 0;
    height: 52px;
    padding: 4px 10px;
    border-top: 1px solid var(--line);
    background: var(--surface);
  }
  .brand {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 7px;
    white-space: nowrap;
  }
  .brand > span {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--color-accent-text);
  }
  select,
  button {
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-raised);
    color: inherit;
  }
  select {
    min-width: 118px;
    padding: 0 30px 0 10px;
  }
  button {
    padding: 0 12px;
  }
  .primary {
    min-width: 88px;
    background: var(--accent);
    color: var(--color-on-accent);
    font-weight: 750;
  }
  .cancel {
    flex: 0 0 44px;
    width: 44px;
    padding: 0;
    color: var(--color-status-failed);
    font-size: 1.25rem;
  }
  .status {
    position: relative;
    display: grid;
    flex: 1 1 auto;
    align-items: center;
    min-width: 90px;
    height: 44px;
    color: var(--muted);
    font-size: 0.72rem;
  }
  .status > span,
  .failure {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status i {
    position: absolute;
    right: 0;
    bottom: 2px;
    left: 0;
    height: 2px;
    overflow: hidden;
    background: var(--line);
  }
  .status em {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .failure {
    justify-self: start;
    width: 100%;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-status-failed);
    text-align: left;
    text-decoration: underline;
  }
  .details {
    flex: 0 0 auto;
    background: transparent;
  }
  select:focus-visible,
  button:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: 1px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  @media (max-width: 720px) {
    .deep-map-bar {
      gap: 5px;
      padding-inline: 6px;
    }
    .brand strong {
      display: none;
    }
    .brand > span {
      width: 28px;
    }
    select {
      min-width: 102px;
      max-width: 110px;
    }
    .details {
      width: 44px;
      padding: 0;
      overflow: hidden;
      color: transparent;
    }
    .details::after {
      color: var(--text);
      content: '⋯';
      font-size: 1.2rem;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .status em {
      transition: none;
    }
  }
</style>

<script lang="ts">
  import { onDestroy } from 'svelte';
  import { parseCommandErrorV1 } from './command-error';
  import {
    querySettingsRecovery,
    recoverInvalidModelProfiles,
    type SettingsRecoveryV1,
  } from './settings-recovery';

  interface Props {
    diagnose?: typeof querySettingsRecovery;
    recover?: typeof recoverInvalidModelProfiles;
    onrecovered: () => void;
  }
  let {
    diagnose = querySettingsRecovery,
    recover = recoverInvalidModelProfiles,
    onrecovered,
  }: Props = $props();
  let recoveryState = $state<'idle' | 'checking' | 'diagnosed' | 'recovering' | 'error'>('idle');
  let diagnosis = $state<SettingsRecoveryV1 | null>(null);
  let message = $state('');
  let destroyed = false;
  const labels = {
    coding: 'Agent / Coding',
    mapping: 'Mapping',
    embedding: 'Semantische Suche / Embedding',
  };
  onDestroy(() => {
    destroyed = true;
  });

  async function check() {
    if (recoveryState === 'checking' || recoveryState === 'recovering') return;
    recoveryState = 'checking';
    diagnosis = null;
    try {
      const result = await diagnose();
      if (destroyed) return;
      diagnosis = result;
      recoveryState = 'diagnosed';
    } catch {
      if (destroyed) return;
      recoveryState = 'error';
      message =
        'Die Modellprofile lassen sich nicht sicher isolieren. Es wurde nichts zurückgesetzt. Beende A^3 und sichere den App-Datenordner, bevor du den Support zur Katalog-Wiederherstellung kontaktierst. Ein erneuter Prüfversuch bleibt möglich.';
    }
  }

  async function confirm() {
    if (recoveryState !== 'diagnosed' || !diagnosis?.invalidProfiles.length) return;
    recoveryState = 'recovering';
    try {
      await recover(diagnosis.settingsRevision);
      if (!destroyed) onrecovered();
    } catch (error) {
      if (destroyed) return;
      recoveryState = 'error';
      diagnosis = null;
      message =
        parseCommandErrorV1(error)?.code === 'invalidSettingsRequest'
          ? 'Die Einstellungen haben sich inzwischen geändert. Prüfe die aktuelle Konfiguration erneut; die alte Diagnose wird nicht angewendet.'
          : 'Die Wiederherstellung konnte nicht bestätigt werden. Prüfe den aktuellen Stand erneut. Projektwissen und Schlüssel werden durch diese Aktion nicht gelöscht.';
    }
  }
</script>

<section aria-label="Modell-Einstellungen wiederherstellen" class="settings-recovery">
  <h3>Modell-Einstellungen wiederherstellen</h3>
  <p>
    Prüfe lokal, ob einzelne Modellzuordnungen ungültig sind. Dabei wird kein Modell angefragt und
    noch nichts geändert.
  </p>
  {#if recoveryState === 'checking' || recoveryState === 'recovering'}
    <p role="status">
      {recoveryState === 'checking'
        ? 'Modellkonfiguration wird geprüft …'
        : 'Ungültige Zuordnungen werden deaktiviert …'}
    </p>
  {:else if recoveryState === 'diagnosed' && diagnosis}
    {#if diagnosis.invalidProfiles.length > 0}
      <p>Betroffene Rollen: {diagnosis.invalidProfiles.map((role) => labels[role]).join(', ')}.</p>
      <p>
        Nur diese ungültigen Zuordnungen werden deaktiviert. Gültige Modelle, Anbieter,
        API-Schlüssel und Projektwissen bleiben erhalten. Der bisherige Stand bleibt gespeichert.
        Wähle und verifiziere die betroffenen Modelle anschließend erneut.
      </p>
      <button type="button" onclick={confirm}>Ungültige Zuordnungen jetzt deaktivieren</button>
      <button
        type="button"
        onclick={() => {
          diagnosis = null;
          recoveryState = 'idle';
        }}>Abbrechen</button
      >
    {:else}
      <p role="status">
        Keine ungültigen Modellprofile gefunden. Eine Rücksetzung ist nicht erforderlich.
      </p>
      <button type="button" onclick={onrecovered}>Einstellungen erneut laden</button>
    {/if}
  {:else}
    {#if recoveryState === 'error'}<p role="alert">{message}</p>{/if}
    <button type="button" onclick={check}>Modellkonfiguration prüfen</button>
  {/if}
</section>

<style>
  .settings-recovery {
    max-width: 46rem;
    margin-block-start: 1rem;
    padding-block-start: 1rem;
    border-top: 1px solid var(--border-color, currentColor);
  }
  p {
    line-height: 1.5;
  }
  button {
    margin: 0.3rem 0.5rem 0.3rem 0;
  }
</style>

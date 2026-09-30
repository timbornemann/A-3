<script lang="ts">
  import {
    queryMachineRecovery,
    recoverMachineEffect,
    type MachineRecoveryScope,
  } from './agent-machine-recovery';
  let {
    taskId,
    refreshKey,
    onqueued = () => {},
    loader = queryMachineRecovery,
    recoverer = recoverMachineEffect,
  }: {
    taskId: string;
    refreshKey: unknown;
    onqueued?: () => void;
    loader?: typeof queryMachineRecovery;
    recoverer?: typeof recoverMachineEffect;
  } = $props();
  let scope = $state<MachineRecoveryScope | null>(null);
  let operation = $state<{ task: string; scope: string } | null>(null);
  let busy = $derived(operation?.task === taskId);
  let message = $state('');
  $effect(() => {
    const task = taskId;
    void refreshKey;
    let active = true;
    scope = null;
    message = '';
    void loader(task)
      .then((result) => {
        if (active) scope = result.status === 'available' ? result.recovery : null;
      })
      .catch(() => {
        if (active) scope = null;
      });
    return () => {
      active = false;
    };
  });
  async function recover(): Promise<void> {
    if (!scope || busy) return;
    const selected = scope;
    const task = taskId;
    const pending = { task, scope: selected.scope };
    operation = pending;
    try {
      const result = await recoverer(task, selected);
      if (task !== taskId || scope?.scope !== selected.scope) return;
      message =
        result.status === 'queued'
          ? 'Der aktuelle Zustand wird geprüft und die Aufgabe neu geplant.'
          : 'Der Umfang hat sich geändert. Lade den aktuellen Stand erneut.';
      if (result.status === 'queued') {
        scope = null;
        onqueued();
      }
    } catch {
      if (task === taskId)
        message =
          'Die Wiederherstellung konnte nicht gestartet werden. Die Wirkung bleibt ungeklärt.';
    } finally {
      if (operation?.task === pending.task && operation.scope === pending.scope) operation = null;
    }
  }
</script>

{#if scope}
  <section class="machine-recovery" aria-label="Ungeklärte Maschinenwirkung">
    <strong>Die Wirkung dieser Aktion ist ungeklärt</strong>
    <code>{scope.target}</code>
    <small>Aktionsumfang: <code>{scope.scope}</code></small>
    <p>Die ursprüngliche Aktion wird nicht wiederholt und gilt weiterhin als Unknown.</p>
    {#if scope.resourceKind === 'file'}
      <p>
        Mit deiner Auswahl erlaubst du das Lesen genau dieser Datei. Der aktuelle Hash oder ihre
        Abwesenheit wird geprüft; danach wird neu geplant.
      </p>
    {:else}
      <p>
        Weitere Datei-, Netzwerk- oder Veröffentlichungswirkungen können nicht vollständig geprüft
        werden. Mit deiner Auswahl bestätigst du diese Unsicherheit für den angezeigten Umfang und
        verlangst eine neue Planung.
      </p>
    {/if}
    <button type="button" disabled={busy} onclick={recover}
      >{scope.resourceKind === 'file'
        ? 'Dateizustand lesen und neu planen'
        : 'Ungeklärte Wirkung bestätigen und neu planen'}</button
    >
    <p>Du kannst die Aufgabe auch über die vorhandenen Laufsteuerungen abbrechen.</p>
  </section>
{/if}
{#if message}<p role="status">{message}</p>{/if}

<style>
  .machine-recovery {
    display: grid;
    gap: 0.5rem;
    border: 1px solid var(--color-border);
    padding: 1rem;
    border-radius: 0.75rem;
  }
  code {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  p {
    margin: 0;
  }
  button {
    min-height: 44px;
    white-space: normal;
  }
</style>

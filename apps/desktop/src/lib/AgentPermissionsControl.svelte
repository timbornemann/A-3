<script lang="ts">
  import { onMount } from 'svelte';
  import {
    queryAgentPermissions,
    updateAgentPermissions,
    subscribeAgentPermissions,
    type AgentPermissionMode,
    type AgentPermissions,
  } from './agent-permissions';

  let {
    loader = queryAgentPermissions,
    updater = updateAgentPermissions,
    subscriber = subscribeAgentPermissions,
  }: {
    loader?: () => Promise<AgentPermissions>;
    updater?: (settings: AgentPermissions, mode: AgentPermissionMode) => Promise<AgentPermissions>;
    subscriber?: (receive: (settings: AgentPermissions) => void) => Promise<() => void>;
  } = $props();
  let settings = $state<AgentPermissions | null>(null);
  let saving = $state(false);
  let error = $state(false);
  let alive = true;

  function receive(next: AgentPermissions): void {
    if (alive && (!settings || BigInt(next.revision) > BigInt(settings.revision))) settings = next;
  }
  async function refresh(): Promise<void> {
    try {
      receive(await loader());
      if (alive) error = false;
    } catch {
      if (alive) error = true;
    }
  }
  async function select(mode: AgentPermissionMode): Promise<void> {
    if (!settings || saving || settings.mode === mode) return;
    saving = true;
    error = false;
    try {
      receive(await updater(settings, mode));
    } catch {
      await refresh();
      if (alive) error = true;
    } finally {
      if (alive) saving = false;
    }
  }
  onMount(() => {
    let unsubscribe: (() => void) | undefined;
    void subscriber(receive)
      .then((cleanup) => {
        if (alive) {
          unsubscribe = cleanup;
          // Subscribe before reading so a mode change cannot fall between both operations.
          void refresh();
        } else cleanup();
      })
      .catch(() => {
        if (alive) void refresh();
      });
    return () => {
      alive = false;
      unsubscribe?.();
    };
  });
</script>

<div class="permission-control">
  <div class="permission-options" role="group" aria-label="Appweite Agent-Berechtigungen">
    <button
      type="button"
      aria-pressed={settings?.mode === 'askPermissions'}
      disabled={!settings || saving}
      onclick={() => void select('askPermissions')}>Ask permissions</button
    >
    <button
      type="button"
      aria-pressed={settings?.mode === 'fullMachine'}
      disabled={!settings || saving}
      onclick={() => void select('fullMachine')}>Full machine</button
    >
  </div>
  <span class="permission-hint"
    >Appweit · {settings?.mode === 'fullMachine'
      ? 'Unklare Aktionen, Löschen und Veröffentlichung brauchen Freigabe.'
      : 'Änderungen und Prozesse brauchen Freigabe.'}</span
  >
  {#if error}
    <span role="alert">Berechtigungen konnten nicht aktualisiert werden.</span>
    <button type="button" onclick={() => void refresh()}>Erneut laden</button>
  {/if}
</div>

<style>
  .permission-control {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    padding: 0.4rem 0;
  }
  .permission-options {
    display: flex;
    gap: 0.25rem;
  }
  button {
    min-height: 44px;
    padding: 0.4rem 0.65rem;
    border: 1px solid var(--color-border-soft);
    border-radius: 0.5rem;
    color: inherit;
    background: transparent;
    cursor: pointer;
  }
  button[aria-pressed='true'] {
    background: var(--color-surface-muted);
    border-color: currentColor;
  }
  button:focus-visible {
    outline: 2px solid currentColor;
    outline-offset: 2px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .permission-hint {
    font-size: 0.75rem;
    opacity: 0.75;
  }
</style>

<script lang="ts">
  import AgentPermissionsControl from '../src/lib/AgentPermissionsControl.svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import AgentMachineRecovery from '../src/lib/AgentMachineRecovery.svelte';
  import type { AgentPermissionMode, AgentPermissions } from '../src/lib/agent-permissions';
  let current: AgentPermissions = { protocolVersion: 1, revision: '1', mode: 'askPermissions' };
  const listeners = new SvelteSet<(settings: AgentPermissions) => void>();
  async function update(previous: AgentPermissions, mode: AgentPermissionMode) {
    if (previous.revision !== current.revision) throw new Error('Fixture CAS conflict');
    current = { ...current, revision: String(BigInt(current.revision) + 1n), mode };
    listeners.forEach((receive) => receive(current));
    return current;
  }
  async function subscribe(receive: (settings: AgentPermissions) => void) {
    listeners.add(receive);
    return () => {
      listeners.delete(receive);
    };
  }
</script>

<main>
  <h1>A^3 · Berechtigungen</h1>
  <p>Offline-Testfixture: synthetische Einstellungen, keine Werkzeugausführung.</p>
  <div class="panels">
    {#each ['light', 'dark'] as theme (theme)}
      <section
        data-permission-theme
        data-theme={theme}
        aria-label={theme === 'light' ? 'Helles Theme' : 'Dunkles Theme'}
      >
        <h2>{theme === 'light' ? 'Helles Theme' : 'Dunkles Theme'} · 360 px</h2>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (The scrollable history needs keyboard access.) -->
        <div class="conversation" role="region" tabindex="0" aria-label="Scrollbarer Verlauf">
          {#each Array.from({ length: 12 }, (_, index) => index + 1) as number (number)}
            <p>Nachricht {number}: Aktueller belegter Arbeitsstand.</p>
          {/each}
        </div>
        <label
          >Entwurf<textarea placeholder="Nachricht an A^3">Ein unveränderter Entwurf</textarea
          ></label
        >
        <div class="approval" role="region" aria-label="Wartende Freigabe">
          Wartende Aktion: unbekanntes Skript
        </div>
        <AgentPermissionsControl
          loader={async () => current}
          updater={update}
          subscriber={subscribe}
        />
        <AgentMachineRecovery
          taskId={'a'.repeat(64)}
          refreshKey={0}
          loader={async () => ({
            status: 'available',
            recovery: {
              ledgerRevision: 1,
              ledgerStoreVersion: '1',
              scope: 'b'.repeat(64),
              resourceKind: 'file',
              target: 'D:/Beispielprojekt/Maschinenzugriff/ausstehende-datei.txt',
            },
          })}
          recoverer={async () => ({ status: 'queued' })}
        />
      </section>
    {/each}
  </div>
</main>

<style>
  main {
    padding: 1rem;
  }
  h1 {
    font-size: 1.25rem;
  }
  h2 {
    font-size: 1rem;
  }
  .panels {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }
  section {
    width: 360px;
    max-width: 100%;
    padding: 1rem;
    color: var(--color-text);
    background: var(--color-canvas);
    border: 1px solid var(--color-border-soft);
  }
  .conversation {
    height: 140px;
    overflow: auto;
    border: 1px solid var(--color-border-soft);
    padding: 0.5rem;
  }
  label {
    display: block;
    margin-top: 0.75rem;
  }
  textarea {
    width: 100%;
  }
  .approval {
    padding: 0.5rem;
    margin-top: 0.5rem;
    background: var(--color-surface-muted);
  }
</style>

<script lang="ts">
  import { onMount } from 'svelte';
  import { parseCommandErrorV1 } from './command-error';
  import {
    confirmProjectCommandAllowlist,
    queryProjectSettings,
    type DiscoveredCommandKindV1,
    type DiscoveredCommandV1,
    type ProjectCommandConfirmationV1,
    type ProjectSettingsResponseV1,
  } from './project-settings';

  interface Props {
    allowlistConfirmer?: (
      expectedCatalogId: string,
      expectedAllowlistRevision: string | null,
      commandIds: string[],
    ) => Promise<ProjectSettingsResponseV1>;
    projectSettingsLoader?: () => Promise<ProjectSettingsResponseV1>;
  }

  type View =
    | { kind: 'loading' }
    | { kind: 'ready'; response: ProjectSettingsResponseV1 }
    | { kind: 'error'; message: string };
  type Action =
    | { kind: 'idle' }
    | { kind: 'submitting' }
    | { kind: 'saved' }
    | { kind: 'error'; message: string };

  let {
    allowlistConfirmer = confirmProjectCommandAllowlist,
    projectSettingsLoader = queryProjectSettings,
  }: Props = $props();

  let view = $state<View>({ kind: 'loading' });
  let action = $state<Action>({ kind: 'idle' });
  let selectedCommandIds = $state<string[]>([]);
  let confirmedCommandIds = $state<string[]>([]);

  onMount(() => {
    void loadProjectSettings();
  });

  async function loadProjectSettings(): Promise<void> {
    view = { kind: 'loading' };
    action = { kind: 'idle' };
    try {
      applyResponse(await projectSettingsLoader());
    } catch (error) {
      view = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  function applyResponse(response: ProjectSettingsResponseV1): void {
    view = { kind: 'ready', response };
    const commands =
      response.result.status === 'available' ? response.result.settings.commands : null;
    confirmedCommandIds =
      commands?.status === 'available'
        ? commands.commands
            .filter((command) => command.selected)
            .map((command) => command.commandId)
        : [];
    selectedCommandIds = [...confirmedCommandIds];
  }

  function setCommandSelected(commandId: string, selected: boolean): void {
    selectedCommandIds = selected
      ? [...selectedCommandIds, commandId].sort()
      : selectedCommandIds.filter((candidate) => candidate !== commandId);
    action = { kind: 'idle' };
  }

  function isSelected(commandId: string): boolean {
    return selectedCommandIds.includes(commandId);
  }

  function selectionChanged(): boolean {
    return selectedCommandIds.join('\0') !== [...confirmedCommandIds].sort().join('\0');
  }

  async function confirmSelection(): Promise<void> {
    if (
      view.kind !== 'ready' ||
      view.response.result.status !== 'available' ||
      view.response.result.settings.commands.status !== 'available' ||
      selectedCommandIds.length === 0 ||
      !selectionChanged()
    ) {
      return;
    }
    const commandSettings = view.response.result.settings.commands;
    const expectedRevision = confirmationRevision(commandSettings.confirmation);
    action = { kind: 'submitting' };
    try {
      applyResponse(
        await allowlistConfirmer(commandSettings.catalogId, expectedRevision, selectedCommandIds),
      );
      action = { kind: 'saved' };
    } catch (error) {
      action = { kind: 'error', message: recoveryMessage(error) };
    }
  }

  function confirmationRevision(confirmation: ProjectCommandConfirmationV1): string | null {
    return confirmation.status === 'notConfirmed' ? null : confirmation.revision;
  }

  function confirmationText(confirmation: ProjectCommandConfirmationV1): string {
    if (confirmation.status === 'notConfirmed') {
      return 'Noch keine Befehle bestätigt. Kein entdeckter Befehl ist automatisch freigegeben.';
    }
    if (confirmation.status === 'stale') {
      return 'Die frühere Auswahl ist durch geänderte Manifest-Evidenz veraltet und vollständig deaktiviert. Bitte aktuelle Befehle neu prüfen.';
    }
    return `Aktuelle Auswahl bestätigt · Revision ${confirmation.revision} · Unix ms ${confirmation.confirmedAtUnixMillis}`;
  }

  function commandKindLabel(kind: DiscoveredCommandKindV1): string {
    const labels: Record<DiscoveredCommandKindV1, string> = {
      build: 'Build',
      format: 'Format-Check',
      lint: 'Lint',
      test: 'Test',
    };
    return labels[kind];
  }

  function directArgv(command: DiscoveredCommandV1): string {
    return [command.executable, ...command.arguments.map((argument) => JSON.stringify(argument))]
      .join(' ')
      .trim();
  }

  function workingDirectoryLabel(hex: string | null): string {
    if (hex === null) return 'Worktree-Root';
    const bytes = new Uint8Array(hex.length / 2);
    for (let index = 0; index < hex.length; index += 2) {
      bytes[index / 2] = Number.parseInt(hex.slice(index, index + 2), 16);
    }
    try {
      const decoded = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      return Array.from(decoded)
        .map((character) =>
          character < ' ' || character === '\u007f'
            ? `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`
            : character,
        )
        .join('');
    } catch {
      return `Nicht-UTF-8-Pfad · hex ${hex}`;
    }
  }

  function recoveryMessage(error: unknown): string {
    const code = parseCommandErrorV1(error)?.code;
    if (code === 'invalidProjectSettingsRequest') {
      return 'Katalog oder Auswahl haben sich geändert. Lade die aktuellen Projekt-Settings neu und bestätige sie erneut.';
    }
    if (code === 'noActiveProject') {
      return 'Öffne zuerst einen lokalen Git-Worktree.';
    }
    if (code === 'projectOperationBusy') {
      return 'Eine andere Projektaktion läuft noch. Lade danach die aktuellen Projekt-Settings neu.';
    }
    if (code === 'localStorageCorrupt' || code === 'localStorageInvalidData') {
      return 'Die lokalen Bestätigungsdaten sind ungültig oder beschädigt und wurden nicht verändert. Sichere die App-Daten.';
    }
    return 'Projekt-Settings konnten nicht sicher gelesen oder bestätigt werden.';
  }
</script>

<section class="project-settings" aria-labelledby="project-settings-heading">
  <div class="settings-section-heading">
    <div>
      <p class="section-kicker">Projekt-Policy</p>
      <h3 id="project-settings-heading">Indexignore und sichere Befehle</h3>
      <p>
        Ausschlüsse stammen ausschließlich aus <code>.a3/project.toml</code>. Befehle werden aus
        veröffentlichter Manifest-Evidenz abgeleitet und hier nur bestätigt, nicht ausgeführt.
      </p>
    </div>
    <button type="button" onclick={loadProjectSettings}>Projekt-Settings neu laden</button>
  </div>

  {#if view.kind === 'loading'}
    <p class="settings-status" role="status" aria-live="polite">
      Projekt-Settings werden lokal rekonstruiert …
    </p>
  {:else if view.kind === 'error'}
    <div class="settings-error" role="status" aria-live="polite">
      <p>{view.message}</p>
      <button type="button" onclick={loadProjectSettings}>Projekt-Settings erneut laden</button>
    </div>
  {:else if view.response.result.status === 'noProject'}
    <p class="project-settings-empty" role="status">
      Kein aktiver Worktree. Modell-Settings bleiben unabhängig davon nutzbar.
    </p>
  {:else}
    <div class="project-policy-grid">
      <div class="project-policy-block" aria-labelledby="indexignore-heading">
        <div>
          <h4 id="indexignore-heading">Indexignore</h4>
          <p class="settings-meta">Read-only · repository-eigene Konfiguration</p>
        </div>
        {#if !view.response.result.settings.ignore.configurationPresent}
          <p>
            Keine <code>.a3/project.toml</code> vorhanden. Es gelten die sicheren eingebauten Ausschlüsse.
          </p>
        {:else if view.response.result.settings.ignore.patterns.length === 0}
          <p>Konfiguration vorhanden, ohne zusätzliche Ausschlussmuster.</p>
        {:else}
          <ul class="ignore-pattern-list" aria-label="Aktive zusätzliche Ausschlussmuster">
            {#each view.response.result.settings.ignore.patterns as pattern, index (index)}
              <li><code>{pattern}</code></li>
            {/each}
          </ul>
        {/if}
        <p class="settings-guidance">
          Änderungen erfolgen bewusst im Repository. Nach einer Änderung muss der Index neu
          veröffentlicht werden, bevor neue Evidenz verwendet wird.
        </p>
      </div>

      <div class="project-policy-block command-policy" aria-labelledby="command-allowlist-heading">
        <div>
          <h4 id="command-allowlist-heading">Command Allowlist</h4>
          <p class="settings-meta">Direktes argv · keine Shell · Netzwerk aus</p>
        </div>
        {#if view.response.result.settings.commands.status === 'noPublishedIndex'}
          <p role="status">
            Noch kein veröffentlichter Index. A^3 leitet vorher keine Projektbefehle ab.
          </p>
        {:else}
          {@const commandSettings = view.response.result.settings.commands}
          <p
            class:stale-confirmation={commandSettings.confirmation.status === 'stale'}
            class="confirmation-state"
            role="status"
          >
            {confirmationText(commandSettings.confirmation)}
          </p>
          {#if commandSettings.commands.length === 0}
            <p>In der aktuellen Manifest-Evidenz wurden keine sicheren Befehle erkannt.</p>
          {:else}
            <ul class="command-catalog" aria-label="Aktueller sicherer Befehlskatalog">
              {#each commandSettings.commands as command (command.commandId)}
                <li>
                  <label class="command-selection">
                    <input
                      type="checkbox"
                      checked={isSelected(command.commandId)}
                      disabled={action.kind === 'submitting'}
                      onchange={(event) =>
                        setCommandSelected(command.commandId, event.currentTarget.checked)}
                    />
                    <span>
                      <strong>{commandKindLabel(command.kind)}</strong>
                      <span class="command-cwd"
                        >Arbeitsordner: {workingDirectoryLabel(command.workingDirectoryHex)}</span
                      >
                      <code class="command-argv">{directArgv(command)}</code>
                      <small>{command.evidenceCount} aktuelle Evidenzquelle(n)</small>
                    </span>
                  </label>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="command-confirmation-actions">
            <button
              class="primary-action"
              type="button"
              disabled={action.kind === 'submitting' ||
                selectedCommandIds.length === 0 ||
                !selectionChanged()}
              onclick={confirmSelection}
            >
              {action.kind === 'submitting'
                ? 'Auswahl wird bestätigt …'
                : 'Ausgewählte direkte Befehle bestätigen'}
            </button>
            <p>
              Eine Bestätigung gilt nur für diesen exakten evidenzgebundenen Katalog. Änderungen an
              Manifesten machen sie automatisch unwirksam.
            </p>
          </div>
          {#if action.kind === 'saved'}
            <p class="settings-success" role="status" aria-live="polite">
              Auswahl dauerhaft bestätigt. Es wurde kein Befehl ausgeführt.
            </p>
          {:else if action.kind === 'error'}
            <p class="settings-error-message" role="alert">{action.message}</p>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</section>

<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import {
    controlAgentApproval,
    queryAgentApprovalV2,
    type AgentApprovalControlActionV1,
    type AgentApprovalControlResponseV1,
    type AgentApprovalResponseV2,
    type AgentApprovalResponseV1,
    type AgentApprovalV2,
  } from './agent-approval';

  interface Props {
    taskId: string;
    onwrite?: () => void;
    refreshKey?: string | number;
    loader?: (taskId: string) => Promise<AgentApprovalResponseV2 | AgentApprovalResponseV1>;
    controller?: (
      taskId: string,
      approval: AgentApprovalV2,
      action: AgentApprovalControlActionV1,
    ) => Promise<AgentApprovalControlResponseV1>;
    onChanged?: () => Promise<void> | void;
  }

  type View =
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: AgentApprovalResponseV2['result'] };
  type PendingChoice = 'allowOnce' | 'deny' | null;

  let {
    taskId,
    onwrite,
    refreshKey = 0,
    loader = queryAgentApprovalV2,
    controller = controlAgentApproval,
    onChanged = () => undefined,
  }: Props = $props();
  let view = $state<View>({ kind: 'loading' });
  let choice = $state<PendingChoice>(null);
  let controlling = $state(false);
  let message = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let requestNumber = 0;
  let observedTaskId = '';
  let refreshing = false;
  let refreshQueued = false;
  let refreshFailed = $state(false);
  let disposed = false;

  $effect(() => {
    const selectedTask = taskId;
    const refresh = refreshKey;
    untrack(() => {
      if (selectedTask !== observedTaskId) {
        requestNumber += 1;
        observedTaskId = selectedTask;
        view = { kind: 'loading' };
        choice = null;
        message = null;
        actionError = null;
        controlling = false;
        refreshing = false;
        refreshQueued = false;
        refreshFailed = false;
      }
      if (selectedTask.length > 0 && refresh !== undefined) void load();
    });
  });

  onDestroy(() => {
    disposed = true;
    requestNumber += 1;
  });

  function approvalIdentity(result: AgentApprovalResponseV2['result']): string | null {
    return result.status === 'available'
      ? `${result.approval.approvalRevision}:${result.approval.ledgerStoreVersion}:${result.approval.status}`
      : null;
  }

  async function load(afterControl = false): Promise<void> {
    if (disposed || (controlling && !afterControl)) return;
    if (refreshing) {
      refreshQueued = true;
      return;
    }
    const request = ++requestNumber;
    const selectedTask = taskId;
    refreshing = true;
    try {
      const response = await loader(selectedTask);
      if (request === requestNumber && selectedTask === taskId) {
        if (
          response.result.status === 'activityChanged' &&
          view.kind === 'result' &&
          view.result.status === 'available'
        ) {
          refreshFailed = true;
          choice = null;
          return;
        }
        if (
          view.kind !== 'result' ||
          approvalIdentity(view.result) !== approvalIdentity(response.result)
        )
          choice = null;
        view = { kind: 'result', result: response.result };
        refreshFailed = false;
      }
    } catch {
      if (request === requestNumber) {
        refreshFailed = true;
        choice = null;
        if (view.kind !== 'result' || view.result.status !== 'available') view = { kind: 'error' };
      }
    } finally {
      if (request === requestNumber) {
        refreshing = false;
        if (refreshQueued) {
          refreshQueued = false;
          void load();
        }
      }
    }
  }

  async function apply(action: AgentApprovalControlActionV1): Promise<void> {
    if (
      disposed ||
      controlling ||
      refreshFailed ||
      view.kind !== 'result' ||
      view.result.status !== 'available'
    )
      return;
    const approval = view.result.approval;
    const selectedTask = taskId;
    const owner = ++requestNumber;
    refreshing = false;
    refreshQueued = false;
    controlling = true;
    message = null;
    actionError = null;
    try {
      const response = await controller(selectedTask, approval, action);
      if (disposed || selectedTask !== taskId || owner !== requestNumber) return;
      if (response.result.status === 'applied') {
        message = outcomeMessage(response.result);
        choice = null;
        await onChanged();
        if (!disposed && selectedTask === taskId) await load(true);
      } else if (response.result.status === 'activityChanged') {
        actionError =
          'Die Anfrage oder der Arbeitsplan hat sich geändert. Der aktuelle Stand wurde geladen.';
        choice = null;
        await onChanged();
        if (!disposed && selectedTask === taskId) await load(true);
      } else {
        actionError = 'Diese Entscheidung ist im aktuellen dauerhaften Zustand nicht verfügbar.';
        await load(true);
      }
    } catch {
      if (disposed || selectedTask !== taskId) return;
      actionError =
        'Die Entscheidung konnte nicht gespeichert werden. Bitte prüfe den aktuellen Stand.';
    } finally {
      if (!disposed && selectedTask === taskId) controlling = false;
    }
  }

  function confirmPending(): void {
    if (choice !== null) void apply(choice);
  }

  function outcomeMessage(
    result: Extract<AgentApprovalControlResponseV1['result'], { status: 'applied' }>,
  ): string {
    switch (result.outcome) {
      case 'grantStored':
        return 'Die einmalige Freigabe wurde gespeichert. Die Aktion wurde noch nicht gestartet.';
      case 'denied':
        return 'Die Anfrage wurde abgelehnt. Plane die Aufgabe neu oder brich sie ab, um fortzufahren.';
      case 'revoked':
        return 'Die noch ungenutzte Freigabe wurde widerrufen.';
      case 'continueRequested':
        return result.runtimeStart === 'queued'
          ? 'Der Agent wird mit der einmaligen Freigabe fortgesetzt.'
          : result.runtimeStart === 'failed'
            ? 'Die Freigabe bleibt aktiv; der Agent konnte gerade nicht fortgesetzt werden.'
            : 'Die Freigabe bleibt aktiv; richte zuerst ein geeignetes Agentenmodell ein.';
    }
  }

  function classLabel(value: AgentApprovalV2['actionClass']): string {
    return (
      {
        read: 'Lesen',
        derive: 'Ableiten',
        write: 'Schreiben',
        executeSafe: 'Bekannten Prozess ausführen',
        executeOpen: 'Offenen Prozess ausführen',
        network: 'Netzwerk',
        destructive: 'Destruktiv',
        publish: 'Veröffentlichen',
        outsideRoot: 'Außerhalb des Worktrees',
      } as const
    )[value];
  }

  function riskLabel(value: AgentApprovalV2['risk']): string {
    return ({ low: 'Niedrig', moderate: 'Moderat', high: 'Hoch', critical: 'Kritisch' } as const)[
      value
    ];
  }

  function statusLabel(value: AgentApprovalV2['status']): string {
    return (
      {
        pending: 'Entscheidung ausstehend',
        active: 'Freigabe aktiv',
        consumed: 'Freigabe verbraucht',
        revoked: 'Freigabe widerrufen',
        expired: 'Freigabe abgelaufen',
        denied: 'Anfrage abgelehnt',
      } as const
    )[value];
  }

  function readableTime(value: string): string {
    const date = new Date(Number(value));
    return Number.isNaN(date.getTime())
      ? 'Zeitpunkt nicht darstellbar'
      : new Intl.DateTimeFormat('de-DE', { dateStyle: 'medium', timeStyle: 'short' }).format(date);
  }

  function operationLabel(operation: 'add' | 'update' | 'move' | 'delete'): string {
    return { add: 'Anlegen', update: 'Ändern', move: 'Verschieben', delete: 'Löschen' }[operation];
  }
</script>

<section class="approval-center" aria-labelledby="approval-center-heading">
  <header>
    <div>
      <h3 id="approval-center-heading">Aktion freigeben</h3>
    </div>
    <div class="header-actions">
      {#if view.kind === 'result' && view.result.status === 'available'}
        <span class="status-chip">{statusLabel(view.result.approval.status)}</span>
      {/if}
      {#if onwrite}<button type="button" class="write-message" onclick={onwrite}
          >Nachricht schreiben</button
        >{/if}
    </div>
  </header>
  {#if refreshFailed && view.kind === 'result'}
    <p class="bounded-note" role="status">
      Diese Freigabe wird erneut geprüft. Entscheidungen sind bis zum aktuellen Stand gesperrt.
    </p>
    <button type="button" onclick={() => load()}>Erneut prüfen</button>
  {/if}

  {#if view.kind === 'loading'}
    <p role="status" aria-live="polite">Freigabe wird geladen …</p>
  {:else if view.kind === 'error'}
    <div class="error-state" role="alert">
      <p>Die Freigabeanfrage konnte nicht geladen werden.</p>
      <button type="button" onclick={() => load()}>Erneut prüfen</button>
    </div>
  {:else if view.result.status === 'unavailable'}
    <p class="empty-state">Für diese Aufgabe ist gerade keine Freigabe erforderlich.</p>
  {:else if view.result.status === 'activityChanged'}
    <div class="error-state" role="status">
      <p>Die Anfrage oder der Arbeitsplan hat sich geändert. Lade den aktuellen Stand.</p>
      <button type="button" onclick={() => load()}>Aktuellen Stand laden</button>
    </div>
  {:else if view.result.status === 'goalRevisionMismatch'}
    <p class="error-state" role="alert">
      Das Ziel wurde geändert. Der Arbeitsplan muss vor einer Freigabe aktualisiert werden.
    </p>
  {:else if view.result.status === 'noProject' || view.result.status === 'taskNotFound' || view.result.status === 'ledgerUnavailable'}
    <p class="empty-state">
      Wähle eine aktuelle Aufgabe mit Arbeitsplan, um eine Aktion freizugeben.
    </p>
  {:else if view.result.status === 'available'}
    {@const approval = view.result.approval}
    <!-- The exact scope can exceed a small window and must remain keyboard-scrollable. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="approval-content" tabindex="0" role="region" aria-label="Aktion und Freigabeumfang">
      <dl class="approval-facts">
        <div>
          <dt>Aktion</dt>
          <dd>{classLabel(approval.actionClass)}</dd>
        </div>
        <div>
          <dt>Risiko</dt>
          <dd>{riskLabel(approval.risk)}</dd>
        </div>
        <div>
          <dt>Gilt für</dt>
          <dd>Nur diese Aktion, die angezeigten Ziele und den aktuellen Lauf. Einmalig.</dd>
        </div>
        <div>
          <dt>Grund</dt>
          <dd>
            {approval.reason === 'workspacePolicy'
              ? 'Die Projektregeln verlangen deine Freigabe.'
              : 'Die Sicherheitsregeln verlangen deine Freigabe.'}
          </dd>
        </div>
        <div>
          <dt>Gültig bis</dt>
          <dd>{readableTime(approval.expiresAtUnixMillis)}</dd>
        </div>
      </dl>

      {#if approval.action.kind === 'patch'}
        <section class="action-detail" aria-labelledby="approval-patch-heading">
          <h4 id="approval-patch-heading">Diese Dateien werden geändert</h4>
          <p>{approval.action.patch.rationale}</p>
          <ul>
            {#each approval.action.patch.files as file, index (`${file.operation}-${index}`)}
              <li>
                <strong>{operationLabel(file.operation)}</strong>
                <code
                  >{file.sourcePath?.displayPath ?? '∅'} → {file.targetPath?.displayPath ??
                    '∅'}</code
                >
              </li>
            {/each}
          </ul>
        </section>
      {:else if approval.action.kind === 'machine'}
        <section class="action-detail" aria-labelledby="approval-machine-heading">
          <h4 id="approval-machine-heading">
            {approval.action.resourceKind === 'http'
              ? 'Diese Adresse wird gelesen'
              : 'Diese externe Datei ist betroffen'}
          </h4>
          <p>
            {({ read: 'Lesen', write: 'Schreiben', delete: 'Löschen' } as const)[
              approval.action.operation
            ]}
          </p>
          <code>{approval.action.target}</code>
          {#if approval.action.operation === 'delete'}<p>Die Datei wird dauerhaft gelöscht.</p>{/if}
        </section>
      {:else}
        {@const process = approval.action.process}
        <section class="action-detail" aria-labelledby="approval-process-heading">
          <h4 id="approval-process-heading">Dieser Befehl wird ausgeführt</h4>
          <dl>
            <div>
              <dt>Programm und Argumente</dt>
              <dd>
                <ol class="argv">
                  {#each [process.executable, ...process.arguments] as argument, index (`${index}-${argument}`)}
                    <li>
                      <span class="sr-only">{index === 0 ? 'Programm' : `Argument ${index}`}</span>
                      <code>{JSON.stringify(argument)}</code>
                    </li>
                  {/each}
                </ol>
              </dd>
            </div>
            <div>
              <dt>Arbeitsordner</dt>
              <dd>
                <code
                  >{process.workingDirectory.kind === 'root'
                    ? '.'
                    : process.workingDirectory.path.displayPath}</code
                >
              </dd>
            </div>
            <div>
              <dt>Netzwerk</dt>
              <dd>
                {process.network.kind === 'denied'
                  ? 'Nicht angefordert'
                  : `Angefordert · Scope ${process.network.scopeDigest}`}
              </dd>
            </div>
          </dl>
        </section>
      {/if}

      <details class="technical-details approval-audit">
        <summary>Technische Freigabedetails</summary>
        <dl>
          <div>
            <dt>Exakter Scope</dt>
            <dd><code>{approval.scopeDigest}</code></dd>
          </div>
          <div>
            <dt>Arbeitsplan</dt>
            <dd>Revision {approval.ledgerRevision} · <code>{approval.stepId}</code></dd>
          </div>
          <div>
            <dt>Zeitanker</dt>
            <dd>{approval.requestedAtUnixMillis}–{approval.expiresAtUnixMillis} ms</dd>
          </div>
          {#if approval.action.kind === 'patch'}
            {#each approval.action.patch.files as file, index (`${file.operation}-${index}`)}
              <div>
                <dt>Pfaddetails {index + 1}</dt>
                <dd>
                  {#if file.sourcePath}<span
                      >Quelle (Bytes): <code>{file.sourcePath.pathHex}</code></span
                    >{/if}
                  {#if file.targetPath}<span
                      >Ziel (Bytes): <code>{file.targetPath.pathHex}</code></span
                    >{/if}
                </dd>
              </div>
            {/each}
          {/if}
          {#if approval.action.kind === 'process'}
            {@const process = approval.action.process}
            <div>
              <dt>Umgebungsvariablen</dt>
              <dd>
                {process.environmentAllowlist.length === 0
                  ? 'Keine'
                  : process.environmentAllowlist.join(', ')}
              </dd>
            </div>
            <div>
              <dt>Zeitlimit</dt>
              <dd>{Number(process.timeoutMillis) / 1000} Sekunden</dd>
            </div>
            <div>
              <dt>Ausgabegrenzen</dt>
              <dd>stdout {process.stdoutLimit} B · stderr {process.stderrLimit} B</dd>
            </div>
            <div>
              <dt>Modus</dt>
              <dd>
                {{
                  knownSafe: 'Bekannter Befehl',
                  open: 'Offene Ausführung',
                  shell: 'Shell-Ausführung',
                }[process.executionMode]}
              </dd>
            </div>
            <div>
              <dt>Befehlstyp</dt>
              <dd>
                {{
                  test: 'Tests',
                  build: 'Build',
                  diagnostic: 'Diagnose',
                  lint: 'Codeprüfung',
                  format: 'Formatierung',
                  command: 'Befehl',
                }[process.processKind]}
              </dd>
            </div>
            <div>
              <dt>Planbindung</dt>
              <dd>
                {process.planBinding.kind === 'unbound'
                  ? 'Ungebunden'
                  : 'An den aktuellen Arbeitsschritt gebunden'}
              </dd>
            </div>

            <div>
              <dt>Specification-ID</dt>
              <dd><code>{approval.action.process.specificationId}</code></dd>
            </div>
          {/if}
        </dl>
      </details>
    </div>
    {#if approval.status === 'pending'}
      <div class="decision-row">
        <fieldset class="decision-options">
          <legend class="sr-only">Entscheidung auswählen</legend>
          <label
            ><input
              type="radio"
              name={`approval-${taskId}`}
              aria-label="Diese Aktion einmal erlauben"
              value="allowOnce"
              bind:group={choice}
              disabled={!approval.canAllowOnce || controlling || refreshFailed}
            /> Einmal erlauben</label
          >
          <label
            ><input
              type="radio"
              name={`approval-${taskId}`}
              aria-label="Ablehnen und diesen Schritt stoppen"
              value="deny"
              bind:group={choice}
              disabled={!approval.canDeny || controlling || refreshFailed}
            /> Ablehnen & stoppen</label
          >
        </fieldset>
        <button
          type="button"
          class="confirm-decision"
          disabled={choice === null || controlling || refreshFailed}
          onclick={confirmPending}>Entscheidung bestätigen</button
        >
      </div>
    {:else if approval.status === 'active'}
      <p class="bounded-note">
        Die Freigabe ist gespeichert. Erst „Agent fortsetzen“ startet die Aktion. Bis dahin kannst
        du die Freigabe widerrufen.
      </p>
      <div class="decision-actions">
        <button
          class="confirm-decision"
          type="button"
          disabled={!approval.canContinue || controlling || refreshFailed}
          onclick={() => apply('continue')}>Agent fortsetzen</button
        >
        <button
          type="button"
          disabled={!approval.canRevoke || controlling || refreshFailed}
          onclick={() => apply('revoke')}>Freigabe widerrufen</button
        >
      </div>
    {:else if approval.status === 'denied'}
      <p class="bounded-note">
        Die Aktion wurde abgelehnt. Du kannst die Aufgabe neu planen oder abbrechen.
      </p>
    {:else}
      <p class="bounded-note">Diese Anfrage ist abgeschlossen oder nicht mehr verwendbar.</p>
    {/if}
  {:else}
    <p class="empty-state">Der Freigabestatus ist gerade nicht verfügbar.</p>
  {/if}

  {#if message !== null}<p class="success-state" role="status" aria-live="polite">{message}</p>{/if}
  {#if actionError !== null}<p class="error-state" role="alert">{actionError}</p>{/if}
</section>

<style>
  .approval-center {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-height: min(26rem, calc(52dvh - 2rem));
    gap: var(--space-2);
    padding: 0;
  }
  .approval-content {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    display: grid;
    gap: var(--space-2);
  }
  header,
  .decision-row,
  .decision-actions,
  .bounded-note {
    flex-shrink: 0;
  }
  .decision-options input {
    min-height: 1rem;
    min-width: 1rem;
    width: 1rem;
    height: 1rem;
    padding: 0;
  }
  .decision-options label {
    font-size: var(--font-size-xs);
  }
  .action-detail h4 {
    font-size: var(--font-size-sm);
  }
  header {
    align-items: center;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  h3 {
    font-size: var(--font-size-base);
  }
  .write-message {
    min-height: var(--control-min-size);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
  }
  .write-message:hover {
    background: var(--color-surface-muted);
    color: var(--color-text);
  }
  h3,
  h4,
  p {
    margin-block: 0;
  }
  .status-chip {
    color: var(--color-accent-text);
    font-size: var(--font-size-xs);
    padding: 0.3rem 0;
  }
  .action-detail dl {
    display: grid;
    gap: 0;
    margin: 0;
  }
  .action-detail dl div {
    display: grid;
    grid-template-columns: minmax(6rem, 0.35fr) minmax(0, 1fr);
    align-items: baseline;
    gap: 0.5rem 0.9rem;
    padding: 0.25rem 0;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .approval-facts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-1) var(--space-3);
    margin: 0;
  }
  .approval-facts > div {
    display: grid;
    align-content: start;
    gap: 0.15rem var(--space-2);
    grid-template-columns: auto 1fr;
    align-items: baseline;
    font-size: var(--font-size-xs);
  }
  .approval-facts > div:nth-child(5) {
    grid-column: 3;
    grid-row: 1;
  }
  .approval-facts > div:nth-child(3),
  .approval-facts > div:nth-child(4) {
    grid-column: 1 / -1;
    grid-template-columns: 4rem minmax(0, 1fr);
    font-size: var(--font-size-xs);
    color: var(--color-muted);
  }
  dt {
    color: var(--color-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  code {
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .action-detail {
    display: grid;
    gap: 0.25rem;
    min-width: 0;
  }
  .action-detail ul {
    display: grid;
    gap: 0.15rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .action-detail li {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding-block: 0;
  }
  .argv {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .argv li {
    padding: var(--space-1) var(--space-2);
    background: var(--color-canvas);
    border-radius: var(--radius-control);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .decision-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border-soft);
  }
  .decision-options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: 0;
    margin: 0;
    border: 0;
  }
  .decision-options label {
    align-items: center;
    display: grid;
    min-height: var(--control-min-size);
    gap: 0.55rem;
    grid-template-columns: auto 1fr;
    cursor: pointer;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    transition:
      background 120ms ease,
      border-color 120ms ease;
  }
  .decision-options label:has(input:checked) {
    border-color: var(--color-accent);
    background: var(--color-accent-surface);
  }
  .confirm-decision:not(:disabled) {
    background: var(--color-accent-strong);
    color: var(--color-on-accent);
    border-color: var(--color-accent-strong);
  }
  @media (prefers-reduced-motion: reduce) {
    .decision-options label {
      transition: none;
    }
  }
  .technical-details {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .technical-details summary {
    display: flex;
    align-items: center;
    min-height: var(--control-min-size);
    cursor: pointer;
  }
  .technical-details summary::before {
    content: '+';
    margin-right: 0.5rem;
  }
  .technical-details[open] summary::before {
    content: '−';
  }
  .technical-details dl {
    display: grid;
    gap: 0.8rem;
    margin: 0.4rem 0;
  }
  .approval-audit {
    border-bottom: 1px solid var(--color-border-soft);
  }
  button,
  .decision-options label,
  .technical-details summary {
    transition:
      background var(--motion-fast, 120ms) var(--ease-out, ease-out),
      color var(--motion-fast, 120ms) var(--ease-out, ease-out);
  }
  .decision-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }
  button {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-control);
    color: inherit;
    cursor: pointer;
    padding: 0.55rem 0.8rem;
    min-height: var(--control-min-size);
  }
  button:hover:not(:disabled),
  .decision-options label:hover {
    background: var(--color-surface-subtle);
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .bounded-note,
  .empty-state {
    color: var(--color-muted);
  }
  .error-state {
    color: var(--color-danger);
  }
  .success-state {
    color: var(--color-positive);
  }
  @media (max-width: 760px) {
    .header-actions .status-chip {
      display: none;
    }
    .approval-facts {
      grid-template-columns: 1fr 1fr;
    }
    .approval-facts > div:nth-child(5) {
      grid-column: 1 / -1;
      grid-row: auto;
    }
  }
</style>

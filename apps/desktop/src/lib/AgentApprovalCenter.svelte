<script lang="ts">
  import {
    controlAgentApproval,
    queryAgentApproval,
    type AgentApprovalControlActionV1,
    type AgentApprovalControlResponseV1,
    type AgentApprovalResponseV1,
    type AgentApprovalV1,
  } from './agent-approval';

  interface Props {
    taskId: string;
    loader?: (taskId: string) => Promise<AgentApprovalResponseV1>;
    controller?: (
      taskId: string,
      approval: AgentApprovalV1,
      action: AgentApprovalControlActionV1,
    ) => Promise<AgentApprovalControlResponseV1>;
    onChanged?: () => Promise<void> | void;
  }

  type View =
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: AgentApprovalResponseV1['result'] };
  type PendingChoice = 'allowOnce' | 'deny' | null;

  let {
    taskId,
    loader = queryAgentApproval,
    controller = controlAgentApproval,
    onChanged = () => undefined,
  }: Props = $props();
  let view = $state<View>({ kind: 'loading' });
  let choice = $state<PendingChoice>(null);
  let controlling = $state(false);
  let message = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let requestNumber = 0;

  $effect(() => {
    choice = null;
    message = null;
    actionError = null;
    if (taskId.length > 0) void load();
  });

  async function load(): Promise<void> {
    const request = ++requestNumber;
    view = { kind: 'loading' };
    try {
      const response = await loader(taskId);
      if (request === requestNumber) view = { kind: 'result', result: response.result };
    } catch {
      if (request === requestNumber) view = { kind: 'error' };
    }
  }

  async function apply(action: AgentApprovalControlActionV1): Promise<void> {
    if (controlling || view.kind !== 'result' || view.result.status !== 'available') return;
    const approval = view.result.approval;
    controlling = true;
    message = null;
    actionError = null;
    try {
      const response = await controller(taskId, approval, action);
      if (response.result.status === 'applied') {
        message = outcomeMessage(response.result);
        choice = null;
        await onChanged();
        await load();
      } else if (response.result.status === 'activityChanged') {
        actionError =
          'Die Anfrage oder der Arbeitsplan hat sich geändert. Der aktuelle Stand wurde geladen.';
        choice = null;
        await onChanged();
        await load();
      } else {
        actionError = 'Diese Entscheidung ist im aktuellen dauerhaften Zustand nicht verfügbar.';
        await load();
      }
    } catch {
      actionError =
        'Die Entscheidung konnte nicht gespeichert werden. Bitte prüfe den aktuellen Stand.';
    } finally {
      controlling = false;
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

  function classLabel(value: AgentApprovalV1['actionClass']): string {
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

  function riskLabel(value: AgentApprovalV1['risk']): string {
    return ({ low: 'Niedrig', moderate: 'Moderat', high: 'Hoch', critical: 'Kritisch' } as const)[
      value
    ];
  }

  function statusLabel(value: AgentApprovalV1['status']): string {
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
      <p>Deine Entscheidung</p>
      <h3 id="approval-center-heading">Aktion freigeben</h3>
    </div>
    {#if view.kind === 'result' && view.result.status === 'available'}
      <span class="status-chip">{statusLabel(view.result.approval.status)}</span>
    {/if}
  </header>

  {#if view.kind === 'loading'}
    <p role="status" aria-live="polite">Freigabe wird geladen …</p>
  {:else if view.kind === 'error'}
    <div class="error-state" role="alert">
      <p>Die Freigabeanfrage konnte nicht geladen werden.</p>
      <button type="button" onclick={load}>Erneut prüfen</button>
    </div>
  {:else if view.result.status === 'unavailable'}
    <p class="empty-state">Für diese Aufgabe ist gerade keine Freigabe erforderlich.</p>
  {:else if view.result.status === 'activityChanged'}
    <div class="error-state" role="status">
      <p>Die Anfrage oder der Arbeitsplan hat sich geändert. Lade den aktuellen Stand.</p>
      <button type="button" onclick={load}>Aktuellen Stand laden</button>
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
                >{file.sourcePath?.displayPath ?? '∅'} → {file.targetPath?.displayPath ?? '∅'}</code
              >
              <details class="technical-details">
                <summary>Pfaddetails</summary>
                {#if file.sourcePath !== null}
                  <small>Quelle (Bytes): <code>{file.sourcePath.pathHex}</code></small>
                {/if}
                {#if file.targetPath !== null}
                  <small>Ziel (Bytes): <code>{file.targetPath.pathHex}</code></small>
                {/if}
              </details>
            </li>
          {/each}
        </ul>
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
                  <li><span>argv[{index}]</span> <code>{JSON.stringify(argument)}</code></li>
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
            <dt>Umgebungsvariablen</dt>
            <dd>
              {process.environmentAllowlist.length === 0
                ? 'Keine'
                : process.environmentAllowlist.join(', ')}
            </dd>
          </div>
          <div>
            <dt>Zeitlimit</dt>
            <dd>{process.timeoutMillis} ms</dd>
          </div>
          <div>
            <dt>Ausgabegrenzen</dt>
            <dd>stdout {process.stdoutLimit} B · stderr {process.stderrLimit} B</dd>
          </div>
          <div>
            <dt>Modus</dt>
            <dd>{process.executionMode}</dd>
          </div>
          <div>
            <dt>Befehlstyp</dt>
            <dd>{process.processKind}</dd>
          </div>
          <div>
            <dt>Planbindung</dt>
            <dd>
              {process.planBinding.kind === 'unbound'
                ? 'Ungebunden'
                : `Schritt ${process.planBinding.stepId}`}
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
        {#if approval.action.kind === 'process'}
          <div>
            <dt>Specification-ID</dt>
            <dd><code>{approval.action.process.specificationId}</code></dd>
          </div>
        {/if}
      </dl>
    </details>

    {#if approval.status === 'pending'}
      <fieldset class="decision-options">
        <legend>Entscheidung auswählen</legend>
        <label
          ><input
            type="radio"
            name={`approval-${taskId}`}
            value="allowOnce"
            bind:group={choice}
            disabled={!approval.canAllowOnce || controlling}
          /> Diese Aktion einmal erlauben</label
        >
        <label
          ><input
            type="radio"
            name={`approval-${taskId}`}
            value="deny"
            bind:group={choice}
            disabled={!approval.canDeny || controlling}
          /> Ablehnen und diesen Schritt stoppen</label
        >
      </fieldset>
      <button type="button" disabled={choice === null || controlling} onclick={confirmPending}
        >Entscheidung bestätigen</button
      >
    {:else if approval.status === 'active'}
      <p class="bounded-note">
        Die Freigabe ist gespeichert. Erst „Agent fortsetzen“ startet die Aktion. Bis dahin kannst
        du die Freigabe widerrufen.
      </p>
      <div class="decision-actions">
        <button
          type="button"
          disabled={!approval.canContinue || controlling}
          onclick={() => apply('continue')}>Agent fortsetzen</button
        >
        <button
          type="button"
          disabled={!approval.canRevoke || controlling}
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
    display: grid;
    min-width: 0;
    gap: 1.1rem;
    padding: 0.3rem 0;
  }
  header {
    align-items: start;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  header p {
    color: var(--color-muted);
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    margin: 0;
    text-transform: uppercase;
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
  .approval-facts,
  .action-detail dl {
    display: grid;
    gap: 0;
    margin: 0;
  }
  .approval-facts div,
  .action-detail dl div {
    display: grid;
    grid-template-columns: minmax(6rem, 0.35fr) minmax(0, 1fr);
    align-items: baseline;
    gap: 0.5rem 0.9rem;
    padding: 0.7rem 0;
    border-bottom: 1px solid var(--color-border-soft);
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
    gap: 0.65rem;
    min-width: 0;
  }
  .action-detail ul {
    display: grid;
    gap: 0.45rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .action-detail li {
    display: grid;
    gap: 0.25rem;
    padding-block: 0.4rem;
  }
  .action-detail small {
    color: var(--color-muted);
    overflow-wrap: anywhere;
  }
  .argv {
    display: grid;
    gap: 0.25rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .argv li {
    grid-template-columns: auto 1fr;
  }
  .argv span {
    color: var(--color-muted);
    font-size: 0.78rem;
  }
  .decision-options {
    border: 0;
    border-top: 1px solid var(--color-border-soft);
    display: grid;
    gap: 0.3rem;
    margin: 0;
    padding: 0.8rem 0 0;
  }
  .decision-options legend {
    padding: 0 0.6rem 0 0;
    font-weight: 650;
  }
  .decision-options label {
    align-items: center;
    display: grid;
    min-height: var(--control-min-size);
    gap: 0.55rem;
    grid-template-columns: auto 1fr;
    cursor: pointer;
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
  .technical-details small {
    display: block;
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
</style>

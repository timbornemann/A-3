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
        actionError = 'Approval oder Ledger haben sich geändert. Der aktuelle Stand wurde geladen.';
        choice = null;
        await onChanged();
        await load();
      } else {
        actionError = 'Diese Entscheidung ist im aktuellen dauerhaften Zustand nicht verfügbar.';
        await load();
      }
    } catch {
      actionError = 'Die Approval-Entscheidung konnte nicht sicher abgeschlossen werden.';
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
        return 'Die exakte einmalige Freigabe wurde gespeichert. Die Mutation wurde noch nicht gestartet.';
      case 'denied':
        return 'Die Anfrage wurde abgelehnt; der Schritt ist dauerhaft blockiert und kann über Replan oder Cancel aufgelöst werden.';
      case 'revoked':
        return 'Die noch ungenutzte Freigabe wurde widerrufen.';
      case 'continueRequested':
        return result.runtimeStart === 'queued'
          ? 'Ein neuer vom Scheduler verwalteter Agent-Versuch wurde mit der exakten Freigabe eingereiht.'
          : result.runtimeStart === 'failed'
            ? 'Die Freigabe bleibt aktiv; der Agent-Versuch konnte nicht eingereiht werden.'
            : 'Die Freigabe bleibt aktiv; derzeit ist keine ausführbare Agent-Capability verfügbar.';
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
</script>

<section class="approval-center" aria-labelledby="approval-center-heading">
  <header>
    <div>
      <p>Explizite Policy-Entscheidung</p>
      <h3 id="approval-center-heading">Approval Center</h3>
    </div>
    {#if view.kind === 'result' && view.result.status === 'available'}
      <span class="status-chip">{statusLabel(view.result.approval.status)}</span>
    {/if}
  </header>

  {#if view.kind === 'loading'}
    <p role="status" aria-live="polite">Exakte Approval-Anfrage wird geprüft …</p>
  {:else if view.kind === 'error'}
    <div class="error-state" role="alert">
      <p>Die Approval-Anfrage konnte nicht sicher gelesen werden.</p>
      <button type="button" onclick={load}>Erneut prüfen</button>
    </div>
  {:else if view.result.status === 'unavailable'}
    <p class="empty-state">Für diese Aufgabe liegt keine aktuelle Approval-Anfrage vor.</p>
  {:else if view.result.status === 'activityChanged'}
    <div class="error-state" role="status">
      <p>Approval oder Ledger haben sich während des Lesens geändert.</p>
      <button type="button" onclick={load}>Aktuellen Stand laden</button>
    </div>
  {:else if view.result.status === 'goalRevisionMismatch'}
    <p class="error-state" role="alert">
      Goal R{view.result.currentRevision} und Ledger-Goal R{view.result.ledgerRevision} stimmen nicht
      überein.
    </p>
  {:else if view.result.status === 'noProject' || view.result.status === 'taskNotFound' || view.result.status === 'ledgerUnavailable'}
    <p class="empty-state">
      Ohne aktuelle Aufgabe und Ledger ist keine Approval-Entscheidung möglich.
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
        <dt>Scope</dt>
        <dd><code>{approval.scopeDigest}</code></dd>
      </div>
      <div>
        <dt>Grund</dt>
        <dd>
          {approval.reason === 'workspacePolicy'
            ? 'Workspace-Policy verlangt Freigabe'
            : 'System-Policy verlangt Freigabe'}
        </dd>
      </div>
      <div>
        <dt>Gültigkeit</dt>
        <dd>{approval.requestedAtUnixMillis}–{approval.expiresAtUnixMillis} ms</dd>
      </div>
      <div>
        <dt>Anker</dt>
        <dd>
          Ledger R{approval.ledgerRevision} · Schritt <code>{approval.stepId.slice(0, 12)}</code>
        </dd>
      </div>
    </dl>

    {#if approval.action.kind === 'patch'}
      <section class="action-detail" aria-labelledby="approval-patch-heading">
        <h4 id="approval-patch-heading">Exakter Patch-Scope</h4>
        <p>{approval.action.patch.rationale}</p>
        <ul>
          {#each approval.action.patch.files as file, index (`${file.operation}-${index}`)}
            <li>
              <strong>{file.operation}</strong>
              <code
                >{file.sourcePath?.displayPath ?? '∅'} → {file.targetPath?.displayPath ?? '∅'}</code
              >
              {#if file.sourcePath !== null}
                <small>Quelle (Bytes): <code>{file.sourcePath.pathHex}</code></small>
              {/if}
              {#if file.targetPath !== null}
                <small>Ziel (Bytes): <code>{file.targetPath.pathHex}</code></small>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      {@const process = approval.action.process}
      <section class="action-detail" aria-labelledby="approval-process-heading">
        <h4 id="approval-process-heading">Exakte ProcessSpec</h4>
        <dl>
          <div>
            <dt>argv</dt>
            <dd>
              <ol class="argv">
                {#each [process.executable, ...process.arguments] as argument, index (`${index}-${argument}`)}
                  <li><span>argv[{index}]</span> <code>{JSON.stringify(argument)}</code></li>
                {/each}
              </ol>
            </dd>
          </div>
          <div>
            <dt>CWD</dt>
            <dd>
              <code
                >{process.workingDirectory.kind === 'root'
                  ? '.'
                  : process.workingDirectory.path.displayPath}</code
              >
            </dd>
          </div>
          <div>
            <dt>Env-Namen</dt>
            <dd>
              {process.environmentAllowlist.length === 0
                ? 'Keine'
                : process.environmentAllowlist.join(', ')}
            </dd>
          </div>
          <div>
            <dt>Timeout</dt>
            <dd>{process.timeoutMillis} ms</dd>
          </div>
          <div>
            <dt>Outputgrenzen</dt>
            <dd>stdout {process.stdoutLimit} B · stderr {process.stderrLimit} B</dd>
          </div>
          <div>
            <dt>Modus</dt>
            <dd>{process.executionMode}</dd>
          </div>
          <div>
            <dt>Process-Klasse</dt>
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
          <div>
            <dt>Specification-ID</dt>
            <dd><code>{process.specificationId}</code></dd>
          </div>
        </dl>
      </section>
    {/if}

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
          /> Einmalig für genau diese Aktion und diesen Scope erlauben</label
        >
        <label
          ><input
            type="radio"
            name={`approval-${taskId}`}
            value="deny"
            bind:group={choice}
            disabled={!approval.canDeny || controlling}
          /> Ablehnen und Schritt blockieren</label
        >
      </fieldset>
      <button type="button" disabled={choice === null || controlling} onclick={confirmPending}
        >Ausgewählte Entscheidung bestätigen</button
      >
    {:else if approval.status === 'active'}
      <p class="bounded-note">
        Die Freigabe ist gespeichert, aber noch ungenutzt. Erst „Agent fortsetzen“ startet einen
        neuen vom Scheduler verwalteten Versuch.
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
        Die Ablehnung ist dauerhaft. Nutze die Run-Steuerung für Replan oder Cancel.
      </p>
    {:else}
      <p class="bounded-note">Diese Anfrage ist abgeschlossen oder nicht mehr verwendbar.</p>
    {/if}
  {:else}
    <p class="empty-state">Der Approval-Zustand ist nicht verfügbar.</p>
  {/if}

  {#if message !== null}<p class="success-state" role="status" aria-live="polite">{message}</p>{/if}
  {#if actionError !== null}<p class="error-state" role="alert">{actionError}</p>{/if}
</section>

<style>
  .approval-center {
    border: 1px solid var(--color-border-soft);
    border-radius: 0.9rem;
    display: grid;
    gap: 0.9rem;
    padding: 1rem;
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
    border: 1px solid var(--color-border-soft);
    border-radius: 999px;
    padding: 0.3rem 0.65rem;
  }
  .approval-facts,
  .action-detail dl {
    display: grid;
    gap: 0.55rem;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    margin: 0;
  }
  .approval-facts div,
  .action-detail dl div {
    background: color-mix(in srgb, var(--color-surface-raised) 88%, var(--color-info-surface));
    border-radius: 0.55rem;
    display: grid;
    gap: 0.2rem;
    padding: 0.65rem;
  }
  dt {
    color: var(--color-muted);
    font-size: 0.78rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  code {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .action-detail {
    display: grid;
    gap: 0.65rem;
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
    border: 1px solid var(--color-border-soft);
    border-radius: 0.7rem;
    display: grid;
    gap: 0.7rem;
    padding: 0.8rem;
  }
  .decision-options label {
    align-items: start;
    display: grid;
    gap: 0.55rem;
    grid-template-columns: auto 1fr;
  }
  .decision-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }
  button {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-strong);
    border-radius: 0.55rem;
    color: inherit;
    cursor: pointer;
    padding: 0.55rem 0.8rem;
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

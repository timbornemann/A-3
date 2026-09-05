<script lang="ts">
  import {
    queryAgentInspection,
    queryAgentInspectionLog,
    type AgentCriterionInspectionV1,
    type AgentDiffFileV1,
    type AgentInspectionLogResponseV1,
    type AgentInspectionResponseV1,
    type AgentInspectionStreamV1,
    type AgentProcessInspectionV1,
    type AgentVerificationEvidenceV1,
    type AgentVerificationStepV1,
  } from './agent-inspection';
  import VirtualDiffRows from './VirtualDiffRows.svelte';

  interface Props {
    taskId: string;
    loader?: (taskId: string) => Promise<AgentInspectionResponseV1>;
    logLoader?: (
      taskId: string,
      revision: string,
      inspectionId: string,
      stream: AgentInspectionStreamV1,
      offset: number,
    ) => Promise<AgentInspectionLogResponseV1>;
  }

  type InspectionView =
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: AgentInspectionResponseV1['result'] };
  type DiffLayout = 'unified' | 'sideBySide';
  const PROCESS_STREAMS: AgentInspectionStreamV1[] = ['stdout', 'stderr'];
  type LogView =
    | { kind: 'idle' }
    | { kind: 'loading'; text: string }
    | {
        kind: 'available';
        nextOffset: number | null;
        pageTruncated: boolean;
        sourceTruncated: boolean;
        text: string;
      }
    | { kind: 'changed' }
    | { kind: 'unavailable' }
    | { kind: 'error' };

  let {
    taskId,
    loader = queryAgentInspection,
    logLoader = queryAgentInspectionLog,
  }: Props = $props();
  let view = $state<InspectionView>({ kind: 'loading' });
  let layout = $state<DiffLayout>('unified');
  let logs = $state<Record<string, LogView>>({});
  let requestSequence = 0;
  let observedTaskId = '';

  $effect(() => {
    if (taskId !== observedTaskId) {
      observedTaskId = taskId;
      void loadInspection();
    }
  });

  async function loadInspection(): Promise<void> {
    const request = ++requestSequence;
    view = { kind: 'loading' };
    logs = {};
    try {
      const response = await loader(taskId);
      if (request === requestSequence && taskId === observedTaskId) {
        view = { kind: 'result', result: response.result };
      }
    } catch {
      if (request === requestSequence) view = { kind: 'error' };
    }
  }

  function logKey(inspectionId: string, stream: AgentInspectionStreamV1): string {
    return `${inspectionId}:${stream}`;
  }

  function logView(inspectionId: string, stream: AgentInspectionStreamV1): LogView {
    return logs[logKey(inspectionId, stream)] ?? { kind: 'idle' };
  }

  async function loadLog(
    process: AgentProcessInspectionV1,
    stream: AgentInspectionStreamV1,
    offset: number,
  ): Promise<void> {
    if (
      view.kind !== 'result' ||
      view.result.status !== 'available' ||
      view.result.inspection.inspectionRevision === null
    ) {
      return;
    }
    const selectedTaskId = taskId;
    const selectedRevision = view.result.inspection.inspectionRevision;
    const key = logKey(process.inspectionId, stream);
    const prior = logs[key];
    const previousText = prior?.kind === 'available' ? prior.text : '';
    logs[key] = { kind: 'loading', text: previousText };
    try {
      const response = await logLoader(
        selectedTaskId,
        selectedRevision,
        process.inspectionId,
        stream,
        offset,
      );
      if (
        taskId !== selectedTaskId ||
        view.kind !== 'result' ||
        view.result.status !== 'available' ||
        view.result.inspection.inspectionRevision !== selectedRevision
      ) {
        return;
      }
      if (response.result.status === 'inspectionChanged') {
        logs[key] = { kind: 'changed' };
      } else if (response.result.status !== 'available') {
        logs[key] = { kind: 'unavailable' };
      } else {
        const page = response.result.page;
        logs[key] = {
          kind: 'available',
          nextOffset: page.nextOffset,
          pageTruncated: page.pageTruncated,
          sourceTruncated: page.sourceTruncated,
          text: `${previousText}${page.text}`,
        };
      }
    } catch {
      if (
        taskId === selectedTaskId &&
        view.kind === 'result' &&
        view.result.status === 'available' &&
        view.result.inspection.inspectionRevision === selectedRevision
      ) {
        logs[key] = { kind: 'error' };
      }
    }
  }

  function processKindLabel(kind: AgentProcessInspectionV1['kind']): string {
    return {
      build: 'Build',
      command: 'Command',
      diagnostic: 'Diagnostic',
      format: 'Format',
      lint: 'Lint',
      test: 'Test',
    }[kind];
  }

  function processTerminationLabel(process: AgentProcessInspectionV1): string {
    const termination = process.termination;
    if (termination.kind === 'timedOut') return 'Timeout';
    if (termination.kind === 'cancelled') return 'Abgebrochen';
    if (termination.success) return 'Erfolgreich beendet';
    return termination.code === null ? 'Fehlgeschlagen' : `Exit ${termination.code}`;
  }

  function operationLabel(file: AgentDiffFileV1): string {
    return { add: 'Neu', delete: 'Gelöscht', move: 'Verschoben', update: 'Geändert' }[
      file.operation
    ];
  }

  function attributionLabel(file: AgentDiffFileV1): string {
    return {
      appliedAgent: 'Vom Agenten angewendet',
      external: 'Extern beobachtet',
      proposedAgent: 'Vom Agenten vorgeschlagen',
      unattributed: 'Urheber nicht zuverlässig bestimmt',
    }[file.attribution];
  }

  function criterionStateLabel(criterion: AgentCriterionInspectionV1): string {
    return {
      failed: 'Fehlgeschlagen',
      missing: 'Kein aktiver Step zugeordnet',
      pending: 'Ausstehend',
      proven: 'Bewiesen',
      stale: 'Veraltet',
    }[criterion.proofState];
  }

  function stepStatusLabel(step: AgentVerificationStepV1): string {
    return {
      awaitingApproval: 'Wartet auf Freigabe',
      blocked: 'Blockiert',
      cancelled: 'Abgebrochen',
      completed: 'Abgeschlossen',
      failed: 'Fehlgeschlagen',
      inProgress: 'In Arbeit',
      pending: 'Ausstehend',
      ready: 'Bereit',
      stale: 'Veraltet',
      verifying: 'Wird verifiziert',
    }[step.status];
  }

  function evidenceLabel(evidence: AgentVerificationEvidenceV1): string {
    return {
      command: 'Command-Evidence',
      diagnostic: 'Diagnostic-Evidence',
      diffInvariant: 'Diff-Evidence',
      test: 'Test-Evidence',
      userConfirm: 'User-Confirmation',
    }[evidence.method];
  }
</script>

<section class="inspection-panel" aria-labelledby="agent-inspection-heading">
  <header class="panel-heading">
    <div>
      <p class="eyebrow">Ergebnisse prüfen</p>
      <h3 id="agent-inspection-heading">Änderungen & Prüfungen</h3>
    </div>
    <button class="secondary" type="button" onclick={loadInspection}>Aktualisieren</button>
  </header>

  {#if view.kind === 'loading'}
    <p class="empty-state">Änderungen und Prüfergebnisse werden geladen …</p>
  {:else if view.kind === 'error'}
    <div class="notice error">
      <p>Die Inspektion konnte nicht sicher geladen werden.</p>
      <button type="button" onclick={loadInspection}>Erneut laden</button>
    </div>
  {:else if view.result.status === 'inspectionChanged'}
    <div class="notice warning">
      <p>Der Task hat sich während des Lesens geändert. Die alte Ansicht wird nicht gemischt.</p>
      <button type="button" onclick={loadInspection}>Aktuellen Stand laden</button>
    </div>
  {:else if view.result.status !== 'available'}
    <p class="empty-state">
      {view.result.status === 'ledgerUnavailable'
        ? 'Für diese Aufgabe liegt noch kein prüfbarer Arbeitsplan vor.'
        : view.result.status === 'goalRevisionMismatch'
          ? 'Goal Contract und Ledger müssen vor der Inspektion neu abgeglichen werden.'
          : 'Für diesen Task ist keine Inspektion verfügbar.'}
    </p>
  {:else}
    {@const inspection = view.result.inspection}
    {@const mustCriteria = inspection.verification.criteria.filter(
      (criterion) => criterion.requirement === 'must',
    )}
    {@const doneProven =
      mustCriteria.length > 0 &&
      mustCriteria.every((criterion) => criterion.proofState === 'proven')}
    <section class="verification-summary" aria-labelledby="acceptance-proof-heading">
      <div class="subheading">
        <div>
          <h4 id="acceptance-proof-heading">Abschlussprüfung</h4>
        </div>
      </div>
      <p class:proof-summary={doneProven} class="done-proof-state">
        {doneProven
          ? 'Abschluss belegt · alle Muss-Kriterien sind aktuell nachgewiesen.'
          : 'Abschluss noch nicht belegt · mindestens ein Muss-Kriterium ist noch nicht aktuell nachgewiesen.'}
      </p>
      <ul class="criterion-list">
        {#each inspection.verification.criteria as criterion (criterion.criterionId)}
          <li class:stale={criterion.proofState === 'stale'}>
            <div class="criterion-title">
              <strong>{criterion.requirement === 'must' ? 'Muss' : 'Soll'}</strong>
              <span class:proof={criterion.proofState === 'proven'}
                >{criterionStateLabel(criterion)}</span
              >
            </div>
            <p>{criterion.statement}</p>
            {#if criterion.proofs.length > 0}
              <details class="technical-details">
                <summary>Nachweise anzeigen · {criterion.proofs.length}</summary>
                <ul class="proof-list" aria-label="Exakte Beweise">
                  {#each criterion.proofs as proof (proof.stepId)}
                    <li>
                      <span>Schritt <code>{proof.stepId}</code></span>
                      {#each proof.evidenceIds as evidenceId (evidenceId)}
                        <span>Nachweis <code>{evidenceId}</code></span>
                      {/each}
                    </li>
                  {/each}
                </ul>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
    </section>

    <section class="diff-section" aria-labelledby="patch-heading">
      <div class="subheading">
        <div>
          <h4 id="patch-heading">Dateiänderungen</h4>
        </div>
        {#if inspection.patch !== null}
          <div class="segmented" aria-label="Diff-Darstellung">
            <button
              type="button"
              class:active={layout === 'unified'}
              aria-pressed={layout === 'unified'}
              onclick={() => (layout = 'unified')}>Untereinander</button
            >
            <button
              type="button"
              class:active={layout === 'sideBySide'}
              aria-pressed={layout === 'sideBySide'}
              onclick={() => (layout = 'sideBySide')}>Nebeneinander</button
            >
          </div>
        {/if}
      </div>
      {#if inspection.patch === null}
        <p class="empty-state">
          Kein exakter flüchtiger Patch ist vorhanden. Nach einem Neustart bleiben nur dauerhafte
          Evidence-Metadaten erhalten.
        </p>
      {:else}
        <p class="anchor">{inspection.patch.files.length} Datei(en)</p>
        <div class="file-list">
          {#each inspection.patch.files as file, index (`${file.sourcePath?.pathHex ?? ''}:${file.targetPath?.pathHex ?? ''}:${index}`)}
            <article class="diff-file">
              <header>
                <div>
                  <strong>{operationLabel(file)}</strong>
                  <code class="path"
                    >{file.targetPath?.displayPath ?? file.sourcePath?.displayPath}</code
                  >
                  {#if file.operation === 'move'}
                    <span class="move-source">von {file.sourcePath?.displayPath}</span>
                  {/if}
                </div>
                <div class="file-metrics">
                  <span class="added">+{file.addedLines}</span>
                  <span class="removed">−{file.removedLines}</span>
                  <span>{attributionLabel(file)}</span>
                </div>
              </header>
              <details class="path-proof">
                <summary>Exakte Pfadbytes</summary>
                <code>{file.targetPath?.pathHex ?? file.sourcePath?.pathHex}</code>
              </details>
              {#if file.contentTruncated}
                <p class="truncation-warning">
                  Die Vorschau zeigt nur den Anfang dieser Änderung. Der ausgelassene Teil kann hier
                  nicht nachgeladen werden.
                </p>
              {/if}
              {#each file.hunks as hunk, hunkIndex (`${hunk.beforeStart}:${hunk.afterStart}:${hunkIndex}`)}
                <div class="hunk">
                  <div class="hunk-heading">
                    @@ −{hunk.beforeStart},{hunk.beforeCount} +{hunk.afterStart},{hunk.afterCount} @@
                  </div>
                  <VirtualDiffRows rows={hunk.rows} {layout} />
                </div>
              {/each}
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="process-section" aria-labelledby="process-results-heading">
      <div class="subheading">
        <div>
          <h4 id="process-results-heading">Tests & Befehle</h4>
        </div>
      </div>
      {#if inspection.processes.length === 0}
        <p class="empty-state">In dieser App-Sitzung liegen noch keine Befehlsausgaben vor.</p>
      {:else}
        <div class="process-list">
          {#each inspection.processes as process (process.inspectionId)}
            <article class="process-card">
              <header>
                <div>
                  <strong>{processKindLabel(process.kind)}</strong>
                  <span>{processTerminationLabel(process)}</span>
                </div>
                <span>{process.durationMillis} ms</span>
              </header>
              {#each PROCESS_STREAMS as stream (stream)}
                {@const summary = process[stream]}
                {@const selectedLog = logView(process.inspectionId, stream)}
                <section class="stream" aria-label={stream}>
                  <div class="stream-heading">
                    <strong>{stream}</strong>
                    <span>{summary.observedBytes} Bytes beobachtet</span>
                  </div>
                  {#if summary.redaction !== null}
                    <p class="redacted">Ausgabe redigiert: {summary.redaction}</p>
                  {:else if summary.retainedBytes === '0'}
                    <p class="empty-state">Keine gespeicherte Ausgabe.</p>
                  {:else if selectedLog.kind === 'idle'}
                    <button
                      type="button"
                      aria-label={`${stream}-Log gezielt laden`}
                      onclick={() => loadLog(process, stream, 0)}>Log gezielt laden</button
                    >
                  {:else if selectedLog.kind === 'loading'}
                    {#if selectedLog.text.length > 0}<pre>{selectedLog.text}</pre>{/if}
                    <p>Lade nächste sichere Seite …</p>
                  {:else if selectedLog.kind === 'available'}
                    <pre>{selectedLog.text}</pre>
                    {#if selectedLog.pageTruncated && selectedLog.nextOffset !== null}
                      <p class="truncation-note">
                        Weitere gespeicherte Ausgabe kann geladen werden.
                      </p>
                      <button
                        type="button"
                        aria-label={`Nächste ${stream}-Logseite laden`}
                        onclick={() => loadLog(process, stream, selectedLog.nextOffset ?? 0)}
                        >Nächste Logseite laden</button
                      >
                    {/if}
                  {:else if selectedLog.kind === 'changed'}
                    <p class="truncation-warning">Die Inspection hat sich geändert.</p>
                    <button type="button" onclick={loadInspection}>Aktuellen Stand laden</button>
                  {:else}
                    <p class="truncation-warning">Diese Logseite ist nicht mehr verfügbar.</p>
                  {/if}
                  {#if summary.sourceTruncated || (selectedLog.kind === 'available' && selectedLog.sourceTruncated)}
                    <p class="source-truncated">
                      Ausgabe oberhalb des Speicherlimits wurde dauerhaft verworfen und kann nicht
                      nachgeladen werden.
                    </p>
                  {/if}
                </section>
              {/each}
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="evidence-section" aria-labelledby="step-evidence-heading">
      <div class="subheading">
        <div>
          <h4 id="step-evidence-heading">Schritte & Nachweise</h4>
        </div>
      </div>
      <div class="step-list">
        {#each inspection.verification.steps as step (step.stepId)}
          <article class:stale={step.status === 'stale'} class="verification-step">
            <header>
              <div>
                <strong>{step.intendedOutcome}</strong>
                <span>{stepStatusLabel(step)}</span>
              </div>
            </header>
            {#if step.staleCause !== null}
              <p class="source-truncated">
                Prüfung ist veraltet: {step.staleCause.kind === 'dependency'
                  ? 'ein abhängiger Schritt ist nicht mehr aktuell.'
                  : `${step.staleCause.evidenceIds.length} Nachweis(e) sind nicht mehr aktuell.`}
              </p>
            {/if}
            {#each step.attempts as attempt (attempt.number)}
              <details open={attempt.number === step.attempts.length}>
                <summary>
                  Versuch {attempt.number} · {attempt.outcome.status === 'passed'
                    ? 'Prüfung bestanden'
                    : 'Prüfung fehlgeschlagen'}
                </summary>
                <div class="evidence-list">
                  {#each attempt.evidence as evidence (evidence.evidenceId)}
                    <article
                      class:stale={evidence.freshness.status === 'stale'}
                      class="evidence-card"
                    >
                      <header>
                        <strong>{evidenceLabel(evidence)}</strong>
                        <span>
                          {evidence.evaluation.status === 'passed' ? 'Bestanden' : 'Fehlgeschlagen'}
                          · {evidence.freshness.status === 'fresh' ? 'Aktuell' : 'Veraltet'}
                        </span>
                      </header>
                      <details class="technical-details">
                        <summary>Nachweisdetails</summary>
                        <p>Schritt <code>{step.stepId}</code></p>
                        <p>Nachweis <code>{evidence.evidenceId}</code></p>
                      </details>
                      {#if evidence.detail.kind === 'test'}
                        <p>
                          {evidence.detail.passed} bestanden · {evidence.detail.failed} fehlgeschlagen
                          ·
                          {evidence.detail.ignored} ignoriert
                        </p>
                        {#if evidence.detail.casesTruncated}<p class="truncation-note">
                            Weitere strukturierte Testfälle wurden in dieser Ansicht begrenzt.
                          </p>{/if}
                      {:else if evidence.detail.kind === 'diagnostic'}
                        <p>
                          {evidence.detail.errors} Fehler · {evidence.detail.warnings} Warnungen
                        </p>
                      {:else if evidence.detail.kind === 'diff'}
                        <p>{evidence.detail.changedPaths.length} tatsächlich geänderte Pfade</p>
                        {#if evidence.detail.changedPaths.length > 0}
                          <details class="evidence-paths">
                            <summary>Exakte geänderte Pfade</summary>
                            <ul>
                              {#each evidence.detail.changedPaths as path (path.pathHex)}
                                <li>
                                  <code>{path.displayPath}</code>
                                  <code class="path-bytes">{path.pathHex}</code>
                                </li>
                              {/each}
                            </ul>
                          </details>
                        {/if}
                      {:else if evidence.detail.kind === 'command'}
                        <p>{evidence.detail.command.durationMillis} ms</p>
                      {:else}
                        <p>Vom Benutzer bestätigter Scope <code>{evidence.detail.scopeId}</code></p>
                      {/if}
                    </article>
                  {/each}
                </div>
              </details>
            {/each}
          </article>
        {/each}
      </div>
    </section>
    <details class="technical-details">
      <summary>Technischer Prüfstand</summary>
      <p>
        Zielrevision {inspection.verification.goalRevision} · Planrevision {inspection.verification
          .ledgerRevision}
      </p>
      <p>Veröffentlichter Stand <code>{inspection.verification.publishedSnapshotId}</code></p>
      {#if inspection.patch !== null}<p>
          Änderungsbasis <code>{inspection.patch.snapshotId}</code>
        </p>{/if}
    </details>
  {/if}
</section>

<style>
  .inspection-panel {
    display: grid;
    gap: 1rem;
    padding: 0.3rem 0;
    min-width: 0;
  }

  .panel-heading,
  .subheading,
  .diff-file > header,
  .process-card > header,
  .verification-step > header,
  .evidence-card > header,
  .stream-heading,
  .criterion-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8rem;
  }

  h3,
  h4,
  p {
    margin: 0;
  }

  h4 {
    font-size: 1rem;
  }

  .eyebrow {
    color: var(--color-muted);
    font-size: 0.7rem;
    font-weight: 750;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  .verification-summary,
  .diff-section,
  .process-section,
  .evidence-section {
    display: grid;
    gap: 0.75rem;
    padding-top: 0.9rem;
    border-top: 1px solid var(--color-border-soft);
  }

  .criterion-list,
  .proof-list {
    display: grid;
    gap: 0.55rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .criterion-list > li,
  .diff-file,
  .process-card,
  .verification-step,
  .evidence-card {
    min-width: 0;
    padding: 0.9rem 0;
    border: 0;
    border-bottom: 1px solid var(--color-border-soft);
    background: transparent;
  }

  .criterion-list > li.stale,
  .verification-step.stale,
  .evidence-card.stale {
    border-inline-start: 2px solid var(--color-warning-strong);
    padding-inline: 0.7rem;
    background: var(--color-warning-surface);
  }

  .criterion-title span,
  .file-metrics span,
  .anchor,
  .move-source,
  .stream-heading span,
  .process-card header span,
  .verification-step header span,
  .evidence-card header span {
    color: var(--color-muted);
    font-size: 0.78rem;
  }

  .criterion-title .proof {
    color: var(--color-positive);
    font-weight: 700;
  }

  .done-proof-state {
    padding: 0.65rem 0.75rem;
    border-inline-start: 2px solid var(--color-warning-strong);
    background: var(--color-warning-surface);
  }

  .done-proof-state.proof-summary {
    border-color: var(--color-positive);
    background: var(--color-positive-surface);
    color: var(--color-positive);
  }

  .proof-list {
    margin-top: 0.55rem;
  }

  .proof-list li {
    display: grid;
    gap: 0.25rem;
    padding: 0.4rem 0;
    font-size: 0.78rem;
  }

  code {
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
  }

  .secondary,
  .segmented button,
  .stream button,
  .notice button {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface-raised);
    color: inherit;
    cursor: pointer;
    padding: 0.45rem 0.7rem;
  }

  .segmented {
    display: flex;
  }

  .segmented button {
    border-radius: 0;
  }

  .segmented button:first-child {
    border-radius: var(--radius-control) 0 0 var(--radius-control);
  }

  .segmented button:last-child {
    border-radius: 0 var(--radius-control) var(--radius-control) 0;
  }

  .segmented button.active {
    background: var(--color-code);
    color: var(--color-on-code);
  }

  .file-list,
  .process-list,
  .step-list,
  .evidence-list {
    display: grid;
    gap: 0;
  }

  .path {
    display: block;
    margin-top: 0.2rem;
    font-size: 0.9rem;
  }

  .file-metrics {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.45rem;
  }

  .file-metrics .added {
    color: var(--color-positive);
  }

  .file-metrics .removed {
    color: var(--color-danger);
  }

  .path-proof {
    margin: 0.5rem 0;
    color: var(--color-muted);
    font-size: 0.72rem;
  }

  .evidence-paths ul {
    display: grid;
    gap: 0.35rem;
    margin: 0.45rem 0 0;
    padding: 0;
    list-style: none;
  }

  .evidence-paths li {
    display: grid;
    gap: 0.15rem;
  }

  .path-bytes {
    color: var(--color-muted);
    font-size: 0.7rem;
  }

  .hunk {
    margin-top: 0.65rem;
    overflow: auto;
    border-block: 1px solid var(--color-border-soft);
    background: var(--color-surface-subtle);
  }

  .hunk-heading {
    padding: 0.35rem 0.6rem;
    background: var(--color-surface-muted);
    color: var(--color-neutral);
    font:
      0.75rem ui-monospace,
      SFMono-Regular,
      Consolas,
      monospace;
  }

  .truncation-warning,
  .source-truncated,
  .truncation-note,
  .redacted,
  .notice {
    margin-top: 0.55rem;
    padding: 0.6rem;
    border-radius: var(--radius-control);
    background: var(--color-warning-surface);
    color: var(--color-warning);
    font-size: 0.82rem;
  }

  .source-truncated,
  .redacted {
    background: var(--color-danger-surface);
    color: var(--color-danger);
  }

  .notice.error {
    background: var(--color-danger-surface);
  }

  .process-card,
  .verification-step {
    display: grid;
    gap: 0.65rem;
  }

  .stream {
    padding-top: 0.6rem;
    border-top: 1px solid var(--color-border-soft);
  }

  pre {
    max-height: 18rem;
    overflow: auto;
    padding: 0.7rem;
    border-radius: var(--radius-control);
    background: var(--color-code);
    color: var(--color-on-code);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  details summary {
    cursor: pointer;
    min-height: var(--control-min-size);
    display: flex;
    align-items: center;
  }

  details summary::before {
    content: '+';
    margin-right: 0.5rem;
  }
  details[open] > summary::before {
    content: '−';
  }
  .technical-details {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .technical-details p {
    margin-block: 0.4rem;
  }
  .evidence-card {
    padding-inline-start: 0.8rem;
    border-inline-start: 1px solid var(--color-border-soft);
  }
  button,
  details summary {
    transition:
      background var(--motion-fast, 120ms) var(--ease-out, ease-out),
      color var(--motion-fast, 120ms) var(--ease-out, ease-out);
  }

  .empty-state {
    color: var(--color-muted);
    font-size: 0.86rem;
  }

  @media (max-width: 760px) {
    .panel-heading,
    .subheading,
    .diff-file > header,
    .verification-step > header {
      align-items: flex-start;
      flex-direction: column;
    }

    .file-metrics {
      justify-content: flex-start;
    }
  }
</style>

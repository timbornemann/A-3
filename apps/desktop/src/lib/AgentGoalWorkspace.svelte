<script lang="ts">
  import {
    createAgentGoal,
    queryAgentGoal,
    reviseAgentGoal,
    type AgentGoalContractV1,
    type AgentGoalDraftInputV1,
    type AgentGoalMutationResponseV1,
    type AgentGoalResponseV1,
  } from './agent-goal';
  import { agentGoalRecoveryMessage } from './command-error';
  import GoalTextList from './GoalTextList.svelte';
  import {
    queryTaskLensTasks,
    queryTaskLensTask,
    type TaskLensStepV1,
    type TaskLensTaskResponseV1,
    type TaskLensTaskSummaryV1,
    type TaskLensTasksResponseV1,
  } from './task-lens';

  interface Props {
    activeProject: boolean;
    goalCreator?: (draft: AgentGoalDraftInputV1) => Promise<AgentGoalMutationResponseV1>;
    goalLoader?: (taskId: string) => Promise<AgentGoalResponseV1>;
    goalReviser?: (
      taskId: string,
      expectedRevision: number,
      reason: string,
      draft: AgentGoalDraftInputV1,
    ) => Promise<AgentGoalMutationResponseV1>;
    ledgerLoader?: (query: { taskId: string }) => Promise<TaskLensTaskResponseV1>;
    tasksLoader?: () => Promise<TaskLensTasksResponseV1>;
  }

  type TaskView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'available'; tasks: TaskLensTaskSummaryV1[]; truncated: boolean }
    | { kind: 'noProject' }
    | { kind: 'error' };
  type GoalView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'available'; goal: AgentGoalContractV1 }
    | { kind: 'notFound' }
    | { kind: 'error' };
  type EditorMode = 'closed' | 'create' | 'revise';
  type LedgerView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: TaskLensTaskResponseV1['result'] };

  let {
    activeProject,
    goalCreator = createAgentGoal,
    goalLoader = queryAgentGoal,
    goalReviser = reviseAgentGoal,
    ledgerLoader = queryTaskLensTask,
    tasksLoader = queryTaskLensTasks,
  }: Props = $props();

  let taskView = $state<TaskView>({ kind: 'idle' });
  let goalView = $state<GoalView>({ kind: 'idle' });
  let ledgerView = $state<LedgerView>({ kind: 'idle' });
  let selectedTaskId = $state('');
  let editorMode = $state<EditorMode>('closed');
  let draft = $state<AgentGoalDraftInputV1>(emptyDraft());
  let revisionReason = $state('');
  let submitting = $state(false);
  let actionMessage = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let observedProject = false;
  let taskRequest = 0;
  let goalRequest = 0;
  let ledgerRequest = 0;
  let currentLedgerStep = $derived(
    ledgerView.kind === 'result' && ledgerView.result.status === 'available'
      ? selectCurrentStep(ledgerView.result.steps)
      : null,
  );

  $effect(() => {
    if (activeProject && !observedProject) {
      observedProject = true;
      void loadTasks();
    } else if (!activeProject && observedProject) {
      observedProject = false;
      resetWorkspace();
    }
  });

  async function loadTasks(preferredTaskId?: string): Promise<void> {
    const request = ++taskRequest;
    taskView = { kind: 'loading' };
    actionError = null;
    try {
      const response = await tasksLoader();
      if (request !== taskRequest) return;
      if (response.result.status === 'noProject') {
        taskView = { kind: 'noProject' };
        resetSelection();
        return;
      }
      taskView = {
        kind: 'available',
        tasks: response.result.tasks,
        truncated: response.result.truncated,
      };
      const nextTask =
        response.result.tasks.find((task) => task.taskId === preferredTaskId) ??
        response.result.tasks.find((task) => task.taskId === selectedTaskId) ??
        response.result.tasks[0];
      if (nextTask === undefined) {
        resetSelection();
        startCreate();
        return;
      }
      selectedTaskId = nextTask.taskId;
      editorMode = 'closed';
      await loadGoal(nextTask.taskId);
    } catch {
      if (request === taskRequest) {
        taskView = { kind: 'error' };
        resetSelection();
      }
    }
  }

  async function loadGoal(taskId: string): Promise<void> {
    const request = ++goalRequest;
    goalView = { kind: 'loading' };
    actionError = null;
    try {
      const response = await goalLoader(taskId);
      if (request !== goalRequest || taskId !== selectedTaskId) return;
      if (response.result.status === 'noProject') {
        goalView = { kind: 'idle' };
      } else if (response.result.status === 'taskNotFound') {
        goalView = { kind: 'notFound' };
      } else {
        goalView = { kind: 'available', goal: response.result.goal };
        await loadLedger(taskId);
      }
    } catch {
      if (request === goalRequest) goalView = { kind: 'error' };
    }
  }

  async function loadLedger(taskId: string): Promise<void> {
    const request = ++ledgerRequest;
    ledgerView = { kind: 'loading' };
    try {
      const response = await ledgerLoader({ taskId });
      if (request !== ledgerRequest || taskId !== selectedTaskId) return;
      ledgerView = { kind: 'result', result: response.result };
    } catch {
      if (request === ledgerRequest) ledgerView = { kind: 'error' };
    }
  }

  function chooseTask(event: Event): void {
    const target = event.currentTarget;
    if (!(target instanceof HTMLSelectElement)) return;
    selectedTaskId = target.value;
    editorMode = 'closed';
    actionMessage = null;
    if (selectedTaskId.length > 0) void loadGoal(selectedTaskId);
  }

  function startCreate(): void {
    editorMode = 'create';
    draft = emptyDraft();
    revisionReason = '';
    actionError = null;
    actionMessage = null;
  }

  function startRevision(): void {
    if (goalView.kind !== 'available') return;
    editorMode = 'revise';
    draft = draftFromGoal(goalView.goal);
    revisionReason = '';
    actionError = null;
    actionMessage = null;
  }

  function closeEditor(): void {
    editorMode = 'closed';
    actionError = null;
  }

  function addCriterion(): void {
    if (draft.acceptanceCriteria.length >= 64) return;
    draft.acceptanceCriteria.push({ criterionId: null, requirement: 'must', statement: '' });
  }

  function removeCriterion(index: number): void {
    if (draft.acceptanceCriteria.length === 1) return;
    draft.acceptanceCriteria = draft.acceptanceCriteria.filter(
      (_, criterionIndex) => criterionIndex !== index,
    );
  }

  async function submitGoal(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (submitting) return;
    submitting = true;
    actionError = null;
    actionMessage = null;
    try {
      const response =
        editorMode === 'revise' && goalView.kind === 'available'
          ? await goalReviser(goalView.goal.taskId, goalView.goal.revision, revisionReason, draft)
          : await goalCreator(draft);
      selectedTaskId = response.goal.taskId;
      goalView = { goal: response.goal, kind: 'available' };
      editorMode = 'closed';
      actionMessage =
        response.goal.revision === 1
          ? 'Goal Contract dauerhaft angelegt.'
          : `Revision ${response.goal.revision} dauerhaft angehängt.`;
      await loadTasks(response.goal.taskId);
    } catch (error) {
      actionError = agentGoalRecoveryMessage(error);
    } finally {
      submitting = false;
    }
  }

  function resetSelection(): void {
    selectedTaskId = '';
    goalView = { kind: 'idle' };
    ledgerRequest += 1;
    ledgerView = { kind: 'idle' };
  }

  function resetWorkspace(): void {
    taskRequest += 1;
    goalRequest += 1;
    taskView = { kind: 'idle' };
    resetSelection();
    editorMode = 'closed';
    actionMessage = null;
    actionError = null;
  }

  function emptyDraft(): AgentGoalDraftInputV1 {
    return {
      acceptanceCriteria: [{ criterionId: null, requirement: 'must', statement: '' }],
      constraints: [],
      nonGoals: [],
      objective: '',
      successVerification: '',
      userDecisions: [],
    };
  }

  function draftFromGoal(goal: AgentGoalContractV1): AgentGoalDraftInputV1 {
    return {
      acceptanceCriteria: goal.acceptanceCriteria.map((criterion) => ({ ...criterion })),
      constraints: [...goal.constraints],
      nonGoals: [...goal.nonGoals],
      objective: goal.objective,
      successVerification: goal.successVerification,
      userDecisions: [...goal.userDecisions],
    };
  }

  function requirementLabel(requirement: 'must' | 'should'): string {
    return requirement === 'must' ? 'Muss' : 'Soll';
  }

  function selectCurrentStep(steps: TaskLensStepV1[]): TaskLensStepV1 | null {
    const priority = ['inProgress', 'awaitingApproval', 'verifying', 'blocked'] as const;
    return (
      priority.map((status) => steps.find((step) => step.status === status)).find(Boolean) ?? null
    );
  }

  function stepStatusLabel(status: TaskLensStepV1['status']): string {
    const labels: Record<TaskLensStepV1['status'], string> = {
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
    };
    return labels[status];
  }
</script>

<section class="agent-goal-workspace" aria-labelledby="agent-workspace-heading">
  <div class="workspace-heading">
    <div>
      <p>Goal Contract</p>
      <h2 id="agent-workspace-heading">Agent Workspace</h2>
    </div>
    {#if activeProject}
      <button type="button" onclick={startCreate}>Neue Aufgabe</button>
    {/if}
  </div>

  {#if !activeProject}
    <p class="empty-state">
      Öffne einen lokalen Worktree, um einen dauerhaften Goal Contract anzulegen.
    </p>
  {:else if taskView.kind === 'loading'}
    <p role="status" aria-live="polite">Dauerhafte Aufgaben werden geladen …</p>
  {:else if taskView.kind === 'error'}
    <div class="error-state" role="alert">
      <p>Die dauerhaften Aufgaben konnten nicht sicher gelesen werden.</p>
      <button type="button" onclick={() => loadTasks()}>Erneut laden</button>
    </div>
  {:else if taskView.kind === 'available'}
    {#if taskView.tasks.length > 0}
      <label class="task-selector">
        Aufgabe
        <select value={selectedTaskId} onchange={chooseTask}>
          {#each taskView.tasks as task (task.taskId)}
            <option value={task.taskId}>R{task.goalRevision} · {task.objective}</option>
          {/each}
        </select>
      </label>
      {#if taskView.truncated}
        <p class="bounded-note">Es werden die ersten 20 stabil geordneten Aufgaben gezeigt.</p>
      {/if}
    {:else}
      <p class="empty-state">Noch keine Aufgabe. Lege den ersten dauerhaften Goal Contract an.</p>
    {/if}
  {/if}

  {#if goalView.kind === 'available'}
    <div class="persistent-anchors">
      <div>
        <span>Aktuelles Ziel · Revision {goalView.goal.revision}</span>
        <h3 id="current-goal-heading">{goalView.goal.objective}</h3>
      </div>
      <div>
        <span>Aktueller Schritt</span>
        <h4 id="current-step-heading">
          {#if currentLedgerStep !== null}
            {currentLedgerStep.intendedOutcome}
          {:else if ledgerView.kind === 'loading'}
            Wird geladen …
          {:else if ledgerView.kind === 'result' && ledgerView.result.status === 'ledgerUnavailable'}
            Noch kein dauerhafter Plan
          {:else if ledgerView.kind === 'result' && ledgerView.result.status === 'goalRevisionMismatch'}
            Replan erforderlich
          {:else}
            Kein Schritt wird gerade ausgeführt
          {/if}
        </h4>
        {#if currentLedgerStep !== null}<strong>{stepStatusLabel(currentLedgerStep.status)}</strong
          >{/if}
      </div>
    </div>
    <section class="task-ledger" aria-labelledby="task-ledger-heading">
      <header>
        <p>Durable Plan</p>
        <h3 id="task-ledger-heading">Task Ledger</h3>
      </header>
      {#if ledgerView.kind === 'loading'}
        <p role="status" aria-live="polite">Task Ledger wird geladen …</p>
      {:else if ledgerView.kind === 'error'}
        <div class="error-state" role="alert">
          <p>Das Task Ledger konnte nicht sicher gelesen werden.</p>
          <button type="button" onclick={() => loadLedger(selectedTaskId)}>Erneut laden</button>
        </div>
      {:else if ledgerView.kind === 'result' && (ledgerView.result.status === 'noProject' || ledgerView.result.status === 'taskNotFound')}
        <p class="error-state" role="alert">
          Aufgabe oder aktiver Worktree haben sich geändert. Lade die dauerhaften Aufgaben neu.
        </p>
      {:else if ledgerView.kind === 'result' && ledgerView.result.status === 'ledgerUnavailable'}
        <p class="empty-state">
          Für diesen Goal Contract wurde noch kein dauerhafter Plan erzeugt.
        </p>
      {:else if ledgerView.kind === 'result' && ledgerView.result.status === 'goalRevisionMismatch'}
        <p class="error-state" role="alert">
          Das Ledger gehört zu Goal-Revision {ledgerView.result.ledgerGoalRevision}; aktuell ist
          Revision {ledgerView.result.currentGoalRevision}. Vor Ausführung ist ein Replan
          erforderlich.
        </p>
      {:else if ledgerView.kind === 'result' && ledgerView.result.status === 'available'}
        <p class="ledger-metadata">
          Ledger R{ledgerView.result.ledgerRevision} · Store {ledgerView.result.ledgerStoreVersion}
        </p>
        <ol class="ledger-steps">
          {#each ledgerView.result.steps as step (step.stepId)}
            <li class:current={step.stepId === currentLedgerStep?.stepId}>
              <span>{stepStatusLabel(step.status)}</span>
              <p>{step.intendedOutcome}</p>
            </li>
          {/each}
        </ol>
      {/if}
    </section>
  {/if}

  {#if goalView.kind === 'loading'}
    <p role="status" aria-live="polite">Aktueller Goal Contract wird geladen …</p>
  {:else if goalView.kind === 'notFound'}
    <p class="error-state" role="alert">Die ausgewählte Aufgabe existiert nicht mehr.</p>
  {:else if goalView.kind === 'error'}
    <div class="error-state" role="alert">
      <p>Der aktuelle Goal Contract konnte nicht sicher gelesen werden.</p>
      <button type="button" onclick={() => loadGoal(selectedTaskId)}>Erneut laden</button>
    </div>
  {:else if goalView.kind === 'available'}
    <article class="goal-contract" aria-labelledby="goal-details-heading">
      <header class="goal-actions">
        <h3 id="goal-details-heading">Vertragsdetails</h3>
        <button type="button" onclick={startRevision}>Neue Revision</button>
      </header>
      <div class="goal-metadata">
        <code>{goalView.goal.taskId}</code>
        <span>Zeitanker {goalView.goal.createdAtUnixMillis} ms</span>
        {#if goalView.goal.revisionReason !== null}
          <span>Änderungsgrund: {goalView.goal.revisionReason}</span>
        {/if}
      </div>
      <div class="goal-columns">
        <section aria-labelledby="criteria-heading">
          <h4 id="criteria-heading">Akzeptanzkriterien</h4>
          <ol class="criteria-list">
            {#each goalView.goal.acceptanceCriteria as criterion (criterion.criterionId)}
              <li>
                <span class:should={criterion.requirement === 'should'}
                  >{requirementLabel(criterion.requirement)}</span
                >
                <p>{criterion.statement}</p>
              </li>
            {/each}
          </ol>
        </section>
        <section aria-labelledby="verification-heading">
          <h4 id="verification-heading">Abschlussprüfung</h4>
          <p>{goalView.goal.successVerification}</p>
        </section>
      </div>
      <div class="boundary-grid">
        <section>
          <h4>Constraints</h4>
          {#if goalView.goal.constraints.length === 0}<p>
              Keine zusätzlichen Constraints.
            </p>{:else}<ul>
              {#each goalView.goal.constraints as item (item)}<li>{item}</li>{/each}
            </ul>{/if}
        </section>
        <section>
          <h4>Non-Goals</h4>
          {#if goalView.goal.nonGoals.length === 0}<p>Keine Non-Goals festgelegt.</p>{:else}<ul>
              {#each goalView.goal.nonGoals as item (item)}<li>{item}</li>{/each}
            </ul>{/if}
        </section>
        <section>
          <h4>Nutzerentscheidungen</h4>
          {#if goalView.goal.userDecisions.length === 0}<p>
              Noch keine expliziten Entscheidungen.
            </p>{:else}<ul>
              {#each goalView.goal.userDecisions as item (item)}<li>{item}</li>{/each}
            </ul>{/if}
        </section>
      </div>
    </article>
  {/if}

  {#if editorMode !== 'closed'}
    <form class="goal-editor" aria-labelledby="goal-editor-heading" onsubmit={submitGoal}>
      <div class="editor-heading">
        <div>
          <p>{editorMode === 'create' ? 'Neue Aufgabe' : 'Immutable Revision'}</p>
          <h3 id="goal-editor-heading">
            {editorMode === 'create' ? 'Goal Contract anlegen' : 'Goal Contract revidieren'}
          </h3>
        </div>
        {#if goalView.kind === 'available' || editorMode === 'revise'}
          <button type="button" onclick={closeEditor}>Editor schließen</button>
        {/if}
      </div>
      {#if editorMode === 'revise'}
        <label>
          Änderungsgrund
          <textarea required maxlength="4096" rows="2" bind:value={revisionReason}></textarea>
        </label>
      {/if}
      <label>
        Ziel
        <textarea required maxlength="16384" rows="4" bind:value={draft.objective}></textarea>
      </label>
      <fieldset class="criteria-editor">
        <legend>Akzeptanzkriterien</legend>
        {#each draft.acceptanceCriteria as criterion, index (criterion.criterionId ?? `new-${index}`)}
          <div class="criterion-editor">
            <label>
              Kriterium {index + 1}
              <textarea required maxlength="4096" rows="2" bind:value={criterion.statement}
              ></textarea>
            </label>
            <label>
              Anforderung
              <select bind:value={criterion.requirement}>
                <option value="must">Muss · blockiert Done</option>
                <option value="should">Soll · bleibt sichtbar</option>
              </select>
            </label>
            <button
              type="button"
              disabled={draft.acceptanceCriteria.length === 1}
              onclick={() => removeCriterion(index)}
            >
              Kriterium entfernen
            </button>
          </div>
        {/each}
        <button
          type="button"
          disabled={draft.acceptanceCriteria.length >= 64}
          onclick={addCriterion}
        >
          Kriterium hinzufügen
        </button>
      </fieldset>
      <GoalTextList
        legend="Constraints"
        itemLabel="Constraint"
        addLabel="Constraint hinzufügen"
        bind:values={draft.constraints}
      />
      <GoalTextList
        legend="Non-Goals"
        itemLabel="Non-Goal"
        addLabel="Non-Goal hinzufügen"
        bind:values={draft.nonGoals}
      />
      <GoalTextList
        legend="Nutzerentscheidungen"
        itemLabel="Entscheidung"
        addLabel="Entscheidung hinzufügen"
        bind:values={draft.userDecisions}
      />
      <label>
        Abschlussprüfung
        <textarea required maxlength="8192" rows="3" bind:value={draft.successVerification}
        ></textarea>
      </label>
      <div class="editor-actions">
        <button class="primary" type="submit" disabled={submitting}>
          {submitting
            ? 'Wird dauerhaft gespeichert …'
            : editorMode === 'create'
              ? 'Goal Contract anlegen'
              : 'Neue Revision anhängen'}
        </button>
        {#if goalView.kind === 'available' || editorMode === 'revise'}
          <button type="button" disabled={submitting} onclick={closeEditor}>Abbrechen</button>
        {/if}
      </div>
    </form>
  {/if}

  {#if actionMessage !== null}
    <p class="success-state" role="status" aria-live="polite">{actionMessage}</p>
  {/if}
  {#if actionError !== null}
    <p class="error-state" role="alert">{actionError}</p>
  {/if}
</section>

<style>
  .agent-goal-workspace {
    background: color-mix(in srgb, var(--surface, #ffffff) 95%, #eef3ff);
    border: 1px solid var(--line, #d8d9df);
    border-radius: 1rem;
    display: grid;
    gap: 1rem;
    padding: clamp(1rem, 2vw, 1.6rem);
  }

  .workspace-heading,
  .editor-heading,
  .goal-actions,
  .editor-actions {
    align-items: start;
    display: flex;
    gap: 1rem;
    justify-content: space-between;
  }

  .workspace-heading p,
  .editor-heading p {
    color: var(--muted, #646b79);
    font-size: 0.78rem;
    font-weight: 800;
    letter-spacing: 0.12em;
    margin: 0 0 0.25rem;
    text-transform: uppercase;
  }

  h2,
  h3,
  h4,
  p {
    margin-top: 0;
  }

  .task-selector,
  .goal-editor > label,
  .criterion-editor label {
    display: grid;
    font-weight: 700;
    gap: 0.4rem;
  }

  select,
  textarea {
    box-sizing: border-box;
    font: inherit;
    width: 100%;
  }

  textarea {
    resize: vertical;
  }

  .goal-contract {
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.9rem;
    overflow: clip;
  }

  .persistent-anchors {
    background: var(--surface, #ffffff);
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.9rem;
    box-shadow: 0 0.4rem 1rem rgb(17 39 30 / 8%);
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    padding: 1rem;
    position: sticky;
    top: 0;
    z-index: 3;
  }

  .persistent-anchors span,
  .bounded-note,
  .goal-metadata {
    color: var(--muted, #646b79);
    font-size: 0.85rem;
  }

  .persistent-anchors h3,
  .persistent-anchors h4 {
    margin: 0.3rem 0 0;
  }

  .goal-actions {
    background: var(--surface, #ffffff);
    border-bottom: 1px solid var(--line, #d8d9df);
    padding: 1rem;
  }

  .goal-metadata {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    padding: 0.8rem 1rem 0;
  }

  .goal-metadata code {
    max-width: 100%;
    overflow-wrap: anywhere;
  }

  .goal-columns,
  .boundary-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    padding: 1rem;
  }

  .boundary-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    padding-top: 0;
  }

  .goal-columns section,
  .boundary-grid section {
    background: var(--surface, #ffffff);
    border-radius: 0.7rem;
    padding: 0.9rem;
  }

  .criteria-list {
    display: grid;
    gap: 0.65rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .criteria-list li {
    align-items: start;
    display: grid;
    gap: 0.55rem;
    grid-template-columns: auto minmax(0, 1fr);
  }

  .criteria-list li > span {
    background: #153c70;
    border-radius: 99px;
    color: white;
    font-size: 0.72rem;
    font-weight: 800;
    padding: 0.2rem 0.5rem;
  }

  .criteria-list li > span.should {
    background: #68521a;
  }

  .criteria-list p {
    margin: 0;
  }

  .goal-editor {
    background: var(--surface, #ffffff);
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.9rem;
    display: grid;
    gap: 1rem;
    padding: 1rem;
  }

  .task-ledger {
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.9rem;
    display: grid;
    gap: 0.8rem;
    padding: 1rem;
  }

  .task-ledger header p,
  .persistent-anchors > div > span {
    color: var(--muted, #646b79);
    font-size: 0.78rem;
    font-weight: 800;
    letter-spacing: 0.1em;
    margin: 0 0 0.25rem;
    text-transform: uppercase;
  }

  .task-ledger header h3,
  .persistent-anchors h4 {
    margin: 0;
  }

  .ledger-metadata {
    color: var(--muted, #646b79);
    font-size: 0.85rem;
    margin: 0;
  }

  .ledger-steps {
    display: grid;
    gap: 0.55rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .ledger-steps li {
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.55rem;
    display: grid;
    gap: 0.55rem;
    grid-template-columns: minmax(8rem, auto) minmax(0, 1fr);
    padding: 0.7rem;
  }

  .ledger-steps li.current {
    border-color: #153c70;
  }

  .ledger-steps span {
    font-weight: 700;
  }

  .ledger-steps p {
    margin: 0;
  }

  .criteria-editor {
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.75rem;
    display: grid;
    gap: 0.8rem;
    margin: 0;
    padding: 0.9rem;
  }

  .criterion-editor {
    align-items: end;
    display: grid;
    gap: 0.6rem;
    grid-template-columns: minmax(0, 1fr) minmax(10rem, 0.35fr) auto;
  }

  .primary {
    background: #153c70;
    color: white;
  }

  .success-state {
    background: #e6f5ec;
    border-left: 0.25rem solid #287847;
    padding: 0.8rem;
  }

  .error-state {
    background: #fff0f0;
    border-left: 0.25rem solid #a32d2d;
    padding: 0.8rem;
  }

  .empty-state {
    color: var(--muted, #646b79);
  }

  @media (max-width: 860px) {
    .goal-columns,
    .boundary-grid,
    .criterion-editor {
      grid-template-columns: 1fr;
    }

    .persistent-anchors {
      grid-template-columns: 1fr;
    }

    .ledger-steps li {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 560px) {
    .workspace-heading,
    .editor-heading,
    .goal-actions,
    .editor-actions {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>

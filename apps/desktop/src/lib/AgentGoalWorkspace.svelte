<script lang="ts">
  import {
    queryAgentActivity,
    type AgentActivityEventV1,
    type AgentActivityResponseV1,
    type AgentActivityRunV1,
    type AgentControllerStateV1,
    type AgentSelectedActionV1,
  } from './agent-activity';
  import {
    controlAgentTaskRun,
    queryAgentTaskRecovery,
    type AgentTaskControlActionV1,
    type AgentTaskControlResponseV1,
    type AgentTaskRecoveryResponseV1,
    type AgentTaskRuntimeStateV1,
  } from './agent-control';
  import {
    createAgentGoal,
    queryAgentGoal,
    reviseAgentGoal,
    type AgentGoalContractV1,
    type AgentGoalDraftInputV1,
    type AgentGoalMutationResponseV1,
    type AgentGoalResponseV1,
  } from './agent-goal';
  import {
    queryAgentInspection,
    queryAgentInspectionLog,
    type AgentInspectionLogResponseV1,
    type AgentInspectionResponseV1,
    type AgentInspectionStreamV1,
  } from './agent-inspection';
  import {
    controlAgentApproval,
    queryAgentApproval,
    type AgentApprovalControlActionV1,
    type AgentApprovalControlResponseV1,
    type AgentApprovalResponseV1,
    type AgentApprovalV1,
  } from './agent-approval';
  import AgentApprovalCenter from './AgentApprovalCenter.svelte';
  import AgentInspectionPanel from './AgentInspectionPanel.svelte';
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
    approvalLoader?: (taskId: string) => Promise<AgentApprovalResponseV1>;
    approvalController?: (
      taskId: string,
      approval: AgentApprovalV1,
      action: AgentApprovalControlActionV1,
    ) => Promise<AgentApprovalControlResponseV1>;
    activityLoader?: (taskId: string) => Promise<AgentActivityResponseV1>;
    goalCreator?: (draft: AgentGoalDraftInputV1) => Promise<AgentGoalMutationResponseV1>;
    goalLoader?: (taskId: string) => Promise<AgentGoalResponseV1>;
    goalReviser?: (
      taskId: string,
      expectedRevision: number,
      reason: string,
      draft: AgentGoalDraftInputV1,
    ) => Promise<AgentGoalMutationResponseV1>;
    inspectionLoader?: (taskId: string) => Promise<AgentInspectionResponseV1>;
    inspectionLogLoader?: (
      taskId: string,
      revision: string,
      inspectionId: string,
      stream: AgentInspectionStreamV1,
      offset: number,
    ) => Promise<AgentInspectionLogResponseV1>;
    ledgerLoader?: (query: { taskId: string }) => Promise<TaskLensTaskResponseV1>;
    recoveryLoader?: (taskId: string) => Promise<AgentTaskRecoveryResponseV1>;
    runController?: (
      taskId: string,
      expectedLedgerRevision: number,
      expectedLedgerStoreVersion: string,
      action: AgentTaskControlActionV1,
    ) => Promise<AgentTaskControlResponseV1>;
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
  type ActivityView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: AgentActivityResponseV1['result'] };
  type RecoveryView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'error' }
    | { kind: 'result'; result: AgentTaskRecoveryResponseV1['result'] };

  let {
    activeProject,
    approvalLoader = queryAgentApproval,
    approvalController = controlAgentApproval,
    activityLoader = queryAgentActivity,
    goalCreator = createAgentGoal,
    goalLoader = queryAgentGoal,
    goalReviser = reviseAgentGoal,
    inspectionLoader = queryAgentInspection,
    inspectionLogLoader = queryAgentInspectionLog,
    ledgerLoader = queryTaskLensTask,
    recoveryLoader = queryAgentTaskRecovery,
    runController = controlAgentTaskRun,
    tasksLoader = queryTaskLensTasks,
  }: Props = $props();

  let taskView = $state<TaskView>({ kind: 'idle' });
  let goalView = $state<GoalView>({ kind: 'idle' });
  let ledgerView = $state<LedgerView>({ kind: 'idle' });
  let activityView = $state<ActivityView>({ kind: 'idle' });
  let recoveryView = $state<RecoveryView>({ kind: 'idle' });
  let selectedTaskId = $state('');
  let editorMode = $state<EditorMode>('closed');
  let draft = $state<AgentGoalDraftInputV1>(emptyDraft());
  let revisionReason = $state('');
  let submitting = $state(false);
  let controllingRun = $state(false);
  let actionMessage = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let observedProject = false;
  let taskRequest = 0;
  let goalRequest = 0;
  let ledgerRequest = 0;
  let activityRequest = 0;
  let recoveryRequest = 0;
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
        await Promise.all([loadLedger(taskId), loadActivity(taskId), loadRecovery(taskId)]);
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

  async function loadActivity(taskId: string): Promise<void> {
    const request = ++activityRequest;
    activityView = { kind: 'loading' };
    try {
      const response = await activityLoader(taskId);
      if (request !== activityRequest || taskId !== selectedTaskId) return;
      activityView = { kind: 'result', result: response.result };
    } catch {
      if (request === activityRequest) activityView = { kind: 'error' };
    }
  }

  async function loadRecovery(taskId: string): Promise<void> {
    const request = ++recoveryRequest;
    recoveryView = { kind: 'loading' };
    try {
      const response = await recoveryLoader(taskId);
      if (request !== recoveryRequest || taskId !== selectedTaskId) return;
      recoveryView = { kind: 'result', result: response.result };
    } catch {
      if (request === recoveryRequest) recoveryView = { kind: 'error' };
    }
  }

  async function refreshAfterApproval(): Promise<void> {
    await Promise.all([
      loadLedger(selectedTaskId),
      loadActivity(selectedTaskId),
      loadRecovery(selectedTaskId),
    ]);
  }

  async function applyRunControl(action: AgentTaskControlActionV1): Promise<void> {
    if (controllingRun || recoveryView.kind !== 'result') {
      return;
    }
    const anchor =
      recoveryView.result.status === 'available'
        ? recoveryView.result.recovery
        : recoveryView.result.status === 'paused'
          ? recoveryView.result.recovery
          : recoveryView.result.status === 'runtimeOwned'
            ? recoveryView.result.runtime
            : null;
    if (anchor === null) return;
    controllingRun = true;
    actionError = null;
    actionMessage = null;
    try {
      const response = await runController(
        selectedTaskId,
        anchor.ledgerRevision,
        anchor.ledgerStoreVersion,
        action,
      );
      switch (response.result.status) {
        case 'applied':
          actionMessage = controlOutcomeMessage(
            response.result.outcome,
            response.result.runtimeStart,
          );
          await Promise.all([
            loadLedger(selectedTaskId),
            loadActivity(selectedTaskId),
            loadRecovery(selectedTaskId),
          ]);
          break;
        case 'accepted':
          actionMessage =
            response.result.outcome === 'pauseRequested'
              ? 'Pause wurde angefordert. Pausiert gilt der Run erst nach beendetem Worker und geprüftem Recovery-Checkpoint.'
              : 'Cancel wurde angefordert. Der terminale Zustand erscheint erst nach Worker-Stopp und dauerhaftem H11-Commit.';
          await Promise.all([
            loadLedger(selectedTaskId),
            loadActivity(selectedTaskId),
            loadRecovery(selectedTaskId),
          ]);
          break;
        case 'mutationReconciliationRequired':
          actionError =
            'Eine Mutation mit unbekannter Wirkung muss zuerst durch einen vollständigen Indexlauf reconciliert werden. Cancel bleibt möglich.';
          await loadRecovery(selectedTaskId);
          break;
        case 'resumeRequiresReplan':
          actionError =
            'Fortsetzen ist wegen veralteter Evidence oder einer unbekannten Mutationswirkung gesperrt. Wähle Replan oder Cancel.';
          await loadRecovery(selectedTaskId);
          break;
        case 'activityChanged':
          actionError =
            'Ledger oder Run haben sich geändert. Der aktuelle Stand wurde neu geladen.';
          await Promise.all([
            loadLedger(selectedTaskId),
            loadActivity(selectedTaskId),
            loadRecovery(selectedTaskId),
          ]);
          break;
        case 'noProject':
        case 'taskNotFound':
        case 'ledgerUnavailable':
        case 'goalRevisionMismatch':
        case 'runUnavailable':
        case 'runNotControllable':
          actionError = 'Der Run ist in seinem aktuellen dauerhaften Zustand nicht steuerbar.';
          await Promise.all([loadActivity(selectedTaskId), loadRecovery(selectedTaskId)]);
          break;
      }
    } catch {
      actionError = 'Die Run-Steuerung konnte nicht sicher abgeschlossen werden.';
    } finally {
      controllingRun = false;
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
    activityRequest += 1;
    activityView = { kind: 'idle' };
    recoveryRequest += 1;
    recoveryView = { kind: 'idle' };
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

  function controllerStateLabel(state: AgentControllerStateV1): string {
    const labels: Record<AgentControllerStateV1, string> = {
      awaitApproval: 'Wartet auf Freigabe',
      cancelled: 'Abgebrochen',
      done: 'Erfolgreich abgeschlossen',
      execute: 'Ausführung',
      failed: 'Fehlgeschlagen',
      intake: 'Aufnahme',
      localize: 'Kontextsuche',
      plan: 'Planung',
      replan: 'Neuplanung',
      verify: 'Verifikation',
    };
    return labels[state];
  }

  function agentRuntimeStateLabel(state: AgentTaskRuntimeStateV1): string {
    const labels: Record<AgentTaskRuntimeStateV1, string> = {
      cancelling: 'Cancel läuft',
      pausing: 'Pause läuft',
      queued: 'Eingereiht',
      running: 'Läuft',
    };
    return labels[state];
  }

  function activityEventLabel(item: AgentActivityEventV1): string {
    const event = item.event;
    switch (event.kind) {
      case 'runStarted':
        return 'Run gestartet';
      case 'stateTransition':
        return `${controllerStateLabel(event.from)} → ${controllerStateLabel(event.to)}`;
      case 'contextCompiled':
        return 'Context Pack kompiliert';
      case 'modelInteraction':
        return event.turn?.selectedAction === null || event.turn === null
          ? 'Modellantwort · keine Ausführung'
          : `Aktionsauswahl ${selectedActionLabel(event.turn.selectedAction)} · noch keine Ausführung`;
      case 'toolAction':
        return 'Ausführungsaktion · Tool tatsächlich aufgerufen';
      case 'ledgerUpdated':
        return `Replan · Ledger R${event.fromRevision} → R${event.toRevision}`;
      case 'verificationRecorded':
        return 'Verifikation aufgezeichnet';
      case 'approvalRecorded':
        return 'Freigabeereignis aufgezeichnet';
      case 'diagnostic':
        return 'Diagnose aufgezeichnet';
    }
  }

  function selectedActionLabel(action: AgentSelectedActionV1): string {
    const labels: Record<AgentSelectedActionV1, string> = {
      applyPatch: 'Patch',
      finish: 'Abschlussprüfung',
      inspect: 'Inspektion',
      run: 'Prozess',
      search: 'Suche',
      updateLedger: 'Ledger-Update',
    };
    return labels[action];
  }

  function eventCodeLabel(code: AgentActivityEventV1['code']): string | null {
    if (code === 'none') return null;
    const labels: Record<Exclude<AgentActivityEventV1['code'], 'none'>, string> = {
      cancellation: 'Abbruch beobachtet',
      controllerDecision: 'Controller-Entscheidung',
      invalidModelOutput: 'Ungültige Modellausgabe',
      policyDecision: 'Policy-Entscheidung',
      stateRecovered: 'Zustand wiederhergestellt',
      timeout: 'Zeitlimit erreicht',
      toolFailure: 'Tool fehlgeschlagen',
      userRequest: 'Nutzeranforderung',
      verificationFailure: 'Verifikation fehlgeschlagen',
    };
    return labels[code];
  }

  function isProblemEvent(item: AgentActivityEventV1): boolean {
    return item.outcome === 'failed' || item.outcome === 'denied' || item.outcome === 'cancelled';
  }

  function latestContextSequence(run: AgentActivityRunV1): string | null {
    return (
      [...run.timeline].reverse().find((item) => item.event.kind === 'contextCompiled')?.sequence ??
      null
    );
  }

  function controlOutcomeMessage(
    outcome: 'cancelled' | 'replanRequired' | 'resumed',
    runtimeStart: 'failed' | 'queued' | 'unavailable' | null,
  ): string {
    const messages = {
      cancelled: 'Der Agent Run wurde dauerhaft abgebrochen.',
      replanRequired:
        runtimeStart === 'queued'
          ? 'Der Recovery-Stand wurde committed und ein neuer besessener Replan-Versuch eingereiht.'
          : runtimeStart === 'failed'
            ? 'Der Recovery-Stand wurde committed, aber der besessene Replan-Versuch konnte nicht eingereiht werden.'
            : 'Der Recovery-Stand wurde committed; ohne ausführbare Agent-Capability bleibt Replan bereit.',
      resumed:
        runtimeStart === 'queued'
          ? 'Der aktuelle Snapshot wurde übernommen und ein neuer besessener Versuch eingereiht.'
          : runtimeStart === 'failed'
            ? 'Der aktuelle Snapshot wurde übernommen, aber der besessene Versuch konnte nicht eingereiht werden.'
            : 'Der aktuelle Snapshot wurde übernommen; ohne ausführbare Agent-Capability bleibt Resume bereit.',
    } as const;
    return messages[outcome];
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
    <AgentInspectionPanel
      taskId={selectedTaskId}
      loader={inspectionLoader}
      logLoader={inspectionLogLoader}
    />
    <AgentApprovalCenter
      taskId={selectedTaskId}
      loader={approvalLoader}
      controller={approvalController}
      onChanged={refreshAfterApproval}
    />
    <section class="agent-activity" aria-labelledby="agent-activity-heading">
      <header>
        <p>Durable Run</p>
        <h3 id="agent-activity-heading">Aktivität, Kontext und Budget</h3>
      </header>
      {#if activityView.kind === 'loading'}
        <p role="status" aria-live="polite">Run-Aktivität wird geladen …</p>
      {:else if activityView.kind === 'error'}
        <div class="error-state" role="alert">
          <p>Die Run-Aktivität konnte nicht sicher gelesen werden.</p>
          <button type="button" onclick={() => loadActivity(selectedTaskId)}>Erneut laden</button>
        </div>
      {:else if activityView.kind === 'result' && activityView.result.status === 'activityChanged'}
        <div class="error-state" role="status">
          <p>Ledger oder Run haben sich während des Lesens geändert.</p>
          <button type="button" onclick={() => loadActivity(selectedTaskId)}>
            Aktuellen Stand laden
          </button>
        </div>
      {:else if activityView.kind === 'result' && (activityView.result.status === 'noProject' || activityView.result.status === 'taskNotFound')}
        <p class="error-state" role="alert">
          Aufgabe oder aktiver Worktree sind für diese Aktivität nicht mehr verfügbar.
        </p>
      {:else if activityView.kind === 'result' && activityView.result.status === 'ledgerUnavailable'}
        <p class="empty-state">Ohne Task Ledger existiert noch kein kontrollierter Agent Run.</p>
      {:else if activityView.kind === 'result' && activityView.result.status === 'goalRevisionMismatch'}
        <p class="error-state" role="alert">
          Goal R{activityView.result.currentRevision} und Ledger-Goal R{activityView.result
            .ledgerRevision}
          stimmen nicht überein. Vor weiterer Ausführung ist ein Replan erforderlich.
        </p>
      {:else if activityView.kind === 'result' && activityView.result.status === 'available'}
        {@const activity = activityView.result.activity}
        {#if activity.blockers.length > 0}
          <section class="blockers" aria-labelledby="agent-blockers-heading">
            <h4 id="agent-blockers-heading">Offene Blocker</h4>
            <ul>
              {#each activity.blockers as blocker (blocker.stepId)}
                <li>
                  <strong
                    >{blocker.status === 'awaitingApproval'
                      ? 'Freigabe nötig'
                      : 'Blockiert'}</strong
                  >
                  <span>{blocker.reason}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        {#if activity.run === null}
          <p class="empty-state">Für die Ledger-Schritte wurde noch kein Run-Versuch gestartet.</p>
        {:else}
          {@const run = activity.run}
          <div class="run-summary">
            <div>
              <span>Controllerzustand</span>
              <strong>{controllerStateLabel(run.state)}</strong>
            </div>
            <span class:terminal={run.terminal} class="run-lifecycle">
              {run.terminal ? 'Terminaler Zustand' : 'Run aktiv oder fortsetzbar'}
            </span>
            <code>Versuch {run.attemptNumber} · Run {run.runId.slice(0, 12)}</code>
          </div>
          <section class="run-controls" aria-labelledby="run-controls-heading">
            <div>
              <p>Explizite Recovery</p>
              <h4 id="run-controls-heading">Run steuern</h4>
            </div>
            {#if recoveryView.kind === 'loading'}
              <p role="status" aria-live="polite">Sichere Steueroptionen werden geprüft …</p>
            {:else if recoveryView.kind === 'error'}
              <div class="error-state" role="alert">
                <p>Die Recovery-Anker konnten nicht sicher geprüft werden.</p>
                <button type="button" onclick={() => loadRecovery(selectedTaskId)}>
                  Erneut prüfen
                </button>
              </div>
            {:else if recoveryView.kind === 'result' && recoveryView.result.status === 'activityChanged'}
              <div class="error-state" role="status">
                <p>Der Run hat sich während der Recovery-Prüfung geändert.</p>
                <button type="button" onclick={() => loadRecovery(selectedTaskId)}>
                  Aktuellen Stand prüfen
                </button>
              </div>
            {:else if recoveryView.kind === 'result' && recoveryView.result.status === 'runtimeOwned'}
              {@const runtime = recoveryView.result.runtime}
              <dl class="recovery-facts">
                <div>
                  <dt>Produktlaufzeit</dt>
                  <dd>{agentRuntimeStateLabel(runtime.runtimeState)}</dd>
                </div>
                <div>
                  <dt>Controller</dt>
                  <dd>{controllerStateLabel(runtime.controllerState)}</dd>
                </div>
                <div>
                  <dt>Ledgeranker</dt>
                  <dd>R{runtime.ledgerRevision} · V{runtime.ledgerStoreVersion}</dd>
                </div>
              </dl>
              <p class="bounded-note">
                Dieser Prozess besitzt den Worker. Recovery unterbricht deshalb keinen laufenden
                Toolversuch; Pause und Cancel stoppen zuerst kooperativ die Produktlaufzeit.
              </p>
              <div class="control-actions" aria-label="Agent Run Laufzeit-Aktionen">
                <button
                  type="button"
                  disabled={controllingRun || !runtime.canPause}
                  onclick={() => applyRunControl('pause')}
                >
                  Pause
                </button>
                <button
                  class="danger-action"
                  type="button"
                  disabled={controllingRun || runtime.runtimeState === 'cancelling'}
                  onclick={() => applyRunControl('cancel')}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={controllingRun}
                  onclick={() => loadRecovery(selectedTaskId)}
                >
                  Status aktualisieren
                </button>
              </div>
            {:else if recoveryView.kind === 'result' && (recoveryView.result.status === 'available' || recoveryView.result.status === 'paused')}
              {@const recovery = recoveryView.result.recovery}
              {#if recoveryView.result.status === 'paused'}
                <p class="bounded-note" role="status">
                  Produktlaufzeit pausiert · der Worker ist beendet und der Recovery-Checkpoint
                  wurde geprüft.
                </p>
              {/if}
              <dl class="recovery-facts">
                <div>
                  <dt>Snapshot</dt>
                  <dd>{recovery.snapshotChanged ? 'Geändert' : 'Unverändert'}</dd>
                </div>
                <div>
                  <dt>Stale Evidence</dt>
                  <dd>{recovery.staleEvidenceCount}</dd>
                </div>
                <div>
                  <dt>Unterbrochene Toolversuche</dt>
                  <dd>{recovery.interruptedToolAttempts}</dd>
                </div>
              </dl>
              {#if recovery.mutationReconciliationRequired}
                <p class="error-state" role="alert">
                  Eine Mutation hat eine unbekannte Wirkung. Resume und Replan bleiben bis zu einem
                  autoritativen Full-Scan gesperrt; Cancel bleibt erreichbar.
                </p>
              {:else if recovery.mutationReplanRequired}
                <p class="bounded-note">
                  Die unbekannte Mutationswirkung wurde reconciliert. Vor weiterer Mutation ist
                  Replan erforderlich.
                </p>
              {:else if recovery.staleEvidenceCount > 0}
                <p class="bounded-note">
                  Abgeschlossene Evidence ist veraltet. Resume ist gesperrt; Replan öffnet
                  betroffene Schritte kontrolliert neu.
                </p>
              {/if}
              <div class="control-actions" aria-label="Agent Run Recovery-Aktionen">
                <button
                  type="button"
                  disabled={controllingRun || !recovery.canResume}
                  onclick={() => applyRunControl('resume')}
                >
                  Resume
                </button>
                <button
                  type="button"
                  disabled={controllingRun || recovery.mutationReconciliationRequired}
                  onclick={() => applyRunControl('replan')}
                >
                  Replan
                </button>
                <button
                  class="danger-action"
                  type="button"
                  disabled={controllingRun}
                  onclick={() => applyRunControl('cancel')}
                >
                  Cancel
                </button>
              </div>
            {:else if recoveryView.kind === 'result' && recoveryView.result.status === 'runNotControllable'}
              <p class="bounded-note">
                Dieser Run ist {controllerStateLabel(recoveryView.result.state)} und nicht mehr steuerbar.
              </p>
            {:else if recoveryView.kind === 'result' && recoveryView.result.status === 'runUnavailable'}
              <p class="empty-state">Noch kein aktiver Run-Versuch vorhanden.</p>
            {:else if recoveryView.kind === 'result'}
              <p class="error-state" role="alert">
                Goal, Ledger oder aktiver Worktree stimmen für die Run-Steuerung nicht überein.
              </p>
            {/if}
          </section>
          {#if !run.ledgerRevisionMatchesCurrent}
            <p class="bounded-note">
              Dieser letzte Run gehört zu Ledger R{run.ledgerRevision}; aktuell ist R{activity.currentLedgerRevision}.
            </p>
          {/if}
          <div class="context-budget-grid">
            <section aria-labelledby="context-status-heading">
              <h4 id="context-status-heading">Context</h4>
              <dl>
                <div>
                  <dt>Snapshot</dt>
                  <dd><code>{run.currentSnapshotId.slice(0, 16)}</code></dd>
                </div>
                <div>
                  <dt>Letztes Context Pack</dt>
                  <dd>
                    {#if latestContextSequence(run) === null}
                      Nicht im sichtbaren Journalfenster
                    {:else}
                      Ereignis #{latestContextSequence(run)}
                    {/if}
                  </dd>
                </div>
                <div>
                  <dt>Stand</dt>
                  <dd>{run.updatedAtUnixMillis} ms</dd>
                </div>
              </dl>
            </section>
            <section aria-labelledby="run-budget-heading">
              <h4 id="run-budget-heading">Run-Budget</h4>
              <dl class="budget-list">
                <div>
                  <dt>Turns</dt>
                  <dd>{run.usage.turnCount} / {run.budget.turnLimit}</dd>
                </div>
                <div>
                  <dt>Prompt-Tokens</dt>
                  <dd>{run.usage.promptTokens} / {run.budget.promptTokenLimit}</dd>
                </div>
                <div>
                  <dt>Output-Tokens</dt>
                  <dd>{run.usage.outputTokens} / {run.budget.outputTokenLimit}</dd>
                </div>
                <div>
                  <dt>Aktionen</dt>
                  <dd>{run.usage.actionCount} / {run.budget.actionLimit}</dd>
                </div>
                <div>
                  <dt>Reparaturen</dt>
                  <dd>{run.usage.repairCount} / {run.budget.repairLimit}</dd>
                </div>
                <div>
                  <dt>Zeit am letzten Ereignis</dt>
                  <dd>
                    {run.usage.elapsedAtLastEventMillis} / {run.budget.durationLimitMillis} ms
                  </dd>
                </div>
              </dl>
            </section>
          </div>
          <section class="activity-timeline" aria-labelledby="activity-timeline-heading">
            <div class="timeline-heading">
              <h4 id="activity-timeline-heading">Conversation- und Action-Timeline</h4>
              {#if run.earlierEventsOmitted}<span>Ältere Ereignisse ausgeblendet</span>{/if}
            </div>
            <ol>
              {#each run.timeline as item (item.sequence)}
                <li class:problem={isProblemEvent(item)}>
                  <span class="event-sequence">#{item.sequence}</span>
                  <div>
                    <strong>{activityEventLabel(item)}</strong>
                    <p>
                      {item.occurredAtUnixMillis} ms
                      {#if eventCodeLabel(item.code) !== null}
                        · {eventCodeLabel(item.code)}
                      {/if}
                      {#if item.outcome !== null}
                        · {item.outcome}{/if}
                    </p>
                  </div>
                </li>
              {/each}
            </ol>
          </section>
        {/if}
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
    background: color-mix(in srgb, var(--color-surface-raised) 95%, var(--color-info-surface));
    border: 1px solid var(--color-border-soft);
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
    color: var(--color-muted);
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
    border: 1px solid var(--color-border-soft);
    border-radius: 0.9rem;
    overflow: clip;
  }

  .persistent-anchors {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-soft);
    border-radius: 0.9rem;
    box-shadow: 0 0.4rem 1rem var(--color-shadow);
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
    color: var(--color-muted);
    font-size: 0.85rem;
  }

  .persistent-anchors h3,
  .persistent-anchors h4 {
    margin: 0.3rem 0 0;
  }

  .goal-actions {
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border-soft);
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
    background: var(--color-surface-raised);
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
    background: var(--color-info);
    border-radius: 99px;
    color: var(--color-on-accent);
    font-size: 0.72rem;
    font-weight: 800;
    padding: 0.2rem 0.5rem;
  }

  .criteria-list li > span.should {
    background: var(--color-warning);
  }

  .criteria-list p {
    margin: 0;
  }

  .goal-editor {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-soft);
    border-radius: 0.9rem;
    display: grid;
    gap: 1rem;
    padding: 1rem;
  }

  .task-ledger,
  .agent-activity {
    border: 1px solid var(--color-border-soft);
    border-radius: 0.9rem;
    display: grid;
    gap: 0.8rem;
    padding: 1rem;
  }

  .task-ledger header p,
  .agent-activity > header p,
  .persistent-anchors > div > span {
    color: var(--color-muted);
    font-size: 0.78rem;
    font-weight: 800;
    letter-spacing: 0.1em;
    margin: 0 0 0.25rem;
    text-transform: uppercase;
  }

  .task-ledger header h3,
  .agent-activity > header h3,
  .persistent-anchors h4 {
    margin: 0;
  }

  .ledger-metadata {
    color: var(--color-muted);
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
    border: 1px solid var(--color-border-soft);
    border-radius: 0.55rem;
    display: grid;
    gap: 0.55rem;
    grid-template-columns: minmax(8rem, auto) minmax(0, 1fr);
    padding: 0.7rem;
  }

  .ledger-steps li.current {
    border-color: var(--color-info);
  }

  .ledger-steps span {
    font-weight: 700;
  }

  .ledger-steps p {
    margin: 0;
  }

  .blockers {
    background: var(--color-warning-surface);
    border-left: 0.25rem solid var(--color-warning-strong);
    padding: 0.8rem;
  }

  .blockers h4,
  .blockers ul {
    margin-bottom: 0;
  }

  .blockers ul {
    display: grid;
    gap: 0.5rem;
    list-style: none;
    padding: 0;
  }

  .blockers li {
    display: grid;
    gap: 0.2rem;
  }

  .run-summary {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 1rem;
  }

  .run-summary > div {
    display: grid;
    gap: 0.15rem;
  }

  .run-summary > div > span,
  .timeline-heading span {
    color: var(--color-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .run-lifecycle {
    background: var(--color-info-surface);
    border-radius: 99px;
    color: var(--color-info);
    font-size: 0.78rem;
    font-weight: 800;
    padding: 0.3rem 0.65rem;
  }

  .run-lifecycle.terminal {
    background: var(--color-neutral-surface);
    color: var(--color-neutral);
  }

  .run-controls {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-soft);
    border-radius: 0.65rem;
    display: grid;
    gap: 0.75rem;
    padding: 0.8rem;
  }

  .run-controls > div:first-child p {
    color: var(--color-muted);
    font-size: 0.72rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    margin: 0;
    text-transform: uppercase;
  }

  .run-controls h4 {
    margin: 0.2rem 0 0;
  }

  .recovery-facts {
    display: grid;
    gap: 0.5rem;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    margin: 0;
  }

  .recovery-facts > div {
    display: grid;
    gap: 0.2rem;
  }

  .recovery-facts dt {
    color: var(--color-muted);
    font-size: 0.78rem;
  }

  .recovery-facts dd {
    font-weight: 700;
    margin: 0;
  }

  .control-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
  }

  .danger-action {
    border-color: var(--color-danger-strong);
    color: var(--color-danger-strong);
  }

  .context-budget-grid {
    display: grid;
    gap: 0.8rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .context-budget-grid > section {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-soft);
    border-radius: 0.65rem;
    padding: 0.8rem;
  }

  .context-budget-grid h4 {
    margin-bottom: 0.65rem;
  }

  .context-budget-grid dl {
    display: grid;
    gap: 0.45rem;
    margin: 0;
  }

  .context-budget-grid dl > div {
    display: flex;
    gap: 0.8rem;
    justify-content: space-between;
  }

  .context-budget-grid dt {
    color: var(--color-muted);
  }

  .context-budget-grid dd {
    margin: 0;
    overflow-wrap: anywhere;
    text-align: right;
  }

  .activity-timeline {
    display: grid;
    gap: 0.65rem;
  }

  .timeline-heading {
    align-items: baseline;
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    justify-content: space-between;
  }

  .timeline-heading h4 {
    margin: 0;
  }

  .activity-timeline ol {
    display: grid;
    gap: 0.45rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .activity-timeline li {
    align-items: start;
    border-left: 0.2rem solid var(--color-info);
    display: grid;
    gap: 0.65rem;
    grid-template-columns: auto minmax(0, 1fr);
    padding: 0.55rem 0.7rem;
  }

  .activity-timeline li.problem {
    background: var(--color-danger-surface);
    border-left-color: var(--color-danger-strong);
  }

  .activity-timeline li p {
    color: var(--color-muted);
    font-size: 0.82rem;
    margin: 0.2rem 0 0;
  }

  .event-sequence {
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
  }

  .criteria-editor {
    border: 1px solid var(--color-border-soft);
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
    background: var(--color-info);
    color: var(--color-on-accent);
  }

  .success-state {
    background: var(--color-positive-surface);
    border-left: 0.25rem solid var(--color-positive);
    padding: 0.8rem;
  }

  .error-state {
    background: var(--color-danger-surface);
    border-left: 0.25rem solid var(--color-danger-strong);
    padding: 0.8rem;
  }

  .empty-state {
    color: var(--color-muted);
  }

  @media (max-width: 860px) {
    .goal-columns,
    .boundary-grid,
    .context-budget-grid,
    .recovery-facts,
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

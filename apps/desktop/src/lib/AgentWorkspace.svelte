<script lang="ts">
  import { untrack } from 'svelte';
  import {
    controlAgentSession,
    queryAgentSession,
    queryAgentSessions,
    queryUiPreferences,
    submitAgentMessage,
    updateAgentWorkspaceLayout,
    type AgentSessionControlActionV1,
    type AgentSessionModeV1,
    type AgentSessionResponseV1,
    type AgentSessionsResponseV1,
    type AgentSessionStateV1,
    type AgentSessionSummaryV1,
    type AgentSessionV1,
    type UiPreferencesV1,
  } from './agent-session';
  import {
    queryAgentActivity,
    type AgentActivityResponseV1,
    type AgentActivityV1,
  } from './agent-activity';
  import type {
    AgentApprovalControlActionV1,
    AgentApprovalControlResponseV1,
    AgentApprovalResponseV1,
    AgentApprovalV1,
  } from './agent-approval';
  import type {
    AgentInspectionLogResponseV1,
    AgentInspectionResponseV1,
    AgentInspectionStreamV1,
  } from './agent-inspection';
  import type { GlobalRunStatus } from './global-status';
  import AgentApprovalCenter from './AgentApprovalCenter.svelte';
  import AgentInspectionPanel from './AgentInspectionPanel.svelte';
  import { parseChatMarkdown } from './chat-markdown';

  interface Props {
    activeProject: boolean;
    activityLoader?: (taskId: string) => Promise<AgentActivityResponseV1>;
    approvalController?: (
      taskId: string,
      approval: AgentApprovalV1,
      action: AgentApprovalControlActionV1,
    ) => Promise<AgentApprovalControlResponseV1>;
    approvalLoader?: (taskId: string) => Promise<AgentApprovalResponseV1>;
    inspectionLoader?: (taskId: string) => Promise<AgentInspectionResponseV1>;
    inspectionLogLoader?: (
      taskId: string,
      revision: string,
      inspectionId: string,
      stream: AgentInspectionStreamV1,
      offset: number,
    ) => Promise<AgentInspectionLogResponseV1>;
    onRunStatusChange?: (status: GlobalRunStatus) => void;
    sessionController?: (
      sessionId: string,
      revision: string,
      action: AgentSessionControlActionV1,
    ) => Promise<AgentSessionResponseV1>;
    sessionLoader?: (sessionId: string) => Promise<AgentSessionResponseV1>;
    sessionsLoader?: (options?: {
      includeArchived?: boolean;
      search?: string | null;
    }) => Promise<AgentSessionsResponseV1>;
    messageSubmitter?: (input: {
      expectedSessionRevision?: string | null;
      message: string;
      mode?: AgentSessionModeV1;
      sessionId?: string | null;
    }) => Promise<AgentSessionResponseV1>;
    pollIntervalMs?: number;
  }

  type SessionsView =
    | { kind: 'idle' }
    | { kind: 'loading' }
    | { kind: 'noProject' }
    | { kind: 'available'; sessions: AgentSessionSummaryV1[] }
    | { kind: 'error' };
  type SessionView =
    | { kind: 'new' }
    | { kind: 'loading' }
    | { kind: 'available'; session: AgentSessionV1 }
    | { kind: 'missing' }
    | { kind: 'error' };
  type InspectorTab = 'progress' | 'changes' | 'review';

  let {
    activeProject,
    activityLoader = queryAgentActivity,
    approvalController,
    approvalLoader,
    inspectionLoader,
    inspectionLogLoader,
    onRunStatusChange = () => {},
    sessionController = controlAgentSession,
    sessionLoader = queryAgentSession,
    sessionsLoader = queryAgentSessions,
    messageSubmitter = submitAgentMessage,
    pollIntervalMs = 700,
  }: Props = $props();

  let sessionsView = $state<SessionsView>({ kind: 'idle' });
  let sessionView = $state<SessionView>({ kind: 'new' });
  let selectedSessionId = $state<string | null>(null);
  let newMode = $state<AgentSessionModeV1>('agent');
  let composer = $state('');
  let pendingMessage = $state<string | null>(null);
  let submitting = $state(false);
  let actionError = $state<string | null>(null);
  let searchInput = $state('');
  let includeArchived = $state(false);
  let sessionMenuOpen = $state(false);
  let historyOpen = $state(true);
  let inspectorOpen = $state(true);
  let inspectorTab = $state<InspectorTab>('progress');
  let preferences = $state<UiPreferencesV1>({
    inspectorCollapsed: false,
    inspectorWidth: 400,
    protocolVersion: 1,
    revision: '0',
    sessionRailCollapsed: false,
    sessionRailWidth: 264,
  });
  let activity = $state<AgentActivityV1 | null>(null);
  let activityLoading = $state(false);
  let observedProject = false;
  let sessionRequest = 0;
  let sessionsRequest = 0;
  let activityRequest = 0;

  const selectedSession = $derived(sessionView.kind === 'available' ? sessionView.session : null);
  const selectedSummary = $derived(selectedSession?.summary ?? null);
  const activeTaskId = $derived(selectedSession?.activeTaskId ?? null);
  const presentationCanBeHidden = $derived(
    selectedSummary !== null &&
      !['running', 'awaitingApproval', 'paused'].includes(selectedSummary.state),
  );
  const canSubmit = $derived(
    activeProject &&
      !submitting &&
      composer.trim().length > 0 &&
      composer.length <= 256 * 1024 &&
      (!selectedSummary ||
        [
          'draft',
          'awaitingUser',
          'awaitingPlanReview',
          'completed',
          'failed',
          'cancelled',
        ].includes(selectedSummary.state)),
  );

  $effect(() => {
    if (activeProject && !observedProject) {
      observedProject = true;
      void initialize();
    } else if (!activeProject && observedProject) {
      observedProject = false;
      reset();
    }
  });

  $effect(() => {
    if (activeTaskId) void loadActivity(activeTaskId);
    else activity = null;
  });

  $effect(() => {
    const sessionId =
      selectedSummary && ['running', 'awaitingApproval', 'paused'].includes(selectedSummary.state)
        ? selectedSummary.sessionId
        : null;
    if (!sessionId) return;
    let stopped = false;
    let timer: number | undefined;
    const schedule = (): void => {
      timer = window.setTimeout(async () => {
        await pollSession(sessionId);
        if (!stopped) schedule();
      }, pollIntervalMs);
    };
    schedule();
    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  });

  $effect(() => {
    if (!activeProject) onRunStatusChange({ kind: 'noProject' });
    else if (activityLoading) onRunStatusChange({ kind: 'loading' });
    else if (activity?.run) onRunStatusChange({ kind: 'available', state: activity.run.state });
    else onRunStatusChange({ kind: 'idle' });
  });

  $effect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const inspectorDrawer = window.matchMedia('(max-width: 1100px)');
    const historyDrawer = window.matchMedia('(max-width: 760px)');
    const adaptPanes = (): void => {
      const saved = untrack(() => preferences);
      inspectorOpen = inspectorDrawer.matches ? false : !saved.inspectorCollapsed;
      historyOpen = historyDrawer.matches ? false : !saved.sessionRailCollapsed;
    };
    adaptPanes();
    inspectorDrawer.addEventListener('change', adaptPanes);
    historyDrawer.addEventListener('change', adaptPanes);
    return () => {
      inspectorDrawer.removeEventListener('change', adaptPanes);
      historyDrawer.removeEventListener('change', adaptPanes);
    };
  });

  async function initialize(): Promise<void> {
    actionError = null;
    await Promise.all([loadPreferences(), loadSessions()]);
  }

  function reset(): void {
    sessionsView = { kind: 'noProject' };
    sessionView = { kind: 'new' };
    selectedSessionId = null;
    composer = '';
    pendingMessage = null;
    activity = null;
  }

  async function loadPreferences(): Promise<void> {
    try {
      const next = await queryUiPreferences();
      preferences = next;
      historyOpen = mediaMatches('(min-width: 761px)', true) && !next.sessionRailCollapsed;
      inspectorOpen = mediaMatches('(min-width: 1101px)', true) && !next.inspectorCollapsed;
    } catch {
      // Valid defaults remain usable when nonessential layout persistence is unavailable.
    }
  }

  async function loadSessions(preferredId: string | null = selectedSessionId): Promise<void> {
    const request = ++sessionsRequest;
    sessionsView = { kind: 'loading' };
    try {
      const response = await sessionsLoader({
        includeArchived,
        search: searchInput.trim() || null,
      });
      if (request !== sessionsRequest) return;
      if (response.result.status === 'noProject') {
        sessionsView = { kind: 'noProject' };
        return;
      }
      const sessions = response.result.sessions;
      sessionsView = { kind: 'available', sessions };
      const nextId =
        preferredId && sessions.some((session) => session.sessionId === preferredId)
          ? preferredId
          : sessions[0]?.sessionId;
      if (nextId) await selectSession(nextId);
      else startNewSession();
    } catch {
      if (request === sessionsRequest) sessionsView = { kind: 'error' };
    }
  }

  async function selectSession(sessionId: string): Promise<void> {
    const request = ++sessionRequest;
    selectedSessionId = sessionId;
    sessionMenuOpen = false;
    actionError = null;
    sessionView = { kind: 'loading' };
    try {
      const response = await sessionLoader(sessionId);
      if (request !== sessionRequest || selectedSessionId !== sessionId) return;
      if (response.result.status === 'available')
        sessionView = { kind: 'available', session: response.result.session };
      else if (response.result.status === 'notFound') sessionView = { kind: 'missing' };
      else sessionView = { kind: 'error' };
    } catch {
      if (request === sessionRequest) sessionView = { kind: 'error' };
    }
  }

  async function pollSession(sessionId: string): Promise<void> {
    try {
      const response = await sessionLoader(sessionId);
      if (selectedSessionId !== sessionId || response.result.status !== 'available') return;
      sessionView = { kind: 'available', session: response.result.session };
      if (response.result.session.activeTaskId) {
        await loadActivity(response.result.session.activeTaskId);
      }
      if (
        !['running', 'awaitingApproval', 'paused'].includes(response.result.session.summary.state)
      ) {
        await loadSessions(sessionId);
      }
    } catch {
      // A transient poll error leaves the last verified session projection visible.
    }
  }

  function startNewSession(): void {
    sessionRequest += 1;
    selectedSessionId = null;
    sessionView = { kind: 'new' };
    composer = '';
    pendingMessage = null;
    actionError = null;
    sessionMenuOpen = false;
  }

  async function submit(): Promise<void> {
    if (!canSubmit) return;
    const message = composer.trim();
    const current = selectedSession;
    composer = '';
    pendingMessage = message;
    submitting = true;
    actionError = null;
    try {
      const response = await messageSubmitter(
        current
          ? {
              expectedSessionRevision: current.summary.revision,
              message,
              sessionId: current.summary.sessionId,
            }
          : { message, mode: newMode },
      );
      if (response.result.status === 'available') {
        selectedSessionId = response.result.session.summary.sessionId;
        sessionView = { kind: 'available', session: response.result.session };
        await loadSessions(selectedSessionId);
      } else {
        actionError = 'Das aktive Projekt ist nicht mehr verfügbar.';
      }
    } catch {
      composer = message;
      actionError =
        'Die Nachricht konnte nicht sicher verarbeitet werden. Prüfe Modell und Projektstatus.';
    } finally {
      pendingMessage = null;
      submitting = false;
    }
  }

  function composerKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  async function applySessionAction(action: AgentSessionControlActionV1): Promise<void> {
    const current = selectedSession;
    if (!current || submitting) return;
    actionError = null;
    try {
      const response = await sessionController(
        current.summary.sessionId,
        current.summary.revision,
        action,
      );
      if (response.result.status === 'available') {
        sessionView = { kind: 'available', session: response.result.session };
        await loadSessions(response.result.session.summary.sessionId);
      } else {
        startNewSession();
        await loadSessions(null);
      }
    } catch {
      actionError =
        action.kind === 'implementPlan'
          ? 'Der Plan konnte mit den aktuellen dauerhaften Ankern nicht gestartet werden.'
          : 'Die Session-Aktion konnte nicht abgeschlossen werden.';
    }
  }

  function renameSession(): void {
    const current = selectedSummary;
    if (!current) return;
    const title = window.prompt('Session umbenennen', current.title)?.trim();
    if (title) void applySessionAction({ kind: 'rename', title });
    sessionMenuOpen = false;
  }

  async function loadActivity(taskId: string): Promise<void> {
    const request = ++activityRequest;
    activityLoading = true;
    try {
      const response = await activityLoader(taskId);
      if (request !== activityRequest || taskId !== activeTaskId) return;
      activity = response.result.status === 'available' ? response.result.activity : null;
    } catch {
      if (request === activityRequest) activity = null;
    } finally {
      if (request === activityRequest) activityLoading = false;
    }
  }

  function toggleHistory(): void {
    historyOpen = !historyOpen;
    void persistLayout();
  }

  function toggleInspector(): void {
    inspectorOpen = !inspectorOpen;
    void persistLayout();
  }

  function beginResize(event: PointerEvent, pane: 'history' | 'inspector'): void {
    if (event.button !== 0) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth =
      pane === 'history' ? preferences.sessionRailWidth : preferences.inspectorWidth;
    const move = (moveEvent: PointerEvent): void => {
      const delta = moveEvent.clientX - startX;
      const width =
        pane === 'history'
          ? Math.max(220, Math.min(360, startWidth + delta))
          : Math.max(320, Math.min(640, startWidth - delta));
      preferences =
        pane === 'history'
          ? { ...preferences, sessionRailWidth: width }
          : { ...preferences, inspectorWidth: width };
    };
    const finish = (): void => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', finish);
      void persistLayout();
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', finish, { once: true });
  }

  async function persistLayout(): Promise<void> {
    const layout = {
      inspectorCollapsed: !inspectorOpen,
      inspectorWidth: preferences.inspectorWidth,
      sessionRailCollapsed: !historyOpen,
      sessionRailWidth: preferences.sessionRailWidth,
    };
    try {
      preferences = await updateAgentWorkspaceLayout(preferences, layout);
    } catch {
      // The current layout remains locally usable after an optimistic preference conflict.
    }
  }

  function visibleSessions(): AgentSessionSummaryV1[] {
    return sessionsView.kind === 'available' ? sessionsView.sessions : [];
  }

  function mediaMatches(query: string, fallback: boolean): boolean {
    return typeof window.matchMedia === 'function' ? window.matchMedia(query).matches : fallback;
  }

  function modeLabel(mode: AgentSessionModeV1): string {
    return mode === 'ask' ? 'Ask' : mode === 'plan' ? 'Plan' : 'Agent';
  }

  function stateLabel(state: AgentSessionStateV1): string {
    switch (state) {
      case 'running':
        return 'Arbeitet';
      case 'awaitingUser':
        return 'Antwort nötig';
      case 'awaitingPlanReview':
        return 'Plan prüfen';
      case 'awaitingApproval':
        return 'Freigabe nötig';
      case 'paused':
        return 'Pausiert';
      case 'completed':
        return 'Fertig';
      case 'failed':
        return 'Fehlgeschlagen';
      case 'cancelled':
        return 'Abgebrochen';
      case 'archived':
        return 'Archiviert';
      default:
        return 'Entwurf';
    }
  }

  function relativeTime(value: string): string {
    const delta = Date.now() - Number(value);
    if (!Number.isFinite(delta) || delta < 0) return 'gerade eben';
    const minutes = Math.floor(delta / 60_000);
    if (minutes < 1) return 'gerade eben';
    if (minutes < 60) return `vor ${minutes} Min.`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `vor ${hours} Std.`;
    return new Intl.DateTimeFormat('de-DE', { day: '2-digit', month: 'short' }).format(
      Number(value),
    );
  }
</script>

<section
  class="agent-workspace"
  class:history-collapsed={!historyOpen}
  class:inspector-collapsed={!inspectorOpen}
  style={`--history-width:${preferences.sessionRailWidth}px;--inspector-width:${preferences.inspectorWidth}px`}
  aria-label="Agent Workspace"
>
  {#if !activeProject}
    <div class="no-project">
      <span class="empty-icon" aria-hidden="true">A³</span>
      <h2>Öffne zuerst ein Projekt</h2>
      <p>Chats, Pläne und Agent-Runs bleiben an genau einen lokalen Worktree gebunden.</p>
    </div>
  {:else}
    <aside class="session-rail" aria-label="Session-Verlauf">
      <div class="rail-header">
        <strong>Sessions</strong>
        <button
          class="icon-button"
          type="button"
          onclick={toggleHistory}
          aria-label="Verlauf einklappen">‹</button
        >
      </div>
      <button class="new-session" type="button" onclick={startNewSession}
        ><span>＋</span> Neue Session</button
      >
      <label class="session-search">
        <span class="sr-only">Sessions durchsuchen</span>
        <input
          bind:value={searchInput}
          oninput={() => void loadSessions(selectedSessionId)}
          placeholder="Suchen"
        />
      </label>
      <label class="archive-toggle">
        <input
          type="checkbox"
          bind:checked={includeArchived}
          onchange={() => void loadSessions(null)}
        />
        Archivierte anzeigen
      </label>
      <div class="session-list" role="list">
        {#if sessionsView.kind === 'loading'}
          <p class="rail-state" role="status">Verlauf wird geladen …</p>
        {:else if sessionsView.kind === 'error'}
          <p class="rail-state" role="alert">Verlauf nicht verfügbar.</p>
        {:else if visibleSessions().length === 0}
          <p class="rail-state">Noch keine Sessions.</p>
        {:else}
          {#each visibleSessions() as session (session.sessionId)}
            <button
              class="session-item"
              class:selected={session.sessionId === selectedSessionId}
              type="button"
              onclick={() => void selectSession(session.sessionId)}
              aria-current={session.sessionId === selectedSessionId ? 'true' : undefined}
            >
              <span class="session-title">{session.title}</span>
              <span class="session-meta"
                ><span>{modeLabel(session.mode)}</span><span
                  >{relativeTime(session.updatedAtUnixMillis)}</span
                ></span
              >
              <span
                class:attention={session.state === 'awaitingApproval' ||
                  session.state === 'awaitingUser'}
                class="session-state">{stateLabel(session.state)}</span
              >
            </button>
          {/each}
        {/if}
      </div>
    </aside>

    {#if historyOpen}<div
        class="resize-handle history-resize"
        role="separator"
        aria-label="Verlaufbreite ändern"
        onpointerdown={(event) => beginResize(event, 'history')}
      ></div>{/if}
    {#if !historyOpen}<button
        class="reopen-pane reopen-history"
        type="button"
        onclick={toggleHistory}
        aria-label="Verlauf öffnen">☰</button
      >{/if}

    <main class="conversation" aria-label="Agent-Chat">
      <header class="conversation-header">
        <div>
          <h2>{selectedSummary?.title ?? 'Neue Session'}</h2>
          <p>
            {selectedSummary
              ? `${modeLabel(selectedSummary.mode)} · ${stateLabel(selectedSummary.state)}`
              : 'Beschreibe eine Aufgabe oder stelle eine Frage.'}
          </p>
        </div>
        {#if selectedSummary}
          {#if activeTaskId && selectedSummary.state === 'running'}
            <div class="runtime-controls" aria-label="Agentenlauf steuern">
              <button
                type="button"
                disabled={submitting}
                onclick={() => void applySessionAction({ kind: 'pause' })}>Pausieren</button
              >
              <button
                class="danger"
                type="button"
                disabled={submitting}
                onclick={() => void applySessionAction({ kind: 'cancel' })}>Abbrechen</button
              >
            </div>
          {:else if selectedSummary.state === 'running'}
            <div class="runtime-controls" aria-label="Agentenlauf steuern">
              <button
                class="danger"
                type="button"
                disabled={submitting}
                onclick={() => void applySessionAction({ kind: 'cancel' })}>Abbrechen</button
              >
            </div>
          {:else if activeTaskId && selectedSummary.state === 'paused'}
            <div class="runtime-controls" aria-label="Agentenlauf steuern">
              <button
                class="primary"
                type="button"
                disabled={submitting}
                onclick={() => void applySessionAction({ kind: 'resume' })}>Fortsetzen</button
              >
              <button
                class="danger"
                type="button"
                disabled={submitting}
                onclick={() => void applySessionAction({ kind: 'cancel' })}>Abbrechen</button
              >
            </div>
          {/if}
          <div class="session-menu">
            <button
              class="icon-button"
              type="button"
              aria-label="Session-Aktionen"
              aria-expanded={sessionMenuOpen}
              onclick={() => (sessionMenuOpen = !sessionMenuOpen)}>•••</button
            >
            {#if sessionMenuOpen}
              <div class="menu-popover">
                <button type="button" onclick={renameSession}>Umbenennen</button>
                {#if selectedSummary.mode === 'ask' && selectedSummary.state !== 'archived'}
                  <button
                    type="button"
                    onclick={() => void applySessionAction({ kind: 'switchToPlan' })}
                    >In Plan wechseln</button
                  >
                {/if}
                {#if presentationCanBeHidden}
                  <button
                    type="button"
                    onclick={() =>
                      void applySessionAction({
                        kind: selectedSummary.state === 'archived' ? 'unarchive' : 'archive',
                      })}
                    >{selectedSummary.state === 'archived'
                      ? 'Wiederherstellen'
                      : 'Archivieren'}</button
                  >
                  <button
                    class="danger"
                    type="button"
                    onclick={() => void applySessionAction({ kind: 'deletePresentation' })}
                    >Chat löschen</button
                  >
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </header>

      <div class="message-scroll" aria-live="polite">
        {#if sessionView.kind === 'loading'}
          <div class="center-state" role="status">Session wird geladen …</div>
        {:else if sessionView.kind === 'missing'}
          <div class="center-state">
            <p>Diese Session ist nicht mehr vorhanden.</p>
            <button type="button" onclick={startNewSession}>Neue Session</button>
          </div>
        {:else if sessionView.kind === 'error'}
          <div class="center-state" role="alert">
            <p>Die Session konnte nicht geladen werden.</p>
            <button
              type="button"
              onclick={() => selectedSessionId && void selectSession(selectedSessionId)}
              >Erneut laden</button
            >
          </div>
        {:else if sessionView.kind === 'new'}
          <div class="welcome">
            <span class="welcome-mark" aria-hidden="true">A³</span>
            <h3>Woran möchtest du arbeiten?</h3>
            <p>
              Wähle einen Modus und formuliere dein Ziel. A^3 hält Kontext, Fortschritt und Review
              in dieser Session zusammen.
            </p>
            <div class="starter-grid">
              <button
                type="button"
                onclick={() => {
                  newMode = 'ask';
                  composer = 'Wie ist dieser Teil des Projekts aufgebaut?';
                }}>Projekt verstehen <span>Ask</span></button
              >
              <button
                type="button"
                onclick={() => {
                  newMode = 'plan';
                  composer = 'Erstelle einen umsetzungsreifen Plan für ';
                }}>Änderung planen <span>Plan</span></button
              >
              <button
                type="button"
                onclick={() => {
                  newMode = 'agent';
                  composer = 'Implementiere ';
                }}>Aufgabe umsetzen <span>Agent</span></button
              >
            </div>
          </div>
        {:else}
          <div class="messages">
            {#each sessionView.session.entries as entry (entry.sequence)}
              <article
                class:user-message={entry.kind === 'userMessage'}
                class:agent-message={entry.kind !== 'userMessage'}
                class:plan-message={entry.kind === 'plan'}
                class="message"
              >
                <header>
                  <span
                    >{entry.kind === 'userMessage'
                      ? 'Du'
                      : entry.kind === 'plan'
                        ? `Plan R${entry.planRevision}`
                        : 'A^3'}</span
                  ><time>{relativeTime(entry.createdAtUnixMillis)}</time>
                </header>
                <div class="message-text">
                  {#each parseChatMarkdown(entry.text) as block, blockIndex (blockIndex)}
                    {#if block.kind === 'heading'}
                      <div class="markdown-heading" data-level={block.level}>{block.text}</div>
                    {:else if block.kind === 'paragraph'}
                      <p>{block.text}</p>
                    {:else if block.kind === 'list'}
                      {#if block.ordered}
                        <ol>
                          {#each block.items as item, itemIndex (itemIndex)}<li>{item}</li>{/each}
                        </ol>
                      {:else}
                        <ul>
                          {#each block.items as item, itemIndex (itemIndex)}<li>{item}</li>{/each}
                        </ul>
                      {/if}
                    {:else if block.kind === 'quote'}
                      <blockquote>{block.text}</blockquote>
                    {:else}
                      <pre><code data-language={block.language}>{block.text}</code></pre>
                    {/if}
                  {/each}
                </div>
                {#if entry.kind === 'plan' && entry.planRevision === sessionView.session.summary.currentPlanRevision && sessionView.session.summary.mode === 'plan' && sessionView.session.summary.state === 'awaitingPlanReview'}
                  <div class="plan-actions">
                    <button
                      class="primary"
                      type="button"
                      disabled={submitting}
                      onclick={() =>
                        void applySessionAction({
                          kind: 'implementPlan',
                          planRevision: entry.planRevision ?? 0,
                        })}>Plan umsetzen</button
                    >
                    <button
                      type="button"
                      onclick={() => {
                        composer = 'Überarbeite den Plan mit folgenden Änderungen: ';
                      }}>Änderungen anfragen</button
                    >
                  </div>
                {/if}
              </article>
            {/each}
            {#if pendingMessage}
              <article class="message user-message pending">
                <header><span>Du</span><span>Wird gesendet</span></header>
                <div class="message-text">{pendingMessage}</div>
              </article>
            {/if}
            {#if pendingMessage || sessionView.session.summary.state === 'running'}
              <article class="message agent-message working" role="status">
                <span class="working-dot"></span>
                <div>
                  <strong>A^3 arbeitet</strong>
                  <p>
                    {sessionView.session.summary.mode === 'ask'
                      ? 'Sammelt und prüft Informationen …'
                      : sessionView.session.summary.mode === 'plan'
                        ? 'Strukturiert Entscheidungen und Prüfschritte …'
                        : 'Analysiert Aufgabe, Kontext und sichere Ausführung …'}
                  </p>
                </div>
              </article>
            {/if}
          </div>
        {/if}
      </div>

      <div class="composer-wrap">
        {#if actionError}<p class="composer-error" role="alert">{actionError}</p>{/if}
        <div class="composer-box">
          {#if !selectedSummary}
            <div class="mode-switch" aria-label="Agent-Modus">
              <button
                type="button"
                aria-pressed={newMode === 'ask'}
                onclick={() => (newMode = 'ask')}
                ><strong>Ask</strong><span>Nur lesen und antworten</span></button
              >
              <button
                type="button"
                aria-pressed={newMode === 'agent'}
                onclick={() => (newMode = 'agent')}
                ><strong>Agent</strong><span>Änderungen ausführen</span></button
              >
              <button
                type="button"
                aria-pressed={newMode === 'plan'}
                onclick={() => (newMode = 'plan')}
                ><strong>Plan</strong><span>Gemeinsam ausarbeiten</span></button
              >
            </div>
          {/if}
          <textarea
            bind:value={composer}
            onkeydown={composerKeydown}
            disabled={submitting ||
              (selectedSummary !== null &&
                ![
                  'draft',
                  'awaitingUser',
                  'awaitingPlanReview',
                  'completed',
                  'failed',
                  'cancelled',
                ].includes(selectedSummary.state))}
            aria-label="Nachricht an A^3"
            placeholder={selectedSummary
              ? 'Nachricht senden …'
              : newMode === 'ask'
                ? 'Stelle eine Frage zum Projekt …'
                : newMode === 'plan'
                  ? 'Was möchtest du planen?'
                  : 'Welche Aufgabe soll A^3 erledigen?'}
            rows="3"></textarea>
          <div class="composer-toolbar">
            <div>
              <span class="context-note">● Aktiver Worktree · aktueller Indexkontext</span>
            </div>
            <button
              class="send-button"
              type="button"
              disabled={!canSubmit}
              onclick={() => void submit()}
              aria-label="Nachricht senden">↑</button
            >
          </div>
        </div>
        <p class="composer-hint">
          Enter senden · Shift + Enter neue Zeile · Privilegierte Aktionen brauchen eine explizite
          Freigabe.
        </p>
      </div>
    </main>

    {#if inspectorOpen}<div
        class="resize-handle inspector-resize"
        role="separator"
        aria-label="Inspectorbreite ändern"
        onpointerdown={(event) => beginResize(event, 'inspector')}
      ></div>{/if}
    {#if !inspectorOpen}<button
        class="reopen-pane reopen-inspector"
        type="button"
        onclick={toggleInspector}
        aria-label="Inspector öffnen">◫</button
      >{/if}

    <aside class="inspector" aria-label="Agent Inspector">
      <header class="inspector-header">
        <strong>Inspector</strong><button
          class="icon-button"
          type="button"
          onclick={toggleInspector}
          aria-label="Inspector einklappen">›</button
        >
      </header>
      <nav class="inspector-tabs" aria-label="Inspector Ansichten">
        <button
          type="button"
          aria-current={inspectorTab === 'progress' ? 'page' : undefined}
          onclick={() => (inspectorTab = 'progress')}>Fortschritt</button
        >
        <button
          type="button"
          aria-current={inspectorTab === 'changes' ? 'page' : undefined}
          onclick={() => (inspectorTab = 'changes')}>Änderungen</button
        >
        <button
          type="button"
          aria-current={inspectorTab === 'review' ? 'page' : undefined}
          onclick={() => (inspectorTab = 'review')}>Review</button
        >
      </nav>
      <div class="inspector-content">
        {#if !activeTaskId}
          <div class="inspector-empty">
            <span aria-hidden="true">◎</span>
            <p>Run-Details erscheinen hier, sobald eine geprüfte Agent-Ausführung startet.</p>
          </div>
        {:else if inspectorTab === 'progress'}
          {#if activityLoading}<p role="status">Fortschritt wird geladen …</p>
          {:else if activity?.run}
            <section class="run-summary">
              <p class="section-label">Aktueller Run</p>
              <h3>{activity.run.state}</h3>
              <p>
                Schritt {activity.run.stepId.slice(0, 8)} · Run {activity.run.runId.slice(0, 8)}
              </p>
            </section>
            <ol class="activity-timeline">
              {#each activity.run.timeline as event (event.sequence)}
                <li>
                  <span></span>
                  <div>
                    <strong>{event.code}</strong>
                    <p>{event.event.kind}</p>
                  </div>
                </li>
              {/each}
            </ol>
          {:else}<p>Für diese Aufgabe existiert noch kein aktiver Run.</p>{/if}
        {:else if inspectorTab === 'changes'}
          <AgentInspectionPanel
            taskId={activeTaskId}
            loader={inspectionLoader}
            logLoader={inspectionLogLoader}
          />
        {:else}
          <div class="review-stack">
            <AgentInspectionPanel
              taskId={activeTaskId}
              loader={inspectionLoader}
              logLoader={inspectionLogLoader}
            />
            <AgentApprovalCenter
              taskId={activeTaskId}
              loader={approvalLoader}
              controller={approvalController}
              onChanged={() => loadActivity(activeTaskId)}
            />
          </div>
        {/if}
      </div>
    </aside>
  {/if}
</section>

<style>
  .agent-workspace {
    position: relative;
    display: grid;
    grid-template-columns: var(--history-width) 1px minmax(26rem, 1fr) 1px var(--inspector-width);
    width: 100%;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    color: var(--color-text);
    background: var(--color-surface-raised);
  }
  .agent-workspace.history-collapsed {
    grid-template-columns: 0 minmax(0, 1fr) 1px var(--inspector-width);
  }
  .agent-workspace.inspector-collapsed {
    grid-template-columns: var(--history-width) 1px minmax(0, 1fr) 0;
  }
  .agent-workspace.history-collapsed.inspector-collapsed {
    grid-template-columns: 0 minmax(0, 1fr) 0;
  }
  .session-rail,
  .inspector {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--color-surface-subtle);
  }
  .session-rail {
    display: flex;
    flex-direction: column;
    border-inline-end: 1px solid var(--color-border-soft);
  }
  .history-collapsed .session-rail,
  .inspector-collapsed .inspector {
    visibility: hidden;
  }
  .rail-header,
  .inspector-header {
    display: flex;
    min-height: 3.5rem;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-3);
    border-block-end: 1px solid var(--color-border-soft);
  }
  .icon-button,
  .reopen-pane {
    display: grid;
    width: 2.25rem;
    min-height: 2.25rem;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
  }
  .icon-button:hover,
  .reopen-pane:hover {
    color: var(--color-text);
    background: var(--color-surface-muted);
  }
  .new-session {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 2.6rem;
    margin: var(--space-3);
    gap: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-heading);
    background: var(--color-surface-raised);
    cursor: pointer;
    font-weight: 650;
  }
  .new-session:hover {
    border-color: var(--color-accent);
    color: var(--color-accent-text);
  }
  .session-search {
    padding: 0 var(--space-3) var(--space-2);
  }
  .session-search input {
    width: 100%;
    min-height: 2.35rem;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    color: var(--color-text);
    background: var(--color-surface);
  }
  .archive-toggle {
    display: flex;
    align-items: center;
    padding: 0 var(--space-3) var(--space-2);
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  .archive-toggle input {
    min-height: auto;
  }
  .session-list {
    min-height: 0;
    padding: 0 var(--space-2) var(--space-3);
    overflow-y: auto;
  }
  .session-item {
    display: grid;
    width: 100%;
    min-height: 5rem;
    padding: var(--space-3);
    gap: var(--space-1);
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-text);
    text-align: start;
    background: transparent;
    cursor: pointer;
  }
  .session-item:hover {
    background: var(--color-surface-muted);
  }
  .session-item.selected {
    box-shadow: inset 3px 0 var(--color-accent);
    background: var(--color-accent-surface);
  }
  .session-title {
    overflow: hidden;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session-meta {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .session-state {
    color: var(--color-subtle);
    font-size: var(--font-size-xs);
  }
  .session-state.attention {
    color: var(--color-warning);
    font-weight: 700;
  }
  .rail-state {
    padding: var(--space-3);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  .resize-handle {
    z-index: 3;
    width: 1px;
    min-height: 0;
    background: var(--color-border-soft);
    cursor: col-resize;
  }
  .resize-handle:hover {
    width: 3px;
    margin-inline: -1px;
    background: var(--color-accent);
  }
  .conversation {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-width: 0;
    min-height: 0;
    background: var(--color-surface-raised);
  }
  .conversation-header {
    display: flex;
    min-height: 3.5rem;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) var(--space-4);
    border-block-end: 1px solid var(--color-border-soft);
  }
  .conversation-header h2 {
    margin: 0;
    overflow: hidden;
    font-size: 0.95rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .conversation-header p {
    margin: var(--space-1) 0 0;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .runtime-controls {
    display: flex;
    gap: var(--space-1);
    margin-inline-start: auto;
  }
  .runtime-controls button {
    min-height: 2.75rem;
  }
  .session-menu {
    position: relative;
    margin-inline-start: var(--space-2);
  }
  .menu-popover {
    position: absolute;
    z-index: 20;
    top: calc(100% + var(--space-1));
    right: 0;
    display: grid;
    width: 11rem;
    padding: var(--space-1);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    box-shadow: 0 10px 28px color-mix(in srgb, var(--color-overlay) 22%, transparent);
  }
  .menu-popover button {
    min-height: 2.25rem;
    padding: 0 var(--space-3);
    border: 0;
    color: var(--color-text);
    text-align: start;
    background: transparent;
    cursor: pointer;
  }
  .menu-popover button:hover {
    background: var(--color-surface-muted);
  }
  .menu-popover .danger {
    color: var(--color-danger);
  }
  .message-scroll {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .messages {
    width: min(100% - 2rem, 48rem);
    margin: 0 auto;
    padding: var(--space-6) 0 var(--space-7);
  }
  .message {
    margin-block-end: var(--space-5);
  }
  .message > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-block-end: var(--space-2);
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .message > header span:first-child {
    color: var(--color-heading);
    font-weight: 700;
  }
  .message-text {
    overflow-wrap: anywhere;
    line-height: 1.6;
  }
  .message-text p {
    margin: 0 0 var(--space-3);
    white-space: pre-wrap;
  }
  .message-text p:last-child,
  .message-text :is(ul, ol, pre, blockquote):last-child {
    margin-bottom: 0;
  }
  .message-text :is(ul, ol) {
    margin: 0 0 var(--space-3);
    padding-inline-start: var(--space-5);
  }
  .message-text li + li {
    margin-top: var(--space-1);
  }
  .markdown-heading {
    margin: var(--space-4) 0 var(--space-2);
    color: var(--color-heading);
    font-weight: 750;
  }
  .markdown-heading:first-child {
    margin-top: 0;
  }
  .markdown-heading[data-level='1'],
  .markdown-heading[data-level='2'] {
    font-size: 1.05rem;
  }
  .message-text blockquote {
    margin: 0 0 var(--space-3);
    padding-inline-start: var(--space-3);
    border-inline-start: 2px solid var(--color-border-strong);
    color: var(--color-muted);
  }
  .message-text pre {
    max-width: 100%;
    margin: 0 0 var(--space-3);
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface);
  }
  .message-text code {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    white-space: pre;
  }
  .user-message {
    max-width: min(88%, 38rem);
    margin-inline-start: auto;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-card);
    background: var(--color-surface-subtle);
  }
  .agent-message {
    padding-inline: var(--space-1);
  }
  .plan-message {
    padding: var(--space-4);
    border-inline-start: 3px solid var(--color-accent);
    background: var(--color-surface-subtle);
  }
  .pending {
    opacity: 0.7;
  }
  .working {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    color: var(--color-muted);
  }
  .working p {
    margin: var(--space-1) 0 0;
  }
  .working-dot {
    width: 0.65rem;
    height: 0.65rem;
    margin-top: 0.35rem;
    border-radius: 50%;
    background: var(--color-status-pending);
    box-shadow: 0 0 0 4px var(--color-status-pending-ring);
  }
  .plan-actions {
    display: flex;
    flex-wrap: wrap;
    margin-top: var(--space-4);
    gap: var(--space-2);
  }
  .plan-actions button,
  .center-state button {
    min-height: 2.4rem;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-text);
    background: var(--color-surface);
    cursor: pointer;
  }
  .plan-actions .primary {
    border-color: var(--color-accent-strong);
    color: var(--color-on-accent);
    background: var(--color-accent-strong);
  }
  .welcome,
  .center-state {
    display: grid;
    width: min(100% - 2rem, 44rem);
    min-height: 100%;
    margin: 0 auto;
    place-content: center;
    justify-items: center;
    padding: var(--space-7);
    text-align: center;
  }
  .welcome-mark,
  .empty-icon {
    display: grid;
    width: 3.5rem;
    height: 3.5rem;
    place-items: center;
    margin-block-end: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: 50%;
    color: var(--color-accent-text);
    background: var(--color-accent-surface);
    font-weight: 800;
  }
  .welcome h3 {
    margin: 0 0 var(--space-2);
    font-size: 1.35rem;
  }
  .welcome > p {
    max-width: 34rem;
    color: var(--color-muted);
  }
  .starter-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    width: 100%;
    margin-top: var(--space-4);
    gap: var(--space-2);
  }
  .starter-grid button {
    display: grid;
    min-height: 5.5rem;
    align-content: center;
    padding: var(--space-3);
    gap: var(--space-2);
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    color: var(--color-heading);
    background: var(--color-surface-subtle);
    cursor: pointer;
  }
  .starter-grid button:hover {
    border-color: var(--color-accent);
  }
  .starter-grid span {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .composer-wrap {
    width: min(100%, 52rem);
    margin: 0 auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
  }
  .composer-box {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-card);
    background: var(--color-surface);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--color-shadow) 22%, transparent);
  }
  .mode-switch {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: var(--space-1);
    border-block-end: 1px solid var(--color-border-soft);
    gap: var(--space-1);
  }
  .mode-switch button {
    display: grid;
    min-height: 3rem;
    padding: var(--space-1) var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
  }
  .mode-switch button[aria-pressed='true'] {
    border-color: var(--color-border);
    color: var(--color-heading);
    background: var(--color-accent-surface);
  }
  .mode-switch span {
    font-size: var(--font-size-xs);
  }
  .composer-box textarea {
    display: block;
    width: 100%;
    min-height: 5rem;
    max-height: 14rem;
    padding: var(--space-3) var(--space-4);
    resize: vertical;
    border: 0;
    outline: 0;
    color: var(--color-text);
    background: transparent;
    line-height: 1.5;
  }
  .composer-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2);
  }
  .composer-toolbar > div {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .context-note {
    color: var(--color-subtle);
    font-size: var(--font-size-xs);
  }
  .send-button {
    display: grid;
    width: 2.35rem;
    min-height: 2.35rem;
    place-items: center;
    border: 0;
    border-radius: 50%;
    color: var(--color-on-accent);
    background: var(--color-accent-strong);
    cursor: pointer;
    font-size: 1.2rem;
  }
  .send-button:disabled {
    color: var(--color-subtle);
    background: var(--color-surface-muted);
    cursor: default;
  }
  .composer-hint {
    margin: var(--space-2) 0 0;
    color: var(--color-subtle);
    text-align: center;
    font-size: var(--font-size-xs);
  }
  .composer-error {
    margin: 0 0 var(--space-2);
    color: var(--color-danger);
    font-size: var(--font-size-sm);
  }
  .inspector {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    border-inline-start: 1px solid var(--color-border-soft);
  }
  .inspector-tabs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: var(--space-2);
    gap: var(--space-1);
    border-block-end: 1px solid var(--color-border-soft);
  }
  .inspector-tabs button {
    min-height: 2.25rem;
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
    font-size: var(--font-size-sm);
  }
  .inspector-tabs button[aria-current='page'] {
    color: var(--color-accent-text);
    background: var(--color-accent-surface);
    font-weight: 700;
  }
  .inspector-content {
    min-height: 0;
    padding: var(--space-3);
    overflow: auto;
  }

  .review-stack {
    display: grid;
    gap: 1rem;
  }
  .inspector-empty {
    display: grid;
    min-height: 16rem;
    place-content: center;
    justify-items: center;
    padding: var(--space-5);
    color: var(--color-muted);
    text-align: center;
  }
  .inspector-empty span {
    font-size: 1.5rem;
  }
  .section-label {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
  }
  .run-summary h3 {
    margin: var(--space-1) 0;
  }
  .run-summary > p:last-child {
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .activity-timeline {
    display: grid;
    padding: 0;
    margin: var(--space-4) 0;
    gap: var(--space-3);
    list-style: none;
  }
  .activity-timeline li {
    display: grid;
    grid-template-columns: 0.7rem minmax(0, 1fr);
    gap: var(--space-2);
  }
  .activity-timeline li > span {
    width: 0.5rem;
    height: 0.5rem;
    margin-top: 0.35rem;
    border-radius: 50%;
    background: var(--color-border-strong);
  }
  .activity-timeline p {
    margin: var(--space-1) 0 0;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .reopen-pane {
    position: absolute;
    z-index: 5;
    top: var(--space-2);
    border: 1px solid var(--color-border-soft);
    background: var(--color-surface);
  }
  .reopen-history {
    left: var(--space-2);
  }
  .reopen-inspector {
    right: var(--space-2);
  }
  .no-project {
    display: grid;
    grid-column: 1 / -1;
    min-height: 100%;
    place-content: center;
    justify-items: center;
    padding: var(--space-7);
    color: var(--color-muted);
    text-align: center;
  }
  .no-project h2 {
    margin-bottom: var(--space-2);
    color: var(--color-heading);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  @media (max-width: 1100px) {
    .agent-workspace {
      grid-template-columns: var(--history-width) 1px minmax(0, 1fr);
    }
    .agent-workspace.history-collapsed {
      grid-template-columns: 0 minmax(0, 1fr);
    }
    .inspector {
      position: absolute;
      z-index: 12;
      inset: 0 0 0 auto;
      width: min(var(--inspector-width), calc(100% - 3rem));
      box-shadow: -12px 0 34px color-mix(in srgb, var(--color-overlay) 28%, transparent);
    }
    .inspector-resize {
      display: none;
    }
    .inspector-collapsed .inspector {
      display: none;
    }
    .conversation-header {
      padding-inline-end: 3.5rem;
    }
  }
  @media (max-width: 760px) {
    .agent-workspace,
    .agent-workspace.history-collapsed,
    .agent-workspace.inspector-collapsed,
    .agent-workspace.history-collapsed.inspector-collapsed {
      grid-template-columns: minmax(0, 1fr);
    }
    .session-rail {
      position: absolute;
      z-index: 14;
      inset: 0 auto 0 0;
      width: min(var(--history-width), calc(100% - 3rem));
      box-shadow: 12px 0 34px color-mix(in srgb, var(--color-overlay) 28%, transparent);
    }
    .history-collapsed .session-rail {
      display: none;
    }
    .history-resize {
      display: none;
    }
    .starter-grid {
      grid-template-columns: 1fr;
    }
    .mode-switch span,
    .context-note,
    .composer-hint {
      display: none;
    }
    .messages {
      width: calc(100% - 1rem);
    }
    .composer-wrap {
      padding-inline: var(--space-2);
    }
    .conversation-header {
      padding-inline: 3.5rem;
    }
  }
</style>

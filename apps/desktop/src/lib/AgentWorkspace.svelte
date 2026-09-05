<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import {
    controlAgentSession,
    controlAgentSessionQueue,
    continueAgentResearch,
    implementAgentSessionPlan,
    queryAgentSession,
    queryAgentSessions,
    queryAgentSlashCommands,
    queryUiPreferences,
    submitAgentMessage,
    updateAgentWorkspaceLayout,
    type AgentSessionControlActionV1,
    type AgentPlanStartOutcomeV1,
    type AgentPlanStartResponseV1,
    type AgentResearchDepthV1,
    type AgentResearchDepthSelectionV1,
    type AgentSessionModeV1,
    type AgentSessionResponseV1,
    type AgentSessionsResponseV1,
    type AgentSessionStateV1,
    type AgentSessionSummaryV1,
    type AgentSessionV1,
    type AgentSlashCommandV1,
    type AgentSlashCommandsResponseV1,
    type UiPreferencesV1,
  } from './agent-session';
  import {
    queryAgentActivity,
    type AgentActivityEventV1,
    type AgentActivityResponseV1,
    type AgentActivityV1,
    type AgentControllerStateV1,
    type AgentSelectedActionV1,
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
  import AgentAskResearch from './AgentAskResearch.svelte';
  import AgentDiagrams from './AgentDiagrams.svelte';
  import { queryAgentDiagramArtifact } from './agent-diagram';
  import ChatMarkdown from './ChatMarkdown.svelte';
  import type {
    AgentAskResearchDetailV1,
    AgentWorkTracePresentationV1,
    AgentWorkTraceSourceV2,
  } from './agent-ask-research';
  import { queryAgentWorkTraceProjection } from './agent-ask-research';
  import { agentSessionRecoveryMessage } from './command-error';
  import { queryTaskLensTask, type TaskLensStepV1, type TaskLensTaskResponseV1 } from './task-lens';

  interface Props {
    activeProject: boolean;
    activityLoader?: (taskId: string) => Promise<AgentActivityResponseV1>;
    diagramArtifactLoader?: typeof queryAgentDiagramArtifact;
    researchProjectionLoader?: typeof queryAgentWorkTraceProjection;
    workPlanLoader?: (query: { taskId: string }) => Promise<TaskLensTaskResponseV1>;
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
    planStarter?: (
      sessionId: string,
      revision: string,
      planRevision: number,
    ) => Promise<AgentPlanStartResponseV1>;
    sessionQueueController?: (
      sessionId: string,
      queueRevision: string,
      action: { kind: 'remove'; queueReference: string } | { kind: 'resume' },
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
      researchDepth?: AgentResearchDepthSelectionV1;
      sessionId?: string | null;
    }) => Promise<AgentSessionResponseV1>;
    researchContinuer?: (
      sessionId: string,
      revision: string,
      depth: AgentResearchDepthV1,
    ) => Promise<AgentSessionResponseV1>;
    slashCommandsLoader?: (mode: AgentSessionModeV1) => Promise<AgentSlashCommandsResponseV1>;
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
    diagramArtifactLoader = queryAgentDiagramArtifact,
    researchProjectionLoader = queryAgentWorkTraceProjection,
    approvalController,
    approvalLoader,
    inspectionLoader,
    inspectionLogLoader,
    onRunStatusChange = () => {},
    planStarter = implementAgentSessionPlan,
    sessionController = controlAgentSession,
    sessionQueueController = controlAgentSessionQueue,
    sessionLoader = queryAgentSession,
    sessionsLoader = queryAgentSessions,
    messageSubmitter = submitAgentMessage,
    slashCommandsLoader = queryAgentSlashCommands,
    researchContinuer = continueAgentResearch,
    pollIntervalMs = 700,
    workPlanLoader = queryTaskLensTask,
  }: Props = $props();

  let sessionsView = $state<SessionsView>({ kind: 'idle' });
  let sessionView = $state<SessionView>({ kind: 'new' });
  let selectedSessionId = $state<string | null>(null);
  let targetMode = $state<AgentSessionModeV1>('agent');
  let researchDepth = $state<AgentResearchDepthV1>('standard');
  const researchDepthBySession = new SvelteMap<string, AgentResearchDepthV1>();
  let composer = $state('');
  let slashCommands = $state<AgentSlashCommandV1[]>([]);
  let slashCatalogLoading = $state(false);
  let slashCatalogMode = $state<AgentSessionModeV1 | null>(null);
  let slashCatalogFailedMode = $state<AgentSessionModeV1 | null>(null);
  let paletteIndex = $state(0);
  let paletteDismissed = $state(false);
  let pendingMessage = $state<string | null>(null);
  let submitting = $state(false);
  let actionError = $state<string | null>(null);
  let searchInput = $state('');
  let includeArchived = $state(false);
  let sessionMenuOpen = $state(false);
  let sessionMenuElement = $state<HTMLDivElement | null>(null);
  let sessionMenuTrigger = $state<HTMLButtonElement | null>(null);
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
  let workPlan = $state<TaskLensTaskResponseV1['result'] | null>(null);
  let workPlanLoading = $state(false);
  let observedProject = false;
  let sessionRequest = 0;
  let sessionsRequest = 0;
  let activityRequest = 0;
  let workPlanRequest = 0;
  let researchRefresh = $state(0);
  let recentlyCompletedResearchSequence = $state<string | null>(null);
  let researchProjections = $state.raw<
    Record<string, { detail: AgentAskResearchDetailV1; sources: AgentWorkTraceSourceV2[] }>
  >({});
  let researchPresentations = $state.raw<Record<string, AgentWorkTracePresentationV1>>({});
  let researchSourceRequest = $state<{
    label: string;
    nonce: number;
    userSequence: string;
  } | null>(null);
  let researchSourceRequestNonce = 0;
  let messageScrollElement = $state<HTMLDivElement | null>(null);
  let messageContentElement = $state<HTMLDivElement | null>(null);
  let followConversation = $state(true);
  let followFrame: number | null = null;
  let manualScrollIntent = false;
  let previousScrollTop = 0;
  const autoOpenedAgentTasks = new SvelteSet<string>();

  const CONVERSATION_END_TOLERANCE_PX = 12;

  const selectedSession = $derived(sessionView.kind === 'available' ? sessionView.session : null);
  const selectedSummary = $derived(selectedSession?.summary ?? null);
  // Poll ownership follows the session/halt state, not each freshly loaded object projection.
  const pollingSessionId = $derived(
    selectedSummary &&
      (['running', 'awaitingApproval', 'paused'].includes(selectedSummary.state) ||
        ((selectedSession?.queuedMessages?.length ?? 0) > 0 && !selectedSession?.queuePaused))
      ? selectedSummary.sessionId
      : null,
  );
  const composerMode = $derived(targetMode);
  const commandActive = $derived(isCommandInput(composer));
  const commandChips = $derived(resolveCommandChips(composer, slashCommands));
  const commandSuggestions = $derived(
    paletteDismissed ? [] : resolveCommandSuggestions(composer, slashCommands),
  );
  const commandInputHint = $derived(resolveCommandInputHint(composer, slashCommands));
  const effectiveMessageDepth = $derived<AgentResearchDepthSelectionV1>(
    commandActive ? 'command' : researchDepth,
  );
  const displayedCommandDepth = $derived(
    commandActive
      ? commandChips.some((command) => command.role === 'lens') ||
        commandChips.some((command) => command.depth === 'thorough')
        ? 'thorough'
        : 'standard'
      : researchDepth,
  );
  const activeTaskId = $derived(selectedSession?.activeTaskId ?? null);
  const agentSidebarVisible = $derived(activeTaskId !== null);
  const latestResearchSequence = $derived(
    selectedSession ? latestUserSequence(selectedSession.entries) : null,
  );
  const latestResearchHasResponse = $derived(
    selectedSession?.entries.at(-1)?.kind !== 'userMessage',
  );
  const conversationTurns = $derived.by(() => {
    const turns: {
      key: string;
      userSequence: string | null;
      entries: AgentSessionV1['entries'];
    }[] = [];
    for (const entry of selectedSession?.entries ?? []) {
      if (entry.kind === 'userMessage' || turns.length === 0) {
        turns.push({
          key: `${selectedSummary?.sessionId}:${entry.sequence}`,
          userSequence: entry.kind === 'userMessage' ? entry.sequence : null,
          entries: [],
        });
      }
      turns[turns.length - 1].entries.push(entry);
    }
    return turns;
  });

  function rememberResearchProjection(
    sessionId: string | undefined,
    userSequence: string,
    projection: { detail: AgentAskResearchDetailV1; sources: AgentWorkTraceSourceV2[] },
  ): void {
    if (!sessionId) return;
    const previous = researchProjections[`${sessionId}:${userSequence}`];
    if (previous?.detail === projection.detail && previous?.sources === projection.sources) return;
    researchProjections = {
      ...researchProjections,
      [`${sessionId}:${userSequence}`]: projection,
    };
  }

  function openResearchSource(userSequence: string, source: AgentWorkTraceSourceV2): void {
    researchSourceRequestNonce += 1;
    researchSourceRequest = {
      label: source.referenceLabel,
      nonce: researchSourceRequestNonce,
      userSequence,
    };
  }

  function rememberResearchPresentation(
    sessionId: string | undefined,
    userSequence: string,
    presentation: AgentWorkTracePresentationV1,
  ): void {
    if (!sessionId) return;
    const key = `${sessionId}:${userSequence}`;
    const previous = researchPresentations[key];
    if (
      previous &&
      Object.entries(presentation).every(
        ([field, value]) => previous[field as keyof AgentWorkTracePresentationV1] === value,
      )
    )
      return;
    researchPresentations = { ...researchPresentations, [key]: presentation };
    rememberResearchProjection(sessionId, userSequence, presentation);
    if (userSequence === latestResearchSequence) queueConversationFollow();
  }
  const presentationCanBeHidden = $derived(
    selectedSummary !== null &&
      !['running', 'awaitingApproval', 'paused'].includes(selectedSummary.state),
  );
  const canSubmit = $derived(
    activeProject &&
      !submitting &&
      composer.trim().length > 0 &&
      composer.length <= 256 * 1024 &&
      commandInputHint === null &&
      (!commandActive ||
        (!slashCatalogLoading && slashCatalogMode === composerMode && slashCommands.length > 0)),
  );

  function modeIsSelectable(mode: AgentSessionModeV1): boolean {
    return selectedSession?.modeOptions?.find((option) => option.mode === mode)?.selectable ?? true;
  }

  function modeRequiresPlanReview(mode: AgentSessionModeV1): boolean {
    if (mode !== 'agent') return false;
    return (
      selectedSession?.modeOptions?.find((option) => option.mode === mode)?.requiresPlanReview ??
      selectedSummary?.mode !== 'agent'
    );
  }

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
    if (!sessionMenuOpen) return;
    const closeOnOutsidePointer = (event: PointerEvent): void => {
      if (!sessionMenuElement?.contains(event.target as Node)) sessionMenuOpen = false;
    };
    document.addEventListener('pointerdown', closeOnOutsidePointer);
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer);
  });

  $effect(() => {
    const mode = composerMode;
    const needsCatalog = commandActive;
    if (
      !activeProject ||
      !needsCatalog ||
      slashCatalogMode === mode ||
      slashCatalogFailedMode === mode ||
      slashCatalogLoading
    )
      return;
    void loadSlashCommands(mode);
  });

  $effect(() => {
    if (activeTaskId) void Promise.all([loadActivity(activeTaskId), loadWorkPlan(activeTaskId)]);
    else {
      activity = null;
      workPlan = null;
    }
  });

  $effect(() => {
    const taskId = activeTaskId;
    if (!taskId || autoOpenedAgentTasks.has(taskId)) return;
    autoOpenedAgentTasks.add(taskId);
    inspectorOpen = true;
  });

  $effect(() => {
    const sessionId = pollingSessionId;
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

  onDestroy(() => {
    if (followFrame !== null) window.cancelAnimationFrame(followFrame);
  });

  async function toggleSessionMenu(): Promise<void> {
    sessionMenuOpen = !sessionMenuOpen;
    if (!sessionMenuOpen) return;
    await tick();
    sessionMenuElement?.querySelector<HTMLButtonElement>('.menu-popover button')?.focus();
  }

  function handleSessionMenuKeydown(event: KeyboardEvent): void {
    if (event.target instanceof Node && messageScrollElement?.contains(event.target))
      handleConversationKeydown(event);
    if (event.key !== 'Escape' || !sessionMenuOpen) return;
    event.preventDefault();
    sessionMenuOpen = false;
    sessionMenuTrigger?.focus();
  }

  function scrollConversationToEnd(viewport: HTMLDivElement): void {
    const tail = viewport.querySelector<HTMLElement>(
      '.conversation-turn:last-child .ask-research[data-live="true"][open] .research-steps li:last-child',
    );
    const maximum = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    // Keep the latest *work*, not the source-list footer, visible in small windows.
    const end = tail
      ? Math.max(
          0,
          Math.min(
            maximum,
            viewport.scrollTop +
              tail.getBoundingClientRect().bottom -
              viewport.getBoundingClientRect().bottom +
              12,
          ),
        )
      : maximum;
    if (Math.abs(viewport.scrollTop - end) > 1) viewport.scrollTop = end;
    previousScrollTop = viewport.scrollTop;
  }

  function queueConversationFollow(): void {
    const viewport = messageScrollElement;
    if (!viewport || !followConversation) return;
    if (followFrame !== null) return;
    followFrame = window.requestAnimationFrame(() => {
      followFrame = null;
      if (followConversation && viewport === messageScrollElement)
        scrollConversationToEnd(viewport);
    });
  }

  function resumeConversationFollow(): void {
    followConversation = true;
    manualScrollIntent = false;
    queueConversationFollow();
  }

  function handleConversationScroll(event: Event): void {
    const viewport = event.currentTarget as HTMLDivElement;
    const distanceFromEnd = viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop;
    // Layout clamping and our own writes are not user intent. Only a deliberate
    // downward scroll may reattach after the reader has left the live tail.
    if (
      manualScrollIntent &&
      viewport.scrollTop > previousScrollTop &&
      distanceFromEnd <= CONVERSATION_END_TOLERANCE_PX
    )
      resumeConversationFollow();
    previousScrollTop = viewport.scrollTop;
  }

  function handleConversationWheel(event: WheelEvent): void {
    if (event.deltaY !== 0) pauseConversationFollow();
  }

  function pauseConversationFollow(): void {
    followConversation = false;
    manualScrollIntent = true;
    previousScrollTop = messageScrollElement?.scrollTop ?? 0;
    if (followFrame !== null) window.cancelAnimationFrame(followFrame);
    followFrame = null;
  }

  function handleConversationPointerDown(): void {
    pauseConversationFollow();
  }

  function handleConversationTouchStart(): void {
    pauseConversationFollow();
  }

  function handleConversationKeydown(event: KeyboardEvent): void {
    if (['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(event.key))
      pauseConversationFollow();
  }

  $effect(() => {
    const viewport = messageScrollElement;
    const content = messageContentElement;
    if (!viewport || !content || typeof ResizeObserver === 'undefined') return;
    // Observe real geometry, not poll identities. Coalesce after layout; never
    // measure/freeze block heights or write during ResizeObserver delivery.
    const observer = new ResizeObserver(() => untrack(queueConversationFollow));
    observer.observe(content);
    observer.observe(viewport);
    untrack(queueConversationFollow);
    return () => observer.disconnect();
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
    researchDepthBySession.clear();
    researchDepth = 'standard';
    sessionsView = { kind: 'noProject' };
    sessionView = { kind: 'new' };
    selectedSessionId = null;
    composer = '';
    slashCommands = [];
    slashCatalogMode = null;
    slashCatalogFailedMode = null;
    paletteIndex = 0;
    paletteDismissed = false;
    pendingMessage = null;
    activity = null;
    recentlyCompletedResearchSequence = null;
    researchProjections = {};
    researchPresentations = {};
    researchSourceRequest = null;
    researchSourceRequestNonce = 0;
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

  async function loadSlashCommands(mode: AgentSessionModeV1): Promise<void> {
    slashCatalogLoading = true;
    slashCatalogFailedMode = null;
    try {
      const response = await slashCommandsLoader(mode);
      if (composerMode !== mode) return;
      slashCommands = response.commands;
      slashCatalogMode = mode;
      paletteIndex = 0;
    } catch {
      if (composerMode === mode) {
        slashCommands = [];
        slashCatalogMode = null;
        slashCatalogFailedMode = mode;
      }
    } finally {
      slashCatalogLoading = false;
    }
  }

  function retrySlashCommands(): void {
    const mode = composerMode;
    slashCatalogFailedMode = null;
    void loadSlashCommands(mode);
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
      const currentSessionAlreadyVisible =
        nextId !== undefined &&
        selectedSessionId === nextId &&
        sessionView.kind === 'available' &&
        sessionView.session.summary.sessionId === nextId;
      if (nextId) {
        if (!currentSessionAlreadyVisible) await selectSession(nextId);
      } else {
        startNewSession();
      }
    } catch {
      if (request === sessionsRequest) sessionsView = { kind: 'error' };
    }
  }

  async function selectSession(sessionId: string): Promise<void> {
    const request = ++sessionRequest;
    if (selectedSessionId !== sessionId) {
      recentlyCompletedResearchSequence = null;
      resumeConversationFollow();
    }
    selectedSessionId = sessionId;
    targetMode =
      sessionView.kind === 'available' && sessionView.session.summary.sessionId === sessionId
        ? sessionView.session.summary.mode
        : targetMode;
    researchDepth = researchDepthBySession.get(sessionId) ?? 'standard';
    sessionMenuOpen = false;
    actionError = null;
    sessionView = { kind: 'loading' };
    try {
      const response = await sessionLoader(sessionId);
      if (request !== sessionRequest || selectedSessionId !== sessionId) return;
      if (response.result.status === 'available') {
        sessionView = { kind: 'available', session: response.result.session };
        targetMode = response.result.session.summary.mode;
        await tick();
        queueConversationFollow();
      } else if (response.result.status === 'notFound') sessionView = { kind: 'missing' };
      else sessionView = { kind: 'error' };
    } catch {
      if (request === sessionRequest) sessionView = { kind: 'error' };
    }
  }

  async function pollSession(sessionId: string): Promise<void> {
    try {
      const response = await sessionLoader(sessionId);
      if (selectedSessionId !== sessionId || response.result.status !== 'available') return;
      const previous = selectedSession;
      const next = response.result.session;
      if (previous && !isMonotonicSessionProjection(previous, next)) return;
      if (previous?.summary.state === 'running' && next.summary.state !== 'running') {
        recentlyCompletedResearchSequence = latestUserSequence(next.entries);
      }
      sessionView = { kind: 'available', session: next };
      researchRefresh += 1;
      if (next.activeTaskId) {
        await Promise.all([loadActivity(next.activeTaskId), loadWorkPlan(next.activeTaskId)]);
      }
      if (!['running', 'awaitingApproval', 'paused'].includes(next.summary.state)) {
        await loadSessions(sessionId);
      }
    } catch {
      // A transient poll error leaves the last verified session projection visible.
    }
  }

  function startNewSession(): void {
    sessionRequest += 1;
    selectedSessionId = null;
    targetMode = 'agent';
    researchDepth = 'standard';
    sessionView = { kind: 'new' };
    composer = '';
    paletteIndex = 0;
    paletteDismissed = false;
    pendingMessage = null;
    actionError = null;
    sessionMenuOpen = false;
    recentlyCompletedResearchSequence = null;
    resumeConversationFollow();
  }

  async function submit(): Promise<void> {
    if (!canSubmit) return;
    const message = composer.trim();
    const submittedDepth = effectiveMessageDepth;
    composer = '';
    await dispatchMessage(message, targetMode, submittedDepth, true);
  }

  async function dispatchMessage(
    message: string,
    mode: AgentSessionModeV1,
    submittedDepth: AgentResearchDepthSelectionV1,
    restoreComposerOnFailure: boolean,
  ): Promise<void> {
    const current = selectedSession;
    resumeConversationFollow();
    pendingMessage = message;
    submitting = true;
    actionError = null;
    recentlyCompletedResearchSequence = null;
    try {
      const response = await messageSubmitter(
        current
          ? {
              expectedSessionRevision: current.summary.revision,
              message,
              mode,
              researchDepth: submittedDepth,
              sessionId: current.summary.sessionId,
            }
          : { message, mode, researchDepth: submittedDepth },
      );
      if (response.result.status === 'available') {
        selectedSessionId = response.result.session.summary.sessionId;
        researchDepthBySession.set(response.result.session.summary.sessionId, researchDepth);
        sessionView = { kind: 'available', session: response.result.session };
        researchRefresh += 1;
        await tick();
        queueConversationFollow();
        await loadSessions(selectedSessionId);
      } else {
        actionError = 'Das aktive Projekt ist nicht mehr verfügbar.';
      }
    } catch (error) {
      if (restoreComposerOnFailure) composer = message;
      actionError = agentSessionRecoveryMessage(error, 'submit');
    } finally {
      pendingMessage = null;
      submitting = false;
    }
  }

  async function regenerateDiagram(userSequence: string): Promise<void> {
    const current = selectedSession;
    if (!current || submitting) return;
    const original = current.entries.find(
      (entry) => entry.kind === 'userMessage' && entry.sequence === userSequence,
    )?.text;
    if (!original) {
      actionError = 'Der ursprüngliche Diagrammauftrag ist nicht mehr in dieser Ansicht verfügbar.';
      return;
    }
    const subject = original
      .replace(/^\s*\/diagram(?:\s+|$)/u, '')
      .trim()
      .slice(0, 48 * 1024);
    if (!subject) {
      actionError = 'Das Thema des ursprünglichen Diagramms konnte nicht wiederhergestellt werden.';
      return;
    }
    targetMode = 'ask';
    const message = `/diagram ${subject}\n\nErzeuge das Diagramm erneut als einfache, sicher darstellbare Struktur. Behalte nur belegte Elemente und Beziehungen bei.`;
    await dispatchMessage(message, 'ask', 'command', false);
  }

  async function removeQueuedMessage(queueReference: string): Promise<void> {
    const current = selectedSession;
    if (!current?.queueRevision) return;
    actionError = null;
    try {
      const response = await sessionQueueController(
        current.summary.sessionId,
        current.queueRevision,
        { kind: 'remove', queueReference },
      );
      if (response.result.status === 'available')
        sessionView = { kind: 'available', session: response.result.session };
    } catch {
      actionError = 'Die vorgemerkte Nachricht konnte nicht entfernt werden. Lade die Session neu.';
    }
  }

  async function resumeQueuedMessages(): Promise<void> {
    const current = selectedSession;
    if (!current?.queueRevision) return;
    actionError = null;
    try {
      const response = await sessionQueueController(
        current.summary.sessionId,
        current.queueRevision,
        { kind: 'resume' },
      );
      if (response.result.status === 'available')
        sessionView = { kind: 'available', session: response.result.session };
    } catch {
      actionError = 'Die Warteschlange konnte nicht fortgesetzt werden. Lade die Session neu.';
    }
  }

  async function continueResearch(): Promise<void> {
    const current = selectedSession;
    if (!current || submitting) return;
    resumeConversationFollow();
    submitting = true;
    actionError = null;
    try {
      researchDepthBySession.set(current.summary.sessionId, researchDepth);
      const response = await researchContinuer(
        current.summary.sessionId,
        current.summary.revision,
        researchDepth,
      );
      if (response.result.status !== 'available') {
        actionError = 'Die Recherche kann für diese Session nicht fortgesetzt werden.';
        return;
      }
      sessionView = { kind: 'available', session: response.result.session };
      researchRefresh += 1;
      await tick();
      queueConversationFollow();
      await loadSessions(response.result.session.summary.sessionId);
    } catch {
      actionError = 'Die Recherche konnte nicht sicher fortgesetzt werden.';
    } finally {
      submitting = false;
    }
  }

  function composerKeydown(event: KeyboardEvent): void {
    if (commandSuggestions.length > 0) {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        paletteIndex = (paletteIndex + 1) % commandSuggestions.length;
        return;
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        paletteIndex = (paletteIndex - 1 + commandSuggestions.length) % commandSuggestions.length;
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        paletteDismissed = true;
        return;
      }
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        selectSlashSuggestion(commandSuggestions[paletteIndex] ?? commandSuggestions[0]);
        return;
      }
    }
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  function composerInput(): void {
    paletteDismissed = false;
    paletteIndex = 0;
  }

  function selectSlashSuggestion(command: AgentSlashCommandV1 | undefined): void {
    if (!command) return;
    const current = composer.trimStart();
    const trailingSpace = /\s$/u.test(current);
    const tokens = current.split(/\s+/u).filter(Boolean);
    if (trailingSpace) tokens.push('');
    if (tokens.length <= 1) {
      composer =
        command.role === 'lens' && command.implicitPrimary
          ? `${command.implicitPrimary} ${command.name} `
          : `${command.name} `;
    } else {
      tokens[tokens.length - 1] = command.name;
      composer = `${tokens.join(' ')} `;
    }
    paletteIndex = 0;
    paletteDismissed = false;
  }

  function removeCommandChip(name: string): void {
    const tokens = composer.trimStart().split(/\s+/u).filter(Boolean);
    const index = tokens.indexOf(name);
    if (index < 0) return;
    tokens.splice(index, 1);
    composer = tokens.join(' ');
    if (composer.length > 0) composer += ' ';
    paletteIndex = 0;
    paletteDismissed = false;
  }

  function selectResearchDepth(depth: AgentResearchDepthV1): void {
    researchDepth = depth;
    if (selectedSessionId) researchDepthBySession.set(selectedSessionId, depth);
  }

  function isCommandInput(value: string): boolean {
    const trimmed = value.trimStart();
    return trimmed.startsWith('/') && !trimmed.startsWith('//');
  }

  function resolveCommandChips(
    value: string,
    catalog: AgentSlashCommandV1[],
  ): AgentSlashCommandV1[] {
    if (!isCommandInput(value)) return [];
    const byName = new Map(catalog.map((command) => [command.name, command]));
    const chips: AgentSlashCommandV1[] = [];
    for (const token of value.trimStart().split(/\s+/u)) {
      if (!token.startsWith('/')) break;
      const command = byName.get(token);
      if (!command) break;
      chips.push(command);
      if (chips.length === 3) break;
    }
    return chips;
  }

  function entryCommandChips(entry: AgentSessionV1['entries'][number]): Array<{ name: string }> {
    return entry.command
      ? [entry.command.primary, ...entry.command.lenses].map((name) => ({ name }))
      : [];
  }

  function resolveCommandSuggestions(
    value: string,
    catalog: AgentSlashCommandV1[],
  ): AgentSlashCommandV1[] {
    if (!isCommandInput(value) || catalog.length === 0) return [];
    const trimmed = value.trimStart();
    const onlyCommandTokens = /^(?:\/[a-z0-9-]*\s*){1,3}$/u.test(trimmed);
    if (!onlyCommandTokens) return [];
    const trailingSpace = /\s$/u.test(trimmed);
    const tokens = trimmed.split(/\s+/u).filter(Boolean);
    const prefix = trailingSpace ? '' : (tokens.pop() ?? '');
    const selected = new Set(tokens);
    const first = catalog.find((command) => command.name === tokens[0]);
    const selectingLens = first !== undefined;
    return catalog.filter(
      (command) =>
        command.available &&
        !selected.has(command.name) &&
        (!selectingLens || command.role === 'lens') &&
        command.name.startsWith(prefix),
    );
  }

  function commandSubjectText(value: string, catalog: Array<{ name: string }>): string {
    const names = new Set(catalog.map((command) => command.name));
    const tokens = value.trimStart().split(/\s+/u).filter(Boolean);
    let consumed = 0;
    while (consumed < tokens.length && consumed < 3 && names.has(tokens[consumed])) consumed += 1;
    const subject = tokens.slice(consumed).join(' ');
    if (subject) return subject;
    if (tokens[0] === '/impact') return 'Aktuelle lokale Änderungen untersuchen';
    if (tokens[0] === '/review' || tokens[0] === '/todos') return 'Gesamtes Repository untersuchen';
    return 'Konkretes Ziel gemeinsam klären';
  }

  function resolveCommandInputHint(value: string, catalog: AgentSlashCommandV1[]): string | null {
    if (!isCommandInput(value) || catalog.length === 0) return null;
    const byName = new Map(catalog.map((command) => [command.name, command]));
    const tokens = value.trimStart().split(/\s+/u).filter(Boolean);
    const leading: AgentSlashCommandV1[] = [];
    let subjectStart = 0;
    for (const [index, token] of tokens.entries()) {
      if (!token.startsWith('/')) {
        subjectStart = index;
        break;
      }
      const command = byName.get(token);
      if (!command) return `„${token}“ ist unbekannt. Wähle einen Command aus der Liste.`;
      leading.push(command);
      subjectStart = index + 1;
    }
    const primary = leading.filter((command) => command.role === 'primary');
    const lenses = leading.filter((command) => command.role === 'lens');
    if (primary.length > 1) return 'Pro Nachricht ist genau ein Haupt-Command erlaubt.';
    if (leading[0]?.role === 'lens' && primary.length > 0)
      return 'Eine allein verwendete Linse nutzt automatisch /review; ein weiterer Haupt-Command ist nicht erlaubt.';
    if (lenses.length > 2) return 'Pro Nachricht sind höchstens zwei Linsen erlaubt.';
    if (new Set(lenses.map((lens) => lens.name)).size !== lenses.length)
      return 'Jede Linse darf nur einmal verwendet werden.';
    const effectivePrimary = primary[0] ?? byName.get(leading[0]?.implicitPrimary ?? '');
    if (!effectivePrimary?.available)
      return 'Dieser Command ist im aktuellen Modus nicht verfügbar.';
    if (effectivePrimary.requiresSubject && tokens.slice(subjectStart).join(' ').length === 0)
      return `${effectivePrimary.name} braucht ein konkretes Thema.`;
    return null;
  }

  function latestUserSequence(entries: AgentSessionV1['entries']): string | null {
    return [...entries].reverse().find((entry) => entry.kind === 'userMessage')?.sequence ?? null;
  }

  function isMonotonicSessionProjection(previous: AgentSessionV1, next: AgentSessionV1): boolean {
    if (previous.summary.sessionId !== next.summary.sessionId) return true;
    const previousRevision = BigInt(previous.summary.revision);
    const nextRevision = BigInt(next.summary.revision);
    if (nextRevision < previousRevision) return false;
    const previousTail = previous.entries.at(-1)?.sequence;
    const nextTail = next.entries.at(-1)?.sequence;
    if (previousTail === undefined) return true;
    if (nextTail === undefined) return false;
    return BigInt(nextTail) >= BigInt(previousTail);
  }

  async function applySessionAction(action: AgentSessionControlActionV1): Promise<void> {
    const current = selectedSession;
    if (!current || submitting) return;
    actionError = null;
    try {
      if (action.kind === 'implementPlan') {
        const start = await planStarter(
          current.summary.sessionId,
          current.summary.revision,
          action.planRevision,
        );
        if (start.result.status === 'available') {
          sessionView = { kind: 'available', session: start.result.session };
          actionError = planStartOutcomeMessage(start.result.outcome);
          await loadSessions(start.result.session.summary.sessionId);
        } else if (start.result.status === 'notFound') {
          startNewSession();
          await loadSessions(null);
        } else actionError = 'Das aktive Projekt ist nicht mehr verfügbar.';
        return;
      }
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
    } catch (error) {
      actionError = agentSessionRecoveryMessage(
        error,
        action.kind === 'implementPlan' ? 'implementPlan' : 'control',
      );
      if (action.kind === 'implementPlan') await selectSession(current.summary.sessionId);
    }
  }

  function planStartOutcomeMessage(outcome: AgentPlanStartOutcomeV1): string | null {
    switch (outcome) {
      case 'started':
        return null;
      case 'queued':
        return 'Der geprüfte Plan startet automatisch, sobald die laufende Vorbereitung abgeschlossen ist.';
      case 'planChanged':
        return 'Der Plan wurde inzwischen geändert. Prüfe und bestätige die aktuell sichtbare Revision.';
      case 'indexChanged':
        return 'Der Projektstand hat sich geändert. Lass den Plan mit aktuellen Quellen neu prüfen.';
      case 'unavailable':
        return 'Die lokale Agentenlaufzeit ist momentan nicht verfügbar. Der geprüfte Plan bleibt erhalten.';
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

  async function loadWorkPlan(taskId: string): Promise<void> {
    const request = ++workPlanRequest;
    workPlanLoading = true;
    try {
      const response = await workPlanLoader({ taskId });
      if (request !== workPlanRequest || taskId !== activeTaskId) return;
      workPlan = response.result;
    } catch {
      if (request === workPlanRequest) workPlan = null;
    } finally {
      if (request === workPlanRequest) workPlanLoading = false;
    }
  }

  function workPlanStepStatus(status: TaskLensStepV1['status']): string {
    const labels: Record<TaskLensStepV1['status'], string> = {
      awaitingApproval: 'Wartet auf Freigabe',
      blocked: 'Braucht Entscheidung',
      cancelled: 'Abgebrochen',
      completed: 'Erledigt',
      failed: 'Fehlgeschlagen',
      inProgress: 'In Arbeit',
      pending: 'Geplant',
      ready: 'Als Nächstes',
      stale: 'Erneut zu prüfen',
      verifying: 'Wird geprüft',
    };
    return labels[status];
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

  function controllerStateLabel(state: AgentControllerStateV1): string {
    const labels: Record<AgentControllerStateV1, string> = {
      intake: 'Aufgabe wird aufgenommen',
      localize: 'Relevante Stellen werden gesucht',
      plan: 'Vorgehen wird vorbereitet',
      execute: 'Änderungen werden umgesetzt',
      verify: 'Ergebnis wird geprüft',
      replan: 'Vorgehen wird angepasst',
      awaitApproval: 'Wartet auf deine Freigabe',
      done: 'Erfolgreich abgeschlossen',
      failed: 'Konnte nicht abgeschlossen werden',
      cancelled: 'Abgebrochen',
    };
    return labels[state];
  }

  function selectedActionLabel(action: AgentSelectedActionV1): string {
    const labels: Record<AgentSelectedActionV1, string> = {
      search: 'Weitere Belege suchen',
      inspect: 'Relevanten Code lesen',
      updateLedger: 'Vorgehen anpassen',
      finish: 'Abschluss prüfen',
      applyPatch: 'Änderung anwenden',
      run: 'Prüfung ausführen',
    };
    return labels[action];
  }

  function activityEventLabel(item: AgentActivityEventV1): string {
    const event = item.event;
    switch (event.kind) {
      case 'runStarted':
        return 'Umsetzung vorbereitet';
      case 'stateTransition':
        return controllerStateLabel(event.to);
      case 'contextCompiled':
        return 'Arbeitskontext zusammengestellt';
      case 'modelInteraction':
        return event.turn?.selectedAction
          ? selectedActionLabel(event.turn.selectedAction)
          : 'Nächsten sicheren Schritt bestimmt';
      case 'toolAction':
        return 'Sichere Aktion ausgeführt';
      case 'ledgerUpdated':
        return 'Vorgehen an neue Erkenntnisse angepasst';
      case 'verificationRecorded':
        return 'Ergebnis anhand der Kriterien geprüft';
      case 'approvalRecorded':
        return 'Freigabe verarbeitet';
      case 'diagnostic':
        return 'Arbeitszustand geprüft';
    }
  }

  function activityEventFeedback(item: AgentActivityEventV1): string {
    if (item.outcome === 'failed') return 'Fehlgeschlagen – der Arbeitsstand bleibt erhalten.';
    if (item.outcome === 'denied') return 'Nicht erlaubt – es wurde nichts ausgeführt.';
    if (item.outcome === 'cancelled') return 'Abgebrochen – es folgen keine weiteren Aktionen.';
    switch (item.code) {
      case 'timeout':
        return 'Das Zeitlimit wurde erreicht.';
      case 'invalidModelOutput':
        return 'Der Vorschlag war nicht sicher ausführbar.';
      case 'toolFailure':
        return 'Die Aktion konnte nicht sicher abgeschlossen werden.';
      case 'verificationFailure':
        return 'Die Prüfung hat noch offene Probleme gefunden.';
      case 'policyDecision':
        return 'Die Sicherheitsregeln wurden vor der Ausführung geprüft.';
      case 'stateRecovered':
        return 'Der letzte sichere Arbeitsstand wurde wiederhergestellt.';
      case 'cancellation':
        return 'Der Abbruch wurde verarbeitet.';
      case 'userRequest':
        return 'Aus deiner bestätigten Aufgabe abgeleitet.';
      case 'controllerDecision':
        return 'Vom sicheren Ablaufcontroller bestätigt.';
      case 'none':
        return item.outcome === 'succeeded' ? 'Erledigt.' : 'Wird verarbeitet.';
    }
  }

  function activityEventState(
    item: AgentActivityEventV1,
    latestSequence: string | undefined,
    terminal: boolean,
  ): 'active' | 'cancelled' | 'done' | 'failed' {
    if (item.outcome === 'failed' || item.outcome === 'denied') return 'failed';
    if (item.outcome === 'cancelled') return 'cancelled';
    if (!terminal && item.sequence === latestSequence) return 'active';
    return 'done';
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

<svelte:window onkeydown={handleSessionMenuKeydown} />

<section
  class="agent-workspace"
  class:history-collapsed={!historyOpen}
  class:inspector-collapsed={!agentSidebarVisible || !inspectorOpen}
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
          <div class="header-actions">
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
            {#if activeTaskId && !inspectorOpen}
              <button
                class="icon-button"
                type="button"
                onclick={toggleInspector}
                aria-label="Agentenlauf öffnen">◫</button
              >
            {/if}
            <div class="session-menu" bind:this={sessionMenuElement}>
              <button
                bind:this={sessionMenuTrigger}
                class="icon-button"
                type="button"
                aria-label="Session-Aktionen"
                aria-expanded={sessionMenuOpen}
                onclick={() => void toggleSessionMenu()}>•••</button
              >
              {#if sessionMenuOpen}
                <div class="menu-popover">
                  <button type="button" onclick={renameSession}>Umbenennen</button>
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
          </div>
        {/if}
      </header>

      <div
        class="message-scroll"
        bind:this={messageScrollElement}
        role="region"
        aria-label="Nachrichtenverlauf"
        onscroll={handleConversationScroll}
        onwheel={handleConversationWheel}
        onpointerdown={handleConversationPointerDown}
        ontouchstart={handleConversationTouchStart}
      >
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
                  targetMode = 'ask';
                  composer = 'Wie ist dieser Teil des Projekts aufgebaut?';
                }}>Projekt verstehen <span>Ask</span></button
              >
              <button
                type="button"
                onclick={() => {
                  targetMode = 'plan';
                  composer = 'Erstelle einen umsetzungsreifen Plan für ';
                }}>Änderung planen <span>Plan</span></button
              >
              <button
                type="button"
                onclick={() => {
                  targetMode = 'agent';
                  composer = 'Implementiere ';
                }}>Aufgabe umsetzen <span>Agent</span></button
              >
            </div>
          </div>
        {:else}
          <div class="messages" bind:this={messageContentElement}>
            {#each conversationTurns as turn (turn.key)}
              <div class="conversation-turn" data-turn={turn.userSequence}>
                {#each turn.entries as entry, entryIndex (entry.sequence)}
                  {@const entryCommands =
                    entry.kind === 'userMessage' ? entryCommandChips(entry) : []}
                  {@const responseUserSequence =
                    entry.kind !== 'userMessage' && entryIndex === 1 ? turn.userSequence : null}
                  {@const entryResearchSources = responseUserSequence
                    ? (researchProjections[
                        `${sessionView.session.summary.sessionId}:${responseUserSequence}`
                      ]?.sources ?? [])
                    : []}
                  {@const displayedEntryText =
                    entry.kind === 'userMessage' &&
                    entry.text.startsWith('Recherche fortsetzen. Ursprüngliche Frage:\n')
                      ? 'Recherche fortsetzen'
                      : entryCommands.length > 0
                        ? commandSubjectText(entry.text, entryCommands)
                        : entry.text}
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
                      {#if entryCommands.length > 0}
                        <div class="message-command-chips" aria-label="Slash Commands">
                          {#each entryCommands as command (command.name)}
                            <span>{command.name}</span>
                          {/each}
                        </div>
                      {/if}
                      <ChatMarkdown
                        text={displayedEntryText}
                        sources={entryResearchSources}
                        onsource={(source) =>
                          responseUserSequence && openResearchSource(responseUserSequence, source)}
                      />
                    </div>
                    {#if responseUserSequence}
                      <AgentDiagrams
                        artifactLoader={diagramArtifactLoader}
                        sessionId={sessionView.session.summary.sessionId}
                        userSequence={responseUserSequence}
                        refreshKey={`${sessionView.session.summary.sessionId}:${responseUserSequence}:completed`}
                        summaries={sessionView.session.entries.find(
                          (candidate) => candidate.sequence === responseUserSequence,
                        )?.diagrams}
                        onregenerate={() => regenerateDiagram(responseUserSequence)}
                      />
                    {/if}
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
                {#if turn.userSequence}
                  <AgentAskResearch
                    projectionLoader={researchProjectionLoader}
                    sessionId={sessionView.session.summary.sessionId}
                    userSequence={turn.userSequence}
                    refreshKey={turn.userSequence === latestResearchSequence
                      ? `${sessionView.session.summary.revision}-${researchRefresh}`
                      : `${turn.key}:completed`}
                    live={turn.userSequence === latestResearchSequence &&
                      !latestResearchHasResponse &&
                      sessionView.session.summary.state === 'running'}
                    recentlyCompleted={turn.userSequence === recentlyCompletedResearchSequence}
                    responseVisible={turn.entries.length > 1}
                    sourceRequest={researchSourceRequest}
                    presentation={researchPresentations[turn.key] ?? null}
                    onprojectionchange={(projection) =>
                      rememberResearchProjection(
                        sessionView.kind === 'available'
                          ? sessionView.session.summary.sessionId
                          : undefined,
                        turn.userSequence ?? '',
                        projection,
                      )}
                    onpresentationchange={(presentation) =>
                      rememberResearchPresentation(
                        sessionView.kind === 'available'
                          ? sessionView.session.summary.sessionId
                          : undefined,
                        turn.userSequence ?? '',
                        presentation,
                      )}
                    oncontinue={() => void continueResearch()}
                  />
                {/if}
              </div>
            {/each}
            {#if pendingMessage}
              <article class="message user-message pending">
                <header><span>Du</span><span>Wird gesendet</span></header>
                <div class="message-text">{pendingMessage}</div>
              </article>
            {/if}
            {#if (pendingMessage || sessionView.session.summary.state === 'running') && !latestResearchSequence}
              <article class="message agent-message working" role="status">
                <span class="working-dot"></span>
                <div>
                  <strong>A^3 arbeitet</strong>
                  <p>
                    {sessionView.session.summary.mode === 'plan'
                      ? 'Strukturiert Entscheidungen und Prüfschritte …'
                      : sessionView.session.summary.mode === 'ask'
                        ? 'Recherche wird vorbereitet …'
                        : 'Analysiert Aufgabe, Kontext und sichere Ausführung …'}
                  </p>
                </div>
              </article>
            {/if}
          </div>
        {/if}
      </div>

      <div class="composer-wrap">
        {#if !followConversation && selectedSummary}
          <button class="follow-latest" type="button" onclick={resumeConversationFollow}>
            ↓ Zum neuesten Schritt
          </button>
        {/if}
        {#if selectedSession?.queuedMessages && selectedSession.queuedMessages.length > 0}
          <section class="message-queue" aria-label="Vorgemerkte Nachrichten">
            <header>
              <strong>{selectedSession.queuedMessages.length} vorgemerkt</strong>
              {#if selectedSession.queuePaused}
                <button type="button" onclick={() => void resumeQueuedMessages()}
                  >Mit Warteschlange fortfahren</button
                >
              {/if}
            </header>
            <ol>
              {#each selectedSession.queuedMessages.slice(0, 3) as queued (queued.queueReference)}
                <li>
                  <span class="queue-position">{queued.position}</span>
                  <span class="queue-mode">{modeLabel(queued.targetMode)}</span>
                  <span class="queue-preview">{queued.preview}</span>
                  <button
                    type="button"
                    aria-label={`Vorgemerkte Nachricht ${queued.position} entfernen`}
                    onclick={() => void removeQueuedMessage(queued.queueReference)}>×</button
                  >
                </li>
              {/each}
            </ol>
            {#if selectedSession.queuedMessages.length > 3}
              <details>
                <summary>{selectedSession.queuedMessages.length - 3} weitere</summary>
                <ol start="4">
                  {#each selectedSession.queuedMessages.slice(3) as queued (queued.queueReference)}
                    <li>
                      <span class="queue-position">{queued.position}</span>
                      <span class="queue-mode">{modeLabel(queued.targetMode)}</span>
                      <span class="queue-preview">{queued.preview}</span>
                      <button
                        type="button"
                        aria-label={`Vorgemerkte Nachricht ${queued.position} entfernen`}
                        onclick={() => void removeQueuedMessage(queued.queueReference)}>×</button
                      >
                    </li>
                  {/each}
                </ol>
              </details>
            {/if}
          </section>
        {/if}
        {#if actionError}<p class="composer-error" role="alert">{actionError}</p>{/if}
        <div class="composer-box">
          <div class="mode-switch" aria-label="Modus für die nächste Nachricht">
            <button
              type="button"
              aria-label="Ask Nur lesen und antworten"
              disabled={!modeIsSelectable('ask')}
              class:executing={selectedSummary?.mode === 'ask' &&
                selectedSummary.state === 'running'}
              aria-pressed={targetMode === 'ask'}
              onclick={() => (targetMode = 'ask')}
              ><strong>Ask</strong><span
                >{selectedSummary?.mode === 'ask' && selectedSummary.state === 'running'
                  ? 'Wird ausgeführt'
                  : targetMode === 'ask'
                    ? 'Als Nächstes'
                    : 'Lesen & antworten'}</span
              ></button
            >
            <button
              type="button"
              aria-label="Plan Gemeinsam ausarbeiten"
              disabled={!modeIsSelectable('plan')}
              class:executing={selectedSummary?.mode === 'plan' &&
                selectedSummary.state === 'running'}
              aria-pressed={targetMode === 'plan'}
              onclick={() => (targetMode = 'plan')}
              ><strong>Plan</strong><span
                >{selectedSummary?.mode === 'plan' && selectedSummary.state === 'running'
                  ? 'Wird ausgeführt'
                  : targetMode === 'plan'
                    ? 'Als Nächstes'
                    : 'Plan erarbeiten'}</span
              ></button
            >
            <button
              type="button"
              aria-label="Agent Änderungen ausführen"
              disabled={!modeIsSelectable('agent')}
              class:executing={selectedSummary?.mode === 'agent' &&
                selectedSummary.state === 'running'}
              class:requires-review={targetMode === 'agent' && modeRequiresPlanReview('agent')}
              aria-pressed={targetMode === 'agent'}
              onclick={() => (targetMode = 'agent')}
              ><strong>Agent</strong><span
                >{selectedSummary?.mode === 'agent' && selectedSummary.state === 'running'
                  ? 'Wird ausgeführt'
                  : targetMode === 'agent' && modeRequiresPlanReview('agent')
                    ? 'Nach Planfreigabe'
                    : targetMode === 'agent'
                      ? 'Als Nächstes'
                      : 'Sicher umsetzen'}</span
              ></button
            >
          </div>
          {#if commandChips.length > 0}
            <div class="composer-command-chips" aria-label="Aktive Slash Commands">
              {#each commandChips as command (command.name)}
                <button
                  type="button"
                  onclick={() => removeCommandChip(command.name)}
                  aria-label={`${command.name} entfernen`}
                  ><span>{command.name}</span><span aria-hidden="true">×</span></button
                >
              {/each}
              <span class="command-depth"
                >{displayedCommandDepth === 'thorough' ? 'Gründlich' : 'Standard'} · automatisch</span
              >
            </div>
          {/if}
          <textarea
            bind:value={composer}
            onkeydown={composerKeydown}
            oninput={composerInput}
            disabled={submitting}
            aria-label="Nachricht an A^3"
            placeholder={selectedSummary
              ? 'Nachricht senden …'
              : targetMode === 'ask'
                ? 'Stelle eine Frage zum Projekt …'
                : targetMode === 'plan'
                  ? 'Was möchtest du planen?'
                  : 'Welche Aufgabe soll A^3 erledigen?'}
            rows="3"></textarea>
          {#if commandSuggestions.length > 0}
            <div class="slash-palette" role="listbox" aria-label="Slash Commands">
              {#each commandSuggestions as command, index (command.name)}
                <button
                  type="button"
                  role="option"
                  aria-selected={index === paletteIndex}
                  class:active={index === paletteIndex}
                  onclick={() => selectSlashSuggestion(command)}
                >
                  <span><strong>{command.name}</strong><small>{command.title}</small></span>
                  <span class="palette-description">{command.description}</span>
                  <span class="palette-depth"
                    >{command.depth === 'thorough' ? 'Gründlich' : 'Standard'}</span
                  >
                </button>
              {/each}
            </div>
          {:else if commandActive && slashCatalogLoading}
            <div class="slash-palette-loading" role="status">Commands werden geladen …</div>
          {:else if commandActive && slashCatalogFailedMode === composerMode}
            <div class="slash-palette-loading slash-palette-failed" role="alert">
              <span>Die Commands konnten nicht geladen werden.</span>
              <button type="button" onclick={retrySlashCommands}>Erneut laden</button>
            </div>
          {/if}
          {#if commandInputHint}
            <p class="slash-command-error" role="alert">{commandInputHint}</p>
          {/if}
          <div class="composer-toolbar">
            <div>
              <span class="context-note">● Aktiver Worktree · aktueller Indexkontext</span>
            </div>
            <div class="research-depth" aria-label="Recherche-Tiefe für die nächste Nachricht">
              <button
                type="button"
                disabled={commandActive}
                title={commandActive
                  ? 'Der aktive Slash Command legt die Recherche-Tiefe fest.'
                  : 'Standard für die nächste Nachricht verwenden'}
                aria-pressed={displayedCommandDepth === 'standard'}
                onclick={() => selectResearchDepth('standard')}>Standard</button
              >
              <button
                type="button"
                disabled={commandActive}
                title={commandActive
                  ? 'Der aktive Slash Command legt die Recherche-Tiefe fest.'
                  : 'Gründlich für die nächste Nachricht verwenden'}
                aria-pressed={displayedCommandDepth === 'thorough'}
                onclick={() => selectResearchDepth('thorough')}>Gründlich</button
              >
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

    {#if agentSidebarVisible && inspectorOpen}<div
        class="resize-handle inspector-resize"
        role="separator"
        aria-label="Inspectorbreite ändern"
        onpointerdown={(event) => beginResize(event, 'inspector')}
      ></div>{/if}

    {#if agentSidebarVisible && activeTaskId}
      {@const visibleTaskId = activeTaskId}
      <aside class="inspector" aria-label="Agentenlauf">
        <header class="inspector-header">
          <strong>Agentenlauf</strong>
          <button
            class="icon-button"
            type="button"
            onclick={toggleInspector}
            aria-label="Agentenlauf einklappen">›</button
          >
        </header>
        <nav class="inspector-tabs" aria-label="Agentenlauf Ansichten">
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
          {#if inspectorTab === 'progress'}
            {#if workPlanLoading && workPlan === null}
              <p role="status">Arbeitsplan wird geladen …</p>
            {:else if workPlan?.status === 'available'}
              {@const completedSteps = workPlan.steps.filter(
                (step) => step.status === 'completed',
              ).length}
              <section class="agent-work-plan" aria-labelledby="agent-work-plan-heading">
                <header>
                  <div>
                    <p class="section-label">Arbeitsplan · Revision {workPlan.ledgerRevision}</p>
                    <h3 id="agent-work-plan-heading">
                      {completedSteps} von {workPlan.steps.length} Schritten erledigt
                    </h3>
                  </div>
                  <span>{workPlan.steps.length} Todos</span>
                </header>
                {#if workPlan.ledgerRevision > 1}
                  <p class="adaptive-plan-note">
                    Nach einem neuen Befund angepasst; bestätigte Arbeit bleibt erhalten.
                  </p>
                {/if}
                <ol>
                  {#each workPlan.steps as step, index (step.stepId)}
                    <li class:active={step.status === 'inProgress' || step.status === 'verifying'}>
                      <span class="todo-marker" aria-hidden="true">
                        {step.status === 'completed' ? '✓' : index + 1}
                      </span>
                      <div>
                        <strong>{step.intendedOutcome}</strong>
                        <small>{workPlanStepStatus(step.status)}</small>
                      </div>
                    </li>
                  {/each}
                </ol>
              </section>
            {/if}
            {#if activityLoading}<p role="status">Fortschritt wird geladen …</p>
            {:else if activity?.run}
              <section class="run-summary">
                <p class="section-label">Umsetzung & Prüfung</p>
                <h3>{controllerStateLabel(activity.run.state)}</h3>
                <p>Der sichere Agent arbeitet den belegten Plan schrittweise ab.</p>
              </section>
              <ol class="activity-timeline">
                {#each activity.run.timeline as event (event.sequence)}
                  {@const eventState = activityEventState(
                    event,
                    activity.run.timeline.at(-1)?.sequence,
                    activity.run.terminal,
                  )}
                  <li
                    class={eventState}
                    aria-current={eventState === 'active' ? 'step' : undefined}
                  >
                    <span aria-hidden="true">{eventState === 'done' ? '✓' : ''}</span>
                    <div>
                      <strong>{activityEventLabel(event)}</strong>
                      <p>{activityEventFeedback(event)}</p>
                    </div>
                  </li>
                {/each}
              </ol>
            {:else}<p>Für diese Aufgabe existiert noch kein aktiver Run.</p>{/if}
          {:else if inspectorTab === 'changes'}
            <AgentInspectionPanel
              taskId={visibleTaskId}
              loader={inspectionLoader}
              logLoader={inspectionLogLoader}
            />
          {:else}
            <div class="review-stack">
              <AgentInspectionPanel
                taskId={visibleTaskId}
                loader={inspectionLoader}
                logLoader={inspectionLogLoader}
              />
              <AgentApprovalCenter
                taskId={visibleTaskId}
                loader={approvalLoader}
                controller={approvalController}
                onChanged={() => loadActivity(visibleTaskId)}
              />
            </div>
          {/if}
        </div>
      </aside>
    {/if}
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
  .header-actions {
    position: relative;
    z-index: 15;
    display: flex;
    min-width: max-content;
    align-items: center;
    margin-inline-start: var(--space-3);
    gap: var(--space-1);
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
    width: min(11rem, calc(100vw - 2 * var(--space-3)));
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
    overflow-anchor: none;
    overscroll-behavior: contain;
    scroll-behavior: auto;
  }
  .messages {
    width: min(100% - 2rem, 48rem);
    margin: 0 auto;
    padding: var(--space-6) 0 var(--space-7);
  }
  .conversation-turn + .conversation-turn {
    margin-block-start: var(--space-5);
  }
  .follow-latest {
    display: block;
    margin: 0 auto var(--space-2);
    color: var(--color-muted);
    font-size: var(--font-size-xs);
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
  .message-command-chips,
  .composer-command-chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  .message-command-chips {
    margin-block-end: var(--space-2);
  }
  .message-command-chips span,
  .composer-command-chips button {
    border: 1px solid var(--color-accent);
    border-radius: 999px;
    color: var(--color-accent-text);
    background: var(--color-accent-surface);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
  .message-command-chips span {
    padding: 0.2rem var(--space-2);
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
  .message-queue {
    margin-block-end: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface-subtle);
    font-size: var(--font-size-xs);
  }
  .message-queue > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-block-end: var(--space-1);
    color: var(--color-muted);
  }
  .message-queue ol {
    display: grid;
    padding: 0;
    margin: 0;
    gap: 0.2rem;
    list-style: none;
  }
  .message-queue li {
    display: grid;
    grid-template-columns: 1.25rem 3.5rem minmax(0, 1fr) 1.75rem;
    min-height: 1.8rem;
    align-items: center;
    gap: var(--space-1);
  }
  .queue-position,
  .queue-mode {
    color: var(--color-muted);
  }
  .queue-position {
    text-align: center;
  }
  .queue-preview {
    overflow: hidden;
    color: var(--color-text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .message-queue button {
    min-height: 1.7rem;
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-control);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
  }
  .message-queue button:hover {
    color: var(--color-text);
    background: var(--color-surface-muted);
  }
  .message-queue summary {
    margin-top: var(--space-1);
    color: var(--color-muted);
    cursor: pointer;
  }
  .composer-box {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-card);
    background: var(--color-surface);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--color-shadow) 22%, transparent);
  }
  .mode-switch {
    position: relative;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: var(--space-1);
    border-block-end: 1px solid var(--color-border-soft);
    gap: var(--space-1);
  }
  .mode-switch button {
    position: relative;
    z-index: 1;
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
  .mode-switch button.executing::before {
    position: absolute;
    top: 0.45rem;
    right: 0.45rem;
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 50%;
    content: '';
    background: var(--color-status-pending);
    box-shadow: 0 0 0 3px var(--color-status-pending-ring);
  }
  .mode-switch button.requires-review span {
    color: var(--color-warning);
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
  .composer-command-chips {
    padding: var(--space-2) var(--space-3) 0;
  }
  .composer-command-chips button {
    display: inline-flex;
    min-height: 1.8rem;
    align-items: center;
    padding: 0 var(--space-2);
    gap: var(--space-1);
    cursor: pointer;
  }
  .command-depth {
    margin-inline-start: auto;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .slash-palette,
  .slash-palette-loading {
    margin: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface-raised, var(--color-surface));
    box-shadow: 0 8px 24px color-mix(in srgb, var(--color-shadow) 18%, transparent);
  }
  .slash-palette {
    display: grid;
    max-height: 15rem;
    overflow-y: auto;
    padding: var(--space-1);
  }
  .slash-palette > button {
    display: grid;
    grid-template-columns: minmax(8rem, auto) 1fr auto;
    min-height: 3.1rem;
    align-items: center;
    padding: var(--space-2);
    gap: var(--space-3);
    border: 0;
    border-radius: calc(var(--radius-control) - 2px);
    color: var(--color-text);
    background: transparent;
    cursor: pointer;
    text-align: start;
  }
  .slash-palette > button.active,
  .slash-palette > button:hover {
    background: var(--color-accent-surface);
  }
  .slash-palette > button > span:first-child {
    display: grid;
  }
  .slash-palette strong {
    color: var(--color-heading);
    font-family: var(--font-mono);
  }
  .slash-palette small,
  .palette-description,
  .palette-depth,
  .slash-palette-loading {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .slash-palette-loading {
    padding: var(--space-3);
  }
  .slash-palette-failed {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  .slash-palette-failed button {
    min-height: 2rem;
    padding: 0 var(--space-2);
    cursor: pointer;
  }
  .slash-command-error {
    margin: var(--space-2) var(--space-3) 0;
    color: var(--color-danger-text, var(--color-danger));
    font-size: var(--font-size-xs);
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
  .research-depth {
    display: flex;
    padding: 0.15rem;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface-subtle);
  }
  .research-depth button {
    min-height: 1.9rem;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: calc(var(--radius-control) - 2px);
    color: var(--color-muted);
    background: transparent;
    cursor: pointer;
    font-size: var(--font-size-xs);
  }
  .research-depth button[aria-pressed='true'] {
    color: var(--color-heading);
    background: var(--color-surface);
    box-shadow: 0 1px 3px color-mix(in srgb, var(--color-shadow) 20%, transparent);
  }
  .research-depth button:disabled {
    cursor: default;
    opacity: 0.65;
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
  .agent-work-plan {
    display: grid;
    margin-block-end: var(--space-4);
    gap: var(--space-2);
  }
  .agent-work-plan > header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .agent-work-plan h3 {
    margin: var(--space-1) 0 0;
    font-size: var(--font-size-base);
  }
  .agent-work-plan > header > span {
    flex: 0 0 auto;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .adaptive-plan-note {
    padding: var(--space-2);
    margin: 0;
    border-inline-start: 2px solid var(--color-accent);
    color: var(--color-muted);
    background: var(--color-accent-surface);
    font-size: var(--font-size-xs);
  }
  .agent-work-plan ol {
    display: grid;
    padding: 0;
    margin: 0;
    gap: var(--space-1);
    list-style: none;
  }
  .agent-work-plan li {
    display: grid;
    grid-template-columns: 1.5rem minmax(0, 1fr);
    align-items: start;
    padding: var(--space-2);
    gap: var(--space-2);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
  }
  .agent-work-plan li.active {
    border-color: var(--color-accent);
    background: var(--color-accent-surface);
  }
  .todo-marker {
    display: grid;
    width: 1.35rem;
    height: 1.35rem;
    place-items: center;
    border-radius: 50%;
    color: var(--color-muted);
    background: var(--color-surface-muted);
    font-size: var(--font-size-xs);
  }
  .agent-work-plan li.active .todo-marker {
    color: var(--color-accent-text);
    box-shadow: 0 0 0 3px var(--color-status-pending-ring);
  }
  .agent-work-plan li > div {
    display: grid;
    gap: 0.15rem;
  }
  .agent-work-plan small {
    color: var(--color-muted);
  }
  .run-summary h3 {
    margin: var(--space-1) 0;
  }
  .run-summary > p:last-child {
    color: var(--color-muted);
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
    position: relative;
    display: grid;
    grid-template-columns: 0.7rem minmax(0, 1fr);
    gap: var(--space-2);
  }
  .activity-timeline li:not(:last-child)::before {
    position: absolute;
    top: 0.85rem;
    bottom: calc(-1 * var(--space-3) - 0.35rem);
    left: 0.225rem;
    width: 1px;
    content: '';
    background: var(--color-border-strong);
  }
  .activity-timeline li > span {
    z-index: 1;
    display: grid;
    width: 0.5rem;
    height: 0.5rem;
    margin-top: 0.35rem;
    place-items: center;
    border-radius: 50%;
    color: var(--color-on-accent);
    background: var(--color-border-strong);
    font-size: 0.42rem;
    line-height: 1;
  }
  .activity-timeline li.done > span,
  .activity-timeline li.done::before {
    background: var(--color-status-success);
  }
  .activity-timeline li.active > span {
    background: var(--color-status-pending);
    box-shadow: 0 0 0 4px var(--color-status-pending-ring);
  }
  .activity-timeline li.failed > span,
  .activity-timeline li.failed::before {
    background: var(--color-danger);
  }
  .activity-timeline li.cancelled > span,
  .activity-timeline li.cancelled::before {
    background: var(--color-warning);
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
      padding-inline-end: var(--space-3);
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

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import { parseAgentDiagramSummaryV1, type AgentDiagramSummaryV1 } from './agent-diagram';

const STABLE_ID = /^[0-9a-f]{64}$/u;
const DECIMAL = /^(?:0|[1-9][0-9]{0,18})$/u;
const MODES = ['ask', 'plan', 'agent'] as const;
const RESEARCH_DEPTHS = ['standard', 'thorough'] as const;
const RESEARCH_DEPTH_SELECTIONS = ['standard', 'thorough', 'command'] as const;
const SLASH_COMMAND_ROLES = ['primary', 'lens'] as const;
const STATES = [
  'draft',
  'running',
  'awaitingUser',
  'awaitingPlanReview',
  'awaitingApproval',
  'paused',
  'completed',
  'failed',
  'cancelled',
  'archived',
] as const;
const ENTRY_KINDS = ['userMessage', 'assistantSummary', 'plan', 'finalReport', 'activity'] as const;
const utf8 = new TextEncoder();

export type AgentSessionModeV1 = (typeof MODES)[number];
export type AgentResearchDepthV1 = (typeof RESEARCH_DEPTHS)[number];
export type AgentResearchDepthSelectionV1 = (typeof RESEARCH_DEPTH_SELECTIONS)[number];
export type AgentSessionStateV1 = (typeof STATES)[number];
export type AgentSessionEntryKindV1 = (typeof ENTRY_KINDS)[number];

export interface AgentSessionSummaryV1 {
  currentPlanRevision: number | null;
  mode: AgentSessionModeV1;
  revision: string;
  sessionId: string;
  state: AgentSessionStateV1;
  title: string;
  updatedAtUnixMillis: string;
}

export interface AgentSessionEntryV1 {
  command?: AgentSessionCommandChipsV1 | null;
  createdAtUnixMillis: string;
  diagrams?: AgentDiagramSummaryV1[];
  kind: AgentSessionEntryKindV1;
  planRevision: number | null;
  sequence: string;
  text: string;
}

export interface AgentSessionCommandChipsV1 {
  catalogVersion: 1;
  depth: AgentResearchDepthV1;
  lenses: string[];
  primary: string;
}

export interface AgentSessionV1 {
  activeTaskId: string | null;
  entries: AgentSessionEntryV1[];
  hasOlderEntries: boolean;
  modeOptions?: AgentSessionModeOptionV1[];
  queuePaused?: boolean;
  queuedMessages?: AgentQueuedMessageSummaryV1[];
  queueRevision?: string;
  summary: AgentSessionSummaryV1;
}

export interface AgentSessionModeOptionV1 {
  mode: AgentSessionModeV1;
  requiresPlanReview: boolean;
  selectable: boolean;
}

export interface AgentQueuedMessageSummaryV1 {
  enqueuedAtUnixMillis: string;
  position: number;
  preview: string;
  queueReference: string;
  targetMode: AgentSessionModeV1;
}

export type AgentSessionsResultV1 =
  | { status: 'noProject' }
  | { nextCursor: string | null; sessions: AgentSessionSummaryV1[]; status: 'available' };

export interface AgentSessionsResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentSessionsResultV1;
}

export type AgentSessionResultV1 =
  | { status: 'noProject' }
  | { status: 'notFound' }
  | { session: AgentSessionV1; status: 'available' };

export interface AgentSessionResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentSessionResultV1;
  submissionOutcome?: 'started' | 'queued' | 'requiresPlanReview';
}

export type AgentPlanStartOutcomeV1 =
  'started' | 'queued' | 'planChanged' | 'indexChanged' | 'unavailable';

export type AgentPlanStartResultV1 =
  | { status: 'noProject' }
  | { status: 'notFound' }
  | { outcome: AgentPlanStartOutcomeV1; session: AgentSessionV1; status: 'available' };

export interface AgentPlanStartResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentPlanStartResultV1;
}

export interface AgentSlashCommandV1 {
  available: boolean;
  depth: AgentResearchDepthV1;
  description: string;
  implicitPrimary: string | null;
  name: string;
  requiresSubject: boolean;
  role: (typeof SLASH_COMMAND_ROLES)[number];
  title: string;
}

export interface AgentSlashCommandsResponseV1 {
  catalogVersion: 1;
  commands: AgentSlashCommandV1[];
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export type AgentSessionControlActionV1 =
  | { kind: 'pause' }
  | { kind: 'resume' }
  | { kind: 'cancel' }
  | { kind: 'switchToPlan' }
  | { kind: 'implementPlan'; planRevision: number }
  | { kind: 'rename'; title: string }
  | { kind: 'archive' }
  | { kind: 'unarchive' }
  | { kind: 'deletePresentation' };

export interface UiPreferencesV1 {
  inspectorCollapsed: boolean;
  inspectorWidth: number;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  revision: string;
  sessionRailCollapsed: boolean;
  sessionRailWidth: number;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentSessions(
  options: {
    beforeUpdatedAtUnixMillis?: string | null;
    includeArchived?: boolean;
    limit?: number;
    search?: string | null;
  } = {},
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionsResponseV1> {
  const request = {
    beforeUpdatedAtUnixMillis: options.beforeUpdatedAtUnixMillis ?? null,
    includeArchived: options.includeArchived ?? false,
    limit: options.limit ?? 50,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    search: options.search?.trim() || null,
  };
  return parseAgentSessionsResponseV1(await invokeCommand('query_agent_sessions', { request }));
}

export async function queryAgentSession(
  sessionId: string,
  beforeSequence: string | null = null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionResponseV1> {
  requireStableId(sessionId, 'Agent session');
  const request = {
    beforeSequence,
    limit: 128,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    sessionId,
  };
  return parseAgentSessionResponseV3(await invokeCommand('query_agent_session_v3', { request }));
}

export async function submitAgentMessage(
  input: {
    expectedSessionRevision?: string | null;
    message: string;
    mode?: AgentSessionModeV1;
    researchDepth?: AgentResearchDepthSelectionV1;
    sessionId?: string | null;
  },
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionResponseV1> {
  const message = parseText(input.message, 256 * 1024, 'Agent message');
  if (input.sessionId) requireStableId(input.sessionId, 'Agent session');
  const researchDepth = input.researchDepth ?? 'standard';
  if (!RESEARCH_DEPTH_SELECTIONS.includes(researchDepth))
    throw new Error('Invalid research depth.');
  const request = {
    contextReferences: [],
    expectedSessionRevision: input.expectedSessionRevision ?? null,
    message,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    researchDepth,
    sessionId: input.sessionId ?? null,
    targetMode: input.mode ?? 'agent',
  };
  return parseSubmitAgentMessageResponseV4(
    await invokeCommand('submit_agent_message_v4', { request }),
  );
}

export async function controlAgentSessionQueue(
  sessionId: string,
  expectedQueueRevision: string,
  action: { kind: 'remove'; queueReference: string } | { kind: 'resume' },
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionResponseV1> {
  requireStableId(sessionId, 'Agent session');
  requireDecimal(expectedQueueRevision, 'Agent queue revision', true);
  if (action.kind === 'remove') requireStableId(action.queueReference, 'Agent queue item');
  return parseAgentSessionResponseV3(
    await invokeCommand('control_agent_session_queue', {
      request: {
        action,
        expectedQueueRevision,
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        sessionId,
      },
    }),
  );
}

export async function queryAgentSlashCommands(
  mode: AgentSessionModeV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSlashCommandsResponseV1> {
  if (!MODES.includes(mode)) throw new Error('Invalid Agent mode.');
  const request = { mode, protocolVersion: CURRENT_PROTOCOL_VERSION };
  return parseAgentSlashCommandsResponseV1(
    await invokeCommand('query_agent_slash_commands', { request }),
  );
}

export function parseAgentSlashCommandsResponseV1(payload: unknown): AgentSlashCommandsResponseV1 {
  const value = object(
    payload,
    ['catalogVersion', 'commands', 'protocolVersion'],
    'Agent slash-command response',
  );
  protocol(value.protocolVersion);
  if (value.catalogVersion !== 1 || !Array.isArray(value.commands) || value.commands.length > 32)
    fail('Agent slash-command response');
  const names = new Set<string>();
  const commands = value.commands.map((candidate) => {
    const command = object(
      candidate,
      [
        'available',
        'depth',
        'description',
        'implicitPrimary',
        'name',
        'requiresSubject',
        'role',
        'title',
      ],
      'Agent slash command',
    );
    if (
      typeof command.available !== 'boolean' ||
      typeof command.description !== 'string' ||
      command.description.length === 0 ||
      command.description.length > 512 ||
      typeof command.name !== 'string' ||
      !/^\/[a-z][a-z0-9-]{0,31}$/u.test(command.name) ||
      typeof command.requiresSubject !== 'boolean' ||
      typeof command.title !== 'string' ||
      command.title.length === 0 ||
      command.title.length > 128 ||
      !RESEARCH_DEPTHS.includes(command.depth as AgentResearchDepthV1) ||
      !SLASH_COMMAND_ROLES.includes(command.role as AgentSlashCommandV1['role']) ||
      (command.implicitPrimary !== null &&
        (typeof command.implicitPrimary !== 'string' ||
          !/^\/[a-z][a-z0-9-]{0,31}$/u.test(command.implicitPrimary))) ||
      names.has(command.name)
    )
      fail('Agent slash command');
    names.add(command.name);
    return command as unknown as AgentSlashCommandV1;
  });
  return { catalogVersion: 1, commands, protocolVersion: CURRENT_PROTOCOL_VERSION };
}

export async function continueAgentResearch(
  sessionId: string,
  expectedSessionRevision: string,
  researchDepth: AgentResearchDepthV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionResponseV1> {
  requireStableId(sessionId, 'Agent session');
  requireDecimal(expectedSessionRevision, 'Agent session revision', false);
  if (!RESEARCH_DEPTHS.includes(researchDepth)) throw new Error('Invalid research depth.');
  const request = {
    expectedSessionRevision,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    researchDepth,
    sessionId,
  };
  return parseAgentSessionResponseV1(await invokeCommand('continue_agent_research', { request }));
}

export async function controlAgentSession(
  sessionId: string,
  expectedSessionRevision: string,
  action: AgentSessionControlActionV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentSessionResponseV1> {
  requireStableId(sessionId, 'Agent session');
  requireDecimal(expectedSessionRevision, 'Agent session revision', false);
  const request = {
    action,
    expectedSessionRevision,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    sessionId,
  };
  return parseAgentSessionResponseV1(await invokeCommand('control_agent_session', { request }));
}

export async function implementAgentSessionPlan(
  sessionId: string,
  expectedSessionRevision: string,
  planRevision: number,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentPlanStartResponseV1> {
  requireStableId(sessionId, 'Agent session');
  requireDecimal(expectedSessionRevision, 'Agent session revision', false);
  if (!integer(planRevision, 1, 4_294_967_295)) throw new Error('Invalid plan revision.');
  const request = {
    expectedSessionRevision,
    planRevision,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    sessionId,
  };
  return parseAgentPlanStartResponseV1(
    await invokeCommand('control_agent_session_v2', { request }),
  );
}

export async function queryUiPreferences(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<UiPreferencesV1> {
  const request = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  return parseUiPreferencesV1(await invokeCommand('query_ui_preferences', { request }));
}

export async function updateAgentWorkspaceLayout(
  current: UiPreferencesV1,
  layout: Pick<
    UiPreferencesV1,
    'inspectorCollapsed' | 'inspectorWidth' | 'sessionRailCollapsed' | 'sessionRailWidth'
  >,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<UiPreferencesV1> {
  const request = {
    expectedRevision: current.revision,
    ...layout,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
  };
  return parseUiPreferencesV1(await invokeCommand('update_agent_workspace_layout', { request }));
}

export function parseAgentSessionsResponseV1(payload: unknown): AgentSessionsResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent sessions response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent sessions result');
  if (result.status === 'noProject') {
    exact(result, ['status'], 'Agent sessions result');
    return value as unknown as AgentSessionsResponseV1;
  }
  exact(result, ['nextCursor', 'sessions', 'status'], 'Agent sessions result');
  if (result.status !== 'available' || !Array.isArray(result.sessions))
    fail('Agent sessions result');
  const sessions = result.sessions.map(parseSummary);
  if (sessions.length > 50) fail('Agent sessions page');
  if (result.nextCursor !== null) requireDecimal(result.nextCursor, 'Agent session cursor', true);
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { ...result, sessions },
  } as AgentSessionsResponseV1;
}

export function parseAgentSessionResponseV1(payload: unknown): AgentSessionResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent session response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent session result');
  if (result.status === 'noProject' || result.status === 'notFound') {
    exact(result, ['status'], 'Agent session result');
    return value as unknown as AgentSessionResponseV1;
  }
  exact(result, ['session', 'status'], 'Agent session result');
  if (result.status !== 'available') fail('Agent session result');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { session: parseSession(result.session), status: 'available' },
  };
}

export function parseAgentSessionResponseV2(payload: unknown): AgentSessionResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent session V2 response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent session V2 result');
  if (result.status === 'noProject' || result.status === 'notFound') {
    exact(result, ['status'], 'Agent session V2 result');
    return {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: result.status },
    } as AgentSessionResponseV1;
  }
  exact(result, ['projection', 'status'], 'Agent session V2 result');
  if (result.status !== 'available') fail('Agent session V2 result');
  const projection = object(
    result.projection,
    ['entryAugmentations', 'session'],
    'Agent session V2 projection',
  );
  if (!Array.isArray(projection.entryAugmentations) || projection.entryAugmentations.length > 128)
    fail('Agent session V2 projection');
  const session = parseSession(projection.session);
  const entries = new Map(session.entries.map((entry) => [entry.sequence, entry]));
  const seen = new Set<string>();
  for (const candidate of projection.entryAugmentations) {
    const augmentation = object(
      candidate,
      ['command', 'diagrams', 'userSequence'],
      'Agent session augmentation',
    );
    requireDecimal(augmentation.userSequence, 'Agent session augmentation sequence', false);
    const entry = entries.get(augmentation.userSequence);
    if (!entry || entry.kind !== 'userMessage' || seen.has(augmentation.userSequence))
      fail('Agent session augmentation');
    seen.add(augmentation.userSequence);
    if (!Array.isArray(augmentation.diagrams) || augmentation.diagrams.length > 3)
      fail('Agent session augmentation diagrams');
    const diagrams = augmentation.diagrams.map(parseAgentDiagramSummaryV1);
    if (diagrams.some((diagram) => diagram.userSequence !== augmentation.userSequence))
      fail('Agent session augmentation diagrams');
    entry.command =
      augmentation.command === null ? null : parseSessionCommandChips(augmentation.command);
    entry.diagrams = diagrams;
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { session, status: 'available' },
  };
}

export function parseAgentSessionResponseV3(payload: unknown): AgentSessionResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent session V3 response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent session V3 result');
  if (result.status === 'noProject' || result.status === 'notFound') {
    exact(result, ['status'], 'Agent session V3 result');
    return {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: result.status },
    } as AgentSessionResponseV1;
  }
  exact(result, ['projection', 'status'], 'Agent session V3 result');
  if (result.status !== 'available') fail('Agent session V3 result');
  const projection = object(
    result.projection,
    ['modeOptions', 'projection', 'queuePaused', 'queueRevision', 'queuedMessages'],
    'Agent session V3 projection',
  );
  const nested = parseAgentSessionV2Projection(projection.projection);
  requireDecimal(projection.queueRevision, 'Agent queue revision', true);
  if (typeof projection.queuePaused !== 'boolean') fail('Agent queue paused');
  if (!Array.isArray(projection.modeOptions) || projection.modeOptions.length !== 3)
    fail('Agent mode options');
  const modeOptions = projection.modeOptions.map((candidate) => {
    const option = object(
      candidate,
      ['mode', 'requiresPlanReview', 'selectable'],
      'Agent mode option',
    );
    if (
      !MODES.includes(option.mode as AgentSessionModeV1) ||
      typeof option.requiresPlanReview !== 'boolean' ||
      typeof option.selectable !== 'boolean'
    )
      fail('Agent mode option');
    return option as unknown as AgentSessionModeOptionV1;
  });
  if (new Set(modeOptions.map((option) => option.mode)).size !== MODES.length)
    fail('Agent mode options');
  if (!Array.isArray(projection.queuedMessages) || projection.queuedMessages.length > 16)
    fail('Agent queued messages');
  const queuedMessages = projection.queuedMessages.map((candidate, index) => {
    const queued = object(
      candidate,
      ['enqueuedAtUnixMillis', 'position', 'preview', 'queueReference', 'targetMode'],
      'Agent queued message',
    );
    requireStableId(queued.queueReference, 'Agent queue item');
    requireDecimal(queued.enqueuedAtUnixMillis, 'Agent queue time', true);
    if (
      queued.position !== index + 1 ||
      !MODES.includes(queued.targetMode as AgentSessionModeV1) ||
      typeof queued.preview !== 'string' ||
      queued.preview.length === 0 ||
      queued.preview.length > 121
    )
      fail('Agent queued message');
    return queued as unknown as AgentQueuedMessageSummaryV1;
  });
  nested.modeOptions = modeOptions;
  nested.queuePaused = projection.queuePaused;
  nested.queueRevision = projection.queueRevision;
  nested.queuedMessages = queuedMessages;
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { session: nested, status: 'available' },
  };
}

function parseAgentSessionV2Projection(payload: unknown): AgentSessionV1 {
  const parsed = parseAgentSessionResponseV2({
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { projection: payload, status: 'available' },
  });
  if (parsed.result.status !== 'available') fail('Agent session V2 projection');
  return parsed.result.session;
}

export function parseSubmitAgentMessageResponseV4(payload: unknown): AgentSessionResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent submit V4 response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent submit V4 result');
  if (result.status === 'noProject') {
    exact(result, ['status'], 'Agent submit V4 result');
    return { protocolVersion: CURRENT_PROTOCOL_VERSION, result: { status: 'noProject' } };
  }
  exact(result, ['outcome', 'projection', 'status'], 'Agent submit V4 result');
  if (
    result.status !== 'available' ||
    !['started', 'queued', 'requiresPlanReview'].includes(result.outcome as string)
  )
    fail('Agent submit V4 result');
  const parsed = parseAgentSessionResponseV3({
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { projection: result.projection, status: 'available' },
  });
  parsed.submissionOutcome = result.outcome as AgentSessionResponseV1['submissionOutcome'];
  return parsed;
}

export function parseAgentPlanStartResponseV1(payload: unknown): AgentPlanStartResponseV1 {
  const value = object(payload, ['protocolVersion', 'result'], 'Agent plan-start response');
  protocol(value.protocolVersion);
  const result = object(value.result, undefined, 'Agent plan-start result');
  if (result.status === 'noProject' || result.status === 'notFound') {
    exact(result, ['status'], 'Agent plan-start result');
    return {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: result.status },
    } as AgentPlanStartResponseV1;
  }
  exact(result, ['outcome', 'projection', 'status'], 'Agent plan-start result');
  if (
    result.status !== 'available' ||
    !['started', 'queued', 'planChanged', 'indexChanged', 'unavailable'].includes(
      result.outcome as string,
    )
  )
    fail('Agent plan-start result');
  const parsed = parseAgentSessionResponseV3({
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: { projection: result.projection, status: 'available' },
  });
  if (parsed.result.status !== 'available') fail('Agent plan-start result');
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      outcome: result.outcome as AgentPlanStartOutcomeV1,
      session: parsed.result.session,
      status: 'available',
    },
  };
}

export function parseUiPreferencesV1(payload: unknown): UiPreferencesV1 {
  const value = object(
    payload,
    [
      'inspectorCollapsed',
      'inspectorWidth',
      'protocolVersion',
      'revision',
      'sessionRailCollapsed',
      'sessionRailWidth',
    ],
    'UI preferences',
  );
  protocol(value.protocolVersion);
  requireDecimal(value.revision, 'UI preferences revision', true);
  if (
    !integer(value.sessionRailWidth, 220, 360) ||
    !integer(value.inspectorWidth, 320, 640) ||
    typeof value.sessionRailCollapsed !== 'boolean' ||
    typeof value.inspectorCollapsed !== 'boolean'
  ) {
    fail('UI preferences');
  }
  return value as unknown as UiPreferencesV1;
}

function parseSession(payload: unknown): AgentSessionV1 {
  const value = object(
    payload,
    ['activeTaskId', 'entries', 'hasOlderEntries', 'summary'],
    'Agent session',
  );
  if (
    (value.activeTaskId !== null && typeof value.activeTaskId !== 'string') ||
    !Array.isArray(value.entries) ||
    value.entries.length > 128 ||
    typeof value.hasOlderEntries !== 'boolean'
  ) {
    fail('Agent session');
  }
  if (typeof value.activeTaskId === 'string') requireStableId(value.activeTaskId, 'Agent task');
  const entries = value.entries.map(parseEntry);
  for (let index = 1; index < entries.length; index += 1) {
    if (BigInt(entries[index - 1].sequence) >= BigInt(entries[index].sequence)) {
      fail('Agent session entry order');
    }
  }
  return {
    activeTaskId: value.activeTaskId,
    entries,
    hasOlderEntries: value.hasOlderEntries,
    summary: parseSummary(value.summary),
  } as AgentSessionV1;
}

function parseSummary(payload: unknown): AgentSessionSummaryV1 {
  const value = object(
    payload,
    [
      'currentPlanRevision',
      'mode',
      'revision',
      'sessionId',
      'state',
      'title',
      'updatedAtUnixMillis',
    ],
    'Agent session summary',
  );
  requireStableId(value.sessionId, 'Agent session');
  requireDecimal(value.revision, 'Agent session revision', false);
  requireDecimal(value.updatedAtUnixMillis, 'Agent session time', true);
  if (
    !MODES.includes(value.mode as AgentSessionModeV1) ||
    !STATES.includes(value.state as AgentSessionStateV1) ||
    typeof value.title !== 'string' ||
    utf8.encode(value.title).length === 0 ||
    utf8.encode(value.title).length > 120 ||
    (value.currentPlanRevision !== null && !integer(value.currentPlanRevision, 1, 4_294_967_295))
  ) {
    fail('Agent session summary');
  }
  return value as unknown as AgentSessionSummaryV1;
}

function parseEntry(payload: unknown): AgentSessionEntryV1 {
  const value = object(
    payload,
    ['createdAtUnixMillis', 'kind', 'planRevision', 'sequence', 'text'],
    'Agent session entry',
  );
  requireDecimal(value.sequence, 'Agent session sequence', false);
  requireDecimal(value.createdAtUnixMillis, 'Agent session entry time', true);
  if (
    !ENTRY_KINDS.includes(value.kind as AgentSessionEntryKindV1) ||
    typeof value.text !== 'string' ||
    utf8.encode(value.text).length === 0 ||
    utf8.encode(value.text).length > 256 * 1024 ||
    (value.planRevision !== null && !integer(value.planRevision, 1, 4_294_967_295)) ||
    (value.kind === 'plan') !== (value.planRevision !== null)
  ) {
    fail('Agent session entry');
  }
  return {
    ...(value as unknown as AgentSessionEntryV1),
    command: null,
    diagrams: [],
  };
}

function parseSessionCommandChips(payload: unknown): AgentSessionCommandChipsV1 {
  const value = object(
    payload,
    ['catalogVersion', 'depth', 'lenses', 'primary'],
    'Agent session command chips',
  );
  if (
    value.catalogVersion !== 1 ||
    !RESEARCH_DEPTHS.includes(value.depth as AgentResearchDepthV1) ||
    typeof value.primary !== 'string' ||
    !/^\/[a-z][a-z0-9-]{0,31}$/u.test(value.primary) ||
    !Array.isArray(value.lenses) ||
    value.lenses.length > 2 ||
    value.lenses.some(
      (lens) => typeof lens !== 'string' || !/^\/[a-z][a-z0-9-]{0,31}$/u.test(lens),
    ) ||
    new Set(value.lenses).size !== value.lenses.length
  ) {
    fail('Agent session command chips');
  }
  return value as unknown as AgentSessionCommandChipsV1;
}

function object(
  payload: unknown,
  keys: string[] | undefined,
  label: string,
): Record<string, unknown> {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) fail(label);
  const value = payload as Record<string, unknown>;
  if (keys) exact(value, keys, label);
  return value;
}

function exact(value: Record<string, unknown>, keys: string[], label: string): void {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    fail(label);
  }
}

function protocol(value: unknown): void {
  if (value !== CURRENT_PROTOCOL_VERSION) fail('protocol version');
}

function requireStableId(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !STABLE_ID.test(value)) fail(`${label} identity`);
}

function requireDecimal(
  value: unknown,
  label: string,
  allowZero: boolean,
): asserts value is string {
  if (typeof value !== 'string' || !DECIMAL.test(value) || (!allowZero && value === '0'))
    fail(label);
}

function parseText(value: string, maxBytes: number, label: string): string {
  const normalized = value.trim();
  if (normalized.length === 0 || utf8.encode(normalized).length > maxBytes) fail(label);
  return normalized;
}

function integer(value: unknown, minimum: number, maximum: number): value is number {
  return Number.isInteger(value) && Number(value) >= minimum && Number(value) <= maximum;
}

function fail(label: string): never {
  throw new Error(`${label} does not match V1.`);
}

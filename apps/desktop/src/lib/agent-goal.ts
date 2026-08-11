import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const DECIMAL_MILLIS_PATTERN = /^(?:0|[1-9][0-9]{0,18})$/;
const MAX_PERSISTED_MILLIS = 9_223_372_036_854_775_807n;
const MAX_ITEMS = 64;
const MAX_OBJECTIVE_BYTES = 16 * 1_024;
const MAX_ITEM_BYTES = 4 * 1_024;
const MAX_SUCCESS_BYTES = 8 * 1_024;
const MAX_REVISION = 4_294_967_295;
const utf8 = new TextEncoder();

export type AgentGoalCriterionRequirementV1 = 'must' | 'should';

export interface AgentGoalCriterionInputV1 {
  criterionId: string | null;
  requirement: AgentGoalCriterionRequirementV1;
  statement: string;
}

export interface AgentGoalDraftInputV1 {
  acceptanceCriteria: AgentGoalCriterionInputV1[];
  constraints: string[];
  nonGoals: string[];
  objective: string;
  successVerification: string;
  userDecisions: string[];
}

export interface AgentGoalCriterionV1 {
  criterionId: string;
  requirement: AgentGoalCriterionRequirementV1;
  statement: string;
}

export interface AgentGoalContractV1 {
  acceptanceCriteria: AgentGoalCriterionV1[];
  constraints: string[];
  createdAtUnixMillis: string;
  nonGoals: string[];
  objective: string;
  previousRevision: number | null;
  revision: number;
  revisionReason: string | null;
  successVerification: string;
  taskId: string;
  userDecisions: string[];
}

export type AgentGoalResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { goal: AgentGoalContractV1; status: 'available' };

export interface AgentGoalResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: AgentGoalResultV1;
}

export interface AgentGoalMutationResponseV1 {
  goal: AgentGoalContractV1;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentGoal(
  taskId: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentGoalResponseV1> {
  if (!isStableId(taskId)) throw new Error('Agent Goal task identity does not match V1.');
  const request = { protocolVersion: CURRENT_PROTOCOL_VERSION, taskId };
  const response = parseAgentGoalResponseV1(await invokeCommand('query_agent_goal', { request }));
  if (response.result.status === 'available' && response.result.goal.taskId !== taskId) {
    throw new Error('Agent Goal response does not match its task selection.');
  }
  return response;
}

export async function createAgentGoal(
  draft: AgentGoalDraftInputV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentGoalMutationResponseV1> {
  const canonical = validateAgentGoalDraftInputV1(draft, 'create');
  const request = { draft: canonical, protocolVersion: CURRENT_PROTOCOL_VERSION };
  const response = parseAgentGoalMutationResponseV1(
    await invokeCommand('create_agent_goal', { request }),
  );
  if (
    response.goal.revision !== 1 ||
    response.goal.previousRevision !== null ||
    response.goal.revisionReason !== null
  ) {
    throw new Error('Created Agent Goal is not an initial immutable revision.');
  }
  return response;
}

export async function reviseAgentGoal(
  taskId: string,
  expectedRevision: number,
  revisionReason: string,
  draft: AgentGoalDraftInputV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentGoalMutationResponseV1> {
  if (!isStableId(taskId) || !isRevision(expectedRevision) || expectedRevision === MAX_REVISION) {
    throw new Error('Agent Goal revision selection does not match V1.');
  }
  const canonicalReason = parseText(revisionReason, MAX_ITEM_BYTES, 'revision reason');
  const canonical = validateAgentGoalDraftInputV1(draft, 'revise');
  const request = {
    draft: canonical,
    expectedRevision,
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    revisionReason: canonicalReason,
    taskId,
  };
  const response = parseAgentGoalMutationResponseV1(
    await invokeCommand('revise_agent_goal', { request }),
  );
  if (
    response.goal.taskId !== taskId ||
    response.goal.revision !== expectedRevision + 1 ||
    response.goal.previousRevision !== expectedRevision ||
    response.goal.revisionReason !== canonicalReason
  ) {
    throw new Error('Revised Agent Goal does not match its immutable predecessor.');
  }
  return response;
}

export function validateAgentGoalDraftInputV1(
  draft: AgentGoalDraftInputV1,
  mode: 'create' | 'revise',
): AgentGoalDraftInputV1 {
  if (!isRecord(draft) || !hasExactKeys(draft, draftKeys())) {
    throw new Error('Agent Goal draft does not match the V1 schema.');
  }
  const acceptanceCriteria = parseCriterionInputs(draft.acceptanceCriteria, mode);
  return {
    acceptanceCriteria,
    constraints: parseTextList(draft.constraints, 'constraints'),
    nonGoals: parseTextList(draft.nonGoals, 'non-goals'),
    objective: parseText(draft.objective, MAX_OBJECTIVE_BYTES, 'objective'),
    successVerification: parseText(
      draft.successVerification,
      MAX_SUCCESS_BYTES,
      'success verification',
    ),
    userDecisions: parseTextList(draft.userDecisions, 'user decisions'),
  };
}

export function parseAgentGoalResponseV1(payload: unknown): AgentGoalResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Agent Goal response does not match V1.');
  }
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: parseAgentGoalResult(payload.result),
  };
}

export function parseAgentGoalMutationResponseV1(payload: unknown): AgentGoalMutationResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['goal', 'protocolVersion']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Agent Goal mutation response does not match V1.');
  }
  return { goal: parseAgentGoalContract(payload.goal), protocolVersion: CURRENT_PROTOCOL_VERSION };
}

function parseAgentGoalResult(value: unknown): AgentGoalResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Agent Goal result does not match V1.');
  }
  if (value.status === 'noProject' || value.status === 'taskNotFound') {
    if (!hasExactKeys(value, ['status'])) throw new Error('Agent Goal absence result is invalid.');
    return { status: value.status };
  }
  if (value.status === 'available' && hasExactKeys(value, ['goal', 'status'])) {
    return { goal: parseAgentGoalContract(value.goal), status: 'available' };
  }
  throw new Error('Agent Goal result uses an unsupported state.');
}

function parseAgentGoalContract(value: unknown): AgentGoalContractV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'acceptanceCriteria',
      'constraints',
      'createdAtUnixMillis',
      'nonGoals',
      'objective',
      'previousRevision',
      'revision',
      'revisionReason',
      'successVerification',
      'taskId',
      'userDecisions',
    ]) ||
    typeof value.taskId !== 'string' ||
    !isStableId(value.taskId) ||
    !isRevision(value.revision)
  ) {
    throw new Error('Agent Goal contract does not match V1.');
  }
  const link = parseRevisionLink(value.revision, value.previousRevision, value.revisionReason);
  const createdAtUnixMillis = parseUnixMillis(value.createdAtUnixMillis);
  return {
    acceptanceCriteria: parseProjectedCriteria(value.acceptanceCriteria),
    constraints: parseTextList(value.constraints, 'constraints'),
    createdAtUnixMillis,
    nonGoals: parseTextList(value.nonGoals, 'non-goals'),
    objective: parseText(value.objective, MAX_OBJECTIVE_BYTES, 'objective'),
    previousRevision: link.previousRevision,
    revision: value.revision,
    revisionReason: link.revisionReason,
    successVerification: parseText(
      value.successVerification,
      MAX_SUCCESS_BYTES,
      'success verification',
    ),
    taskId: value.taskId,
    userDecisions: parseTextList(value.userDecisions, 'user decisions'),
  };
}

function parseCriterionInputs(
  value: unknown,
  mode: 'create' | 'revise',
): AgentGoalCriterionInputV1[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_ITEMS) {
    throw new Error('Agent Goal acceptance criteria are outside V1 bounds.');
  }
  const ids = new Set<string>();
  const statements = new Set<string>();
  return value.map((item) => {
    if (
      !isRecord(item) ||
      !hasExactKeys(item, ['criterionId', 'requirement', 'statement']) ||
      !isRequirement(item.requirement) ||
      (item.criterionId !== null &&
        (typeof item.criterionId !== 'string' || !isStableId(item.criterionId))) ||
      (mode === 'create' && item.criterionId !== null)
    ) {
      throw new Error('Agent Goal acceptance criterion does not match V1.');
    }
    const statement = parseText(item.statement, MAX_ITEM_BYTES, 'acceptance criterion');
    if (statements.has(statement) || (item.criterionId !== null && ids.has(item.criterionId))) {
      throw new Error('Agent Goal acceptance criteria contain duplicates.');
    }
    statements.add(statement);
    if (item.criterionId !== null) ids.add(item.criterionId);
    return {
      criterionId: item.criterionId,
      requirement: item.requirement,
      statement,
    };
  });
}

function parseProjectedCriteria(value: unknown): AgentGoalCriterionV1[] {
  const parsed = parseCriterionInputs(value, 'revise');
  if (parsed.some((criterion) => criterion.criterionId === null)) {
    throw new Error('Durable Agent Goal criterion has no identity.');
  }
  return parsed.map((criterion) => ({
    criterionId: criterion.criterionId as string,
    requirement: criterion.requirement,
    statement: criterion.statement,
  }));
}

function parseRevisionLink(
  revision: number,
  previousRevision: unknown,
  revisionReason: unknown,
): Pick<AgentGoalContractV1, 'previousRevision' | 'revisionReason'> {
  if (revision === 1) {
    if (previousRevision === null && revisionReason === null) {
      return { previousRevision: null, revisionReason: null };
    }
    throw new Error('Initial Agent Goal revision metadata is invalid.');
  }
  if (
    !isRevision(previousRevision) ||
    previousRevision !== revision - 1 ||
    typeof revisionReason !== 'string'
  ) {
    throw new Error('Agent Goal does not name its immediate predecessor.');
  }
  return {
    previousRevision,
    revisionReason: parseText(revisionReason, MAX_ITEM_BYTES, 'revision reason'),
  };
}

function parseTextList(value: unknown, field: string): string[] {
  if (!Array.isArray(value) || value.length > MAX_ITEMS) {
    throw new Error(`Agent Goal ${field} are outside V1 bounds.`);
  }
  const unique = new Set<string>();
  return value.map((item) => {
    const parsed = parseText(item, MAX_ITEM_BYTES, field);
    if (unique.has(parsed)) throw new Error(`Agent Goal ${field} contain duplicates.`);
    unique.add(parsed);
    return parsed;
  });
}

function parseText(value: unknown, maximumBytes: number, field: string): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > maximumBytes ||
    value !== value.replace(/\r\n?/gu, '\n').trim() ||
    utf8.encode(value).length > maximumBytes ||
    Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return (
        code !== undefined &&
        ((code <= 31 && code !== 9 && code !== 10) || (code >= 127 && code <= 159))
      );
    })
  ) {
    throw new Error(`Agent Goal ${field} is invalid.`);
  }
  return value;
}

function parseUnixMillis(value: unknown): string {
  if (
    typeof value !== 'string' ||
    !DECIMAL_MILLIS_PATTERN.test(value) ||
    BigInt(value) > MAX_PERSISTED_MILLIS
  ) {
    throw new Error('Agent Goal creation timestamp is invalid.');
  }
  return value;
}

function isRequirement(value: unknown): value is AgentGoalCriterionRequirementV1 {
  return value === 'must' || value === 'should';
}

function isStableId(value: string): boolean {
  return STABLE_ID_PATTERN.test(value);
}

function isRevision(value: unknown): value is number {
  return (
    typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= MAX_REVISION
  );
}

function draftKeys(): string[] {
  return [
    'acceptanceCriteria',
    'constraints',
    'nonGoals',
    'objective',
    'successVerification',
    'userDecisions',
  ];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const keys = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return (
    keys.length === sortedExpected.length &&
    keys.every((key, index) => key === sortedExpected[index])
  );
}

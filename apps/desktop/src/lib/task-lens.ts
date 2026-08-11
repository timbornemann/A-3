import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';
import type {
  ModuleDependencyEndpointV1,
  ModuleDependencyProviderV1,
  ModuleDependencyRelationV1,
  ModuleDependencyResolutionV1,
  ModuleDependencySourceRangeV1,
} from './module-dependency-graph';
import type {
  ProjectMapSearchEvidenceV1,
  ProjectMapSearchSymbolKindV1,
} from './project-map-search';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_TASKS = 20;
const MAX_STEPS = 256;
const MAX_ENTRIES = 64;
const MAX_CLAIMS = 128;
const MAX_CLAIM_EVIDENCE = 16;
const MAX_MANIFESTS = 16;
const MAX_PATH_BYTES = 131_072;
const MAX_TEXT_BYTES = 16_384;
const MAX_GOAL_BYTES = 8_192;
const MAX_SEED_BYTES = 4_096;
const MAX_FUSION_SCORE = 100_000;
const utf8 = new TextEncoder();

export interface QueryTaskLensTasksRequestV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface TaskLensTaskQueryV1 {
  taskId: string;
}

export interface QueryTaskLensTaskRequestV1 extends TaskLensTaskQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface TaskLensCompileQueryV1 extends TaskLensTaskQueryV1 {
  stepId: string;
}

export interface CompileTaskLensRequestV1 extends TaskLensCompileQueryV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface TaskLensTaskSummaryV1 {
  goalRevision: number;
  objective: string;
  taskId: string;
}

export type TaskLensTasksResultV1 =
  | { status: 'noProject' }
  | { status: 'available'; tasks: TaskLensTaskSummaryV1[]; truncated: boolean };

export interface TaskLensTasksResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: TaskLensTasksResultV1;
}

export type TaskLensStepStatusV1 =
  | 'pending'
  | 'ready'
  | 'inProgress'
  | 'blocked'
  | 'awaitingApproval'
  | 'verifying'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'stale';

export interface TaskLensStepV1 {
  intendedOutcome: string;
  status: TaskLensStepStatusV1;
  stepId: string;
}

export type TaskLensTaskResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable'; task: TaskLensTaskSummaryV1 }
  | {
      currentGoalRevision: number;
      ledgerGoalRevision: number;
      status: 'goalRevisionMismatch';
      taskId: string;
    }
  | {
      ledgerRevision: number;
      ledgerStoreVersion: string;
      status: 'available';
      steps: TaskLensStepV1[];
      task: TaskLensTaskSummaryV1;
    };

export interface TaskLensTaskResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: TaskLensTaskResultV1;
}

export type TaskLensPriorityV1 = 'exact' | 'evidence' | 'semantic';
export type TaskLensRetrievalChannelV1 =
  'exact' | 'lexical' | 'graph' | 'test' | 'memory' | 'semantic';

export interface TaskLensRetrievalSourceV1 {
  channel: TaskLensRetrievalChannelV1;
  normalizedScoreBasisPoints: number;
}

export type TaskLensEntryReasonV1 =
  | { kind: 'repositoryAnchor' }
  | {
      finalScore: number;
      kind: 'retrieval';
      priority: TaskLensPriorityV1;
      rank: number;
      sources: TaskLensRetrievalSourceV1[];
    }
  | { claimId: string; kind: 'claim' };

export interface TaskLensPathV1 {
  pathDisplay: string;
  pathHex: string;
}

export type TaskLensModuleKindV1 = 'manifestBoundary' | 'pathBoundary' | 'graphCommunity';

export type TaskLensEntryTargetV1 =
  | {
      entrypointCount: number;
      fileCount: number;
      kind: 'repository';
      languageCount: number;
      modulePolicyVersion: number;
      packageCount: number;
      symbolCount: number;
    }
  | {
      kind: 'module';
      manifests: ProjectMapSearchEvidenceV1[];
      manifestsTruncated: boolean;
      moduleId: string;
      moduleKind: TaskLensModuleKindV1;
      root: TaskLensPathV1 | null;
    }
  | { evidence: ProjectMapSearchEvidenceV1; kind: 'file' }
  | {
      evidence: ProjectMapSearchEvidenceV1;
      kind: 'symbol';
      name: string;
      signature: string | null;
      symbolId: string;
      symbolKind: ProjectMapSearchSymbolKindV1;
    }
  | { evidence: ProjectMapSearchEvidenceV1; kind: 'sourceSpan'; symbolId: string };

export interface TaskLensEntryV1 {
  estimatedTokens: number;
  position: number;
  reason: TaskLensEntryReasonV1;
  target: TaskLensEntryTargetV1;
}

export type TaskLensClaimKindV1 = 'fact' | 'observation' | 'hypothesis';
export type TaskLensClaimPolarityV1 = 'affirms' | 'denies';

export type TaskLensClaimPredicateV1 =
  | { kind: 'path'; path: TaskLensPathV1 }
  | { kind: 'symbol'; symbolId: string }
  | {
      kind: 'relation';
      relation: ModuleDependencyRelationV1;
      source: ModuleDependencyEndpointV1;
      target: ModuleDependencyEndpointV1;
    }
  | { kind: 'observed'; statement: string }
  | { kind: 'architecturalIntent'; statement: string };

export interface TaskLensGraphEdgeEvidenceV1 {
  confidenceBasisPoints: number;
  contentHash: string;
  evidenceId: string;
  pathHex: string;
  provider: ModuleDependencyProviderV1;
  range: ModuleDependencySourceRangeV1;
  resolution: ModuleDependencyResolutionV1;
  source: ModuleDependencyEndpointV1;
  target: ModuleDependencyEndpointV1;
}

export type TaskLensClaimEvidenceV1 =
  | { evidenceId: string; kind: 'file'; revision: ProjectMapSearchEvidenceV1 }
  | {
      evidenceId: string;
      kind: 'symbol';
      name: string;
      revision: ProjectMapSearchEvidenceV1;
      signature: string | null;
      symbolId: string;
      symbolKind: ProjectMapSearchSymbolKindV1;
    }
  | { edge: TaskLensGraphEdgeEvidenceV1; kind: 'graphEdge'; relation: ModuleDependencyRelationV1 };

export interface TaskLensClaimV1 {
  claimId: string;
  confidenceBasisPoints: number;
  evidence: TaskLensClaimEvidenceV1[];
  kind: TaskLensClaimKindV1;
  moduleId: string;
  polarity: TaskLensClaimPolarityV1;
  predicate: TaskLensClaimPredicateV1;
}

export interface TaskLensV1 {
  claims: TaskLensClaimV1[];
  digest: string;
  entries: TaskLensEntryV1[];
  estimatedTokens: number;
  excludedStaleClaims: number;
  fusionPolicyVersion: 1;
  goalRevision: number;
  goalSeed: string;
  indexRunId: string;
  ledgerRevision: number;
  ledgerStoreVersion: string;
  policyVersion: 1;
  snapshotId: string;
  stepId: string;
  stepSeed: string;
  taskId: string;
  tokenBudget: number;
  truncated: boolean;
}

export type TaskLensCompileResultV1 =
  | { status: 'noProject' }
  | { status: 'taskNotFound' }
  | { status: 'ledgerUnavailable' }
  | {
      currentGoalRevision: number;
      ledgerGoalRevision: number;
      status: 'goalRevisionMismatch';
      taskId: string;
    }
  | { status: 'stepUnavailable' }
  | { status: 'noPublishedIndex' }
  | { lens: TaskLensV1; status: 'available' };

export interface TaskLensCompileResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: TaskLensCompileResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryTaskLensTasks(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<TaskLensTasksResponseV1> {
  const request: QueryTaskLensTasksRequestV1 = { protocolVersion: CURRENT_PROTOCOL_VERSION };
  return parseTaskLensTasksResponseV1(await invokeCommand('query_task_lens_tasks', { request }));
}

export async function queryTaskLensTask(
  query: TaskLensTaskQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<TaskLensTaskResponseV1> {
  if (!isStableId(query.taskId)) throw new Error('Task Lens task identity does not match V1.');
  const request: QueryTaskLensTaskRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    taskId: query.taskId,
  };
  const response = parseTaskLensTaskResponseV1(
    await invokeCommand('query_task_lens_task', { request }),
  );
  assertTaskBinding(response.result, request.taskId);
  return response;
}

export async function compileTaskLens(
  query: TaskLensCompileQueryV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<TaskLensCompileResponseV1> {
  if (!isStableId(query.taskId) || !isStableId(query.stepId)) {
    throw new Error('Task Lens selection does not match V1.');
  }
  const request: CompileTaskLensRequestV1 = {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    stepId: query.stepId,
    taskId: query.taskId,
  };
  const response = parseTaskLensCompileResponseV1(
    await invokeCommand('compile_task_lens', { request }),
  );
  if (
    (response.result.status === 'available' &&
      (response.result.lens.taskId !== request.taskId ||
        response.result.lens.stepId !== request.stepId)) ||
    (response.result.status === 'goalRevisionMismatch' && response.result.taskId !== request.taskId)
  ) {
    throw new Error('Task Lens response does not match its durable selection.');
  }
  return response;
}

export function parseTaskLensTasksResponseV1(payload: unknown): TaskLensTasksResponseV1 {
  const result = parseEnvelope(payload, parseTasksResult);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result };
}

export function parseTaskLensTaskResponseV1(payload: unknown): TaskLensTaskResponseV1 {
  const result = parseEnvelope(payload, parseTaskResult);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result };
}

export function parseTaskLensCompileResponseV1(payload: unknown): TaskLensCompileResponseV1 {
  const result = parseEnvelope(payload, parseCompileResult);
  return { protocolVersion: CURRENT_PROTOCOL_VERSION, result };
}

function parseEnvelope<T>(payload: unknown, parse: (value: unknown) => T): T {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Task Lens response does not match V1.');
  }
  return parse(payload.result);
}

function parseTasksResult(value: unknown): TaskLensTasksResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  if (value.status === 'noProject' && hasExactKeys(value, ['status']))
    return { status: 'noProject' };
  if (
    value.status === 'available' &&
    hasExactKeys(value, ['status', 'tasks', 'truncated']) &&
    Array.isArray(value.tasks) &&
    value.tasks.length <= MAX_TASKS &&
    typeof value.truncated === 'boolean'
  ) {
    const tasks = value.tasks.map(parseTaskSummary);
    if (tasks.some((task, index) => index > 0 && tasks[index - 1].taskId >= task.taskId)) {
      throw new Error('Task Lens task list violates stable identity ordering.');
    }
    return { status: 'available', tasks, truncated: value.truncated };
  }
  return invalidResult();
}

function parseTaskResult(value: unknown): TaskLensTaskResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  if (
    (value.status === 'noProject' || value.status === 'taskNotFound') &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status };
  }
  if (value.status === 'ledgerUnavailable' && hasExactKeys(value, ['status', 'task'])) {
    return { status: 'ledgerUnavailable', task: parseTaskSummary(value.task) };
  }
  if (value.status === 'goalRevisionMismatch') return parseGoalMismatch(value);
  if (
    value.status === 'available' &&
    hasExactKeys(value, ['ledgerRevision', 'ledgerStoreVersion', 'status', 'steps', 'task']) &&
    isPositiveU32(value.ledgerRevision) &&
    isPositiveIntegerString(value.ledgerStoreVersion) &&
    Array.isArray(value.steps) &&
    value.steps.length > 0 &&
    value.steps.length <= MAX_STEPS
  ) {
    const task = parseTaskSummary(value.task);
    const steps = value.steps.map(parseStep);
    if (
      new Set(steps.map((step) => step.stepId)).size !== steps.length ||
      steps.some((step, index) => index > 0 && steps[index - 1].stepId >= step.stepId)
    ) {
      throw new Error('Task Lens steps violate stable identity ordering.');
    }
    return {
      ledgerRevision: value.ledgerRevision,
      ledgerStoreVersion: value.ledgerStoreVersion,
      status: 'available',
      steps,
      task,
    };
  }
  return invalidResult();
}

function parseCompileResult(value: unknown): TaskLensCompileResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') return invalidResult();
  if (
    [
      'noProject',
      'taskNotFound',
      'ledgerUnavailable',
      'stepUnavailable',
      'noPublishedIndex',
    ].includes(value.status) &&
    hasExactKeys(value, ['status'])
  ) {
    return { status: value.status } as TaskLensCompileResultV1;
  }
  if (value.status === 'goalRevisionMismatch') return parseGoalMismatch(value);
  if (value.status === 'available' && hasExactKeys(value, ['lens', 'status'])) {
    return { lens: parseLens(value.lens), status: 'available' };
  }
  return invalidResult();
}

function parseGoalMismatch(value: Record<string, unknown>) {
  if (
    !hasExactKeys(value, ['currentGoalRevision', 'ledgerGoalRevision', 'status', 'taskId']) ||
    value.status !== 'goalRevisionMismatch' ||
    !isStableId(value.taskId) ||
    !isPositiveU32(value.currentGoalRevision) ||
    !isPositiveU32(value.ledgerGoalRevision) ||
    value.currentGoalRevision === value.ledgerGoalRevision
  ) {
    return invalidResult();
  }
  return {
    currentGoalRevision: value.currentGoalRevision,
    ledgerGoalRevision: value.ledgerGoalRevision,
    status: 'goalRevisionMismatch' as const,
    taskId: value.taskId,
  };
}

function parseTaskSummary(value: unknown): TaskLensTaskSummaryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['goalRevision', 'objective', 'taskId']) ||
    !isPositiveU32(value.goalRevision) ||
    !isStableId(value.taskId) ||
    !isBoundedText(value.objective, MAX_GOAL_BYTES, true)
  ) {
    throw new Error('Task Lens response contains an invalid task summary.');
  }
  return { goalRevision: value.goalRevision, objective: value.objective, taskId: value.taskId };
}

function parseStep(value: unknown): TaskLensStepV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['intendedOutcome', 'status', 'stepId']) ||
    !isStableId(value.stepId) ||
    !isStepStatus(value.status) ||
    !isBoundedText(value.intendedOutcome, MAX_GOAL_BYTES, true)
  ) {
    throw new Error('Task Lens response contains an invalid active-plan step.');
  }
  return { intendedOutcome: value.intendedOutcome, status: value.status, stepId: value.stepId };
}

function parseLens(value: unknown): TaskLensV1 {
  const keys = [
    'claims',
    'digest',
    'entries',
    'estimatedTokens',
    'excludedStaleClaims',
    'fusionPolicyVersion',
    'goalRevision',
    'goalSeed',
    'indexRunId',
    'ledgerRevision',
    'ledgerStoreVersion',
    'policyVersion',
    'snapshotId',
    'stepId',
    'stepSeed',
    'taskId',
    'tokenBudget',
    'truncated',
  ];
  if (
    !isRecord(value) ||
    !hasExactKeys(value, keys) ||
    value.policyVersion !== 1 ||
    value.fusionPolicyVersion !== 1 ||
    !isStableId(value.taskId) ||
    !isStableId(value.stepId) ||
    !isStableId(value.indexRunId) ||
    !isStableId(value.snapshotId) ||
    !isStableId(value.digest) ||
    !isPositiveU32(value.goalRevision) ||
    !isPositiveU32(value.ledgerRevision) ||
    !isPositiveIntegerString(value.ledgerStoreVersion) ||
    !isIntegerBetween(value.tokenBudget, 256, 32_768) ||
    !isIntegerBetween(value.estimatedTokens, 1, value.tokenBudget as number) ||
    !isIntegerBetween(value.excludedStaleClaims, 0, 65_535) ||
    !isBoundedText(value.goalSeed, MAX_SEED_BYTES, true) ||
    !isBoundedText(value.stepSeed, MAX_SEED_BYTES, true) ||
    !Array.isArray(value.entries) ||
    value.entries.length === 0 ||
    value.entries.length > MAX_ENTRIES ||
    !Array.isArray(value.claims) ||
    value.claims.length > MAX_CLAIMS ||
    typeof value.truncated !== 'boolean'
  ) {
    throw new Error('Task Lens response contains an invalid Lens projection.');
  }
  const entries = value.entries.map(parseEntry);
  const claims = value.claims.map(parseClaim);
  const entryIds = entries.map((entry) => targetIdentity(entry.target));
  const claimIds = claims.map((claim) => claim.claimId);
  if (
    entries[0].target.kind !== 'repository' ||
    entries[0].reason.kind !== 'repositoryAnchor' ||
    entries.some((entry, index) => entry.position !== index + 1) ||
    entries.reduce((sum, entry) => sum + entry.estimatedTokens, 0) !== value.estimatedTokens ||
    new Set(entryIds).size !== entryIds.length ||
    new Set(claimIds).size !== claims.length ||
    claims.some((claim, index) => index > 0 && claims[index - 1].claimId >= claim.claimId)
  ) {
    throw new Error('Task Lens response violates deterministic entry or claim invariants.');
  }
  return {
    claims,
    digest: value.digest,
    entries,
    estimatedTokens: value.estimatedTokens,
    excludedStaleClaims: value.excludedStaleClaims,
    fusionPolicyVersion: 1,
    goalRevision: value.goalRevision,
    goalSeed: value.goalSeed,
    indexRunId: value.indexRunId,
    ledgerRevision: value.ledgerRevision,
    ledgerStoreVersion: value.ledgerStoreVersion,
    policyVersion: 1,
    snapshotId: value.snapshotId,
    stepId: value.stepId,
    stepSeed: value.stepSeed,
    taskId: value.taskId,
    tokenBudget: value.tokenBudget,
    truncated: value.truncated,
  };
}

function parseEntry(value: unknown): TaskLensEntryV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['estimatedTokens', 'position', 'reason', 'target']) ||
    !isIntegerBetween(value.position, 1, MAX_ENTRIES) ||
    !isIntegerBetween(value.estimatedTokens, 1, 65_535)
  ) {
    throw new Error('Task Lens response contains an invalid entry.');
  }
  return {
    estimatedTokens: value.estimatedTokens,
    position: value.position,
    reason: parseEntryReason(value.reason),
    target: parseEntryTarget(value.target),
  };
}

function parseEntryReason(value: unknown): TaskLensEntryReasonV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalidEntryReason();
  if (value.kind === 'repositoryAnchor' && hasExactKeys(value, ['kind']))
    return { kind: value.kind };
  if (
    value.kind === 'claim' &&
    hasExactKeys(value, ['claimId', 'kind']) &&
    isStableId(value.claimId)
  ) {
    return { claimId: value.claimId, kind: 'claim' };
  }
  if (
    value.kind === 'retrieval' &&
    hasExactKeys(value, ['finalScore', 'kind', 'priority', 'rank', 'sources']) &&
    isIntegerBetween(value.rank, 1, 32) &&
    isIntegerBetween(value.finalScore, 0, MAX_FUSION_SCORE) &&
    isPriority(value.priority) &&
    Array.isArray(value.sources) &&
    value.sources.length > 0 &&
    value.sources.length <= 6
  ) {
    const sources = value.sources.map(parseRetrievalSource);
    const channels = sources.map((source) => source.channel);
    const expectedPriority = channels.includes('exact')
      ? 'exact'
      : channels.some((channel) => channel !== 'semantic')
        ? 'evidence'
        : 'semantic';
    if (
      new Set(channels).size !== channels.length ||
      channels.some(
        (channel, index) => index > 0 && channelRank(channels[index - 1]) >= channelRank(channel),
      ) ||
      value.priority !== expectedPriority
    ) {
      return invalidEntryReason();
    }
    return {
      finalScore: value.finalScore,
      kind: 'retrieval',
      priority: value.priority,
      rank: value.rank,
      sources,
    };
  }
  return invalidEntryReason();
}

function invalidEntryReason(): never {
  throw new Error('Task Lens response contains invalid retrieval provenance.');
}

function parseRetrievalSource(value: unknown): TaskLensRetrievalSourceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['channel', 'normalizedScoreBasisPoints']) ||
    !isChannel(value.channel) ||
    !isIntegerBetween(value.normalizedScoreBasisPoints, 0, 10_000)
  ) {
    throw new Error('Task Lens response contains an invalid retrieval source.');
  }
  return {
    channel: value.channel,
    normalizedScoreBasisPoints: value.normalizedScoreBasisPoints,
  };
}

function parseEntryTarget(value: unknown): TaskLensEntryTargetV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalidTarget();
  if (
    value.kind === 'repository' &&
    hasExactKeys(value, [
      'entrypointCount',
      'fileCount',
      'kind',
      'languageCount',
      'modulePolicyVersion',
      'packageCount',
      'symbolCount',
    ]) &&
    isPositiveU32(value.modulePolicyVersion) &&
    [
      value.entrypointCount,
      value.fileCount,
      value.languageCount,
      value.packageCount,
      value.symbolCount,
    ].every((item) => isIntegerBetween(item, 0, 4_294_967_295))
  ) {
    return value as TaskLensEntryTargetV1;
  }
  if (
    value.kind === 'module' &&
    hasExactKeys(value, [
      'kind',
      'manifests',
      'manifestsTruncated',
      'moduleId',
      'moduleKind',
      'root',
    ]) &&
    isStableId(value.moduleId) &&
    isModuleKind(value.moduleKind) &&
    (value.root === null || isRecord(value.root)) &&
    Array.isArray(value.manifests) &&
    value.manifests.length <= MAX_MANIFESTS &&
    typeof value.manifestsTruncated === 'boolean'
  ) {
    return {
      kind: 'module',
      manifests: value.manifests.map((item) => parseEvidence(item, false)),
      manifestsTruncated: value.manifestsTruncated,
      moduleId: value.moduleId,
      moduleKind: value.moduleKind,
      root: value.root === null ? null : parsePath(value.root),
    };
  }
  if (value.kind === 'file' && hasExactKeys(value, ['evidence', 'kind'])) {
    return { evidence: parseEvidence(value.evidence, false), kind: 'file' };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, ['evidence', 'kind', 'name', 'signature', 'symbolId', 'symbolKind']) &&
    isStableId(value.symbolId) &&
    isSymbolKind(value.symbolKind) &&
    isBoundedText(value.name, MAX_TEXT_BYTES, false) &&
    (value.signature === null || isBoundedText(value.signature, MAX_TEXT_BYTES, true))
  ) {
    return {
      evidence: parseEvidence(value.evidence, true),
      kind: 'symbol',
      name: value.name,
      signature: value.signature,
      symbolId: value.symbolId,
      symbolKind: value.symbolKind,
    };
  }
  if (
    value.kind === 'sourceSpan' &&
    hasExactKeys(value, ['evidence', 'kind', 'symbolId']) &&
    isStableId(value.symbolId)
  ) {
    return {
      evidence: parseEvidence(value.evidence, true),
      kind: 'sourceSpan',
      symbolId: value.symbolId,
    };
  }
  return invalidTarget();
}

function invalidTarget(): never {
  throw new Error('Task Lens response contains an invalid evidence target.');
}

function parseClaim(value: unknown): TaskLensClaimV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'claimId',
      'confidenceBasisPoints',
      'evidence',
      'kind',
      'moduleId',
      'polarity',
      'predicate',
    ]) ||
    !isStableId(value.claimId) ||
    !isStableId(value.moduleId) ||
    !isClaimKind(value.kind) ||
    (value.polarity !== 'affirms' && value.polarity !== 'denies') ||
    !isIntegerBetween(value.confidenceBasisPoints, 0, 10_000) ||
    !Array.isArray(value.evidence) ||
    value.evidence.length > MAX_CLAIM_EVIDENCE
  ) {
    throw new Error('Task Lens response contains an invalid claim.');
  }
  const predicate = parseClaimPredicate(value.predicate);
  const evidence = value.evidence.map(parseClaimEvidence);
  const evidenceIds = evidence.map(claimEvidenceIdentity);
  const expectedKind =
    predicate.kind === 'observed'
      ? 'observation'
      : predicate.kind === 'architecturalIntent' || value.polarity === 'denies'
        ? 'hypothesis'
        : 'fact';
  if (
    value.kind !== expectedKind ||
    (predicate.kind !== 'architecturalIntent' && evidence.length === 0) ||
    new Set(evidenceIds).size !== evidence.length ||
    evidenceIds.some((id, index) => index > 0 && evidenceIds[index - 1] >= id)
  ) {
    throw new Error('Task Lens claim classification or Evidence is inconsistent.');
  }
  return {
    claimId: value.claimId,
    confidenceBasisPoints: value.confidenceBasisPoints,
    evidence,
    kind: value.kind,
    moduleId: value.moduleId,
    polarity: value.polarity,
    predicate,
  };
}

function parseClaimPredicate(value: unknown): TaskLensClaimPredicateV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalidPredicate();
  if (value.kind === 'path' && hasExactKeys(value, ['kind', 'path'])) {
    return { kind: 'path', path: parsePath(value.path) };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, ['kind', 'symbolId']) &&
    isStableId(value.symbolId)
  ) {
    return { kind: 'symbol', symbolId: value.symbolId };
  }
  if (
    value.kind === 'relation' &&
    hasExactKeys(value, ['kind', 'relation', 'source', 'target']) &&
    isRelation(value.relation)
  ) {
    return {
      kind: 'relation',
      relation: value.relation,
      source: parseEndpoint(value.source),
      target: parseEndpoint(value.target),
    };
  }
  if (
    (value.kind === 'observed' || value.kind === 'architecturalIntent') &&
    hasExactKeys(value, ['kind', 'statement']) &&
    isBoundedText(value.statement, 2_048, true)
  ) {
    return { kind: value.kind, statement: value.statement };
  }
  return invalidPredicate();
}

function invalidPredicate(): never {
  throw new Error('Task Lens response contains an invalid claim predicate.');
}

function parseClaimEvidence(value: unknown): TaskLensClaimEvidenceV1 {
  if (!isRecord(value) || typeof value.kind !== 'string') return invalidClaimEvidence();
  if (
    value.kind === 'file' &&
    hasExactKeys(value, ['evidenceId', 'kind', 'revision']) &&
    isStableId(value.evidenceId)
  ) {
    return {
      evidenceId: value.evidenceId,
      kind: 'file',
      revision: parseEvidence(value.revision, false),
    };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, [
      'evidenceId',
      'kind',
      'name',
      'revision',
      'signature',
      'symbolId',
      'symbolKind',
    ]) &&
    isStableId(value.evidenceId) &&
    isStableId(value.symbolId) &&
    isSymbolKind(value.symbolKind) &&
    isBoundedText(value.name, MAX_TEXT_BYTES, false) &&
    (value.signature === null || isBoundedText(value.signature, MAX_TEXT_BYTES, true))
  ) {
    return {
      evidenceId: value.evidenceId,
      kind: 'symbol',
      name: value.name,
      revision: parseEvidence(value.revision, true),
      signature: value.signature,
      symbolId: value.symbolId,
      symbolKind: value.symbolKind,
    };
  }
  if (
    value.kind === 'graphEdge' &&
    hasExactKeys(value, ['edge', 'kind', 'relation']) &&
    isRelation(value.relation)
  ) {
    return { edge: parseGraphEdge(value.edge), kind: 'graphEdge', relation: value.relation };
  }
  return invalidClaimEvidence();
}

function invalidClaimEvidence(): never {
  throw new Error('Task Lens response contains invalid claim Evidence.');
}

function parseGraphEdge(value: unknown): TaskLensGraphEdgeEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'confidenceBasisPoints',
      'evidenceId',
      'pathHex',
      'provider',
      'range',
      'resolution',
      'source',
      'target',
    ]) ||
    !isStableId(value.evidenceId) ||
    !isStableId(value.contentHash) ||
    !isPathHex(value.pathHex) ||
    !isProvider(value.provider) ||
    !isResolution(value.resolution) ||
    !isIntegerBetween(value.confidenceBasisPoints, 0, 10_000)
  ) {
    throw new Error('Task Lens response contains invalid graph Evidence.');
  }
  return {
    confidenceBasisPoints: value.confidenceBasisPoints,
    contentHash: value.contentHash,
    evidenceId: value.evidenceId,
    pathHex: value.pathHex,
    provider: value.provider,
    range: parseSourceRange(value.range),
    resolution: value.resolution,
    source: parseEndpoint(value.source),
    target: parseEndpoint(value.target),
  };
}

function parseEndpoint(value: unknown): ModuleDependencyEndpointV1 {
  if (!isRecord(value) || typeof value.kind !== 'string')
    throw new Error('Invalid graph endpoint.');
  if (
    value.kind === 'file' &&
    hasExactKeys(value, ['kind', 'pathHex']) &&
    isPathHex(value.pathHex)
  ) {
    return { kind: 'file', pathHex: value.pathHex };
  }
  if (
    value.kind === 'symbol' &&
    hasExactKeys(value, ['kind', 'symbolId']) &&
    isStableId(value.symbolId)
  ) {
    return { kind: 'symbol', symbolId: value.symbolId };
  }
  throw new Error('Task Lens response contains an invalid graph endpoint.');
}

function parseEvidence(value: unknown, requireRange: boolean): ProjectMapSearchEvidenceV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['contentHash', 'declarationRange', 'pathDisplay', 'pathHex']) ||
    !isStableId(value.contentHash) ||
    !isPathHex(value.pathHex) ||
    typeof value.pathDisplay !== 'string' ||
    value.pathDisplay.length === 0 ||
    containsUnsupportedControl(value.pathDisplay)
  ) {
    throw new Error('Task Lens response contains invalid revision Evidence.');
  }
  const declarationRange =
    value.declarationRange === null ? null : parseSourceRange(value.declarationRange);
  if ((requireRange && declarationRange === null) || (!requireRange && declarationRange !== null)) {
    throw new Error('Task Lens revision Evidence has an inconsistent declaration range.');
  }
  return {
    contentHash: value.contentHash,
    declarationRange,
    pathDisplay: value.pathDisplay,
    pathHex: value.pathHex,
  };
}

function parsePath(value: unknown): TaskLensPathV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['pathDisplay', 'pathHex']) ||
    !isPathHex(value.pathHex) ||
    typeof value.pathDisplay !== 'string' ||
    value.pathDisplay.length === 0 ||
    containsUnsupportedControl(value.pathDisplay)
  ) {
    throw new Error('Task Lens response contains an invalid path projection.');
  }
  return { pathDisplay: value.pathDisplay, pathHex: value.pathHex };
}

function parseSourceRange(value: unknown): ModuleDependencySourceRangeV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['end', 'endByte', 'start', 'startByte']) ||
    !isIntegerBetween(value.startByte, 0, 4_294_967_295) ||
    !isIntegerBetween(value.endByte, 1, 4_294_967_295) ||
    value.startByte >= value.endByte
  ) {
    throw new Error('Task Lens response contains an invalid source range.');
  }
  const start = parsePosition(value.start);
  const end = parsePosition(value.end);
  if (end.row < start.row || (end.row === start.row && end.column < start.column)) {
    throw new Error('Task Lens response contains a reversed source range.');
  }
  return { end, endByte: value.endByte, start, startByte: value.startByte };
}

function parsePosition(value: unknown) {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['column', 'row']) ||
    !isIntegerBetween(value.column, 0, 4_294_967_295) ||
    !isIntegerBetween(value.row, 0, 4_294_967_295)
  ) {
    throw new Error('Task Lens response contains an invalid source position.');
  }
  return { column: value.column, row: value.row };
}

function assertTaskBinding(result: TaskLensTaskResultV1, taskId: string): void {
  if (
    ((result.status === 'available' || result.status === 'ledgerUnavailable') &&
      result.task.taskId !== taskId) ||
    (result.status === 'goalRevisionMismatch' && result.taskId !== taskId)
  ) {
    throw new Error('Task Lens task response does not match its durable selection.');
  }
}

function targetIdentity(target: TaskLensEntryTargetV1): string {
  switch (target.kind) {
    case 'repository':
      return '0';
    case 'module':
      return `1:${target.moduleId}`;
    case 'symbol':
      return `2:${target.symbolId}`;
    case 'file':
      return `3:${target.evidence.pathHex}`;
    case 'sourceSpan':
      return `4:${target.symbolId}:${target.evidence.pathHex}`;
  }
}

function claimEvidenceIdentity(value: TaskLensClaimEvidenceV1): string {
  return value.kind === 'graphEdge' ? value.edge.evidenceId : value.evidenceId;
}

function channelRank(value: TaskLensRetrievalChannelV1): number {
  return ['exact', 'lexical', 'graph', 'test', 'memory', 'semantic'].indexOf(value);
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && STABLE_ID_PATTERN.test(value);
}

function isPathHex(value: unknown): value is string {
  return typeof value === 'string' && HEX_PATTERN.test(value) && value.length <= MAX_PATH_BYTES * 2;
}

function isPositiveU32(value: unknown): value is number {
  return isIntegerBetween(value, 1, 4_294_967_295);
}

function isPositiveIntegerString(value: unknown): value is string {
  return typeof value === 'string' && /^[1-9][0-9]*$/.test(value);
}

function isIntegerBetween(value: unknown, minimum: number, maximum: number): value is number {
  return Number.isInteger(value) && (value as number) >= minimum && (value as number) <= maximum;
}

function isStepStatus(value: unknown): value is TaskLensStepStatusV1 {
  return [
    'pending',
    'ready',
    'inProgress',
    'blocked',
    'awaitingApproval',
    'verifying',
    'completed',
    'failed',
    'cancelled',
    'stale',
  ].includes(value as TaskLensStepStatusV1);
}

function isPriority(value: unknown): value is TaskLensPriorityV1 {
  return ['exact', 'evidence', 'semantic'].includes(value as TaskLensPriorityV1);
}

function isChannel(value: unknown): value is TaskLensRetrievalChannelV1 {
  return ['exact', 'lexical', 'graph', 'test', 'memory', 'semantic'].includes(
    value as TaskLensRetrievalChannelV1,
  );
}

function isModuleKind(value: unknown): value is TaskLensModuleKindV1 {
  return ['manifestBoundary', 'pathBoundary', 'graphCommunity'].includes(
    value as TaskLensModuleKindV1,
  );
}

function isClaimKind(value: unknown): value is TaskLensClaimKindV1 {
  return ['fact', 'observation', 'hypothesis'].includes(value as TaskLensClaimKindV1);
}

function isSymbolKind(value: unknown): value is ProjectMapSearchSymbolKindV1 {
  return [
    'module',
    'namespace',
    'function',
    'method',
    'struct',
    'enum',
    'trait',
    'interface',
    'class',
    'implementation',
    'typeAlias',
    'constant',
    'static',
    'variable',
    'field',
    'variant',
    'parameter',
  ].includes(value as ProjectMapSearchSymbolKindV1);
}

function isRelation(value: unknown): value is ModuleDependencyRelationV1 {
  return [
    'imports',
    'exports',
    'calls',
    'implements',
    'extends',
    'reads',
    'writes',
    'configures',
    'tests',
    'builds',
    'documents',
  ].includes(value as ModuleDependencyRelationV1);
}

function isProvider(value: unknown): value is ModuleDependencyProviderV1 {
  return ['treeSitter', 'manifest', 'languageHeuristic'].includes(
    value as ModuleDependencyProviderV1,
  );
}

function isResolution(value: unknown): value is ModuleDependencyResolutionV1 {
  return [
    'adapterLocalSymbol',
    'adapterFile',
    'exactModuleReference',
    'uniqueFileLocalName',
    'uniqueQualifiedName',
  ].includes(value as ModuleDependencyResolutionV1);
}

function isBoundedText(value: unknown, maxBytes: number, allowLayout: boolean): value is string {
  return (
    typeof value === 'string' &&
    value.trim().length > 0 &&
    utf8.encode(value).length <= maxBytes &&
    !Array.from(value).some((character) => {
      const code = character.codePointAt(0);
      return (
        code === undefined ||
        code === 0 ||
        (code < 32 && (!allowLayout || (character !== '\n' && character !== '\t'))) ||
        code === 127
      );
    })
  );
}

function containsUnsupportedControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.codePointAt(0);
    return code === undefined || code === 0 || code < 32 || code === 127;
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  const sorted = [...expected].sort();
  return actual.length === sorted.length && actual.every((key, index) => key === sorted[index]);
}

function invalidResult(): never {
  throw new Error('Task Lens response contains an invalid result.');
}

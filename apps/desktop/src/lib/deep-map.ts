import { invoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const PROFILE_ID = /^[0-9a-f]{64}$/;
const PROVIDER_ID = /^[A-Za-z0-9._-]{1,128}$/;
const MODEL_ID = /^[A-Za-z0-9._/@:-]{1,512}$/;
const U64_DECIMAL = /^(0|[1-9][0-9]{0,19})$/;
const U64_MAX = BigInt('18446744073709551615');

export interface DeepMapBudgetV1 {
  tokenLimit: number;
  timeLimitMillis: number;
  toolCallLimit: number;
}

export interface DeepMapModelV1 {
  profileId: string;
  profileVersion: number;
  providerId: string;
  modelId: string;
  contextTokens: number;
  outputTokens: number;
}

export type DeepMapActivityStateV1 =
  | 'idle'
  | 'queued'
  | 'running'
  | 'pausing'
  | 'paused'
  | 'cancelling'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface DeepMapProgressV1 {
  completed: string;
  total: string;
}

export interface DeepMapActivityV1 {
  state: DeepMapActivityStateV1;
  budget: DeepMapBudgetV1 | null;
  progress: DeepMapProgressV1 | null;
  confirmedSteps: string;
  totalSteps: string;
}

export interface DeepMapConfigurationV1 {
  model: DeepMapModelV1;
  minimumBudget: DeepMapBudgetV1;
  defaultBudget: DeepMapBudgetV1;
  maximumBudget: DeepMapBudgetV1;
}

export type DeepMapStatusResponseV1 = {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result:
    | { status: 'noProject' }
    | { status: 'unavailable' }
    | {
        status: 'available';
        configuration: DeepMapConfigurationV1;
        activity: DeepMapActivityV1;
      };
};

export interface DeepMapControlResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  accepted: true;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  invoke<unknown>(command, arguments_);

export async function queryDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapStatusResponseV1> {
  const value = await invokeCommand('query_deep_map', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseDeepMapStatusResponseV1(value);
}

export async function startDeepMap(
  budget: DeepMapBudgetV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  const validated = parseBudget(budget);
  const value = await invokeCommand('start_deep_map', {
    request: { budget: validated, protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseControlResponse(value);
}

export async function pauseDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('pause_deep_map', invokeCommand);
}

export async function resumeDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('resume_deep_map', invokeCommand);
}

export async function cancelDeepMap(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<DeepMapControlResponseV1> {
  return control('cancel_deep_map', invokeCommand);
}

async function control(
  command: string,
  invokeCommand: InvokeCommand,
): Promise<DeepMapControlResponseV1> {
  const value = await invokeCommand(command, {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseControlResponse(value);
}

export function parseDeepMapStatusResponseV1(value: unknown): DeepMapStatusResponseV1 {
  const root = record(value, ['protocolVersion', 'result']);
  if (root.protocolVersion !== CURRENT_PROTOCOL_VERSION) {
    throw new Error('Unsupported Deep Map protocol version');
  }
  const result = record(root.result);
  if (result.status === 'noProject' || result.status === 'unavailable') {
    exactKeys(result, ['status']);
    return {
      protocolVersion: CURRENT_PROTOCOL_VERSION,
      result: { status: result.status },
    };
  }
  if (result.status !== 'available') {
    throw new Error('Invalid Deep Map availability state');
  }
  exactKeys(result, ['activity', 'configuration', 'status']);
  return {
    protocolVersion: CURRENT_PROTOCOL_VERSION,
    result: {
      status: 'available',
      configuration: parseConfiguration(result.configuration),
      activity: parseActivity(result.activity),
    },
  };
}

function parseConfiguration(value: unknown): DeepMapConfigurationV1 {
  const object = record(value, ['defaultBudget', 'maximumBudget', 'minimumBudget', 'model']);
  const minimumBudget = parseBudget(object.minimumBudget);
  const defaultBudget = parseBudget(object.defaultBudget);
  const maximumBudget = parseBudget(object.maximumBudget);
  if (
    minimumBudget.tokenLimit !== 1 ||
    minimumBudget.timeLimitMillis !== 1 ||
    minimumBudget.toolCallLimit !== 1 ||
    maximumBudget.tokenLimit !== 1_000_000 ||
    maximumBudget.timeLimitMillis !== 86_400_000 ||
    maximumBudget.toolCallLimit !== 4_096 ||
    !budgetContains(maximumBudget, defaultBudget)
  ) {
    throw new Error('Invalid Deep Map budget envelope');
  }
  return {
    model: parseModel(object.model),
    minimumBudget,
    defaultBudget,
    maximumBudget,
  };
}

function parseModel(value: unknown): DeepMapModelV1 {
  const object = record(value, [
    'contextTokens',
    'modelId',
    'outputTokens',
    'profileId',
    'profileVersion',
    'providerId',
  ]);
  if (
    typeof object.profileId !== 'string' ||
    !PROFILE_ID.test(object.profileId) ||
    !integer(object.profileVersion, 1, 65_535) ||
    typeof object.providerId !== 'string' ||
    !PROVIDER_ID.test(object.providerId) ||
    typeof object.modelId !== 'string' ||
    !MODEL_ID.test(object.modelId) ||
    !integer(object.contextTokens, 1_024, 1_048_576) ||
    !integer(object.outputTokens, 1, 262_144) ||
    object.outputTokens > object.contextTokens
  ) {
    throw new Error('Invalid verified Deep Map model');
  }
  return object as unknown as DeepMapModelV1;
}

function parseActivity(value: unknown): DeepMapActivityV1 {
  const object = record(value, ['budget', 'confirmedSteps', 'progress', 'state', 'totalSteps']);
  const states = new Set<DeepMapActivityStateV1>([
    'idle',
    'queued',
    'running',
    'pausing',
    'paused',
    'cancelling',
    'succeeded',
    'failed',
    'cancelled',
  ]);
  if (typeof object.state !== 'string' || !states.has(object.state as DeepMapActivityStateV1)) {
    throw new Error('Invalid Deep Map lifecycle state');
  }
  const state = object.state as DeepMapActivityStateV1;
  const budget = object.budget === null ? null : parseBudget(object.budget);
  const progress = object.progress === null ? null : parseProgress(object.progress);
  const confirmedSteps = decimal(object.confirmedSteps);
  const totalSteps = decimal(object.totalSteps);
  if (
    confirmedSteps > totalSteps ||
    (state === 'idle' && (budget !== null || progress !== null || totalSteps !== BigInt(0))) ||
    (['queued', 'running', 'pausing', 'paused', 'cancelling'].includes(state) && budget === null) ||
    (state === 'paused' && (totalSteps === BigInt(0) || confirmedSteps >= totalSteps)) ||
    (state === 'succeeded' && confirmedSteps !== totalSteps) ||
    (progress !== null && !['queued', 'running', 'pausing', 'cancelling'].includes(state))
  ) {
    throw new Error('Contradictory Deep Map lifecycle state');
  }
  return {
    state,
    budget,
    progress,
    confirmedSteps: object.confirmedSteps as string,
    totalSteps: object.totalSteps as string,
  };
}

function parseProgress(value: unknown): DeepMapProgressV1 {
  const object = record(value, ['completed', 'total']);
  const completed = decimal(object.completed);
  const total = decimal(object.total);
  if (total === BigInt(0) || completed > total) {
    throw new Error('Invalid Deep Map progress');
  }
  return { completed: object.completed as string, total: object.total as string };
}

function parseBudget(value: unknown): DeepMapBudgetV1 {
  const object = record(value, ['timeLimitMillis', 'tokenLimit', 'toolCallLimit']);
  if (
    !integer(object.tokenLimit, 1, 1_000_000) ||
    !integer(object.timeLimitMillis, 1, 86_400_000) ||
    !integer(object.toolCallLimit, 1, 4_096)
  ) {
    throw new Error('Invalid Deep Map budget');
  }
  return object as unknown as DeepMapBudgetV1;
}

function parseControlResponse(value: unknown): DeepMapControlResponseV1 {
  const object = record(value, ['accepted', 'protocolVersion']);
  if (object.protocolVersion !== CURRENT_PROTOCOL_VERSION || object.accepted !== true) {
    throw new Error('Invalid Deep Map control response');
  }
  return { accepted: true, protocolVersion: CURRENT_PROTOCOL_VERSION };
}

function budgetContains(outer: DeepMapBudgetV1, inner: DeepMapBudgetV1): boolean {
  return (
    inner.tokenLimit <= outer.tokenLimit &&
    inner.timeLimitMillis <= outer.timeLimitMillis &&
    inner.toolCallLimit <= outer.toolCallLimit
  );
}

function decimal(value: unknown): bigint {
  if (typeof value !== 'string' || !U64_DECIMAL.test(value)) {
    throw new Error('Invalid lossless Deep Map count');
  }
  const parsed = BigInt(value);
  if (parsed > U64_MAX) {
    throw new Error('Deep Map count exceeds u64');
  }
  return parsed;
}

function integer(value: unknown, minimum: number, maximum: number): value is number {
  return (
    Number.isSafeInteger(value) && (value as number) >= minimum && (value as number) <= maximum
  );
}

function record(value: unknown, keys?: string[]): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Invalid Deep Map response object');
  }
  const object = value as Record<string, unknown>;
  if (keys !== undefined) {
    exactKeys(object, keys);
  }
  return object;
}

function exactKeys(value: Record<string, unknown>, expected: string[]): void {
  const actual = Object.keys(value).sort();
  const sorted = [...expected].sort();
  if (actual.length !== sorted.length || actual.some((key, index) => key !== sorted[index])) {
    throw new Error('Unexpected Deep Map response field');
  }
}

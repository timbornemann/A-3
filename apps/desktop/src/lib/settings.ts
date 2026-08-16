import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const DECIMAL_PATTERN = /^(?:0|[1-9][0-9]*)$/;
const PROFILE_ID_PATTERN = /^[0-9a-f]{64}$/;
const PROVIDER_ID_PATTERN = /^[A-Za-z0-9._-]{1,128}$/;
const MODEL_ID_PATTERN = /^[A-Za-z0-9._+/@:-]{1,512}$/;
const MAX_PERSISTED_INTEGER = 9_223_372_036_854_775_807n;

export type ModelEndpointScopeV1 = 'localLoopback' | 'remote';
export type ModelProviderKindV1 = 'ollama' | 'gemini';
export type ProviderHealthStatusV1 =
  'notChecked' | 'healthy' | 'capabilityLimited' | 'unreachable' | 'cancelled' | 'remoteBlocked';
export type ModelProfileActivationV1 = 'executable' | 'capabilityLimited';
export type ModelRoleV1 = 'coding' | 'mapping' | 'embedding';

export interface ModelEndpointV1 {
  origin: string;
  providerId: string;
  scope: ModelEndpointScopeV1;
}

export interface ProviderHealthV1 {
  checkedAtUnixMillis: string | null;
  status: ProviderHealthStatusV1;
}

export interface LlmRoleProfileV1 {
  activation: ModelProfileActivationV1;
  contextTokens: number;
  modelId: string;
  outputTokens: number;
  parallelism: number;
  probedAtUnixMillis: string;
  profileId: string;
  structuredOutput: 'verified' | 'unavailable';
  toolCallMode: 'disabled' | 'nativeProviderReported';
}

export interface EmbeddingRoleProfileV1 {
  dimension: number;
  maxBatchSize: number;
  modelId: string;
  probedAtUnixMillis: string;
  profileId: string;
}

export interface DataPrivacySettingsV1 {
  automaticProviderDiscoveryEnabled: false;
  cloudSyncEnabled: false;
  promptResponseLoggingEnabled: false;
  remoteRequestsWithoutApprovalEnabled: false;
  telemetryEnabled: false;
}

export interface SettingsV1 {
  codingProfile: LlmRoleProfileV1 | null;
  embeddingProfile: EmbeddingRoleProfileV1 | null;
  endpoint: ModelEndpointV1 | null;
  mappingProfile: LlmRoleProfileV1 | null;
  privacy: DataPrivacySettingsV1;
  probeActive: boolean;
  providerHealth: ProviderHealthV1 | null;
  revision: string;
}

export interface SettingsResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  settings: SettingsV1;
}

export interface LlmModelProbeInputV1 {
  contextTokens: number;
  modelId: string;
  outputTokens: number;
  parallelism: number;
  role: 'coding' | 'mapping';
}

export interface EmbeddingModelProbeInputV1 {
  maxBatchSize: number;
  modelId: string;
  role: 'embedding';
}

export type ModelProbeInputV1 = LlmModelProbeInputV1 | EmbeddingModelProbeInputV1;

export interface CancelModelProbeResponseV1 {
  cancellationRequested: boolean;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

export interface ProviderModelsResponseV1 {
  modelIds: string[];
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  providerKind: ModelProviderKindV1;
  settingsRevision: string;
  truncated: boolean;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function querySettings(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<SettingsResponseV1> {
  const payload = await invokeCommand('query_settings', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseSettingsResponseV1(payload);
}

export async function configureModelProvider(
  expectedSettingsRevision: string,
  providerKind: ModelProviderKindV1,
  endpointOrigin: string | null,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<SettingsResponseV1> {
  assertCanonicalDecimal(expectedSettingsRevision, 'Settings revision');
  const payload = await invokeCommand('configure_model_provider', {
    request: {
      endpointOrigin,
      expectedSettingsRevision,
      providerKind,
      protocolVersion: CURRENT_PROTOCOL_VERSION,
    },
  });
  return parseSettingsResponseV1(payload);
}

export async function discoverProviderModels(
  expectedSettingsRevision: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProviderModelsResponseV1> {
  assertCanonicalDecimal(expectedSettingsRevision, 'Settings revision');
  const payload = await invokeCommand('discover_provider_models', {
    request: {
      expectedSettingsRevision,
      protocolVersion: CURRENT_PROTOCOL_VERSION,
    },
  });
  return parseProviderModelsResponseV1(payload);
}

export async function probeModelRole(
  expectedSettingsRevision: string,
  input: ModelProbeInputV1,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<SettingsResponseV1> {
  assertCanonicalDecimal(expectedSettingsRevision, 'Settings revision');
  const request =
    input.role === 'embedding'
      ? {
          embeddingLimits: { maxBatchSize: input.maxBatchSize },
          expectedSettingsRevision,
          llmLimits: null,
          modelId: input.modelId,
          protocolVersion: CURRENT_PROTOCOL_VERSION,
          role: input.role,
        }
      : {
          embeddingLimits: null,
          expectedSettingsRevision,
          llmLimits: {
            contextTokens: input.contextTokens,
            outputTokens: input.outputTokens,
            parallelism: input.parallelism,
          },
          modelId: input.modelId,
          protocolVersion: CURRENT_PROTOCOL_VERSION,
          role: input.role,
        };
  const payload = await invokeCommand('probe_model_role', { request });
  return parseSettingsResponseV1(payload);
}

export async function cancelModelProbe(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<CancelModelProbeResponseV1> {
  const payload = await invokeCommand('cancel_model_probe', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['cancellationRequested', 'protocolVersion']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    typeof payload.cancellationRequested !== 'boolean'
  ) {
    throw new Error('Model probe cancellation response does not match the V1 schema.');
  }
  return {
    cancellationRequested: payload.cancellationRequested,
    protocolVersion: payload.protocolVersion,
  };
}

export function parseSettingsResponseV1(payload: unknown): SettingsResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'settings']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Settings response does not match the V1 schema.');
  }
  return {
    protocolVersion: payload.protocolVersion,
    settings: parseSettings(payload.settings),
  };
}

export function parseProviderModelsResponseV1(payload: unknown): ProviderModelsResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, [
      'modelIds',
      'protocolVersion',
      'providerKind',
      'settingsRevision',
      'truncated',
    ]) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    (payload.providerKind !== 'ollama' && payload.providerKind !== 'gemini') ||
    !Array.isArray(payload.modelIds) ||
    payload.modelIds.length > 256 ||
    typeof payload.settingsRevision !== 'string' ||
    typeof payload.truncated !== 'boolean'
  ) {
    throw new Error('Provider model catalog does not match the V1 schema.');
  }
  assertCanonicalDecimal(payload.settingsRevision, 'Settings revision');
  if (!payload.modelIds.every(isModelId)) {
    throw new Error('Provider model catalog contains invalid or non-canonical model IDs.');
  }
  const modelIds = payload.modelIds;
  if (modelIds.some((modelId, index) => index > 0 && modelIds[index - 1]! >= modelId)) {
    throw new Error('Provider model catalog contains invalid or non-canonical model IDs.');
  }
  return {
    modelIds,
    protocolVersion: payload.protocolVersion,
    providerKind: payload.providerKind,
    settingsRevision: payload.settingsRevision,
    truncated: payload.truncated,
  };
}

function parseSettings(value: unknown): SettingsV1 {
  const keys = [
    'codingProfile',
    'embeddingProfile',
    'endpoint',
    'mappingProfile',
    'privacy',
    'probeActive',
    'providerHealth',
    'revision',
  ];
  if (!isRecord(value) || !hasExactKeys(value, keys) || typeof value.probeActive !== 'boolean') {
    throw new Error('Settings response contains an invalid settings snapshot.');
  }
  assertCanonicalDecimal(value.revision, 'Settings revision');
  return {
    codingProfile: parseNullable(value.codingProfile, parseLlmProfile),
    embeddingProfile: parseNullable(value.embeddingProfile, parseEmbeddingProfile),
    endpoint: parseNullable(value.endpoint, parseEndpoint),
    mappingProfile: parseNullable(value.mappingProfile, parseLlmProfile),
    privacy: parsePrivacy(value.privacy),
    probeActive: value.probeActive,
    providerHealth: parseNullable(value.providerHealth, parseProviderHealth),
    revision: value.revision,
  };
}

function parseEndpoint(value: unknown): ModelEndpointV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['origin', 'providerId', 'scope']) ||
    typeof value.origin !== 'string' ||
    value.origin.length > 2_048 ||
    typeof value.providerId !== 'string' ||
    !PROVIDER_ID_PATTERN.test(value.providerId) ||
    (value.scope !== 'localLoopback' && value.scope !== 'remote')
  ) {
    throw new Error('Settings response contains an invalid endpoint.');
  }
  let parsed: URL;
  try {
    parsed = new URL(value.origin);
  } catch {
    throw new Error('Settings response contains an invalid endpoint.');
  }
  if (
    parsed.origin !== value.origin ||
    parsed.username !== '' ||
    parsed.password !== '' ||
    parsed.pathname !== '/' ||
    parsed.search !== '' ||
    parsed.hash !== '' ||
    (value.scope === 'remote' && parsed.protocol !== 'https:') ||
    (value.scope === 'localLoopback' && parsed.protocol !== 'http:')
  ) {
    throw new Error('Settings response contains an invalid endpoint.');
  }
  return { origin: value.origin, providerId: value.providerId, scope: value.scope };
}

function parseProviderHealth(value: unknown): ProviderHealthV1 {
  const statuses = new Set<ProviderHealthStatusV1>([
    'notChecked',
    'healthy',
    'capabilityLimited',
    'unreachable',
    'cancelled',
    'remoteBlocked',
  ]);
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['checkedAtUnixMillis', 'status']) ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status as ProviderHealthStatusV1) ||
    (value.checkedAtUnixMillis !== null && typeof value.checkedAtUnixMillis !== 'string')
  ) {
    throw new Error('Settings response contains invalid provider health.');
  }
  if (value.checkedAtUnixMillis !== null) {
    assertCanonicalDecimal(value.checkedAtUnixMillis, 'Provider health timestamp');
  }
  return {
    checkedAtUnixMillis: value.checkedAtUnixMillis,
    status: value.status as ProviderHealthStatusV1,
  };
}

function parseLlmProfile(value: unknown): LlmRoleProfileV1 {
  const keys = [
    'activation',
    'contextTokens',
    'modelId',
    'outputTokens',
    'parallelism',
    'probedAtUnixMillis',
    'profileId',
    'structuredOutput',
    'toolCallMode',
  ];
  if (
    !isRecord(value) ||
    !hasExactKeys(value, keys) ||
    typeof value.profileId !== 'string' ||
    !PROFILE_ID_PATTERN.test(value.profileId) ||
    !isModelId(value.modelId) ||
    !isIntegerInRange(value.contextTokens, 1_024, 1_048_576) ||
    !isIntegerInRange(value.outputTokens, 1, 262_144) ||
    !isIntegerInRange(value.parallelism, 1, 64) ||
    (value.structuredOutput !== 'verified' && value.structuredOutput !== 'unavailable') ||
    (value.toolCallMode !== 'disabled' && value.toolCallMode !== 'nativeProviderReported') ||
    (value.activation !== 'executable' && value.activation !== 'capabilityLimited') ||
    typeof value.probedAtUnixMillis !== 'string' ||
    (value.activation === 'executable' && value.structuredOutput !== 'verified') ||
    (value.activation === 'capabilityLimited' && value.structuredOutput !== 'unavailable')
  ) {
    throw new Error('Settings response contains an invalid LLM profile.');
  }
  assertCanonicalDecimal(value.probedAtUnixMillis, 'LLM probe timestamp');
  return value as unknown as LlmRoleProfileV1;
}

function parseEmbeddingProfile(value: unknown): EmbeddingRoleProfileV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      'dimension',
      'maxBatchSize',
      'modelId',
      'probedAtUnixMillis',
      'profileId',
    ]) ||
    typeof value.profileId !== 'string' ||
    !PROFILE_ID_PATTERN.test(value.profileId) ||
    !isModelId(value.modelId) ||
    !isIntegerInRange(value.dimension, 1, 8_192) ||
    !isIntegerInRange(value.maxBatchSize, 1, 64) ||
    typeof value.probedAtUnixMillis !== 'string'
  ) {
    throw new Error('Settings response contains an invalid embedding profile.');
  }
  assertCanonicalDecimal(value.probedAtUnixMillis, 'Embedding probe timestamp');
  return value as unknown as EmbeddingRoleProfileV1;
}

function parsePrivacy(value: unknown): DataPrivacySettingsV1 {
  const keys = [
    'automaticProviderDiscoveryEnabled',
    'cloudSyncEnabled',
    'promptResponseLoggingEnabled',
    'remoteRequestsWithoutApprovalEnabled',
    'telemetryEnabled',
  ];
  if (!isRecord(value) || !hasExactKeys(value, keys) || keys.some((key) => value[key] !== false)) {
    throw new Error('Settings response violates the fail-closed privacy contract.');
  }
  return value as unknown as DataPrivacySettingsV1;
}

function parseNullable<T>(value: unknown, parser: (candidate: unknown) => T): T | null {
  return value === null ? null : parser(value);
}

function assertCanonicalDecimal(value: unknown, label: string): asserts value is string {
  if (
    typeof value !== 'string' ||
    !DECIMAL_PATTERN.test(value) ||
    BigInt(value) > MAX_PERSISTED_INTEGER
  ) {
    throw new Error(`${label} is not a bounded canonical decimal.`);
  }
}

function isModelId(value: unknown): value is string {
  return typeof value === 'string' && MODEL_ID_PATTERN.test(value);
}

function isIntegerInRange(value: unknown, minimum: number, maximum: number): value is number {
  return Number.isInteger(value) && (value as number) >= minimum && (value as number) <= maximum;
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

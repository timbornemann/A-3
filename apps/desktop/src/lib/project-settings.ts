import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const ID_PATTERN = /^[0-9a-f]{64}$/;
const POSITIVE_DECIMAL_PATTERN = /^[1-9][0-9]*$/;
const LOWER_HEX_PATTERN = /^(?:[0-9a-f]{2})+$/;
const MAX_PERSISTED_INTEGER = 9_223_372_036_854_775_807n;
const MAX_IGNORE_PATTERNS = 256;
const MAX_IGNORE_PATTERN_BYTES = 1_024;
const MAX_COMMANDS = 256;
const MAX_EXECUTABLE_BYTES = 4 * 1_024;
const MAX_ARGUMENTS = 256;
const MAX_ARGUMENT_BYTES = 4 * 1_024;
const MAX_TOTAL_ARGUMENT_BYTES = 64 * 1_024;
const MAX_WORKING_DIRECTORY_HEX_LENGTH = 2 * 131_072;

export type DiscoveredCommandKindV1 = 'test' | 'build' | 'lint' | 'format';

export interface ProjectIgnoreSettingsV1 {
  configurationPresent: boolean;
  patterns: string[];
}

export interface DiscoveredCommandV1 {
  arguments: string[];
  commandId: string;
  evidenceCount: number;
  executable: string;
  kind: DiscoveredCommandKindV1;
  selected: boolean;
  workingDirectoryHex: string | null;
}

export type ProjectCommandConfirmationV1 =
  | { status: 'notConfirmed' }
  | { confirmedAtUnixMillis: string; revision: string; status: 'current' | 'stale' };

export type ProjectCommandSettingsV1 =
  | { status: 'noPublishedIndex' }
  | {
      catalogId: string;
      commands: DiscoveredCommandV1[];
      confirmation: ProjectCommandConfirmationV1;
      status: 'available';
    };

export interface ActiveProjectSettingsV1 {
  commands: ProjectCommandSettingsV1;
  ignore: ProjectIgnoreSettingsV1;
}

export type ProjectSettingsResultV1 =
  { status: 'noProject' } | { settings: ActiveProjectSettingsV1; status: 'available' };

export interface ProjectSettingsResponseV1 {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: ProjectSettingsResultV1;
}

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryProjectSettings(
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectSettingsResponseV1> {
  const payload = await invokeCommand('query_project_settings', {
    request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
  });
  return parseProjectSettingsResponseV1(payload);
}

export async function confirmProjectCommandAllowlist(
  expectedCatalogId: string,
  expectedAllowlistRevision: string | null,
  commandIds: string[],
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<ProjectSettingsResponseV1> {
  assertId(expectedCatalogId, 'Command catalog ID');
  if (expectedAllowlistRevision !== null) {
    assertPositiveDecimal(expectedAllowlistRevision, 'Command allowlist revision');
  }
  if (
    commandIds.length === 0 ||
    commandIds.length > MAX_COMMANDS ||
    new Set(commandIds).size !== commandIds.length
  ) {
    throw new Error('Command selection is empty, duplicated, or outside the V1 bound.');
  }
  commandIds.forEach((commandId) => assertId(commandId, 'Command ID'));

  const payload = await invokeCommand('confirm_project_command_allowlist', {
    request: {
      commandIds: [...commandIds].sort(),
      expectedAllowlistRevision,
      expectedCatalogId,
      protocolVersion: CURRENT_PROTOCOL_VERSION,
    },
  });
  return parseProjectSettingsResponseV1(payload);
}

export function parseProjectSettingsResponseV1(payload: unknown): ProjectSettingsResponseV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, ['protocolVersion', 'result']) ||
    payload.protocolVersion !== CURRENT_PROTOCOL_VERSION
  ) {
    throw new Error('Project Settings response does not match the V1 schema.');
  }
  return {
    protocolVersion: payload.protocolVersion,
    result: parseResult(payload.result),
  };
}

function parseResult(value: unknown): ProjectSettingsResultV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Project Settings response contains an invalid result.');
  }
  if (value.status === 'noProject' && hasExactKeys(value, ['status'])) {
    return { status: 'noProject' };
  }
  if (value.status !== 'available' || !hasExactKeys(value, ['settings', 'status'])) {
    throw new Error('Project Settings response contains an invalid result.');
  }
  return { settings: parseActiveSettings(value.settings), status: 'available' };
}

function parseActiveSettings(value: unknown): ActiveProjectSettingsV1 {
  if (!isRecord(value) || !hasExactKeys(value, ['commands', 'ignore'])) {
    throw new Error('Project Settings response contains an invalid active-project snapshot.');
  }
  return { commands: parseCommands(value.commands), ignore: parseIgnore(value.ignore) };
}

function parseIgnore(value: unknown): ProjectIgnoreSettingsV1 {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['configurationPresent', 'patterns']) ||
    typeof value.configurationPresent !== 'boolean' ||
    !Array.isArray(value.patterns) ||
    value.patterns.length > MAX_IGNORE_PATTERNS
  ) {
    throw new Error('Project Settings response contains invalid ignore settings.');
  }
  const patterns = value.patterns.map((pattern) => {
    if (
      typeof pattern !== 'string' ||
      pattern.length === 0 ||
      utf8Length(pattern) > MAX_IGNORE_PATTERN_BYTES ||
      hasControlCharacter(pattern)
    ) {
      throw new Error('Project Settings response contains an invalid ignore pattern.');
    }
    return pattern;
  });
  if (!value.configurationPresent && patterns.length !== 0) {
    throw new Error('Missing project configuration cannot own ignore patterns.');
  }
  return { configurationPresent: value.configurationPresent, patterns };
}

function parseCommands(value: unknown): ProjectCommandSettingsV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Project Settings response contains invalid command settings.');
  }
  if (value.status === 'noPublishedIndex' && hasExactKeys(value, ['status'])) {
    return { status: 'noPublishedIndex' };
  }
  if (
    value.status !== 'available' ||
    !hasExactKeys(value, ['catalogId', 'commands', 'confirmation', 'status']) ||
    typeof value.catalogId !== 'string' ||
    !Array.isArray(value.commands) ||
    value.commands.length > MAX_COMMANDS
  ) {
    throw new Error('Project Settings response contains invalid command settings.');
  }
  assertId(value.catalogId, 'Command catalog ID');
  const commands = value.commands.map(parseCommand);
  if (new Set(commands.map((command) => command.commandId)).size !== commands.length) {
    throw new Error('Project Settings response repeats a command ID.');
  }
  const confirmation = parseConfirmation(value.confirmation);
  const selectedCount = commands.filter((command) => command.selected).length;
  if (
    (confirmation.status === 'current' && selectedCount === 0) ||
    (confirmation.status !== 'current' && selectedCount !== 0)
  ) {
    throw new Error('Project Settings response contains an inconsistent command selection.');
  }
  return {
    catalogId: value.catalogId,
    commands,
    confirmation,
    status: 'available',
  };
}

function parseCommand(value: unknown): DiscoveredCommandV1 {
  const keys = [
    'arguments',
    'commandId',
    'evidenceCount',
    'executable',
    'kind',
    'selected',
    'workingDirectoryHex',
  ];
  if (
    !isRecord(value) ||
    !hasExactKeys(value, keys) ||
    typeof value.commandId !== 'string' ||
    typeof value.executable !== 'string' ||
    utf8Length(value.executable) === 0 ||
    utf8Length(value.executable) > MAX_EXECUTABLE_BYTES ||
    hasControlCharacter(value.executable) ||
    !isCommandKind(value.kind) ||
    typeof value.selected !== 'boolean' ||
    !Number.isInteger(value.evidenceCount) ||
    (value.evidenceCount as number) < 1 ||
    (value.evidenceCount as number) > 16 ||
    !Array.isArray(value.arguments) ||
    value.arguments.length > MAX_ARGUMENTS ||
    !isWorkingDirectoryHex(value.workingDirectoryHex)
  ) {
    throw new Error('Project Settings response contains an invalid discovered command.');
  }
  assertId(value.commandId, 'Command ID');
  let totalArgumentBytes = 0;
  const arguments_ = value.arguments.map((argument) => {
    if (typeof argument !== 'string' || argument.includes('\0')) {
      throw new Error('Project Settings response contains an invalid command argument.');
    }
    const bytes = utf8Length(argument);
    if (bytes > MAX_ARGUMENT_BYTES) {
      throw new Error('Project Settings response contains an oversized command argument.');
    }
    totalArgumentBytes += bytes;
    return argument;
  });
  if (totalArgumentBytes > MAX_TOTAL_ARGUMENT_BYTES) {
    throw new Error('Project Settings response contains an oversized argv.');
  }
  return {
    arguments: arguments_,
    commandId: value.commandId,
    evidenceCount: value.evidenceCount as number,
    executable: value.executable,
    kind: value.kind,
    selected: value.selected,
    workingDirectoryHex: value.workingDirectoryHex,
  };
}

function parseConfirmation(value: unknown): ProjectCommandConfirmationV1 {
  if (!isRecord(value) || typeof value.status !== 'string') {
    throw new Error('Project Settings response contains an invalid command confirmation.');
  }
  if (value.status === 'notConfirmed' && hasExactKeys(value, ['status'])) {
    return { status: 'notConfirmed' };
  }
  if (
    (value.status !== 'current' && value.status !== 'stale') ||
    !hasExactKeys(value, ['confirmedAtUnixMillis', 'revision', 'status']) ||
    typeof value.revision !== 'string' ||
    typeof value.confirmedAtUnixMillis !== 'string'
  ) {
    throw new Error('Project Settings response contains an invalid command confirmation.');
  }
  assertPositiveDecimal(value.revision, 'Command allowlist revision');
  assertNonNegativeDecimal(value.confirmedAtUnixMillis, 'Confirmation timestamp');
  return {
    confirmedAtUnixMillis: value.confirmedAtUnixMillis,
    revision: value.revision,
    status: value.status,
  };
}

function isWorkingDirectoryHex(value: unknown): value is string | null {
  return (
    value === null ||
    (typeof value === 'string' &&
      value.length <= MAX_WORKING_DIRECTORY_HEX_LENGTH &&
      LOWER_HEX_PATTERN.test(value))
  );
}

function isCommandKind(value: unknown): value is DiscoveredCommandKindV1 {
  return value === 'test' || value === 'build' || value === 'lint' || value === 'format';
}

function assertId(value: string, label: string): void {
  if (!ID_PATTERN.test(value)) throw new Error(`${label} is not canonical.`);
}

function assertPositiveDecimal(value: string, label: string): void {
  if (!POSITIVE_DECIMAL_PATTERN.test(value) || BigInt(value) > MAX_PERSISTED_INTEGER) {
    throw new Error(`${label} is not a bounded positive decimal.`);
  }
}

function assertNonNegativeDecimal(value: string, label: string): void {
  if (!/^(?:0|[1-9][0-9]*)$/.test(value) || BigInt(value) > MAX_PERSISTED_INTEGER) {
    throw new Error(`${label} is not a bounded canonical decimal.`);
  }
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

function hasControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return (
      codePoint !== undefined && (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f))
    );
  });
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

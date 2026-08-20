import { CURRENT_PROTOCOL_VERSION } from './health';

const MAX_SAFE_MESSAGE_LENGTH = 512;

export type ErrorCodeV1 =
  | 'unsupportedProtocolVersion'
  | 'projectSelectionFailed'
  | 'projectSelectionUnavailable'
  | 'notGitRepository'
  | 'projectRootRequired'
  | 'unsupportedRepository'
  | 'invalidRepositoryMetadata'
  | 'localStorageUnavailable'
  | 'localStorageCorrupt'
  | 'localStorageUpgradeRequired'
  | 'localStorageInvalidData'
  | 'projectIdentityConflict'
  | 'noActiveProject'
  | 'invalidRepositoryTreeQuery'
  | 'repositoryTreeDirectoryUnavailable'
  | 'invalidModuleTreeQuery'
  | 'moduleTreeParentUnavailable'
  | 'invalidModuleDependencyGraphQuery'
  | 'invalidModuleRuntimeMapQuery'
  | 'invalidModuleRuntimeFlowQuery'
  | 'invalidModuleCardDetailQuery'
  | 'invalidModuleCardEvidenceQuery'
  | 'invalidProjectMapSearchQuery'
  | 'invalidTaskLensSelection'
  | 'taskLensUnavailable'
  | 'invalidAgentGoal'
  | 'agentGoalTaskNotFound'
  | 'agentGoalRevisionConflict'
  | 'agentGoalUnavailable'
  | 'invalidAgentInspectionQuery'
  | 'agentInspectionUnavailable'
  | 'invalidAgentTaskControl'
  | 'agentTaskControlUnavailable'
  | 'invalidAgentApprovalRequest'
  | 'agentApprovalUnavailable'
  | 'indexRebuildAlreadyPending'
  | 'indexRebuildUnavailable'
  | 'projectOperationBusy'
  | 'projectNotInList'
  | 'projectRemovalUnavailable'
  | 'deepMapUnavailable'
  | 'invalidDeepMapBudget'
  | 'deepMapAlreadyPending'
  | 'deepMapNotRunning'
  | 'deepMapNotPaused'
  | 'invalidSettingsRequest'
  | 'modelEndpointInvalid'
  | 'modelProbeAlreadyActive'
  | 'modelSettingsUnavailable'
  | 'providerCredentialInvalid'
  | 'providerCredentialMissing'
  | 'providerCredentialRecoveryRequired'
  | 'providerCredentialStoreUnavailable'
  | 'invalidProjectSettingsRequest'
  | 'projectSettingsUnavailable';

export interface CommandErrorV1 {
  code: ErrorCodeV1;
  message: string;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
}

const ERROR_CODES = new Set<ErrorCodeV1>([
  'unsupportedProtocolVersion',
  'projectSelectionFailed',
  'projectSelectionUnavailable',
  'notGitRepository',
  'projectRootRequired',
  'unsupportedRepository',
  'invalidRepositoryMetadata',
  'localStorageUnavailable',
  'localStorageCorrupt',
  'localStorageUpgradeRequired',
  'localStorageInvalidData',
  'projectIdentityConflict',
  'noActiveProject',
  'invalidRepositoryTreeQuery',
  'repositoryTreeDirectoryUnavailable',
  'invalidModuleTreeQuery',
  'moduleTreeParentUnavailable',
  'invalidModuleDependencyGraphQuery',
  'invalidModuleRuntimeMapQuery',
  'invalidModuleRuntimeFlowQuery',
  'invalidModuleCardDetailQuery',
  'invalidModuleCardEvidenceQuery',
  'invalidProjectMapSearchQuery',
  'invalidTaskLensSelection',
  'taskLensUnavailable',
  'invalidAgentGoal',
  'agentGoalTaskNotFound',
  'agentGoalRevisionConflict',
  'agentGoalUnavailable',
  'invalidAgentInspectionQuery',
  'agentInspectionUnavailable',
  'invalidAgentTaskControl',
  'agentTaskControlUnavailable',
  'invalidAgentApprovalRequest',
  'agentApprovalUnavailable',
  'indexRebuildAlreadyPending',
  'indexRebuildUnavailable',
  'projectOperationBusy',
  'projectNotInList',
  'projectRemovalUnavailable',
  'deepMapUnavailable',
  'invalidDeepMapBudget',
  'deepMapAlreadyPending',
  'deepMapNotRunning',
  'deepMapNotPaused',
  'invalidSettingsRequest',
  'modelEndpointInvalid',
  'modelProbeAlreadyActive',
  'modelSettingsUnavailable',
  'providerCredentialInvalid',
  'providerCredentialMissing',
  'providerCredentialRecoveryRequired',
  'providerCredentialStoreUnavailable',
  'invalidProjectSettingsRequest',
  'projectSettingsUnavailable',
]);

export function parseCommandErrorV1(value: unknown): CommandErrorV1 | null {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['code', 'message', 'protocolVersion']) ||
    value.protocolVersion !== CURRENT_PROTOCOL_VERSION ||
    typeof value.code !== 'string' ||
    !ERROR_CODES.has(value.code as ErrorCodeV1) ||
    typeof value.message !== 'string' ||
    value.message.length === 0 ||
    value.message.length > MAX_SAFE_MESSAGE_LENGTH ||
    Array.from(value.message).some((character) => character < ' ' || character === '\u007f')
  ) {
    return null;
  }
  return {
    code: value.code as ErrorCodeV1,
    message: value.message,
    protocolVersion: value.protocolVersion,
  };
}

export function projectOpenRecoveryMessage(error: unknown): string {
  const code = parseCommandErrorV1(error)?.code;
  const messages: Partial<Record<ErrorCodeV1, string>> = {
    unsupportedProtocolVersion:
      'UI und Core verwenden unterschiedliche Protokollversionen. Starte A^3 neu und installiere bei Bedarf das aktuelle Build.',
    projectSelectionFailed:
      'Der native Ordnerdialog konnte nicht sicher verwendet werden. Starte A^3 neu und versuche die Auswahl erneut.',
    projectSelectionUnavailable:
      'Der gewählte Ordner ist nicht mehr verfügbar. Prüfe Laufwerk und Zugriffsrechte und wähle ihn erneut.',
    notGitRepository: 'Wähle den Stammordner eines lokalen Git-Repositories.',
    projectRootRequired: 'Wähle den Root des Git-Worktrees und keinen Unterordner.',
    unsupportedRepository:
      'Dieses Repository-Layout wird nicht unterstützt. Wähle einen normalen lokalen Git-Worktree.',
    invalidRepositoryMetadata:
      'Die lokalen Git-Metadaten sind ungültig. Prüfe oder repariere das Repository und wähle den Worktree erneut.',
    localStorageUnavailable:
      'Der lokale A^3-Speicher ist nicht verfügbar. Prüfe freien Speicherplatz und App-Data-Zugriffsrechte und versuche es erneut.',
    localStorageCorrupt:
      'Der lokale A^3-Speicher ist beschädigt und wurde nicht verändert. Sichere die App-Daten und stelle ein Backup wieder her.',
    localStorageUpgradeRequired:
      'Die lokalen A^3-Daten stammen aus einer neueren Version. Öffne sie mit derselben oder einer neueren A^3-Version.',
    localStorageInvalidData:
      'Die lokalen A^3-Projektdaten sind ungültig und wurden nicht verändert. Sichere die App-Daten, bevor du sie reparierst.',
    projectIdentityConflict:
      'Der Worktree widerspricht seiner gespeicherten Identität. Prüfe, ob du den richtigen Worktree-Root gewählt hast.',
    projectOperationBusy:
      'Eine andere Projektaktion läuft noch. Warte kurz und versuche die Auswahl erneut.',
  };
  return (
    (code === undefined ? undefined : messages[code]) ??
    'Der Ordner konnte nicht sicher geöffnet werden. Prüfe, ob er ein erreichbarer Git-Worktree-Root ist, und versuche es erneut.'
  );
}

export function projectActionRecoveryMessage(error: unknown, action: 'rebuild' | 'remove'): string {
  const code = parseCommandErrorV1(error)?.code;
  const common: Partial<Record<ErrorCodeV1, string>> = {
    unsupportedProtocolVersion:
      'UI und Core verwenden unterschiedliche Protokollversionen. Starte A^3 neu.',
    noActiveProject: 'Öffne zuerst einen lokalen Git-Worktree und versuche es erneut.',
    projectOperationBusy:
      'Eine andere Projektaktion läuft noch. Warte kurz und versuche es erneut.',
    localStorageUnavailable:
      'Der lokale A^3-Speicher ist nicht verfügbar. Prüfe Speicherplatz und Zugriffsrechte und versuche es erneut.',
    localStorageCorrupt:
      'Der lokale A^3-Speicher ist beschädigt und wurde nicht verändert. Stelle vor weiteren Schritten ein Backup sicher.',
    localStorageUpgradeRequired:
      'Die lokalen A^3-Daten benötigen dieselbe oder eine neuere A^3-Version.',
    localStorageInvalidData:
      'Die lokalen A^3-Daten sind ungültig und wurden nicht verändert. Sichere sie vor einer Reparatur.',
    projectIdentityConflict:
      'Die gespeicherte Projektidentität hat sich geändert. Öffne den richtigen Worktree-Root erneut.',
  };
  if (code !== undefined && common[code] !== undefined) {
    return common[code];
  }
  if (action === 'rebuild') {
    const rebuild: Partial<Record<ErrorCodeV1, string>> = {
      indexRebuildAlreadyPending:
        'Ein Rebuild läuft bereits. Aktualisiere den Status und warte auf den Abschluss.',
      indexRebuildUnavailable:
        'Der Index-Koordinator ist nicht verfügbar. Aktualisiere den Status und starte A^3 bei Bedarf neu.',
    };
    return (
      (code === undefined ? undefined : rebuild[code]) ??
      'Der Rebuild konnte nicht sicher angefordert werden. Aktualisiere den Status und versuche es erneut.'
    );
  }
  const removal: Partial<Record<ErrorCodeV1, string>> = {
    projectNotInList:
      'Der Worktree ist bereits nicht mehr in der Projektliste. Aktualisiere die Liste oder öffne ihn erneut.',
    projectRemovalUnavailable:
      'Der Worktree konnte nicht sicher deaktiviert werden. Aktualisiere den Status und starte A^3 bei Bedarf neu.',
  };
  return (
    (code === undefined ? undefined : removal[code]) ??
    'Der Worktree konnte nicht sicher entfernt werden. Aktualisiere den Status und versuche es erneut.'
  );
}

export function deepMapRecoveryMessage(error: unknown): string {
  const code = parseCommandErrorV1(error)?.code;
  const messages: Partial<Record<ErrorCodeV1, string>> = {
    unsupportedProtocolVersion:
      'UI und Core verwenden unterschiedliche Protokollversionen. Starte A^3 neu.',
    noActiveProject: 'Öffne zuerst einen lokalen Git-Worktree.',
    deepMapUnavailable:
      'Für Deep Map ist noch kein live verifiziertes lokales Mapping-Modell verfügbar.',
    invalidDeepMapBudget:
      'Das gewählte Token-, Zeit- oder Werkzeugbudget liegt außerhalb der festen Grenzen.',
    deepMapAlreadyPending:
      'Eine Deep-Map-Aktion oder ein pausierter Checkpoint ist bereits aktiv. Aktualisiere den Status.',
    deepMapNotRunning: 'Es läuft kein Deep-Map-Job, der pausiert oder abgebrochen werden kann.',
    deepMapNotPaused: 'Es ist kein validierter pausierter Deep-Map-Checkpoint vorhanden.',
  };
  return (
    (code === undefined ? undefined : messages[code]) ??
    'Die Deep-Map-Aktion konnte nicht sicher ausgeführt werden. Aktualisiere den Status und versuche es erneut.'
  );
}

export function agentGoalRecoveryMessage(error: unknown): string {
  const code = parseCommandErrorV1(error)?.code;
  const messages: Partial<Record<ErrorCodeV1, string>> = {
    unsupportedProtocolVersion:
      'UI und Core verwenden unterschiedliche Protokollversionen. Starte A^3 neu.',
    noActiveProject: 'Öffne zuerst einen lokalen Git-Worktree.',
    invalidAgentGoal:
      'Prüfe Pflichtfelder, UTF-8-Limits, doppelte Einträge und die Must-/Should-Kriterien.',
    agentGoalTaskNotFound:
      'Die Aufgabe existiert in diesem Worktree nicht mehr. Aktualisiere die Aufgabenliste.',
    agentGoalRevisionConflict:
      'Der Goal Contract wurde zwischenzeitlich geändert. Lade die aktuelle Revision neu und übernimm deine Änderung bewusst erneut.',
    agentGoalUnavailable:
      'Core-Metadaten oder der lokale Goal-Speicher sind nicht verfügbar. Versuche es erneut.',
    localStorageUnavailable:
      'Der lokale A^3-Speicher ist nicht verfügbar. Prüfe Speicherplatz und Zugriffsrechte.',
    localStorageCorrupt:
      'Der lokale A^3-Speicher ist beschädigt und wurde nicht verändert. Sichere die App-Daten.',
    localStorageUpgradeRequired: 'Die Goal-Daten benötigen dieselbe oder eine neuere A^3-Version.',
    localStorageInvalidData:
      'Die lokalen Goal-Daten verletzen den gespeicherten Vertrag und wurden nicht verändert.',
  };
  return (
    (code === undefined ? undefined : messages[code]) ??
    'Der Goal Contract konnte nicht sicher verarbeitet werden. Aktualisiere ihn und versuche es erneut.'
  );
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

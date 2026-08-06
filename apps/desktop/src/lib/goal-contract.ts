import { CURRENT_PROTOCOL_VERSION } from './health';

const STABLE_ID_PATTERN = /^[0-9a-f]{64}$/;
const DECIMAL_MILLIS_PATTERN = /^(?:0|[1-9][0-9]{0,18})$/;
const MAX_PERSISTED_MILLIS = 9_223_372_036_854_775_807n;
const MAX_COLLECTION_ITEMS = 64;
const MAX_OBJECTIVE_BYTES = 16 * 1_024;
const MAX_ITEM_BYTES = 4 * 1_024;
const MAX_SUCCESS_VERIFICATION_BYTES = 8 * 1_024;
const MAX_REVISION_REASON_BYTES = 4 * 1_024;
const MAX_REVISION = 4_294_967_295;

export interface AcceptanceCriterionV1 {
  criterionId: string;
  statement: string;
}

export interface GoalContractDraftV1 {
  acceptanceCriteria: AcceptanceCriterionV1[];
  constraints: string[];
  nonGoals: string[];
  objective: string;
  successVerification: string;
  userDecisions: string[];
}

export interface GoalContractV1 extends GoalContractDraftV1 {
  createdAtUnixMillis: string;
  previousRevision: number | null;
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  revision: number;
  revisionReason: string | null;
  taskId: string;
}

export function parseGoalContractV1(payload: unknown): GoalContractV1 {
  if (
    !isRecord(payload) ||
    !hasExactKeys(payload, [
      'acceptanceCriteria',
      'constraints',
      'createdAtUnixMillis',
      'nonGoals',
      'objective',
      'previousRevision',
      'protocolVersion',
      'revision',
      'revisionReason',
      'successVerification',
      'taskId',
      'userDecisions',
    ])
  ) {
    throw new Error('Goal Contract does not match the V1 schema.');
  }
  if (payload.protocolVersion !== CURRENT_PROTOCOL_VERSION) {
    throw new Error('Goal Contract uses an unsupported protocol version.');
  }
  if (typeof payload.taskId !== 'string' || !STABLE_ID_PATTERN.test(payload.taskId)) {
    throw new Error('Goal Contract contains an invalid task identity.');
  }
  if (!isRevision(payload.revision)) {
    throw new Error('Goal Contract contains an invalid revision.');
  }

  const revisionLink = parseRevisionLink(
    payload.revision,
    payload.previousRevision,
    payload.revisionReason,
  );
  const acceptanceCriteria = parseAcceptanceCriteria(payload.acceptanceCriteria);
  const constraints = parseUniqueTextList(payload.constraints, 'constraints');
  const nonGoals = parseUniqueTextList(payload.nonGoals, 'non-goals');
  const userDecisions = parseUniqueTextList(payload.userDecisions, 'user decisions');
  const createdAtUnixMillis = parseUnixMillis(payload.createdAtUnixMillis);

  return {
    acceptanceCriteria,
    constraints,
    createdAtUnixMillis,
    nonGoals,
    objective: parseGoalText(payload.objective, MAX_OBJECTIVE_BYTES, 'objective'),
    previousRevision: revisionLink.previousRevision,
    protocolVersion: payload.protocolVersion,
    revision: payload.revision,
    revisionReason: revisionLink.revisionReason,
    successVerification: parseGoalText(
      payload.successVerification,
      MAX_SUCCESS_VERIFICATION_BYTES,
      'success verification',
    ),
    taskId: payload.taskId,
    userDecisions,
  };
}

function parseRevisionLink(
  revision: number,
  previousRevision: unknown,
  revisionReason: unknown,
): Pick<GoalContractV1, 'previousRevision' | 'revisionReason'> {
  if (revision === 1) {
    if (previousRevision === null && revisionReason === null) {
      return { previousRevision: null, revisionReason: null };
    }
    throw new Error('Initial Goal Contract contains invalid revision metadata.');
  }
  if (
    !isRevision(previousRevision) ||
    previousRevision !== revision - 1 ||
    typeof revisionReason !== 'string'
  ) {
    throw new Error('Goal Contract revision does not name its immediate predecessor.');
  }
  return {
    previousRevision,
    revisionReason: parseGoalText(revisionReason, MAX_REVISION_REASON_BYTES, 'revision reason'),
  };
}

function parseAcceptanceCriteria(value: unknown): AcceptanceCriterionV1[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_COLLECTION_ITEMS) {
    throw new Error('Goal Contract contains an invalid acceptance-criteria list.');
  }
  const criterionIds = new Set<string>();
  const statements = new Set<string>();
  return value.map((item) => {
    if (
      !isRecord(item) ||
      !hasExactKeys(item, ['criterionId', 'statement']) ||
      typeof item.criterionId !== 'string' ||
      !STABLE_ID_PATTERN.test(item.criterionId)
    ) {
      throw new Error('Goal Contract contains an invalid acceptance criterion.');
    }
    const statement = parseGoalText(item.statement, MAX_ITEM_BYTES, 'acceptance criterion');
    if (criterionIds.has(item.criterionId) || statements.has(statement)) {
      throw new Error('Goal Contract contains duplicate acceptance criteria.');
    }
    criterionIds.add(item.criterionId);
    statements.add(statement);
    return { criterionId: item.criterionId, statement };
  });
}

function parseUniqueTextList(value: unknown, field: string): string[] {
  if (!Array.isArray(value) || value.length > MAX_COLLECTION_ITEMS) {
    throw new Error(`Goal Contract contains an invalid ${field} list.`);
  }
  const unique = new Set<string>();
  return value.map((item) => {
    const text = parseGoalText(item, MAX_ITEM_BYTES, field);
    if (unique.has(text)) {
      throw new Error(`Goal Contract contains duplicate ${field}.`);
    }
    unique.add(text);
    return text;
  });
}

function parseGoalText(value: unknown, maximumBytes: number, field: string): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > maximumBytes ||
    value !== value.replace(/\r\n?/gu, '\n').trim() ||
    containsUnsupportedControl(value) ||
    new TextEncoder().encode(value).length > maximumBytes
  ) {
    throw new Error(`Goal Contract contains an invalid ${field}.`);
  }
  return value;
}

function containsUnsupportedControl(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return (
      codePoint !== undefined &&
      ((codePoint <= 31 && codePoint !== 9 && codePoint !== 10) ||
        (codePoint >= 127 && codePoint <= 159))
    );
  });
}

function parseUnixMillis(value: unknown): string {
  if (
    typeof value !== 'string' ||
    !DECIMAL_MILLIS_PATTERN.test(value) ||
    BigInt(value) > MAX_PERSISTED_MILLIS
  ) {
    throw new Error('Goal Contract contains an invalid creation timestamp.');
  }
  return value;
}

function isRevision(value: unknown): value is number {
  return (
    typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= MAX_REVISION
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

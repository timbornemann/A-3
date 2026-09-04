import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { CURRENT_PROTOCOL_VERSION, type InvokeCommand } from './health';

const STABLE_ID = /^[0-9a-f]{64}$/u;
const ARTIFACT_REF = /^[0-9a-f]{128}$/u;
const DECIMAL = /^(?:0|[1-9][0-9]{0,18})$/u;
const KINDS = ['flowchart', 'sequence', 'class', 'state', 'entityRelationship'] as const;

export type AgentDiagramKindV1 = (typeof KINDS)[number];
export type AgentDiagramExportFormatV1 = 'svg' | 'png';
export type AgentDiagramExportThemeV1 = 'light' | 'dark' | 'transparent';

export interface AgentDiagramSummaryV1 {
  artifactRef: string;
  description: string;
  kind: AgentDiagramKindV1;
  stale: boolean;
  title: string;
  userSequence: string;
}

export interface AgentDiagramArtifactV1 {
  mermaid: string;
  summary: AgentDiagramSummaryV1;
}

export type AgentDiagramArtifactsResponseV1 = {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result:
    | { kind: 'noProject' }
    | { kind: 'notFound' }
    | { artifacts: AgentDiagramSummaryV1[]; kind: 'available' };
};

export type AgentDiagramArtifactResponseV1 = {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result:
    | { kind: 'noProject' }
    | { kind: 'notFound' }
    | { artifact: AgentDiagramArtifactV1; kind: 'available' };
};

export type AgentDiagramExportResponseV1 = {
  protocolVersion: typeof CURRENT_PROTOCOL_VERSION;
  result: { kind: 'cancelled' | 'exported' | 'notFound' | 'invalidPayload' | 'failed' };
};

const invokeThroughTauri: InvokeCommand = (command, arguments_) =>
  tauriInvoke<unknown>(command, arguments_);

export async function queryAgentDiagramArtifacts(
  sessionId: string,
  userSequence: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentDiagramArtifactsResponseV1> {
  stable(sessionId, 'Session');
  decimal(userSequence, 'Turn');
  return parseList(
    await invokeCommand('query_agent_diagram_artifacts', {
      request: {
        protocolVersion: CURRENT_PROTOCOL_VERSION,
        sessionId,
        userSequence,
      },
    }),
  );
}

export async function queryAgentDiagramArtifact(
  sessionId: string,
  artifactRef: string,
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentDiagramArtifactResponseV1> {
  stable(sessionId, 'Session');
  artifactReference(artifactRef);
  return parseArtifact(
    await invokeCommand('query_agent_diagram_artifact', {
      request: { artifactRef, protocolVersion: CURRENT_PROTOCOL_VERSION, sessionId },
    }),
  );
}

export async function exportAgentDiagram(
  input: {
    artifactRef: string;
    format: AgentDiagramExportFormatV1;
    renderedPayload: string;
    sessionId: string;
    theme: AgentDiagramExportThemeV1;
  },
  invokeCommand: InvokeCommand = invokeThroughTauri,
): Promise<AgentDiagramExportResponseV1> {
  stable(input.sessionId, 'Session');
  artifactReference(input.artifactRef);
  if (
    !['svg', 'png'].includes(input.format) ||
    !['light', 'dark', 'transparent'].includes(input.theme)
  )
    throw new Error('Ungültige Exportoption.');
  if (!input.renderedPayload || input.renderedPayload.length > 16 * 1024 * 1024)
    throw new Error('Das gerenderte Diagramm ist zu groß.');
  const response = obj(
    await invokeCommand('export_agent_diagram', {
      request: { ...input, protocolVersion: CURRENT_PROTOCOL_VERSION },
    }),
  );
  protocol(response.protocolVersion);
  const result = obj(response.result);
  if (
    !['cancelled', 'exported', 'notFound', 'invalidPayload', 'failed'].includes(String(result.kind))
  )
    throw new Error('Ungültige Diagramm-Exportantwort.');
  return response as AgentDiagramExportResponseV1;
}

function parseList(payload: unknown): AgentDiagramArtifactsResponseV1 {
  const response = obj(payload);
  protocol(response.protocolVersion);
  const result = obj(response.result);
  if (result.kind === 'noProject' || result.kind === 'notFound')
    return response as AgentDiagramArtifactsResponseV1;
  if (
    result.kind !== 'available' ||
    !Array.isArray(result.artifacts) ||
    result.artifacts.length > 3
  )
    throw new Error('Ungültige Diagrammliste.');
  result.artifacts.forEach(parseAgentDiagramSummaryV1);
  return response as AgentDiagramArtifactsResponseV1;
}

function parseArtifact(payload: unknown): AgentDiagramArtifactResponseV1 {
  const response = obj(payload);
  protocol(response.protocolVersion);
  const result = obj(response.result);
  if (result.kind === 'noProject' || result.kind === 'notFound')
    return response as AgentDiagramArtifactResponseV1;
  if (result.kind !== 'available') throw new Error('Ungültige Diagrammantwort.');
  const artifact = obj(result.artifact);
  parseAgentDiagramSummaryV1(artifact.summary);
  if (
    typeof artifact.mermaid !== 'string' ||
    artifact.mermaid.length > 65_536 ||
    !/^(?:flowchart TD|sequenceDiagram|classDiagram|stateDiagram-v2|erDiagram)\n/u.test(
      artifact.mermaid,
    ) ||
    artifact.mermaid.includes('<') ||
    artifact.mermaid.includes('click ') ||
    artifact.mermaid.includes('%%{')
  )
    throw new Error('Ungültiges Diagramm.');
  return response as AgentDiagramArtifactResponseV1;
}

export function parseAgentDiagramSummaryV1(value: unknown): AgentDiagramSummaryV1 {
  const item = obj(value);
  artifactReference(item.artifactRef);
  decimal(item.userSequence, 'Turn');
  if (
    !KINDS.includes(item.kind as AgentDiagramKindV1) ||
    typeof item.title !== 'string' ||
    item.title.length === 0 ||
    item.title.length > 256 ||
    typeof item.description !== 'string' ||
    item.description.length === 0 ||
    item.description.length > 2_048 ||
    typeof item.stale !== 'boolean'
  )
    throw new Error('Ungültige Diagrammübersicht.');
  return item as unknown as AgentDiagramSummaryV1;
}

function obj(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error('Ungültige Diagrammantwort.');
  return value as Record<string, unknown>;
}

function protocol(value: unknown): void {
  if (value !== CURRENT_PROTOCOL_VERSION) throw new Error('Nicht unterstützte Protokollversion.');
}

function stable(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !STABLE_ID.test(value))
    throw new Error(`${label} ist ungültig.`);
}

function artifactReference(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !ARTIFACT_REF.test(value))
    throw new Error('Diagramm ist ungültig.');
}

function decimal(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !DECIMAL.test(value)) throw new Error(`${label} ist ungültig.`);
}

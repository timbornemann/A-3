import { describe, expect, it, vi } from 'vitest';
import {
  exportAgentDiagram,
  queryAgentDiagramArtifact,
  queryAgentDiagramArtifacts,
} from './agent-diagram';

const sessionId = 'a'.repeat(64);
const artifactRef = 'b'.repeat(128);
const summary = {
  artifactRef,
  description: 'Belegter Ablauf',
  kind: 'flowchart' as const,
  stale: false,
  title: 'Ablauf',
  userSequence: '1',
};

describe('Agent diagram IPC', () => {
  it('loads bounded summaries and only Core-compiled Mermaid', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: { artifacts: [summary], kind: 'available' },
      })
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: {
          artifact: { mermaid: 'flowchart TD\n  n0["Start"]\n', summary },
          kind: 'available',
        },
      });

    const list = await queryAgentDiagramArtifacts(sessionId, '1', invoke);
    const detail = await queryAgentDiagramArtifact(sessionId, artifactRef, invoke);

    expect(list.result.kind).toBe('available');
    expect(detail.result.kind).toBe('available');
    expect(invoke).toHaveBeenNthCalledWith(1, 'query_agent_diagram_artifacts', {
      request: { protocolVersion: 1, sessionId, userSequence: '1' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'query_agent_diagram_artifact', {
      request: { artifactRef, protocolVersion: 1, sessionId },
    });
  });

  it('rejects raw directives and exports without a destination path', async () => {
    await expect(
      queryAgentDiagramArtifact(
        sessionId,
        artifactRef,
        vi.fn().mockResolvedValue({
          protocolVersion: 1,
          result: {
            artifact: { mermaid: 'flowchart TD\nclick n0 "https://example.com"', summary },
            kind: 'available',
          },
        }),
      ),
    ).rejects.toThrow(/Ungültiges Diagramm/u);

    const invoke = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: { kind: 'exported' },
    });
    await exportAgentDiagram(
      {
        artifactRef,
        format: 'svg',
        renderedPayload: '<svg xmlns="http://www.w3.org/2000/svg"></svg>',
        sessionId,
        theme: 'light',
      },
      invoke,
    );
    const request = invoke.mock.calls[0][1].request as Record<string, unknown>;
    expect(request).not.toHaveProperty('path');
    expect(request).toMatchObject({ artifactRef, format: 'svg', sessionId, theme: 'light' });
  });
});

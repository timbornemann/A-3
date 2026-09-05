import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import MapInspector from './MapInspector.svelte';
import type {
  ProjectMapAtlasNodeV1,
  ProjectMapAtlasRelationV1,
  ProjectMapEntityContextV1,
} from './project-map-atlas';

const id = (value: number): string => value.toString(16).padStart(64, '0');

function node(value: number, displayName: string): ProjectMapAtlasNodeV1 {
  return {
    claimBadgeCount: 0,
    currentRiskCount: '0',
    detail: null,
    dimmed: false,
    displayName,
    evidenceId: id(value + 10),
    fileCount: '1',
    kind: 'file',
    mappingStatus: null,
    memberCount: '0',
    nodeId: id(value),
    parentNodeId: id(9),
    purpose: null,
    rank: value,
    selection: {
      evidenceId: id(value + 10),
      kind: 'file',
      moduleId: id(9),
      ordinal: value,
    },
    symbolCount: '1',
    volume: '1',
  };
}

function relation(
  source: ProjectMapAtlasNodeV1,
  target: ProjectMapAtlasNodeV1,
  kind: 'imports' | 'calls',
): ProjectMapAtlasRelationV1 {
  return {
    claimBadgeCount: 0,
    confidenceBasisPoints: 10_000,
    evidence: null,
    evidenceCount: '1',
    provider: 'treeSitter',
    relation: kind,
    sourceNodeId: source.nodeId,
    targetNodeId: target.nodeId,
    uncertainty: null,
  };
}

describe('MapInspector', () => {
  it('offers the step explorer for a callable without granting a path capability', async () => {
    const onfunctionflow = vi.fn();
    const selected: ProjectMapAtlasNodeV1 = {
      ...node(1, 'A'),
      kind: 'callable',
      selection: { kind: 'symbol', moduleId: id(9), symbolId: id(1), evidenceId: id(11) },
    };
    render(MapInspector, {
      props: {
        selected,
        onfunctionflow,
        context: { kind: 'idle' },
        flow: { kind: 'idle' },
        inventory: { kind: 'idle' },
        preview: { kind: 'idle' },
        onclose: vi.fn(),
        onevidence: vi.fn(),
        onflow: vi.fn(),
        oninventory: vi.fn(),
        onopen: vi.fn(),
        onselect: vi.fn(),
      },
    });
    await fireEvent.click(
      screen.getByRole('button', { name: 'Schritte und Werte in Abläufe erkunden' }),
    );
    expect(onfunctionflow).toHaveBeenCalledTimes(1);
  });
  it('renders one related entity when multiple relations reference the same node', () => {
    const selected = node(1, 'source.py');
    const related = node(2, 'shared.py');
    const context: ProjectMapEntityContextV1 = {
      architectureRelationCount: '2',
      architectureRelations: [
        relation(selected, related, 'imports'),
        relation(selected, related, 'calls'),
      ],
      boundaryCount: '0',
      boundaryNodes: [],
      boundaryRelations: [],
      claims: [],
      documentRelationCount: '0',
      entity: selected,
      indexRunId: id(20),
      relatedNodes: [related, related],
      relationCounts: [
        { incoming: '0', outgoing: '1', relation: 'imports' },
        { incoming: '0', outgoing: '1', relation: 'calls' },
      ],
      snapshotId: id(21),
      sourceEdgesTruncated: false,
    };

    render(MapInspector, {
      props: {
        context: { kind: 'available', value: context },
        flow: { kind: 'idle' },
        inventory: { kind: 'idle' },
        onclose: vi.fn(),
        onevidence: vi.fn(),
        onflow: vi.fn(),
        oninventory: vi.fn(),
        onopen: vi.fn(),
        onselect: vi.fn(),
        preview: { kind: 'idle' },
        selected,
      },
    });

    expect(screen.getAllByRole('button', { name: /shared\.py/ })).toHaveLength(1);
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    expect(screen.getAllByText('Datei')).toHaveLength(2);
    expect(screen.getByText('Verbindungen')).toBeTruthy();
    expect(screen.getAllByText('Importiert').length).toBeGreaterThan(0);
    expect(screen.queryByText('imports')).toBeNull();
  });
});

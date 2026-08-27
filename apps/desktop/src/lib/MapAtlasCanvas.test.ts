import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import MapAtlasCanvas from './MapAtlasCanvas.svelte';
import type {
  ProjectMapAtlasNodeV1,
  ProjectMapAtlasRelationV1,
  ProjectMapAtlasSceneV1,
} from './project-map-atlas';

const id = (value: number): string => value.toString(16).padStart(64, '0');

function node(rank: number, displayName: string): ProjectMapAtlasNodeV1 {
  return {
    claimBadgeCount: 0,
    currentRiskCount: '0',
    detail: null,
    dimmed: false,
    displayName,
    evidenceId: id(rank + 20),
    fileCount: '1',
    kind: 'file',
    mappingStatus: null,
    memberCount: '0',
    nodeId: id(rank),
    parentNodeId: id(19),
    purpose: null,
    rank,
    selection: {
      evidenceId: id(rank + 20),
      kind: 'file',
      moduleId: id(19),
      ordinal: rank,
    },
    symbolCount: '4',
    volume: '4',
  };
}

function relation(
  source: ProjectMapAtlasNodeV1,
  target: ProjectMapAtlasNodeV1,
): ProjectMapAtlasRelationV1 {
  return {
    claimBadgeCount: 0,
    confidenceBasisPoints: 10_000,
    evidence: null,
    evidenceCount: '1',
    provider: 'treeSitter',
    relation: 'imports',
    sourceNodeId: source.nodeId,
    targetNodeId: target.nodeId,
    uncertainty: null,
  };
}

function scene(nodes: ProjectMapAtlasNodeV1[]): ProjectMapAtlasSceneV1 {
  return {
    boundariesTruncated: false,
    boundaryCount: '0',
    breadcrumb: [{ label: 'Projekt', selection: null }],
    indexRunId: id(30),
    inspectedEdgeCount: '1',
    level: 'module',
    nodeCount: String(nodes.length),
    nodes,
    nodesTruncated: false,
    policyVersion: 1,
    relationCount: '1',
    relations: [relation(nodes[0], nodes[1])],
    relationsTruncated: false,
    selection: null,
    snapshotId: id(31),
    sourceEdgesTruncated: false,
    unresolvedCount: '0',
  };
}

describe('MapAtlasCanvas', () => {
  it('renders a routed dependency graph and emphasizes the selected node connections', async () => {
    const source = node(1, 'source.py');
    const target = node(2, 'target.py');
    const onselect = vi.fn();
    const onopen = vi.fn();
    const view = render(MapAtlasCanvas, {
      onopen,
      onselect,
      scene: scene([source, target]),
      selectedNodeId: source.nodeId,
    });

    const sourceButton = screen.getByRole('button', {
      name: /source\.py.*1 sichtbare Verbindungen/,
    });
    const targetButton = screen.getByRole('button', {
      name: /target\.py.*1 sichtbare Verbindungen/,
    });
    expect(parseFloat(sourceButton.style.left)).toBeLessThan(parseFloat(targetButton.style.left));

    const route = view.container.querySelector<SVGPathElement>('.route[data-relation="imports"]');
    expect(route?.getAttribute('d')).toMatch(/^M .+ C .+/);
    expect(route?.classList.contains('incident')).toBe(true);
    expect(view.container.querySelector('.route-label')?.textContent).toBe('importiert');

    await fireEvent.click(targetButton);
    expect(onselect).toHaveBeenCalledWith(target);
    await fireEvent.dblClick(targetButton);
    expect(onopen).toHaveBeenCalledWith(target);
  });
});

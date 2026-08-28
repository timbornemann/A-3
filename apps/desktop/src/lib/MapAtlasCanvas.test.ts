import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
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
    expect(route?.getAttribute('d')).toMatch(/^M .+ [HV] .+/);
    expect(route?.getAttribute('d')).not.toContain(' C ');
    expect(route?.classList.contains('incident')).toBe(true);
    expect(view.container.querySelector('.route-label')?.textContent).toBe('importiert');

    await fireEvent.click(targetButton);
    await waitFor(() => expect(onselect).toHaveBeenCalledWith(target));
    await fireEvent.dblClick(targetButton);
    expect(onopen).toHaveBeenCalledWith(target);
  });

  it('does not issue a competing selection read when a pointer double-click opens a node', async () => {
    const source = node(1, 'source.py');
    const target = node(2, 'target.py');
    const onselect = vi.fn();
    const onopen = vi.fn();
    render(MapAtlasCanvas, {
      onopen,
      onselect,
      scene: scene([source, target]),
      selectedNodeId: null,
    });
    const targetButton = screen.getByRole('button', { name: /target\.py, Datei/ });

    await fireEvent.click(targetButton, { detail: 1 });
    await fireEvent.click(targetButton, { detail: 2 });
    await fireEvent.dblClick(targetButton);
    await new Promise((resolve) => setTimeout(resolve, 210));

    expect(onselect).not.toHaveBeenCalled();
    expect(onopen).toHaveBeenCalledOnce();
    expect(onopen).toHaveBeenCalledWith(target);
  });

  it('pans by pointer drag, zooms at the pointer, and keeps the summary outside the map plane', async () => {
    const source = node(1, 'source.py');
    const target = node(2, 'target.py');
    const view = render(MapAtlasCanvas, {
      onopen: vi.fn(),
      onselect: vi.fn(),
      scene: scene([source, target]),
      selectedNodeId: null,
    });
    const canvas = screen.getByRole('region', { name: 'Progressiver Architektur-Atlas' });
    Object.defineProperty(canvas, 'scrollLeft', { configurable: true, value: 100, writable: true });
    Object.defineProperty(canvas, 'scrollTop', { configurable: true, value: 80, writable: true });
    vi.spyOn(canvas, 'getBoundingClientRect').mockReturnValue({
      bottom: 620,
      height: 620,
      left: 0,
      right: 900,
      toJSON: () => undefined,
      top: 0,
      width: 900,
      x: 0,
      y: 0,
    });

    await fireEvent.pointerDown(canvas, { button: 0, clientX: 200, clientY: 160, pointerId: 7 });
    await fireEvent.pointerMove(canvas, { clientX: 150, clientY: 120, pointerId: 7 });
    expect(canvas.scrollLeft).toBe(150);
    expect(canvas.scrollTop).toBe(120);
    expect(canvas.classList.contains('dragging')).toBe(true);
    await fireEvent.pointerUp(canvas, { pointerId: 7 });
    expect(canvas.classList.contains('dragging')).toBe(false);

    await fireEvent.wheel(canvas, { clientX: 300, clientY: 220, deltaY: -120 });
    await waitFor(() => expect(Number(canvas.dataset.zoom)).toBeGreaterThan(1));
    expect(view.container.querySelector('.canvas-plane')?.getAttribute('style')).toMatch(
      /transform:\s*scale\(/,
    );

    const summary = screen.getByText('Nichtgrafische Zusammenfassung').closest('details');
    expect(summary?.parentElement?.classList.contains('canvas-shell')).toBe(true);
    expect(view.container.querySelector('.canvas-plane .atlas-summary')).toBeNull();
  });
});

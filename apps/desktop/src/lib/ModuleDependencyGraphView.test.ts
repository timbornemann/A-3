import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ModuleDependencyGraphV1 } from './module-dependency-graph';
import ModuleDependencyGraphView from './ModuleDependencyGraphView.svelte';

function largeRepositoryGraph(): ModuleDependencyGraphV1 {
  const nodes = Array.from({ length: 50 }, (_, index) => ({
    kind: index === 0 ? ('manifestBoundary' as const) : ('pathBoundary' as const),
    moduleId: index.toString(16).padStart(64, '0'),
    name: index === 0 ? 'Repository' : `module-${index}`,
    nameTruncated: false,
    representativeEvidence: null,
    rootPathHex: index === 0 ? null : '737263',
  }));
  return {
    centerModuleId: nodes[0]!.moduleId,
    edges: [],
    edgesTruncated: true,
    indexRunId: 'a'.repeat(64),
    inspectedEdgeCount: '4096',
    nodes,
    nodesTruncated: true,
    observedEdgeGroupCount: '4096',
    observedNeighborCount: '4096',
    snapshotId: 'b'.repeat(64),
    sourceEdgesTruncated: true,
    unmappedEdgeCount: '0',
  };
}

describe('ModuleDependencyGraphView', () => {
  it('renders only the requested 50-node subset while exposing large-repository truncation', () => {
    render(ModuleDependencyGraphView, {
      props: {
        graph: largeRepositoryGraph(),
        onClearEvidence: vi.fn(),
        onSelectEvidence: vi.fn(),
        selectedEvidence: null,
      },
    });

    const renderedModules = screen.getByRole('list', { name: 'Gerenderte Module' });
    expect(within(renderedModules).getAllByRole('listitem')).toHaveLength(50);
    expect(screen.getAllByText('4.096+').length).toBeGreaterThan(0);
    expect(screen.getByText(/Weitere beobachtete Module sind nicht gerendert/u)).toBeTruthy();
    expect(screen.getByText(/4.096-Kanten-Grenze/u)).toBeTruthy();
  });
});

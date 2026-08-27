import { describe, expect, it } from 'vitest';
import { layoutAtlasNodes } from './map-atlas-layout';
import type { ProjectMapAtlasNodeV1 } from './project-map-atlas';

const node = (rank: number, volume: string, kind: ProjectMapAtlasNodeV1['kind'] = 'file') =>
  ({ kind, nodeId: String(rank).padStart(64, '0'), rank, volume }) as ProjectMapAtlasNodeV1;

describe('deterministic Atlas treemap', () => {
  it('returns identical rectangles and keeps extreme volumes bounded', () => {
    const nodes = [node(1, '1'), node(2, '1000000'), node(3, '2')];
    const first = layoutAtlasNodes(nodes, 900, 600);
    const second = layoutAtlasNodes([...nodes], 900, 600);
    expect([...first.byId]).toEqual([...second.byId]);
    const widths = [...first.byId.values()].map((rect) => rect.width);
    expect(Math.max(...widths) / Math.min(...widths)).toBeLessThan(9);
  });

  it('places unresolved boundaries in a dedicated right-edge dock', () => {
    const layout = layoutAtlasNodes([node(1, '4'), node(2, '1', 'boundary')], 1000, 500);
    expect(layout.byId.get(node(2, '1').nodeId)?.x).toBeGreaterThan(750);
  });
});

import { describe, expect, it } from 'vitest';
import { atlasRelationKey, layoutAtlasNodes, type AtlasRect } from './map-atlas-layout';
import type { ProjectMapAtlasNodeV1, ProjectMapAtlasRelationV1 } from './project-map-atlas';

const node = (rank: number, volume: string, kind: ProjectMapAtlasNodeV1['kind'] = 'file') =>
  ({
    kind,
    nodeId: String(rank).padStart(64, '0'),
    rank,
    volume,
  }) as ProjectMapAtlasNodeV1;

const relation = (
  source: ProjectMapAtlasNodeV1,
  target: ProjectMapAtlasNodeV1,
): ProjectMapAtlasRelationV1 =>
  ({
    relation: 'imports',
    sourceNodeId: source.nodeId,
    targetNodeId: target.nodeId,
  }) as ProjectMapAtlasRelationV1;

describe('deterministic relation-aware Atlas layout', () => {
  it('orders a directed dependency graph from left to right and remains deterministic', () => {
    const nodes = [node(1, '1'), node(2, '1000000'), node(3, '2')];
    const relations = [relation(nodes[0], nodes[1]), relation(nodes[1], nodes[2])];
    const first = layoutAtlasNodes(nodes, relations, 1_200, 640);
    const second = layoutAtlasNodes([...nodes].reverse(), [...relations].reverse(), 1_200, 640);

    expect([...first.byId]).toEqual([...second.byId]);
    expect([...first.routes]).toEqual([...second.routes]);
    expect(first.byId.get(nodes[0].nodeId)?.x).toBeLessThan(
      first.byId.get(nodes[1].nodeId)?.x ?? 0,
    );
    expect(first.byId.get(nodes[1].nodeId)?.x).toBeLessThan(
      first.byId.get(nodes[2].nodeId)?.x ?? 0,
    );
    const areas = [...first.byId.values()].map((rect) => rect.width * rect.height);
    expect(Math.max(...areas) / Math.min(...areas)).toBeLessThanOrEqual(2.3);
  });

  it('keeps cycles together without overlapping cards', () => {
    const nodes = [node(1, '4'), node(2, '3'), node(3, '2')];
    const relations = [
      relation(nodes[0], nodes[1]),
      relation(nodes[1], nodes[0]),
      relation(nodes[1], nodes[2]),
    ];
    const layout = layoutAtlasNodes(nodes, relations, 900, 540);
    const rectangles = [...layout.byId.values()];

    expect(rectangles).toHaveLength(3);
    for (let left = 0; left < rectangles.length; left += 1) {
      for (let right = left + 1; right < rectangles.length; right += 1) {
        expect(overlaps(rectangles[left], rectangles[right])).toBe(false);
      }
    }
  });

  it('grows a dense graph inside its viewport instead of shrinking nodes into tiles', () => {
    const nodes = Array.from({ length: 12 }, (_, index) => node(index + 1, '1'));
    const relations = nodes.slice(0, -1).map((source, index) => relation(source, nodes[index + 1]));
    const layout = layoutAtlasNodes(nodes, relations, 720, 520);

    expect(layout.width).toBeGreaterThan(720);
    for (const rect of layout.byId.values()) {
      expect(rect.width).toBeGreaterThanOrEqual(176);
      expect(rect.height).toBeGreaterThanOrEqual(88);
    }
  });

  it('places unconnected entities and unresolved boundaries in labeled edge bands', () => {
    const source = node(1, '4');
    const target = node(2, '2');
    const unconnected = node(3, '1');
    const boundary = node(4, '1', 'boundary');
    const layout = layoutAtlasNodes(
      [source, target, unconnected, boundary],
      [relation(source, target)],
      1_200,
      600,
    );

    expect(layout.bands.map((band) => band.kind)).toEqual(['unconnected', 'boundary']);
    expect(layout.byId.get(boundary.nodeId)?.x).toBeGreaterThan(
      layout.byId.get(unconnected.nodeId)?.x ?? 0,
    );
  });

  it('routes a branched graph through angular tracks without crossing unrelated cards', () => {
    const nodes = Array.from({ length: 6 }, (_, index) => node(index + 1, String(index + 1)));
    const relations = [
      relation(nodes[0], nodes[1]),
      relation(nodes[0], nodes[2]),
      relation(nodes[1], nodes[3]),
      relation(nodes[2], nodes[3]),
      relation(nodes[3], nodes[4]),
      relation(nodes[2], nodes[5]),
    ];
    const layout = layoutAtlasNodes(nodes, relations, 1_280, 720);

    for (const edge of relations) {
      const route = layout.routes.get(atlasRelationKey(edge));
      expect(route?.d).toMatch(/^M [0-9.]+ [0-9.]+(?: [HVL] [0-9.]+(?: [0-9.]+)?)+$/);
      expect(route?.d).not.toMatch(/[CQ]/);
      if (route === undefined) continue;
      for (const candidate of nodes) {
        if (candidate.nodeId === edge.sourceNodeId || candidate.nodeId === edge.targetNodeId)
          continue;
        const rect = layout.byId.get(candidate.nodeId);
        if (rect === undefined) continue;
        for (let index = 1; index < route.points.length; index += 1) {
          expect(segmentCrossesInterior(route.points[index - 1], route.points[index], rect)).toBe(
            false,
          );
        }
      }
    }
  });
});

function overlaps(left: AtlasRect, right: AtlasRect): boolean {
  return !(
    left.x + left.width <= right.x ||
    right.x + right.width <= left.x ||
    left.y + left.height <= right.y ||
    right.y + right.height <= left.y
  );
}

function segmentCrossesInterior(
  source: { x: number; y: number },
  target: { x: number; y: number },
  rect: AtlasRect,
): boolean {
  if (source.x === target.x) {
    return (
      source.x > rect.x &&
      source.x < rect.x + rect.width &&
      Math.max(source.y, target.y) > rect.y &&
      Math.min(source.y, target.y) < rect.y + rect.height
    );
  }
  if (source.y === target.y) {
    return (
      source.y > rect.y &&
      source.y < rect.y + rect.height &&
      Math.max(source.x, target.x) > rect.x &&
      Math.min(source.x, target.x) < rect.x + rect.width
    );
  }
  return true;
}

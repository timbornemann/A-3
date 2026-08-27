import type { ProjectMapAtlasNodeV1, ProjectMapAtlasRelationV1 } from './project-map-atlas';

export interface AtlasRect {
  height: number;
  width: number;
  x: number;
  y: number;
}

export interface AtlasBand {
  kind: 'boundary' | 'unconnected';
  label: string;
  width: number;
  x: number;
}

export interface AtlasLayout {
  bands: readonly AtlasBand[];
  byId: ReadonlyMap<string, AtlasRect>;
  height: number;
  width: number;
}

interface SizedNode {
  height: number;
  node: ProjectMapAtlasNodeV1;
  width: number;
}

interface LayoutColumn {
  group: 'boundary' | 'connected' | 'unconnected';
  nodes: readonly SizedNode[];
  width: number;
}

const SIDE_MARGIN = 48;
const TOP_MARGIN = 52;
const BOTTOM_MARGIN = 42;
const COLUMN_GAP = 104;
const GROUP_GAP = 152;
const ROW_GAP = 36;
const MIN_NODE_WIDTH = 176;
const MIN_NODE_HEIGHT = 88;
const MAX_AREA_SCALE = 2.25;

/**
 * Deterministic relation-aware Atlas layout.
 *
 * Directed architecture relations form stable left-to-right layers. Cycles remain together,
 * unresolved boundaries stay at the right edge, and dense scenes grow inside the scrollable
 * canvas instead of shrinking interactive nodes or covering the route layer with a treemap.
 */
export function layoutAtlasNodes(
  nodes: readonly ProjectMapAtlasNodeV1[],
  relations: readonly ProjectMapAtlasRelationV1[],
  width: number,
  height: number,
): AtlasLayout {
  const safeWidth = Math.max(320, Math.floor(width));
  const safeHeight = Math.max(280, Math.floor(height));
  const entities = nodes.filter((node) => node.kind !== 'boundary').sort(byRank);
  const boundaries = nodes.filter((node) => node.kind === 'boundary').sort(byRank);
  const entityIds = new Set(entities.map((node) => node.nodeId));
  const edges = relations
    .filter(
      (relation) => entityIds.has(relation.sourceNodeId) && entityIds.has(relation.targetNodeId),
    )
    .sort(byRelation);
  const degree = new Map(entities.map((node) => [node.nodeId, 0]));
  for (const edge of edges) {
    degree.set(edge.sourceNodeId, (degree.get(edge.sourceNodeId) ?? 0) + 1);
    degree.set(edge.targetNodeId, (degree.get(edge.targetNodeId) ?? 0) + 1);
  }

  const connected = entities.filter((node) => (degree.get(node.nodeId) ?? 0) > 0);
  const unconnected = entities.filter((node) => (degree.get(node.nodeId) ?? 0) === 0);
  const sizes = sizedNodes(nodes);
  const maxRows = Math.max(
    2,
    Math.floor((safeHeight - TOP_MARGIN - BOTTOM_MARGIN + ROW_GAP) / (MIN_NODE_HEIGHT + ROW_GAP)),
  );
  const columns: LayoutColumn[] = [];

  for (const layer of orderedLayers(connected, edges)) {
    appendColumns(columns, layer, 'connected', sizes, maxRows);
  }
  appendColumns(columns, unconnected, 'unconnected', sizes, maxRows);
  appendColumns(columns, boundaries, 'boundary', sizes, maxRows);

  if (columns.length === 0) {
    return { bands: [], byId: new Map(), height: safeHeight, width: safeWidth };
  }

  const baseGaps = columns
    .slice(1)
    .map((column, index) => (column.group === columns[index].group ? COLUMN_GAP : GROUP_GAP));
  const minimumWidth =
    SIDE_MARGIN * 2 +
    columns.reduce((sum, column) => sum + column.width, 0) +
    baseGaps.reduce((sum, gap) => sum + gap, 0);
  const worldWidth = Math.max(safeWidth, minimumWidth);
  const extraGap =
    columns.length > 1 ? Math.max(0, (worldWidth - minimumWidth) / (columns.length - 1)) : 0;
  const columnHeights = columns.map(columnHeight);
  const worldHeight = Math.max(
    safeHeight,
    TOP_MARGIN + Math.max(...columnHeights, 0) + BOTTOM_MARGIN,
  );
  const byId = new Map<string, AtlasRect>();
  const columnX: number[] = [];
  let x = columns.length === 1 ? (worldWidth - columns[0].width) / 2 : SIDE_MARGIN;

  columns.forEach((column, columnIndex) => {
    columnX.push(x);
    const contentHeight = columnHeights[columnIndex];
    let y = Math.max(TOP_MARGIN, (worldHeight - contentHeight) / 2);
    for (const item of column.nodes) {
      byId.set(item.node.nodeId, {
        height: item.height,
        width: item.width,
        x: x + (column.width - item.width) / 2,
        y,
      });
      y += item.height + ROW_GAP;
    }
    if (columnIndex < columns.length - 1) {
      x += column.width + baseGaps[columnIndex] + extraGap;
    }
  });

  return {
    bands: layoutBands(columns, columnX),
    byId,
    height: worldHeight,
    width: worldWidth,
  };
}

function orderedLayers(
  nodes: readonly ProjectMapAtlasNodeV1[],
  edges: readonly ProjectMapAtlasRelationV1[],
): ProjectMapAtlasNodeV1[][] {
  if (nodes.length === 0) return [];
  const nodeById = new Map(nodes.map((node) => [node.nodeId, node]));
  const components = stronglyConnectedComponents(nodes, edges);
  const componentByNode = new Map<string, number>();
  components.forEach((component, index) => {
    for (const nodeId of component) componentByNode.set(nodeId, index);
  });
  const outgoing = components.map(() => new Set<number>());
  const incoming = components.map(() => new Set<number>());
  for (const edge of edges) {
    const source = componentByNode.get(edge.sourceNodeId);
    const target = componentByNode.get(edge.targetNodeId);
    if (source === undefined || target === undefined || source === target) continue;
    outgoing[source].add(target);
    incoming[target].add(source);
  }
  const componentRank = components.map((component) =>
    Math.min(...component.map((nodeId) => nodeById.get(nodeId)?.rank ?? Number.MAX_SAFE_INTEGER)),
  );
  const layer = components.map(() => 0);
  const remainingIncoming = incoming.map((parents) => parents.size);
  const ready = components
    .map((_, index) => index)
    .filter((index) => remainingIncoming[index] === 0)
    .sort((left, right) => componentRank[left] - componentRank[right]);
  while (ready.length > 0) {
    const source = ready.shift();
    if (source === undefined) break;
    for (const target of [...outgoing[source]].sort(
      (left, right) => componentRank[left] - componentRank[right],
    )) {
      layer[target] = Math.max(layer[target], layer[source] + 1);
      remainingIncoming[target] -= 1;
      if (remainingIncoming[target] === 0) {
        ready.push(target);
        ready.sort((left, right) => componentRank[left] - componentRank[right]);
      }
    }
  }

  const result: ProjectMapAtlasNodeV1[][] = [];
  components.forEach((component, componentIndex) => {
    const targetLayer = (result[layer[componentIndex]] ??= []);
    for (const nodeId of component) {
      const node = nodeById.get(nodeId);
      if (node !== undefined) targetLayer.push(node);
    }
  });
  for (const nodesInLayer of result) nodesInLayer.sort(byRank);
  minimizeCrossings(result, edges);
  return result;
}

function stronglyConnectedComponents(
  nodes: readonly ProjectMapAtlasNodeV1[],
  edges: readonly ProjectMapAtlasRelationV1[],
): string[][] {
  const rankById = new Map(nodes.map((node) => [node.nodeId, node.rank]));
  const adjacent = new Map(nodes.map((node) => [node.nodeId, [] as string[]]));
  for (const edge of edges) adjacent.get(edge.sourceNodeId)?.push(edge.targetNodeId);
  for (const targets of adjacent.values()) {
    targets.sort(
      (left, right) =>
        (rankById.get(left) ?? Number.MAX_SAFE_INTEGER) -
          (rankById.get(right) ?? Number.MAX_SAFE_INTEGER) || left.localeCompare(right),
    );
  }
  const discovered = new Map<string, number>();
  const lowLink = new Map<string, number>();
  const stack: string[] = [];
  const onStack = new Set<string>();
  const components: string[][] = [];
  let nextIndex = 0;

  function visit(nodeId: string): void {
    discovered.set(nodeId, nextIndex);
    lowLink.set(nodeId, nextIndex);
    nextIndex += 1;
    stack.push(nodeId);
    onStack.add(nodeId);
    for (const targetId of adjacent.get(nodeId) ?? []) {
      if (!discovered.has(targetId)) {
        visit(targetId);
        lowLink.set(nodeId, Math.min(lowLink.get(nodeId) ?? 0, lowLink.get(targetId) ?? 0));
      } else if (onStack.has(targetId)) {
        lowLink.set(nodeId, Math.min(lowLink.get(nodeId) ?? 0, discovered.get(targetId) ?? 0));
      }
    }
    if (lowLink.get(nodeId) !== discovered.get(nodeId)) return;
    const component: string[] = [];
    while (stack.length > 0) {
      const member = stack.pop();
      if (member === undefined) break;
      onStack.delete(member);
      component.push(member);
      if (member === nodeId) break;
    }
    component.sort(
      (left, right) =>
        (rankById.get(left) ?? Number.MAX_SAFE_INTEGER) -
          (rankById.get(right) ?? Number.MAX_SAFE_INTEGER) || left.localeCompare(right),
    );
    components.push(component);
  }

  for (const node of nodes) {
    if (!discovered.has(node.nodeId)) visit(node.nodeId);
  }
  return components;
}

function minimizeCrossings(
  layers: ProjectMapAtlasNodeV1[][],
  edges: readonly ProjectMapAtlasRelationV1[],
): void {
  const neighbors = new Map<string, Set<string>>();
  for (const edge of edges) {
    (neighbors.get(edge.sourceNodeId) ?? createNeighborSet(neighbors, edge.sourceNodeId)).add(
      edge.targetNodeId,
    );
    (neighbors.get(edge.targetNodeId) ?? createNeighborSet(neighbors, edge.targetNodeId)).add(
      edge.sourceNodeId,
    );
  }
  for (let pass = 0; pass < 4; pass += 1) {
    const forward = pass % 2 === 0;
    const indexes = forward
      ? layers.map((_, index) => index).slice(1)
      : layers
          .map((_, index) => index)
          .slice(0, -1)
          .reverse();
    for (const layerIndex of indexes) {
      const reference = layers[layerIndex + (forward ? -1 : 1)];
      const order = new Map(reference.map((node, index) => [node.nodeId, index]));
      layers[layerIndex].sort((left, right) => {
        const leftCenter = barycenter(left.nodeId, neighbors, order);
        const rightCenter = barycenter(right.nodeId, neighbors, order);
        if (leftCenter === null && rightCenter === null) return byRank(left, right);
        if (leftCenter === null) return 1;
        if (rightCenter === null) return -1;
        return leftCenter - rightCenter || byRank(left, right);
      });
    }
  }
}

function createNeighborSet(store: Map<string, Set<string>>, nodeId: string): Set<string> {
  const value = new Set<string>();
  store.set(nodeId, value);
  return value;
}

function barycenter(
  nodeId: string,
  neighbors: ReadonlyMap<string, ReadonlySet<string>>,
  order: ReadonlyMap<string, number>,
): number | null {
  const positions = [...(neighbors.get(nodeId) ?? [])]
    .map((neighbor) => order.get(neighbor))
    .filter((position): position is number => position !== undefined);
  if (positions.length === 0) return null;
  return positions.reduce((sum, position) => sum + position, 0) / positions.length;
}

function sizedNodes(nodes: readonly ProjectMapAtlasNodeV1[]): Map<string, SizedNode> {
  if (nodes.length === 0) return new Map();
  const volumes = nodes.map((node) => Math.max(1, safeVolume(node.volume)));
  const minimum = Math.min(...volumes);
  const clamped = volumes.map((volume) => Math.min(volume, minimum * 8));
  const maximum = Math.max(...clamped, minimum);
  return new Map(
    nodes.map((node, index) => {
      const normalized = maximum === minimum ? 0 : (clamped[index] - minimum) / (maximum - minimum);
      const areaScale = 1 + normalized * (MAX_AREA_SCALE - 1);
      const linearScale = Math.sqrt(areaScale);
      const boundaryScale = node.kind === 'boundary' ? 0.9 : 1;
      return [
        node.nodeId,
        {
          height: Math.round(MIN_NODE_HEIGHT * linearScale * boundaryScale),
          node,
          width: Math.round(MIN_NODE_WIDTH * linearScale * boundaryScale),
        },
      ];
    }),
  );
}

function appendColumns(
  columns: LayoutColumn[],
  nodes: readonly ProjectMapAtlasNodeV1[],
  group: LayoutColumn['group'],
  sizes: ReadonlyMap<string, SizedNode>,
  maxRows: number,
): void {
  for (let start = 0; start < nodes.length; start += maxRows) {
    const chunk = nodes
      .slice(start, start + maxRows)
      .map((node) => sizes.get(node.nodeId))
      .filter((item): item is SizedNode => item !== undefined);
    if (chunk.length === 0) continue;
    columns.push({
      group,
      nodes: chunk,
      width: Math.max(...chunk.map((item) => item.width)),
    });
  }
}

function columnHeight(column: LayoutColumn): number {
  return (
    column.nodes.reduce((sum, item) => sum + item.height, 0) +
    Math.max(0, column.nodes.length - 1) * ROW_GAP
  );
}

function layoutBands(columns: readonly LayoutColumn[], columnX: readonly number[]): AtlasBand[] {
  const bands: AtlasBand[] = [];
  for (const group of ['unconnected', 'boundary'] as const) {
    const indexes = columns
      .map((column, index) => (column.group === group ? index : -1))
      .filter((index) => index >= 0);
    if (indexes.length === 0) continue;
    const first = indexes[0];
    const last = indexes[indexes.length - 1];
    bands.push({
      kind: group,
      label: group === 'boundary' ? 'Externe und ungelöste Ziele' : 'Ohne sichtbare Route',
      width: columnX[last] + columns[last].width - columnX[first],
      x: columnX[first],
    });
  }
  return bands;
}

function safeVolume(value: string): number {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : Number.MAX_SAFE_INTEGER;
}

function byRank(left: ProjectMapAtlasNodeV1, right: ProjectMapAtlasNodeV1): number {
  return left.rank - right.rank || left.nodeId.localeCompare(right.nodeId);
}

function byRelation(left: ProjectMapAtlasRelationV1, right: ProjectMapAtlasRelationV1): number {
  return (
    left.sourceNodeId.localeCompare(right.sourceNodeId) ||
    left.targetNodeId.localeCompare(right.targetNodeId) ||
    left.relation.localeCompare(right.relation)
  );
}

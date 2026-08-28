import type { ProjectMapAtlasNodeV1, ProjectMapAtlasRelationV1 } from './project-map-atlas';

export interface AtlasRect {
  height: number;
  width: number;
  x: number;
  y: number;
}

export interface AtlasPoint {
  x: number;
  y: number;
}

export interface AtlasRoute {
  d: string;
  labelX: number;
  labelY: number;
  points: readonly AtlasPoint[];
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
  routes: ReadonlyMap<string, AtlasRoute>;
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

type RouteSide = 'bottom' | 'left' | 'right' | 'top';

interface RouteEndpoint {
  grid: GridPoint;
  prefix: readonly AtlasPoint[];
}

interface EndpointPlan {
  sourceSide: RouteSide;
  targetSide: RouteSide;
}

interface GridPoint {
  x: number;
  y: number;
}

interface RoutingChannels {
  horizontal: readonly number[];
  maximumX: number;
  maximumY: number;
  vertical: readonly number[];
}

const SIDE_MARGIN = 64;
const TOP_MARGIN = 64;
const BOTTOM_MARGIN = 64;
const COLUMN_GAP = 176;
const GROUP_GAP = 224;
const ROW_GAP = 52;
const NODE_WANDER = 72;
const MIN_NODE_WIDTH = 176;
const MIN_NODE_HEIGHT = 88;
const MAX_AREA_SCALE = 2.25;
const ROUTE_GRID = 16;
const ROUTE_CLEARANCE = 16;

/**
 * Deterministic relation-aware Atlas layout.
 *
 * The graph is layered without force physics, then relaxed vertically around its neighbours.
 * Routing is part of the same result: a bounded set of Manhattan corridors keeps routes outside
 * card interiors and strongly penalises occupied tracks and crossings. The result therefore
 * behaves like a stable transit diagram instead of a card grid with unrelated curves drawn over it.
 */
export function layoutAtlasNodes(
  nodes: readonly ProjectMapAtlasNodeV1[],
  relations: readonly ProjectMapAtlasRelationV1[],
  width: number,
  height: number,
  routedRelations: readonly ProjectMapAtlasRelationV1[] = relations,
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
  const layers = orderedLayers(connected, edges);
  const viewportRows = Math.max(
    3,
    Math.floor((safeHeight - TOP_MARGIN - BOTTOM_MARGIN + ROW_GAP) / (MIN_NODE_HEIGHT + ROW_GAP)),
  );
  const largestLayer = Math.max(...layers.map((layer) => layer.length), 0);
  const maxRows = Math.max(viewportRows, Math.min(8, largestLayer));
  const columns: LayoutColumn[] = [];

  for (const layer of layers) appendColumns(columns, layer, 'connected', sizes, maxRows);
  appendColumns(columns, unconnected, 'unconnected', sizes, maxRows);
  appendColumns(columns, boundaries, 'boundary', sizes, maxRows);

  if (columns.length === 0) {
    return {
      bands: [],
      byId: new Map(),
      height: safeHeight,
      routes: new Map(),
      width: safeWidth,
    };
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
    const stagger = columns.length === 1 ? 0 : ((columnIndex * 37) % 73) - 36;
    let y = clamp(
      (worldHeight - contentHeight) / 2 + stagger,
      TOP_MARGIN,
      worldHeight - BOTTOM_MARGIN - contentHeight,
    );
    for (const item of column.nodes) {
      const wander =
        column.group === 'connected' && column.nodes.length > 1
          ? stableWander(item.node.rank, columnIndex, NODE_WANDER / 2)
          : 0;
      byId.set(item.node.nodeId, {
        height: item.height,
        width: item.width,
        x: x + (column.width - item.width) / 2 + wander,
        y,
      });
      y += item.height + ROW_GAP;
    }
    if (columnIndex < columns.length - 1) {
      x += column.width + baseGaps[columnIndex] + extraGap;
    }
  });

  relaxColumns(columns, byId, edges, worldHeight);
  return {
    bands: layoutBands(columns, columnX),
    byId,
    height: worldHeight,
    routes: routeRelations(routedRelations, byId, worldWidth, worldHeight),
    width: worldWidth,
  };
}

export function atlasRelationKey(relation: ProjectMapAtlasRelationV1): string {
  return `${relation.sourceNodeId}:${relation.targetNodeId}:${relation.relation}`;
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
  const neighbors = neighborIds(edges);
  for (let pass = 0; pass < 6; pass += 1) {
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

function relaxColumns(
  columns: readonly LayoutColumn[],
  byId: Map<string, AtlasRect>,
  edges: readonly ProjectMapAtlasRelationV1[],
  worldHeight: number,
): void {
  const neighbors = neighborIds(edges);
  for (let pass = 0; pass < 8; pass += 1) {
    const indexes = columns.map((_, index) => index);
    if (pass % 2 === 1) indexes.reverse();
    for (const columnIndex of indexes) {
      const column = columns[columnIndex];
      const desired = column.nodes.map((item) => {
        const neighborCenters = [...(neighbors.get(item.node.nodeId) ?? [])]
          .map((nodeId) => byId.get(nodeId))
          .filter((rect): rect is AtlasRect => rect !== undefined)
          .map((rect) => rect.y + rect.height / 2)
          .sort((left, right) => left - right);
        const current = byId.get(item.node.nodeId);
        if (current === undefined || neighborCenters.length === 0) {
          return current === undefined ? worldHeight / 2 : current.y + current.height / 2;
        }
        const middle = Math.floor(neighborCenters.length / 2);
        const center =
          neighborCenters.length % 2 === 0
            ? (neighborCenters[middle - 1] + neighborCenters[middle]) / 2
            : neighborCenters[middle];
        return center + stableWander(item.node.rank, columnIndex, 22);
      });
      placeRelaxedColumn(column, desired, byId, worldHeight);
    }
  }
}

function placeRelaxedColumn(
  column: LayoutColumn,
  desiredCenters: readonly number[],
  byId: Map<string, AtlasRect>,
  worldHeight: number,
): void {
  const top = TOP_MARGIN;
  const bottom = worldHeight - BOTTOM_MARGIN;
  const positions: number[] = [];
  let cursor = top;
  for (let index = 0; index < column.nodes.length; index += 1) {
    const item = column.nodes[index];
    const position = Math.max(cursor, desiredCenters[index] - item.height / 2);
    positions.push(position);
    cursor = position + item.height + ROW_GAP;
  }
  const overflow = cursor - ROW_GAP - bottom;
  if (overflow > 0) {
    for (let index = 0; index < positions.length; index += 1) positions[index] -= overflow;
  }
  for (let index = positions.length - 2; index >= 0; index -= 1) {
    positions[index] = Math.min(
      positions[index],
      positions[index + 1] - ROW_GAP - column.nodes[index].height,
    );
  }
  const underflow = top - (positions[0] ?? top);
  if (underflow > 0) {
    for (let index = 0; index < positions.length; index += 1) positions[index] += underflow;
  }
  column.nodes.forEach((item, index) => {
    const current = byId.get(item.node.nodeId);
    if (current !== undefined) byId.set(item.node.nodeId, { ...current, y: positions[index] });
  });
}

function routeRelations(
  relations: readonly ProjectMapAtlasRelationV1[],
  byId: ReadonlyMap<string, AtlasRect>,
  width: number,
  height: number,
): ReadonlyMap<string, AtlasRoute> {
  const ordered = relations
    .filter((relation) => byId.has(relation.sourceNodeId) && byId.has(relation.targetNodeId))
    .sort(routePriority);
  const endpointPlans = new Map<string, EndpointPlan>();
  const portGroups = new Map<string, string[]>();
  for (const relation of ordered) {
    const source = byId.get(relation.sourceNodeId);
    const target = byId.get(relation.targetNodeId);
    if (source === undefined || target === undefined) continue;
    const plan = endpointSides(source, target);
    const key = atlasRelationKey(relation);
    endpointPlans.set(key, plan);
    addPortGroup(portGroups, `${relation.sourceNodeId}:${plan.sourceSide}`, key);
    addPortGroup(portGroups, `${relation.targetNodeId}:${plan.targetSide}`, key);
  }
  for (const keys of portGroups.values()) keys.sort();

  const blocked = blockedGridPoints(byId, width, height);
  const channels = routingChannels(blocked, width, height);
  const usedSegments = new Map<string, number>();
  const usedDirections = new Map<string, number>();
  const routes = new Map<string, AtlasRoute>();
  ordered.forEach((relation, relationIndex) => {
    const key = atlasRelationKey(relation);
    const plan = endpointPlans.get(key);
    const sourceRect = byId.get(relation.sourceNodeId);
    const targetRect = byId.get(relation.targetNodeId);
    if (plan === undefined || sourceRect === undefined || targetRect === undefined) return;
    const source = routeEndpoint(
      sourceRect,
      plan.sourceSide,
      portIndex(portGroups, relation.sourceNodeId, plan.sourceSide, key),
      portCount(portGroups, relation.sourceNodeId, plan.sourceSide),
    );
    const target = routeEndpoint(
      targetRect,
      plan.targetSide,
      portIndex(portGroups, relation.targetNodeId, plan.targetSide, key),
      portCount(portGroups, relation.targetNodeId, plan.targetSide),
    );
    const gridPath =
      findMetroRoute(source.grid, target.grid, blocked, usedSegments, usedDirections, channels) ??
      [];
    let points = simplifyPoints([
      ...source.prefix,
      ...gridPath.map(fromGrid),
      ...[...target.prefix].reverse(),
    ]);
    if (gridPath.length === 0) points = fallbackRoute(source, target, relationIndex, height);
    reserveRoute(points, usedSegments, usedDirections);
    routes.set(key, routeFromPoints(points));
  });
  return routes;
}

function findMetroRoute(
  start: GridPoint,
  target: GridPoint,
  blocked: ReadonlySet<string>,
  usedSegments: ReadonlyMap<string, number>,
  usedDirections: ReadonlyMap<string, number>,
  channels: RoutingChannels,
): GridPoint[] | null {
  const { maximumX, maximumY } = channels;
  const xTracks = localTracks(maximumX, (start.x + target.x) / 2);
  const yTracks = localTracks(maximumY, (start.y + target.y) / 2);
  let bestPath: GridPoint[] | null = null;
  let bestScore = Number.POSITIVE_INFINITY;
  let validCandidateCount = 0;
  const consider = (candidate: readonly GridPoint[]) => {
    const path = simplifyGridPoints(candidate);
    const score = metroRouteScore(path, blocked, usedSegments, usedDirections);
    if (score === null) return false;
    validCandidateCount += 1;
    if (score < bestScore) {
      bestPath = path;
      bestScore = score;
    }
    return score === uncongestedRouteScore(path) || validCandidateCount >= 4;
  };

  if (consider([start, target])) return bestPath;
  for (const x of xTracks) {
    if (consider([start, { x, y: start.y }, { x, y: target.y }, target])) return bestPath;
  }
  for (const y of yTracks) {
    if (consider([start, { x: start.x, y }, { x: target.x, y }, target])) return bestPath;
  }
  const sourceTracks = [...channels.vertical]
    .sort((left, right) => Math.abs(left - start.x) - Math.abs(right - start.x) || left - right)
    .slice(0, 3);
  const targetTracks = [...channels.vertical]
    .sort((left, right) => Math.abs(left - target.x) - Math.abs(right - target.x) || left - right)
    .slice(0, 3);
  const horizontalTracks = [...channels.horizontal]
    .sort(
      (left, right) =>
        Math.abs(left - (start.y + target.y) / 2) - Math.abs(right - (start.y + target.y) / 2) ||
        left - right,
    )
    .slice(0, 5);
  for (const y of horizontalTracks) {
    for (const sourceX of sourceTracks) {
      for (const targetX of targetTracks) {
        if (
          consider([
            start,
            { x: sourceX, y: start.y },
            { x: sourceX, y },
            { x: targetX, y },
            { x: targetX, y: target.y },
            target,
          ])
        )
          return bestPath;
      }
    }
  }
  return bestPath;
}

function uncongestedRouteScore(path: readonly GridPoint[]): number {
  let score = Math.max(0, path.length - 2) * 22;
  for (let index = 1; index < path.length; index += 1) {
    score += manhattan(path[index - 1], path[index]) * 10;
  }
  return score;
}

function isFreeVerticalTrack(x: number, maximumY: number, blocked: ReadonlySet<string>): boolean {
  for (let y = 1; y <= maximumY; y += 1) {
    if (blocked.has(`${x}:${y}`)) return false;
  }
  return true;
}

function isFreeHorizontalTrack(y: number, maximumX: number, blocked: ReadonlySet<string>): boolean {
  for (let x = 1; x <= maximumX; x += 1) {
    if (blocked.has(`${x}:${y}`)) return false;
  }
  return true;
}

function routingChannels(
  blocked: ReadonlySet<string>,
  width: number,
  height: number,
): RoutingChannels {
  const maximumX = Math.max(2, Math.floor(width / ROUTE_GRID) - 1);
  const maximumY = Math.max(2, Math.floor(height / ROUTE_GRID) - 1);
  const vertical: number[] = [];
  const horizontal: number[] = [];
  for (let x = 1; x <= maximumX; x += 1) {
    if (isFreeVerticalTrack(x, maximumY, blocked)) vertical.push(x);
  }
  for (let y = 1; y <= maximumY; y += 1) {
    if (isFreeHorizontalTrack(y, maximumX, blocked)) horizontal.push(y);
  }
  return { horizontal, maximumX, maximumY, vertical };
}

function localTracks(maximum: number, center: number): number[] {
  const middle = clamp(Math.round(center), 1, maximum);
  const tracks = new Set<number>([1, maximum, middle]);
  for (let offset = 1; offset <= 8; offset += 1) {
    tracks.add(clamp(middle - offset, 1, maximum));
    tracks.add(clamp(middle + offset, 1, maximum));
  }
  return [...tracks];
}

function metroRouteScore(
  path: readonly GridPoint[],
  blocked: ReadonlySet<string>,
  usedSegments: ReadonlyMap<string, number>,
  usedDirections: ReadonlyMap<string, number>,
): number | null {
  let score = Math.max(0, path.length - 2) * 22;
  for (let index = 1; index < path.length; index += 1) {
    const source = path[index - 1];
    const target = path[index];
    if (source.x !== target.x && source.y !== target.y) return null;
    const stepX = Math.sign(target.x - source.x);
    const stepY = Math.sign(target.y - source.y);
    const orientation = stepX === 0 ? 2 : 1;
    let current = source;
    while (current.x !== target.x || current.y !== target.y) {
      const next = { x: current.x + stepX, y: current.y + stepY };
      const isEndpoint =
        (next.x === path[0].x && next.y === path[0].y) ||
        (next.x === path.at(-1)?.x && next.y === path.at(-1)?.y);
      if (!isEndpoint && blocked.has(gridKey(next))) return null;
      const existingDirections = usedDirections.get(gridKey(next)) ?? 0;
      const crossing = existingDirections !== 0 && (existingDirections & orientation) === 0;
      score +=
        10 +
        (usedSegments.get(gridSegmentKey(current, next)) ?? 0) * 1_000 +
        (existingDirections === 0 ? 0 : 18) +
        (crossing ? 240 : 0);
      current = next;
    }
  }
  return score;
}

function simplifyGridPoints(points: readonly GridPoint[]): GridPoint[] {
  const result: GridPoint[] = [];
  for (const point of points) {
    const previous = result.at(-1);
    if (previous?.x === point.x && previous.y === point.y) continue;
    result.push(point);
    while (result.length >= 3) {
      const first = result[result.length - 3];
      const middle = result[result.length - 2];
      const last = result[result.length - 1];
      if (
        (first.x === middle.x && middle.x === last.x) ||
        (first.y === middle.y && middle.y === last.y)
      ) {
        result.splice(result.length - 2, 1);
      } else break;
    }
  }
  return result;
}

function endpointSides(source: AtlasRect, target: AtlasRect): EndpointPlan {
  const sourceCenter = center(source);
  const targetCenter = center(target);
  const horizontal =
    Math.abs(targetCenter.x - sourceCenter.x) >= Math.abs(targetCenter.y - sourceCenter.y) * 0.72;
  if (horizontal) {
    const forward = targetCenter.x >= sourceCenter.x;
    return {
      sourceSide: forward ? 'right' : 'left',
      targetSide: forward ? 'left' : 'right',
    };
  }
  const downward = targetCenter.y >= sourceCenter.y;
  return {
    sourceSide: downward ? 'bottom' : 'top',
    targetSide: downward ? 'top' : 'bottom',
  };
}

function routeEndpoint(
  rect: AtlasRect,
  side: RouteSide,
  index: number,
  count: number,
): RouteEndpoint {
  const padding = 14;
  const horizontalSide = side === 'left' || side === 'right';
  const length = horizontalSide ? rect.height : rect.width;
  const offset = padding + ((length - padding * 2) * (index + 1)) / (count + 1);
  const port =
    side === 'left'
      ? { x: rect.x, y: rect.y + offset }
      : side === 'right'
        ? { x: rect.x + rect.width, y: rect.y + offset }
        : side === 'top'
          ? { x: rect.x + offset, y: rect.y }
          : { x: rect.x + offset, y: rect.y + rect.height };
  const escape =
    side === 'left'
      ? { x: rect.x - ROUTE_CLEARANCE, y: port.y }
      : side === 'right'
        ? { x: rect.x + rect.width + ROUTE_CLEARANCE, y: port.y }
        : side === 'top'
          ? { x: port.x, y: rect.y - ROUTE_CLEARANCE }
          : { x: port.x, y: rect.y + rect.height + ROUTE_CLEARANCE };
  const grid = {
    x:
      side === 'left'
        ? Math.floor(escape.x / ROUTE_GRID)
        : side === 'right'
          ? Math.ceil(escape.x / ROUTE_GRID)
          : Math.round(escape.x / ROUTE_GRID),
    y:
      side === 'top'
        ? Math.floor(escape.y / ROUTE_GRID)
        : side === 'bottom'
          ? Math.ceil(escape.y / ROUTE_GRID)
          : Math.round(escape.y / ROUTE_GRID),
  };
  const snapped = fromGrid(grid);
  const elbow = horizontalSide ? { x: snapped.x, y: port.y } : { x: port.x, y: snapped.y };
  return { grid, prefix: simplifyPoints([port, elbow, snapped]) };
}

function blockedGridPoints(
  byId: ReadonlyMap<string, AtlasRect>,
  width: number,
  height: number,
): ReadonlySet<string> {
  const blocked = new Set<string>();
  const maximumX = Math.floor(width / ROUTE_GRID);
  const maximumY = Math.floor(height / ROUTE_GRID);
  for (const rect of byId.values()) {
    const left = Math.max(0, Math.floor((rect.x - ROUTE_CLEARANCE / 2) / ROUTE_GRID));
    const right = Math.min(
      maximumX,
      Math.ceil((rect.x + rect.width + ROUTE_CLEARANCE / 2) / ROUTE_GRID),
    );
    const top = Math.max(0, Math.floor((rect.y - ROUTE_CLEARANCE / 2) / ROUTE_GRID));
    const bottom = Math.min(
      maximumY,
      Math.ceil((rect.y + rect.height + ROUTE_CLEARANCE / 2) / ROUTE_GRID),
    );
    for (let x = left; x <= right; x += 1) {
      for (let y = top; y <= bottom; y += 1) blocked.add(`${x}:${y}`);
    }
  }
  return blocked;
}

function fallbackRoute(
  source: RouteEndpoint,
  target: RouteEndpoint,
  index: number,
  height: number,
): AtlasPoint[] {
  const upper = index % 2 === 0;
  const gutter = upper ? 24 + (index % 5) * 6 : height - 24 - (index % 5) * 6;
  const sourceGrid = fromGrid(source.grid);
  const targetGrid = fromGrid(target.grid);
  return simplifyPoints([
    ...source.prefix,
    sourceGrid,
    { x: sourceGrid.x, y: gutter },
    { x: targetGrid.x, y: gutter },
    targetGrid,
    ...[...target.prefix].reverse(),
  ]);
}

function routeFromPoints(points: readonly AtlasPoint[]): AtlasRoute {
  let d = `M ${round(points[0].x)} ${round(points[0].y)}`;
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const point = points[index];
    if (previous.y === point.y) d += ` H ${round(point.x)}`;
    else if (previous.x === point.x) d += ` V ${round(point.y)}`;
    else d += ` L ${round(point.x)} ${round(point.y)}`;
  }
  let longest = { length: -1, source: points[0], target: points[1] ?? points[0] };
  for (let index = 1; index < points.length; index += 1) {
    const source = points[index - 1];
    const target = points[index];
    const length = Math.abs(target.x - source.x) + Math.abs(target.y - source.y);
    if (length > longest.length) longest = { length, source, target };
  }
  return {
    d,
    labelX: (longest.source.x + longest.target.x) / 2,
    labelY: (longest.source.y + longest.target.y) / 2 - 7,
    points,
  };
}

function reserveRoute(
  points: readonly AtlasPoint[],
  segments: Map<string, number>,
  directions: Map<string, number>,
): void {
  for (let index = 1; index < points.length; index += 1) {
    const source = toGrid(points[index - 1]);
    const target = toGrid(points[index]);
    if (source.x !== target.x && source.y !== target.y) continue;
    const stepX = Math.sign(target.x - source.x);
    const stepY = Math.sign(target.y - source.y);
    const orientation = stepX === 0 ? 2 : 1;
    let current = source;
    while (current.x !== target.x || current.y !== target.y) {
      const next = { x: current.x + stepX, y: current.y + stepY };
      const segment = gridSegmentKey(current, next);
      segments.set(segment, (segments.get(segment) ?? 0) + 1);
      const point = gridKey(next);
      directions.set(point, (directions.get(point) ?? 0) | orientation);
      current = next;
    }
  }
}

function simplifyPoints(points: readonly AtlasPoint[]): AtlasPoint[] {
  const unique: AtlasPoint[] = [];
  for (const point of points) {
    const previous = unique.at(-1);
    if (previous?.x === point.x && previous.y === point.y) continue;
    unique.push(point);
    while (unique.length >= 3) {
      const first = unique[unique.length - 3];
      const middle = unique[unique.length - 2];
      const last = unique[unique.length - 1];
      if (
        (first.x === middle.x && middle.x === last.x) ||
        (first.y === middle.y && middle.y === last.y)
      ) {
        unique.splice(unique.length - 2, 1);
      } else break;
    }
  }
  return unique;
}

function neighborIds(
  edges: readonly ProjectMapAtlasRelationV1[],
): ReadonlyMap<string, ReadonlySet<string>> {
  const neighbors = new Map<string, Set<string>>();
  for (const edge of edges) {
    (neighbors.get(edge.sourceNodeId) ?? createNeighborSet(neighbors, edge.sourceNodeId)).add(
      edge.targetNodeId,
    );
    (neighbors.get(edge.targetNodeId) ?? createNeighborSet(neighbors, edge.targetNodeId)).add(
      edge.sourceNodeId,
    );
  }
  return neighbors;
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
      width:
        Math.max(...chunk.map((item) => item.width)) +
        (group === 'connected' && chunk.length > 1 ? NODE_WANDER : 0),
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

function addPortGroup(groups: Map<string, string[]>, key: string, relationKey: string): void {
  const group = groups.get(key) ?? [];
  group.push(relationKey);
  groups.set(key, group);
}

function portIndex(
  groups: ReadonlyMap<string, readonly string[]>,
  nodeId: string,
  side: RouteSide,
  relationKey: string,
): number {
  return Math.max(0, groups.get(`${nodeId}:${side}`)?.indexOf(relationKey) ?? 0);
}

function portCount(
  groups: ReadonlyMap<string, readonly string[]>,
  nodeId: string,
  side: RouteSide,
): number {
  return Math.max(1, groups.get(`${nodeId}:${side}`)?.length ?? 1);
}

function center(rect: AtlasRect): AtlasPoint {
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

function safeVolume(value: string): number {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : Number.MAX_SAFE_INTEGER;
}

function byRank(left: ProjectMapAtlasNodeV1, right: ProjectMapAtlasNodeV1): number {
  return left.rank - right.rank || left.nodeId.localeCompare(right.nodeId);
}

function byRelation(left: ProjectMapAtlasRelationV1, right: ProjectMapAtlasRelationV1): number {
  return atlasRelationKey(left).localeCompare(atlasRelationKey(right));
}

function routePriority(left: ProjectMapAtlasRelationV1, right: ProjectMapAtlasRelationV1): number {
  return (
    Number(left.uncertainty != null) - Number(right.uncertainty != null) ||
    compareDecimalCounts(right.evidenceCount ?? '0', left.evidenceCount ?? '0') ||
    right.confidenceBasisPoints - left.confidenceBasisPoints ||
    byRelation(left, right)
  );
}

function compareDecimalCounts(left: string, right: string): number {
  return left.length - right.length || left.localeCompare(right);
}

function gridKey(point: GridPoint): string {
  return `${point.x}:${point.y}`;
}

function gridSegmentKey(source: GridPoint, target: GridPoint): string {
  return source.x < target.x || (source.x === target.x && source.y < target.y)
    ? `${source.x}:${source.y}-${target.x}:${target.y}`
    : `${target.x}:${target.y}-${source.x}:${source.y}`;
}

function toGrid(point: AtlasPoint): GridPoint {
  return { x: Math.round(point.x / ROUTE_GRID), y: Math.round(point.y / ROUTE_GRID) };
}

function fromGrid(point: GridPoint): AtlasPoint {
  return { x: point.x * ROUTE_GRID, y: point.y * ROUTE_GRID };
}

function manhattan(left: GridPoint, right: GridPoint): number {
  return Math.abs(left.x - right.x) + Math.abs(left.y - right.y);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

function stableWander(rank: number, column: number, maximum: number): number {
  const unit = ((rank * 29 + column * 17) % 101) / 100;
  return (unit * 2 - 1) * maximum;
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

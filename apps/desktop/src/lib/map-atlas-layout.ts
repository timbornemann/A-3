import type { ProjectMapAtlasNodeV1 } from './project-map-atlas';

export interface AtlasRect {
  height: number;
  width: number;
  x: number;
  y: number;
}

export interface AtlasLayout {
  byId: ReadonlyMap<string, AtlasRect>;
  height: number;
  width: number;
}

const GAP = 8;
const EDGE_DOCK_RATIO = 0.18;

/** Deterministic binary treemap with an optional right-edge dock for boundary stubs. */
export function layoutAtlasNodes(
  nodes: readonly ProjectMapAtlasNodeV1[],
  width: number,
  height: number,
): AtlasLayout {
  const safeWidth = Math.max(1, Math.floor(width));
  const safeHeight = Math.max(1, Math.floor(height));
  const entities = nodes.filter((node) => node.kind !== 'boundary').sort(byRank);
  const boundaries = nodes.filter((node) => node.kind === 'boundary').sort(byRank);
  const byId = new Map<string, AtlasRect>();
  const dockWidth = boundaries.length === 0 ? 0 : Math.max(144, safeWidth * EDGE_DOCK_RATIO);
  const entityWidth = Math.max(1, safeWidth - dockWidth - (dockWidth > 0 ? GAP : 0));
  partition(entities, { height: safeHeight, width: entityWidth, x: 0, y: 0 }, byId);
  if (boundaries.length > 0) {
    partition(
      boundaries,
      { height: safeHeight, width: dockWidth, x: entityWidth + GAP, y: 0 },
      byId,
      true,
    );
  }
  return { byId, height: safeHeight, width: safeWidth };
}

function partition(
  nodes: readonly ProjectMapAtlasNodeV1[],
  rect: AtlasRect,
  result: Map<string, AtlasRect>,
  forceVertical = false,
): void {
  if (nodes.length === 0) return;
  if (nodes.length === 1) {
    result.set(nodes[0].nodeId, inset(rect));
    return;
  }
  const weights = clampedWeights(nodes);
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  let leftTotal = weights[0];
  let split = 1;
  while (split < weights.length - 1 && leftTotal + weights[split] <= total / 2) {
    leftTotal += weights[split];
    split += 1;
  }
  const ratio = Math.min(0.8, Math.max(0.2, leftTotal / total));
  const vertical = forceVertical || rect.width >= rect.height;
  if (vertical) {
    const firstWidth = rect.width * ratio;
    partition(nodes.slice(0, split), { ...rect, width: firstWidth }, result, forceVertical);
    partition(
      nodes.slice(split),
      { ...rect, width: rect.width - firstWidth, x: rect.x + firstWidth },
      result,
      forceVertical,
    );
  } else {
    const firstHeight = rect.height * ratio;
    partition(nodes.slice(0, split), { ...rect, height: firstHeight }, result);
    partition(
      nodes.slice(split),
      { ...rect, height: rect.height - firstHeight, y: rect.y + firstHeight },
      result,
    );
  }
}

function clampedWeights(nodes: readonly ProjectMapAtlasNodeV1[]): number[] {
  const values = nodes.map((node) => Math.max(1, safeVolume(node.volume)));
  const minimum = Math.min(...values);
  return values.map((value) => Math.min(value, minimum * 8));
}

function safeVolume(value: string): number {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : Number.MAX_SAFE_INTEGER;
}

function inset(rect: AtlasRect): AtlasRect {
  const horizontal = Math.min(GAP, Math.max(0, rect.width / 5));
  const vertical = Math.min(GAP, Math.max(0, rect.height / 5));
  return {
    height: Math.max(1, rect.height - vertical),
    width: Math.max(1, rect.width - horizontal),
    x: rect.x + horizontal / 2,
    y: rect.y + vertical / 2,
  };
}

function byRank(left: ProjectMapAtlasNodeV1, right: ProjectMapAtlasNodeV1): number {
  return left.rank - right.rank || left.nodeId.localeCompare(right.nodeId);
}

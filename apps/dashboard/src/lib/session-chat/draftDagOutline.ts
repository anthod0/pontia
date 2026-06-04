import type { JsonObject } from '../../api/types';

export interface DraftDagOutlineInput {
  workItems: JsonObject[];
  edges: JsonObject[];
}

export interface DraftDagOutlineItem {
  id: string;
  title: string;
  description: string;
  kind: string;
  executionProfileId: string;
  priority: string;
  optional: boolean;
  parallelizable: boolean;
  raw: JsonObject;
}

export interface DraftDagDependency {
  fromId: string;
  toId: string;
  edgeType: string;
  label: string;
  resolved: boolean;
}

export interface DraftDagComponent {
  id: string;
  layers: DraftDagOutlineItem[][];
  compactText: string;
}

export interface DraftDagOutline {
  totalWorkItems: number;
  totalEdges: number;
  rootIds: string[];
  leafIds: string[];
  components: DraftDagComponent[];
  dependencies: DraftDagDependency[];
  warnings: string[];
}

export function buildDraftDagOutline(input: DraftDagOutlineInput): DraftDagOutline {
  const items = input.workItems.map(toOutlineItem).filter((item): item is DraftDagOutlineItem => item !== null);
  const itemById = new Map(items.map((item) => [item.id, item]));
  const order = new Map(items.map((item, index) => [item.id, index]));
  const warnings: string[] = [];

  const dependencies = input.edges.map((edge) => toDependency(edge, itemById));
  for (const dependency of dependencies) {
    if (!dependency.resolved) {
      warnings.push(`Dependency ${dependency.fromId} -> ${dependency.toId} references an unknown work item.`);
    }
  }

  const validDependencies = dependencies.filter((dependency) => dependency.resolved);
  const outgoing = new Map<string, string[]>();
  const incoming = new Map<string, string[]>();
  const neighbors = new Map<string, Set<string>>();
  for (const item of items) {
    outgoing.set(item.id, []);
    incoming.set(item.id, []);
    neighbors.set(item.id, new Set());
  }

  for (const dependency of validDependencies) {
    outgoing.get(dependency.fromId)?.push(dependency.toId);
    incoming.get(dependency.toId)?.push(dependency.fromId);
    neighbors.get(dependency.fromId)?.add(dependency.toId);
    neighbors.get(dependency.toId)?.add(dependency.fromId);
  }

  const rootIds = sortIds(items.filter((item) => (incoming.get(item.id)?.length ?? 0) === 0).map((item) => item.id), order);
  const leafIds = sortIds(items.filter((item) => (outgoing.get(item.id)?.length ?? 0) === 0).map((item) => item.id), order);
  const components = connectedComponents(items, neighbors, order).map((ids, index) => {
    const layers = topologicalLayers(ids, itemById, outgoing, incoming, order, warnings);
    return {
      id: `flow-${index + 1}`,
      layers,
      compactText: compactLayers(layers),
    };
  });

  return {
    totalWorkItems: items.length,
    totalEdges: input.edges.length,
    rootIds,
    leafIds,
    components,
    dependencies,
    warnings,
  };
}

function toOutlineItem(value: JsonObject): DraftDagOutlineItem | null {
  const id = stringField(value, 'temp_id', stringField(value, 'work_item_id', ''));
  if (!id) return null;
  return {
    id,
    title: stringField(value, 'title', id),
    description: stringField(value, 'description', ''),
    kind: stringField(value, 'kind', 'work'),
    executionProfileId: stringField(value, 'execution_profile_id', 'default'),
    priority: numberField(value, 'priority'),
    optional: booleanField(value, 'optional'),
    parallelizable: booleanField(value, 'parallelizable'),
    raw: value,
  };
}

function toDependency(edge: JsonObject, itemById: Map<string, DraftDagOutlineItem>): DraftDagDependency {
  const fromId = stringField(edge, 'from_work_item_id');
  const toId = stringField(edge, 'to_work_item_id');
  const edgeType = stringField(edge, 'edge_type', 'depends_on');
  return {
    fromId,
    toId,
    edgeType,
    label: `${fromId} → ${toId}`,
    resolved: itemById.has(fromId) && itemById.has(toId),
  };
}

function connectedComponents(items: DraftDagOutlineItem[], neighbors: Map<string, Set<string>>, order: Map<string, number>): string[][] {
  const visited = new Set<string>();
  const components: string[][] = [];
  for (const item of items) {
    if (visited.has(item.id)) continue;
    const stack = [item.id];
    const component: string[] = [];
    visited.add(item.id);
    while (stack.length) {
      const id = stack.pop();
      if (!id) continue;
      component.push(id);
      for (const neighbor of neighbors.get(id) ?? []) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          stack.push(neighbor);
        }
      }
    }
    components.push(sortIds(component, order));
  }
  return components.sort((left, right) => (order.get(left[0]) ?? 0) - (order.get(right[0]) ?? 0));
}

function topologicalLayers(
  ids: string[],
  itemById: Map<string, DraftDagOutlineItem>,
  outgoing: Map<string, string[]>,
  incoming: Map<string, string[]>,
  order: Map<string, number>,
  warnings: string[],
): DraftDagOutlineItem[][] {
  const remaining = new Set(ids);
  const indegree = new Map(ids.map((id) => [id, (incoming.get(id) ?? []).filter((parent) => remaining.has(parent)).length]));
  const layers: DraftDagOutlineItem[][] = [];

  while (remaining.size) {
    const layerIds = sortIds([...remaining].filter((id) => (indegree.get(id) ?? 0) === 0), order);
    if (!layerIds.length) {
      const cycleIds = sortIds([...remaining], order);
      warnings.push(`DAG cycle detected around ${cycleIds.join(', ')}; showing remaining work items together.`);
      layers.push(cycleIds.map((id) => itemById.get(id)).filter((item): item is DraftDagOutlineItem => Boolean(item)));
      break;
    }

    layers.push(layerIds.map((id) => itemById.get(id)).filter((item): item is DraftDagOutlineItem => Boolean(item)));
    for (const id of layerIds) {
      remaining.delete(id);
      for (const child of outgoing.get(id) ?? []) {
        if (remaining.has(child)) indegree.set(child, (indegree.get(child) ?? 0) - 1);
      }
    }
  }

  return layers;
}

function compactLayers(layers: DraftDagOutlineItem[][]): string {
  return layers.map((layer) => {
    const labels = layer.map((item) => item.id);
    return labels.length > 1 ? `(${labels.join(' | ')})` : (labels[0] ?? '—');
  }).join(' -> ');
}

function sortIds(ids: string[], order: Map<string, number>): string[] {
  return [...ids].sort((left, right) => (order.get(left) ?? 0) - (order.get(right) ?? 0) || left.localeCompare(right));
}

function stringField(value: JsonObject, key: string, fallback = '—'): string {
  const field = value[key];
  return typeof field === 'string' && field.trim() ? field : fallback;
}

function numberField(value: JsonObject, key: string): string {
  const field = value[key];
  return typeof field === 'number' ? String(field) : '—';
}

function booleanField(value: JsonObject, key: string): boolean {
  return value[key] === true;
}

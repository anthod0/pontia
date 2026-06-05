import dagre from 'dagre';
import type { Edge, Node } from '@xyflow/svelte';
import type { JsonObject, TaskDagView, WorkItemView } from '../../api/types';

export interface WorkItemNodeData extends Record<string, unknown> {
  label: string;
  description: string;
  kind: string;
  state: string;
  priority: number;
  workItem?: WorkItemView;
}

export type WorkItemFlowNode = Node<WorkItemNodeData, 'workItem'>;
export type WorkItemFlowEdge = Edge<{ edgeType: string }>;

export interface DagFlowModel {
  nodes: WorkItemFlowNode[];
  edges: WorkItemFlowEdge[];
}

export interface DraftDagFlowInput {
  workItems: JsonObject[];
  edges: JsonObject[];
}

interface FlowItem {
  id: string;
  label: string;
  description: string;
  kind: string;
  state: string;
  priority: number;
  workItem?: WorkItemView;
}

interface FlowEdge {
  id: string;
  source: string;
  target: string;
  edgeType: string;
}

const NODE_WIDTH = 260;
const NODE_HEIGHT = 118;

function workItemState(item: WorkItemView): string {
  const schedulerState = item.runtime?.current_state ?? (item.active ? 'active' : 'inactive');
  if (schedulerState === 'replan_anchor' && item.runtime?.outcome_state) {
    return `replan_anchor · ${item.runtime.outcome_state}`;
  }
  return schedulerState;
}

export function buildDagFlow(dag: TaskDagView): DagFlowModel {
  return buildFlowModel(
    dag.work_items.map((item) => ({
      id: item.work_item_id,
      label: item.title,
      description: item.description,
      kind: item.kind,
      state: workItemState(item),
      priority: item.priority,
      workItem: item,
    })),
    dag.edges.map((edge) => ({
      id: edge.edge_id,
      source: edge.from_work_item_id,
      target: edge.to_work_item_id,
      edgeType: edge.edge_type,
    })),
  );
}

export function buildDraftDagFlow(input: DraftDagFlowInput): DagFlowModel {
  return buildFlowModel(
    input.workItems.map(toDraftFlowItem).filter((item): item is FlowItem => item !== null),
    input.edges.map(toDraftFlowEdge).filter((edge): edge is FlowEdge => edge !== null),
  );
}

function buildFlowModel(items: FlowItem[], flowEdges: FlowEdge[]): DagFlowModel {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: 'LR', ranksep: 90, nodesep: 45, marginx: 20, marginy: 20 });

  for (const item of items) {
    graph.setNode(item.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }

  for (const edge of flowEdges) {
    graph.setEdge(edge.source, edge.target);
  }

  dagre.layout(graph);

  const nodes = items.map<WorkItemFlowNode>((item) => {
    const position = graph.node(item.id) ?? { x: 0, y: 0 };
    return {
      id: item.id,
      type: 'workItem',
      position: {
        x: position.x - NODE_WIDTH / 2,
        y: position.y - NODE_HEIGHT / 2,
      },
      data: {
        label: item.label,
        description: item.description,
        kind: item.kind,
        state: item.state,
        priority: item.priority,
        workItem: item.workItem,
      },
      style: `width: ${NODE_WIDTH}px; height: ${NODE_HEIGHT}px;`,
      sourcePosition: 'right' as WorkItemFlowNode['sourcePosition'],
      targetPosition: 'left' as WorkItemFlowNode['targetPosition'],
    };
  });

  const edges = flowEdges.map<WorkItemFlowEdge>((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: 'smoothstep',
    animated: false,
    data: { edgeType: edge.edgeType },
  }));

  return { nodes, edges };
}

function toDraftFlowItem(value: JsonObject): FlowItem | null {
  const id = stringField(value, 'temp_id', stringField(value, 'work_item_id', ''));
  if (!id) return null;
  return {
    id,
    label: stringField(value, 'title', id),
    description: stringField(value, 'description', ''),
    kind: stringField(value, 'kind', 'work'),
    state: 'proposed',
    priority: numberField(value, 'priority'),
  };
}

function toDraftFlowEdge(value: JsonObject): FlowEdge | null {
  const source = stringField(value, 'from_work_item_id', '');
  const target = stringField(value, 'to_work_item_id', '');
  if (!source || !target) return null;
  return {
    id: stringField(value, 'edge_id', `${source}-${target}`),
    source,
    target,
    edgeType: stringField(value, 'edge_type', 'depends_on'),
  };
}

function stringField(value: JsonObject, key: string, fallback: string): string {
  const field = value[key];
  return typeof field === 'string' && field.trim() ? field : fallback;
}

function numberField(value: JsonObject, key: string): number {
  const field = value[key];
  return typeof field === 'number' ? field : 0;
}

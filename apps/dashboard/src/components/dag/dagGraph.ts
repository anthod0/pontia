import dagre from 'dagre';
import type { Edge, Node } from '@xyflow/svelte';
import type { TaskDagView, WorkItemView } from '../../api/types';

export interface WorkItemNodeData extends Record<string, unknown> {
  label: string;
  description: string;
  kind: string;
  state: string;
  priority: number;
  workItem: WorkItemView;
}

export type WorkItemFlowNode = Node<WorkItemNodeData, 'workItem'>;
export type WorkItemFlowEdge = Edge<{ edgeType: string }>;

export interface DagFlowModel {
  nodes: WorkItemFlowNode[];
  edges: WorkItemFlowEdge[];
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
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: 'LR', ranksep: 90, nodesep: 45, marginx: 20, marginy: 20 });

  for (const item of dag.work_items) {
    graph.setNode(item.work_item_id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }

  for (const edge of dag.edges) {
    graph.setEdge(edge.from_work_item_id, edge.to_work_item_id);
  }

  dagre.layout(graph);

  const nodes = dag.work_items.map<WorkItemFlowNode>((item) => {
    const position = graph.node(item.work_item_id) ?? { x: 0, y: 0 };
    return {
      id: item.work_item_id,
      type: 'workItem',
      position: {
        x: position.x - NODE_WIDTH / 2,
        y: position.y - NODE_HEIGHT / 2,
      },
      data: {
        label: item.title,
        description: item.description,
        kind: item.kind,
        state: workItemState(item),
        priority: item.priority,
        workItem: item,
      },
      style: `width: ${NODE_WIDTH}px; height: ${NODE_HEIGHT}px;`,
      sourcePosition: 'right' as WorkItemFlowNode['sourcePosition'],
      targetPosition: 'left' as WorkItemFlowNode['targetPosition'],
    };
  });

  const edges = dag.edges.map<WorkItemFlowEdge>((edge) => ({
    id: edge.edge_id,
    source: edge.from_work_item_id,
    target: edge.to_work_item_id,
    type: 'smoothstep',
    animated: false,
    data: { edgeType: edge.edge_type },
  }));

  return { nodes, edges };
}

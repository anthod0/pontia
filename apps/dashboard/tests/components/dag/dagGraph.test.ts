import { expect, test } from 'vitest';
import { buildDagFlow, buildDraftDagFlow } from '../../../src/components/dag/dagGraph';

const item = (overrides) => ({
  work_item_id: 'item-1',
  task_id: 'task-1',
  title: 'Planning',
  description: 'Plan the implementation',
  kind: 'planning',
  action: 'plan',
  execution_profile_id: 'default',
  execution_profile_version: null,
  active: true,
  priority: 0,
  optional: false,
  parallelizable: true,
  acceptance_criteria: [],
  metadata: {},
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  runtime: null,
  ...overrides,
});

const edge = (from, to) => ({
  edge_id: `${from}-${to}`,
  task_id: 'task-1',
  from_work_item_id: from,
  to_work_item_id: to,
  edge_type: 'depends_on',
  created_at: '2026-01-01T00:00:00Z',
});

test('builds Svelte Flow nodes and edges from DAG work items', () => {
  const flow = buildDagFlow({
    task_id: 'task-1',
    summary: {
      total_work_items: 2,
      ready_work_items: 1,
      running_work_items: 0,
      completed_work_items: 0,
      blocked_work_items: 0,
      failed_work_items: 0,
      open_signals: 0,
      total_runs: 0,
    },
    work_items: [
      item({ work_item_id: 'plan', title: 'Plan', runtime: { current_state: 'completed' } }),
      item({ work_item_id: 'code', title: 'Code', runtime: { current_state: 'ready' } }),
    ],
    edges: [edge('plan', 'code')],
    runs: [],
    signals: [],
  });

  expect(flow.nodes.map((node) => node.id)).toEqual(['plan', 'code']);
  expect(flow.edges.map((edge) => [edge.source, edge.target])).toEqual([['plan', 'code']]);
  expect(flow.nodes[0].type).toBe('workItem');
  expect(flow.nodes[0].data.label).toBe('Plan');
  expect(flow.nodes[0].data.state).toBe('completed');
});

test('pins rendered node dimensions to match dagre layout assumptions', () => {
  const flow = buildDagFlow({
    task_id: 'task-1',
    summary: {
      total_work_items: 1,
      ready_work_items: 1,
      running_work_items: 0,
      completed_work_items: 0,
      blocked_work_items: 0,
      failed_work_items: 0,
      open_signals: 0,
      total_runs: 0,
    },
    work_items: [
      item({
        work_item_id: 'very-long',
        title: 'Very long work item title that should not change graph geometry',
        description: 'Long descriptions should be clipped within the node instead of pushing nearby nodes around.',
        kind: 'implementation-with-an-extraordinarily-long-kind-label',
      }),
    ],
    edges: [],
    runs: [],
    signals: [],
  });

  expect(flow.nodes[0].style).toMatch(/width: 260px/);
  expect(flow.nodes[0].style).toMatch(/height: 118px/);
});

test('lays out dependent work items from left to right', () => {
  const flow = buildDagFlow({
    task_id: 'task-1',
    summary: {
      total_work_items: 3,
      ready_work_items: 0,
      running_work_items: 0,
      completed_work_items: 0,
      blocked_work_items: 0,
      failed_work_items: 0,
      open_signals: 0,
      total_runs: 0,
    },
    work_items: [
      item({ work_item_id: 'plan', title: 'Plan' }),
      item({ work_item_id: 'code', title: 'Code' }),
      item({ work_item_id: 'test', title: 'Test' }),
    ],
    edges: [edge('plan', 'code'), edge('code', 'test')],
    runs: [],
    signals: [],
  });

  const byId = new Map(flow.nodes.map((node) => [node.id, node]));
  expect(byId.get('plan').position.x < byId.get('code').position.x).toBeTruthy();
  expect(byId.get('code').position.x < byId.get('test').position.x).toBeTruthy();
});

test('builds the shared flow view from draft proposal work items', () => {
  const flow = buildDraftDagFlow({
    workItems: [
      {
        temp_id: 'draft-plan',
        title: 'Draft plan',
        description: 'Break down the task',
        kind: 'planning',
        execution_profile_id: 'planner',
        priority: 3,
        optional: false,
        parallelizable: true,
      },
      {
        temp_id: 'draft-code',
        title: 'Draft code',
        description: 'Implement the task',
        kind: 'implementation',
        execution_profile_id: 'executor',
        priority: 5,
      },
    ],
    edges: [{ from_work_item_id: 'draft-plan', to_work_item_id: 'draft-code', edge_type: 'blocks' }],
  });

  expect(flow.nodes.map((node) => node.id)).toEqual(['draft-plan', 'draft-code']);
  expect(flow.edges.map((edge) => [edge.source, edge.target, edge.data.edgeType])).toEqual([['draft-plan', 'draft-code', 'blocks']]);
  expect(flow.nodes[0].data.label).toBe('Draft plan');
  expect(flow.nodes[0].data.state).toBe('proposed');
  expect(flow.nodes[0].data.priority).toBe(3);
});

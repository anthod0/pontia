import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildDagFlow } from './dagGraph.ts';

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

  assert.deepEqual(flow.nodes.map((node) => node.id), ['plan', 'code']);
  assert.deepEqual(flow.edges.map((edge) => [edge.source, edge.target]), [['plan', 'code']]);
  assert.equal(flow.nodes[0].type, 'workItem');
  assert.equal(flow.nodes[0].data.label, 'Plan');
  assert.equal(flow.nodes[0].data.state, 'completed');
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

  assert.match(flow.nodes[0].style, /width: 260px/);
  assert.match(flow.nodes[0].style, /height: 118px/);
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
  assert.ok(byId.get('plan').position.x < byId.get('code').position.x);
  assert.ok(byId.get('code').position.x < byId.get('test').position.x);
});

import { expect, test } from 'vitest';
import { buildDraftDagOutline } from '../../../src/lib/session-chat/draftDagOutline';

const workItem = (id: string, title = id, overrides = {}) => ({
  temp_id: id,
  title,
  description: `${title} description`,
  kind: 'implementation',
  action: 'Do work',
  execution_profile_id: 'coder',
  priority: 0,
  optional: false,
  parallelizable: true,
  acceptance_criteria: [],
  ...overrides,
});

const edge = (from: string, to: string, edgeType = 'depends_on') => ({
  from_work_item_id: from,
  to_work_item_id: to,
  edge_type: edgeType,
});

test('groups fork and join dependencies into topological layers', () => {
  const outline = buildDraftDagOutline({
    workItems: [workItem('A'), workItem('B'), workItem('C'), workItem('D')],
    edges: [edge('A', 'B'), edge('A', 'C'), edge('B', 'D'), edge('C', 'D')],
  });

  expect(outline.totalWorkItems).toBe(4);
  expect(outline.totalEdges).toBe(4);
  expect(outline.components).toHaveLength(1);
  expect(outline.components[0].layers.map((layer) => layer.map((item) => item.id))).toEqual([
    ['A'],
    ['B', 'C'],
    ['D'],
  ]);
  expect(outline.components[0].compactText).toBe('A -> (B | C) -> D');
});

test('keeps independent dependency chains as separate flows', () => {
  const outline = buildDraftDagOutline({
    workItems: [workItem('A'), workItem('B'), workItem('C'), workItem('D')],
    edges: [edge('A', 'B'), edge('C', 'D')],
  });

  expect(outline.components.map((component) => component.compactText)).toEqual(['A -> B', 'C -> D']);
  expect(outline.rootIds).toEqual(['A', 'C']);
  expect(outline.leafIds).toEqual(['B', 'D']);
});

test('reports unresolved edges without hiding valid outline details', () => {
  const outline = buildDraftDagOutline({
    workItems: [workItem('A'), workItem('B')],
    edges: [edge('A', 'B'), edge('missing', 'B')],
  });

  expect(outline.components[0].compactText).toBe('A -> B');
  expect(outline.warnings).toContain('Dependency missing -> B references an unknown work item.');
  expect(outline.dependencies.map((dependency) => dependency.label)).toEqual(['A → B', 'missing → B']);
});

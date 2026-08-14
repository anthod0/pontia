import { describe, expect, test } from 'vitest';
import { groupWorkflowPhases, selectedPhaseOrdinal } from '../src/pages/workflows/phases.ts';
import type { WorkflowNodeView } from '../src/api/types.ts';

function node(nodeId: string, phase: string, submitted = false): WorkflowNodeView {
  return {
    node_id: nodeId,
    phase,
    title: nodeId,
    status: submitted ? 'submitted' : 'pending',
    session_id: null,
    session_state: null,
    submitted_at: submitted ? '2026-08-14T00:00:00Z' : null,
  };
}

describe('Workflow Phase tabs', () => {
  test('folds only consecutive equal phases and keeps repeated names as separate tabs', () => {
    const phases = groupWorkflowPhases([
      node('a', 'Research', true),
      node('b', 'Research'),
      node('c', 'Review'),
      node('d', 'Research'),
    ], 'c');

    expect(phases.map(({ ordinal, name, submittedCount, current, nodes }) => ({
      ordinal, name, submittedCount, current, nodes: nodes.map((item) => item.node_id),
    }))).toEqual([
      { ordinal: 1, name: 'Research', submittedCount: 1, current: false, nodes: ['a', 'b'] },
      { ordinal: 2, name: 'Review', submittedCount: 0, current: true, nodes: ['c'] },
      { ordinal: 3, name: 'Research', submittedCount: 0, current: false, nodes: ['d'] },
    ]);
  });

  test('accepts only an in-range integer query ordinal', () => {
    const phases = groupWorkflowPhases([node('a', 'A'), node('b', 'B')], 'a');
    expect(selectedPhaseOrdinal('2', phases)).toBe(2);
    expect(selectedPhaseOrdinal('0', phases)).toBeNull();
    expect(selectedPhaseOrdinal('3', phases)).toBeNull();
    expect(selectedPhaseOrdinal('1.5', phases)).toBeNull();
    expect(selectedPhaseOrdinal(null, phases)).toBeNull();
  });
});

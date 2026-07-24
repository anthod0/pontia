import { describe, expect, test } from 'vitest';
import type { TurnView } from '../../../src/api/types';
import { buildSessionTurnTree } from '../../../src/pages/sessions/sessionTurnTree';

function turn(overrides: Partial<TurnView> & Pick<TurnView, 'turn_id'>): TurnView {
  return {
    turn_id: overrides.turn_id,
    session_id: 'session-1',
    parent_turn_id: null,
    topology_status: 'root',
    state: 'completed',
    input: { summary: overrides.turn_id },
    output: { summary: `${overrides.turn_id} output` },
    failure: null,
    created_at: '2026-01-01T00:00:00Z',
    started_at: null,
    completed_at: null,
    metadata: {},
    ...overrides,
  };
}

describe('buildSessionTurnTree', () => {
  test('orders branches under their parent and highlights the current ancestor chain', () => {
    const rows = buildSessionTurnTree([
      turn({ turn_id: 'root' }),
      turn({ turn_id: 'branch-b', parent_turn_id: 'root', topology_status: 'linked', created_at: '2026-01-01T00:02:00Z' }),
      turn({ turn_id: 'branch-a', parent_turn_id: 'root', topology_status: 'linked', created_at: '2026-01-01T00:01:00Z' }),
      turn({ turn_id: 'leaf', parent_turn_id: 'branch-a', topology_status: 'linked', created_at: '2026-01-01T00:03:00Z' }),
    ], 'leaf');

    expect(rows.map((row) => [row.turn.turn_id, row.depth])).toEqual([
      ['root', 0],
      ['branch-a', 1],
      ['leaf', 2],
      ['branch-b', 1],
    ]);
    expect(rows.filter((row) => row.isCurrentBranch).map((row) => row.turn.turn_id)).toEqual([
      'root',
      'branch-a',
      'leaf',
    ]);
    expect(rows.find((row) => row.turn.turn_id === 'leaf')?.isCurrent).toBe(true);
    expect(rows.find((row) => row.turn.turn_id === 'root')?.childCount).toBe(2);
  });

  test('keeps unknown, orphaned, and cyclic turns visible without recursing forever', () => {
    const rows = buildSessionTurnTree([
      turn({ turn_id: 'unknown', topology_status: 'unknown' }),
      turn({ turn_id: 'orphan', parent_turn_id: 'missing', topology_status: 'linked' }),
      turn({ turn_id: 'cycle-a', parent_turn_id: 'cycle-b', topology_status: 'linked' }),
      turn({ turn_id: 'cycle-b', parent_turn_id: 'cycle-a', topology_status: 'linked' }),
    ], 'cycle-a');

    expect(rows).toHaveLength(4);
    expect(Object.fromEntries(rows.map((row) => [row.turn.turn_id, row.relation]))).toEqual({
      'cycle-a': 'cycle',
      'cycle-b': 'cycle',
      orphan: 'orphan',
      unknown: 'unknown',
    });
  });
});

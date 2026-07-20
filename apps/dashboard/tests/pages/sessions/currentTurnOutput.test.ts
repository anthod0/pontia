import { expect, test } from 'vitest';
import { selectCurrentTurnOutput } from '../../../src/pages/sessions/currentTurnOutput';

const turn = (overrides) => ({
  turn_id: 'turn-old',
  session_id: 'session-1',
  state: 'completed',
  input: { summary: 'old input' },
  output: { summary: 'old output' },
  failure: null,
  created_at: '2026-01-01T00:00:00Z',
  started_at: '2026-01-01T00:00:01Z',
  completed_at: '2026-01-01T00:00:02Z',
  metadata: {},
  ...overrides,
});

test('selects the current branch leaf even when it is terminal and not the newest turn', () => {
  const selected = selectCurrentTurnOutput(
    { current_turn_id: 'turn-current' },
    [
      turn({ turn_id: 'turn-current', state: 'completed', output: { summary: 'current output' }, created_at: '2026-01-01T00:00:00Z' }),
      turn({ turn_id: 'turn-newer', output: { summary: 'newer completed output' }, created_at: '2026-01-01T00:05:00Z' }),
    ],
  );

  expect(selected?.turn.turn_id).toBe('turn-current');
  expect(selected?.title).toBe('Current branch turn output');
  expect(selected?.outputSummary).toBe('current output');
});

test('falls back to the newest turn when the session has no current turn', () => {
  const selected = selectCurrentTurnOutput(
    { current_turn_id: null },
    [
      turn({ turn_id: 'turn-a', created_at: '2026-01-01T00:00:00Z' }),
      turn({ turn_id: 'turn-b', created_at: '2026-01-01T00:10:00Z', output: { summary: 'latest output' } }),
    ],
  );

  expect(selected?.turn.turn_id).toBe('turn-b');
  expect(selected?.title).toBe('Latest turn output');
  expect(selected?.outputSummary).toBe('latest output');
});

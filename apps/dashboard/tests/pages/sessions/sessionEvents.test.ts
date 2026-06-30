import { expect, test } from 'vitest';
import { sessionEventDetailRows, sessionEventSummary } from '../../../src/pages/sessions/sessionEvents';

test('summarizes session event payloads using important human-readable fields', () => {
  expect(sessionEventSummary({ summary: 'Turn completed', state: 'completed' })).toBe('Turn completed · state=completed');
  expect(sessionEventSummary({ output: { summary: 'Implemented log view' }, reason: 'done' })).toBe('Implemented log view · reason=done');
  expect(sessionEventSummary({ error: { message: 'adapter timed out' } })).toBe('adapter timed out');
});

test('falls back to compact key value summaries instead of raw json', () => {
  expect(sessionEventSummary({ state: 'running', nested: { changed: true }, count: 3 })).toBe('state=running · nested={…} · count=3');
  expect(sessionEventSummary({})).toBe('No payload details');
});

test('builds expandable detail rows for ids and payload metadata', () => {
  const rows = sessionEventDetailRows({
    event_id: 'evt_1234567890abcdef',
    session_id: 'sess_abcdef123456',
    turn_id: 'turn_abcdef123456',
    source: 'runtime',
    type: 'turn.completed',
    time: '2026-01-01T00:00:00Z',
    payload: { summary: 'Done' },
  });

  expect(rows).toContainEqual(['Event ID', 'evt_1234567890abcdef']);
  expect(rows).toContainEqual(['Session ID', 'sess_abcdef123456']);
  expect(rows).toContainEqual(['Turn ID', 'turn_abcdef123456']);
  expect(rows).toContainEqual(['Source', 'runtime']);
});

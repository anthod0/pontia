import { visibleSessions } from './sessionFilters.js';
import type { SessionView } from '../api/types';

function session(session_id: string, state: string): SessionView {
  return {
    session_id,
    client_type: 'pi',
    state,
    current_turn_id: null,
    workspace_id: null,
    workspace: null,
    capabilities: {},
    created_at: '2026-05-08T00:00:00Z',
    updated_at: '2026-05-08T00:00:00Z',
    metadata: {},
  };
}

const sessions = [
  session('active', 'idle'),
  session('finished', 'exited'),
  session('errored', 'error'),
];

const visible = visibleSessions(sessions).map((item) => item.session_id);

if (JSON.stringify(visible) !== JSON.stringify(['active', 'errored'])) {
  throw new Error(`expected exited sessions to be hidden, got ${JSON.stringify(visible)}`);
}
